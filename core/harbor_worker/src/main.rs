//! `harbor_worker <path-to-core-library>`: load Harbor's core library and
//! run its inference worker (`harbor_worker_main`) on stdin/stdout.
//!
//! Spawned by the core's supervisor (decision 0022 phase 3); not meant to be
//! run by hand. Exit codes: the worker's own, or 64 (usage), 65 (library
//! could not be loaded), 66 (library has no worker entry point).

use std::ffi::OsString;

fn main() {
    let Some(library) = std::env::args_os().nth(1) else {
        eprintln!("usage: harbor_worker <path to the Harbor core library>");
        std::process::exit(64);
    };
    std::process::exit(run(library));
}

#[cfg(unix)]
fn run(library: OsString) -> i32 {
    use std::ffi::{c_char, c_int, c_void, CString};
    use std::os::unix::ffi::OsStrExt;
    extern "C" {
        fn dlopen(path: *const c_char, flags: c_int) -> *mut c_void;
        fn dlsym(handle: *mut c_void, name: *const c_char) -> *mut c_void;
        fn dlerror() -> *const c_char;
    }
    // RTLD_NOW is 2 on Darwin, glibc and bionic.
    const RTLD_NOW: c_int = 0x2;
    let Ok(path) = CString::new(library.as_bytes()) else {
        return 64;
    };
    // SAFETY: dlopen/dlsym with NUL-terminated strings; the symbol is the
    // core's `extern "C" fn() -> i32` entry point.
    unsafe {
        let handle = dlopen(path.as_ptr(), RTLD_NOW);
        if handle.is_null() {
            let e = dlerror();
            if !e.is_null() {
                eprintln!(
                    "harbor_worker: {}",
                    std::ffi::CStr::from_ptr(e).to_string_lossy()
                );
            }
            return 65;
        }
        let sym = dlsym(handle, c"harbor_worker_main".as_ptr());
        if sym.is_null() {
            eprintln!("harbor_worker: library has no harbor_worker_main");
            return 66;
        }
        let entry: extern "C" fn() -> i32 = std::mem::transmute(sym);
        entry()
    }
}

#[cfg(windows)]
fn run(library: OsString) -> i32 {
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStrExt;
    extern "system" {
        fn LoadLibraryW(name: *const u16) -> *mut c_void;
        fn GetProcAddress(module: *mut c_void, name: *const u8) -> *mut c_void;
    }
    let wide: Vec<u16> = library.encode_wide().chain(std::iter::once(0)).collect();
    // SAFETY: documented Win32 loader calls; the symbol is the core's
    // `extern "C" fn() -> i32` entry point.
    unsafe {
        let module = LoadLibraryW(wide.as_ptr());
        if module.is_null() {
            eprintln!("harbor_worker: cannot load {}", library.to_string_lossy());
            return 65;
        }
        let sym = GetProcAddress(module, c"harbor_worker_main".as_ptr().cast());
        if sym.is_null() {
            eprintln!("harbor_worker: library has no harbor_worker_main");
            return 66;
        }
        let entry: extern "C" fn() -> i32 = std::mem::transmute(sym);
        entry()
    }
}
