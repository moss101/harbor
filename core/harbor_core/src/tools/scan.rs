//! `ocr.extract` - text from a typed paste, a PDF or a photographed page.
//!
//! The text of a document is the ground every later check (invoice fields,
//! drafted figures) is held against, so OCR is a tool with a typed result,
//! not a model call: it names the engine, keeps page numbers, says per page
//! whether the text came from the PDF's own text layer or from recognition,
//! and flags low-confidence pages. With no OCR engine on the device a scanned
//! PDF is a typed `Unavailable` error - never a silent empty document.

use serde_json::{json, Value};

use harbor_inference::ocr::{ocr_host, OcrError, OcrImage};

use super::builtin::{detect_kind, ArtifactKind};
use super::{RiskClass, Tool, ToolContext, ToolError, ToolSpec};

/// Below this mean confidence a page is reported for the person to check.
const LOW_CONFIDENCE: f64 = 0.6;

pub struct OcrExtract {
    spec: ToolSpec,
}

impl OcrExtract {
    pub fn new() -> Self {
        OcrExtract {
            spec: ToolSpec {
                id: "ocr.extract".into(),
                description: "Get the text of an attached PDF or image (scans are recognised on-device by the platform OCR engine; PDF pages that already carry text are used as they are). With no artifact, returns the typed fallback text. Result: text, per-page text with source (text_layer or ocr) and confidence, the engine, and the pages worth checking by eye.".into(),
                args_schema: json!({
                    "type": "object",
                    "properties": {
                        "artifact_id": {"type": ["string", "null"], "maxLength": 128},
                        "fallback": {"type": ["string", "null"], "maxLength": 2000000},
                        "languages": {"type": "array", "maxItems": 8, "items": {"type": "string", "maxLength": 12}},
                        "max_pages": {"type": "integer", "minimum": 1, "maximum": 200}
                    },
                    "additionalProperties": false
                }),
                risk: RiskClass::Read,
                requires: vec![],
                timeout_ms: 180_000,
                max_output_bytes: 4 * 1024 * 1024,
            },
        }
    }
}

impl Default for OcrExtract {
    fn default() -> Self {
        Self::new()
    }
}

fn is_image(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0x89, b'P', b'N', b'G']) || bytes.starts_with(&[0xFF, 0xD8, 0xFF])
}

