//! Diagnostics without telemetry (production plan C1).
//!
//! Harbor's contract is local-only: nothing leaves the device unless the
//! user hands it over. Production support still needs evidence, so this
//! module keeps a **rolling, encrypted crash/error log** under the data
//! root and builds a **diagnostics export** the user can share by hand.
//!
//! - The log is a sequence of AEAD-sealed frames under a key derived from
//!   the workspace key (domain-separated, never stored raw). It is part of
//!   the plaintext-at-rest inspection like every other durable surface.
//! - Records carry a level, a source (core, ffi, app), a redacted message,
//!   an optional context (the FFI method, the Dart library) and an optional
//!   backtrace. Redaction replaces absolute paths and long quoted strings
//!   and caps the length, because an error string is the one place a
//!   document fragment could ride along.
//! - The export is a zip with `diagnostics.json` (build, runtime, device
//!   class, installed model ids, run counts by state, the redaction policy)
//!   and `records.jsonl`. **No document content, no chunks, no prompts.**
//!   The inspection test byte-scans it for the same sentinels as the data
//!   root.
//! - A panic hook records Rust panics; the Dart side records
//!   `FlutterError` / `PlatformDispatcher.onError` through the FFI.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use harbor_store::keys::{aead_open, aead_seal, KeyMaterial};

pub const SCHEMA: &str = "harbor.diagnostics/v1";
const FRAME_MAGIC: &[u8; 3] = b"HD1";
const AAD: &[u8] = b"harbor.diagnostics/v1";
/// Rolling bound: older records are dropped when the log exceeds this.
pub const MAX_RECORDS: usize = 500;
const MAX_MESSAGE_CHARS: usize = 800;

