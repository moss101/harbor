//! Media towers and vision OCR as seen from the supervisor: thin RPC
//! wrappers over a [`WorkerProvider`] (decision 0022 phase 3).

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use super::protocol;
use super::supervisor::{Inner, WorkerProvider};
use crate::multimodal::{MediaEmbed, MediaPart};
use crate::ocr::{OcrEngine, OcrError, OcrImage, OcrOutput, OcrPage};
use crate::provider::ProviderError;

pub struct WorkerMedia {
    inner: Arc<Inner>,
    package: String,
    vision: bool,
    audio: bool,
    sample_rate: Option<u32>,
}

impl WorkerMedia {
    pub fn open(w: &WorkerProvider, package: &str) -> Result<WorkerMedia, ProviderError> {
        let f = w.inner.call_loaded(
            package,
            &serde_json::json!({"op": "media_open", "package": package}),
            &[],
            None,
            None,
        )?;
        let v = &f.json["ok"];
        Ok(WorkerMedia {
            inner: w.inner.clone(),
            package: package.to_string(),
            vision: v["vision"].as_bool().unwrap_or(false),
            audio: v["audio"].as_bool().unwrap_or(false),
            sample_rate: v["sample_rate"].as_u64().map(|r| r as u32),
        })
    }
}

impl MediaEmbed for WorkerMedia {
    fn embed(&self, parts: &[MediaPart]) -> Result<Vec<f32>, ProviderError> {
        let mut blobs: Vec<Vec<u8>> = Vec::new();
        let mut push = |b: Vec<u8>| {
            blobs.push(b);
            blobs.len() - 1
        };
        let wire: Vec<serde_json::Value> = parts
            .iter()
            .map(|p| match p {
                MediaPart::Text(t) => serde_json::json!({"kind": "text", "text": t}),
                MediaPart::Image(b) => {
                    serde_json::json!({"kind": "image", "blob": push(b.clone())})
                }
                MediaPart::AudioFile(b) => {
                    serde_json::json!({"kind": "audio_file", "blob": push(b.clone())})
                }
                MediaPart::Audio(pcm) => {
                    serde_json::json!({"kind": "audio", "blob": push(protocol::f32s_to_bytes(pcm))})
                }
                MediaPart::Video(frames) => {
                    let ids: Vec<usize> = frames.iter().map(|f| push(f.clone())).collect();
                    serde_json::json!({"kind": "video", "blobs": ids})
                }
            })
            .collect();
        let refs: Vec<&[u8]> = blobs.iter().map(|b| b.as_slice()).collect();
        let body = serde_json::json!({"op": "media_embed", "package": self.package, "parts": wire});
        // A restarted worker has no media towers open: reopen them first.
        let f = match self
            .inner
            .call_loaded(&self.package, &body, &refs, None, None)
        {
            Err(ProviderError::Backend(m)) if m.contains("media not open") => {
                self.inner.call_loaded(
                    &self.package,
                    &serde_json::json!({"op": "media_open", "package": self.package}),
                    &[],
                    None,
                    None,
                )?;
                self.inner
                    .call_loaded(&self.package, &body, &refs, None, None)?
            }
            other => other?,
        };
        protocol::bytes_to_f32s(f.blobs.first().map(|b| b.as_slice()).unwrap_or(&[]))
    }

    fn supports_vision(&self) -> bool {
        self.vision
    }

    fn supports_audio(&self) -> bool {
        self.audio
    }

    fn audio_sample_rate(&self) -> Option<u32> {
        self.sample_rate
    }
}

impl Drop for WorkerMedia {
    fn drop(&mut self) {
        // Free the towers in the worker (best effort; a dead worker has none).
        let _ = self.inner.call(
            &serde_json::json!({"op": "media_release", "package": self.package}),
            &[],
            None,
            None,
        );
    }
}

pub struct WorkerOcr {
    inner: Arc<Inner>,
    package: String,
    name: String,
    languages: Vec<String>,
}

