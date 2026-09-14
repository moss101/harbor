//! Durable knowledge index over the FFI boundary.
//!
//! Chunks and vectors persist in `<data_root>/db/knowledge.db`, sealed
//! with ChaCha20-Poly1305 under a workspace-derived key
//! (`harbor_store::kcipher`): at rest the database holds no chunk text
//! and no vectors, matching the run event log and blob store guarantees
//! (policy 13). The in-memory index rebuilds on open. Embeddings come
//! from the pinned GGUF runtime (`harbor_inference::GgufLlamaCppProvider`)
//! with an installed embedding package — the index identity records the
//! actual model revision and dimension, and incompatible identities never
//! merge.
//!
//! Source replacement is versioned: ingesting a `source_id` replaces ALL
//! of its chunks (stale higher-ordinal chunks of a previous, longer
//! version cannot survive). Removal deletes the rows and revokes the
//! source in the live index, so previously cited chunks report `Removed`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;

use harbor_canonical::JsonValue;
use harbor_inference::gguf::GgufLlamaCppProvider;
use harbor_inference::provider::{Capabilities, ModelProvider, ModelRef};
use harbor_knowledge::chunk::{Chunker, ChunkerConfig};
use harbor_knowledge::identity::{embed_model_identity, ChunkerConfig as CC, IndexIdentity, Normalization};
use harbor_knowledge::index::{KnowledgeIndex, Source, SourceChunk};
use harbor_store::keys::KeyMaterial;
use rusqlite::Connection;

/// One source ready for ingestion: (id, title, text).
pub type SourceInput = (String, String, String);

/// A chunk as persisted (already unsealed).
pub struct PersistedChunk {
    pub source_id: String,
    pub chunk_id: String,
    pub title: String,
    pub content_hash: String,
    pub ordinal: u32,
    pub text: String,
    pub vector: Vec<f32>,
}

