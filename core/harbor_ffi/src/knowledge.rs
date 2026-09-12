//! Durable knowledge index over the FFI boundary.
//!
//! Chunks and vectors persist in the workspace store database; the
//! in-memory index rebuilds on open. Embeddings come from the pinned GGUF
//! runtime (`harbor_inference::GgufLlamaCppProvider`) with an installed
//! embedding package — the index identity records the actual model
//! revision and dimension, and incompatible identities never merge.

use std::path::PathBuf;
use std::sync::Mutex;

use harbor_canonical::JsonValue;
use harbor_inference::gguf::GgufLlamaCppProvider;
use harbor_inference::provider::{Capabilities, ModelProvider, ModelRef};
use harbor_knowledge::chunk::{Chunker, ChunkerConfig};
use harbor_knowledge::identity::{embed_model_identity, ChunkerConfig as CC, IndexIdentity, Normalization};
use harbor_knowledge::index::{KnowledgeIndex, Source, SourceChunk};
use rusqlite::Connection;

pub struct KnowledgeService {
    index: Mutex<KnowledgeIndex>,
    provider: GgufLlamaCppProvider,
    embedding_package: String,
    db_path: PathBuf,
    models_root: PathBuf,
}

#[derive(Debug, thiserror::Error)]
pub enum KnowledgeFfiError {
    #[error("db: {0}")]
    Db(String),
    #[error("provider: {0}")]
    Provider(String),
    #[error("no embedding model installed (package {0})")]
    NoEmbeddingModel(String),
}

impl KnowledgeService {
    /// Open (or create) the durable index. `embedding_package` names an
    /// installed GGUF embedding model from the modelhub store.
    pub fn open(data_root: &std::path::Path, embedding_package: &str) -> Result<Self, KnowledgeFfiError> {
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
        let index = Mutex::new(KnowledgeIndex::new(identity));
        let svc = KnowledgeService {
            index,
            provider,
            embedding_package: embedding_package.into(),
            db_path,
            models_root: data_root.join("models"),
        };
        svc.init_db()?;
        svc.load_persisted()?;
        Ok(svc)
    }

    pub fn identity_hash(&self) -> String {
        self.index.lock().unwrap().identity.canonical_hash()
    }

    pub fn embedding_dimension(&self) -> u32 {
        self.index.lock().unwrap().identity.embedding.dimension
    }

    fn init_db(&self) -> Result<(), KnowledgeFfiError> {
        let conn = Connection::open(&self.db_path).map_err(|e| KnowledgeFfiError::Db(e.to_string()))?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS knowledge_chunks (
                source_id TEXT NOT NULL,
                chunk_id TEXT NOT NULL,
                title TEXT NOT NULL,
                content_hash TEXT NOT NULL,
                ordinal INTEGER NOT NULL,
                text TEXT NOT NULL,
                vector BLOB NOT NULL,
                PRIMARY KEY (source_id, chunk_id)
            );",
        )
        .map_err(|e| KnowledgeFfiError::Db(e.to_string()))?;
        Ok(())
    }

    fn load_persisted(&self) -> Result<(), KnowledgeFfiError> {
        let conn = Connection::open(&self.db_path).map_err(|e| KnowledgeFfiError::Db(e.to_string()))?;
        let mut stmt = conn
            .prepare("SELECT source_id, chunk_id, title, content_hash, ordinal, text, vector FROM knowledge_chunks")
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
        let mut index = self.index.lock().unwrap();
        let mut seen_sources: std::collections::BTreeMap<String, (String, String)> = Default::default();
        let mut chunks: Vec<SourceChunk> = Vec::new();
        for row in rows {
            let (sid, cid, title, hash, ordinal, text, vec_bytes) = row.map_err(|e| KnowledgeFfiError::Db(e.to_string()))?;
            seen_sources.insert(sid.clone(), (title, hash));
            let vector: Vec<f32> = vec_bytes
                .chunks_exact(4)
                .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
                .collect();
            chunks.push(SourceChunk {
                source_id: sid,
                chunk_id: cid,
                ordinal: ordinal as u32,
                text,
                vector,
            });
        }
        for (sid, (title, hash)) in seen_sources {
            let sc: Vec<SourceChunk> = chunks.iter().filter(|c| c.source_id == sid).cloned().collect();
            index
                .add_source(Source { source_id: sid, title, content_hash: hash, indexed_at: chrono::Utc::now() }, sc)
                .map_err(|e| KnowledgeFfiError::Provider(e.to_string()))?;
        }
        Ok(())
    }

    /// Ingest sources: chunk -> embed (pinned runtime) -> persist + index.
    pub fn ingest(&self, sources: &[(String, String, String)]) -> Result<serde_json::Value, KnowledgeFfiError> {
        let model_ref = ModelRef::InstalledPackage { package_id: self.embedding_package.clone() };
        let cfg = ChunkerConfig { target_graphemes: 800, overlap_graphemes: 80, respect_paragraphs: true };
        let conn = Connection::open(&self.db_path).map_err(|e| KnowledgeFfiError::Db(e.to_string()))?;
        let mut ingested = 0usize;
        for (id, title, text) in sources {
            let chunks = Chunker::chunk(text, &cfg);
            let texts: Vec<String> = chunks.iter().map(|c| c.text.clone()).collect();
            let vectors = self
                .provider
                .embed(&model_ref, &texts)
                .map_err(|e| KnowledgeFfiError::Provider(e.to_string()))?;
            let content_hash = harbor_canonical::sha256_hex(text.as_bytes());
            for (c, v) in chunks.iter().zip(vectors.iter()) {
                let mut vec_bytes = Vec::with_capacity(v.len() * 4);
                for f in v {
                    vec_bytes.extend_from_slice(&f.to_le_bytes());
                }
                conn.execute(
                    "INSERT OR REPLACE INTO knowledge_chunks (source_id, chunk_id, title, content_hash, ordinal, text, vector)
                     VALUES (?1,?2,?3,?4,?5,?6,?7)",
                    rusqlite::params![id, format!("{id}-{}", c.ordinal), title, content_hash, c.ordinal as i64, c.text, vec_bytes],
                )
                .map_err(|e| KnowledgeFfiError::Db(e.to_string()))?;
            }
            ingested += 1;
        }
        self.load_persisted()?;
        Ok(serde_json::json!({ "sources": ingested, "identity": self.identity_hash() }))
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
        let hits = index.search(&q, top_k);
        let citations: Vec<serde_json::Value> = hits
            .iter()
            .map(|c| {
                serde_json::json!({
                    "source_id": c.source_id,
                    "title": c.title,
                    "chunk_id": c.chunk_id,
                    "score": c.score,
                    "state": format!("{:?}", c.state),
                    "content_hash": c.content_hash,
                })
            })
            .collect();
        Ok(serde_json::json!({ "citations": citations }))
    }

    pub fn supports_chat(&self) -> bool {
        let model_ref = ModelRef::InstalledPackage { package_id: self.embedding_package.clone() };
        self.provider.supports(&model_ref, &Capabilities::Chat)
    }
}
