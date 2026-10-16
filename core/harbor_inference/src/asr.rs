//! Speech-recognition host bridge (decision 0019). The OS owns the model
//! (Apple's on-device speech model); Harbor owns policy. Mirrors
//! `native/apple/include/harbor_asr_host.h`. On-device only: a host that
//! reports any other execution location is refused.

use std::ffi::{c_char, c_int, CStr, CString};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, OnceLock};

use serde_json::Value;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct AsrHostVtable {
    pub descriptor: unsafe extern "C" fn() -> *mut c_char,
    #[allow(clippy::type_complexity)]
    pub transcribe: unsafe extern "C" fn(
        request_json: *const c_char,
        data: *const u8,
        len: usize,
        cancel: *const AtomicBool,
        out: *mut *mut c_char,
    ) -> c_int,
    pub free_string: unsafe extern "C" fn(s: *mut c_char),
}

#[derive(Debug, thiserror::Error)]
pub enum AsrError {
    #[error("no speech engine is registered on this device")]
    Unavailable,
    #[error("speech host refused: {0}")]
    Refused(String),
    #[error("{0}")]
    Unsupported(String),
    #[error("transcription failed: {0}")]
    Failed(String),
    #[error("transcription cancelled")]
    Cancelled,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AsrSegment {
    pub start: f64,
    pub end: f64,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct AsrOutput {
    pub engine: String,
    pub locale: String,
    pub text: String,
    pub segments: Vec<AsrSegment>,
}

pub trait AsrEngine: Send + Sync {
    fn name(&self) -> String;
    /// Locales the OS model can recognise (installed or installable).
    fn supported_locales(&self) -> Vec<String>;
    /// Locales usable right now, without any download.
    fn installed_locales(&self) -> Vec<String>;
    fn transcribe(
        &self,
        audio_file: &[u8],
        locale: &str,
        cancel: &AtomicBool,
    ) -> Result<AsrOutput, AsrError>;
}

pub struct AsrHost {
    vtable: AsrHostVtable,
    engine: String,
    supported: Vec<String>,
    installed: Vec<String>,
}

// SAFETY: the host contract requires callbacks be callable from any thread.
unsafe impl Send for AsrHost {}
unsafe impl Sync for AsrHost {}

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

impl AsrHost {
    pub fn from_vtable(vtable: AsrHostVtable) -> Result<Self, AsrError> {
        let raw = unsafe { (vtable.descriptor)() };
        if raw.is_null() {
            return Err(AsrError::Refused("host returned no descriptor".into()));
        }
        let text = unsafe { CStr::from_ptr(raw) }
            .to_string_lossy()
            .into_owned();
        unsafe { (vtable.free_string)(raw) };
        let d: Value = serde_json::from_str(&text)
            .map_err(|e| AsrError::Refused(format!("descriptor: {e}")))?;
        if d["schema"] != "harbor.asr_host/v1" {
            return Err(AsrError::Refused(
                "descriptor schema must be harbor.asr_host/v1".into(),
            ));
        }
        if d["execution_location"] != "on_device" {
            return Err(AsrError::Refused(
                "a speech host must execute on-device; remote speech recognition is not a host"
                    .into(),
            ));
        }
        Ok(AsrHost {
            vtable,
            engine: d["engine"].as_str().unwrap_or("unknown").to_string(),
            supported: strings(&d["supported_locales"]),
            installed: strings(&d["installed_locales"]),
        })
    }
}

impl AsrEngine for AsrHost {
    fn name(&self) -> String {
        self.engine.clone()
    }
    fn supported_locales(&self) -> Vec<String> {
        self.supported.clone()
    }
    fn installed_locales(&self) -> Vec<String> {
        self.installed.clone()
    }
    fn transcribe(
        &self,
        audio_file: &[u8],
        locale: &str,
        cancel: &AtomicBool,
    ) -> Result<AsrOutput, AsrError> {
        let req = CString::new(serde_json::json!({ "locale": locale }).to_string())
            .map_err(|e| AsrError::Failed(e.to_string()))?;
        let mut out: *mut c_char = std::ptr::null_mut();
        let code = unsafe {
            (self.vtable.transcribe)(
                req.as_ptr(),
                audio_file.as_ptr(),
                audio_file.len(),
                cancel,
                &mut out,
            )
        };
        let text = if out.is_null() {
            String::new()
        } else {
            let t = unsafe { CStr::from_ptr(out) }
                .to_string_lossy()
                .into_owned();
            unsafe { (self.vtable.free_string)(out) };
            t
        };
        let v: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
        let msg = v["error"].as_str().unwrap_or("host failure").to_string();
        match code {
            0 => {}
            1 => return Err(AsrError::Unsupported(msg)),
            3 => return Err(AsrError::Cancelled),
            _ => return Err(AsrError::Failed(msg)),
        }
        Ok(AsrOutput {
            engine: v["engine"].as_str().unwrap_or(&self.engine).to_string(),
            locale: v["locale"].as_str().unwrap_or(locale).to_string(),
            text: v["text"].as_str().unwrap_or("").trim().to_string(),
            segments: v["segments"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .map(|s| AsrSegment {
                            start: s["start"].as_f64().unwrap_or(0.0),
                            end: s["end"].as_f64().unwrap_or(0.0),
                            text: s["text"].as_str().unwrap_or("").to_string(),
                        })
                        .collect()
                })
                .unwrap_or_default(),
        })
    }
}

static ENGINE: OnceLock<Mutex<Option<Arc<dyn AsrEngine>>>> = OnceLock::new();

fn slot() -> &'static Mutex<Option<Arc<dyn AsrEngine>>> {
    ENGINE.get_or_init(|| Mutex::new(None))
}

pub fn register_asr_engine(engine: Arc<dyn AsrEngine>) {
    *slot().lock().unwrap() = Some(engine);
}

/// Register a platform host through its C vtable (functions and their backing
/// library must stay loaded for the life of the process).
pub fn register_asr_host(vtable: AsrHostVtable) -> Result<(), AsrError> {
    register_asr_engine(Arc::new(AsrHost::from_vtable(vtable)?));
    Ok(())
}

pub fn clear_asr_engine() {
    *slot().lock().unwrap() = None;
}

pub fn asr_engine() -> Result<Arc<dyn AsrEngine>, AsrError> {
    slot().lock().unwrap().clone().ok_or(AsrError::Unavailable)
}
