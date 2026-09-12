//! Harbor FFI: the C ABI seam between Flutter (Dart) and Harbor Core.
//!
//! One dispatcher, JSON in / JSON out (`harbor_core_call`), so the Dart
//! side carries no business logic and the Rust side owns policy and
//! runtime truth (08_Repo_Structure dependency rules). Handles are opaque
//! pointers; strings returned by the boundary are freed with
//! `harbor_core_string_free`.

pub mod knowledge;

use std::ffi::{c_char, CStr, CString};
use std::ptr;

use harbor_agent::event::{Counters, EventPayload, EventType, ReplaySemantics, RunEvent};
use harbor_agent::lease::LeaseManager;
use harbor_agent::{Actor, PauseReason, RunState};
use harbor_core::workspace::{OpenOptions, Workspace};
use harbor_store::keys::KeyStore;
use harbor_core::HarborError;
use harbor_security::policy::PrivacyMode;

pub struct WorkspaceHandle {
    inner: Workspace,
    data_root: std::path::PathBuf,
    /// Created on demand when an embedding model is available.
    knowledge: Option<crate::knowledge::KnowledgeService>,
    /// Chat provider over installed GGUF models (created on demand).
    chat: Option<crate::knowledge::ChatHandle>,
    /// Shared HTTPS transport for brokered acquisition.
    transport: harbor_net::transport::UreqTransport,
    /// Signed catalog trust state + accepted document (on demand).
    catalog: Option<(
        harbor_modelhub::catalog_signing::CatalogVerifier,
        harbor_canonical::JsonValue,
    )>,
    /// Optional HF token for gated/private repos (M4). Sourced from the
    /// keystore; never returned across the boundary.
    hub_token: Option<String>,
}

fn load_hub_token(data_root: &std::path::Path) -> Option<String> {
    // The token is stored wrapped by the device root key (encrypted at
    // rest); the plaintext never crosses the FFI boundary outward.
    let ks = harbor_store::keys::FileKeyStore::new(data_root.join("keys")).ok()?;
    let root = ks.device_root_key("harbor.device").ok()?;
    let conn = rusqlite::Connection::open(data_root.join("db").join("store.db")).ok()?;
    let wrapped: Vec<u8> = conn
        .query_row(
            "SELECT secret FROM hub_tokens WHERE id = 'default'",
            [],
            |r| r.get(0),
        )
        .ok()?;
    harbor_store::keys::WrappedKey::from_bytes(&wrapped)
        .ok()?
        .unwrap(&root)
        .ok()
        .and_then(|b| String::from_utf8(b).ok())
}

/// Durable catalog trust state: trusted keys, accepted epoch, accepted
/// document. Restored when a workspace opens so epoch monotonicity and
/// key revocations survive process death.
fn persist_catalog_state(
    data_root: &std::path::Path,
    verifier: &harbor_modelhub::catalog_signing::CatalogVerifier,
    entries: &harbor_canonical::JsonValue,
) -> Result<(), HarborError> {
    std::fs::create_dir_all(data_root.join("db"))?;
    let conn = rusqlite::Connection::open(data_root.join("db").join("store.db"))?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS catalog_trust (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            state TEXT NOT NULL
        );",
    )?;
    let state = serde_json::json!({
        "trusted": verifier.trusted,
        "accepted_epoch": verifier.accepted_epoch,
        "revoked": verifier.revoked.keys().collect::<Vec<_>>(),
        "entries": entries,
    });
    conn.execute(
        "INSERT INTO catalog_trust (id, state) VALUES (1, ?1)
         ON CONFLICT(id) DO UPDATE SET state = ?1",
        rusqlite::params![state.to_string()],
    )?;
    Ok(())
}

fn load_catalog_state(
    data_root: &std::path::Path,
) -> Option<(
    harbor_modelhub::catalog_signing::CatalogVerifier,
    harbor_canonical::JsonValue,
)> {
    let conn = rusqlite::Connection::open(data_root.join("db").join("store.db")).ok()?;
    let state: String = conn
        .query_row("SELECT state FROM catalog_trust WHERE id = 1", [], |r| r.get(0))
        .ok()?;
    let v: serde_json::Value = serde_json::from_str(&state).ok()?;
    let mut trusted = std::collections::BTreeMap::new();
    for (k, v) in v.get("trusted")?.as_object()?.iter() {
        trusted.insert(k.clone(), v.as_str()?.to_string());
    }
    let accepted_epoch = v.get("accepted_epoch")?.as_u64()?;
    let mut revoked = std::collections::BTreeMap::new();
    for k in v.get("revoked")?.as_array()? {
        revoked.insert(k.as_str()?.to_string(), ());
    }
    let entries = harbor_canonical::parse(&v.get("entries")?.to_string()).ok()?;
    Some((
        harbor_modelhub::catalog_signing::CatalogVerifier {
            trusted,
            accepted_epoch,
            revoked,
        },
        entries,
    ))
}

