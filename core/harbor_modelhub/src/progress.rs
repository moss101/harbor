//! Shared progress + cancellation surface for long-running core
//! operations (model acquisition, knowledge ingestion, grounded
//! generation). Runtime layers (FFI ops) own the state; the workers
//! update it cooperatively at natural boundaries (chunks, tokens,
//! files) so the UI can poll real progress and cancel.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;

/// Cooperative progress + cancel state for one background operation.
///
/// `phase` is a short machine-readable stage name ("resolving",
/// "downloading", "verifying", "installing", "generating", "ingesting");
/// `detail` is human-readable context ("weights.gguf 3/4"). Byte and
/// item counters are monotonic for the current unit of work.
#[derive(Default, Debug)]
pub struct AcquireProgress {
    pub phase: Mutex<String>,
    pub detail: Mutex<String>,
    pub bytes_done: AtomicU64,
    pub bytes_total: AtomicU64,
    pub items_done: AtomicU64,
    pub items_total: AtomicU64,
    pub cancel: AtomicBool,
}

impl AcquireProgress {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_phase(&self, phase: &str) {
        *self.phase.lock().unwrap() = phase.to_string();
    }

    pub fn phase(&self) -> String {
        self.phase.lock().unwrap().clone()
    }

    pub fn set_detail(&self, detail: &str) {
        *self.detail.lock().unwrap() = detail.to_string();
    }

    pub fn detail(&self) -> String {
        self.detail.lock().unwrap().clone()
    }

    pub fn request_cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    pub fn snapshot(&self) -> ProgressSnapshot {
        ProgressSnapshot {
            phase: self.phase(),
            detail: self.detail(),
            bytes_done: self.bytes_done.load(Ordering::Relaxed),
            bytes_total: self.bytes_total.load(Ordering::Relaxed),
            items_done: self.items_done.load(Ordering::Relaxed),
            items_total: self.items_total.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ProgressSnapshot {
    pub phase: String,
    pub detail: String,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub items_done: u64,
    pub items_total: u64,
}
