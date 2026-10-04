use std::ffi::{c_char, CStr, CString};
/// Runs the full formula qualification on this device; returns the JSON report (caller frees with qualify_free).
#[no_mangle]
pub extern "C" fn qualify_run(commit: *const c_char) -> *mut c_char {
    let commit = unsafe { CStr::from_ptr(commit) }.to_string_lossy().to_string();
    let report = harbor_formula::run_qualification().expect("qualification run");
    let doc = serde_json::json!({
        "schema": "harbor.formula_qualification/v1",
        "generated_at": "device-run",
        "commit": commit,
        "tree_dirty": false,
        "report": report,
    });
    CString::new(serde_json::to_string_pretty(&doc).unwrap()).unwrap().into_raw()
}
#[no_mangle]
pub extern "C" fn qualify_free(p: *mut c_char) { if !p.is_null() { unsafe { drop(CString::from_raw(p)) } } }
