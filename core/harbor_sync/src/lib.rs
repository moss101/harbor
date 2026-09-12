//! Harbor optional sync (M4): E2EE record replication.
//!
//! Authority: `11_Sync_Protocol.md`, `03_Architecture_Contracts.md` §12.
//!
//! - Sync replicates content/history only: never OS handles, local
//!   capability receipts, executor leases, or allow-once approval receipts.
//! - Every record is an authenticated envelope with group, device, device
//!   sequence, key epoch, HLC timestamp, tombstone flag, previous-record
//!   hash and AEAD ciphertext (unique per-record nonce).
//! - Epochs are monotonic and advance on revocation/expiry; restored old
//!   backups can never roll the epoch backward.
//! - Only appearance theme, display density and UI language use field-level
//!   LWW; privacy, capabilities, routing locks, approvals and key settings
//!   never do (type-enforced).

pub mod envelope;
pub mod lww;
pub mod transfer;

pub use envelope::{RecordEnvelope, SyncGroup, SyncIdentity, SyncRecordType, SyncError};
pub use lww::{lww_merge, is_lww_safe_field, LwwField};
pub use transfer::{TransferCoordinator, TransferState, TransferStatus, TRANSFER_ACK_TIMEOUT_DAYS};
