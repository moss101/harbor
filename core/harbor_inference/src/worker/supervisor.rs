//! Supervisor side of the inference worker (decision 0022 phase 3).
//!
//! [`WorkerProvider`] implements the provider contract by forwarding every
//! call to a child process that owns llama.cpp. When the child dies the
//! pending call returns [`ProviderError::RuntimeCrashed`] instead of the
//! whole app dying, and the next call starts a fresh child.
//!
//! How the death is read:
//! - killed by SIGKILL / SIGTERM, or a clean exit: stopped from outside
//!   (low-memory killer, Android's phantom-process limit, the supervisor
//!   itself). Its sentinels are discarded — that says nothing about the GPU.
//! - anything else (SIGSEGV, SIGABRT, SIGBUS, a Windows exception code):
//!   a crash. Its sentinels are recovered into the qualification store
//!   (phase 1), which may pin the key to the CPU.
//!
//! A call that died is retried once, and only when the recovery pinned a
//! key to the CPU, so the retry runs on a different path than the one that
//! crashed. Otherwise the crash is returned to the caller.
//!
//! The child is the `harbor_worker` launcher, which loads the same core
//! library the app uses and calls its `harbor_worker_main`: one copy of
//! llama.cpp on disk, one code path in both processes. The child exits when
//! its stdin closes, so it never outlives the app.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use harbor_modelhub::install::PackageInstaller;

use super::protocol::{self, error_from_json, Frame};
use crate::provider::{
    Capabilities, ChatRequest, ChatResponse, ModelProvider, ModelRef, ProviderError, Usage,
};
use crate::resilience::{runtime_dir_for, RuntimeState};

/// Whether model work runs in a worker process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerMode {
    /// Always in-process (iOS, where an app cannot spawn processes).
    Off,
    /// In a worker when the launcher is present, else in-process.
    Auto,
    /// In a worker; a missing launcher is an error.
    Required,
}

impl WorkerMode {
    /// From `HARBOR_INFERENCE_WORKER` (`0`/`off`, `1`/`on`/`required`;
    /// anything else is auto). iOS is always off.
    pub fn from_env() -> WorkerMode {
        if cfg!(target_os = "ios") {
            return WorkerMode::Off;
        }
        match std::env::var("HARBOR_INFERENCE_WORKER").as_deref() {
            Ok("0") | Ok("off") => WorkerMode::Off,
            Ok("1") | Ok("on") | Ok("required") => WorkerMode::Required,
            _ => WorkerMode::Auto,
        }
    }
}

/// Where the launcher and the core library are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerConfig {
    pub launcher: PathBuf,
    pub library: PathBuf,
}

/// The launcher's file name on this platform. On Android it is packaged
/// as a "library" so the installer extracts it into the executable native
/// library directory.
pub fn launcher_name() -> &'static str {
    if cfg!(target_os = "android") {
        "libharbor_worker.so"
    } else if cfg!(windows) {
        "harbor_worker.exe"
    } else {
        "harbor_worker"
    }
}

impl WorkerConfig {
    /// `HARBOR_WORKER_PATH` / `HARBOR_WORKER_LIB`, else the launcher next to
    /// the loaded core library (or in `../Helpers`, `../MacOS` beside a
    /// macOS `Frameworks` directory, or next to the executable).
    pub fn discover() -> Option<WorkerConfig> {
        let library = std::env::var_os("HARBOR_WORKER_LIB")
            .map(PathBuf::from)
            .or_else(this_library_path)?;
        let launcher = match std::env::var_os("HARBOR_WORKER_PATH") {
            Some(p) => PathBuf::from(p),
            None => {
                let name = launcher_name();
                let lib_dir = library.parent()?.to_path_buf();
                let mut candidates = vec![
                    lib_dir.join(name),
                    lib_dir.join("../Helpers").join(name),
                    lib_dir.join("../MacOS").join(name),
                ];
                if let Some(exe_dir) = std::env::current_exe()
                    .ok()
                    .and_then(|e| e.parent().map(Path::to_path_buf))
                {
                    candidates.push(exe_dir.join(name));
                }
                candidates.into_iter().find(|p| p.is_file())?
            }
        };
        (launcher.is_file() && library.is_file()).then_some(WorkerConfig { launcher, library })
    }
}

