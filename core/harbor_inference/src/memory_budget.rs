//! Pre-load memory budget (decision 0022, phase 2).
//!
//! Before llama.cpp maps a model, estimate what the load will need (weights +
//! KV cache + compute buffers) and refuse with a typed [`BudgetError`] when it
//! cannot fit, instead of being killed by jetsam (iOS) or the low-memory
//! killer (Android) halfway through the load.
//!
//! What "available" means differs per platform, deliberately:
//!
//! * **iOS** — `os_proc_available_memory()`: the bytes this process may still
//!   allocate before jetsam kills it. This is the real limit and the one that
//!   matters most. (It returns 0 where no limit applies, e.g. the simulator;
//!   that reads as "unknown".)
//! * **Android, Linux** — `MemAvailable` from `/proc/meminfo`, the kernel's
//!   estimate of memory available without swapping.
//! * **macOS, Windows** — total physical memory (`hw.memsize`,
//!   `GlobalMemoryStatusEx().ullTotalPhys`). On desktop we only refuse what
//!   can never fit; swap and memory compression handle the rest.
//!
//! `HARBOR_MEMORY_BUDGET_BYTES` overrides the platform query (tests, and
//! forcing a refusal on a device).
//!
//! No llama.cpp dependency and no extra crates: the few OS calls are
//! declared by hand below.

use crate::gguf_check::GgufSummary;

/// Environment variable that overrides [`available_bytes`].
pub const BUDGET_ENV: &str = "HARBOR_MEMORY_BUDGET_BYTES";

/// Fixed allowance for llama.cpp's compute buffers (graph scratch, logits,
/// the output/embedding buffer, Metal/Vulkan staging) on top of weights and
/// KV cache. Measured compute buffers for Harbor's catalog models at their
/// configured contexts are tens to low hundreds of MiB; 256 MiB is a
/// conservative single number rather than a per-architecture model. It is a
/// floor-of-safety, not an exact prediction.
pub const COMPUTE_MARGIN_BYTES: u64 = 256 << 20;

/// Bytes per KV-cache element: llama.cpp's default cache type is f16.
pub const KV_BYTES_PER_ELEMENT: u64 = 2;

/// Refusal reasons.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BudgetError {
    #[error(
        "model needs about {} MiB but only {} MiB is available",
        required / (1 << 20),
        available / (1 << 20)
    )]
    Insufficient { required: u64, available: u64 },
}

/// Bytes the process can use for a model load on this platform, or `None`
/// when it cannot be determined (then no refusal is made).
pub fn available_bytes() -> Option<u64> {
    if let Ok(v) = std::env::var(BUDGET_ENV) {
        if let Some(n) = parse_override(&v) {
            return Some(n);
        }
    }
    platform_available_bytes()
}

/// KV-cache bytes for `n_ctx` tokens: `n_ctx × layers × kv_heads ×
/// (key_dim + value_dim) × 2` (f16). Per-head dims come from
/// `attention.key_length`/`value_length`, else `embedding_length /
/// head_count`; `head_count_kv` defaults to `head_count`. Models without
/// attention heads (recurrent, projectors) contribute 0.
pub fn estimate_kv_bytes(summary: &GgufSummary, n_ctx: u32) -> u64 {
    let layers = summary.block_count.unwrap_or(0);
    let heads = summary.head_count.unwrap_or(0);
    let kv_heads = summary.head_count_kv.unwrap_or(heads);
    let derived = match (summary.embedding_length, heads) {
        (Some(e), h) if h > 0 => e / h,
        _ => 0,
    };
    let k = summary.key_length.unwrap_or(derived);
    let v = summary.value_length.unwrap_or(derived);
    (n_ctx as u64)
        .saturating_mul(layers)
        .saturating_mul(kv_heads)
        .saturating_mul(k.saturating_add(v))
        .saturating_mul(KV_BYTES_PER_ELEMENT)
}

/// Estimated bytes to load `summary` with an `n_ctx`-token context: tensor
/// bytes + KV cache + [`COMPUTE_MARGIN_BYTES`]. For a model plus projector,
/// add the projector's `tensor_bytes` to this.
pub fn estimate_load_bytes(summary: &GgufSummary, n_ctx: u32) -> u64 {
    summary
        .tensor_bytes
        .saturating_add(estimate_kv_bytes(summary, n_ctx))
        .saturating_add(COMPUTE_MARGIN_BYTES)
}