impl Tool for OcrExtract {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn call(&self, ctx: &ToolContext<'_>, args: &Value) -> Result<Value, ToolError> {
        let tool = self.spec.id.clone();
        let fallback = args.get("fallback").and_then(Value::as_str).unwrap_or("");
        let id = args
            .get("artifact_id")
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty());
        let Some(id) = id else {
            if fallback.trim().is_empty() {
                return Err(ToolError::failed(
                    &tool,
                    "no document: paste the text or attach a PDF or a photo",
                ));
            }
            return Ok(json!({
                "source": "typed", "engine": null, "text": fallback,
                "pages": [], "ocr_pages": 0, "check_pages": [],
            }));
        };
        let a = ctx.artifacts.get(id).ok_or_else(|| {
            ToolError::Unavailable(
                tool.clone(),
                format!("artifact {id} is not attached to this run"),
            )
        })?;
        let languages: Vec<String> = args
            .get("languages")
            .and_then(Value::as_array)
            .map(|l| {
                l.iter()
                    .filter_map(|x| x.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        let max_pages = args.get("max_pages").and_then(Value::as_u64).unwrap_or(50) as u32;
        let kind = match detect_kind(&a.bytes) {
            ArtifactKind::Pdf => "pdf",
            ArtifactKind::Text => {
                let t = String::from_utf8_lossy(&a.bytes).to_string();
                return Ok(json!({
                    "source": "text_file", "engine": null, "text": t,
                    "pages": [], "ocr_pages": 0, "check_pages": [],
                }));
            }
            _ if is_image(&a.bytes) => "image",
            other => {
                return Err(ToolError::failed(
                    &tool,
                    format!("artifact {id} is {other:?}; OCR reads PDFs and PNG/JPEG images"),
                ))
            }
        };
        ctx.check_alive(&tool)?;
        let (engine, pages) = match ocr_host() {
            Ok(host) if kind == "image" || host.reads_pdf() => {
                let out = host
                    .recognize(kind, &a.bytes, &languages, max_pages, ctx.cancel)
                    .map_err(|e| match e {
                        OcrError::Cancelled => ToolError::Cancelled(tool.clone()),
                        other => ToolError::failed(&tool, other.to_string()),
                    })?;
                (Some(out.engine), out.pages)
            }
            Ok(host) => {
                // An image-reading engine (the vision LLM): use the PDF's own
                // text where it has any, and read the embedded scan of every
                // other page.
                let pdf = harbor_render::pdf::extract_pages(&a.bytes)
                    .map_err(|e| ToolError::failed(&tool, e.to_string()))?;
                let scans = harbor_render::scan_images::page_images(&a.bytes)
                    .map_err(|e| ToolError::failed(&tool, e.to_string()))?;
                let mut pages = Vec::new();
                for no in 1..=pdf.page_count.min(max_pages as usize) as u32 {
                    ctx.check_alive(&tool)?;
                    if let Some(p) = pdf.pages.iter().find(|p| p.index as u32 == no) {
                        pages.push(harbor_inference::ocr::OcrPage {
                            page: no,
                            text: p.text.clone(),
                            source: "text_layer".into(),
                            confidence: 1.0,
                        });
                        continue;
                    }
                    let Some(img) = scans.iter().find(|i| i.page == no) else {
                        return Err(ToolError::failed(
                            &tool,
                            format!("page {no} has neither text nor a readable scan image"),
                        ));
                    };
                    let image = match &img.data {
                        harbor_render::scan_images::PageImageData::Jpeg(b) => OcrImage::Encoded(b),
                        harbor_render::scan_images::PageImageData::Rgb {
                            width,
                            height,
                            data,
                        } => OcrImage::Rgb {
                            width: *width,
                            height: *height,
                            data,
                        },
                    };
                    let (text, confidence) = host
                        .recognize_image(image, &languages, ctx.cancel)
                        .map_err(|e| match e {
                            OcrError::Cancelled => ToolError::Cancelled(tool.clone()),
                            other => ToolError::failed(&tool, other.to_string()),
                        })?;
                    pages.push(harbor_inference::ocr::OcrPage {
                        page: no,
                        text,
                        source: "ocr".into(),
                        confidence,
                    });
                }
                (Some(host.name()), pages)
            }
            Err(_) if kind == "pdf" => {
                // No engine: the PDF's own text is still readable; a scan is not.
                let pdf = harbor_render::pdf::extract_pages(&a.bytes)
                    .map_err(|e| ToolError::failed(&tool, e.to_string()))?;
                if pdf.pages.len() < pdf.page_count || pdf.pages.is_empty() {
                    return Err(ToolError::Unavailable(
                        tool,
                        format!(
                            "{} of {} page(s) have no text layer (a scan) and this device has no OCR engine",
                            pdf.page_count - pdf.pages.len(),
                            pdf.page_count
                        ),
                    ));
                }
                (
                    None,
                    pdf.pages
                        .into_iter()
                        .map(|p| harbor_inference::ocr::OcrPage {
                            page: p.index as u32,
                            text: p.text,
                            source: "text_layer".into(),
                            confidence: 1.0,
                        })
                        .collect(),
                )
            }
            Err(e) => return Err(ToolError::Unavailable(tool, e.to_string())),
        };
        let text = pages
            .iter()
            .map(|p| p.text.trim())
            .filter(|t| !t.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n");
        if text.is_empty() {
            return Err(ToolError::failed(
                &tool,
                "no text could be read from the document",
            ));
        }
        // A language model reading a page can drop or invent a line while
        // reporting high confidence (measured), so EVERY page it recognised
        // is listed for a human look; a platform engine's own score decides
        // for the others.
        let llm_engine = engine
            .as_deref()
            .is_some_and(|e| e.starts_with("llm-vision"));
        let check: Vec<u32> = pages
            .iter()
            .filter(|p| p.source == "ocr" && (llm_engine || p.confidence < LOW_CONFIDENCE))
            .map(|p| p.page)
            .collect();
        Ok(json!({
            "source": if pages.iter().any(|p| p.source == "ocr") { "ocr" } else { "text_layer" },
            "engine": engine,
            "text": text,
            "pages": pages.iter().map(|p| json!({
                "page": p.page, "text": p.text, "source": p.source,
                "confidence": (p.confidence * 1000.0).round() / 1000.0,
            })).collect::<Vec<_>>(),
            "ocr_pages": pages.iter().filter(|p| p.source == "ocr").count(),
            "check_pages": check,
        }))
    }
}