/// Summary of one indexed source (management UI).
pub struct SourceInfo {
    pub source_id: String,
    pub title: String,
    pub content_hash: String,
    pub chunks: u64,
    pub bytes: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum KnowledgeFfiError {
    #[error("db: {0}")]
    Db(String),
    #[error("provider: {0}")]
    Provider(String),
    #[error("crypto: sealed knowledge chunk failed to open (wrong key or corrupted row)")]
    Crypto,
    #[error("no embedding model installed (package {0})")]
    NoEmbeddingModel(String),
    #[error("source not found: {0}")]
    SourceNotFound(String),
    #[error("ingestion cancelled")]
    Cancelled,
}

/// Sealed persistence layer for the knowledge database. Independent of
/// the embedding provider so the real on-disk format is unit-testable
/// and qualifies for the plaintext-at-rest inspection.
pub struct KnowledgeStore {
    db_path: PathBuf,
    key: KeyMaterial,
}

impl KnowledgeStore {
    pub fn open(db_path: &Path, key: KeyMaterial) -> Result<Self, KnowledgeFfiError> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| KnowledgeFfiError::Db(e.to_string()))?;
        }
        let store = KnowledgeStore { db_path: db_path.to_path_buf(), key };
        store.init_db()?;
        Ok(store)
    }

    fn connect(&self) -> Result<Connection, KnowledgeFfiError> {
        Connection::open(&self.db_path).map_err(|e| KnowledgeFfiError::Db(e.to_string()))
    }

    /// Create the sealed-chunks schema; migrate a legacy PLAINTEXT
    /// database (pre-encryption development format with `text`/`vector`
    /// columns) by sealing every row under the current key.
    fn init_db(&self) -> Result<(), KnowledgeFfiError> {
        let conn = self.connect()?;
        // Legacy detection: the sealed schema has no `text` column.
        let legacy = conn
            .prepare("SELECT text FROM knowledge_chunks LIMIT 0")
            .is_ok();
        if legacy {
            self.migrate_legacy_plaintext(&conn)?;
        }
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS knowledge_chunks (
                source_id TEXT NOT NULL,
                chunk_id TEXT NOT NULL,
                title TEXT NOT NULL,
                content_hash TEXT NOT NULL,
                ordinal INTEGER NOT NULL,
                text_sealed BLOB NOT NULL,
                vector_sealed BLOB NOT NULL,
                PRIMARY KEY (source_id, chunk_id)
            );",
        )
        .map_err(|e| KnowledgeFfiError::Db(e.to_string()))?;
        Ok(())
    }

    /// One-time migration: read legacy plaintext rows, write them sealed,
    /// then drop the legacy table. Chunk text was private workspace
    /// content even in the legacy format — leaving it on disk unsealed
    /// would violate policy 13.
    fn migrate_legacy_plaintext(&self, conn: &Connection) -> Result<(), KnowledgeFfiError> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS knowledge_chunks_legacy (
                source_id TEXT NOT NULL,
                chunk_id TEXT NOT NULL,
                title TEXT NOT NULL,
                content_hash TEXT NOT NULL,
                ordinal INTEGER NOT NULL,
                text TEXT NOT NULL,
                vector BLOB NOT NULL,
                PRIMARY KEY (source_id, chunk_id)
            );
            INSERT OR IGNORE INTO knowledge_chunks_legacy SELECT * FROM knowledge_chunks;
            DROP TABLE knowledge_chunks;",
        )
        .map_err(|e| KnowledgeFfiError::Db(e.to_string()))?;
        let mut stmt = conn
            .prepare(
                "SELECT source_id, chunk_id, title, content_hash, ordinal, text, vector
                 FROM knowledge_chunks_legacy",
            )
            .map_err(|e| KnowledgeFfiError::Db(e.to_string()))?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, i64>(4)?,
                    r.get::<_, String>(5)?,
                    r.get::<_, Vec<u8>>(6)?,
                ))
            })
            .map_err(|e| KnowledgeFfiError::Db(e.to_string()))?;
        let mut sealed_rows: Vec<(String, String, String, String, i64, Vec<u8>, Vec<u8>)> =
            Vec::new();
        for row in rows {
            let (sid, cid, title, hash, ordinal, text, vec_bytes) =
                row.map_err(|e| KnowledgeFfiError::Db(e.to_string()))?;
            let vector: Vec<f32> = vec_bytes
                .chunks_exact(4)
                .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
                .collect();
            let text_sealed = harbor_store::kcipher::seal_text(&self.key, &sid, &cid, &text)
                .map_err(|_| KnowledgeFfiError::Crypto)?;
            let vector_sealed =
                harbor_store::kcipher::seal_vector(&self.key, &sid, &cid, &vector)
                    .map_err(|_| KnowledgeFfiError::Crypto)?;
            sealed_rows.push((sid, cid, title, hash, ordinal, text_sealed, vector_sealed));
        }
        drop(stmt);
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS knowledge_chunks (
                source_id TEXT NOT NULL,
                chunk_id TEXT NOT NULL,
                title TEXT NOT NULL,
                content_hash TEXT NOT NULL,
                ordinal INTEGER NOT NULL,
                text_sealed BLOB NOT NULL,
                vector_sealed BLOB NOT NULL,
                PRIMARY KEY (source_id, chunk_id)
            );",
        )
        .map_err(|e| KnowledgeFfiError::Db(e.to_string()))?;
        for (sid, cid, title, hash, ordinal, text_sealed, vector_sealed) in sealed_rows {
            conn.execute(
                "INSERT OR REPLACE INTO knowledge_chunks
                 (source_id, chunk_id, title, content_hash, ordinal, text_sealed, vector_sealed)
                 VALUES (?1,?2,?3,?4,?5,?6,?7)",
                rusqlite::params![sid, cid, title, hash, ordinal, text_sealed, vector_sealed],
            )
            .map_err(|e| KnowledgeFfiError::Db(e.to_string()))?;
        }
        conn.execute_batch("DROP TABLE knowledge_chunks_legacy;")
            .map_err(|e| KnowledgeFfiError::Db(e.to_string()))?;
        Ok(())
    }

    /// Load every persisted chunk, unsealing text + vector. A row that
    /// fails to open under the current key is a hard error: silently
    /// skipping it would pretend the index is complete.
    pub fn load_chunks(&self) -> Result<Vec<PersistedChunk>, KnowledgeFfiError> {
        let conn = self.connect()?;
        let mut stmt = conn
            .prepare(
                "SELECT source_id, chunk_id, title, content_hash, ordinal, text_sealed, vector_sealed
                 FROM knowledge_chunks",
            )
            .map_err(|e| KnowledgeFfiError::Db(e.to_string()))?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, i64>(4)?,
                    r.get::<_, Vec<u8>>(5)?,
                    r.get::<_, Vec<u8>>(6)?,
                ))
            })
            .map_err(|e| KnowledgeFfiError::Db(e.to_string()))?;
        let mut out = Vec::new();
        for row in rows {
            let (sid, cid, title, hash, ordinal, text_sealed, vector_sealed) =
                row.map_err(|e| KnowledgeFfiError::Db(e.to_string()))?;
            let text = harbor_store::kcipher::open_text(&self.key, &sid, &cid, &text_sealed)
                .map_err(|_| KnowledgeFfiError::Crypto)?;
            let vector =
                harbor_store::kcipher::open_vector(&self.key, &sid, &cid, &vector_sealed)
                    .map_err(|_| KnowledgeFfiError::Crypto)?;
            out.push(PersistedChunk {
                source_id: sid,
                chunk_id: cid,
                title,
                content_hash: hash,
                ordinal: ordinal as u32,
                text,
                vector,
            });
        }
        Ok(out)
    }

    /// Replace one source wholesale: delete its previous chunks, insert
    /// the new sealed set. One transaction — a crash leaves either the
    /// old or the new version, never a mix.
    pub fn replace_source(
        &self,
        source_id: &str,
        title: &str,
        content_hash: &str,
        chunks: &[(u32, String, Vec<f32>)], // (ordinal, text, vector)
    ) -> Result<(), KnowledgeFfiError> {
        let mut conn = self.connect()?;
        let tx = conn
            .transaction()
            .map_err(|e| KnowledgeFfiError::Db(e.to_string()))?;
        tx.execute("DELETE FROM knowledge_chunks WHERE source_id = ?1", [source_id])
            .map_err(|e| KnowledgeFfiError::Db(e.to_string()))?;
        for (ordinal, text, vector) in chunks {
            let cid = format!("{source_id}-{ordinal}");
            let text_sealed = harbor_store::kcipher::seal_text(&self.key, source_id, &cid, text)
                .map_err(|_| KnowledgeFfiError::Crypto)?;
            let vector_sealed =
                harbor_store::kcipher::seal_vector(&self.key, source_id, &cid, vector)
                    .map_err(|_| KnowledgeFfiError::Crypto)?;
            tx.execute(
                "INSERT INTO knowledge_chunks
                 (source_id, chunk_id, title, content_hash, ordinal, text_sealed, vector_sealed)
                 VALUES (?1,?2,?3,?4,?5,?6,?7)",
                rusqlite::params![source_id, cid, title, content_hash, *ordinal as i64, text_sealed, vector_sealed],
            )
            .map_err(|e| KnowledgeFfiError::Db(e.to_string()))?;
        }
        tx.commit().map_err(|e| KnowledgeFfiError::Db(e.to_string()))?;
        Ok(())
    }

    /// Remove a source's rows. Returns false when nothing was stored.
    pub fn remove_source(&self, source_id: &str) -> Result<bool, KnowledgeFfiError> {
        let conn = self.connect()?;
        let deleted = conn
            .execute("DELETE FROM knowledge_chunks WHERE source_id = ?1", [source_id])
            .map_err(|e| KnowledgeFfiError::Db(e.to_string()))?;
        Ok(deleted > 0)
    }

    /// List indexed sources for the management UI.
    pub fn sources(&self) -> Result<Vec<SourceInfo>, KnowledgeFfiError> {
        let conn = self.connect()?;
        let mut stmt = conn
            .prepare(
                "SELECT source_id, MAX(title), MAX(content_hash), COUNT(*), SUM(LENGTH(text_sealed) + LENGTH(vector_sealed))
                 FROM knowledge_chunks GROUP BY source_id ORDER BY source_id",
            )
            .map_err(|e| KnowledgeFfiError::Db(e.to_string()))?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, i64>(4)?,
                ))
            })
            .map_err(|e| KnowledgeFfiError::Db(e.to_string()))?;
        let mut out = Vec::new();
        for row in rows {
            let (sid, title, hash, chunks, bytes) =
                row.map_err(|e| KnowledgeFfiError::Db(e.to_string()))?;
            out.push(SourceInfo {
                source_id: sid,
                title,
                content_hash: hash,
                chunks: chunks as u64,
                bytes: bytes.max(0) as u64,
            });
        }
        Ok(out)
    }
}

