//! Runtime resilience, phase 1 of decision 0022: crash sentinels and the
//! per-device qualification store.
//!
//! A GPU backend can crash the process (a driver segfault, a llama.cpp
//! assert) or compute wrongly without failing (decisions 0015/0016). An
//! in-process check cannot observe a crash, but a file written before the
//! risky step and removed after it can: if the file is still there when the
//! next process starts, the step never finished.
//!
//! Each (backend, device, platform, model, runtime) combination is a
//! [`DeviceKey`]. Its [`QualState`] lives in `qualification.json` under the
//! runtime state directory, and decides whether a load may use the GPU:
//!
//! - **init sentinel**: armed around the GPU model load (shader compilation,
//!   pipeline creation, weight upload). A stale one means the load crashed:
//!   the key becomes `crashed` at once.
//! - **first-work sentinel**: armed around the first GPU workload of a key
//!   still `verifying`. A stale one also pins the key at once — that window
//!   is where a broken kernel crashes.
//! - **work sentinel**: armed around every later GPU workload. One stale
//!   sentinel is not enough evidence (the OS kills apps mid-generation for
//!   other reasons); two consecutive ones pin the key.
//!
//! A pinned key (`crashed`, `canary_failed`) loads on the true CPU path.
//! A key whose `platform` or `runtime` changes is a new key, so a driver or
//! llama.cpp update re-tests the GPU once. [`RuntimeState::reset`] clears a
//! key on request (diagnostics). Sentinels carry the owning process id, so a
//! sentinel belonging to a live process (a worker, decision 0022 phase 3) is
//! never mistaken for a crash.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use serde::{Deserialize, Serialize};

/// Consecutive interrupted workloads that pin a qualified key.
pub const WORK_CRASH_THRESHOLD: u32 = 2;

/// The runtime state directory for a models root. It lives inside the
/// root (the installer lists only directories holding a package manifest),
/// so the qualification record travels with the models it describes.
pub fn runtime_dir_for(installed_root: &Path) -> PathBuf {
    installed_root.join(".runtime")
}

/// One GPU execution combination.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct DeviceKey {
    /// ggml backend name ("Metal", "Vulkan", ...).
    pub backend: String,
    /// Device description as ggml reports it ("Apple M3 Max", "Adreno (TM) 829").
    pub device: String,
    /// OS build (and on Android the build fingerprint, which pins the GPU
    /// driver shipped with it); see [`platform_fingerprint`].
    pub platform: String,
    /// sha256 of the weights file (from the installed package manifest).
    pub model_sha256: String,
    /// The linked llama.cpp revision.
    pub runtime: String,
}