/// The file this code was loaded from (the core dylib / .so / .dll). In a
/// statically linked test binary this is the executable, which does not
/// export `harbor_worker_main`; tests set `HARBOR_WORKER_LIB`.
fn this_library_path() -> Option<PathBuf> {
    #[cfg(unix)]
    {
        use std::ffi::{c_char, c_int, c_void, CStr};
        #[repr(C)]
        struct DlInfo {
            dli_fname: *const c_char,
            dli_fbase: *mut c_void,
            dli_sname: *const c_char,
            dli_saddr: *mut c_void,
        }
        extern "C" {
            fn dladdr(addr: *const c_void, info: *mut DlInfo) -> c_int;
        }
        let mut info = DlInfo {
            dli_fname: std::ptr::null(),
            dli_fbase: std::ptr::null_mut(),
            dli_sname: std::ptr::null(),
            dli_saddr: std::ptr::null_mut(),
        };
        // SAFETY: dladdr only reads the loader's tables for this address.
        let ok = unsafe { dladdr(this_library_path as *const c_void, &mut info) };
        if ok == 0 || info.dli_fname.is_null() {
            return None;
        }
        // SAFETY: dli_fname is a NUL-terminated path owned by the loader.
        let name = unsafe { CStr::from_ptr(info.dli_fname) };
        Some(PathBuf::from(name.to_string_lossy().to_string()))
    }
    #[cfg(windows)]
    {
        use std::ffi::c_void;
        use std::os::windows::ffi::OsStringExt;
        extern "system" {
            fn GetModuleHandleExW(flags: u32, name: *const u16, module: *mut *mut c_void) -> i32;
            fn GetModuleFileNameW(module: *mut c_void, buf: *mut u16, size: u32) -> u32;
        }
        const FROM_ADDRESS: u32 = 0x4;
        const UNCHANGED_REFCOUNT: u32 = 0x2;
        let mut module = std::ptr::null_mut();
        // SAFETY: documented Win32 calls on an address inside this module.
        unsafe {
            if GetModuleHandleExW(
                FROM_ADDRESS | UNCHANGED_REFCOUNT,
                this_library_path as *const u16,
                &mut module,
            ) == 0
            {
                return None;
            }
            let mut buf = vec![0u16; 32768];
            let n = GetModuleFileNameW(module, buf.as_mut_ptr(), buf.len() as u32);
            if n == 0 {
                return None;
            }
            Some(PathBuf::from(std::ffi::OsString::from_wide(
                &buf[..n as usize],
            )))
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        None
    }
}

struct Conn {
    child: Mutex<Child>,
    stdin: Mutex<ChildStdin>,
    pending: Arc<Mutex<HashMap<u64, mpsc::Sender<Frame>>>>,
    next_id: AtomicU64,
    pid: u32,
    loaded: Mutex<BTreeSet<String>>,
    closed: Arc<AtomicBool>,
    /// Set once the death has been read, so concurrent callers that saw the
    /// same death do not recover it twice.
    reaped: Mutex<Option<Death>>,
}

#[derive(Clone, Debug)]
struct Death {
    message: String,
    /// The recovery pinned at least one key to the CPU.
    pinned: bool,
}

impl Conn {
    fn send(&self, json: &serde_json::Value, blobs: &[&[u8]]) -> std::io::Result<()> {
        protocol::write_frame(&mut *self.stdin.lock().unwrap(), json, blobs)
    }
}

pub(crate) struct Inner {
    config: WorkerConfig,
    models_root: PathBuf,
    context_tokens: u32,
    runtime: Arc<RuntimeState>,
    conn: Mutex<Option<Arc<Conn>>>,
    /// Packages pinned to the CPU by a failed canary in this session; sent
    /// again to every new worker (the store also remembers them).
    pinned: Mutex<BTreeSet<String>>,
    restarts: AtomicU64,
}

/// A provider whose llama.cpp runs in a supervised worker process.
#[derive(Clone)]
pub struct WorkerProvider {
    pub(crate) inner: Arc<Inner>,
}

impl WorkerProvider {
    pub fn new(config: WorkerConfig, models_root: impl Into<PathBuf>) -> WorkerProvider {
        let models_root = models_root.into();
        WorkerProvider {
            inner: Arc::new(Inner {
                runtime: RuntimeState::open(runtime_dir_for(&models_root)),
                config,
                models_root,
                context_tokens: 2048,
                conn: Mutex::new(None),
                pinned: Mutex::new(BTreeSet::new()),
                restarts: AtomicU64::new(0),
            }),
        }
    }

    pub fn with_context_tokens(self, n: u32) -> WorkerProvider {
        let mut inner = Arc::try_unwrap(self.inner).unwrap_or_else(|a| Inner {
            config: a.config.clone(),
            models_root: a.models_root.clone(),
            context_tokens: a.context_tokens,
            runtime: a.runtime.clone(),
            conn: Mutex::new(None),
            pinned: Mutex::new(a.pinned.lock().unwrap().clone()),
            restarts: AtomicU64::new(0),
        });
        inner.context_tokens = n;
        WorkerProvider {
            inner: Arc::new(inner),
        }
    }

    /// The running worker's process id (None before the first call).
    pub fn worker_pid(&self) -> Option<u32> {
        self.inner
            .conn
            .lock()
            .unwrap()
            .as_ref()
            .filter(|c| !c.closed.load(Ordering::SeqCst))
            .map(|c| c.pid)
    }

    /// Workers started after the first one (each follows a death).
    pub fn restarts(&self) -> u64 {
        self.inner.restarts.load(Ordering::SeqCst)
    }

    pub fn runtime_state(&self) -> &Arc<RuntimeState> {
        &self.inner.runtime
    }

    pub fn models_root(&self) -> &Path {
        &self.inner.models_root
    }

    /// Start the worker now (and check its protocol) instead of on the
    /// first call.
    pub fn start(&self) -> Result<(), ProviderError> {
        self.inner.connection().map(|_| ())
    }

    /// Stop the worker (it is restarted by the next call). Its sentinels are
    /// discarded: a stop we asked for is not a crash.
    pub fn stop(&self) {
        let conn = self.inner.conn.lock().unwrap().take();
        if let Some(c) = conn {
            c.closed.store(true, Ordering::SeqCst);
            let mut child = c.child.lock().unwrap();
            let _ = child.kill();
            let _ = child.wait();
            self.inner.runtime.discard_sentinels_of(c.pid);
        }
    }

    pub fn generate_cancellable(
        &self,
        req: ChatRequest,
        cancel: &AtomicBool,
        tokens_generated: Option<&AtomicU64>,
    ) -> Result<ChatResponse, ProviderError> {
        let package = installed(&req.model)?;
        let messages: Vec<String> = req
            .messages
            .iter()
            .map(canonical_string)
            .collect::<Result<_, _>>()?;
        let schema = req
            .response_schema
            .as_ref()
            .map(canonical_string)
            .transpose()?;
        let body = serde_json::json!({
            "op": "generate",
            "package": package,
            "messages": messages,
            "max_tokens": req.max_tokens,
            "temperature": req.temperature,
            "requires": req.requires.iter().map(|c| c.as_str()).collect::<Vec<_>>(),
            "response_schema": schema,
            "trace_key": req.trace_key,
        });
        let f = self
            .inner
            .call_loaded(&package, &body, &[], tokens_generated, Some(cancel))?;
        let v = &f.json["ok"];
        Ok(ChatResponse {
            content: v["content"].as_str().unwrap_or("").to_string(),
            usage: Usage {
                prompt_tokens: v["prompt_tokens"].as_u64().unwrap_or(0),
                completion_tokens: v["completion_tokens"].as_u64().unwrap_or(0),
            },
            executed_on: v["executed_on"].as_str().unwrap_or(&package).to_string(),
            execution_location: harbor_security::policy::ExecutionLocation::OnDevice,
        })
    }

    /// Pin `package_id` to the CPU (failed canary) and reload it there.
    pub fn force_cpu(&self, package_id: &str) {
        self.inner
            .pinned
            .lock()
            .unwrap()
            .insert(package_id.to_string());
        let conn = self.inner.conn.lock().unwrap().clone();
        if let Some(c) = conn.filter(|c| !c.closed.load(Ordering::SeqCst)) {
            let _ = self.inner.call_on(
                &c,
                &serde_json::json!({"op": "force_cpu", "package": package_id}),
                &[],
                None,
                None,
            );
            c.loaded.lock().unwrap().remove(package_id);
        }
    }

    pub fn mark_qualified(&self, package_id: &str) {
        let _ = self.inner.call(
            &serde_json::json!({"op": "mark_qualified", "package": package_id}),
            &[],
            None,
            None,
        );
    }

    /// See [`crate::gguf::GgufLlamaCppProvider::backend_of`].
    pub fn backend_of(&self, package_id: &str) -> Option<&'static str> {
        let conn = self.inner.conn.lock().unwrap().clone()?;
        if conn.closed.load(Ordering::SeqCst) || !conn.loaded.lock().unwrap().contains(package_id) {
            return None;
        }
        let f = self
            .inner
            .call_on(
                &conn,
                &serde_json::json!({"op": "backend_of", "package": package_id}),
                &[],
                None,
                None,
            )
            .ok()?;
        match f.json["ok"]["backend"].as_str()? {
            "gpu" => Some("gpu"),
            "cpu_pinned" => Some("cpu_pinned"),
            _ => Some("cpu"),
        }
    }

    pub fn weights_file(&self, package_id: &str) -> Result<PathBuf, ProviderError> {
        package_file(
            &self.inner.models_root,
            package_id,
            &["weights", "weights_shard"],
        )
    }

    pub fn mmproj_file(&self, package_id: &str) -> Result<PathBuf, ProviderError> {
        package_file(&self.inner.models_root, package_id, &["mmproj"]).map_err(|e| match e {
            ProviderError::ModelNotFound(m) if m.ends_with(": no file") => {
                ProviderError::UnsupportedCapability("package has no multimodal projector")
            }
            other => other,
        })
    }

    /// Debug builds only: make the worker abort (supervisor tests).
    #[doc(hidden)]
    pub fn debug_abort_worker(&self) -> Result<(), ProviderError> {
        self.inner
            .call(&serde_json::json!({"op": "debug_abort"}), &[], None, None)
            .map(|_| ())
    }
}