pub struct KnowledgeService {
    index: Mutex<KnowledgeIndex>,
    provider: GgufLlamaCppProvider,
    embedding_package: String,
    store: KnowledgeStore,
    models_root: PathBuf,
}

impl KnowledgeService {
    /// Open (or create) the durable index. `embedding_package` names an
    /// installed GGUF embedding model from the modelhub store; `chunk_key`
    /// is the workspace-derived knowledge key (chunks are sealed at rest).
    pub fn open(
        data_root: &Path,
        embedding_package: &str,
        chunk_key: KeyMaterial,
    ) -> Result<Self, KnowledgeFfiError> {
        let db_path = data_root.join("db").join("knowledge.db");
        let provider = GgufLlamaCppProvider::new(data_root.join("models"))
            .map_err(|e| KnowledgeFfiError::Provider(e.to_string()))?;
        let model_ref = ModelRef::InstalledPackage { package_id: embedding_package.into() };
        // The embedding model must be loadable before we can accept sources.
        provider
            .load(&model_ref)
            .map_err(|e| KnowledgeFfiError::NoEmbeddingModel(format!("{embedding_package}: {e}")))?;
        // Identity from the REAL model: embed a probe to learn the dimension.
        let probe = provider
            .embed(&model_ref, &["identity probe".to_string()])
            .map_err(|e| KnowledgeFfiError::Provider(e.to_string()))?;
        let dimension = probe
            .first()
            .map(|v| v.len())
            .ok_or_else(|| KnowledgeFfiError::Provider("empty embedding".into()))? as u32;
        let identity = IndexIdentity {
            embedding: embed_model_identity(
                embedding_package,
                harbor_inference::gguf::runtime_revision(),
                dimension,
            ),
            chunker: "paragraph-window/1".into(),
            chunker_config: CC { target_graphemes: 800, overlap_graphemes: 80, respect_paragraphs: true },
            tokenizer: "grapheme/1".into(),
            normalization: Normalization::Nfc,
            language_policy: "en,ar,mixed".into(),
            encryption_scope: "workspace".into(),
        };
        let store = KnowledgeStore::open(&db_path, chunk_key)?;
        let svc = KnowledgeService {
            index: Mutex::new(KnowledgeIndex::new(identity)),
            provider,
            embedding_package: embedding_package.into(),
            store,
            models_root: data_root.join("models"),
        };
        svc.load_persisted()?;
        Ok(svc)
    }

