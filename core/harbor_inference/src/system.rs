//! System-model host bridge: the C-ABI contract a platform system-model
//! adapter implements (Apple Foundation Models, Android system model,
//! Windows Foundry) to serve generation into the Rust runtime.
//!
//! Authority: `03_Architecture_Contracts.md` §1,
//! `17_Model_Provider_and_Package_Contract.md` (SystemManaged references:
//! "provider and provider-model identity and cannot carry installed
//! files"), `25_Feature_Registry.json` `provider:apple_system` et al.
//!
//! Direction of trust: the host platform owns *availability and
//! execution* (it can only report models the OS provisioned); Rust owns
//! *policy truth*. The bridge therefore enforces, not trusts:
//! - a host that declares or reports any execution location other than
//!   on-device is rejected (the sanctioned remote path is
//!   `RemoteEndpoint` through the egress broker, never a system host);
//! - capability declarations are checked per request with typed errors;
//! - the executed model identity in every response is the host's own
//!   descriptor identity, surfaced for Trust Pulse.
//!
//! System providers stay M4 optional (`default_enabled: false`): this
//! bridge is inert until a host registers it, and core GA never depends
//! on one being present (a missing bridge is `ModelNotFound`, which the
//! router turns into an explicit, visible fallback to a qualified GGUF
//! package under `AllowSubstitution`, never a silent replacement).

use std::ffi::{c_char, CStr, CString};
use std::os::raw::c_int;
use std::sync::atomic::AtomicBool;
use std::sync::Mutex;

use harbor_canonical::JsonValue;

use crate::provider::{
    Capabilities, ChatRequest, ChatResponse, ModelProvider, ModelRef, ProviderError, Usage,
};

/// Host status codes for [`SystemHostVtable::generate`].
pub mod host_status {
    use std::ffi::c_int;

    /// Success; `out` holds a response JSON object.
    pub const OK: c_int = 0;
    /// The host does not support a requested capability.
    pub const UNSUPPORTED_CAPABILITY: c_int = 1;
    /// The system model is not available (unprovisioned, disabled, ...).
    pub const UNAVAILABLE: c_int = 2;
    /// The request was cancelled through the cancel flag.
    pub const CANCELLED: c_int = 3;
    /// The host failed to generate; `out` holds `{"error": "..."}`.
    pub const BACKEND: c_int = 4;
    /// The host detected a policy violation on its side.
    pub const POLICY: c_int = 5;
}

/// The C-ABI a native system-model adapter exposes. Mirrored by
/// `native/*/include/harbor_system_host.h`; the two definitions must
/// stay byte-compatible (a bindgen dependency is not warranted for
/// three pointers).
///
/// # Safety contract
/// - The caller keeps `host` and the function pointers alive for the
///   lifetime of every [`SystemHostBridge`] built over the vtable.
/// - Strings returned by the host are host-allocated and MUST be freed
///   only by [`SystemHostVtable::free_string`]; Rust never frees them.
/// - `cancel` points at a Rust `AtomicBool` (`#[repr(transparent)]`
///   over `bool`), readable as a C `const bool *`; the host should poll
///   it and abort generation when it becomes true.
#[repr(C)]
pub struct SystemHostVtable {
    /// Returns a host-allocated descriptor JSON string (see
    /// [`SystemHostBridge::from_vtable`] for the required shape).
    pub descriptor: unsafe extern "C" fn() -> *mut c_char,
    /// Synchronous generation. `request_json` is canonical
    /// `harbor.system_request/v1` JSON; `out` receives the response (or
    /// `{"error": "..."}`) as a host-allocated string.
    #[allow(clippy::type_complexity)]
    pub generate: unsafe extern "C" fn(
        request_json: *const c_char,
        cancel: *const AtomicBool,
        out: *mut *mut c_char,
    ) -> c_int,
    /// Frees a string this host allocated.
    pub free_string: unsafe extern "C" fn(s: *mut c_char),
}

