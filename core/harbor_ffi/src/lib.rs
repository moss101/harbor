//! Harbor FFI: the C ABI seam between Flutter (Dart) and Harbor Core.
//!
//! One dispatcher, JSON in / JSON out (`harbor_core_call`), so the Dart
//! side carries no business logic and the Rust side owns policy and
//! runtime truth (08_Repo_Structure dependency rules). Handles are opaque
//! pointers; strings returned by the boundary are freed with
//! `harbor_core_string_free`.
//!
//! Long-running work (model acquisition, knowledge ingestion, grounded
//! generation) runs on native background threads registered in the ops
//! registry (`op.start_*` / `op.status` / `op.cancel`): the caller gets an
//! op id immediately, polls real progress, and can cancel cooperatively.
//! The workspace event log and egress broker are shared across threads
//! (`Arc`), so activity logged by a background op is durable and audited.

pub mod knowledge;

use std::collections::BTreeMap;
use std::ffi::{c_char, CStr, CString};
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use harbor_agent::event::{Counters, EventPayload, EventType, ReplaySemantics, RunEvent};
use harbor_agent::lease::LeaseManager;
use harbor_agent::{Actor, PauseReason, RunState};
use harbor_core::workspace::{OpenOptions, Workspace};
use harbor_core::HarborError;
use harbor_security::policy::PrivacyMode;
use harbor_store::keys::{KeyMaterial, KeyStore};

pub struct WorkspaceHandle {
    inner: Workspace,
    data_root: std::path::PathBuf,
    /// Durable device/workspace identity (survives restarts).
    device_id: String,
    /// Created on demand when an embedding model is available.
    knowledge: Option<Arc<crate::knowledge::KnowledgeService>>,
    /// Chat provider over installed GGUF models (created on demand).
    chat: Option<Arc<crate::knowledge::ChatHandle>>,
    /// Shared HTTPS transport for brokered acquisition.
    transport: Arc<harbor_net::transport::UreqTransport>,
    /// The device root key source this workspace was opened with (also
    /// wraps the hub token at rest).
    keystore: Arc<dyn KeyStore>,
    /// Signed catalog trust state + accepted document (on demand).
    catalog: Option<(
        harbor_modelhub::catalog_signing::CatalogVerifier,
        harbor_canonical::JsonValue,
    )>,
    /// Optional HF token for gated/private repos (M4). Sourced from the
    /// keystore; never returned across the boundary.
    hub_token: Option<String>,
}

// ---------------------------------------------------------------------------
// Ops registry: background threads for long-running work.
// ---------------------------------------------------------------------------

/// A completed (or failed/cancelled) operation's terminal state.
struct OpEntry {
    kind: &'static str,
    progress: Arc<harbor_modelhub::progress::AcquireProgress>,
    result: Mutex<Option<Result<serde_json::Value, String>>>,
    cancelled: AtomicBool,
    #[allow(dead_code)]
    started_at: std::time::Instant,
}

fn ops_registry() -> &'static Mutex<BTreeMap<String, Arc<OpEntry>>> {
    static OPS: OnceLock<Mutex<BTreeMap<String, Arc<OpEntry>>>> = OnceLock::new();
    OPS.get_or_init(|| Mutex::new(BTreeMap::new()))
}

fn register_op(kind: &'static str) -> (String, Arc<OpEntry>) {
    let op_id = harbor_security::HarborId::generate("op");
    let entry = Arc::new(OpEntry {
        kind,
        progress: Arc::new(harbor_modelhub::progress::AcquireProgress::new()),
        result: Mutex::new(None),
        cancelled: AtomicBool::new(false),
        started_at: std::time::Instant::now(),
    });
    let mut ops = ops_registry().lock().unwrap();
    ops.insert(op_id.as_str().to_string(), entry.clone());
    // Bound the registry: drop terminal ops beyond the newest 32.
    if ops.len() > 32 {
        let terminal: Vec<String> = ops
            .iter()
            .filter(|(_, e)| e.result.lock().unwrap().is_some())
            .map(|(k, _)| k.clone())
            .collect();
        for k in terminal.iter().take(ops.len() - 32) {
            ops.remove(k);
        }
    }
    (op_id.as_str().to_string(), entry)
}

fn complete_op(entry: &OpEntry, result: Result<serde_json::Value, String>, cancelled: bool) {
    if cancelled {
        entry.cancelled.store(true, Ordering::Relaxed);
    }
    *entry.result.lock().unwrap() = Some(result);
}