/// Refuse when `estimate` exceeds [`available_bytes`]. Unknown availability
/// never refuses.
pub fn check_budget(estimate: u64) -> Result<(), BudgetError> {
    check_budget_against(estimate, available_bytes())
}

/// [`check_budget`] against an explicit availability (testable, and for
/// callers that already queried it).
pub fn check_budget_against(estimate: u64, available: Option<u64>) -> Result<(), BudgetError> {
    match available {
        Some(available) if estimate > available => Err(BudgetError::Insufficient {
            required: estimate,
            available,
        }),
        _ => Ok(()),
    }
}

fn parse_override(v: &str) -> Option<u64> {
    v.trim().parse::<u64>().ok()
}

/// `MemAvailable:  12345 kB` → bytes.
#[cfg_attr(
    not(any(target_os = "linux", target_os = "android", test)),
    allow(dead_code)
)]
fn parse_meminfo(text: &str) -> Option<u64> {
    text.lines().find_map(|line| {
        let rest = line.strip_prefix("MemAvailable:")?;
        let mut parts = rest.split_whitespace();
        let n: u64 = parts.next()?.parse().ok()?;
        match parts.next() {
            Some("kB") | None => n.checked_mul(1024),
            _ => None,
        }
    })
}

#[cfg(target_os = "ios")]
fn platform_available_bytes() -> Option<u64> {
    extern "C" {
        // <os/proc.h>, iOS 13+, libSystem.
        fn os_proc_available_memory() -> usize;
    }
    // SAFETY: no arguments, no preconditions; returns a byte count.
    let n = unsafe { os_proc_available_memory() } as u64;
    (n > 0).then_some(n)
}

