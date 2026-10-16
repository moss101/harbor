//! Choosing a vision-model OCR engine replaces the platform engine; releasing
//! it must bring the platform engine back, not leave the device with none.
//! Own test binary: the OCR registry is process-global.
use harbor_inference::ocr::{
    ocr_host, register_ocr_engine, register_ocr_host, release_ocr_engine, OcrEngine, OcrError,
    OcrHostVtable, OcrImage, OcrOutput,
};
use std::ffi::{c_char, CString};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

unsafe extern "C" fn descriptor() -> *mut c_char {
    CString::new(
        r#"{"schema":"harbor.ocr_host/v1","engine":"platform-fake","execution_location":"on_device","languages":["en"]}"#,
    )
    .unwrap()
    .into_raw()
}
unsafe extern "C" fn recognize(
    _: *const c_char,
    _: *const u8,
    _: usize,
    _: *const AtomicBool,
    _: *mut *mut c_char,
) -> std::ffi::c_int {
    1
}
unsafe extern "C" fn free_string(s: *mut c_char) {
    if !s.is_null() {
        drop(CString::from_raw(s));
    }
}

struct Vlm;
impl OcrEngine for Vlm {
    fn name(&self) -> String {
        "vlm-fake".into()
    }
    fn languages(&self) -> Vec<String> {
        vec![]
    }
    fn reads_pdf(&self) -> bool {
        false
    }
    fn recognize(
        &self,
        _: &str,
        _: &[u8],
        _: &[String],
        _: u32,
        _: &AtomicBool,
    ) -> Result<OcrOutput, OcrError> {
        Err(OcrError::Unavailable)
    }
    fn recognize_image(
        &self,
        _: OcrImage<'_>,
        _: &[String],
        _: &AtomicBool,
    ) -> Result<(String, f64), OcrError> {
        Err(OcrError::Unavailable)
    }
}

#[test]
fn releasing_the_vision_engine_restores_the_platform_engine() {
    // No platform engine: release leaves nothing.
    assert!(!release_ocr_engine());
    register_ocr_engine(Arc::new(Vlm));
    assert_eq!(ocr_host().unwrap().name(), "vlm-fake");
    assert!(!release_ocr_engine());
    assert!(ocr_host().is_err());

    register_ocr_host(OcrHostVtable {
        descriptor,
        recognize,
        free_string,
    })
    .unwrap();
    assert_eq!(ocr_host().unwrap().name(), "platform-fake");
    register_ocr_engine(Arc::new(Vlm));
    assert_eq!(ocr_host().unwrap().name(), "vlm-fake");
    assert!(release_ocr_engine());
    assert_eq!(ocr_host().unwrap().name(), "platform-fake");
}
