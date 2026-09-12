//! Harbor FFI: the C ABI seam between Flutter (Dart) and Harbor Core.
//!
//! One dispatcher, JSON in / JSON out (`harbor_core_call`), so the Dart
//! side carries no business logic and the Rust side owns policy and
//! runtime truth (08_Repo_Structure dependency rules). Handles are opaque
//! pointers; strings returned by the boundary are freed with
//! `harbor_core_string_free`.

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
        Ok(WorkspaceHandle { inner: ws, data_root: opts.data_root })
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
