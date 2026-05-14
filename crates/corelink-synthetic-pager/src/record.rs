//! Drill record + persistence trait + fakes.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::error::SyntheticDrillError;
use crate::outcome::{AckOutcome, MttaMs};
use crate::region::Region;
use crate::request::SyntheticDrillId;
use crate::vector::AckVector;

/// One synthetic drill execution record — persisted in D1
/// `synthetic_page_drills` (migration 0042).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct DrillRecord {
    /// Canonical drill id (`SP-<...>`).
    pub drill_id: SyntheticDrillId,
    /// Region that was paged.
    pub region: Region,
    /// On-call engineer slug captured at ack time (e.g. `eng-001`);
    /// zero PII per CTRL-PRIV-001.
    pub engineer_slug: String,
    /// Emit timestamp (ms since epoch).
    pub emit_ts_ms: i64,
    /// Ack timestamp (ms since epoch); `None` if drill closed without
    /// ack.
    pub ack_ts_ms: Option<i64>,
    /// Channel that delivered the ack; `None` if unacked.
    pub ack_vector: Option<AckVector>,
    /// MTTA in ms; `None` if unacked.
    pub mtta_ms: Option<MttaMs>,
    /// Final classification.
    pub outcome: AckOutcome,
    /// Audit-chain correlation id (PAT-CORRELATION-ID-001).
    pub correlation_id: String,
}

impl DrillRecord {
    /// Construct a validated drill record.
    ///
    /// # Errors
    ///
    /// * [`SyntheticDrillError::EmptyEngineer`] if `engineer_slug` is
    ///   blank AND the drill was acked (an unacked drill is allowed to
    ///   carry an empty engineer slug because the rotation engineer is
    ///   resolved at ack-receipt time).
    /// * [`SyntheticDrillError::AckBeforeEmit`] if `ack_ts_ms` predates
    ///   `emit_ts_ms`.
    #[allow(clippy::too_many_arguments, reason = "domain record assembled from upstream cron + webhook fields")]
    pub fn new(
        drill_id: SyntheticDrillId,
        region: Region,
        engineer_slug: impl Into<String>,
        emit_ts_ms: i64,
        ack_ts_ms: Option<i64>,
        ack_vector: Option<AckVector>,
        mtta_ms: Option<MttaMs>,
        outcome: AckOutcome,
        correlation_id: impl Into<String>,
    ) -> Result<Self, SyntheticDrillError> {
        let engineer_slug: String = engineer_slug.into();
        if ack_ts_ms.is_some() && engineer_slug.trim().is_empty() {
            return Err(SyntheticDrillError::EmptyEngineer);
        }
        if let Some(ts) = ack_ts_ms {
            if ts < emit_ts_ms {
                return Err(SyntheticDrillError::AckBeforeEmit {
                    emit_ts_ms,
                    ack_ts_ms: ts,
                });
            }
        }
        Ok(Self {
            drill_id,
            region,
            engineer_slug,
            emit_ts_ms,
            ack_ts_ms,
            ack_vector,
            mtta_ms,
            outcome,
            correlation_id: correlation_id.into(),
        })
    }
}

/// Persistence trait — host adapter binds to D1 (CF Worker) or any
/// other store. Library is pure; recorder is the I/O boundary.
pub trait DrillRecorder {
    /// Record (idempotent on `drill_id`).
    ///
    /// # Errors
    ///
    /// Adapter-defined; library callers receive
    /// [`SyntheticDrillError::Recorder`].
    fn record(&self, drill: &DrillRecord) -> Result<(), SyntheticDrillError>;

    /// Lookup most-recent drill for a region.
    ///
    /// # Errors
    ///
    /// Adapter-defined; library callers receive
    /// [`SyntheticDrillError::Recorder`].
    fn latest_for_region(&self, region: Region) -> Result<Option<DrillRecord>, SyntheticDrillError>;
}

/// In-memory recorder for tests + the cron worker fake path.
#[derive(Clone, Debug, Default)]
pub struct InMemoryDrillRecorder {
    inner: Arc<Mutex<HashMap<String, DrillRecord>>>,
}

impl InMemoryDrillRecorder {
    /// New empty recorder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of drills recorded (for testing).
    ///
    /// # Errors
    ///
    /// [`SyntheticDrillError::Internal`] on mutex poisoning.
    pub fn len(&self) -> Result<usize, SyntheticDrillError> {
        let g = self
            .inner
            .lock()
            .map_err(|e| SyntheticDrillError::Internal(format!("mutex poison: {e}")))?;
        Ok(g.len())
    }

