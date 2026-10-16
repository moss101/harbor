//! `speech.transcribe` - a spoken request or a recorded meeting as text.
//!
//! Speech is just another way to type: the transcript is TEXT that enters the
//! same typed pipeline (router suggestions, skill inputs, grounding checks)
//! as anything the person typed, and it authorizes nothing. The engine is the
//! OS's own on-device model; with none registered, or a language the model
//! does not cover, the tool says so instead of guessing.

use serde_json::{json, Value};

use harbor_inference::asr::{asr_engine, AsrError};

use super::{RiskClass, Tool, ToolContext, ToolError, ToolSpec};

pub struct SpeechTranscribe {
    spec: ToolSpec,
}

impl SpeechTranscribe {
    pub fn new() -> Self {
        SpeechTranscribe {
            spec: ToolSpec {
                id: "speech.transcribe".into(),
                description: "Transcribe an attached audio recording on-device with the operating system's own speech model. Result: text, timed segments, the engine and locale. Languages the model does not cover are refused, never approximated.".into(),
                args_schema: json!({
                    "type": "object",
                    "properties": {
                        "artifact_id": {"type": "string", "minLength": 1, "maxLength": 128},
                        "locale": {"type": "string", "maxLength": 16}
                    },
                    "required": ["artifact_id"],
                    "additionalProperties": false
                }),
                risk: RiskClass::Read,
                requires: vec![],
                timeout_ms: 300_000,
                max_output_bytes: 2 * 1024 * 1024,
            },
        }
    }
}

impl Default for SpeechTranscribe {
    fn default() -> Self {
        Self::new()
    }
}

impl Tool for SpeechTranscribe {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn call(&self, ctx: &ToolContext<'_>, args: &Value) -> Result<Value, ToolError> {
        let tool = self.spec.id.clone();
        let id = args
            .get("artifact_id")
            .and_then(Value::as_str)
            .unwrap_or("");
        let locale = args
            .get("locale")
            .and_then(Value::as_str)
            .unwrap_or("en_US");
        let a = ctx.artifacts.get(id).ok_or_else(|| {
            ToolError::Unavailable(
                tool.clone(),
                format!("artifact {id} is not attached to this run"),
            )
        })?;
        let engine =
            asr_engine().map_err(|e| ToolError::Unavailable(tool.clone(), e.to_string()))?;
        ctx.check_alive(&tool)?;
        let out = engine
            .transcribe(&a.bytes, locale, ctx.cancel)
            .map_err(|e| match e {
                AsrError::Cancelled => ToolError::Cancelled(tool.clone()),
                AsrError::Unsupported(m) => ToolError::failed(
                    &tool,
                    format!(
                        "{m}. Speech models on this device cover: {}",
                        engine.installed_locales().join(", ")
                    ),
                ),
                other => ToolError::failed(&tool, other.to_string()),
            })?;
        if out.text.is_empty() {
            return Err(ToolError::failed(
                &tool,
                "no speech was recognised in the recording",
            ));
        }
        Ok(json!({
            "engine": out.engine,
            "locale": out.locale,
            "text": out.text,
            "segments": out.segments.iter().map(|s| json!({"start": s.start, "end": s.end, "text": s.text})).collect::<Vec<_>>(),
        }))
    }
}