/// A [`ModelProvider`] over a registered system-host vtable.
pub struct SystemHostBridge {
    vtable: SystemHostVtable,
    provider_id: String,
    model_id: String,
    available: bool,
    unavailable_reason: Option<String>,
    /// Leaked once per registration: the trait returns `&'static`
    /// declarations, and registration is process-lifetime by contract.
    capabilities: &'static [Capabilities],
    /// The descriptor's `identity` object (OS, arch, runtime revision),
    /// surfaced with diagnostics and evidence.
    identity: JsonValue,
    /// The last response's host metadata (guided output, degradations),
    /// for diagnostics; never affects policy.
    last_host_metadata: Mutex<Option<JsonValue>>,
}

// SAFETY: the host context pointer is opaque to Rust and every host
// contract (header + decision 0009) requires the callbacks be callable
// from any thread; nothing else in the bridge is shared mutably except
// the mutex-guarded metadata.
unsafe impl Send for SystemHostBridge {}
unsafe impl Sync for SystemHostBridge {}

fn capability_from_wire(s: &str) -> Option<Capabilities> {
    match s {
        "chat" => Some(Capabilities::Chat),
        "tools" => Some(Capabilities::Tools),
        "structured_output" => Some(Capabilities::StructuredOutput),
        "embeddings" => Some(Capabilities::Embeddings),
        "vision" => Some(Capabilities::Vision),
        "audio" => Some(Capabilities::Audio),
        _ => None,
    }
}