#[derive(Debug, thiserror::Error)]
pub enum DiagnosticsError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("crypto")]
    Crypto,
    #[error("zip: {0}")]
    Zip(String),
    #[error("{0}")]
    Other(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiagnosticRecord {
    pub at: DateTime<Utc>,
    /// `panic`, `error`, `warn`, `info`.
    pub level: String,
    /// `core`, `ffi`, `app`.
    pub source: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backtrace: Option<String>,
}

/// Replace absolute paths and long quoted strings; cap the length. The
/// policy is documented in the export so a reader knows what was removed.
pub fn redact(message: &str) -> String {
    static PATH_RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    static QUOTE_RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    // Absolute paths only (a leading `/` or drive letter at a token
    // boundary); source-relative locations like `src/executor.rs:10` are
    // build facts, not user data, and stay.
    let path_re = PATH_RE.get_or_init(|| {
        regex::Regex::new(
            r#"(^|[^A-Za-z0-9_./\\-])((?:[A-Za-z]:\\|/)(?:[^\s"'`<>|]+[\\/])+[^\s"'`<>|:;,)]*)"#,
        )
        .expect("path regex")
    });
    let quote_re =
        QUOTE_RE.get_or_init(|| regex::Regex::new(r#""([^"]{49,})""#).expect("quote regex"));
    let mut out = path_re
        .replace_all(message, |c: &regex::Captures| {
            let prefix = c.get(1).map(|m| m.as_str()).unwrap_or_default();
            let whole = c.get(2).map(|m| m.as_str()).unwrap_or_default();
            let tail = match whole.rsplit_once('.') {
                Some((_, ext))
                    if !ext.is_empty()
                        && ext.len() <= 6
                        && ext.chars().all(|ch| ch.is_ascii_alphanumeric()) =>
                {
                    format!("<path:.{ext}>")
                }
                _ => "<path>".to_string(),
            };
            format!("{prefix}{tail}")
        })
        .into_owned();
    out = quote_re.replace_all(&out, "\"<redacted>\"").into_owned();
    if out.chars().count() > MAX_MESSAGE_CHARS {
        out = out.chars().take(MAX_MESSAGE_CHARS).collect::<String>() + "…";
    }
    out
}

pub const REDACTION_POLICY: &[&str] = &[
    "absolute file paths replaced by <path:.ext>",
    "quoted strings longer than 48 characters replaced by \"<redacted>\"",
    "messages capped at 800 characters",
    "no document content, knowledge chunks, prompts or model outputs are ever recorded",
];

/// Where the log lives under the data root.
pub fn log_path(data_root: &Path) -> PathBuf {
    data_root.join("diagnostics").join("log.hdiag")
}

pub struct DiagnosticsLog {
    path: PathBuf,
    key: KeyMaterial,
    lock: Mutex<()>,
}

impl DiagnosticsLog {
    /// Open (or create) the log for a data root, sealing under a key derived
    /// from the workspace key.
    pub fn open(data_root: &Path, workspace_kek: &KeyMaterial) -> Result<Self, DiagnosticsError> {
        let path = log_path(data_root);
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p)?;
        }
        Ok(DiagnosticsLog {
            path,
            key: KeyMaterial::derive_subkey(workspace_kek, "harbor.diagnostics/v1"),
            lock: Mutex::new(()),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Append one record (message redacted here, so callers cannot forget).
    pub fn record(&self, mut rec: DiagnosticRecord) -> Result<(), DiagnosticsError> {
        rec.message = redact(&rec.message);
        rec.context = rec.context.map(|c| redact(&c));
        rec.backtrace = rec.backtrace.map(|b| {
            let b = redact(&b);
            b.chars().take(4000).collect()
        });
        // A panic inside `record` (or a hook firing while the lock is held)
        // must never deadlock: skip instead.
        let Ok(_guard) = self.lock.try_lock() else {
            return Err(DiagnosticsError::Other("diagnostics log busy".into()));
        };
        let frame = self.seal(&rec)?;
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        f.write_all(&frame)?;
        f.sync_data()?;
        drop(f);
        self.roll_if_needed()
    }

    fn seal(&self, rec: &DiagnosticRecord) -> Result<Vec<u8>, DiagnosticsError> {
        let plaintext =
            serde_json::to_vec(rec).map_err(|e| DiagnosticsError::Other(e.to_string()))?;
        let mut nonce = [0u8; 12];
        rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut nonce);
        let ct =
            aead_seal(&self.key, &nonce, &plaintext, AAD).map_err(|_| DiagnosticsError::Crypto)?;
        let mut frame = Vec::with_capacity(3 + 12 + 4 + ct.len());
        frame.extend_from_slice(FRAME_MAGIC);
        frame.extend_from_slice(&nonce);
        frame.extend_from_slice(&(ct.len() as u32).to_le_bytes());
        frame.extend_from_slice(&ct);
        Ok(frame)
    }

    fn parse_frames(&self, bytes: &[u8]) -> (Vec<DiagnosticRecord>, usize) {
        let mut out = Vec::new();
        let mut corrupt = 0usize;
        let mut i = 0usize;
        while i + 19 <= bytes.len() {
            if &bytes[i..i + 3] != FRAME_MAGIC {
                corrupt += 1;
                break;
            }
            let mut nonce = [0u8; 12];
            nonce.copy_from_slice(&bytes[i + 3..i + 15]);
            let len =
                u32::from_le_bytes([bytes[i + 15], bytes[i + 16], bytes[i + 17], bytes[i + 18]])
                    as usize;
            let start = i + 19;
            let Some(end) = start.checked_add(len).filter(|e| *e <= bytes.len()) else {
                corrupt += 1;
                break;
            };
            match aead_open(&self.key, &nonce, &bytes[start..end], AAD)
                .ok()
                .and_then(|pt| serde_json::from_slice::<DiagnosticRecord>(&pt).ok())
            {
                Some(rec) => out.push(rec),
                None => corrupt += 1,
            }
            i = end;
        }
        (out, corrupt)
    }

    /// All readable records, oldest first (corrupt frames are skipped and
    /// counted).
    pub fn records(&self) -> Result<(Vec<DiagnosticRecord>, usize), DiagnosticsError> {
        if !self.path.exists() {
            return Ok((Vec::new(), 0));
        }
        let bytes = std::fs::read(&self.path)?;
        Ok(self.parse_frames(&bytes))
    }

    fn roll_if_needed(&self) -> Result<(), DiagnosticsError> {
        let bytes = std::fs::read(&self.path)?;
        let (records, _) = self.parse_frames(&bytes);
        if records.len() <= MAX_RECORDS {
            return Ok(());
        }
        let keep = &records[records.len() - MAX_RECORDS..];
        let mut out = Vec::new();
        for r in keep {
            out.extend(self.seal(r)?);
        }
        let tmp = self.path.with_extension("tmp");
        std::fs::write(&tmp, &out)?;
        std::fs::rename(&tmp, &self.path)?;
        Ok(())
    }

    /// Install a process-wide panic hook that records panics here and
    /// then calls the previous hook. Idempotent per process.
    pub fn install_panic_hook(log: Arc<DiagnosticsLog>) {
        static INSTALLED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
        static SINK: std::sync::OnceLock<Mutex<Option<Arc<DiagnosticsLog>>>> =
            std::sync::OnceLock::new();
        let sink = SINK.get_or_init(|| Mutex::new(None));
        if let Ok(mut s) = sink.lock() {
            *s = Some(log);
        }
        INSTALLED.get_or_init(|| {
            let previous = std::panic::take_hook();
            std::panic::set_hook(Box::new(move |info| {
                let message = info
                    .payload()
                    .downcast_ref::<&str>()
                    .map(|s| s.to_string())
                    .or_else(|| info.payload().downcast_ref::<String>().cloned())
                    .unwrap_or_else(|| "panic".into());
                let location = info
                    .location()
                    .map(|l| format!("{}:{}", l.file(), l.line()));
                let backtrace = std::backtrace::Backtrace::force_capture().to_string();
                if let Some(log) = SINK
                    .get()
                    .and_then(|s| s.try_lock().ok())
                    .and_then(|s| s.clone())
                {
                    let _ = log.record(DiagnosticRecord {
                        at: Utc::now(),
                        level: "panic".into(),
                        source: "core".into(),
                        message,
                        context: location,
                        backtrace: Some(backtrace),
                    });
                }
                previous(info);
            }));
        });
    }
}

/// Facts about the build and host that make a report actionable without
/// identifying the user beyond a device-class description.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExportFacts {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_version: Option<String>,
    pub core_version: String,
    pub runtime_revision: String,
    pub os: String,
    pub arch: String,
    /// Short hash of the durable device id (correlates reports, does not
    /// identify a person).
    pub device_hash: String,
    pub workspace_id_hash: String,
    pub privacy_mode: String,
    pub installed_models: Vec<Value>,
    pub runs_by_state: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extra: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExportReport {
    pub path: PathBuf,
    pub bytes: u64,
    pub records: usize,
    pub corrupt_frames: usize,
    pub sha256: String,
}

/// Write the diagnostics bundle (zip) to `destination` (created new; an
/// existing file is refused so an export never overwrites user data).
pub fn export(
    log: &DiagnosticsLog,
    facts: &ExportFacts,
    destination: &Path,
) -> Result<ExportReport, DiagnosticsError> {
    if destination.exists() {
        return Err(DiagnosticsError::Other(format!(
            "destination {} already exists",
            destination.display()
        )));
    }
    let (records, corrupt) = log.records()?;
    let manifest = json!({
        "schema": SCHEMA,
        "generated_at": Utc::now().to_rfc3339(),
        "contents": ["diagnostics.json", "records.jsonl"],
        "contains": "crash/error records (redacted), build and runtime identity, device class, installed model ids, run counts",
        "never_contains": ["document content", "knowledge chunks", "prompts", "model outputs", "file paths", "keys or tokens"],
        "redaction_policy": REDACTION_POLICY,
        "facts": facts,
        "record_count": records.len(),
        "corrupt_frames": corrupt,
        "levels": {
            "panic": records.iter().filter(|r| r.level == "panic").count(),
            "error": records.iter().filter(|r| r.level == "error").count(),
            "warn": records.iter().filter(|r| r.level == "warn").count(),
            "info": records.iter().filter(|r| r.level == "info").count(),
        },
    });
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut buf);
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        zip.start_file("diagnostics.json", opts)
            .map_err(|e| DiagnosticsError::Zip(e.to_string()))?;
        zip.write_all(
            serde_json::to_string_pretty(&manifest)
                .unwrap_or_default()
                .as_bytes(),
        )?;
        zip.start_file("records.jsonl", opts)
            .map_err(|e| DiagnosticsError::Zip(e.to_string()))?;
        for r in &records {
            zip.write_all(serde_json::to_string(r).unwrap_or_default().as_bytes())?;
            zip.write_all(b"\n")?;
        }
        zip.finish()
            .map_err(|e| DiagnosticsError::Zip(e.to_string()))?;
    }
    let bytes = buf.into_inner();
    if let Some(p) = destination.parent() {
        std::fs::create_dir_all(p)?;
    }
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)?;
    f.write_all(&bytes)?;
    f.sync_all()?;
    Ok(ExportReport {
        path: destination.to_path_buf(),
        bytes: bytes.len() as u64,
        records: records.len(),
        corrupt_frames: corrupt,
        sha256: harbor_canonical::sha256_hex(&bytes),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key() -> KeyMaterial {
        KeyMaterial([7u8; 32])
    }

    #[test]
    fn redaction_strips_paths_long_quotes_and_caps_length() {
        let m = redact("failed to open /Users/amina/Documents/Q3-plan.docx: \"Dear Amina Haddad, your reference is HB-42 and the amount due is 1,250\" (again)");
        assert!(!m.contains("amina/Documents"), "{m}");
        assert!(m.contains("<path:.docx>"), "{m}");
        assert!(m.contains("\"<redacted>\""), "{m}");
        assert!(!m.contains("Haddad"));
        // Paths with spaces lose their directory part at least.
        let m = redact("open /Users/amina/Documents/Q3 plan.docx failed");
        assert!(!m.contains("amina"), "{m}");
        // Source-relative locations are build facts and stay readable.
        assert_eq!(
            redact("harbor_core/src/executor.rs:10"),
            "harbor_core/src/executor.rs:10"
        );
        let long = "x".repeat(2000);
        assert!(redact(&long).chars().count() <= MAX_MESSAGE_CHARS + 1);
        assert_eq!(
            redact("cell D2 changed since it was read"),
            "cell D2 changed since it was read"
        );
        assert!(redact("C:\\Users\\x\\report.xlsx missing").contains("<path:.xlsx>"));
    }

    #[test]
    fn records_round_trip_sealed_and_roll() {
        let dir = tempfile::tempdir().unwrap();
        let log = DiagnosticsLog::open(dir.path(), &key()).unwrap();
        for i in 0..(MAX_RECORDS + 20) {
            log.record(DiagnosticRecord {
                at: Utc::now(),
                level: "error".into(),
                source: "core".into(),
                message: format!("boom {i} in /tmp/private/doc{i}.docx"),
                context: Some("op.start_skill_run".into()),
                backtrace: None,
            })
            .unwrap();
        }
        let (records, corrupt) = log.records().unwrap();
        assert_eq!(corrupt, 0);
        assert_eq!(records.len(), MAX_RECORDS);
        assert!(records[0].message.starts_with("boom 20 "));
        assert!(records.last().unwrap().message.contains("<path:.docx>"));
        // Sealed at rest: the plaintext never appears in the file.
        let raw = std::fs::read(log.path()).unwrap();
        assert!(!raw.windows(4).any(|w| w == b"boom"));
        assert!(raw.starts_with(FRAME_MAGIC));
        // A different key cannot read it.
        let other = DiagnosticsLog::open(dir.path(), &KeyMaterial([9u8; 32])).unwrap();
        let (none, bad) = other.records().unwrap();
        assert!(none.is_empty());
        assert_eq!(bad, MAX_RECORDS);
    }

    #[test]
    fn export_bundle_has_manifest_and_records_and_never_overwrites() {
        let dir = tempfile::tempdir().unwrap();
        let log = DiagnosticsLog::open(dir.path(), &key()).unwrap();
        log.record(DiagnosticRecord {
            at: Utc::now(),
            level: "panic".into(),
            source: "core".into(),
            message: "index out of bounds".into(),
            context: Some("executor.rs:10".into()),
            backtrace: Some("0: a\n1: b".into()),
        })
        .unwrap();
        let facts = ExportFacts {
            app_version: Some("1.1.0+2".into()),
            core_version: "0.1.0".into(),
            runtime_revision: "llama.cpp/x".into(),
            os: "macos".into(),
            arch: "aarch64".into(),
            device_hash: "abcd".into(),
            workspace_id_hash: "ef01".into(),
            privacy_mode: "LocalOnly".into(),
            installed_models: vec![json!({"id": "qwen"})],
            runs_by_state: json!({"COMPLETED": 3}),
            extra: None,
        };
        let dest = dir.path().join("out").join("harbor-diagnostics.zip");
        let report = export(&log, &facts, &dest).unwrap();
        assert_eq!(report.records, 1);
        assert!(report.bytes > 0);
        let mut z = zip::ZipArchive::new(std::fs::File::open(&dest).unwrap()).unwrap();
        let names: Vec<String> = (0..z.len())
            .map(|i| z.by_index(i).unwrap().name().to_string())
            .collect();
        assert_eq!(names, ["diagnostics.json", "records.jsonl"]);
        let mut s = String::new();
        std::io::Read::read_to_string(&mut z.by_name("diagnostics.json").unwrap(), &mut s).unwrap();
        let m: Value = serde_json::from_str(&s).unwrap();
        assert_eq!(m["schema"], SCHEMA);
        assert_eq!(m["levels"]["panic"], 1);
        assert_eq!(m["facts"]["app_version"], "1.1.0+2");
        assert!(export(&log, &facts, &dest).is_err(), "never overwrites");
    }

    #[test]
    fn panic_hook_records_panics() {
        let dir = tempfile::tempdir().unwrap();
        let log = Arc::new(DiagnosticsLog::open(dir.path(), &key()).unwrap());
        DiagnosticsLog::install_panic_hook(log.clone());
        let r = std::panic::catch_unwind(|| {
            panic!("simulated crash in /Users/x/secret.docx");
        });
        assert!(r.is_err());
        let (records, corrupt) = log.records().unwrap();
        let p = records
            .iter()
            .find(|r| r.level == "panic")
            .unwrap_or_else(|| panic!("panic recorded; got {records:?} ({corrupt} corrupt)"));
        assert!(p.message.contains("simulated crash"));
        assert!(p.message.contains("<path:.docx>"));
        assert!(p
            .context
            .as_deref()
            .unwrap_or_default()
            .contains("diagnostics.rs"));
        assert!(p.backtrace.is_some());
    }
}