    /// Whether the recorder is empty.
    ///
    /// # Errors
    ///
    /// [`SyntheticDrillError::Internal`] on mutex poisoning.
    pub fn is_empty(&self) -> Result<bool, SyntheticDrillError> {
        Ok(self.len()? == 0)
    }
}

impl DrillRecorder for InMemoryDrillRecorder {
    fn record(&self, drill: &DrillRecord) -> Result<(), SyntheticDrillError> {
        let mut g = self
            .inner
            .lock()
            .map_err(|e| SyntheticDrillError::Internal(format!("mutex poison: {e}")))?;
        g.insert(drill.drill_id.as_str().to_string(), drill.clone());
        Ok(())
    }

    fn latest_for_region(&self, region: Region) -> Result<Option<DrillRecord>, SyntheticDrillError> {
        let g = self
            .inner
            .lock()
            .map_err(|e| SyntheticDrillError::Internal(format!("mutex poison: {e}")))?;
        let latest = g
            .values()
            .filter(|d| d.region == region)
            .max_by_key(|d| d.emit_ts_ms)
            .cloned();
        Ok(latest)
    }
}

/// Adversarial recorder that fails every call. Used to pin the fail-
/// CLOSED audit envelope in property tests.
#[derive(Clone, Debug, Default)]
pub struct FailingDrillRecorder;

impl DrillRecorder for FailingDrillRecorder {
    fn record(&self, _drill: &DrillRecord) -> Result<(), SyntheticDrillError> {
        Err(SyntheticDrillError::Recorder(
            "adversarial fixture: record refused".to_string(),
        ))
    }

    fn latest_for_region(
        &self,
        _region: Region,
    ) -> Result<Option<DrillRecord>, SyntheticDrillError> {
        Err(SyntheticDrillError::Recorder(
            "adversarial fixture: latest refused".to_string(),
        ))
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;

    fn sample_record(id: &str, region: Region, emit: i64) -> DrillRecord {
        DrillRecord::new(
            SyntheticDrillId::new(id).unwrap(),
            region,
            "eng-001",
            emit,
            Some(emit + 60_000),
            Some(AckVector::MobilePush),
            Some(MttaMs::from_diff(emit, emit + 60_000)),
            AckOutcome::Acked,
            "corr-xyz",
        )
        .unwrap()
    }

    #[test]
    fn record_and_lookup_canonical() {
        let rec = InMemoryDrillRecorder::new();
        rec.record(&sample_record("SP-W1", Region::Americas, 1_000))
            .unwrap();
        rec.record(&sample_record("SP-W2", Region::Americas, 2_000))
            .unwrap();
        rec.record(&sample_record("SP-W3", Region::Emea, 3_000))
            .unwrap();
        assert_eq!(rec.len().unwrap(), 3);
        let latest = rec.latest_for_region(Region::Americas).unwrap().unwrap();
        assert_eq!(latest.drill_id.as_str(), "SP-W2");
    }

    #[test]
    fn ack_before_emit_rejected() {
        let r = DrillRecord::new(
            SyntheticDrillId::new("SP-X").unwrap(),
            Region::Apac,
            "eng-002",
            5_000,
            Some(1_000),
            Some(AckVector::Sms),
            Some(MttaMs::from_diff(5_000, 1_000)),
            AckOutcome::Escalated,
            "corr",
        );
        assert!(matches!(r, Err(SyntheticDrillError::AckBeforeEmit { .. })));
    }

    #[test]
    fn empty_engineer_rejected_when_acked() {
        let r = DrillRecord::new(
            SyntheticDrillId::new("SP-X").unwrap(),
            Region::Apac,
            "",
            5_000,
            Some(6_000),
            Some(AckVector::Sms),
            Some(MttaMs::from_diff(5_000, 6_000)),
            AckOutcome::Acked,
            "corr",
        );
        assert!(matches!(r, Err(SyntheticDrillError::EmptyEngineer)));
    }

    #[test]
    fn empty_engineer_allowed_when_unacked() {
        let r = DrillRecord::new(
            SyntheticDrillId::new("SP-X").unwrap(),
            Region::Apac,
            "",
            5_000,
            None,
            None,
            None,
            AckOutcome::Unacked,
            "corr",
        );
        assert!(r.is_ok());
    }

    #[test]
    fn failing_recorder_refuses_all_calls() {
        let f = FailingDrillRecorder;
        let r = f.record(&sample_record("SP-Z", Region::Apac, 1));
        assert!(matches!(r, Err(SyntheticDrillError::Recorder(_))));
        let l = f.latest_for_region(Region::Apac);
        assert!(matches!(l, Err(SyntheticDrillError::Recorder(_))));
    }
}