#[cfg(target_os = "macos")]
fn platform_available_bytes() -> Option<u64> {
    use std::ffi::{c_char, c_int, c_void};
    extern "C" {
        fn sysctlbyname(
            name: *const c_char,
            oldp: *mut c_void,
            oldlenp: *mut usize,
            newp: *mut c_void,
            newlen: usize,
        ) -> c_int;
    }
    let mut value: u64 = 0;
    let mut len = std::mem::size_of::<u64>();
    // SAFETY: NUL-terminated name; oldp points at a u64 and oldlenp holds
    // its size; no new value is written.
    let rc = unsafe {
        sysctlbyname(
            c"hw.memsize".as_ptr(),
            (&mut value as *mut u64).cast(),
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    (rc == 0 && len == std::mem::size_of::<u64>() && value > 0).then_some(value)
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn platform_available_bytes() -> Option<u64> {
    parse_meminfo(&std::fs::read_to_string("/proc/meminfo").ok()?)
}

#[cfg(windows)]
fn platform_available_bytes() -> Option<u64> {
    #[repr(C)]
    struct MemoryStatusEx {
        dw_length: u32,
        dw_memory_load: u32,
        ull_total_phys: u64,
        ull_avail_phys: u64,
        ull_total_page_file: u64,
        ull_avail_page_file: u64,
        ull_total_virtual: u64,
        ull_avail_virtual: u64,
        ull_avail_extended_virtual: u64,
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GlobalMemoryStatusEx(buffer: *mut MemoryStatusEx) -> i32;
    }
    let mut status = MemoryStatusEx {
        dw_length: std::mem::size_of::<MemoryStatusEx>() as u32,
        dw_memory_load: 0,
        ull_total_phys: 0,
        ull_avail_phys: 0,
        ull_total_page_file: 0,
        ull_avail_page_file: 0,
        ull_total_virtual: 0,
        ull_avail_virtual: 0,
        ull_avail_extended_virtual: 0,
    };
    // SAFETY: dw_length is set to the struct size as the API requires.
    let ok = unsafe { GlobalMemoryStatusEx(&mut status) };
    (ok != 0 && status.ull_total_phys > 0).then_some(status.ull_total_phys)
}

#[cfg(not(any(
    target_os = "ios",
    target_os = "macos",
    target_os = "linux",
    target_os = "android",
    windows
)))]
fn platform_available_bytes() -> Option<u64> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gguf_check::{check_bytes, tests::Builder};

    fn summary() -> GgufSummary {
        let bytes = Builder::minimal().build();
        check_bytes(&bytes, bytes.len() as u64).unwrap()
    }

    #[test]
    fn kv_estimate_uses_derived_head_dim() {
        // minimal(): 2 layers, 4 heads, 2 kv heads, embd 64 → head dim 16.
        let s = summary();
        assert_eq!(estimate_kv_bytes(&s, 1000), 1000 * 2 * 2 * (16 + 16) * 2);
    }

    #[test]
    fn kv_estimate_prefers_explicit_dims_and_defaults_kv_heads() {
        let mut s = summary();
        s.key_length = Some(128);
        s.value_length = Some(64);
        s.head_count_kv = None; // → head_count (4)
        assert_eq!(estimate_kv_bytes(&s, 10), 10 * 2 * 4 * (128 + 64) * 2);
    }

    #[test]
    fn kv_estimate_is_zero_without_attention() {
        let mut s = summary();
        s.head_count = None;
        s.head_count_kv = None;
        assert_eq!(estimate_kv_bytes(&s, 4096), 0);
        let mut s = summary();
        s.block_count = None;
        assert_eq!(estimate_kv_bytes(&s, 4096), 0);
    }

    #[test]
    fn load_estimate_sums_weights_kv_and_margin() {
        let s = summary();
        assert_eq!(
            estimate_load_bytes(&s, 512),
            s.tensor_bytes + estimate_kv_bytes(&s, 512) + COMPUTE_MARGIN_BYTES
        );
    }

    #[test]
    fn estimate_saturates_instead_of_overflowing() {
        let mut s = summary();
        s.tensor_bytes = u64::MAX - 1;
        s.block_count = Some(u64::MAX);
        assert_eq!(estimate_load_bytes(&s, u32::MAX), u64::MAX);
    }

    #[test]
    fn refuses_when_estimate_exceeds_available() {
        assert_eq!(
            check_budget_against(2 << 30, Some(1 << 30)),
            Err(BudgetError::Insufficient {
                required: 2 << 30,
                available: 1 << 30
            })
        );
        let msg = check_budget_against(2 << 30, Some(1 << 30))
            .unwrap_err()
            .to_string();
        assert!(
            msg.contains("2048 MiB") && msg.contains("1024 MiB"),
            "{msg}"
        );
    }

    #[test]
    fn allows_when_it_fits_exactly_or_availability_unknown() {
        assert_eq!(check_budget_against(1 << 30, Some(1 << 30)), Ok(()));
        assert_eq!(check_budget_against(u64::MAX, None), Ok(()));
    }

    #[test]
    fn refusal_end_to_end_from_a_gguf_summary() {
        let s = summary();
        let need = estimate_load_bytes(&s, 2048);
        assert!(check_budget_against(need, Some(need - 1)).is_err());
        assert!(check_budget_against(need, Some(need)).is_ok());
    }

    #[test]
    fn override_parsing() {
        assert_eq!(parse_override(" 1048576\n"), Some(1 << 20));
        assert_eq!(parse_override("lots"), None);
        assert_eq!(parse_override("-1"), None);
    }

    #[test]
    fn meminfo_parsing() {
        let text = "MemTotal:       16303428 kB\nMemFree:  1 kB\nMemAvailable:    8151714 kB\n";
        assert_eq!(parse_meminfo(text), Some(8151714 * 1024));
        assert_eq!(parse_meminfo("MemTotal: 5 kB\n"), None);
        assert_eq!(parse_meminfo("MemAvailable: x kB\n"), None);
        assert_eq!(parse_meminfo("MemAvailable: 1 MB\n"), None);
    }

    #[cfg(any(target_os = "macos", target_os = "linux", windows))]
    #[test]
    fn desktop_platform_reports_memory() {
        let n = platform_available_bytes().expect("platform memory query");
        assert!(n >= 256 << 20, "implausible memory size {n}");
    }
}