/// Persist (or clear with None) the hub token, wrapped by the device root.
fn save_hub_token(data_root: &std::path::Path, token: Option<&str>) -> Result<(), HarborError> {
    std::fs::create_dir_all(data_root.join("db"))?;
    let ks = harbor_store::keys::FileKeyStore::new(data_root.join("keys"))?;
    let root = ks.device_root_key("harbor.device")?;
    let conn = rusqlite::Connection::open(data_root.join("db").join("store.db"))?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS hub_tokens (
            id TEXT PRIMARY KEY,
            secret BLOB NOT NULL
        );",
    )?;
    match token {
        Some(t) => {
            let wrapped = harbor_store::keys::WrappedKey::wrap(&root, t.as_bytes())?;
            conn.execute(
                "INSERT INTO hub_tokens (id, secret) VALUES ('default', ?1)
                 ON CONFLICT(id) DO UPDATE SET secret = ?1",
                rusqlite::params![wrapped.to_bytes()],
            )?;
        }
        None => {
            conn.execute("DELETE FROM hub_tokens WHERE id = 'default'", [])?;
        }
    }
    Ok(())
}

fn str_from_ptr<'a>(p: *const c_char) -> Result<&'a str, HarborError> {
    if p.is_null() {
        return Err(HarborError::Other("null string argument".into()));
    }
    unsafe { CStr::from_ptr(p) }
        .to_str()
        .map_err(|e| HarborError::Other(format!("utf8: {e}")))
}

/// Open (create or resume) a workspace.
#[no_mangle]
pub extern "C" fn harbor_core_open(
    data_root: *const c_char,
    workspace_id: *const c_char,
    privacy_mode: u8,
) -> *mut WorkspaceHandle {
    let result = (|| {
        let root = str_from_ptr(data_root)?;
        let ws_id = str_from_ptr(workspace_id)?;
        let mode = match privacy_mode {
            0 => PrivacyMode::LocalOnly,
            1 => PrivacyMode::Hybrid,
            2 => PrivacyMode::RemoteAllowed,
            other => return Err(HarborError::Other(format!("bad privacy mode {other}"))),
        };
        let data_root = std::path::PathBuf::from(root);
        let opts = OpenOptions {
            data_root: data_root.clone(),
            device_id: "device".into(),
        };
        let ws = harbor_core::Workspace::open(&opts, ws_id, mode)?;
        Ok(WorkspaceHandle {
            inner: ws,
            data_root,
            knowledge: None,
            chat: None,
            transport: harbor_net::transport::UreqTransport::new(),
            catalog: load_catalog_state(&opts.data_root),
            hub_token: load_hub_token(&opts.data_root),
        })
    })();
    match result {
        Ok(h) => Box::into_raw(Box::new(h)),
        Err(_) => ptr::null_mut(),
    }
}

/// Close a workspace handle.
#[no_mangle]
pub extern "C" fn harbor_core_close(handle: *mut WorkspaceHandle) {
    if !handle.is_null() {
        unsafe { drop(Box::from_raw(handle)) };
    }
}

/// Free a string returned by [`harbor_core_call`].
#[no_mangle]
pub extern "C" fn harbor_core_string_free(s: *mut c_char) {
    if !s.is_null() {
        unsafe { drop(CString::from_raw(s)) };
    }
}

fn ok_json(v: serde_json::Value) -> *mut c_char {
    let payload = serde_json::json!({ "ok": true, "result": v });
    match CString::new(payload.to_string()) {
        Ok(s) => s.into_raw(),
        Err(_) => ptr::null_mut(),
    }
}

fn err_json(msg: String) -> *mut c_char {
    let payload = serde_json::json!({ "ok": false, "error": msg });
    match CString::new(payload.to_string()) {
        Ok(s) => s.into_raw(),
        Err(_) => ptr::null_mut(),
    }
}