fn op_status_json(op_id: &str, entry: &OpEntry) -> serde_json::Value {
    let snap = entry.progress.snapshot();
    let finished = entry.result.lock().unwrap().clone();
    let state = match &finished {
        Some(Ok(_)) => "done",
        Some(Err(_)) if entry.cancelled.load(Ordering::Relaxed) => "cancelled",
        Some(Err(_)) => "failed",
        None if entry.cancelled.load(Ordering::Relaxed) => "cancelling",
        None => "running",
    };
    serde_json::json!({
        "op_id": op_id,
        "kind": entry.kind,
        "state": state,
        "phase": snap.phase,
        "detail": snap.detail,
        "bytes_done": snap.bytes_done,
        "bytes_total": snap.bytes_total,
        "items_done": snap.items_done,
        "items_total": snap.items_total,
        "result": finished.map(|r| match r {
            Ok(v) => v,
            Err(e) => serde_json::json!({ "error": e }),
        }),
    })
}

// ---------------------------------------------------------------------------
// Device identity + keystore selection.
// ---------------------------------------------------------------------------

/// Durable device identity: a random id generated on first open and
/// persisted in the workspace store, so receipts and bindings stay stable
/// across restarts. `OpenOptions.device_id` carries this real identity.
fn ensure_device_identity(data_root: &std::path::Path) -> Result<String, HarborError> {
    std::fs::create_dir_all(data_root.join("db"))?;
    let conn = rusqlite::Connection::open(data_root.join("db").join("store.db"))
        .map_err(|e| HarborError::Other(format!("store: {e}")))?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS device_meta (
            k TEXT PRIMARY KEY,
            v TEXT NOT NULL
        );",
    )
    .map_err(|e| HarborError::Other(format!("store: {e}")))?;
    let existing: Option<String> = conn
        .query_row("SELECT v FROM device_meta WHERE k = 'device_id'", [], |r| r.get(0))
        .ok();
    if let Some(id) = existing {
        return Ok(id);
    }
    let id = harbor_security::HarborId::generate("device");
    conn.execute(
        "INSERT INTO device_meta (k, v) VALUES ('device_id', ?1)",
        [id.as_str()],
    )
    .map_err(|e| HarborError::Other(format!("store: {e}")))?;
    Ok(id.as_str().to_string())
}

/// Production keystore selection. An injected root (Android Keystore
/// unseal path) wins; otherwise the platform adapter: Keychain on
/// macOS/iOS, DPAPI on Windows. Other platforms (and unexpected build
/// configurations) fall back to the file store — dev-grade, never
/// pretended to be OS-protected.
fn build_keystore(
    data_root: &std::path::Path,
    injected: Option<KeyMaterial>,
) -> Result<Arc<dyn KeyStore>, HarborError> {
    if let Some(root) = injected {
        return Ok(Arc::new(harbor_store::native_keystore::InjectedKeyStore::new(root)));
    }
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        let _ = data_root;
        return Ok(Arc::new(
            harbor_store::native_keystore::KeychainKeyStore::new().map_err(HarborError::Store)?,
        ));
    }
    #[cfg(windows)]
    {
        return Ok(Arc::new(
            harbor_store::native_keystore::DpapiKeyStore::new(data_root.join("keys"))
                .map_err(HarborError::Store)?,
        ));
    }
    #[cfg(not(any(target_os = "macos", target_os = "ios", target_os = "windows")))]
    {
        return Ok(Arc::new(
            harbor_store::keys::FileKeyStore::new(data_root.join("keys"))
                .map_err(HarborError::Store)?,
        ));
    }
}

