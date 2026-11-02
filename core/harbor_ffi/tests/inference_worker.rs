//! Decision 0022 phase 3: the inference worker, end to end through the real
//! launcher (`target/<profile>/harbor_worker`) and the real core library
//! (`libharbor_ffi`), on the real llama.cpp runtime.
//!
//! Build both first: `cargo build -p harbor_ffi -p harbor_worker`.

#![cfg(feature = "gguf")]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;

use harbor_canonical::JsonValue;
use harbor_inference::provider::{ChatRequest, ModelProvider, ModelRef, ProviderError};
use harbor_inference::resilience::{runtime_dir_for, QualState, RuntimeState};
use harbor_inference::worker::{WorkerConfig, WorkerProvider};
use harbor_modelhub::install::{PackageFile, PackageInstaller, PackageManifest, RuntimeBinding};

/// The fault hook reads the environment of the worker at spawn: tests that
/// set it must not overlap with tests that spawn workers.
static ENV: Mutex<()> = Mutex::new(());

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// `fixtures/models`, or `HARBOR_FIXTURES_DIR` on a device.
fn fixtures_dir() -> PathBuf {
    std::env::var_os("HARBOR_FIXTURES_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| repo_root().join("fixtures/models"))
}

fn config() -> WorkerConfig {
    // On a device the launcher and library are pushed next to the test.
    if let (Some(launcher), Some(library)) = (
        std::env::var_os("HARBOR_WORKER_PATH"),
        std::env::var_os("HARBOR_WORKER_LIB"),
    ) {
        return WorkerConfig {
            launcher: launcher.into(),
            library: library.into(),
        };
    }
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../target"));
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    let dir = target.join(profile);
    let lib = dir.join(if cfg!(target_os = "macos") {
        "libharbor_ffi.dylib"
    } else if cfg!(windows) {
        "harbor_ffi.dll"
    } else {
        "libharbor_ffi.so"
    });
    let launcher = dir.join(if cfg!(windows) {
        "harbor_worker.exe"
    } else {
        "harbor_worker"
    });
    assert!(
        lib.is_file() && launcher.is_file(),
        "build the core library and launcher first: cargo build -p harbor_ffi -p harbor_worker ({} / {})",
        lib.display(),
        launcher.display()
    );
    WorkerConfig {
        launcher,
        library: lib,
    }
}

/// The gitignored model fixtures, or None (skip) when absent — unless
/// `HARBOR_REQUIRE_FIXTURES=1`, where absence fails.
fn fixtures_present(names: &[&str]) -> bool {
    let missing: Vec<&&str> = names
        .iter()
        .filter(|n| !fixtures_dir().join(n).exists())
        .collect();
    if missing.is_empty() {
        return true;
    }
    assert!(
        std::env::var("HARBOR_REQUIRE_FIXTURES").as_deref() != Ok("1"),
        "fixtures missing: {missing:?}"
    );
    eprintln!("SKIP: fixtures missing: {missing:?}");
    false
}

fn install(root: &Path, id: &str, fixture: &str) -> ModelRef {
    let bytes = std::fs::read(fixtures_dir().join(fixture))
        .unwrap_or_else(|e| panic!("fixture {fixture}: {e}"));
    let installer = PackageInstaller::new(root);
    let file = PackageFile {
        role: "weights".into(),
        path: fixture.into(),
        sha256: harbor_canonical::sha256_hex(&bytes),
        size_bytes: bytes.len() as u64,
    };
    let manifest = PackageManifest {
        schema: "harbor.model/v3".into(),
        id: id.into(),
        reference_type: "installed_package".into(),
        files: vec![file.clone()],
        runtime: RuntimeBinding {
            kind: "gguf/llama.cpp".into(),
            min_revision: "0.1.156".into(),
            targets: vec![std::env::consts::ARCH.to_string()],
        },
    };
    let mut staged = installer.begin(id).unwrap();
    installer.ingest_file(&mut staged, &file, &bytes).unwrap();
    assert!(installer.validate(&staged, &manifest).unwrap().ok);
    installer
        .commit(&mut staged, &manifest, chrono::Utc::now())
        .unwrap();
    ModelRef::InstalledPackage {
        package_id: id.into(),
    }
}

fn story(model: &ModelRef, max_tokens: u32) -> ChatRequest {
    ChatRequest {
        model: model.clone(),
        messages: vec![JsonValue::object([
            ("role", JsonValue::str("user")),
            ("content", JsonValue::str("Once upon a time")),
        ])],
        max_tokens,
        temperature: 0.0,
        requires: vec![],
        response_schema: None,
        trace_key: None,
    }
}

#[test]
fn worker_generates_and_embeds_like_the_in_process_runtime() {
    let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
    if !fixtures_present(&["stories260K.gguf", "bge-small-en-v1.5-q8_0.gguf"]) {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let chat = install(dir.path(), "stories260k", "stories260K.gguf");
    let emb = install(dir.path(), "bge-small", "bge-small-en-v1.5-q8_0.gguf");
    let worker = WorkerProvider::new(config(), dir.path()).with_context_tokens(512);
    worker.load(&chat).unwrap();
    let pid = worker.worker_pid().expect("worker running");
    assert_ne!(pid, std::process::id());

    let progress = AtomicU64::new(0);
    let never = AtomicBool::new(false);
    let r = worker
        .generate_cancellable(story(&chat, 24), &never, Some(&progress))
        .unwrap();
    assert!(r.usage.completion_tokens > 0);

    // Same weights, same greedy decoding, in this process: same text.
    let local = harbor_inference::GgufLlamaCppProvider::new(dir.path())
        .unwrap()
        .with_context_tokens(512);
    local.load(&chat).unwrap();
    assert_eq!(local.generate(story(&chat, 24)).unwrap().content, r.content);

    let texts = vec![
        "The cat sat.".to_string(),
        "Quarterly revenue rose.".to_string(),
    ];
    let a = worker.embed(&emb, &texts).unwrap();
    local.load(&emb).unwrap();
    let b = ModelProvider::embed(&local, &emb, &texts).unwrap();
    assert_eq!(a.len(), 2);
    for (x, y) in a.iter().zip(&b) {
        assert_eq!(x.len(), y.len());
        let cos = harbor_knowledge::index::cosine(x, y);
        assert!(
            cos > 0.9999,
            "worker and in-process embeddings differ: {cos}"
        );
    }
    assert_eq!(worker.restarts(), 0);
}

#[test]
fn a_worker_crash_is_a_typed_error_and_the_next_call_restarts_it() {
    let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
    if !fixtures_present(&["stories260K.gguf"]) {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let chat = install(dir.path(), "stories260k", "stories260K.gguf");
    let worker = WorkerProvider::new(config(), dir.path()).with_context_tokens(512);
    worker.load(&chat).unwrap();
    let first = worker.worker_pid().unwrap();
    let e = worker.debug_abort_worker().unwrap_err();
    assert!(
        matches!(&e, ProviderError::RuntimeCrashed(m) if m.contains("crashed")),
        "{e}"
    );
    // This process is alive, and the next call just works on a new worker
    // (which reloads the model itself).
    let r = worker.generate(story(&chat, 8)).unwrap();
    assert!(r.usage.completion_tokens > 0);
    assert_ne!(worker.worker_pid().unwrap(), first);
    assert_eq!(worker.restarts(), 1);
}

#[cfg(unix)]
#[test]
fn an_idle_worker_killed_by_the_os_is_replaced_silently_and_not_counted() {
    let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
    if !fixtures_present(&["stories260K.gguf"]) {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let chat = install(dir.path(), "stories260k", "stories260K.gguf");
    let worker = WorkerProvider::new(config(), dir.path()).with_context_tokens(512);
    worker.load(&chat).unwrap();
    worker.generate(story(&chat, 4)).unwrap();
    let pid = worker.worker_pid().unwrap();
    assert!(std::process::Command::new("kill")
        .args(["-9", &pid.to_string()])
        .status()
        .unwrap()
        .success());
    std::thread::sleep(std::time::Duration::from_millis(300));
    let r = worker.generate(story(&chat, 4)).unwrap();
    assert!(r.usage.completion_tokens > 0);
    assert_eq!(worker.restarts(), 1);
    let state = RuntimeState::open(runtime_dir_for(dir.path()));
    assert!(
        state.records().iter().all(|r| !r.state.pins_cpu()),
        "an OS kill must never pin the GPU: {:?}",
        state.records()
    );
}

/// The whole chain on a GPU machine: the worker dies inside its first GPU
/// workload, the supervisor reads it as a crash, recovery pins the key, and
/// the SAME call is retried once on a fresh worker that runs on the CPU.
#[test]
fn a_gpu_crash_in_the_worker_pins_cpu_and_the_call_succeeds_on_retry() {
    let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
    if !fixtures_present(&["stories260K.gguf"]) {
        return;
    }
    if !harbor_inference::gguf::gpu_backend_present()
        || std::env::var("HARBOR_GGUF_CPU_ONLY").as_deref() == Ok("1")
    {
        eprintln!("no GPU backend here: nothing to pin");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let chat = install(dir.path(), "stories260k", "stories260K.gguf");
    std::env::set_var("HARBOR_FAULT_ABORT_AT_TOKEN", "1");
    let worker = WorkerProvider::new(config(), dir.path()).with_context_tokens(512);
    let result = worker.generate(story(&chat, 8));
    std::env::remove_var("HARBOR_FAULT_ABORT_AT_TOKEN");
    let r = result.expect("retried on the CPU");
    assert!(r.usage.completion_tokens > 0);
    assert_eq!(worker.restarts(), 1);
    assert_eq!(worker.backend_of("stories260k"), Some("cpu_pinned"));
    let state = RuntimeState::open(runtime_dir_for(dir.path()));
    let records = state.records();
    assert_eq!(records.len(), 1, "{records:?}");
    assert_eq!(records[0].state, QualState::Crashed);
}

#[test]
fn cancellation_crosses_the_process_boundary() {
    let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
    if !fixtures_present(&["stories260K.gguf"]) {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let chat = install(dir.path(), "stories260k", "stories260K.gguf");
    std::env::set_var("HARBOR_FAULT_SLOW_TOKEN_MS", "30");
    let worker = WorkerProvider::new(config(), dir.path()).with_context_tokens(512);
    worker.load(&chat).unwrap();
    std::env::remove_var("HARBOR_FAULT_SLOW_TOKEN_MS");
    let cancel = AtomicBool::new(false);
    let progress = AtomicU64::new(0);
    let r = std::thread::scope(|s| {
        s.spawn(|| {
            while progress.load(Ordering::Relaxed) < 2 {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            cancel.store(true, Ordering::Relaxed);
        });
        worker.generate_cancellable(story(&chat, 400), &cancel, Some(&progress))
    });
    assert!(matches!(r, Err(ProviderError::Cancelled)), "{r:?}");
    assert!(
        progress.load(Ordering::Relaxed) >= 2,
        "progress crossed the boundary"
    );
    // Still usable afterwards, on the same worker.
    assert!(worker.generate(story(&chat, 2)).is_ok());
    assert_eq!(worker.restarts(), 0);
}
