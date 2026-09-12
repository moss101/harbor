//! Harbor security: privacy policy, capability records, protected-effect
//! intents and approval receipts.
//!
//! Authority: `02_Runtime_Effect_and_Artifact_Contracts.md` (effects and
//! approvals), `schemas/effect_intent.schema.json`,
//! `schemas/approval_receipt.schema.json`, `13_Network_and_Storage_Policy.md`.
//!
//! The validation functions here are the executable form of the semantic
//! checks in `tools/contracts.py` (`validate_approval_binding`,
//! `validate_effect_update`): receipt lifetime at most 15 minutes, binding
//! equality across effect/receipt/batch, device and executor-generation
//! binding, and the closed effect transition graph in which an uncertain
//! outcome is never silently retried.

pub mod capability;
pub mod effect;
pub mod ids;
pub mod policy;
pub mod receipt;

pub use capability::{Capability, CapabilityRegistry, CapabilityScope};
pub use effect::{EffectClass, EffectIntent, EffectState, IdempotencyMode};
pub use ids::{HarborId, IdError};
pub use policy::{EgressClass, ExecutionLocation, PrivacyMode};
pub use receipt::{ApprovalReceipt, ReceiptError, MAX_RECEIPT_LIFETIME};