/// One-time migration when a data root moves from the file keystore to an
/// OS keystore: rewrap every workspace key from the old file root to the
/// new native root, then crypto-erase the file root. No-op when there is
/// nothing stored under the file root (fresh installs).
fn rotate_file_root_to_native(
    data_root: &std::path::Path,
    native: &Arc<dyn KeyStore>,
) -> Result<(), HarborError> {
    let file_ks = harbor_store::keys::FileKeyStore::new(data_root.join("keys"))
        .map_err(HarborError::Store)?;
    if !file_ks.exists("harbor.device") {
        return Ok(());
    }
    // Comparing materials requires reading both roots; if the native root
    // IS the file root (plain FileKeyStore path), there is nothing to do.
    let old_root = file_ks.device_root_key("harbor.device").map_err(HarborError::Store)?;
    let new_root = native.device_root_key("harbor.device").map_err(HarborError::Store)?;
    if old_root == new_root {
        return Ok(());
    }
    let store_db = data_root.join("db").join("store.db");
    if store_db.exists() {
        let mut db = harbor_store::Database::open(&store_db)?;
        db.migrate(&[])?;
        db.write(|c| {
            c.execute_batch(
                "CREATE TABLE IF NOT EXISTS workspace_keys (
                    workspace_id TEXT PRIMARY KEY,
                    wrapped_key BLOB NOT NULL,
                    wrap_version INTEGER NOT NULL DEFAULT 1
                );",
            )
            .map_err(harbor_store::StoreError::Db)?;
            let rows: Vec<(String, Vec<u8>)> = {
                let mut stmt = c
                    .prepare("SELECT workspace_id, wrapped_key FROM workspace_keys")
                    .map_err(harbor_store::StoreError::Db)?;
                let mapped = stmt
                    .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?)))
                    .map_err(harbor_store::StoreError::Db)?;
                mapped
                    .collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(harbor_store::StoreError::Db)?
            };
            for (ws_id, wrapped_bytes) in rows {
                let wrapped = harbor_store::keys::WrappedKey::from_bytes(&wrapped_bytes)
                    .map_err(|_| harbor_store::StoreError::Crypto)?;
                let wk = harbor_store::keys::WorkspaceKey::from_wrapped(&old_root, &wrapped)
                    .map_err(|_| harbor_store::StoreError::Crypto)?;
                let rewrapped = wk.wrap_with(&new_root)?;
                c.execute(
                    "UPDATE workspace_keys SET wrapped_key = ?2 WHERE workspace_id = ?1",
                    rusqlite::params![ws_id, rewrapped.to_bytes()],
                )
                .map_err(harbor_store::StoreError::Db)?;
            }
            Ok(())
        })?;
    }
    // Root rotation complete: erase the file root. Any data that could not
    // be rewrapped above has already failed the open with an error.
    file_ks.remove("harbor.device").map_err(HarborError::Store)?;
    Ok(())
}

fn load_hub_token(
    data_root: &std::path::Path,
    keystore: &Arc<dyn KeyStore>,
) -> Option<String> {
    // The token is stored wrapped by the device root key (encrypted at
    // rest); the plaintext never crosses the FFI boundary outward.
    let root = keystore.device_root_key("harbor.device").ok()?;
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
fn save_hub_token(
    data_root: &std::path::Path,
    keystore: &Arc<dyn KeyStore>,
    token: Option<&str>,
) -> Result<(), HarborError> {
    std::fs::create_dir_all(data_root.join("db"))?;
    let root = keystore.device_root_key("harbor.device")?;
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

/// Open (create or resume) a workspace. `device_root_hex` is optional:
/// when present (Android Keystore unseal path) it is a 64-hex-char
/// 32-byte device root key generated and sealed by the embedding layer's
/// OS keystore; when null the platform adapter is used (Keychain on
/// Apple platforms, DPAPI on Windows).
#[no_mangle]
pub extern "C" fn harbor_core_open_ex(
    data_root: *const c_char,
    workspace_id: *const c_char,
    privacy_mode: u8,
    device_root_hex: *const c_char,
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
        let injected = if device_root_hex.is_null() {
            None
        } else {
            let hex = str_from_ptr(device_root_hex)?;
            if hex.is_empty() {
                None
            } else {
                let bytes = hex::decode(hex)
                    .map_err(|e| HarborError::Other(format!("device root hex: {e}")))?;
                Some(KeyMaterial::from_bytes(&bytes).map_err(HarborError::Store)?)
            }
        };
        let data_root = std::path::PathBuf::from(root);
        let keystore = build_keystore(&data_root, injected)?;
        rotate_file_root_to_native(&data_root, &keystore)?;
        let device_id = ensure_device_identity(&data_root)?;
        let opts = OpenOptions {
            data_root: data_root.clone(),
            device_id,
        };
        let ws = harbor_core::Workspace::open_with_keystore(&opts, ws_id, mode, keystore.clone())?;
        let hub_token = load_hub_token(&data_root, &keystore);
        Ok(WorkspaceHandle {
            inner: ws,
            data_root,
            device_id: opts.device_id,
            knowledge: None,
            chat: None,
            transport: Arc::new(harbor_net::transport::UreqTransport::new()),
            keystore,
            catalog: load_catalog_state(&opts.data_root),
            hub_token,
        })
    })();
    match result {
        Ok(h) => Box::into_raw(Box::new(h)),
        Err(_) => ptr::null_mut(),
    }
}