impl SystemHostBridge {
    /// Probe the host's descriptor and build the bridge.
    ///
    /// Refuses, as typed errors, any host that does not speak the
    /// declared schema or that claims an execution location other than
    /// on-device: a system host is by definition OS-provisioned local
    /// execution (17 §"SystemManaged"), and the sanctioned remote path
    /// is `RemoteEndpoint` behind the egress broker.
    pub fn from_vtable(vtable: SystemHostVtable) -> Result<Self, ProviderError> {
        let raw = unsafe { (vtable.descriptor)() };
        if raw.is_null() {
            return Err(ProviderError::Backend(
                "system host returned no descriptor".into(),
            ));
        }
        let text = unsafe { CStr::from_ptr(raw) }.to_string_lossy().into_owned();
        unsafe { (vtable.free_string)(raw) };
        let d = harbor_canonical::parse(&text)
            .map_err(|e| ProviderError::Backend(format!("descriptor is not canonical JSON: {e}")))?;
        if d.get("schema").and_then(|v| v.as_str()) != Some("harbor.system_host/v1") {
            return Err(ProviderError::Backend(
                "descriptor schema must be harbor.system_host/v1".into(),
            ));
        }
        let provider_id = d
            .get("provider_id")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| ProviderError::Backend("descriptor missing provider_id".into()))?
            .to_string();
        let model_id = d
            .get("model_id")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| ProviderError::Backend("descriptor missing model_id".into()))?
            .to_string();
        match d.get("execution_location").and_then(|v| v.as_str()) {
            Some("on_device") => {}
            other => {
                return Err(ProviderError::Policy(format!(
                    "a system host must execute on-device; descriptor says {other:?}"
                )))
            }
        }
        let mut caps = Vec::new();
        for c in d.get("capabilities").and_then(|v| v.as_array()).unwrap_or(&[]) {
            let Some(c) = c.as_str().and_then(capability_from_wire) else {
                return Err(ProviderError::Backend(format!(
                    "descriptor has unknown capability {:?}",
                    c.as_str()
                )));
            };
            caps.push(c);
        }
        let available = d.get("available").and_then(|v| v.as_bool()).unwrap_or(false);
        let unavailable_reason = d
            .get("unavailable_reason")
            .filter(|v| !v.is_null())
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let identity = d
            .get("identity")
            .cloned()
            .unwrap_or_else(|| JsonValue::object(std::iter::empty::<(String, JsonValue)>()));
        Ok(SystemHostBridge {
            vtable,
            provider_id,
            model_id,
            available,
            unavailable_reason,
            capabilities: Box::leak(caps.into_boxed_slice()),
            identity,
            last_host_metadata: Mutex::new(None),
        })
    }

    /// The descriptor identity block (OS, arch, runtime revision) for
    /// evidence and Trust Pulse surfaces.
    pub fn identity(&self) -> &JsonValue {
        &self.identity
    }

    /// Why the host is unavailable, when it is.
    pub fn unavailable_reason(&self) -> Option<&str> {
        self.unavailable_reason.as_deref()
    }

    /// The system provider/model identity this bridge serves, in the
    /// `executed_on` form (`provider_id/model_id`).
    pub fn provider_model(&self) -> String {
        format!("{}/{}", self.provider_id, self.model_id)
    }

    /// Host metadata from the last response (`guided`, `degradations`,
    /// ...). Diagnostics only; policy never reads it.
    pub fn last_host_metadata(&self) -> Option<JsonValue> {
        self.last_host_metadata.lock().unwrap().clone()
    }

    fn model_ref(&self) -> ModelRef {
        ModelRef::SystemManaged {
            provider_id: self.provider_id.clone(),
            model_id: self.model_id.clone(),
        }
    }

    /// Serialize the request in the `harbor.system_request/v1` wire form.
    /// Canonical JSON prohibits floats, so temperature crosses as
    /// thousandths (`temperature_milli`: 200 == 0.2).
    fn request_wire(&self, req: &ChatRequest) -> Result<CString, ProviderError> {
        let int = |v: i64, what: &str| {
            JsonValue::int(v).map_err(|e| ProviderError::Backend(format!("{what}: {e}")))
        };
        let mut pairs: Vec<(String, JsonValue)> = vec![
            (
                "schema".into(),
                JsonValue::str("harbor.system_request/v1"),
            ),
            (
                "model".into(),
                JsonValue::object([
                    ("provider_id", JsonValue::str(&self.provider_id)),
                    ("model_id", JsonValue::str(&self.model_id)),
                ]),
            ),
            ("messages".into(), JsonValue::Array(req.messages.clone())),
            ("max_tokens".into(), int(req.max_tokens as i64, "max_tokens")?),
            (
                "temperature_milli".into(),
                int((req.temperature * 1000.0).round() as i64, "temperature")?,
            ),
        ];
        pairs.push((
            "requires".into(),
            JsonValue::Array(
                req.requires
                    .iter()
                    .map(|c| JsonValue::str(c.as_str()))
                    .collect(),
            ),
        ));
        if let Some(schema) = &req.response_schema {
            pairs.push(("response_schema".into(), schema.clone()));
        }
        if let Some(k) = &req.trace_key {
            pairs.push(("trace_key".into(), JsonValue::str(k)));
        }
        let wire = JsonValue::object(pairs);
        let bytes = wire
            .to_canonical_bytes()
            .map_err(|e| ProviderError::Backend(format!("request canonicalization: {e}")))?;
        CString::new(bytes)
            .map_err(|e| ProviderError::Backend(format!("request contains NUL: {e}")))
    }

    /// Generate with cooperative cancellation: the cancel flag is shared
    /// with the host, which polls it and aborts its generation task.
    pub fn generate_cancellable(
        &self,
        req: ChatRequest,
        cancel: &AtomicBool,
    ) -> Result<ChatResponse, ProviderError> {
        let model = self.model_ref();
        if req.model != model {
            return Err(ProviderError::ModelNotFound(format!(
                "bridge serves {}/{}; request names {:?}",
                self.provider_id, self.model_id, req.model
            )));
        }
        for need in &req.requires {
            if !self.capabilities.contains(need) {
                return Err(ProviderError::UnsupportedCapability(need.as_str()));
            }
        }
        if !self.available {
            return Err(ProviderError::ModelNotFound(format!(
                "system model unavailable: {}",
                self.unavailable_reason.as_deref().unwrap_or("unknown reason")
            )));
        }
        let wire = self.request_wire(&req)?;
        let mut out: *mut c_char = std::ptr::null_mut();
        let status = unsafe { (self.vtable.generate)(wire.as_ptr(), cancel, &mut out) };
        let response_text = if out.is_null() {
            None
        } else {
            let t = unsafe { CStr::from_ptr(out) }.to_string_lossy().into_owned();
            unsafe { (self.vtable.free_string)(out) };
            Some(t)
        };
        let text = response_text.unwrap_or_else(|| "{}".into());
        let body = harbor_canonical::parse(&text).map_err(|e| {
            ProviderError::Backend(format!("host response is not canonical JSON: {e}"))
        })?;
        match status {
            host_status::OK => {}
            host_status::UNSUPPORTED_CAPABILITY => {
                // The capability gate already ran Rust-side against the
                // declared capabilities; a host refusing anyway is a
                // contract violation on the host, reported as a backend
                // failure with its message.
                return Err(ProviderError::Backend(format!(
                    "host declined a capability it declared: {}",
                    body.get("error").and_then(|v| v.as_str()).unwrap_or("")
                )));
            }
            host_status::UNAVAILABLE => {
                return Err(ProviderError::ModelNotFound(
                    body.get("error")
                        .and_then(|v| v.as_str())
                        .unwrap_or("system model unavailable")
                        .to_string(),
                ))
            }
            host_status::CANCELLED => return Err(ProviderError::Cancelled),
            host_status::BACKEND | host_status::POLICY => {
                let kind = if status == host_status::POLICY { "policy" } else { "backend" };
                return Err(ProviderError::Backend(format!(
                    "system host {kind}: {}",
                    body.get("error").and_then(|v| v.as_str()).unwrap_or("")
                )));
            }
            other => {
                return Err(ProviderError::Backend(format!(
                    "system host returned unknown status {other}"
                )))
            }
        }
        // Rust owns policy truth: validate the executed location rather
        // than trusting the host's word for anything but identity.
        match body.get("execution_location").and_then(|v| v.as_str()) {
            Some("on_device") => {}
            other => {
                return Err(ProviderError::Policy(format!(
                    "system host reported execution location {other:?}; a LocalOnly run \
                     may never execute off-device"
                )))
            }
        }
        *self.last_host_metadata.lock().unwrap() = body
            .get("host_metadata")
            .cloned();
        let content = body
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ProviderError::Backend("host response missing content".into()))?
            .to_string();
        let num = |k: &str| body.get(k).and_then(|v| v.as_int()).unwrap_or(0).max(0) as u64;
        Ok(ChatResponse {
            content,
            usage: Usage {
                prompt_tokens: num("prompt_tokens"),
                completion_tokens: num("completion_tokens"),
            },
            executed_on: body
                .get("executed_on")
                .and_then(|v| v.as_str())
                .unwrap_or(&format!("{}/{}", self.provider_id, self.model_id))
                .to_string(),
            execution_location: harbor_security::policy::ExecutionLocation::OnDevice,
        })
    }
}

