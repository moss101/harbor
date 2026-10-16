//! OCR host bridge (decision 0018). The platform owns the engine (Apple
//! Vision today); Harbor owns policy. Mirrors `native/apple/include/
//! harbor_ocr_host.h`. Execution must be on-device: a host that reports any
//! other location is refused, like the system-model host.

use std::ffi::{c_char, c_int, CStr, CString};
use std::sync::atomic::AtomicBool;
use std::sync::{Mutex, OnceLock};

use serde_json::Value;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct OcrHostVtable {
    pub descriptor: unsafe extern "C" fn() -> *mut c_char,
    #[allow(clippy::type_complexity)]
    pub recognize: unsafe extern "C" fn(
        request_json: *const c_char,
        data: *const u8,
        len: usize,
        cancel: *const AtomicBool,
        out: *mut *mut c_char,
    ) -> c_int,
    pub free_string: unsafe extern "C" fn(s: *mut c_char),
}

#[derive(Debug, thiserror::Error)]
pub enum OcrError {
    #[error("no OCR engine is registered on this device")]
    Unavailable,
    #[error("OCR host refused: {0}")]
    Refused(String),
    #[error("OCR failed: {0}")]
    Failed(String),
    #[error("OCR cancelled")]
    Cancelled,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OcrPage {
    pub page: u32,
    pub text: String,
    /// `text_layer` (the PDF already carried text; nothing was recognised)
    /// or `ocr`.
    pub source: String,
    pub confidence: f64,
}

#[derive(Debug, Clone)]
pub struct OcrOutput {
    pub engine: String,
    pub pages: Vec<OcrPage>,
}

/// One image handed to an engine that reads images (not whole PDFs).
pub enum OcrImage<'a> {
    /// A PNG or JPEG file.
    Encoded(&'a [u8]),
    /// 8-bit RGB, `width * height * 3` bytes.
    Rgb {
        width: u32,
        height: u32,
        data: &'a [u8],
    },
}

/// An on-device text-recognition engine. Platform engines (Apple Vision) and
/// the vision-LLM engine (any installed vision-language GGUF) both implement
/// it; the `ocr.extract` tool does not care which one answers.
pub trait OcrEngine: Send + Sync {
    fn name(&self) -> String;
    fn languages(&self) -> Vec<String>;
    /// True when the engine reads whole PDFs itself (text layers and scans).
    /// Otherwise the tool splits the PDF: text-layer pages are used as they
    /// are and scanned pages are passed to [`OcrEngine::recognize_image`].
    fn reads_pdf(&self) -> bool;
    fn recognize(
        &self,
        kind: &str,
        bytes: &[u8],
        languages: &[String],
        max_pages: u32,
        cancel: &AtomicBool,
    ) -> Result<OcrOutput, OcrError>;
    /// Text of one image and a 0..1 confidence (mean token probability for
    /// the LLM engine; the platform's own score otherwise).
    fn recognize_image(
        &self,
        image: OcrImage<'_>,
        languages: &[String],
        cancel: &AtomicBool,
    ) -> Result<(String, f64), OcrError>;
}

impl OcrEngine for OcrHost {
    fn name(&self) -> String {
        self.engine.clone()
    }
    fn languages(&self) -> Vec<String> {
        self.languages.clone()
    }
    fn reads_pdf(&self) -> bool {
        true
    }
    fn recognize(
        &self,
        kind: &str,
        bytes: &[u8],
        languages: &[String],
        max_pages: u32,
        cancel: &AtomicBool,
    ) -> Result<OcrOutput, OcrError> {
        OcrHost::recognize(self, kind, bytes, languages, max_pages, cancel)
    }
    fn recognize_image(
        &self,
        image: OcrImage<'_>,
        languages: &[String],
        cancel: &AtomicBool,
    ) -> Result<(String, f64), OcrError> {
        let OcrImage::Encoded(bytes) = image else {
            return Err(OcrError::Failed(
                "this engine reads encoded images only".into(),
            ));
        };
        let out = OcrHost::recognize(self, "image", bytes, languages, 1, cancel)?;
        let page = out
            .pages
            .into_iter()
            .next()
            .ok_or_else(|| OcrError::Failed("no text".into()))?;
        Ok((page.text, page.confidence))
    }
}

pub struct OcrHost {
    vtable: OcrHostVtable,
    pub engine: String,
    pub languages: Vec<String>,
}

// SAFETY: the host contract requires callbacks be callable from any thread.
unsafe impl Send for OcrHost {}
unsafe impl Sync for OcrHost {}

impl OcrHost {
    pub fn from_vtable(vtable: OcrHostVtable) -> Result<Self, OcrError> {
        let raw = unsafe { (vtable.descriptor)() };
        if raw.is_null() {
            return Err(OcrError::Refused("host returned no descriptor".into()));
        }
        let text = unsafe { CStr::from_ptr(raw) }
            .to_string_lossy()
            .into_owned();
        unsafe { (vtable.free_string)(raw) };
        let d: Value = serde_json::from_str(&text)
            .map_err(|e| OcrError::Refused(format!("descriptor: {e}")))?;
        if d["schema"] != "harbor.ocr_host/v1" {
            return Err(OcrError::Refused(
                "descriptor schema must be harbor.ocr_host/v1".into(),
            ));
        }
        if d["execution_location"] != "on_device" {
            return Err(OcrError::Refused(
                "an OCR host must execute on-device; remote OCR is not a host".into(),
            ));
        }
        Ok(OcrHost {
            vtable,
            engine: d["engine"].as_str().unwrap_or("unknown").to_string(),
            languages: d["languages"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default(),
        })
    }

    /// `kind` is `pdf` or `image`. PDF pages with a text layer are returned
    /// as `text_layer` and not recognised.
    pub fn recognize(
        &self,
        kind: &str,
        bytes: &[u8],
        languages: &[String],
        max_pages: u32,
        cancel: &AtomicBool,
    ) -> Result<OcrOutput, OcrError> {
        let req = CString::new(
            serde_json::json!({"kind": kind, "languages": languages, "max_pages": max_pages})
                .to_string(),
        )
        .map_err(|e| OcrError::Failed(e.to_string()))?;
        let mut out: *mut c_char = std::ptr::null_mut();
        let code = unsafe {
            (self.vtable.recognize)(req.as_ptr(), bytes.as_ptr(), bytes.len(), cancel, &mut out)
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
        match code {
            0 => {}
            3 => return Err(OcrError::Cancelled),
            _ => {
                return Err(OcrError::Failed(
                    v["error"].as_str().unwrap_or("host failure").to_string(),
                ))
            }
        }
        let pages = v["pages"]
            .as_array()
            .ok_or_else(|| OcrError::Failed("host returned no pages".into()))?
            .iter()
            .map(|p| OcrPage {
                page: p["page"].as_u64().unwrap_or(0) as u32,
                text: p["text"].as_str().unwrap_or("").to_string(),
                source: p["source"].as_str().unwrap_or("ocr").to_string(),
                confidence: p["confidence"].as_f64().unwrap_or(0.0),
            })
            .collect();
        Ok(OcrOutput {
            engine: v["engine"].as_str().unwrap_or(&self.engine).to_string(),
            pages,
        })
    }
}

/// The engine scans are read with, and the platform's own engine (Apple
/// Vision) kept apart: choosing a vision-language model replaces the active
/// engine, and releasing it (memory pressure) must bring the platform engine
/// back rather than leave the device with none.
static HOST: OnceLock<Mutex<Option<std::sync::Arc<dyn OcrEngine>>>> = OnceLock::new();
static PLATFORM: OnceLock<Mutex<Option<std::sync::Arc<dyn OcrEngine>>>> = OnceLock::new();

fn slot() -> &'static Mutex<Option<std::sync::Arc<dyn OcrEngine>>> {
    HOST.get_or_init(|| Mutex::new(None))
}

fn platform_slot() -> &'static Mutex<Option<std::sync::Arc<dyn OcrEngine>>> {
    PLATFORM.get_or_init(|| Mutex::new(None))
}

/// Make `engine` the active OCR engine (a vision-language model the person
/// chose). The platform engine, if any, stays registered underneath.
pub fn register_ocr_engine(engine: std::sync::Arc<dyn OcrEngine>) {
    *slot().lock().unwrap() = Some(engine);
}

/// Register a platform host through its C vtable. The vtable's functions and
/// their backing library must stay loaded for the life of the process.
pub fn register_ocr_host(vtable: OcrHostVtable) -> Result<(), OcrError> {
    let host: std::sync::Arc<dyn OcrEngine> = std::sync::Arc::new(OcrHost::from_vtable(vtable)?);
    *platform_slot().lock().unwrap() = Some(host.clone());
    register_ocr_engine(host);
    Ok(())
}

/// Drop the active engine and fall back to the platform's own (if there is
/// one). Returns whether an engine remains.
pub fn release_ocr_engine() -> bool {
    let platform = platform_slot().lock().unwrap().clone();
    let mut active = slot().lock().unwrap();
    *active = platform;
    active.is_some()
}

/// Forget every engine, platform included.
pub fn clear_ocr_host() {
    *platform_slot().lock().unwrap() = None;
    *slot().lock().unwrap() = None;
}

pub fn ocr_host() -> Result<std::sync::Arc<dyn OcrEngine>, OcrError> {
    slot().lock().unwrap().clone().ok_or(OcrError::Unavailable)
}
