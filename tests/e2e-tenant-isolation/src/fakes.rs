//! In-memory fakes for CAS / D1 / PAT / idempotency / quota / rate-limit /
//! Stripe webhook subsystems. Implementations are partitioned by concern so
//! each source file remains reviewable while the public fake API is unchanged.

mod extended;
mod foundation;
mod stores;

pub use extended::{
    AuditChain, AuditQueryEngine, CmkRotationLedger, ConstantTimeAuthProbe, DsrIntake,
    HierarchicalQuotaStore, KvReplicatedPatStore, MultipartBroker, PatRevokeLedger, RegionRouter,
    StripeWebhookLedger,
};
pub use foundation::{AuditAttempt, AuditCapture, DenyKind, FakeError};
pub use stores::{CasStore, D1Row, D1Store, IdempotencyStore, PatStore, QuotaStore, RateLimiter};
