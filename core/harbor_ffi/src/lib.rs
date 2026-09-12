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
use harbor_core::HarborError;
use harbor_security::policy::PrivacyMode;

pub struct WorkspaceHandle {
    inner: Workspace,
    data_root: std::path::PathBuf,
    /// Created on demand when an embedding model is available.
    knowledge: Option<crate::knowledge::KnowledgeService>,
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
        let opts = OpenOptions {
            data_root: std::path::PathBuf::from(root),
            device_id: "device".into(),
        };
        let ws = harbor_core::Workspace::open(&opts, ws_id, mode)?;
        Ok(WorkspaceHandle { inner: ws, data_root: opts.data_root, knowledge: None })
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
            if bytes.len() < 4 || &bytes[..2] != b"PK" {
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
            let is_deck = content_types.contains("presentationml");
            drop(archive);
            if is_deck {
                let p = harbor_render::DeckPreview::from_pptx(&bytes)
                    .map_err(|e| HarborError::Other(e.to_string()))?;
                Ok(serde_json::json!({
                    "kind": "deck",
                    "preview": serde_json::to_value(&p).map_err(|e| HarborError::Other(e.to_string()))?,
                }))
            } else {
                let p = harbor_render::WorkbookPreview::from_xlsx(&bytes)
                    .map_err(|e| HarborError::Other(e.to_string()))?;
                Ok(serde_json::json!({
                    "kind": "workbook",
                    "preview": serde_json::to_value(&p).map_err(|e| HarborError::Other(e.to_string()))?,
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