impl WorkerOcr {
    pub fn open(w: &WorkerProvider, package: &str) -> Result<(WorkerOcr, String), ProviderError> {
        let f = w.inner.call_loaded(
            package,
            &serde_json::json!({"op": "ocr_open", "package": package}),
            &[],
            None,
            None,
        )?;
        let v = &f.json["ok"];
        Ok((
            WorkerOcr {
                inner: w.inner.clone(),
                package: package.to_string(),
                name: v["engine"].as_str().unwrap_or("").to_string(),
                languages: v["languages"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|s| s.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default(),
            },
            v["backend"].as_str().unwrap_or("cpu").to_string(),
        ))
    }

    /// One OCR call; a restarted worker reopens (and re-verifies) the engine.
    fn call(
        &self,
        body: serde_json::Value,
        blobs: &[&[u8]],
        cancel: &AtomicBool,
    ) -> Result<serde_json::Value, OcrError> {
        let run = || {
            self.inner
                .call_loaded(&self.package, &body, blobs, None, Some(cancel))
        };
        let f = match run() {
            Err(ProviderError::Backend(m)) if m.contains("ocr not open") => {
                self.inner
                    .call_loaded(
                        &self.package,
                        &serde_json::json!({"op": "ocr_open", "package": self.package}),
                        &[],
                        None,
                        None,
                    )
                    .map_err(to_ocr)?;
                run()
            }
            other => other,
        }
        .map_err(to_ocr)?;
        Ok(f.json["ok"].clone())
    }
}

fn to_ocr(e: ProviderError) -> OcrError {
    match e {
        ProviderError::Cancelled => OcrError::Cancelled,
        ProviderError::Backend(m) => OcrError::Failed(
            m.strip_prefix("ocr: OCR failed: ")
                .or_else(|| m.strip_prefix("ocr: "))
                .unwrap_or(&m)
                .to_string(),
        ),
        other => OcrError::Failed(other.to_string()),
    }
}

impl OcrEngine for WorkerOcr {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn languages(&self) -> Vec<String> {
        self.languages.clone()
    }

    fn reads_pdf(&self) -> bool {
        false
    }

    fn recognize(
        &self,
        kind: &str,
        bytes: &[u8],
        languages: &[String],
        max_pages: u32,
        cancel: &AtomicBool,
    ) -> Result<OcrOutput, OcrError> {
        let v = self.call(
            serde_json::json!({"op": "ocr_recognize", "kind": kind,
                "languages": languages, "max_pages": max_pages}),
            &[bytes],
            cancel,
        )?;
        Ok(OcrOutput {
            engine: v["engine"].as_str().unwrap_or(&self.name).to_string(),
            pages: v["pages"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .iter()
                .map(|p| OcrPage {
                    page: p["page"].as_u64().unwrap_or(0) as u32,
                    text: p["text"].as_str().unwrap_or("").to_string(),
                    source: p["source"].as_str().unwrap_or("ocr").to_string(),
                    confidence: p["confidence"].as_f64().unwrap_or(0.0),
                })
                .collect(),
        })
    }

    fn recognize_image(
        &self,
        image: OcrImage<'_>,
        languages: &[String],
        cancel: &AtomicBool,
    ) -> Result<(String, f64), OcrError> {
        let (desc, data) = match image {
            OcrImage::Encoded(b) => (serde_json::json!({"kind": "encoded"}), b),
            OcrImage::Rgb {
                width,
                height,
                data,
            } => (
                serde_json::json!({"kind": "rgb", "width": width, "height": height}),
                data,
            ),
        };
        let v = self.call(
            serde_json::json!({"op": "ocr_image", "image": desc, "languages": languages}),
            &[data],
            cancel,
        )?;
        Ok((
            v["text"].as_str().unwrap_or("").to_string(),
            v["confidence"].as_f64().unwrap_or(0.0),
        ))
    }
}

impl Drop for WorkerOcr {
    fn drop(&mut self) {
        let _ = self
            .inner
            .call(&serde_json::json!({"op": "ocr_release"}), &[], None, None);
    }
}