fn installed(model: &ModelRef) -> Result<String, ProviderError> {
    match model {
        ModelRef::InstalledPackage { package_id } => Ok(package_id.clone()),
        other => Err(ProviderError::ModelNotFound(format!(
            "gguf backend only loads installed packages, got {other:?}"
        ))),
    }
}

fn canonical_string(v: &harbor_canonical::JsonValue) -> Result<String, ProviderError> {
    v.to_canonical_bytes()
        .ok()
        .and_then(|b| String::from_utf8(b).ok())
        .ok_or_else(|| ProviderError::Backend("request is not canonical JSON".into()))
}

fn package_file(root: &Path, package_id: &str, roles: &[&str]) -> Result<PathBuf, ProviderError> {
    let manifest = PackageInstaller::new(root)
        .load_manifest(package_id)
        .map_err(|e| ProviderError::ModelNotFound(format!("{package_id}: {e}")))?;
    let f = manifest
        .files
        .iter()
        .find(|f| roles.contains(&f.role.as_str()))
        .ok_or_else(|| ProviderError::ModelNotFound(format!("{package_id}: no file")))?;
    let path = root.join(package_id).join(&f.path);
    if path.exists() {
        Ok(path)
    } else {
        Err(ProviderError::ModelNotFound(format!(
            "missing file {}",
            f.path
        )))
    }
}

