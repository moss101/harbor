use harbor_ffi::{harbor_core_close, harbor_core_open_ex};
use std::ffi::CString;
#[test]
#[ignore = "reopens a workspace written by an older build (OLD_WS_ROOT)"]
fn old_workspace_opens() {
    let root = CString::new(std::env::var("OLD_WS_ROOT").unwrap()).unwrap();
    let ws = CString::new("ws-old").unwrap();
    let hex =
        CString::new("12721f8a3480ca77d995c914cb6dbc50f401e82e317b3b96e0b0e2b01747ca10").unwrap();
    let h = harbor_core_open_ex(root.as_ptr(), ws.as_ptr(), 0, hex.as_ptr());
    assert!(!h.is_null(), "an older workspace failed to open");
    unsafe { harbor_core_close(h) };
}

#[test]
fn a_failed_open_says_why() {
    use std::ffi::CStr;
    let dir = tempfile::tempdir().unwrap();
    let root = CString::new(dir.path().to_string_lossy().to_string()).unwrap();
    let ws = CString::new("ws").unwrap();
    // An invalid privacy mode makes the open fail deterministically.
    let h = harbor_ffi::harbor_core_open_ex(root.as_ptr(), ws.as_ptr(), 9, std::ptr::null());
    assert!(h.is_null());
    let p = harbor_ffi::harbor_core_last_open_error();
    let msg = unsafe { CStr::from_ptr(p) }.to_string_lossy().to_string();
    unsafe { harbor_ffi::harbor_core_string_free(p) };
    assert!(msg.contains("bad privacy mode 9"), "{msg}");
    // A successful open clears it.
    let hex =
        CString::new("12721f8a3480ca77d995c914cb6dbc50f401e82e317b3b96e0b0e2b01747ca10").unwrap();
    let h = harbor_ffi::harbor_core_open_ex(root.as_ptr(), ws.as_ptr(), 0, hex.as_ptr());
    assert!(!h.is_null());
    unsafe { harbor_core_close(h) };
    let p = harbor_ffi::harbor_core_last_open_error();
    assert_eq!(unsafe { CStr::from_ptr(p) }.to_string_lossy(), "");
    unsafe { harbor_ffi::harbor_core_string_free(p) };
}