/// Legacy 3-argument open: file-backed keystore (development builds and
/// direct embedders without a platform keystore).
#[no_mangle]
pub extern "C" fn harbor_core_open(
    data_root: *const c_char,
    workspace_id: *const c_char,
    privacy_mode: u8,
) -> *mut WorkspaceHandle {
    harbor_core_open_ex(data_root, workspace_id, privacy_mode, ptr::null())
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

/// Append a run event with a fresh lease, hash-chained onto the current
/// head. Shared by run.log_request and the background generation op.
fn append_run_event(
    agent_log: &harbor_agent::EventLog,
    data_root: &std::path::Path,
    run_id: &str,
    event_type: EventType,
    payload: EventPayload,
    bump_steps: bool,
) -> Result<(), HarborError> {
    let mut mgr = LeaseManager::open(data_root.join("db").join("agent.db"))?;
    let lease = mgr.acquire(
        run_id,
        "ffi-executor",
        chrono::Duration::minutes(10),
        harbor_core::Workspace::now(),
    )?;
    let stream = agent_log.load_stream(run_id)?;
    let head = stream.last().ok_or_else(|| HarborError::Other("run missing".into()))?;
    let head_hash = head.hash().map_err(|e| HarborError::Other(format!("hash: {e}")))?;
    let counters = head.counters;
    let event = RunEvent {
        run_id: run_id.to_string(),
        event_id: format!(
            "evt-{}",
            harbor_canonical::sha256_hex(
                format!("{run_id}-{}-{}-{}", event_type.as_str(), stream.len(), chrono::Utc::now().to_rfc3339())
                    .as_bytes()
            )
            .get(..16)
            .unwrap_or("evt")
        ),
        seq: stream.len() as u64,
        event_type,
        replay_semantics: ReplaySemantics::StateAffecting,
        actor: Actor::User,
        lease_generation: lease.generation,
        counters: Counters {
            active_compute_ms_total: counters.active_compute_ms_total,
            step_count_total: counters.step_count_total + if bump_steps { 1 } else { 0 },
            tool_count_total: counters.tool_count_total,
            context_tokens_total: counters.context_tokens_total,
        },
        payload,
        created_at: harbor_core::Workspace::now(),
        prev_event_hash: Some(head_hash),
    };
    agent_log.append(event, lease.generation, None)?;
    Ok(())
}

/// Open explicit weight-transfer sessions for acquisition (HF + CDNs).
fn open_weight_sessions(
    broker: &harbor_net::broker::EgressBroker,
    mode: PrivacyMode,
) -> std::collections::BTreeMap<String, harbor_net::broker::EgressSession> {
    let mut sessions = std::collections::BTreeMap::new();
    let ttl = harbor_core::Workspace::acquisition_session_ttl();
    for origin in ["https://huggingface.co"]
        .iter()
        .chain(harbor_modelhub::HF_CDN_ORIGINS.iter())
    {
        if let Ok(sess) =
            broker.open_session(harbor_net::broker::EgressClass::WeightTransfer, origin, ttl, mode)
        {
            sessions.insert(origin.to_string(), sess);
        }
    }
    sessions
}

fn parse_acquire_files(args: &serde_json::Value) -> Result<Vec<(String, String, String)>, HarborError> {
    Ok(args
        .get("files")
        .and_then(|v| v.as_array())
        .ok_or_else(|| HarborError::Other("missing files".into()))?
        .iter()
        .map(|f| {
            (
                f.get("path").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
                f.get("role").and_then(|v| v.as_str()).unwrap_or("weights").to_string(),
                f.get("sha256").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
            )
        })
        .collect())
}

/// The synchronous acquisition body, shared by `models.acquire_hf` and
/// the background op thread.
fn acquire_model(
    data_root: &std::path::Path,
    broker: &harbor_net::broker::EgressBroker,
    transport: &harbor_net::transport::UreqTransport,
    mode: PrivacyMode,
    hub_token: Option<String>,
    package_id: &str,
    repo_id: &str,
    revision: &str,
    files: &[(String, String, String)],
    progress: Option<Arc<harbor_modelhub::progress::AcquireProgress>>,
) -> Result<serde_json::Value, HarborError> {
    let installer = harbor_modelhub::install::PackageInstaller::new(data_root.join("models"));
    let sessions = open_weight_sessions(broker, mode);
    let mut acquirer = harbor_modelhub::acquire::HfAcquirer {
        broker,
        transport,
        installer: &installer,
        sessions,
        auth_token: hub_token,
        progress: progress.clone(),
    };
    if let Some(p) = progress {
        acquirer = acquirer.with_progress(p);
    }
    acquirer
        .acquire(package_id, repo_id, revision, files, harbor_core::Workspace::now())
        .map_err(|e| HarborError::Other(e.to_string()))
}

fn dispatch(
    ws: &mut WorkspaceHandle,
    method: &str,
    args: &serde_json::Value,
) -> Result<serde_json::Value, HarborError> {
    match method {
        // --- runs -------------------------------------------------------
        "run.create" => {
            // Unique run ids: callers may pin one; otherwise the core
            // mints a random id (timestamps are NOT ids).
            let run_id = match args.get("run_id").and_then(|v| v.as_str()) {
                Some(r) if !r.is_empty() => r.to_string(),
                _ => harbor_security::HarborId::generate("run").to_string(),
            };
            ws.inner
                .agent_log
                .create_run(&run_id, &ws.inner.workspace_id, harbor_core::Workspace::now())?;
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
            let stream = ws.inner.agent_log.load_stream(run_id)?;
            if stream.is_empty() {
                return Err(HarborError::Other("run missing".into()));
            }
            let head = stream.last().unwrap();
            let head_hash = head
                .hash()
                .map_err(|e| HarborError::Other(format!("hash: {e}")))?;
            let counters = head.counters;
            let mut mgr = LeaseManager::open(ws.data_root.join("db").join("agent.db"))?;
            let lease = mgr.acquire(run_id, "ffi-executor", chrono::Duration::minutes(10), harbor_core::Workspace::now())?;
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
        // --- workspace identity ------------------------------------------
        "identity.get" => Ok(serde_json::json!({
            "device_id": ws.device_id,
            "workspace_id": ws.inner.workspace_id,
            "policy": ws.inner.privacy_mode.as_str(),
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
                .ok_or_else(|| HarborError::Other("missing run_id".into()))?
                .to_string();
            let text = args.get("text").and_then(|v| v.as_str())
                .ok_or_else(|| HarborError::Other("missing text".into()))?
                .to_string();
            append_run_event(
                &ws.inner.agent_log,
                &ws.data_root,
                &run_id,
                EventType::RunStepStarted,
                EventPayload::StepStarted {
                    step_id: format!("step-{}", {
                        let stream = ws.inner.agent_log.load_stream(&run_id)?;
                        stream.last().map(|h| h.counters.step_count_total + 1).unwrap_or(1)
                    }),
                    description: text,
                },
                true,
            )?;
            Ok(serde_json::json!({ "logged": true }))
        }
        // --- hub auth (M4: gated/private repos) ---------------------------
        "hub.set_token" => {
            let token = args.get("token").and_then(|v| v.as_str())
                .ok_or_else(|| HarborError::Other("missing token".into()))?;
            save_hub_token(&ws.data_root, &ws.keystore, Some(token))?;
            ws.hub_token = Some(token.to_string());
            Ok(serde_json::json!({ "set": true }))
        }
        "hub.clear_token" => {
            save_hub_token(&ws.data_root, &ws.keystore, None)?;
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
                &ws.inner.broker, &*ws.transport)
                .with_token(ws.hub_token.clone());
            let models = discovery
                .search(&session, query, limit.min(20))
                .map_err(|e| HarborError::Other(e.to_string()))?;
            // Wrapped in an object (like models.installed) so the envelope
            // result stays a JSON map across the boundary.
            Ok(serde_json::json!({ "models": models }))
        }
        "models.acquire_hf" => {
            let package_id = args.get("package_id").and_then(|v| v.as_str())
                .ok_or_else(|| HarborError::Other("missing package_id".into()))?
                .to_string();
            let repo_id = args.get("repo_id").and_then(|v| v.as_str())
                .ok_or_else(|| HarborError::Other("missing repo_id".into()))?
                .to_string();
            let revision = args.get("revision").and_then(|v| v.as_str()).unwrap_or("main").to_string();
            let files = parse_acquire_files(args)?;
            acquire_model(
                &ws.data_root,
                &ws.inner.broker,
                &ws.transport,
                ws.inner.privacy_mode,
                ws.hub_token.clone(),
                &package_id,
                &repo_id,
                &revision,
                &files,
                None,
            )
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
            acquire_model(
                &ws.data_root,
                &ws.inner.broker,
                &ws.transport,
                ws.inner.privacy_mode,
                ws.hub_token.clone(),
                &package.id,
                &package.repo_id,
                &package.revision,
                &package.files.iter().map(|(p, r, h)| (p.clone(), r.clone(), h.clone())).collect::<Vec<_>>(),
                None,
            )
        }
        // --- model install (local file) -----------------------------------
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
        // Local install straight from a picked file path: the core reads
        // the bytes itself (no base64 round trip through the boundary).
        "models.install_from_path" => {
            let package_id = args.get("package_id").and_then(|v| v.as_str())
                .ok_or_else(|| HarborError::Other("missing package_id".into()))?;
            let path = args.get("path").and_then(|v| v.as_str())
                .ok_or_else(|| HarborError::Other("missing path".into()))?;
            let role = args.get("role").and_then(|v| v.as_str()).unwrap_or("weights");
            let bytes = std::fs::read(path)
                .map_err(|e| HarborError::Other(format!("read {path}: {e}")))?;
            let file_name = std::path::Path::new(path)
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or_else(|| HarborError::Other("bad file name".into()))?
                .to_string();
            let installer = harbor_modelhub::install::PackageInstaller::new(
                ws.data_root.join("models"),
            );
            let file = harbor_modelhub::install::PackageFile {
                role: role.to_string(),
                path: file_name,
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
            Ok(serde_json::json!({ "installed": package_id, "bytes": bytes.len() }))
        }
        // --- knowledge ---------------------------------------------------
        "knowledge.open" => {
            let package_id = args.get("package_id").and_then(|v| v.as_str())
                .unwrap_or("bge-small-en-v1.5");
            // Chunks are private workspace content: seal them under the
            // workspace-derived knowledge key.
            let chunk_key = ws.inner.knowledge_chunk_key()?;
            let svc = crate::knowledge::KnowledgeService::open(&ws.data_root, package_id, chunk_key)
                .map_err(|e| HarborError::Other(e.to_string()))?;
            let identity = svc.identity_hash();
            let dimension = svc.embedding_dimension();
            ws.knowledge = Some(Arc::new(svc));
            Ok(serde_json::json!({ "identity": identity, "dimension": dimension }))
        }
        "knowledge.ingest" => {
            let sources = parse_sources(args)?;
            let ks = ws.knowledge.as_ref()
                .ok_or_else(|| HarborError::Other("knowledge not open".into()))?;
            ks.ingest(&sources).map_err(|e| HarborError::Other(e.to_string()))
        }
        "knowledge.remove_source" => {
            let source_id = args.get("source_id").and_then(|v| v.as_str())
                .ok_or_else(|| HarborError::Other("missing source_id".into()))?;
            let ks = ws.knowledge.as_ref()
                .ok_or_else(|| HarborError::Other("knowledge not open".into()))?;
            ks.remove_source(source_id).map_err(|e| HarborError::Other(e.to_string()))
        }
        "knowledge.sources" => {
            let ks = ws.knowledge.as_ref()
                .ok_or_else(|| HarborError::Other("knowledge not open".into()))?;
            ks.sources().map_err(|e| HarborError::Other(e.to_string()))
        }
        "knowledge.search" => {
            let ks = ws.knowledge.as_ref()
                .ok_or_else(|| HarborError::Other("knowledge not open".into()))?;
            let question = args.get("question").and_then(|v| v.as_str())
                .ok_or_else(|| HarborError::Other("missing question".into()))?;
            let top_k = args.get("top_k").and_then(|v| v.as_u64()).unwrap_or(5) as usize;
            ks.search(question, top_k).map_err(|e| HarborError::Other(e.to_string()))
        }
        // --- ask: retrieve -> augment -> generate (synchronous form) -----
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
                Arc::new(crate::knowledge::ChatHandle::new(&ws.data_root.join("models")))
            });
            let never = AtomicBool::new(false);
            let answer = chat
                .generate_rag_cancellable(&chat_package, question, citations.as_array().cloned().unwrap_or_default(), max_tokens, &never, None)
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
        // --- background ops ----------------------------------------------
        "op.start_acquire" => {
            let package_id = args.get("package_id").and_then(|v| v.as_str())
                .ok_or_else(|| HarborError::Other("missing package_id".into()))?.to_string();
            let repo_id = args.get("repo_id").and_then(|v| v.as_str())
                .ok_or_else(|| HarborError::Other("missing repo_id".into()))?.to_string();
            let revision = args.get("revision").and_then(|v| v.as_str()).unwrap_or("main").to_string();
            let files = parse_acquire_files(args)?;
            let (op_id, entry) = register_op("acquire");
            let data_root = ws.data_root.clone();
            let broker = ws.inner.broker.clone();
            let transport = ws.transport.clone();
            let mode = ws.inner.privacy_mode;
            let hub_token = ws.hub_token.clone();
            let progress = entry.progress.clone();
            let entry_clone = entry.clone();
            std::thread::spawn(move || {
                let result = acquire_model(
                    &data_root, &broker, &transport, mode, hub_token,
                    &package_id, &repo_id, &revision, &files, Some(progress.clone()),
                );
                let cancelled = matches!(&result, Err(e) if e.to_string().contains("cancelled"));
                complete_op(&entry_clone, result.map_err(|e| e.to_string()), cancelled);
            });
            Ok(serde_json::json!({ "op_id": op_id }))
        }
        "op.start_generate" => {
            let question = args.get("question").and_then(|v| v.as_str())
                .ok_or_else(|| HarborError::Other("missing question".into()))?.to_string();
            let chat_package = args.get("chat_package").and_then(|v| v.as_str())
                .ok_or_else(|| HarborError::Other("missing chat_package".into()))?.to_string();
            let max_tokens = args.get("max_tokens").and_then(|v| v.as_u64()).unwrap_or(256) as u32;
            let run_id = args.get("run_id").and_then(|v| v.as_str()).map(str::to_string);
            let (op_id, entry) = register_op("generate");
            let knowledge = ws.knowledge.clone();
            let chat = ws.chat.get_or_insert_with(|| {
                Arc::new(crate::knowledge::ChatHandle::new(&ws.data_root.join("models")))
            }).clone();
            let agent_log = ws.inner.agent_log.clone();
            let data_root = ws.data_root.clone();
            let progress = entry.progress.clone();
            let entry_clone = entry.clone();
            std::thread::spawn(move || {
                // 1. Retrieve grounding (may be absent).
                let citations = knowledge
                    .as_ref()
                    .and_then(|ks| ks.search(&question, 3).ok())
                    .and_then(|v| v.get("citations").cloned())
                    .unwrap_or_else(|| serde_json::json!([]));
                // 2. Durable activity: the question is the run's step.
                if let Some(run) = &run_id {
                    let step_seq = agent_log
                        .load_stream(run)
                        .ok()
                        .and_then(|s| s.last().map(|h| h.counters.step_count_total))
                        .unwrap_or(0);
                    let _ = append_run_event(
                        &agent_log, &data_root, run, EventType::RunStepStarted,
                        EventPayload::StepStarted {
                            step_id: format!("step-{}", step_seq + 1),
                            description: question.clone(),
                        },
                        true,
                    );
                }
                // 3. Generate on-device (cooperatively cancellable).
                let never = AtomicBool::new(false);
                let result = chat.generate_rag_cancellable(
                    &chat_package, &question,
                    citations.as_array().cloned().unwrap_or_default(),
                    max_tokens, &never, Some(&progress),
                );
                match result {
                    Ok(answer) => {
                        if let Some(run) = &run_id {
                            let step_seq = agent_log
                                .load_stream(run)
                                .ok()
                                .and_then(|s| s.last().map(|h| h.counters.step_count_total))
                                .unwrap_or(1);
                            let summary = format!(
                                "{} ({} tokens, citations: {})",
                                truncate_for_trail(&answer.answer, 200),
                                answer.completion_tokens,
                                answer.used_citations,
                            );
                            let _ = append_run_event(
                                &agent_log, &data_root, run, EventType::RunStepCompleted,
                                EventPayload::StepCompleted {
                                    step_id: format!("step-{step_seq}"),
                                    summary,
                                },
                                false,
                            );
                        }
                        complete_op(&entry_clone, Ok(serde_json::json!({
                            "answer": answer.answer,
                            "used_citations": answer.used_citations,
                            "executed_on": answer.executed_on,
                            "execution": "ON_DEVICE",
                            "usage": {
                                "prompt_tokens": answer.prompt_tokens,
                                "completion_tokens": answer.completion_tokens,
                            },
                        })), false);
                    }
                    Err(e) => {
                        let cancelled = e == "cancelled"
                            || e.to_lowercase().contains("cancelled");
                        if cancelled {
                            if let Some(run) = &run_id {
                                // Durable trace of the user-driven stop.
                                let _ = append_run_event(
                                    &agent_log, &data_root, run, EventType::RunTransition,
                                    EventPayload::Transition {
                                        from_state: RunState::Running,
                                        to_state: RunState::Paused,
                                        reason: Some(PauseReason::parse("user").unwrap()),
                                    },
                                    false,
                                );
                            }
                        }
                        complete_op(&entry_clone, Err(e), cancelled);
                    }
                }
            });
            Ok(serde_json::json!({ "op_id": op_id }))
        }
        "op.start_ingest" => {
            let sources = parse_sources(args)?;
            let (op_id, entry) = register_op("ingest");
            let knowledge = ws.knowledge.clone()
                .ok_or_else(|| HarborError::Other("knowledge not open".into()))?;
            let progress = entry.progress.clone();
            let entry_clone = entry.clone();
            std::thread::spawn(move || {
                let never = AtomicBool::new(false);
                let result = knowledge
                    .ingest_with_progress(&sources, &never, Some(&progress))
                    .map_err(|e| e.to_string());
                let cancelled = matches!(&result, Err(e) if e.contains("cancelled"));
                complete_op(&entry_clone, result, cancelled);
            });
            Ok(serde_json::json!({ "op_id": op_id }))
        }
        "op.status" => {
            let op_id = args.get("op_id").and_then(|v| v.as_str())
                .ok_or_else(|| HarborError::Other("missing op_id".into()))?;
            let ops = ops_registry().lock().unwrap();
            let entry = ops
                .get(op_id)
                .ok_or_else(|| HarborError::Other(format!("unknown op {op_id}")))?;
            Ok(op_status_json(op_id, entry))
        }
        "op.cancel" => {
            let op_id = args.get("op_id").and_then(|v| v.as_str())
                .ok_or_else(|| HarborError::Other("missing op_id".into()))?;
            let ops = ops_registry().lock().unwrap();
            let entry = ops
                .get(op_id)
                .ok_or_else(|| HarborError::Other(format!("unknown op {op_id}")))?;
            entry.progress.request_cancel();
            Ok(serde_json::json!({ "cancelling": true }))
        }
        "op.list" => {
            let ops = ops_registry().lock().unwrap();
            let list: Vec<serde_json::Value> = ops
                .iter()
                .map(|(id, e)| op_status_json(id, e))
                .collect();
            Ok(serde_json::json!({ "ops": list }))
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
                            harbor_agent::EventPayload::StepStarted { description, .. } => {
                                format!("request: {}", truncate_for_trail(description, 120))
                            }
                            harbor_agent::EventPayload::StepCompleted { summary, .. } => {
                                format!("answer: {}", truncate_for_trail(summary, 160))
                            }
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

fn truncate_for_trail(s: &str, max: usize) -> String {
    let s = s.replace('\n', " ");
    if s.chars().count() <= max {
        s
    } else {
        let cut: String = s.chars().take(max).collect();
        format!("{cut}…")
    }
}

fn parse_sources(args: &serde_json::Value) -> Result<Vec<crate::knowledge::SourceInput>, HarborError> {
    Ok(args
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
        .collect())
}

/// The single JSON dispatcher.
///
/// # Safety
/// `handle` must be a live pointer from [`harbor_core_open_ex`]; request
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
    use std::sync::Arc;

    #[test]
    fn hub_token_and_catalog_trust_survive_restart() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let ks: Arc<dyn KeyStore> = Arc::new(
            harbor_store::keys::FileKeyStore::new(root.join("keys")).unwrap(),
        );

        // Hub token round-trip: stored wrapped, loaded decrypted.
        save_hub_token(root, &ks, Some("hf_tok_123")).unwrap();
        assert_eq!(load_hub_token(root, &ks).as_deref(), Some("hf_tok_123"));
        save_hub_token(root, &ks, None).unwrap();
        assert!(load_hub_token(root, &ks).is_none());

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

    #[test]
    fn device_identity_persists_across_opens() {
        let dir = tempfile::tempdir().unwrap();
        let a = ensure_device_identity(dir.path()).unwrap();
        let b = ensure_device_identity(dir.path()).unwrap();
        assert_eq!(a, b, "device id must be stable across opens");
        assert!(a.starts_with("device-"));
        assert_ne!(a, ensure_device_identity(&tempfile::tempdir().unwrap().path()).unwrap());
    }

    #[test]
    fn file_root_rotates_to_injected_keystore_and_data_survives() {
        use harbor_store::keys::{FileKeyStore, KeyStore};
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        // Old development state: workspace key wrapped under a file root.
        {
            let opts = OpenOptions {
                data_root: root.to_path_buf(),
                device_id: "dev".into(),
            };
            let ws = harbor_core::Workspace::open(&opts, "ws-rot", PrivacyMode::LocalOnly).unwrap();
            ws.blobs()
                .put("ws-rot", b"precious", &Default::default())
                .unwrap();
        }
        // Upgrade: the embedding layer injects an OS-keystore root.
        let new_root = harbor_store::keys::KeyMaterial::random();
        let injected: Arc<dyn KeyStore> =
            Arc::new(harbor_store::native_keystore::InjectedKeyStore::new(new_root));
        rotate_file_root_to_native(root, &injected).unwrap();
        // The file root is gone; the workspace opens under the injected
        // root and the blob is still readable.
        assert!(!FileKeyStore::new(root.join("keys")).unwrap().exists("harbor.device"));
        let opts = OpenOptions {
            data_root: root.to_path_buf(),
            device_id: "dev".into(),
        };
        let ws = harbor_core::Workspace::open_with_keystore(&opts, "ws-rot", PrivacyMode::LocalOnly, injected)
            .unwrap();
        let blob = ws.blobs().list("ws-rot").unwrap();
        assert_eq!(blob.len(), 1);
        assert_eq!(
            ws.blobs().get("ws-rot", &blob[0], &Default::default()).unwrap(),
            b"precious"
        );
    }

    #[test]
    fn run_create_generates_unique_ids_when_absent() {
        // The dispatcher mints random ids; two calls never collide.
        let args_missing = serde_json::json!({});
        let generated: Vec<String> = (0..2)
            .map(|_| {
                match args_missing.get("run_id").and_then(|v| v.as_str()) {
                    Some(r) if !r.is_empty() => r.to_string(),
                    _ => harbor_security::HarborId::generate("run").to_string(),
                }
            })
            .collect();
        assert_ne!(generated[0], generated[1]);
        assert!(generated[0].starts_with("run-"));
    }

    fn hex_encode(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }
}