impl DeviceKey {
    /// Stable short id used for file names (FNV-1a 64 over the fields).
    pub fn id(&self) -> String {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for field in [
            &self.backend,
            &self.device,
            &self.platform,
            &self.model_sha256,
            &self.runtime,
        ] {
            for b in field.as_bytes().iter().chain(std::iter::once(&0u8)) {
                h ^= *b as u64;
                h = h.wrapping_mul(0x0100_0000_01b3);
            }
        }
        format!("{h:016x}")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QualState {
    /// Loaded on the GPU; no workload has completed yet.
    Verifying,
    /// At least one GPU workload completed; no numerics canary has run.
    Ok,
    /// A numerics canary passed on this key.
    Qualified,
    /// A numerics canary failed: pinned to CPU.
    CanaryFailed,
    /// The process died during GPU init, the first workload, or
    /// [`WORK_CRASH_THRESHOLD`] consecutive workloads: pinned to CPU.
    Crashed,
}

impl QualState {
    pub fn as_str(&self) -> &'static str {
        match self {
            QualState::Verifying => "verifying",
            QualState::Ok => "ok",
            QualState::Qualified => "qualified",
            QualState::CanaryFailed => "canary_failed",
            QualState::Crashed => "crashed",
        }
    }

    /// Whether loads for this key must use the CPU.
    pub fn pins_cpu(&self) -> bool {
        matches!(self, QualState::CanaryFailed | QualState::Crashed)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QualRecord {
    pub key: DeviceKey,
    pub state: QualState,
    /// Consecutive interrupted workloads (reset by a completed one).
    pub interrupted: u32,
    /// What last changed the state (shown in diagnostics).
    pub note: String,
    /// RFC 3339 time of the last change.
    pub updated: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct StoreFile {
    version: u32,
    records: BTreeMap<String, QualRecord>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SentinelKind {
    Init,
    First,
    Work,
}

impl SentinelKind {
    fn as_str(&self) -> &'static str {
        match self {
            SentinelKind::Init => "init",
            SentinelKind::First => "first",
            SentinelKind::Work => "work",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s {
            "init" => Some(SentinelKind::Init),
            "first" => Some(SentinelKind::First),
            "work" => Some(SentinelKind::Work),
            _ => None,
        }
    }
}

/// The qualification store and sentinel directory for one data root.
/// Shared per directory within a process (see [`RuntimeState::open`]).
pub struct RuntimeState {
    dir: PathBuf,
    store: Mutex<StoreFile>,
    seq: AtomicU64,
    /// Crashes found by recovery when this state was opened.
    recovered: Vec<QualRecord>,
}

impl std::fmt::Debug for RuntimeState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RuntimeState")
            .field("dir", &self.dir)
            .finish()
    }
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

impl RuntimeState {
    /// Open (once per process and directory) the runtime state under `dir`,
    /// recovering sentinels left by dead processes.
    pub fn open(dir: impl Into<PathBuf>) -> Arc<RuntimeState> {
        static REGISTRY: OnceLock<Mutex<BTreeMap<PathBuf, Arc<RuntimeState>>>> = OnceLock::new();
        let dir: PathBuf = dir.into();
        let canonical = std::fs::create_dir_all(&dir)
            .ok()
            .and_then(|_| dir.canonicalize().ok())
            .unwrap_or_else(|| dir.clone());
        let mut reg = REGISTRY
            .get_or_init(|| Mutex::new(BTreeMap::new()))
            .lock()
            .unwrap();
        reg.entry(canonical.clone())
            .or_insert_with(|| Arc::new(RuntimeState::open_uncached(canonical)))
            .clone()
    }

    /// Open without the per-process registry (tests simulate a new process).
    pub fn open_uncached(dir: PathBuf) -> RuntimeState {
        let _ = std::fs::create_dir_all(dir.join("sentinels"));
        let store = std::fs::read(dir.join("qualification.json"))
            .ok()
            .and_then(|b| serde_json::from_slice::<StoreFile>(&b).ok())
            .unwrap_or(StoreFile {
                version: 1,
                records: BTreeMap::new(),
            });
        let mut state = RuntimeState {
            dir,
            store: Mutex::new(store),
            seq: AtomicU64::new(0),
            recovered: Vec::new(),
        };
        state.recovered = state.recover(true);
        state
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Records whose state changed because recovery found a dead process's
    /// sentinel when this state was opened.
    pub fn recovered(&self) -> &[QualRecord] {
        &self.recovered
    }

    /// Every record, for diagnostics.
    pub fn records(&self) -> Vec<QualRecord> {
        let mut store = self.store.lock().unwrap();
        self.reload(&mut store);
        store.records.values().cloned().collect()
    }

    pub fn state(&self, key: &DeviceKey) -> Option<QualState> {
        let mut store = self.store.lock().unwrap();
        self.reload(&mut store);
        store.records.get(&key.id()).map(|r| r.state)
    }

    /// Re-read the file: a worker process (phase 3) writes the same store.
    /// A missing or unreadable file keeps the cached copy.
    fn reload(&self, store: &mut StoreFile) {
        if let Some(disk) = std::fs::read(self.dir.join("qualification.json"))
            .ok()
            .and_then(|b| serde_json::from_slice::<StoreFile>(&b).ok())
        {
            *store = disk;
        }
    }

    /// Scan for sentinels left by processes that have died since this
    /// state was opened (a supervisor calls it when its worker exits).
    /// Returns the records that changed.
    pub fn recover_now(&self) -> Vec<QualRecord> {
        self.recover(false)
    }

    /// Delete the sentinels of `pid` without counting them: the process was
    /// stopped from outside (the OS reclaiming memory, or its supervisor),
    /// which says nothing about the GPU.
    pub fn discard_sentinels_of(&self, pid: u32) -> usize {
        let Ok(entries) = std::fs::read_dir(self.dir.join("sentinels")) else {
            return 0;
        };
        let mut n = 0;
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.split('.').nth(1).and_then(|p| p.parse::<u32>().ok()) == Some(pid)
                && std::fs::remove_file(entry.path()).is_ok()
            {
                n += 1;
            }
        }
        n
    }

    /// Whether `key` must load on the CPU.
    pub fn pins_cpu(&self, key: &DeviceKey) -> bool {
        self.state(key).is_some_and(|s| s.pins_cpu())
    }

    /// Record a numerics canary result for `key`.
    pub fn record_canary(&self, key: &DeviceKey, passed: bool) {
        let (state, note) = if passed {
            (QualState::Qualified, "numerics canary passed")
        } else {
            (QualState::CanaryFailed, "numerics canary failed")
        };
        self.update(key, |r| {
            // A canary pass never clears a crash pin; only reset does.
            if r.state != QualState::Crashed {
                r.state = state;
                r.note = note.into();
            }
        });
    }

    /// Forget `key` (or every key when `None`): the next load re-tests the GPU.
    pub fn reset(&self, key_id: Option<&str>) -> usize {
        let mut store = self.store.lock().unwrap();
        self.reload(&mut store);
        let removed = match key_id {
            Some(id) => store.records.remove(id).map_or(0, |_| 1),
            None => {
                let n = store.records.len();
                store.records.clear();
                n
            }
        };
        let _ = self.persist(&store);
        removed
    }

    /// Arm the init sentinel for a GPU load of `key`. Drop the guard once
    /// the load returned (successfully or with a typed error).
    pub fn arm_load(self: &Arc<Self>, key: &DeviceKey) -> SentinelGuard {
        self.update(key, |r| {
            if !r.state.pins_cpu() && r.state != QualState::Qualified && r.state != QualState::Ok {
                r.state = QualState::Verifying;
                r.note = "loading on the GPU".into();
            }
        });
        self.arm(key, SentinelKind::Init)
    }

    /// Arm the sentinel for one GPU workload of `key`.
    pub fn arm_work(self: &Arc<Self>, key: &DeviceKey) -> SentinelGuard {
        let kind = match self.state(key) {
            Some(QualState::Verifying) | None => SentinelKind::First,
            _ => SentinelKind::Work,
        };
        self.arm(key, kind)
    }

    fn arm(self: &Arc<Self>, key: &DeviceKey, kind: SentinelKind) -> SentinelGuard {
        let name = format!(
            "{}.{}.{}.{}",
            key.id(),
            std::process::id(),
            kind.as_str(),
            self.seq.fetch_add(1, Ordering::Relaxed)
        );
        let path = self.dir.join("sentinels").join(name);
        // The sentinel must reach the disk before the risky step: a crash
        // right after a buffered write would leave nothing behind.
        let written = std::fs::File::create(&path).and_then(|mut f| {
            use std::io::Write as _;
            f.write_all(&serde_json::to_vec(key).unwrap_or_default())?;
            f.sync_all()
        });
        if let Ok(()) = written {
            if let Ok(d) = std::fs::File::open(self.dir.join("sentinels")) {
                let _ = d.sync_all();
            }
        }
        SentinelGuard {
            state: self.clone(),
            key: key.clone(),
            kind,
            path,
        }
    }

    fn complete(&self, key: &DeviceKey, kind: SentinelKind) {
        match kind {
            SentinelKind::Init => {}
            SentinelKind::First | SentinelKind::Work => self.update(key, |r| {
                r.interrupted = 0;
                if r.state == QualState::Verifying {
                    r.state = QualState::Ok;
                    r.note = "first GPU workload completed".into();
                }
            }),
        }
    }

    fn update(&self, key: &DeviceKey, f: impl FnOnce(&mut QualRecord)) {
        let mut store = self.store.lock().unwrap();
        self.reload(&mut store);
        let rec = store.records.entry(key.id()).or_insert_with(|| QualRecord {
            key: key.clone(),
            state: QualState::Verifying,
            interrupted: 0,
            note: String::new(),
            updated: now(),
        });
        let before = rec.clone();
        f(rec);
        if *rec != before {
            rec.updated = now();
            let _ = self.persist(&store);
        }
    }

    fn persist(&self, store: &StoreFile) -> std::io::Result<()> {
        let path = self.dir.join("qualification.json");
        let tmp = self
            .dir
            .join(format!("qualification.json.{}.tmp", std::process::id()));
        {
            use std::io::Write as _;
            let mut f = std::fs::File::create(&tmp)?;
            f.write_all(&serde_json::to_vec_pretty(store).map_err(std::io::Error::other)?)?;
            f.sync_all()?;
        }
        std::fs::rename(&tmp, &path)
    }

    /// Turn sentinels left by dead processes into crash records.
    fn recover(&self, at_open: bool) -> Vec<QualRecord> {
        let Ok(entries) = std::fs::read_dir(self.dir.join("sentinels")) else {
            return Vec::new();
        };
        let me = std::process::id();
        let mut changed = BTreeMap::new();
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            let parts: Vec<&str> = name.split('.').collect();
            let (Some(pid), Some(kind)) = (
                parts.get(1).and_then(|p| p.parse::<u32>().ok()),
                parts.get(2).and_then(|k| SentinelKind::parse(k)),
            ) else {
                let _ = std::fs::remove_file(&path);
                continue;
            };
            // At open, our own pid can only be a stale file from an earlier
            // process that had the same id (nothing is armed yet); on later
            // scans our own sentinels are live.
            if (pid == me && !at_open) || (pid != me && process_alive(pid)) {
                continue;
            }
            let key: Option<DeviceKey> = std::fs::read(&path)
                .ok()
                .and_then(|b| serde_json::from_slice(&b).ok());
            let _ = std::fs::remove_file(&path);
            // An empty file is a crash between create and write: we cannot
            // name the key, so there is nothing to pin.
            let Some(key) = key else { continue };
            self.update(&key, |r| match kind {
                SentinelKind::Init => {
                    r.state = QualState::Crashed;
                    r.note = "process died while loading on the GPU".into();
                }
                SentinelKind::First => {
                    r.state = QualState::Crashed;
                    r.note = "process died during the first GPU workload".into();
                }
                SentinelKind::Work => {
                    r.interrupted += 1;
                    if r.interrupted >= WORK_CRASH_THRESHOLD && !r.state.pins_cpu() {
                        r.state = QualState::Crashed;
                        r.note = format!(
                            "process died during {} consecutive GPU workloads",
                            r.interrupted
                        );
                    }
                }
            });
            let rec = self.store.lock().unwrap().records.get(&key.id()).cloned();
            if let Some(r) = rec {
                changed.insert(key.id(), r);
            }
        }
        changed.into_values().collect()
    }
}

/// Removes its sentinel on drop. A panic unwinding through the guard is not
/// a crash of the process, so the sentinel is removed then too.
pub struct SentinelGuard {
    state: Arc<RuntimeState>,
    key: DeviceKey,
    kind: SentinelKind,
    path: PathBuf,
}

impl Drop for SentinelGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
        self.state.complete(&self.key, self.kind);
    }
}

/// A key bound to its state, carried by engines that run GPU work outside
/// the provider (media embedder, vision OCR).
#[derive(Clone, Debug)]
pub struct WorkTicket {
    pub state: Arc<RuntimeState>,
    pub key: DeviceKey,
}

impl WorkTicket {
    pub fn arm(&self) -> SentinelGuard {
        self.state.arm_work(&self.key)
    }
}

/// OS build identity. On Apple platforms the GPU driver ships with the OS,
/// so the OS build pins it; on Android the build fingerprint pins the
/// vendor image that carries the GPU driver. On Linux and Windows the GPU
/// driver updates independently and is NOT captured here: after a driver
/// update, reset the key from diagnostics to re-test.
pub fn platform_fingerprint() -> String {
    let arch = std::env::consts::ARCH;
    let os = std::env::consts::OS;
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        let build = apple_sysctl_string("kern.osversion").unwrap_or_default();
        let product = apple_sysctl_string("kern.osproductversion").unwrap_or_default();
        let sim = if cfg!(target_abi = "sim") { "-sim" } else { "" };
        format!("{os}{sim}-{arch}/{product}/{build}")
    }
    #[cfg(target_os = "android")]
    {
        format!(
            "{os}-{arch}/{}",
            android_property("ro.build.fingerprint").unwrap_or_default()
        )
    }
    #[cfg(target_os = "linux")]
    {
        let release = std::fs::read_to_string("/proc/sys/kernel/osrelease").unwrap_or_default();
        format!("{os}-{arch}/{}", release.trim())
    }
    #[cfg(not(any(
        target_os = "macos",
        target_os = "ios",
        target_os = "android",
        target_os = "linux"
    )))]
    {
        format!("{os}-{arch}")
    }
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
fn apple_sysctl_string(name: &str) -> Option<String> {
    use std::ffi::{c_char, c_int, c_void, CString};
    extern "C" {
        fn sysctlbyname(
            name: *const c_char,
            oldp: *mut c_void,
            oldlenp: *mut usize,
            newp: *mut c_void,
            newlen: usize,
        ) -> c_int;
    }
    let cname = CString::new(name).ok()?;
    let mut len: usize = 0;
    // SAFETY: a size query (null buffer) followed by a read into a buffer of
    // exactly that size; both calls follow sysctlbyname(3).
    unsafe {
        if sysctlbyname(
            cname.as_ptr(),
            std::ptr::null_mut(),
            &mut len,
            std::ptr::null_mut(),
            0,
        ) != 0
            || len == 0
        {
            return None;
        }
        let mut buf = vec![0u8; len];
        if sysctlbyname(
            cname.as_ptr(),
            buf.as_mut_ptr() as *mut c_void,
            &mut len,
            std::ptr::null_mut(),
            0,
        ) != 0
        {
            return None;
        }
        buf.truncate(len);
        Some(
            String::from_utf8_lossy(&buf)
                .trim_end_matches('\0')
                .to_string(),
        )
    }
}