impl ModelProvider for SystemHostBridge {
    fn id(&self) -> &str {
        &self.provider_id
    }

    fn capabilities(&self) -> &'static [Capabilities] {
        self.capabilities
    }

    fn supports(&self, model: &ModelRef, need: &Capabilities) -> bool {
        model == &self.model_ref() && self.available && self.capabilities.contains(need)
    }

    fn load(&self, model: &ModelRef) -> Result<(), ProviderError> {
        if model != &self.model_ref() {
            return Err(ProviderError::ModelNotFound(format!(
                "bridge serves {}/{}",
                self.provider_id, self.model_id
            )));
        }
        if !self.available {
            return Err(ProviderError::ModelNotFound(format!(
                "system model unavailable: {}",
                self.unavailable_reason.as_deref().unwrap_or("unknown reason")
            )));
        }
        // The OS owns the model lifecycle; there is nothing to preload.
        Ok(())
    }

    fn unload(&self, _model: &ModelRef) -> Result<(), ProviderError> {
        Ok(())
    }

    fn generate(&self, req: ChatRequest) -> Result<ChatResponse, ProviderError> {
        let never = AtomicBool::new(false);
        self.generate_cancellable(req, &never)
    }

    fn execution_location(&self) -> harbor_security::policy::ExecutionLocation {
        harbor_security::policy::ExecutionLocation::OnDevice
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

    /// Test-double host state: records the last request wire JSON and
    /// scripts the next response.
    struct DoubleState {
        last_request: Option<String>,
        response_content: String,
        response_location: &'static str,
        respect_cancel: bool,
    }
    static DOUBLE: Mutex<DoubleState> = Mutex::new(DoubleState {
        last_request: None,
        response_content: String::new(),
        response_location: "on_device",
        respect_cancel: false,
    });
    /// Serializes tests that script DOUBLE state (they run in parallel
    /// otherwise and would read each other's scenarios).
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    fn reset_double(content: &str) -> () {
        let mut d = DOUBLE.lock().unwrap();
        d.response_content = content.into();
        d.response_location = "on_device";
        d.respect_cancel = false;
    }

    fn double_descriptor(
        available: bool,
        capabilities: &[&str],
    ) -> String {
        serde_json::json!({
            "schema": "harbor.system_host/v1",
            "provider_id": "double-system",
            "model_id": "double-model",
            "available": available,
            "unavailable_reason": if available { None } else { Some("modelNotReady") },
            "capabilities": capabilities,
            "execution_location": "on_device",
            "identity": {"os": "test", "runtime": "double/1"},
        })
        .to_string()
    }

    unsafe extern "C" fn double_descriptor_cb() -> *mut c_char {
        // An available chat + structured-output host.
        CString::new(double_descriptor(true, &["chat", "structured_output"]))
            .unwrap()
            .into_raw()
    }

    unsafe extern "C" fn unavailable_descriptor_cb() -> *mut c_char {
        CString::new(double_descriptor(false, &["chat", "structured_output"]))
            .unwrap()
            .into_raw()
    }

    unsafe extern "C" fn double_generate_cb(
        request_json: *const c_char,
        cancel: *const AtomicBool,
        out: *mut *mut c_char,
    ) -> c_int {
        let req = CStr::from_ptr(request_json).to_string_lossy().into_owned();
        DOUBLE.lock().unwrap().last_request = Some(req);
        let d = DOUBLE.lock().unwrap();
        if d.respect_cancel && (*cancel).load(Ordering::Relaxed) {
            *out = CString::new(r#"{"error":"cancelled"}"#).unwrap().into_raw();
            return host_status::CANCELLED;
        }
        let body = serde_json::json!({
            "content": d.response_content,
            "prompt_tokens": 12,
            "completion_tokens": 34,
            "executed_on": "double-system/double-model",
            "execution_location": d.response_location,
            "host_metadata": {"guided": true},
        });
        *out = CString::new(body.to_string()).unwrap().into_raw();
        host_status::OK
    }

    unsafe extern "C" fn double_free_cb(s: *mut c_char) {
        if !s.is_null() {
            drop(CString::from_raw(s));
        }
    }

    fn double_vtable() -> SystemHostVtable {
        SystemHostVtable {
            descriptor: double_descriptor_cb,
            generate: double_generate_cb,
            free_string: double_free_cb,
        }
    }

    fn request(needs: Vec<Capabilities>, schema: Option<JsonValue>) -> ChatRequest {
        ChatRequest {
            model: ModelRef::SystemManaged {
                provider_id: "double-system".into(),
                model_id: "double-model".into(),
            },
            messages: vec![JsonValue::object([
                ("role", JsonValue::str("system")),
                ("content", JsonValue::str("You are careful.")),
            ])],
            max_tokens: 64,
            temperature: 0.0,
            requires: needs,
            response_schema: schema,
            trace_key: Some("g/n#0".into()),
        }
    }

    #[test]
    fn round_trips_request_and_response_with_identity() {
        let _guard = TEST_LOCK.lock().unwrap();
        reset_double(r#"{"ok": true}"#);
        let bridge = SystemHostBridge::from_vtable(double_vtable()).unwrap();
        let resp = bridge
            .generate(request(vec![Capabilities::Chat, Capabilities::StructuredOutput], None))
            .unwrap();
        assert_eq!(resp.content, "{\"ok\": true}");
        assert_eq!(resp.executed_on, "double-system/double-model");
        assert_eq!(
            resp.execution_location,
            harbor_security::policy::ExecutionLocation::OnDevice
        );
        assert_eq!(resp.usage.prompt_tokens, 12);
        assert_eq!(resp.usage.completion_tokens, 34);
        // The host metadata is surfaced for diagnostics.
        assert_eq!(
            bridge.last_host_metadata().unwrap().get("guided").and_then(|v| v.as_bool()),
            Some(true)
        );
        // The wire request carried the canonical schema and trace key.
        let sent = DOUBLE.lock().unwrap().last_request.clone().unwrap();
        let parsed = harbor_canonical::parse(&sent).unwrap();
        assert_eq!(
            parsed.get("schema").and_then(|v| v.as_str()),
            Some("harbor.system_request/v1")
        );
        assert_eq!(parsed.get("trace_key").and_then(|v| v.as_str()), Some("g/n#0"));
    }

    #[test]
    fn host_claiming_off_device_execution_is_a_policy_error() {
        let _guard = TEST_LOCK.lock().unwrap();
        reset_double("x");
        let bridge = SystemHostBridge::from_vtable(double_vtable()).unwrap();
        DOUBLE.lock().unwrap().response_location = "remote";
        let err = bridge.generate(request(vec![Capabilities::Chat], None)).unwrap_err();
        assert!(matches!(err, ProviderError::Policy(_)), "{err}");
    }

    #[test]
    fn unrequested_capabilities_are_typed_errors() {
        let bridge = SystemHostBridge::from_vtable(double_vtable()).unwrap();
        let err = bridge
            .generate(request(vec![Capabilities::Vision], None))
            .unwrap_err();
        assert!(
            matches!(err, ProviderError::UnsupportedCapability("vision")),
            "{err}"
        );
    }

    #[test]
    fn cancellation_crosses_the_boundary() {
        let _guard = TEST_LOCK.lock().unwrap();
        reset_double("x");
        let bridge = SystemHostBridge::from_vtable(double_vtable()).unwrap();
        DOUBLE.lock().unwrap().respect_cancel = true;
        let cancel = AtomicBool::new(true);
        let err = bridge.generate_cancellable(request(vec![Capabilities::Chat], None), &cancel);
        assert!(matches!(err, Err(ProviderError::Cancelled)));
    }

    #[test]
    fn unavailable_host_is_model_not_found_with_its_reason() {
        let vt = SystemHostVtable {
            descriptor: unavailable_descriptor_cb,
            generate: double_generate_cb,
            free_string: double_free_cb,
        };
        let bridge = SystemHostBridge::from_vtable(vt).unwrap();
        assert_eq!(bridge.unavailable_reason(), Some("modelNotReady"));
        let err = bridge.generate(request(vec![Capabilities::Chat], None)).unwrap_err();
        assert!(
            matches!(&err, ProviderError::ModelNotFound(m) if m.contains("modelNotReady")),
            "{err}"
        );
        assert!(!bridge.supports(
            &ModelRef::SystemManaged {
                provider_id: "double-system".into(),
                model_id: "double-model".into()
            },
            &Capabilities::Chat
        ));
    }

    #[test]
    fn descriptor_must_declare_on_device_execution() {        // A vtable whose descriptor claims remote execution is refused
        // at registration, not discovered mid-run.
        unsafe extern "C" fn remote_descriptor() -> *mut c_char {
            CString::new(
                serde_json::json!({
                    "schema": "harbor.system_host/v1",
                    "provider_id": "remote-pretender",
                    "model_id": "x",
                    "available": true,
                    "capabilities": ["chat"],
                    "execution_location": "remote",
                })
                .to_string(),
            )
            .unwrap()
            .into_raw()
        }
        let vt = SystemHostVtable {
            descriptor: remote_descriptor,
            generate: double_generate_cb,
            free_string: double_free_cb,
        };
        assert!(matches!(
            SystemHostBridge::from_vtable(vt),
            Err(ProviderError::Policy(_))
        ));
    }

    #[test]
    fn router_routes_system_managed_refs_and_never_replaces_them() {
        use crate::router::{Router, RouterPolicy};
        let _guard = TEST_LOCK.lock().unwrap();
        reset_double("hi");
        let bridge = SystemHostBridge::from_vtable(double_vtable()).unwrap();
        let mut r = Router::new(RouterPolicy::AllowSubstitution);
        r.register(Box::new(bridge));
        // Exact system-managed ref routes.
        let (resp, sub) = r.chat(request(vec![Capabilities::Chat], None)).unwrap();
        assert!(sub.is_none());
        assert_eq!(resp.executed_on, "double-system/double-model");
        // A different system model is never silently replaced.
        let mut other = request(vec![Capabilities::Chat], None);
        other.model = ModelRef::SystemManaged {
            provider_id: "apple-system".into(),
            model_id: "foundation-model".into(),
        };
        assert!(matches!(r.chat(other), Err(ProviderError::ModelNotFound(_))));
    }
}
