//! Stable scoped identifiers. Schema rule: `^[A-Za-z0-9][A-Za-z0-9_.:-]*$`,
//! 1..=128 chars unless the field declares otherwise.

use std::fmt;

#[derive(Debug, thiserror::Error)]
pub enum IdError {
    #[error("id must be 1..={max} characters, got {len}")]
    Length { max: usize, len: usize },
    #[error("id must start with an alphanumeric character")]
    Start,
    #[error("id contains an illegal character at position {0}")]
    Char(usize),
    #[error("hash must be 64 lowercase hex characters")]
    Hash,
}

/// A validated Harbor identifier (run, event, effect, batch, receipt...).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct HarborId(String);

impl HarborId {
    pub fn new(value: impl Into<String>) -> Result<Self, IdError> {
        let s = value.into();
        validate_id(&s, 128)?;
        Ok(HarborId(s))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Random identifier with a readable prefix, e.g. `run-01H...`.
    pub fn generate(prefix: &str) -> Self {
        let rand_part: String = {
            use rand::RngCore;
            let mut b = [0u8; 12];
            rand::rngs::OsRng.fill_bytes(&mut b);
            b.iter().map(|x| format!("{x:02x}")).collect()
        };
        HarborId(format!("{prefix}-{rand_part}"))
    }
}

impl fmt::Display for HarborId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for HarborId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

pub fn validate_id(s: &str, max: usize) -> Result<(), IdError> {
    let len = s.len();
    if len == 0 || len > max {
        return Err(IdError::Length { max, len });
    }
    let first = s.as_bytes()[0];
    if !first.is_ascii_alphanumeric() {
        return Err(IdError::Start);
    }
    for (i, b) in s.bytes().enumerate() {
        let ok = b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b':' | b'-');
        if !ok {
            return Err(IdError::Char(i));
        }
    }
    Ok(())
}

/// Validate a lowercase SHA-256 hex string.
pub fn validate_hash(s: &str) -> Result<(), IdError> {
    if s.len() == 64 && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) {
        Ok(())
    } else {
        Err(IdError::Hash)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_and_rejects() {
        assert!(HarborId::new("run-01HQX9").is_ok());
        assert!(HarborId::new("a.b:c_d-e").is_ok());
        assert!(HarborId::new("").is_err());
        assert!(HarborId::new("_leading").is_err());
        assert!(HarborId::new("has space").is_err());
        assert!(HarborId::new("has/slash").is_err());
        assert!(HarborId::new("x".repeat(129)).is_err());
    }

    #[test]
    fn hash_validation() {
        let good = "a".repeat(64);
        assert!(validate_hash(&good).is_ok());
        assert!(validate_hash(&good.to_uppercase()).is_err());
        assert!(validate_hash("abc").is_err());
    }

    #[test]
    fn generated_ids_are_valid() {
        for prefix in ["run", "evt", "fx", "batch", "rcpt"] {
            let id = HarborId::generate(prefix);
            assert!(id.as_str().starts_with(prefix));
            assert!(HarborId::new(id.as_str()).is_ok());
        }
    }
}
