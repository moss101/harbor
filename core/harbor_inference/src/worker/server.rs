//! Worker side of the inference worker (decision 0022 phase 3): a request
//! loop over stdin/stdout that owns the llama.cpp provider, the media
//! towers and the vision OCR engine. Started by `harbor_worker_main` in the
//! core library, which the `harbor_worker` launcher calls.
//!
//! Each request runs on its own thread so a `cancel` can arrive while a
//! generation runs. The process exits when stdin closes (the app went away)
//! or on `shutdown`.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use super::protocol::{self, error_to_json, Frame};
use crate::gguf::GgufLlamaCppProvider;
use crate::provider::{ChatRequest, ModelProvider, ModelRef, ProviderError};

type Out = Arc<Mutex<Box<dyn Write + Send>>>;

#[derive(Default)]
struct State {
    provider: OnceLock<GgufLlamaCppProvider>,
    cancels: Mutex<BTreeMap<u64, Arc<AtomicBool>>>,
    #[cfg(feature = "multimodal")]
    media: Mutex<BTreeMap<String, Arc<crate::multimodal::MediaEmbedder>>>,
    #[cfg(feature = "multimodal")]
    ocr: Mutex<Option<Arc<crate::vision_ocr::VisionOcr>>>,
}

impl State {
    fn provider(&self) -> Result<&GgufLlamaCppProvider, ProviderError> {
        self.provider
            .get()
            .ok_or_else(|| ProviderError::Backend("worker: hello first".into()))
    }
}

fn send(out: &Out, json: serde_json::Value, blobs: &[&[u8]]) {
    let _ = protocol::write_frame(&mut *out.lock().unwrap(), &json, blobs);
}

/// Run the request loop until stdin closes. Returns the process exit code.
pub fn serve(mut input: impl Read, output: impl Write + Send + 'static) -> i32 {
    let out: Out = Arc::new(Mutex::new(Box::new(output)));
    let state = Arc::new(State::default());
    loop {
        let frame = match protocol::read_frame(&mut input) {
            Ok(Some(f)) => f,
            Ok(None) => return 0,
            Err(_) => return 2,
        };
        let id = frame.json["id"].as_u64().unwrap_or(0);
        match frame.json["op"].as_str().unwrap_or("") {
            "cancel" => {
                let target = frame.json["target"].as_u64().unwrap_or(0);
                if let Some(c) = state.cancels.lock().unwrap().get(&target) {
                    c.store(true, Ordering::Relaxed);
                }
                continue;
            }
            "shutdown" => return 0,
            _ => {}
        }
        let cancel = Arc::new(AtomicBool::new(false));
        state.cancels.lock().unwrap().insert(id, cancel.clone());
        let (state, out) = (state.clone(), out.clone());
        std::thread::spawn(move || {
            let reply = handle(&state, &frame, &cancel, &out, id);
            state.cancels.lock().unwrap().remove(&id);
            match reply {
                Ok((value, blobs)) => {
                    let refs: Vec<&[u8]> = blobs.iter().map(|b| b.as_slice()).collect();
                    send(&out, serde_json::json!({"id": id, "ok": value}), &refs)
                }
                Err(e) => send(
                    &out,
                    serde_json::json!({"id": id, "err": error_to_json(&e)}),
                    &[],
                ),
            }
        });
    }
}

fn package(frame: &Frame) -> Result<String, ProviderError> {
    frame.json["package"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| ProviderError::Backend("worker: package missing".into()))
}

fn model(p: &str) -> ModelRef {
    ModelRef::InstalledPackage {
        package_id: p.to_string(),
    }
}

type Reply = Result<(serde_json::Value, Vec<Vec<u8>>), ProviderError>;

