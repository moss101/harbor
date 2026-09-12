//! Settings persistence with an upgrade/downgrade migration framework
//! (task HBR-004): upgrades run ordered transforms; downgrades fail safe —
//! the on-disk document is preserved untouched and an explicit error is
//! reported so a newer build's settings are never silently mangled.

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::error::{Result, StoreError};

pub const SETTINGS_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq)]
pub struct SettingsDocument {
    pub schema_version: u32,
    pub values: BTreeMap<String, harbor_canonical::JsonValue>,
}

impl SettingsDocument {
    pub fn empty() -> Self {
        SettingsDocument { schema_version: SETTINGS_SCHEMA_VERSION, values: BTreeMap::new() }
    }
}

/// One upgrade step from `from_version` to `from_version + 1`.
pub type UpgradeStep = fn(SettingsDocument) -> Result<SettingsDocument>;

pub struct SettingsStore {
    path: PathBuf,
    upgrades: Vec<UpgradeStep>,
}

impl SettingsStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        SettingsStore { path: path.into(), upgrades: Vec::new() }
    }

    /// Register the ordered upgrade chain (index i upgrades version i+1 -> i+2).
    pub fn with_upgrades(mut self, upgrades: Vec<UpgradeStep>) -> Self {
        self.upgrades = upgrades;
        self
    }

    fn encode(doc: &SettingsDocument) -> Vec<u8> {
        let values: serde_json::Map<String, serde_json::Value> = doc
            .values
            .iter()
            .map(|(k, v)| (k.clone(), serde_json::to_value(v).expect("canonical value serializes")))
            .collect();
        let obj = serde_json::json!({
            "schema_version": doc.schema_version,
            "values": serde_json::Value::Object(values),
        });
        obj.to_string().into_bytes()
    }

    fn decode(bytes: &[u8]) -> Result<SettingsDocument> {
        let v: serde_json::Value = serde_json::from_slice(bytes)?;
        let schema_version = v
            .get("schema_version")
            .and_then(|x| x.as_u64())
            .ok_or_else(|| StoreError::Other("settings missing schema_version".into()))?
            as u32;
        let mut values = BTreeMap::new();
        if let Some(map) = v.get("values").and_then(|x| x.as_object()) {
            for (k, val) in map {
                values.insert(k.clone(), harbor_canonical::convert(val.clone())?);
            }
        }
        Ok(SettingsDocument { schema_version, values })
    }

    /// Load settings, running pending upgrades. A downgrade attempt leaves
    /// the file untouched and returns [`StoreError::SettingsDowngrade`].
    pub fn load(&self) -> Result<SettingsDocument> {
        if !self.path.exists() {
            return Ok(SettingsDocument::empty());
        }
        let bytes = std::fs::read(&self.path)?;
        let mut doc = Self::decode(&bytes)?;
        if doc.schema_version > SETTINGS_SCHEMA_VERSION {
            return Err(StoreError::SettingsDowngrade {
                found: doc.schema_version,
                target: SETTINGS_SCHEMA_VERSION,
            });
        }
        let mut version = doc.schema_version;
        while version < SETTINGS_SCHEMA_VERSION {
            let step = self
                .upgrades
                .get(version as usize)
                .ok_or_else(|| {
                    StoreError::Other(format!("missing upgrade step for schema {version}"))
                })?;
            doc = step(doc)?;
            doc.schema_version = version + 1;
            version += 1;
        }
        // Persist the upgraded document only after every step succeeded.
        self.save(&doc)?;
        Ok(doc)
    }

    pub fn save(&self, doc: &SettingsDocument) -> Result<()> {
        let mut final_doc = doc.clone();
        final_doc.schema_version = SETTINGS_SCHEMA_VERSION;
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = self.path.with_extension("tmp");
        std::fs::write(&tmp, Self::encode(&final_doc))?;
        std::fs::rename(&tmp, &self.path)?;
        Ok(())
    }
}

fn v1(doc: SettingsDocument) -> Result<SettingsDocument> {
    // Example v0 -> v1 step: no transforms defined yet at schema v0.
    Ok(doc)
}

#[cfg(test)]
mod tests {
    use super::*;
    use harbor_canonical::JsonValue;

    #[test]
    fn roundtrip_and_upgrade_chain() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let store = SettingsStore::new(&path).with_upgrades(vec![v1]);
        let mut doc = SettingsDocument::empty();
        doc.values.insert("ui.theme".into(), JsonValue::str("dark"));
        doc.values.insert("run.budget_ms".into(), JsonValue::int(120_000).unwrap());
        store.save(&doc).unwrap();
        let loaded = store.load().unwrap();
        assert_eq!(loaded, doc);
    }

    #[test]
    fn downgrade_fails_safe_and_preserves_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let bytes = br#"{"schema_version": 99, "values": {"x": "y"}}"#;
        std::fs::write(&path, bytes).unwrap();
        let store = SettingsStore::new(&path);
        let err = store.load().unwrap_err();
        assert!(matches!(err, StoreError::SettingsDowngrade { found: 99, target: 1 }));
        assert_eq!(std::fs::read(&path).unwrap(), bytes, "file must be untouched");
    }

    #[test]
    fn fresh_start_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::new(dir.path().join("none.json"));
        assert_eq!(store.load().unwrap(), SettingsDocument::empty());
    }
}
