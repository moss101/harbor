//! OCR qualification on the platform engine (decision 0018): scanned PDFs
//! with KNOWN text go through the real Apple Vision host and the
//! `ocr.extract` tool; character error rate is measured against the truth
//! file. macOS only; skips when the host dylib is not built:
//!
//! ```text
//! tools/build_apple_ocr_host.sh
//! cargo test -p harbor_core --test ocr_apple -- --nocapture
//! ```

#![cfg(target_os = "macos")]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use libloading::{Library, Symbol};
use serde_json::{json, Value};

use harbor_core::tools::{MemoryArtifacts, ToolContext, ToolRegistry};
use harbor_inference::ocr::{clear_ocr_host, register_ocr_host, OcrHostVtable};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

/// Levenshtein distance over chars.
fn edit_distance(a: &[char], b: &[char]) -> usize {
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut cur = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            let sub = prev[j] + usize::from(ca != cb);
            cur.push(sub.min(prev[j + 1] + 1).min(cur[j] + 1));
        }
        prev = cur;
    }
    prev[b.len()]
}

fn norm(s: &str) -> Vec<char> {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .collect()
}

fn cer(got: &str, want: &str) -> f64 {
    let (g, w) = (norm(got), norm(want));
    edit_distance(&g, &w) as f64 / w.len().max(1) as f64
}

fn run_ocr(fixture: &str, languages: &[&str]) -> Result<Value, String> {
    let bytes = std::fs::read(root().join("fixtures/ocr").join(fixture)).unwrap();
    let artifacts = MemoryArtifacts::new().with("scan", fixture, bytes);
    let host = json!({});
    let cancel = AtomicBool::new(false);
    let ctx = ToolContext::new(&artifacts, &host, &cancel);
    let registry = ToolRegistry::builtin();
    let allow: BTreeSet<String> = registry.ids().into_iter().collect();
    registry
        .call(
            &ctx,
            "ocr.extract",
            &json!({"artifact_id": "scan", "languages": languages}),
            &allow,
        )
        .map(|r| r.output)
        .map_err(|e| e.to_string())
}

#[test]
fn scanned_pdfs_are_read_on_device_and_measured_against_known_text() {
    let dylib = root().join("native/apple/ocr_host/build/libharbor_ocr_host.dylib");
    if !dylib.exists() {
        eprintln!("SKIP: build the host with tools/build_apple_ocr_host.sh");
        return;
    }
    let truth: Value = serde_json::from_str(
        &std::fs::read_to_string(root().join("fixtures/ocr/truth.json")).unwrap(),
    )
    .unwrap();
    let want = |name: &str, page: usize| -> String {
        truth[name]["pages"][page]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l.as_str().unwrap())
            .collect::<Vec<_>>()
            .join("\n")
    };

    // --- no engine registered: a scan is a typed error, text layers still read
    clear_ocr_host();
    let err = run_ocr("scanned_invoice_en.pdf", &[]).unwrap_err();
    assert!(err.contains("no OCR engine"), "{err}");
    let err = run_ocr("mixed_text_and_scan.pdf", &[]).unwrap_err();
    assert!(err.contains("1 of 2 page(s) have no text layer"), "{err}");

    // --- the real host
    unsafe {
        let lib = Library::new(&dylib).unwrap();
        let descriptor: Symbol<unsafe extern "C" fn() -> *mut std::ffi::c_char> =
            lib.get(b"harbor_ocr_host_descriptor").unwrap();
        let recognize: Symbol<
            unsafe extern "C" fn(
                *const std::ffi::c_char,
                *const u8,
                usize,
                *const AtomicBool,
                *mut *mut std::ffi::c_char,
            ) -> std::ffi::c_int,
        > = lib.get(b"harbor_ocr_host_recognize").unwrap();
        let free: Symbol<unsafe extern "C" fn(*mut std::ffi::c_char)> =
            lib.get(b"harbor_ocr_host_free_string").unwrap();
        register_ocr_host(OcrHostVtable {
            descriptor: *descriptor,
            recognize: *recognize,
            free_string: *free,
        })
        .unwrap();
        // The library must outlive every call through the vtable.
        std::mem::forget(lib);
    }

    let en = run_ocr("scanned_invoice_en.pdf", &["en-US"]).unwrap();
    let en_cer = cer(
        en["text"].as_str().unwrap(),
        &want("scanned_invoice_en.pdf", 0),
    );
    println!(
        "EN scan: engine={} CER={:.4} conf={}\n{}",
        en["engine"], en_cer, en["pages"][0]["confidence"], en["text"]
    );
    assert_eq!(en["source"], "ocr");
    assert_eq!(en["pages"][0]["source"], "ocr");
    assert!(en_cer < 0.03, "English CER {en_cer}");
    // The figures the invoice checks depend on must be read exactly.
    for must in ["INV-1001", "4,200.00", "2026-08-31", "INV-1002", "1,850.50"] {
        assert!(
            en["text"].as_str().unwrap().contains(must),
            "missing {must}:\n{}",
            en["text"]
        );
    }

    // Camera-style inputs: low-resolution PNG and heavily compressed JPEG
    // of the degraded page, read as images (not PDFs).
    for photo in ["photo_invoice_en.png", "photo_invoice_en.jpg"] {
        let r = run_ocr(photo, &["en-US"]).unwrap();
        let c = cer(
            r["text"].as_str().unwrap(),
            &want("scanned_invoice_en_degraded.pdf", 0),
        );
        println!("{photo}: CER={c:.4}\n{}", r["text"]);
        assert_eq!(r["source"], "ocr");
        assert!(c < 0.10, "{photo}: CER {c}");
        for must in ["INV-1001", "4,200.00", "INV-1002", "1,850.50"] {
            assert!(
                r["text"].as_str().unwrap().contains(must),
                "{photo}: missing {must}"
            );
        }
    }

    // A rotated, downsampled, noisy "phone photo" of the same page.
    let dg = run_ocr("scanned_invoice_en_degraded.pdf", &["en-US"]).unwrap();
    let dg_cer = cer(
        dg["text"].as_str().unwrap(),
        &want("scanned_invoice_en_degraded.pdf", 0),
    );
    println!(
        "EN degraded: CER={:.4} conf={}\n{}",
        dg_cer, dg["pages"][0]["confidence"], dg["text"]
    );
    assert!(dg_cer < 0.10, "degraded CER {dg_cer}");

    let ar = run_ocr("scanned_invoice_ar.pdf", &["ar-SA"]).unwrap();
    let ar_cer = cer(
        ar["text"].as_str().unwrap(),
        &want("scanned_invoice_ar.pdf", 0),
    );
    println!(
        "AR scan: CER={:.4} conf={}\n{}",
        ar_cer, ar["pages"][0]["confidence"], ar["text"]
    );
    assert!(ar_cer < 0.25, "Arabic CER {ar_cer}");

    let mixed = run_ocr("mixed_text_and_scan.pdf", &[]).unwrap();
    let pages = mixed["pages"].as_array().unwrap();
    assert_eq!(pages.len(), 2);
    assert_eq!(
        pages[0]["source"], "text_layer",
        "page 1 has a text layer; it is not re-recognised"
    );
    assert_eq!(pages[1]["source"], "ocr");
    assert_eq!(pages[1]["page"], 2, "page numbers survive for citation");
    let p2 = cer(
        pages[1]["text"].as_str().unwrap(),
        &want("mixed_text_and_scan.pdf", 1),
    );
    println!("mixed p2 CER={p2:.4}");
    assert!(p2 < 0.03);
}