    pub fn identity_hash(&self) -> String {
        self.index.lock().unwrap().identity.canonical_hash()
    }

    pub fn embedding_dimension(&self) -> u32 {
        self.index.lock().unwrap().identity.embedding.dimension
    }

    fn load_persisted(&self) -> Result<(), KnowledgeFfiError> {
        let persisted = self.store.load_chunks()?;
        let mut index = self.index.lock().unwrap();
        let mut seen_sources: std::collections::BTreeMap<String, (String, String)> = Default::default();
        for c in &persisted {
            seen_sources
                .entry(c.source_id.clone())
                .or_insert_with(|| (c.title.clone(), c.content_hash.clone()));
        }
        for (sid, (title, hash)) in seen_sources {
            let sc: Vec<SourceChunk> = persisted
                .iter()
                .filter(|c| c.source_id == sid)
                .map(|c| SourceChunk {
                    source_id: c.source_id.clone(),
                    chunk_id: c.chunk_id.clone(),
                    ordinal: c.ordinal,
                    text: c.text.clone(),
                    vector: c.vector.clone(),
                })
                .collect();
            index
                .add_source(Source { source_id: sid, title, content_hash: hash, indexed_at: chrono::Utc::now() }, sc)
                .map_err(|e| KnowledgeFfiError::Provider(e.to_string()))?;
        }
        Ok(())
    }

    /// Ingest sources: chunk -> embed (pinned runtime) -> persist sealed +
    /// index. Ingesting an existing source id REPLACES its previous
    /// chunks entirely (versioned replacement).
    pub fn ingest(&self, sources: &[SourceInput]) -> Result<serde_json::Value, KnowledgeFfiError> {
        self.ingest_with_progress(sources, &AtomicBool::new(false), None)
    }