impl Inner {
    fn connection(&self) -> Result<Arc<Conn>, ProviderError> {
        let old = {
            let slot = self.conn.lock().unwrap();
            match slot.as_ref() {
                Some(c) if !c.closed.load(Ordering::SeqCst) => return Ok(c.clone()),
                other => other.cloned(),
            }
        };
        // A worker that died between calls (typically killed by the OS while
        // the app was in the background): read its death first, then start
        // a new one. Nothing was in flight, so nothing is returned as failed.
        if let Some(old) = old {
            self.reap(&old);
        }
        let mut slot = self.conn.lock().unwrap();
        if let Some(c) = slot.as_ref() {
            if !c.closed.load(Ordering::SeqCst) {
                return Ok(c.clone());
            }
        }
        let conn = Arc::new(self.spawn()?);
        *slot = Some(conn.clone());
        drop(slot);
        let hello = self.call_on(
            &conn,
            &serde_json::json!({
                "op": "hello",
                "protocol": protocol::PROTOCOL_VERSION,
                "models_root": self.models_root.to_string_lossy(),
                "context_tokens": self.context_tokens,
            }),
            &[],
            None,
            None,
        )?;
        if hello.json["ok"]["protocol"].as_u64() != Some(protocol::PROTOCOL_VERSION) {
            return Err(ProviderError::Backend(format!(
                "inference worker speaks protocol {}, expected {}",
                hello.json["ok"]["protocol"],
                protocol::PROTOCOL_VERSION
            )));
        }
        let pinned: Vec<String> = self.pinned.lock().unwrap().iter().cloned().collect();
        for p in pinned {
            self.call_on(
                &conn,
                &serde_json::json!({"op": "force_cpu", "package": p}),
                &[],
                None,
                None,
            )?;
        }
        Ok(conn)
    }