#[cfg(target_os = "android")]
fn android_property(name: &str) -> Option<String> {
    use std::ffi::{c_char, c_int, CString};
    extern "C" {
        fn __system_property_get(name: *const c_char, value: *mut c_char) -> c_int;
    }
    let cname = CString::new(name).ok()?;
    // PROP_VALUE_MAX is 92 bytes including the terminator.
    let mut buf = [0 as c_char; 92];
    // SAFETY: bionic writes at most PROP_VALUE_MAX bytes into `buf`.
    let n = unsafe { __system_property_get(cname.as_ptr(), buf.as_mut_ptr()) };
    if n <= 0 {
        return None;
    }
    let bytes: Vec<u8> = buf[..n as usize].iter().map(|c| *c as u8).collect();
    Some(String::from_utf8_lossy(&bytes).to_string())
}

/// Whether a process with this id is running.
pub fn process_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        extern "C" {
            fn kill(pid: i32, sig: i32) -> i32;
        }
        let Ok(pid) = i32::try_from(pid) else {
            return false;
        };
        // SAFETY: signal 0 performs only the existence/permission check.
        if unsafe { kill(pid, 0) } == 0 {
            return true;
        }
        // EPERM: exists, owned by someone else. ESRCH: gone.
        std::io::Error::last_os_error().raw_os_error() == Some(1)
    }
    #[cfg(windows)]
    {
        use std::ffi::c_void;
        extern "system" {
            fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut c_void;
            fn GetExitCodeProcess(h: *mut c_void, code: *mut u32) -> i32;
            fn CloseHandle(h: *mut c_void) -> i32;
        }
        const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
        const STILL_ACTIVE: u32 = 259;
        // SAFETY: documented Win32 calls; the handle is closed before return.
        unsafe {
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if h.is_null() {
                return false;
            }
            let mut code = 0u32;
            let ok = GetExitCodeProcess(h, &mut code) != 0;
            CloseHandle(h);
            ok && code == STILL_ACTIVE
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = pid;
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(device: &str) -> DeviceKey {
        DeviceKey {
            backend: "Metal".into(),
            device: device.into(),
            platform: "test-os/1".into(),
            model_sha256: "ab".repeat(32),
            runtime: "llama.cpp/test".into(),
        }
    }

    /// Simulate "the process died here": leave the guard's file behind and
    /// rename it to a pid that is certainly not running.
    fn crash(guard: SentinelGuard) {
        let path = guard.path.clone();
        std::mem::forget(guard);
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let dead = name.replacen(&format!(".{}.", std::process::id()), ".999999999.", 1);
        std::fs::rename(&path, path.with_file_name(dead)).unwrap();
    }

    fn fresh(dir: &Path) -> Arc<RuntimeState> {
        Arc::new(RuntimeState::open_uncached(dir.to_path_buf()))
    }

    #[test]
    fn completed_load_and_work_reach_ok_and_leave_no_sentinel() {
        let d = tempfile::tempdir().unwrap();
        let s = fresh(d.path());
        let k = key("gpu");
        drop(s.arm_load(&k));
        assert_eq!(s.state(&k), Some(QualState::Verifying));
        drop(s.arm_work(&k));
        assert_eq!(s.state(&k), Some(QualState::Ok));
        assert_eq!(
            std::fs::read_dir(d.path().join("sentinels"))
                .unwrap()
                .count(),
            0
        );
        let again = fresh(d.path());
        assert_eq!(again.state(&k), Some(QualState::Ok));
        assert!(again.recovered().is_empty());
    }

    #[test]
    fn crash_during_load_pins_cpu_on_next_open() {
        let d = tempfile::tempdir().unwrap();
        let s = fresh(d.path());
        let k = key("gpu");
        crash(s.arm_load(&k));
        let next = fresh(d.path());
        assert_eq!(next.state(&k), Some(QualState::Crashed));
        assert!(next.pins_cpu(&k));
        assert_eq!(next.recovered().len(), 1);
        assert_eq!(
            std::fs::read_dir(d.path().join("sentinels"))
                .unwrap()
                .count(),
            0
        );
    }

    #[test]
    fn crash_during_first_workload_pins_cpu() {
        let d = tempfile::tempdir().unwrap();
        let s = fresh(d.path());
        let k = key("gpu");
        drop(s.arm_load(&k));
        crash(s.arm_work(&k));
        assert!(fresh(d.path()).pins_cpu(&k));
    }

    #[test]
    fn one_interrupted_workload_is_tolerated_two_pin() {
        let d = tempfile::tempdir().unwrap();
        let k = key("gpu");
        let s = fresh(d.path());
        drop(s.arm_load(&k));
        drop(s.arm_work(&k)); // ok
        crash(s.arm_work(&k));
        let s = fresh(d.path());
        assert_eq!(s.state(&k), Some(QualState::Ok));
        crash(s.arm_work(&k));
        let s = fresh(d.path());
        assert_eq!(s.state(&k), Some(QualState::Crashed));
    }

    #[test]
    fn a_completed_workload_resets_the_interrupted_count() {
        let d = tempfile::tempdir().unwrap();
        let k = key("gpu");
        let s = fresh(d.path());
        drop(s.arm_load(&k));
        drop(s.arm_work(&k));
        crash(s.arm_work(&k));
        let s = fresh(d.path());
        drop(s.arm_work(&k));
        crash(s.arm_work(&k));
        assert_eq!(fresh(d.path()).state(&k), Some(QualState::Ok));
    }

    #[cfg(unix)]
    #[test]
    fn live_process_sentinels_are_left_alone() {
        let d = tempfile::tempdir().unwrap();
        let s = fresh(d.path());
        let k = key("gpu");
        let g = s.arm_load(&k);
        // Same pid as a live process: rename to the parent's pid, which is alive.
        let name = g.path.file_name().unwrap().to_string_lossy().to_string();
        let parent = std::os::unix::process::parent_id();
        let moved = g.path.with_file_name(name.replacen(
            &format!(".{}.", std::process::id()),
            &format!(".{parent}."),
            1,
        ));
        std::fs::rename(&g.path, &moved).unwrap();
        std::mem::forget(g);
        let next = fresh(d.path());
        assert_eq!(next.state(&k), Some(QualState::Verifying));
        assert!(moved.exists());
    }

    /// The child half of `a_real_abort_is_recorded`: arm, then abort.
    #[test]
    fn abort_child() {
        let Ok(dir) = std::env::var("HARBOR_SENTINEL_CHILD_DIR") else {
            return;
        };
        let s = RuntimeState::open(PathBuf::from(dir));
        let _g = s.arm_load(&key("gpu"));
        std::process::abort();
    }

    #[test]
    fn a_real_abort_is_recorded() {
        let d = tempfile::tempdir().unwrap();
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "resilience::tests::abort_child",
                "--test-threads=1",
            ])
            .env("HARBOR_SENTINEL_CHILD_DIR", d.path())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(!status.success(), "child must die");
        let next = fresh(d.path());
        assert_eq!(next.state(&key("gpu")), Some(QualState::Crashed));
        assert_eq!(next.recovered().len(), 1);
    }

    #[test]
    fn external_kills_are_discarded_not_counted() {
        let d = tempfile::tempdir().unwrap();
        let s = fresh(d.path());
        let k = key("gpu");
        drop(s.arm_load(&k));
        drop(s.arm_work(&k));
        crash(s.arm_work(&k));
        assert_eq!(s.discard_sentinels_of(999_999_999), 1);
        assert_eq!(fresh(d.path()).state(&k), Some(QualState::Ok));
    }

    #[test]
    fn recover_now_sees_a_later_death_and_another_view_reads_it() {
        let d = tempfile::tempdir().unwrap();
        let s = fresh(d.path());
        let other = fresh(d.path());
        let k = key("gpu");
        let live = s.arm_work(&key("live"));
        crash(other.arm_load(&k));
        let changed = s.recover_now();
        assert_eq!(changed.len(), 1);
        assert!(s.pins_cpu(&k));
        // `other` (another process's view) reads it from disk.
        assert!(other.pins_cpu(&k));
        // This process's live sentinel survives the scan.
        assert!(live.path.exists());
    }

    #[test]
    fn canary_results_and_reset() {
        let d = tempfile::tempdir().unwrap();
        let s = fresh(d.path());
        let (a, b) = (key("a"), key("b"));
        s.record_canary(&a, true);
        s.record_canary(&b, false);
        assert_eq!(s.state(&a), Some(QualState::Qualified));
        assert!(s.pins_cpu(&b));
        assert_eq!(s.reset(Some(&b.id())), 1);
        assert_eq!(s.state(&b), None);
        assert_eq!(s.reset(None), 1);
        assert!(fresh(d.path()).records().is_empty());
    }

    #[test]
    fn canary_pass_never_clears_a_crash() {
        let d = tempfile::tempdir().unwrap();
        let s = fresh(d.path());
        let k = key("gpu");
        crash(s.arm_load(&k));
        let s = fresh(d.path());
        s.record_canary(&k, true);
        assert_eq!(s.state(&k), Some(QualState::Crashed));
    }

    #[test]
    fn a_new_runtime_or_platform_is_a_new_key() {
        let k = key("gpu");
        let mut k2 = k.clone();
        k2.runtime = "llama.cpp/next".into();
        let mut k3 = k.clone();
        k3.platform = "test-os/2".into();
        assert_ne!(k.id(), k2.id());
        assert_ne!(k.id(), k3.id());
        assert_eq!(k.id(), key("gpu").id());
    }

    #[test]
    fn corrupt_store_and_junk_sentinels_do_not_break_open() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(d.path().join("sentinels")).unwrap();
        std::fs::write(d.path().join("qualification.json"), b"{not json").unwrap();
        std::fs::write(d.path().join("sentinels").join("junk"), b"").unwrap();
        std::fs::write(d.path().join("sentinels").join("x.999999999.init.0"), b"").unwrap();
        let s = fresh(d.path());
        assert!(s.records().is_empty());
        assert_eq!(
            std::fs::read_dir(d.path().join("sentinels"))
                .unwrap()
                .count(),
            0
        );
    }

    #[test]
    fn registry_shares_state_per_directory() {
        let d = tempfile::tempdir().unwrap();
        let a = RuntimeState::open(d.path());
        let b = RuntimeState::open(d.path().join("."));
        assert!(Arc::ptr_eq(&a, &b));
    }

    #[test]
    fn platform_fingerprint_names_the_os() {
        let p = platform_fingerprint();
        assert!(p.starts_with(std::env::consts::OS), "{p}");
        #[cfg(target_os = "macos")]
        assert!(p.split('/').count() == 3 && !p.ends_with('/'), "{p}");
    }
}
