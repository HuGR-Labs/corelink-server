//! Failover audit event types + sink (re-used from replica-worker taxonomy).
//!
//! Failover events (`detected` + `resolved`) are defined in
//! [`corelink_replica_worker::ReplicaAuditEventType`]. This module provides
//! a thin shim that re-exports the relevant types and adds a failover-specific
//! sink interface for the router context.

use crate::Region;
pub use corelink_replica_worker::{
    FailingReplicaAuditSink as FailingFailoverAuditSink,
    InMemoryReplicaAuditSink as InMemoryFailoverAuditSink,
    ReplicaAuditEventType as FailoverAuditEventType, ReplicaAuditRecord as FailoverAuditRecord,
    ReplicaAuditSink as FailoverAuditSink,
};
use serde::{Deserialize, Serialize};

/// Failover-specific audit record with router context.
///
/// Wraps the generic [`FailoverAuditRecord`] with routing decision context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailoverRouterAuditRecord {
    /// The underlying audit record.
    pub record: FailoverAuditRecord,
    /// The region that was detected as degraded.
    pub degraded_region: Region,
    /// The sibling region reads were routed to (or `None` if no replica).
    pub routed_to: Option<Region>,
    /// Active trigger signals that caused failover.
    pub triggers: Vec<String>,
}