    /// Cancellable ingest with chunk-level progress for background ops.
    pub fn ingest_with_progress(
        &self,
        sources: &[SourceInput],
        cancel: &AtomicBool,
        progress: Option<&harbor_modelhub::progress::AcquireProgress>,
    ) -> Result<serde_json::Value, KnowledgeFfiError> {
        let model_ref = ModelRef::InstalledPackage { package_id: self.embedding_package.clone() };
        let cfg = ChunkerConfig { target_graphemes: 800, overlap_graphemes: 80, respect_paragraphs: true };
        // Total chunk count for honest progress: chunk everything first.
        let planned: Vec<(String, String, String, Vec<String>)> = sources
            .iter()
            .map(|(id, title, text)| {
                let chunks = Chunker::chunk(text, &cfg)
                    .into_iter()
                    .map(|c| c.text)
                    .collect();
                (id.clone(), title.clone(), text.clone(), chunks)
            })
            .collect();
        let total_chunks: u64 = planned.iter().map(|(_, _, _, cs)| cs.len() as u64).sum();
        if let Some(p) = progress {
            p.items_total.store(total_chunks, Ordering::Relaxed);
            p.set_phase("ingesting");
        }
        let mut chunks_done = 0u64;
        for (id, title, text, chunks) in &planned {
            if cancel.load(Ordering::Relaxed) {
                return Err(KnowledgeFfiError::Cancelled);
            }
            let vectors = self
                .provider
                .embed(&model_ref, chunks)
                .map_err(|e| KnowledgeFfiError::Provider(e.to_string()))?;
            let content_hash = harbor_canonical::sha256_hex(text.as_bytes());
            let rows: Vec<(u32, String, Vec<f32>)> = chunks
                .iter()
                .cloned()
                .zip(vectors.iter().cloned())
                .enumerate()
                .map(|(i, (chunk_text, vector))| (i as u32, chunk_text, vector))
                .collect();
            let indexed_rows: Vec<SourceChunk> = rows
                .iter()
                .map(|(ordinal, chunk_text, vector)| SourceChunk {
                    source_id: id.clone(),
                    chunk_id: format!("{id}-{ordinal}"),
                    ordinal: *ordinal,
                    text: chunk_text.clone(),
                    vector: vector.clone(),
                })
                .collect();
            self.store
                .replace_source(id, title, &content_hash, &rows)?;
            {
                let mut index = self.index.lock().unwrap();
                // Versioned replacement in the live index too.
                let _ = index.remove_source(id);
                index
                    .add_source(
                        Source {
                            source_id: id.clone(),
                            title: title.clone(),
                            content_hash: content_hash.clone(),
                            indexed_at: chrono::Utc::now(),
                        },
                        indexed_rows,
                    )
                    .map_err(|e| KnowledgeFfiError::Provider(e.to_string()))?;
            }
            chunks_done += rows.len() as u64;
            if let Some(p) = progress {
                p.items_done.store(chunks_done, Ordering::Relaxed);
                p.set_detail(title);
            }
        }
        Ok(serde_json::json!({
            "sources": sources.len(),
            "chunks": chunks_done,
            "identity": self.identity_hash(),
        }))
    }

    /// Remove a source from the store and the live index. The source
    /// stays revoked for citation-state purposes (previously cited
    /// chunks report Removed, not Unknown).
    pub fn remove_source(&self, source_id: &str) -> Result<serde_json::Value, KnowledgeFfiError> {
        let deleted = self.store.remove_source(source_id)?;
        {
            let mut index = self.index.lock().unwrap();
            index
                .remove_source(source_id)
                .map_err(|_| KnowledgeFfiError::SourceNotFound(source_id.to_string()))?;
        }
        Ok(serde_json::json!({ "removed": source_id, "had_chunks": deleted }))
    }

    /// Indexed sources for the management UI.
    pub fn sources(&self) -> Result<serde_json::Value, KnowledgeFfiError> {
        let sources = self.store.sources()?;
        let out: Vec<serde_json::Value> = sources
            .iter()
            .map(|s| {
                serde_json::json!({
                    "source_id": s.source_id,
                    "title": s.title,
                    "content_hash": s.content_hash,
                    "chunks": s.chunks,
                    "bytes": s.bytes,
                })
            })
            .collect();
        Ok(serde_json::json!({ "sources": out, "identity": self.identity_hash() }))
    }

    /// Search: embed the question with the SAME model (identity-checked),
    /// return top-k citations with source states.
    pub fn search(&self, question: &str, top_k: usize) -> Result<serde_json::Value, KnowledgeFfiError> {
        let model_ref = ModelRef::InstalledPackage { package_id: self.embedding_package.clone() };
        let qv = self
            .provider
            .embed(&model_ref, &[question.to_string()])
            .map_err(|e| KnowledgeFfiError::Provider(e.to_string()))?;
        let q = qv.into_iter().next().ok_or_else(|| KnowledgeFfiError::Provider("empty query vector".into()))?;
        let index = self.index.lock().unwrap();
        let hits = index.search_with_text(&q, top_k);
        let citations: Vec<serde_json::Value> = hits
            .iter()
            .map(|c| {
                serde_json::json!({
                    "source_id": c.citation.source_id,
                    "title": c.citation.title,
                    "chunk_id": c.citation.chunk_id,
                    "score": c.citation.score,
                    "state": format!("{:?}", c.citation.state),
                    "content_hash": c.citation.content_hash,
                    // Chunk text rides with the citation so grounded
                    // generation can quote evidence; the UI shows only
                    // title/score/state.
                    "_text": c.text,
                })
            })
            .collect();
        Ok(serde_json::json!({ "citations": citations }))
    }