fn handle(state: &State, frame: &Frame, cancel: &AtomicBool, out: &Out, id: u64) -> Reply {
    let j = &frame.json;
    let ok = |v: serde_json::Value| Ok((v, Vec::new()));
    match j["op"].as_str().unwrap_or("") {
        "hello" => {
            let root = j["models_root"].as_str().unwrap_or("");
            let ctx = j["context_tokens"].as_u64().unwrap_or(2048) as u32;
            let p = GgufLlamaCppProvider::new(root)?.with_context_tokens(ctx);
            let _ = state.provider.set(p);
            ok(serde_json::json!({
                "protocol": protocol::PROTOCOL_VERSION,
                "runtime": crate::gguf::runtime_revision(),
                "pid": std::process::id(),
            }))
        }
        "load" => {
            state.provider()?.load(&model(&package(frame)?))?;
            ok(serde_json::json!({}))
        }
        "unload" => {
            let p = package(frame)?;
            #[cfg(feature = "multimodal")]
            state.media.lock().unwrap().remove(&p);
            state.provider()?.unload(&model(&p))?;
            ok(serde_json::json!({}))
        }
        "force_cpu" => {
            let p = package(frame)?;
            #[cfg(feature = "multimodal")]
            state.media.lock().unwrap().remove(&p);
            state.provider()?.force_cpu(&p);
            ok(serde_json::json!({}))
        }
        "mark_qualified" => {
            state.provider()?.mark_qualified(&package(frame)?);
            ok(serde_json::json!({}))
        }
        "backend_of" => {
            let b = state.provider()?.backend_of(&package(frame)?);
            ok(serde_json::json!({ "backend": b }))
        }
        "generate" => {
            let p = package(frame)?;
            let parse = |s: &str| {
                harbor_canonical::parse(s)
                    .map_err(|e| ProviderError::Backend(format!("worker: request json: {e}")))
            };
            let messages = j["messages"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .map(|m| parse(m.as_str().unwrap_or("null")))
                        .collect()
                })
                .transpose()?
                .unwrap_or_default();
            let response_schema = j["response_schema"].as_str().map(parse).transpose()?;
            let requires = j["requires"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|c| c.as_str().and_then(protocol::capability_from_str))
                        .collect()
                })
                .unwrap_or_default();
            let req = ChatRequest {
                model: model(&p),
                messages,
                max_tokens: j["max_tokens"].as_u64().unwrap_or(256) as u32,
                temperature: j["temperature"].as_f64().unwrap_or(0.0) as f32,
                requires,
                response_schema,
                trace_key: j["trace_key"].as_str().map(str::to_string),
            };
            // Progress pump: the token counter goes to the supervisor while
            // the generation runs.
            let counter = Arc::new(AtomicU64::new(0));
            let done = Arc::new(AtomicBool::new(false));
            let pump = {
                let (counter, done, out) = (counter.clone(), done.clone(), out.clone());
                std::thread::spawn(move || {
                    let mut last = 0;
                    while !done.load(Ordering::Relaxed) {
                        let n = counter.load(Ordering::Relaxed);
                        if n != last {
                            last = n;
                            send(&out, serde_json::json!({"id": id, "progress": n}), &[]);
                        }
                        std::thread::sleep(std::time::Duration::from_millis(40));
                    }
                })
            };
            let r = state
                .provider()?
                .generate_cancellable(req, cancel, Some(&counter));
            done.store(true, Ordering::Relaxed);
            let _ = pump.join();
            let r = r?;
            ok(serde_json::json!({
                "content": r.content,
                "prompt_tokens": r.usage.prompt_tokens,
                "completion_tokens": r.usage.completion_tokens,
                "executed_on": r.executed_on,
            }))
        }
        "embed" => {
            let texts: Vec<String> = serde_json::from_value(j["texts"].clone())
                .map_err(|e| ProviderError::Backend(format!("worker: texts: {e}")))?;
            let v = state.provider()?.embed(&model(&package(frame)?), &texts)?;
            Ok((
                serde_json::json!({ "count": v.len() }),
                v.iter().map(|x| protocol::f32s_to_bytes(x)).collect(),
            ))
        }
        #[cfg(feature = "multimodal")]
        "media_open" => media_open(state, &package(frame)?),
        #[cfg(feature = "multimodal")]
        "media_embed" => {
            let p = package(frame)?;
            let m = state
                .media
                .lock()
                .unwrap()
                .get(&p)
                .cloned()
                .ok_or_else(|| ProviderError::Backend("worker: media not open".into()))?;
            let parts = decode_parts(j, &frame.blobs)?;
            let v = m.embed(&parts)?;
            Ok((serde_json::json!({}), vec![protocol::f32s_to_bytes(&v)]))
        }
        #[cfg(feature = "multimodal")]
        "media_release" => {
            state.media.lock().unwrap().remove(&package(frame)?);
            ok(serde_json::json!({}))
        }
        #[cfg(feature = "multimodal")]
        "ocr_open" => {
            use crate::ocr::OcrEngine as _;
            let p = package(frame)?;
            let provider = state.provider()?;
            provider.load(&model(&p))?;
            let (engine, backend) =
                crate::vision_ocr::VisionOcr::open_verified(provider, &model(&p), &p)
                    .map_err(|e| ProviderError::Backend(e.to_string()))?;
            let v = serde_json::json!({
                "engine": engine.name(),
                "backend": backend,
                "languages": engine.languages(),
            });
            *state.ocr.lock().unwrap() = Some(Arc::new(engine));
            ok(v)
        }
        #[cfg(feature = "multimodal")]
        "ocr_image" => {
            use crate::ocr::{OcrEngine as _, OcrImage};
            let e = state
                .ocr
                .lock()
                .unwrap()
                .clone()
                .ok_or_else(|| ProviderError::Backend("worker: ocr not open".into()))?;
            let blob = frame.blobs.first().map(|b| b.as_slice()).unwrap_or(&[]);
            let image = match j["image"]["kind"].as_str() {
                Some("rgb") => OcrImage::Rgb {
                    width: j["image"]["width"].as_u64().unwrap_or(0) as u32,
                    height: j["image"]["height"].as_u64().unwrap_or(0) as u32,
                    data: blob,
                },
                _ => OcrImage::Encoded(blob),
            };
            let langs = string_list(&j["languages"]);
            let (text, confidence) = e.recognize_image(image, &langs, cancel).map_err(ocr_err)?;
            ok(serde_json::json!({ "text": text, "confidence": confidence }))
        }
        #[cfg(feature = "multimodal")]
        "ocr_recognize" => {
            use crate::ocr::OcrEngine as _;
            let e = state
                .ocr
                .lock()
                .unwrap()
                .clone()
                .ok_or_else(|| ProviderError::Backend("worker: ocr not open".into()))?;
            let blob = frame.blobs.first().map(|b| b.as_slice()).unwrap_or(&[]);
            let o = e
                .recognize(
                    j["kind"].as_str().unwrap_or(""),
                    blob,
                    &string_list(&j["languages"]),
                    j["max_pages"].as_u64().unwrap_or(0) as u32,
                    cancel,
                )
                .map_err(ocr_err)?;
            ok(serde_json::json!({
                "engine": o.engine,
                "pages": o.pages.iter().map(|p| serde_json::json!({
                    "page": p.page, "text": p.text, "source": p.source, "confidence": p.confidence,
                })).collect::<Vec<_>>(),
            }))
        }
        #[cfg(feature = "multimodal")]
        "ocr_release" => {
            *state.ocr.lock().unwrap() = None;
            ok(serde_json::json!({}))
        }
        // Supervisor tests: a real abort inside the worker. Debug builds only.
        #[cfg(debug_assertions)]
        "debug_abort" => std::process::abort(),
        other => Err(ProviderError::Backend(format!(
            "worker: unknown op {other:?}"
        ))),
    }
}