    fn spawn(&self) -> Result<Conn, ProviderError> {
        let mut child = Command::new(&self.config.launcher)
            .arg(&self.config.library)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| {
                ProviderError::Backend(format!(
                    "start inference worker {}: {e}",
                    self.config.launcher.display()
                ))
            })?;
        let stdin = child.stdin.take().expect("piped stdin");
        let mut stdout = child.stdout.take().expect("piped stdout");
        let pid = child.id();
        let pending: Arc<Mutex<HashMap<u64, mpsc::Sender<Frame>>>> = Default::default();
        let closed = Arc::new(AtomicBool::new(false));
        {
            let pending = pending.clone();
            let closed = closed.clone();
            std::thread::Builder::new()
                .name("harbor-worker-reader".into())
                .spawn(move || {
                    while let Ok(Some(frame)) = protocol::read_frame(&mut stdout) {
                        let Some(id) = frame.json["id"].as_u64() else {
                            continue;
                        };
                        let tx = pending.lock().unwrap().get(&id).cloned();
                        if let Some(tx) = tx {
                            let _ = tx.send(frame);
                        }
                    }
                    closed.store(true, Ordering::SeqCst);
                    // Dropping the senders wakes every waiting call.
                    pending.lock().unwrap().clear();
                })
                .map_err(|e| ProviderError::Backend(format!("worker reader: {e}")))?;
        }
        Ok(Conn {
            child: Mutex::new(child),
            stdin: Mutex::new(stdin),
            pending,
            next_id: AtomicU64::new(1),
            pid,
            loaded: Mutex::new(BTreeSet::new()),
            closed,
            reaped: Mutex::new(None),
        })
    }

    /// Read how the worker died, recover or discard its sentinels, and
    /// forget the connection. Idempotent per connection.
    fn reap(&self, conn: &Arc<Conn>) -> Death {
        let mut reaped = conn.reaped.lock().unwrap();
        if let Some(d) = reaped.as_ref() {
            return d.clone();
        }
        conn.closed.store(true, Ordering::SeqCst);
        let status = {
            let mut child = conn.child.lock().unwrap();
            let mut status = None;
            for _ in 0..100 {
                if let Ok(Some(s)) = child.try_wait() {
                    status = Some(s);
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            if status.is_none() {
                // Stdout closed but the process lingers: stop it.
                let _ = child.kill();
                status = child.wait().ok();
            }
            status
        };
        let (crashed, how) = classify(status);
        let death = if crashed {
            let changed = self.runtime.recover_now();
            Death {
                message: format!("inference worker crashed ({how})"),
                pinned: changed.iter().any(|r| r.state.pins_cpu()),
            }
        } else {
            self.runtime.discard_sentinels_of(conn.pid);
            Death {
                message: format!("inference worker was stopped ({how})"),
                pinned: false,
            }
        };
        let mut slot = self.conn.lock().unwrap();
        if slot.as_ref().is_some_and(|c| Arc::ptr_eq(c, conn)) {
            *slot = None;
            // The next connection() counts as a restart.
            self.restarts.fetch_add(1, Ordering::SeqCst);
        }
        *reaped = Some(death.clone());
        death
    }

    fn call_on(
        &self,
        conn: &Arc<Conn>,
        body: &serde_json::Value,
        blobs: &[&[u8]],
        progress: Option<&AtomicU64>,
        cancel: Option<&AtomicBool>,
    ) -> Result<Frame, ProviderError> {
        self.call_on_inner(conn, body, blobs, progress, cancel)
            .map_err(|(e, _)| e)
    }

    /// One request on one connection. The error side carries the death, if
    /// the worker died during the call.
    fn call_on_inner(
        &self,
        conn: &Arc<Conn>,
        body: &serde_json::Value,
        blobs: &[&[u8]],
        progress: Option<&AtomicU64>,
        cancel: Option<&AtomicBool>,
    ) -> Result<Frame, (ProviderError, Option<Death>)> {
        let id = conn.next_id.fetch_add(1, Ordering::SeqCst);
        let mut body = body.clone();
        body["id"] = id.into();
        let (tx, rx) = mpsc::channel();
        conn.pending.lock().unwrap().insert(id, tx);
        if conn.closed.load(Ordering::SeqCst) || conn.send(&body, blobs).is_err() {
            conn.pending.lock().unwrap().remove(&id);
            let d = self.reap(conn);
            return Err((ProviderError::RuntimeCrashed(d.message.clone()), Some(d)));
        }
        let mut cancel_sent = false;
        loop {
            // Checked on every wake-up, not only on timeouts: a generation
            // streams progress frames often enough that the wait never
            // times out.
            if !cancel_sent && cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
                cancel_sent = true;
                let _ = conn.send(&serde_json::json!({"op": "cancel", "target": id}), &[]);
            }
            match rx.recv_timeout(Duration::from_millis(50)) {
                Ok(frame) => {
                    if let Some(n) = frame.json.get("progress").and_then(|p| p.as_u64()) {
                        if let Some(p) = progress {
                            p.store(n, Ordering::Relaxed);
                        }
                        continue;
                    }
                    conn.pending.lock().unwrap().remove(&id);
                    if frame.json.get("ok").is_some() {
                        return Ok(frame);
                    }
                    return Err((error_from_json(&frame.json["err"]), None));
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    let d = self.reap(conn);
                    return Err((ProviderError::RuntimeCrashed(d.message.clone()), Some(d)));
                }
            }
        }
    }

    /// A call on the current worker, starting one if needed. Retried once
    /// on a fresh worker when the death pinned a key to the CPU.
    pub(crate) fn call(
        &self,
        body: &serde_json::Value,
        blobs: &[&[u8]],
        progress: Option<&AtomicU64>,
        cancel: Option<&AtomicBool>,
    ) -> Result<Frame, ProviderError> {
        self.with_retry(|conn| self.call_on_inner(conn, body, blobs, progress, cancel))
    }

    /// Like [`call`](Self::call), after making sure `package` is loaded in
    /// the current worker (a restarted worker starts empty).
    pub(crate) fn call_loaded(
        &self,
        package: &str,
        body: &serde_json::Value,
        blobs: &[&[u8]],
        progress: Option<&AtomicU64>,
        cancel: Option<&AtomicBool>,
    ) -> Result<Frame, ProviderError> {
        self.with_retry(|conn| {
            self.ensure_loaded(conn, package)?;
            self.call_on_inner(conn, body, blobs, progress, cancel)
        })
    }

    fn ensure_loaded(
        &self,
        conn: &Arc<Conn>,
        package: &str,
    ) -> Result<(), (ProviderError, Option<Death>)> {
        if conn.loaded.lock().unwrap().contains(package) {
            return Ok(());
        }
        self.call_on_inner(
            conn,
            &serde_json::json!({"op": "load", "package": package}),
            &[],
            None,
            None,
        )?;
        conn.loaded.lock().unwrap().insert(package.to_string());
        Ok(())
    }

    fn with_retry(
        &self,
        mut f: impl FnMut(&Arc<Conn>) -> Result<Frame, (ProviderError, Option<Death>)>,
    ) -> Result<Frame, ProviderError> {
        let conn = self.connection()?;
        match f(&conn) {
            Ok(frame) => Ok(frame),
            Err((_, Some(death))) if death.pinned => {
                let conn = self.connection()?;
                f(&conn).map_err(|(e, _)| e)
            }
            Err((e, _)) => Err(e),
        }
    }
}

