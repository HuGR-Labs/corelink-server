//! Audit sink for region operations — WI-S14-001.
//!
//! INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER: audit fires BEFORE state mutation.
//! Audit failure → RegionError::AuditFailed → blocks downstream operation.

use crate::error::RegionError;
use crate::event::RegionAuditRecord;

/// Audit sink trait — fail-CLOSED per INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER.
pub trait RegionAuditSink: std::fmt::Debug + Send {
    /// Emit a region audit record. Must complete BEFORE state mutation.
    ///
    /// # Errors
    /// Any error here blocks the downstream operation (fail-CLOSED).
    fn emit(&mut self, record: RegionAuditRecord) -> Result<(), RegionError>;
}

/// In-memory audit sink for testing.
#[derive(Debug, Default)]
pub struct InMemoryRegionAuditSink {
    /// All emitted records in order.
    pub records: Vec<RegionAuditRecord>,
}

impl RegionAuditSink for InMemoryRegionAuditSink {
    fn emit(&mut self, record: RegionAuditRecord) -> Result<(), RegionError> {
        self.records.push(record);
        Ok(())
    }
}

/// Failing audit sink — always errors; validates fail-CLOSED behavior.
#[derive(Debug)]
pub struct FailingRegionAuditSink;

impl RegionAuditSink for FailingRegionAuditSink {
    fn emit(&mut self, _record: RegionAuditRecord) -> Result<(), RegionError> {
        Err(RegionError::AuditFailed(
            "FailingRegionAuditSink always fails — fail-CLOSED test".to_owned(),
        ))
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use crate::event::{RegionAuditRecord, EVT_REGION_PROVISIONED};
    use crate::region::Region;

    fn make_record() -> RegionAuditRecord {
        RegionAuditRecord::provisioning(
            Region::Wnam,
            "corelink-cas-wnam".to_owned(),
            "d1-wnam-id".to_owned(),
            "corelink-do-wnam".to_owned(),
            "us".to_owned(),
            1_700_000_000_000,
        )
    }

    #[test]
    fn test_in_memory_sink_accepts_records() {
        let mut sink = InMemoryRegionAuditSink::default();
        sink.emit(make_record()).expect("emit should succeed");
        sink.emit(make_record()).expect("emit should succeed");
        assert_eq!(sink.records.len(), 2);
        assert_eq!(sink.records[0].event_type, EVT_REGION_PROVISIONED);
    }

    #[test]
    fn test_failing_sink_returns_error() {
        let mut sink = FailingRegionAuditSink;
        let result = sink.emit(make_record());
        assert!(result.is_err(), "FailingRegionAuditSink must return error");
        match result {
            Err(RegionError::AuditFailed(_)) => {}
            other => panic!("unexpected result: {:?}", other),
        }
    }

    #[test]
    fn test_audit_record_has_uuid_v7() {
        let rec = make_record();
        // UUIDv7 — version nibble = 7
        assert_eq!(rec.id.get_version_num(), 7);
    }
}
