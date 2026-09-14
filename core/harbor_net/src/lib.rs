//! Harbor Egress Broker and network audit log.
//!
//! Authority: `13_Network_and_Storage_Policy.md`, `03_Architecture_Contracts.md` §2.
//!
//! Every app-controlled HTTP request crosses [`EgressBroker`]:
//! - authorization is per egress class, per origin, via explicit,
//!   expiring sessions (acquisition metadata, weight transfer, ...);
//! - redirects are re-authorized at every origin change and credentials
//!   are stripped on cross-origin hops unless policy explicitly rebinds;
//! - the audit log distinguishes attempted/blocked, dispatched and
//!   completed traffic, is append-only and hash-chained like run events.

pub mod audit;
pub mod broker;
pub mod capture;
pub mod transport;

pub use audit::{AuditSink, NetworkAuditEntry, NetworkEventKind, SqliteAuditSink};
pub use broker::{
    DispatchOutcome, EgressBroker, EgressClass, EgressSession, RedirectDecision, Transport,
    TransportRequest, TransportResponse,
};
pub use transport::UreqTransport;