#[cfg(feature = "multimodal")]
fn ocr_err(e: crate::ocr::OcrError) -> ProviderError {
    match e {
        crate::ocr::OcrError::Cancelled => ProviderError::Cancelled,
        // The prefix lets the supervisor rebuild the OCR error kind.
        other => ProviderError::Backend(format!("ocr: {other}")),
    }
}

#[cfg(feature = "multimodal")]
fn string_list(v: &serde_json::Value) -> Vec<String> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|s| s.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(feature = "multimodal")]
fn media_open(state: &State, p: &str) -> Reply {
    let provider = state.provider()?;
    provider.load(&model(p))?;
    let m = Arc::new(crate::multimodal::MediaEmbedder::open(provider, p)?);
    let v = serde_json::json!({
        "vision": m.supports_vision(),
        "audio": m.supports_audio(),
        "sample_rate": m.audio_sample_rate(),
    });
    state.media.lock().unwrap().insert(p.to_string(), m);
    Ok((v, Vec::new()))
}

/// Parts: `[{"kind": "text", "text": s} | {"kind": "image" | "audio_file" |
/// "audio", "blob": i} | {"kind": "video", "blobs": [i...]}]`.
#[cfg(feature = "multimodal")]
fn decode_parts(
    j: &serde_json::Value,
    blobs: &[Vec<u8>],
) -> Result<Vec<crate::multimodal::MediaPart>, ProviderError> {
    use crate::multimodal::MediaPart;
    let blob = |i: &serde_json::Value| -> Result<Vec<u8>, ProviderError> {
        i.as_u64()
            .and_then(|i| blobs.get(i as usize))
            .cloned()
            .ok_or_else(|| ProviderError::Backend("worker: bad blob index".into()))
    };
    let mut out = Vec::new();
    for p in j["parts"].as_array().cloned().unwrap_or_default() {
        out.push(match p["kind"].as_str().unwrap_or("") {
            "text" => MediaPart::Text(p["text"].as_str().unwrap_or("").to_string()),
            "image" => MediaPart::Image(blob(&p["blob"])?),
            "audio_file" => MediaPart::AudioFile(blob(&p["blob"])?),
            "audio" => MediaPart::Audio(protocol::bytes_to_f32s(&blob(&p["blob"])?)?),
            "video" => MediaPart::Video(
                p["blobs"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
                    .iter()
                    .map(blob)
                    .collect::<Result<_, _>>()?,
            ),
            k => return Err(ProviderError::Backend(format!("worker: part kind {k:?}"))),
        });
    }
    Ok(out)
}

/// Entry point for the worker process (`harbor_worker_main`). The protocol
/// takes over the original stdout; fd 1 is pointed at stderr so stray
/// prints from native code (llama.cpp, drivers) cannot corrupt a frame.
pub fn worker_main() -> i32 {
    #[cfg(unix)]
    {
        use std::os::fd::FromRawFd;
        extern "C" {
            fn dup(fd: i32) -> i32;
            fn dup2(from: i32, to: i32) -> i32;
        }
        // SAFETY: plain descriptor duplication on this process's own fds;
        // `proto` is a fresh descriptor owned by the File below.
        let proto = unsafe { dup(1) };
        if proto < 0 || unsafe { dup2(2, 1) } < 0 {
            return 3;
        }
        let output = unsafe { std::fs::File::from_raw_fd(proto) };
        serve(std::io::stdin().lock(), std::io::BufWriter::new(output))
    }
    #[cfg(not(unix))]
    {
        serve(std::io::stdin().lock(), std::io::stdout())
    }
}