    pub fn supports_chat(&self) -> bool {
        let model_ref = ModelRef::InstalledPackage { package_id: self.embedding_package.clone() };
        self.provider.supports(&model_ref, &Capabilities::Chat)
    }

    pub fn models_root(&self) -> &Path {
        &self.models_root
    }
}

/// Chat-side handle for RAG generation over an installed GGUF chat model.
pub struct ChatHandle {
    provider: GgufLlamaCppProvider,
    loaded: std::sync::Mutex<std::collections::BTreeMap<String, ()>>,
}

/// The result of one grounded generation.
pub struct RagAnswer {
    pub answer: String,
    pub used_citations: bool,
    pub executed_on: String,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
}

impl ChatHandle {
    pub fn new(models_root: &Path) -> Self {
        ChatHandle {
            provider: GgufLlamaCppProvider::new(models_root)
                .expect("chat provider init"),
            loaded: std::sync::Mutex::new(Default::default()),
        }
    }

    /// Retrieve -> augment -> generate, cooperatively cancellable with
    /// real per-token progress.
    pub fn generate_rag_cancellable(
        &self,
        package_id: &str,
        question: &str,
        citations: Vec<serde_json::Value>,
        max_tokens: u32,
        cancel: &AtomicBool,
        progress: Option<&harbor_modelhub::progress::AcquireProgress>,
    ) -> Result<RagAnswer, String> {
        use harbor_inference::provider::{Capabilities, ChatRequest, ModelRef};
        if cancel.load(Ordering::Relaxed) {
            return Err("cancelled".into());
        }
        {
            let mut loaded = self.loaded.lock().unwrap();
            if loaded.insert(package_id.to_string(), ()).is_none() {
                if let Some(p) = progress {
                    p.set_phase("loading");
                    p.set_detail(package_id);
                }
                self.provider
                    .load(&ModelRef::InstalledPackage { package_id: package_id.into() })
                    .map_err(|e| e.to_string())?;
            }
        }
        // Compose: instructions + cited evidence + question.
        let mut context = String::from(
            "Answer using ONLY the evidence below. If the evidence is insufficient, reply exactly: INSUFFICIENT_EVIDENCE\n\n",
        );
        let mut used = false;
        for (i, c) in citations.iter().enumerate() {
            let title = c.get("title").and_then(|v| v.as_str()).unwrap_or("source");
            let _ = title;
            if let Some(evidence) = c.get("_text").and_then(|v| v.as_str()) {
                used = true;
                context.push_str(&format!("[{}] {}\n", i + 1, evidence));
            }
        }
        context.push_str(&format!("\nQuestion: {question}\nAnswer:"));
        if let Some(p) = progress {
            p.set_phase("generating");
            p.set_detail(question);
        }
        let resp = self
            .provider
            .generate_cancellable(
                ChatRequest {
                    model: ModelRef::InstalledPackage { package_id: package_id.into() },
                    messages: vec![JsonValue::object([
                        ("role", JsonValue::str("user")),
                        ("content", JsonValue::str(&context)),
                    ])],
                    max_tokens,
                    temperature: 0.0,
                    requires: vec![Capabilities::Chat],
                },
                cancel,
                progress.map(|p| &p.items_done),
            )
            .map_err(|e| e.to_string())?;
        Ok(RagAnswer {
            answer: resp.content,
            used_citations: used,
            executed_on: resp.executed_on,
            prompt_tokens: resp.usage.prompt_tokens,
            completion_tokens: resp.usage.completion_tokens,
        })
    }

    /// Lease-free bounded generation (kept for direct callers and tests).
    pub fn generate_rag(
        &self,
        package_id: &str,
        question: &str,
        citations: Vec<serde_json::Value>,
        max_tokens: u32,
    ) -> Result<RagAnswer, String> {
        self.generate_rag_cancellable(
            package_id,
            question,
            citations,
            max_tokens,
            &AtomicBool::new(false),
            None,
        )
    }
}