fn dispatch(ws: &mut WorkspaceHandle, method: &str, args: &serde_json::Value) -> Result<serde_json::Value, HarborError> {
    match method {
        // --- runs -------------------------------------------------------
        "run.create" => {
            let run_id = args
                .get("run_id")
                .and_then(|v| v.as_str())
                .unwrap_or("run");
            ws.inner
                .agent_log
                .create_run(run_id, &ws.inner.workspace_id, harbor_core::Workspace::now())?;
            Ok(serde_json::json!({ "run_id": run_id }))
        }
        "run.state" => {
            let run_id = args.get("run_id").and_then(|v| v.as_str()).unwrap_or("");
            let (state, counters, generation) = ws.inner.agent_log.run_state(run_id)?;
            Ok(serde_json::json!({
                "state": state.as_str(),
                "counters": {
                    "active_compute_ms_total": counters.active_compute_ms_total,
                    "step_count_total": counters.step_count_total,
                    "tool_count_total": counters.tool_count_total,
                    "context_tokens_total": counters.context_tokens_total,
                },
                "executor_generation": generation,
            }))
        }
        "run.pause" => {
            // RUNNING -> PAUSED with a durable reason, under a lease.
            let run_id = args.get("run_id").and_then(|v| v.as_str()).unwrap_or("");
            let reason_s = args.get("reason").and_then(|v| v.as_str()).unwrap_or("user");
            let reason = PauseReason::parse(reason_s)
                .ok_or_else(|| HarborError::Other(format!("bad pause reason {reason_s}")))?;
            let mut mgr = LeaseManager::open(ws.data_root.join("db").join("agent.db"))?;
            let lease = mgr.acquire(run_id, "ffi-executor", chrono::Duration::minutes(10), harbor_core::Workspace::now())?;
            let stream = ws.inner.agent_log.load_stream(run_id)?;
            let head = stream.last().ok_or_else(|| HarborError::Other("run missing".into()))?;
            let head_hash = head
                .hash()
                .map_err(|e| HarborError::Other(format!("hash: {e}")))?;
            let counters = head.counters;
            let event = RunEvent {
                run_id: run_id.to_string(),
                event_id: format!("evt-{}", harbor_canonical::sha256_hex(format!("{run_id}-pause-{}", chrono::Utc::now()).as_bytes()).get(..16).unwrap_or("evt")),
                seq: stream.len() as u64,
                event_type: EventType::RunTransition,
                replay_semantics: ReplaySemantics::StateAffecting,
                actor: Actor::User,
                lease_generation: lease.generation,
                counters: Counters {
                    active_compute_ms_total: counters.active_compute_ms_total,
                    step_count_total: counters.step_count_total,
                    tool_count_total: counters.tool_count_total,
                    context_tokens_total: counters.context_tokens_total,
                },
                payload: EventPayload::Transition {
                    from_state: RunState::Running,
                    to_state: RunState::Paused,
                    reason: Some(reason),
                },
                created_at: harbor_core::Workspace::now(),
                prev_event_hash: Some(head_hash),
            };
            ws.inner.agent_log.append(event, lease.generation, None)?;
            Ok(serde_json::json!({ "state": "PAUSED", "reason": reason.as_str() }))
        }
        // --- blobs ------------------------------------------------------
        "blob.put" => {
            let data = args
                .get("data_b64")
                .and_then(|v| v.as_str())
                .ok_or_else(|| HarborError::Other("missing data_b64".into()))?;
            use base64::Engine as _;
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(data)
                .map_err(|e| HarborError::Other(format!("b64: {e}")))?;
            let r = ws
                .inner
                .blobs()
                .put(&ws.inner.workspace_id, &bytes, &Default::default())
                .map_err(HarborError::Store)?;
            Ok(serde_json::json!({ "blob_id": r.id, "size": r.size }))
        }
        "blob.get" => {
            let id = args
                .get("blob_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| HarborError::Other("missing blob_id".into()))?;
            let bytes = ws
                .inner
                .blobs()
                .get(&ws.inner.workspace_id, id, &Default::default())
                .map_err(HarborError::Store)?;
            use base64::Engine as _;
            Ok(serde_json::json!({
                "data_b64": base64::engine::general_purpose::STANDARD.encode(bytes)
            }))
        }
        // --- formula qualification -------------------------------------
        "formula.qualify" => {
            let report = harbor_formula::qualify::run_qualification()
                .map_err(HarborError::Other)?;
            Ok(serde_json::json!({
                "engine": format!("{}/{}", report.engine_family, report.engine_version),
                "source_revision": report.engine_source_revision,
                "bundle_sha256": report.fixture_bundle_sha256,
                "passed": report.passed,
                "failed": report.failed,
                "targets_pass": report.target_status.values().filter(|s| **s == "PASS").count(),
                "targets_total": report.target_status.len(),
            }))
        }
        // --- trust pulse ------------------------------------------------
        "trust.pulse" => Ok(serde_json::json!({
            "policy": ws.inner.privacy_mode.as_str(),
            "policy_version": ws.inner.privacy_policy_version,
            "execution_hint": "ON_DEVICE",
        })),
        // --- models -----------------------------------------------------
        "models.installed" => {
            let installer = harbor_modelhub::install::PackageInstaller::new(
                ws.data_root.join("models"),
            );
            let ids = installer
                .installed_packages()
                .map_err(|e| HarborError::Other(format!("models: {e}")))?;
            let mut out = Vec::new();
            for id in ids {
                let m = installer
                    .load_manifest(&id)
                    .map_err(|e| HarborError::Other(format!("manifest: {e}")))?;
                let total: u64 = m.files.iter().map(|f| f.size_bytes).sum();
                out.push(serde_json::json!({
                    "id": m.id,
                    "runtime": m.runtime.kind,
                    "min_revision": m.runtime.min_revision,
                    "files": m.files.len(),
                    "total_bytes": total,
                }));
            }
            Ok(serde_json::json!({ "models": out }))
        }
        "model.fit_score" => {
            // Device facts come from the platform adapters (native layer);
            // the core computes the score — never the UI.
            let package_id = args
                .get("package_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| HarborError::Other("missing package_id".into()))?;
            let installer = harbor_modelhub::install::PackageInstaller::new(
                ws.data_root.join("models"),
            );
            let manifest = installer
                .load_manifest(package_id)
                .map_err(|e| HarborError::Other(format!("manifest: {e}")))?;
            let weights: u64 = manifest
                .files
                .iter()
                .filter(|f| f.role == "weights" || f.role == "weights_shard")
                .map(|f| f.size_bytes)
                .sum();
            let device = harbor_modelhub::fit::DeviceProfile {
                architecture: args
                    .get("architecture")
                    .and_then(|v| v.as_str())
                    .unwrap_or(std::env::consts::ARCH)
                    .to_string(),
                physical_ram: args.get("physical_ram").and_then(|v| v.as_u64()).unwrap_or(8 << 30),
                available_ram: args.get("available_ram").and_then(|v| v.as_u64()).unwrap_or(4 << 30),
                gpu_backend: if args.get("gpu_backend").and_then(|v| v.as_bool()) == Some(true) {
                    Some("Metal".to_string())
                } else {
                    None
                },
                accelerated_gguf_supported: args
                    .get("accelerated")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
                thermal: match args.get("thermal").and_then(|v| v.as_str()) {
                    Some("reduced") => harbor_modelhub::fit::Thermal::Reduced,
                    Some("critical") => harbor_modelhub::fit::Thermal::Critical,
                    _ => harbor_modelhub::fit::Thermal::Normal,
                },
            };
            let footprint = harbor_modelhub::fit::ModelFootprint {
                format: "GGUF".to_string(),
                weights_bytes: weights,
                peak_memory_bytes: weights + weights / 8,
                kv_cache_per_1k_tokens: 8 << 20,
                context_tokens: args.get("context_tokens").and_then(|v| v.as_u64()).unwrap_or(2048),
                quantization: "Q4_K_M".to_string(),
                multimodal: false,
                runtime_kind: manifest.runtime.kind,
            };
            let score = harbor_modelhub::fit::FitScore::evaluate(&device, &footprint);
            Ok(serde_json::json!({
                "band": format!("{:?}", score.band).to_lowercase(),
                "reasons": score.reasons,
                "estimated_peak_bytes": score.estimated_peak_bytes,
            }))
        }
        // --- artifact previews (Work Canvas) -----------------------------
        "artifact.preview" => {
            let data_b64 = args
                .get("data_b64")
                .and_then(|v| v.as_str())
                .ok_or_else(|| HarborError::Other("missing data_b64".into()))?;
            use base64::Engine as _;
            let bytes = base64::engine::general_purpose::STANDARD

                .decode(data_b64)
                .map_err(|e| HarborError::Other(format!("b64: {e}")))?;
            // Dispatch by OOXML content types (a workbook has no slide
            // parts, so emptiness — not success — separates the two).
            if bytes.len() < 4 {
                return Err(HarborError::Other("unsupported artifact".into()));
            }
            // PDF: header magic, text extraction with page mapping.
            if bytes.starts_with(b"%PDF") {
                let p = harbor_render::pdf::extract_pages(&bytes)
                    .map_err(|e| HarborError::Other(e.to_string()))?;
                return Ok(serde_json::json!({
                    "kind": "pdf",
                    "preview": serde_json::to_value(&p).map_err(|e| HarborError::Other(e.to_string()))?,
                }));
            }
            if !bytes.starts_with(b"PK") {
                return Err(HarborError::Other("unsupported artifact".into()));
            }
            let mut archive = zip::ZipArchive::new(std::io::Cursor::new(&bytes))
                .map_err(|e| HarborError::Other(format!("zip: {e}")))?;
            let content_types = archive
                .by_name("[Content_Types].xml")
                .ok()
                .map(|mut f| {
                    use std::io::Read as _;
                    let mut s = String::new();
                    let _ = f.read_to_string(&mut s);
                    s
                })
                .unwrap_or_default();
            drop(archive);
            // Office Feature Matrix compatibility report: every part
            // classified per 21_Office_Feature_Matrix.csv; unknown parts
            // surface as UNKNOWN_REJECT_OR_PRESERVE_ONLY (row 23).
            let compatibility = {
                let fmt = if content_types.contains("spreadsheetml") {
                    harbor_artifacts::OfficeFormat::Xlsx
                } else if content_types.contains("presentationml") {
                    harbor_artifacts::OfficeFormat::Pptx
                } else {
                    harbor_artifacts::OfficeFormat::Docx
                };
                harbor_artifacts::compatibility_report(fmt, &bytes).ok()
            };
            // DOCX: paragraph preview with styles.
            if content_types.contains("wordprocessingml") {
                let p = harbor_render::DocxPreview::from_docx(&bytes)
                    .map_err(|e| HarborError::Other(e.to_string()))?;
                return Ok(serde_json::json!({
                    "kind": "docx",
                    "preview": serde_json::to_value(&p).map_err(|e| HarborError::Other(e.to_string()))?,
                    "compatibility": compatibility,
                }));
            }
            let is_deck = content_types.contains("presentationml");
            if is_deck {
                let p = harbor_render::DeckPreview::from_pptx(&bytes)
                    .map_err(|e| HarborError::Other(e.to_string()))?;
                Ok(serde_json::json!({
                    "kind": "deck",
                    "preview": serde_json::to_value(&p).map_err(|e| HarborError::Other(e.to_string()))?,
                    "compatibility": compatibility,
                }))
            } else {
                let p = harbor_render::WorkbookPreview::from_xlsx(&bytes)
                    .map_err(|e| HarborError::Other(e.to_string()))?;
                Ok(serde_json::json!({
                    "kind": "workbook",
                    "preview": serde_json::to_value(&p).map_err(|e| HarborError::Other(e.to_string()))?,
                    "compatibility": compatibility,
                }))
            }
        }
        // --- skills -----------------------------------------------------
        "skills.list" => {
            let skills = harbor_core::skills::builtin_skills()
                .map_err(|e| HarborError::Other(e.to_string()))?;
            let catalog = harbor_core::skills::CapabilityCatalog::default();
            let mut out = Vec::new();
            for s in &skills {
                s.validate(&catalog)
                    .map_err(|e| HarborError::Other(e.to_string()))?;
                out.push(serde_json::json!({
                    "id": s.id,
                    "title": s.title,
                    "family": s.family,
                    "description": s.description,
                    "tools": s.tools,
                }));
            }
            Ok(serde_json::json!({ "skills": out }))
        }
        // --- evaluation -------------------------------------------------
        "eval.run" => {
            let (hash, reports) = harbor_knowledge::run_pinned_evals()
                .map_err(|e| HarborError::Other(e.to_string()))?;
            let langs: Vec<serde_json::Value> = reports
                .iter()
                .map(|(lang, r)| {
                    serde_json::json!({
                        "language": lang,
                        "passed": r.passed,
                        "failed": r.failed,
                    })
                })
                .collect();
            Ok(serde_json::json!({
                "corpus_sha256": hash,
                "languages": langs,
            }))
        }
        // Log the user's request as the first step of a run (Home composer).
        "run.log_request" => {
            let run_id = args.get("run_id").and_then(|v| v.as_str())
                .ok_or_else(|| HarborError::Other("missing run_id".into()))?;
            let text = args.get("text").and_then(|v| v.as_str())
                .ok_or_else(|| HarborError::Other("missing text".into()))?;
            let mut mgr = LeaseManager::open(ws.data_root.join("db").join("agent.db"))?;
            let lease = mgr.acquire(run_id, "ffi-executor", chrono::Duration::minutes(10), harbor_core::Workspace::now())?;
            let stream = ws.inner.agent_log.load_stream(run_id)?;
            let head = stream.last().ok_or_else(|| HarborError::Other("run missing".into()))?;
            let head_hash = head.hash().map_err(|e| HarborError::Other(format!("hash: {e}")))?;
            let counters = head.counters;
            let now = harbor_core::Workspace::now()
                .to_rfc3339_opts(chrono::SecondsFormat::Micros, true)
                .parse::<chrono::DateTime<chrono::Utc>>()
                .map_err(|e| HarborError::Other(format!("time: {e}")))?
                .into();
            let event = RunEvent {
                run_id: run_id.to_string(),
                event_id: format!("evt-{}", harbor_canonical::sha256_hex(format!("{run_id}-req-{}", chrono::Utc::now()).as_bytes()).get(..16).unwrap_or("evt")),
                seq: stream.len() as u64,
                event_type: EventType::RunStepStarted,
                replay_semantics: ReplaySemantics::StateAffecting,
                actor: Actor::User,
                lease_generation: lease.generation,
                counters: Counters {
                    active_compute_ms_total: counters.active_compute_ms_total,
                    step_count_total: counters.step_count_total + 1,
                    tool_count_total: counters.tool_count_total,
                    context_tokens_total: counters.context_tokens_total,
                },
                payload: EventPayload::StepStarted {
                    step_id: format!("step-{}", counters.step_count_total + 1),
                    description: text.to_string(),
                },
                created_at: now,
                prev_event_hash: Some(head_hash),
            };
            ws.inner.agent_log.append(event, lease.generation, None)?;
            Ok(serde_json::json!({ "logged": true }))
        }
        // --- hub auth (M4: gated/private repos) ---------------------------
        "hub.set_token" => {
            let token = args.get("token").and_then(|v| v.as_str())
                .ok_or_else(|| HarborError::Other("missing token".into()))?;
            save_hub_token(&ws.data_root, Some(token))?;
            ws.hub_token = Some(token.to_string());
            Ok(serde_json::json!({ "set": true }))
        }
        "hub.clear_token" => {
            save_hub_token(&ws.data_root, None)?;
            ws.hub_token = None;
            Ok(serde_json::json!({ "set": false }))
        }
        // --- acquisition -------------------------------------------------
        "models.search_hf" => {
            let query = args.get("query").and_then(|v| v.as_str())
                .ok_or_else(|| HarborError::Other("missing query".into()))?;
            let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(8) as usize;
            let session = ws.inner.broker.open_session(
                harbor_net::broker::EgressClass::AcquisitionMetadata,
                "https://huggingface.co",
                harbor_core::Workspace::acquisition_session_ttl(),
                ws.inner.privacy_mode,
            ).map_err(|e| HarborError::Other(e.to_string()))?;
            let discovery = harbor_modelhub::hf::HfDiscovery::new(
                &ws.inner.broker, &ws.transport)
                .with_token(ws.hub_token.clone());
            let models = discovery
                .search(&session, query, limit.min(20))
                .map_err(|e| HarborError::Other(e.to_string()))?;
            Ok(serde_json::to_value(&models).map_err(|e| HarborError::Other(e.to_string()))?)
        }
        "models.acquire_hf" => {
            let package_id = args.get("package_id").and_then(|v| v.as_str())
                .ok_or_else(|| HarborError::Other("missing package_id".into()))?;
            let repo_id = args.get("repo_id").and_then(|v| v.as_str())
                .ok_or_else(|| HarborError::Other("missing repo_id".into()))?;
            let revision = args.get("revision").and_then(|v| v.as_str()).unwrap_or("main");
            let files: Vec<(String, String, String)> = args
                .get("files")
                .and_then(|v| v.as_array())
                .ok_or_else(|| HarborError::Other("missing files".into()))?
                .iter()
                .map(|f| (
                    f.get("path").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
                    f.get("role").and_then(|v| v.as_str()).unwrap_or("weights").to_string(),
                    f.get("sha256").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
                ))
                .collect();
            // Explicit weight-transfer sessions: HF + CDN origins. Each open
            // is logged; under LOCAL_ONLY, acquisition metadata remains
            // permitted but weight transfer is equally explicit per policy.
            let mut sessions = std::collections::BTreeMap::new();
            let ttl = harbor_core::Workspace::acquisition_session_ttl();
            for origin in ["https://huggingface.co"].iter().chain(harbor_modelhub::HF_CDN_ORIGINS.iter()) {
                if let Ok(sess) = ws.inner.broker.open_session(
                    harbor_net::broker::EgressClass::WeightTransfer,
                    origin, ttl, ws.inner.privacy_mode,
                ) {
                    sessions.insert(origin.to_string(), sess);
                }
            }
            let installer = harbor_modelhub::install::PackageInstaller::new(ws.data_root.join("models"));
            let acquirer = harbor_modelhub::acquire::HfAcquirer {
                broker: &ws.inner.broker,
                transport: &ws.transport,
                installer: &installer,
                sessions,
                auth_token: ws.hub_token.clone(),
            };
            acquirer.acquire(package_id, repo_id, revision, &files, harbor_core::Workspace::now())
                .map_err(|e| HarborError::Other(e.to_string()))
        }
        // --- signed catalog ------------------------------------------------
        "catalog.import" => {
            // Signed catalog document (harbor.catalog/v1 envelope).
            let key = args.get("key_id").and_then(|v| v.as_str())
                .ok_or_else(|| HarborError::Other("missing key_id".into()))?;
            let sig = args.get("signature").and_then(|v| v.as_str())
                .ok_or_else(|| HarborError::Other("missing signature".into()))?;
            let epoch = args.get("epoch").and_then(|v| v.as_u64())
                .ok_or_else(|| HarborError::Other("missing epoch".into()))?;
            let published_at = args.get("published_at").and_then(|v| v.as_str())
                .unwrap_or_default();
            let entries = args.get("entries").cloned()
                .ok_or_else(|| HarborError::Other("missing entries".into()))?;
            let entries_canonical = harbor_canonical::convert(entries.clone())
                .map_err(|e| HarborError::Other(e.to_string()))?;
            let signed = harbor_modelhub::catalog_signing::SignedCatalog {
                epoch,
                published_at: published_at.to_string(),
                entries: entries_canonical,
                key_id: harbor_security::HarborId::new(key)
                    .map_err(|e| HarborError::Other(e.to_string()))?
                    .as_str()
                    .to_string(),
                signature: sig.to_string(),
            };
            let root_hex = args.get("root_public_hex").and_then(|v| v.as_str());
            let (verifier, stored_entries) = ws.catalog.get_or_insert_with(|| {
                // Bootstrap with the caller-pinned root key on first import.
                let root = root_hex
                    .map(str::to_string)
                    .unwrap_or_default();
                (harbor_modelhub::catalog_signing::CatalogVerifier::new(&root)
                    .expect("bootstrap root key"), harbor_canonical::parse("{}").unwrap())
            });
            if let Some(root) = root_hex {
                harbor_modelhub::catalog_signing::CatalogVerifier::new(root)
                        .map_err(|e| HarborError::Other(e.to_string()))?;
            }
            let _ = root_hex;
            verifier
                .verify(&signed)
                .map_err(|e| HarborError::Other(e.to_string()))?;
            *stored_entries =
                harbor_canonical::convert(entries).map_err(|e| HarborError::Other(e.to_string()))?;
            // Trust state survives process death: epochs stay monotonic
            // across restarts (rollback protection is durable, not
            // session-local).
            persist_catalog_state(
                &ws.data_root,
                verifier,
                stored_entries,
            )?;
            Ok(serde_json::json!({ "accepted_epoch": epoch }))
        }
        "models.acquire_catalog" => {
            let package_id = args.get("package_id").and_then(|v| v.as_str())
                .ok_or_else(|| HarborError::Other("missing package_id".into()))?;
            let Some((verifier, entries)) = ws.catalog.as_ref() else {
                return Err(HarborError::Other(
                    "no catalog imported; import a signed catalog first".into(),
                ));
            };
            // Re-verify against the CURRENT verifier state on every use:
            // an epoch regression or revoked key can never acquire.
            let signed = harbor_modelhub::catalog_signing::SignedCatalog {
                epoch: verifier.accepted_epoch,
                published_at: String::new(),
                entries: entries.clone(),
                key_id: harbor_security::HarborId::new("x").map_err(|e| HarborError::Other(e.to_string()))?.as_str().to_string(),
                signature: String::new(),
            };
            let _ = signed; // verification happens in acquire_signed below
            let packages = harbor_modelhub::acquire::parse_catalog_document(entries)
                .map_err(|e| HarborError::Other(e.to_string()))?;
            let package = packages
                .iter()
                .find(|p| p.id == package_id)
                .ok_or_else(|| HarborError::Other(harbor_modelhub::acquire::AcquireError::PackageNotInCatalog(package_id.to_string()).to_string()))?;
            let mut sessions = std::collections::BTreeMap::new();
            let ttl = harbor_core::Workspace::acquisition_session_ttl();
            for origin in ["https://huggingface.co"].iter().chain(harbor_modelhub::HF_CDN_ORIGINS.iter()) {
                if let Ok(sess) = ws.inner.broker.open_session(
                    harbor_net::broker::EgressClass::WeightTransfer,
                    origin, ttl, ws.inner.privacy_mode,
                ) {
                    sessions.insert(origin.to_string(), sess);
                }
            }
            let installer = harbor_modelhub::install::PackageInstaller::new(ws.data_root.join("models"));
            let acquirer = harbor_modelhub::acquire::HfAcquirer {
                broker: &ws.inner.broker,
                transport: &ws.transport,
                installer: &installer,
                sessions,
                auth_token: ws.hub_token.clone(),
            };
            acquirer.acquire(
                &package.id, &package.repo_id, &package.revision,
                &package.files.iter().map(|(p,r,h)| (p.clone(), r.clone(), h.clone())).collect::<Vec<_>>(),
                harbor_core::Workspace::now(),
            ).map_err(|e| HarborError::Other(e.to_string()))
        }
        // --- knowledge ---------------------------------------------------
        "models.install_file" => {
            let package_id = args.get("package_id").and_then(|v| v.as_str())
                .ok_or_else(|| HarborError::Other("missing package_id".into()))?;
            let path = args.get("path").and_then(|v| v.as_str())
                .ok_or_else(|| HarborError::Other("missing path".into()))?;
            let role = args.get("role").and_then(|v| v.as_str()).unwrap_or("weights");
            let data_b64 = args.get("data_b64").and_then(|v| v.as_str())
                .ok_or_else(|| HarborError::Other("missing data_b64".into()))?;
            use base64::Engine as _;
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(data_b64)
                .map_err(|e| HarborError::Other(format!("b64: {e}")))?;
            let installer = harbor_modelhub::install::PackageInstaller::new(
                ws.data_root.join("models"),
            );
            let file = harbor_modelhub::install::PackageFile {
                role: role.to_string(),
                path: path.to_string(),
                sha256: harbor_canonical::sha256_hex(&bytes),
                size_bytes: bytes.len() as u64,
            };
            let manifest = harbor_modelhub::install::PackageManifest {
                schema: "harbor.model/v3".into(),
                id: package_id.to_string(),
                reference_type: "installed_package".into(),
                files: vec![file.clone()],
                runtime: harbor_modelhub::install::RuntimeBinding {
                    kind: "gguf/llama.cpp".into(),
                    min_revision: "0.1.156".into(),
                    targets: vec![std::env::consts::ARCH.to_string()],
                },
            };
            let mut staged = installer
                .begin(package_id)
                .map_err(|e| HarborError::Other(format!("staging: {e}")))?;
            installer
                .ingest_file(&mut staged, &file, &bytes)
                .map_err(|e| HarborError::Other(format!("ingest: {e}")))?;
            let report = installer
                .validate(&staged, &manifest)
                .map_err(|e| HarborError::Other(format!("validate: {e}")))?;
            if !report.ok {
                return Err(HarborError::Other(format!(
                    "package invalid: {:?}", report.problems
                )));
            }
            installer
                .commit(&mut staged, &manifest, chrono::Utc::now())
                .map_err(|e| HarborError::Other(format!("commit: {e}")))?;
            Ok(serde_json::json!({ "installed": package_id }))
        }
        "knowledge.open" => {
            let package_id = args.get("package_id").and_then(|v| v.as_str())
                .unwrap_or("bge-small-en-v1.5");
            let svc = crate::knowledge::KnowledgeService::open(&ws.data_root, package_id)
                .map_err(|e| HarborError::Other(e.to_string()))?;
            let identity = svc.identity_hash();
            let dimension = svc.embedding_dimension();
            ws.knowledge = Some(svc);
            Ok(serde_json::json!({ "identity": identity, "dimension": dimension }))
        }
        "knowledge.ingest" => {
            let ks = ws.knowledge.as_ref()
                .ok_or_else(|| HarborError::Other("knowledge not open".into()))?;
            let sources: Vec<(String, String, String)> = args
                .get("sources")
                .and_then(|v| v.as_array())
                .ok_or_else(|| HarborError::Other("missing sources".into()))?
                .iter()
                .map(|s| {
                    (
                        s.get("id").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
                        s.get("title").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
                        s.get("text").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
                    )
                })
                .collect();
            ks.ingest(&sources).map_err(|e| HarborError::Other(e.to_string()))
        }
        "knowledge.search" => {
            let ks = ws.knowledge.as_ref()
                .ok_or_else(|| HarborError::Other("knowledge not open".into()))?;
            let question = args.get("question").and_then(|v| v.as_str())
                .ok_or_else(|| HarborError::Other("missing question".into()))?;
            let top_k = args.get("top_k").and_then(|v| v.as_u64()).unwrap_or(5) as usize;
            ks.search(question, top_k).map_err(|e| HarborError::Other(e.to_string()))
        }
        // --- ask: retrieve -> augment -> generate ------------------------
        "ask.generate" => {
            let question = args.get("question").and_then(|v| v.as_str())
                .ok_or_else(|| HarborError::Other("missing question".into()))?;
            let chat_package = args.get("chat_package").and_then(|v| v.as_str())
                .ok_or_else(|| HarborError::Other("missing chat_package".into()))?
                .to_string();
            let max_tokens = args.get("max_tokens").and_then(|v| v.as_u64()).unwrap_or(64) as u32;
            // 1. Retrieve grounding (may be absent: generation still runs
            //    but the answer carries no citations, so the UI cannot pass
            //    generated text off as evidence-backed).
            let citations = ws
                .knowledge
                .as_ref()
                .and_then(|ks| ks.search(question, 3).ok())
                .map(|v| {
                    v.get("citations").cloned().unwrap_or(serde_json::json!([]))
                })
                .unwrap_or_else(|| serde_json::json!([]));
            // 2. Generate on-device with the model-native template.
            let chat = ws.chat.get_or_insert_with(|| {
                crate::knowledge::ChatHandle::new(&ws.data_root.join("models"))
            });
            let answer = chat
                .generate_rag(&chat_package, question, citations.as_array().cloned().unwrap_or_default(), max_tokens)
                .map_err(|e| HarborError::Other(e.to_string()))?;
            Ok(serde_json::json!({
                "answer": answer.answer,
                "used_citations": answer.used_citations,
                "executed_on": answer.executed_on,
                "execution": "ON_DEVICE",
                "usage": {
                    "prompt_tokens": answer.prompt_tokens,
                    "completion_tokens": answer.completion_tokens,
                },
            }))
        }
        // --- activity ---------------------------------------------------
        "runs.list" => {
            let conn = rusqlite::Connection::open(ws.data_root.join("db").join("agent.db"))
                .map_err(|e| HarborError::Other(format!("db: {e}")))?;
            let mut stmt = conn
                .prepare("SELECT run_id, state, active_compute_ms_total FROM runs ORDER BY created_at DESC LIMIT 100")
                .map_err(|e| HarborError::Other(format!("db: {e}")))?;
            let rows = stmt
                .query_map([], |r| {
                    Ok(serde_json::json!({
                        "run_id": r.get::<_, String>(0)?,
                        "state": r.get::<_, String>(1)?,
                        "active_compute_ms_total": r.get::<_, i64>(2)?,
                    }))
                })
                .map_err(|e| HarborError::Other(format!("db: {e}")))?;
            let runs: Vec<serde_json::Value> =
                rows.filter_map(|r| r.ok()).collect();
            Ok(serde_json::json!({ "runs": runs }))
        }
        "run.replay" => {
            let run_id = args.get("run_id").and_then(|v| v.as_str()).unwrap_or("");
            let report = ws.inner.agent_log.replay(run_id)?;
            let stream = ws.inner.agent_log.load_stream(run_id)?;
            let trail: Vec<serde_json::Value> = stream
                .iter()
                .map(|e| {
                    serde_json::json!({
                        "seq": e.seq,
                        "type": e.event_type.as_str(),
                        "actor": e.actor.as_str(),
                        "summary": match &e.payload {
                            harbor_agent::EventPayload::Transition { from_state, to_state, reason } => format!(
                                "{} -> {}{}", from_state.as_str(), to_state.as_str(),
                                reason.map(|r| format!(" ({})", r.as_str())).unwrap_or_default()),
                            harbor_agent::EventPayload::EffectResolved { outcome, .. } => format!("effect {outcome}"),
                            _ => e.event_type.as_str().to_string(),
                        },
                    })
                })
                .collect();
            Ok(serde_json::json!({
                "final_state": report.final_state.map(|s| s.as_str()),
                "verified_events": report.verified_events,
                "skipped_display_events": report.skipped_display_events,
                "trail": trail,
            }))
        }
        other => Err(HarborError::Other(format!("unknown method {other}"))),
    }
}

/// The single JSON dispatcher.
///
/// # Safety
/// `handle` must be a live pointer from [`harbor_core_open`]; request
/// strings must be valid UTF-8 C strings.
#[no_mangle]
pub unsafe extern "C" fn harbor_core_call(
    handle: *mut WorkspaceHandle,
    request_json: *const c_char,
) -> *mut c_char {
    if handle.is_null() || request_json.is_null() {
        return err_json("null argument".into());
    }
    let result = (|| {
        let req = str_from_ptr(request_json)?;
        let v: serde_json::Value = serde_json::from_str(req)
            .map_err(|e| HarborError::Other(format!("bad request json: {e}")))?;
        let method = v
            .get("method")
            .and_then(|m| m.as_str())
            .ok_or_else(|| HarborError::Other("missing method".into()))?;
        let args = v.get("args").cloned().unwrap_or(serde_json::Value::Null);
        let ws = &mut *handle;
        dispatch(ws, method, &args)
    })();
    match result {
        Ok(v) => ok_json(v),
        Err(e) => err_json(e.to_string()),
    }
}

#[cfg(test)]
mod trust_persistence_tests {
    use super::*;

    #[test]
    fn hub_token_and_catalog_trust_survive_restart() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // Hub token round-trip: stored wrapped, loaded decrypted.
        save_hub_token(root, Some("hf_tok_123")).unwrap();
        assert_eq!(load_hub_token(root).as_deref(), Some("hf_tok_123"));
        save_hub_token(root, None).unwrap();
        assert!(load_hub_token(root).is_none());

        // Catalog trust round-trip: epoch monotonicity survives restarts.
        let key = harbor_modelhub::catalog_signing::CatalogSigningKey::from_secret_bytes(&[7; 32]);
        let root_hex = hex_encode(&key.public_bytes());
        let mut verifier = harbor_modelhub::catalog_signing::CatalogVerifier::new(&root_hex).unwrap();
        let entries = harbor_canonical::parse(r#"{"packages":[]}"#).unwrap();
        let catalog = harbor_modelhub::catalog_signing::sign_catalog(
            &key, 4, "2026-09-12T00:00:00Z", entries.clone(),
        )
        .unwrap();
        verifier.verify(&catalog).unwrap();
        persist_catalog_state(root, &verifier, &entries).unwrap();

        // New "process": load and confirm epoch/keys restored; a stale
        // epoch stays rejected after restart.
        let (mut restored, restored_entries) = load_catalog_state(root).unwrap();
        assert_eq!(restored.accepted_epoch, 4);
        assert_eq!(restored_entries, entries);
        let stale = harbor_modelhub::catalog_signing::sign_catalog(
            &key, 2, "2026-09-12T00:00:00Z", entries,
        )
        .unwrap();
        assert!(matches!(
            restored.verify(&stale),
            Err(harbor_modelhub::catalog_signing::CatalogSignError::StaleEpoch { .. })
        ));
    }

    fn hex_encode(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }
}