/// (crashed, description) from an exit status.
fn classify(status: Option<std::process::ExitStatus>) -> (bool, String) {
    let Some(status) = status else {
        return (true, "exit status unknown".into());
    };
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(sig) = status.signal() {
            // SIGKILL / SIGTERM come from outside: the low-memory killer,
            // Android's phantom-process limit, or a stop we asked for.
            return (!matches!(sig, 9 | 15), format!("signal {sig}"));
        }
    }
    match status.code() {
        Some(0) => (false, "exit 0".into()),
        Some(c) => (true, format!("exit {c:#x}")),
        None => (true, "no exit code".into()),
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        if let Some(c) = self.conn.lock().unwrap().take() {
            c.closed.store(true, Ordering::SeqCst);
            let mut child = c.child.lock().unwrap();
            let _ = child.kill();
            let _ = child.wait();
            self.runtime.discard_sentinels_of(c.pid);
        }
    }
}

impl ModelProvider for WorkerProvider {
    fn id(&self) -> &str {
        "harbor.gguf.llama-cpp"
    }

    fn capabilities(&self) -> &'static [Capabilities] {
        &[
            Capabilities::Chat,
            Capabilities::Embeddings,
            Capabilities::StructuredOutput,
        ]
    }

    fn supports(&self, model: &ModelRef, need: &Capabilities) -> bool {
        match (model, need) {
            (ModelRef::InstalledPackage { package_id }, Capabilities::Chat)
            | (ModelRef::InstalledPackage { package_id }, Capabilities::Embeddings)
            | (ModelRef::InstalledPackage { package_id }, Capabilities::StructuredOutput) => {
                self.weights_file(package_id).is_ok()
            }
            _ => false,
        }
    }

    fn load(&self, model: &ModelRef) -> Result<(), ProviderError> {
        let package = installed(model)?;
        self.weights_file(&package)?;
        self.inner
            .with_retry(|conn| {
                self.inner.ensure_loaded(conn, &package)?;
                Ok(Frame {
                    json: serde_json::Value::Null,
                    blobs: vec![],
                })
            })
            .map(|_| ())
    }

    fn unload(&self, model: &ModelRef) -> Result<(), ProviderError> {
        let package = installed(model)?;
        let conn = self.inner.conn.lock().unwrap().clone();
        if let Some(c) = conn.filter(|c| !c.closed.load(Ordering::SeqCst)) {
            if c.loaded.lock().unwrap().remove(&package) {
                self.inner.call_on(
                    &c,
                    &serde_json::json!({"op": "unload", "package": package}),
                    &[],
                    None,
                    None,
                )?;
            }
        }
        Ok(())
    }

    fn generate(&self, req: ChatRequest) -> Result<ChatResponse, ProviderError> {
        let never = AtomicBool::new(false);
        self.generate_cancellable(req, &never, None)
    }

    fn embed(&self, model: &ModelRef, texts: &[String]) -> Result<Vec<Vec<f32>>, ProviderError> {
        let package = installed(model)?;
        let f = self.inner.call_loaded(
            &package,
            &serde_json::json!({"op": "embed", "package": package, "texts": texts}),
            &[],
            None,
            None,
        )?;
        f.blobs.iter().map(|b| protocol::bytes_to_f32s(b)).collect()
    }

    fn execution_location(&self) -> harbor_security::policy::ExecutionLocation {
        harbor_security::policy::ExecutionLocation::OnDevice
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn exit_statuses_are_classified() {
        use std::os::unix::process::ExitStatusExt;
        let s = |raw| Some(std::process::ExitStatus::from_raw(raw));
        assert!(!classify(s(9)).0, "SIGKILL is external");
        assert!(!classify(s(15)).0, "SIGTERM is external");
        assert!(classify(s(11)).0, "SIGSEGV is a crash");
        assert!(classify(s(6)).0, "SIGABRT is a crash");
        assert!(!classify(s(0)).0, "exit 0");
        assert!(classify(s(1 << 8)).0, "exit 1");
        assert!(classify(None).0);
    }

    #[test]
    fn mode_defaults_to_auto_off_ios() {
        if cfg!(target_os = "ios") {
            assert_eq!(WorkerMode::from_env(), WorkerMode::Off);
        }
    }

    #[test]
    fn discovery_needs_both_files() {
        let d = tempfile::tempdir().unwrap();
        let lib = d.path().join("libcore.dylib");
        std::fs::write(&lib, b"").unwrap();
        // No launcher beside it: None (explicit env would be needed).
        std::env::set_var("HARBOR_WORKER_LIB", &lib);
        std::env::remove_var("HARBOR_WORKER_PATH");
        let found = WorkerConfig::discover();
        std::fs::write(d.path().join(launcher_name()), b"").unwrap();
        let found2 = WorkerConfig::discover();
        std::env::remove_var("HARBOR_WORKER_LIB");
        assert!(found.is_none() || found.unwrap().launcher != d.path().join(launcher_name()));
        assert_eq!(found2.unwrap().launcher, d.path().join(launcher_name()));
    }
}
