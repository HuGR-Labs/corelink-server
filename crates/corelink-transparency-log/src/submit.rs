//! The submission seam: the [`RekorSubmitter`] async trait, the fail-OPEN
//! [`witness_or_degrade`] driver, the [`WitnessOutcome`], and the
//! [`InMemoryRekor`] fake that pins the seam's invariants in CI.

use std::sync::Mutex;

use async_trait::async_trait;

use crate::entry::{RekorHashedRekord, SignedEntry};
use crate::error::TransparencyLogError;
use crate::witness::RekorWitnessRecord;

/// The submission seam to a Rekor transparency log instance.
///
/// CoreLink is a **submitter**: it hands Rekor a proposed `hashedrekord` entry
/// and receives back the recorded `{log_index, inclusion_proof}`. The single
/// binding implementation ADR-0066 leaves open is the real HTTPS client that
/// `POST`s to `https://rekor.sigstore.dev/api/v1/log/entries`; this trait is
/// the frozen seam it slots into. [`InMemoryRekor`] is the CI fake.
///
/// `async` because submission is a network call; `Send + Sync` so a single
/// submitter can be shared across the out-of-band retry workers.
#[async_trait]
pub trait RekorSubmitter: core::fmt::Debug + Send + Sync {
    /// Submit a proposed entry to the public Rekor log and return the recorded
    /// witness.
    ///
    /// # Errors
    ///
    /// - [`TransparencyLogError::Transport`] — transient availability fault
    ///   (the fail-OPEN driver degrades, never propagates, this).
    /// - [`TransparencyLogError::Rejected`] — Rekor rejected the entry
    ///   (deterministic; surfaced as `Err`).
    /// - [`TransparencyLogError::ResponseParse`] — malformed response.
    async fn submit(
        &self,
        entry: &RekorHashedRekord,
    ) -> Result<RekorWitnessRecord, TransparencyLogError>;
}

/// The outcome of attempting to witness a signed entry on the public log.
///
/// Per ADR-0066 the witness fails OPEN: an availability fault yields
/// [`WitnessOutcome::Degraded`] (queue for out-of-band retry), NEVER an error
/// that could reach a caller's write path. Only a deterministic rejection /
/// serialization bug surfaces as `Err` from [`witness_or_degrade`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WitnessOutcome {
    /// The entry was witnessed; persist the record alongside the entry.
    Witnessed(RekorWitnessRecord),
    /// Rekor was unreachable; the entry remains durable and unwitnessed.
    /// Carries the transient reason for the retry queue's diagnostics.
    Degraded(String),
}

impl WitnessOutcome {
    /// Whether the entry was witnessed.
    #[must_use]
    pub const fn is_witnessed(&self) -> bool {
        matches!(self, Self::Witnessed(_))
    }

    /// The witness record, if witnessed.
    #[must_use]
    pub const fn record(&self) -> Option<&RekorWitnessRecord> {
        match self {
            Self::Witnessed(r) => Some(r),
            Self::Degraded(_) => None,
        }
    }
}

/// Witness a signed entry on the public Rekor log, **failing OPEN** on any
/// transient transport fault (ADR-0066 §Consequences).
///
/// This is the post-hoc driver a consumer calls off the write path: it builds
/// the canonical `hashedrekord`, submits it, and maps the result so a Rekor
/// outage degrades witnessing instead of erroring. The signed entry is assumed
/// already durably logged (ADR-0065) before this is called.
///
/// # Errors
///
/// Returns `Err` only for **deterministic** faults the retry queue cannot fix:
/// [`TransparencyLogError::Rejected`] (Rekor refused the entry) or
/// [`TransparencyLogError::Serialization`] (a bug building the entry). Transient
/// transport faults are folded into [`WitnessOutcome::Degraded`].
pub async fn witness_or_degrade(
    submitter: &dyn RekorSubmitter,
    entry: &SignedEntry,
) -> WitnessOutcome {
    let body = entry.to_rekor_hashedrekord();
    match submitter.submit(&body).await {
        Ok(record) => {
            tracing::debug!(
                log_index = record.log_index,
                entry_uuid = %record.entry_uuid,
                "transparency.witness: entry witnessed on public Rekor log",
            );
            WitnessOutcome::Witnessed(record)
        }
        Err(e) if e.is_transient() => {
            tracing::warn!(
                error = %e,
                "transparency.witness: Rekor unreachable — degrading (fail-OPEN), queue for retry",
            );
            WitnessOutcome::Degraded(e.to_string())
        }
        Err(e) => {
            // Deterministic rejection / serialization bug: NOT retry-able.
            // Still fail-OPEN with respect to the caller's write path — we
            // surface a Degraded so the entry's durability is never coupled to
            // a Rekor schema disagreement; the reason is recorded for triage.
            tracing::error!(
                error = %e,
                "transparency.witness: Rekor rejected entry (non-transient) — degrading, needs triage",
            );
            WitnessOutcome::Degraded(e.to_string())
        }
    }
}

/// In-memory fake [`RekorSubmitter`] for CI — records every submitted entry,
/// hands back monotonically increasing log indices + a deterministic inclusion
/// proof, and can be armed to simulate a Rekor outage (transient) or rejection
/// (deterministic) to exercise the fail-OPEN policy.
///
/// Any real HTTPS Rekor client MUST satisfy the same contract this fake pins.
#[derive(Debug)]
pub struct InMemoryRekor {
    state: Mutex<RekorState>,
}

#[derive(Debug, Default)]
struct RekorState {
    next_index: u64,
    submitted: Vec<RekorHashedRekord>,
    fault: Option<TransparencyLogError>,
}

impl Default for InMemoryRekor {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryRekor {
    /// Construct an empty fake at log index 0.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: Mutex::new(RekorState::default()),
        }
    }

    /// Arm the next (and subsequent) submissions to fail with `fault` until
    /// [`InMemoryRekor::clear_fault`] is called.
    ///
    /// # Errors
    ///
    /// Returns [`TransparencyLogError::Transport`] if the internal mutex is
    /// poisoned (defensive — never on the happy path).
    pub fn arm_fault(&self, fault: TransparencyLogError) -> Result<(), TransparencyLogError> {
        let mut guard = self
            .state
            .lock()
            .map_err(|e| TransparencyLogError::Transport(format!("fake mutex poisoned: {e}")))?;
        guard.fault = Some(fault);
        Ok(())
    }

    /// Clear any armed fault.
    ///
    /// # Errors
    ///
    /// Returns [`TransparencyLogError::Transport`] if the internal mutex is
    /// poisoned.
    pub fn clear_fault(&self) -> Result<(), TransparencyLogError> {
        let mut guard = self
            .state
            .lock()
            .map_err(|e| TransparencyLogError::Transport(format!("fake mutex poisoned: {e}")))?;
        guard.fault = None;
        Ok(())
    }

    /// Number of entries submitted so far.
    ///
    /// # Errors
    ///
    /// Returns [`TransparencyLogError::Transport`] if the internal mutex is
    /// poisoned.
    pub fn submitted_count(&self) -> Result<usize, TransparencyLogError> {
        let guard = self
            .state
            .lock()
            .map_err(|e| TransparencyLogError::Transport(format!("fake mutex poisoned: {e}")))?;
        Ok(guard.submitted.len())
    }
}

#[async_trait]
impl RekorSubmitter for InMemoryRekor {
    async fn submit(
        &self,
        entry: &RekorHashedRekord,
    ) -> Result<RekorWitnessRecord, TransparencyLogError> {
        let mut guard = self
            .state
            .lock()
            .map_err(|e| TransparencyLogError::Transport(format!("fake mutex poisoned: {e}")))?;

        // Armed fault clones the variant (a fault is "sticky" until cleared) so
        // the fail-OPEN policy can be exercised across repeated submissions.
        if let Some(fault) = &guard.fault {
            let cloned = match fault {
                TransparencyLogError::Transport(m) => TransparencyLogError::Transport(m.clone()),
                TransparencyLogError::Rejected(m) => TransparencyLogError::Rejected(m.clone()),
                TransparencyLogError::ResponseParse(m) => {
                    TransparencyLogError::ResponseParse(m.clone())
                }
                TransparencyLogError::Serialization(m) => {
                    TransparencyLogError::Serialization(m.clone())
                }
            };
            return Err(cloned);
        }

        let index = guard.next_index;
        guard.next_index = guard.next_index.saturating_add(1);
        guard.submitted.push(entry.clone());

        // Deterministic synthetic response keyed off the content digest so the
        // record is reproducible per-entry — mirrors the public-instance shape
        // (`from_rekor_response_json` parses exactly this).
        let digest = &entry.spec.data.hash.value;
        let body = format!(
            r#"{{
              "{digest}": {{
                "logIndex": {index},
                "logID": "fake-rekor-log-id",
                "verification": {{
                  "inclusionProof": {{
                    "logIndex": {index},
                    "treeSize": {tree_size},
                    "rootHash": "{digest}",
                    "hashes": [],
                    "checkpoint": "in-memory-rekor\n{tree_size}\n{digest}\n"
                  }}
                }}
              }}
            }}"#,
            tree_size = index.saturating_add(1),
        );
        RekorWitnessRecord::from_rekor_response_json(&body, 0)
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "unit tests may use unwrap/expect/panic"
)]
mod tests {
    use super::*;

    fn sample_entry(req: &str) -> SignedEntry {
        SignedEntry::new(
            format!("{{\"request_id\":\"{req}\"}}").into_bytes(),
            "ed25519",
            b"-----BEGIN PUBLIC KEY-----\nABC\n-----END PUBLIC KEY-----\n".to_vec(),
            vec![0x09, 0x09],
        )
    }

    #[tokio::test]
    async fn witnesses_and_increments_log_index() {
        let rekor = InMemoryRekor::new();
        let a = witness_or_degrade(&rekor, &sample_entry("a")).await;
        let b = witness_or_degrade(&rekor, &sample_entry("b")).await;
        assert!(a.is_witnessed());
        assert!(b.is_witnessed());
        assert_eq!(a.record().unwrap().log_index, 0);
        assert_eq!(b.record().unwrap().log_index, 1);
        assert_eq!(rekor.submitted_count().unwrap(), 2);
    }

    #[tokio::test]
    async fn transport_fault_degrades_not_errors() {
        let rekor = InMemoryRekor::new();
        rekor
            .arm_fault(TransparencyLogError::Transport("dns timeout".into()))
            .unwrap();
        let out = witness_or_degrade(&rekor, &sample_entry("a")).await;
        assert!(!out.is_witnessed());
        match out {
            WitnessOutcome::Degraded(reason) => assert!(reason.contains("dns timeout")),
            WitnessOutcome::Witnessed(_) => panic!("should have degraded"),
        }
        // Nothing was recorded — durable entry remains unwitnessed.
        assert_eq!(rekor.submitted_count().unwrap(), 0);
    }

    #[tokio::test]
    async fn rejection_also_degrades_protecting_write_path() {
        let rekor = InMemoryRekor::new();
        rekor
            .arm_fault(TransparencyLogError::Rejected("bad signature".into()))
            .unwrap();
        let out = witness_or_degrade(&rekor, &sample_entry("a")).await;
        // Even a deterministic rejection must not couple to the write path.
        assert!(matches!(out, WitnessOutcome::Degraded(_)));
    }

    #[tokio::test]
    async fn recovers_after_fault_cleared() {
        let rekor = InMemoryRekor::new();
        rekor
            .arm_fault(TransparencyLogError::Transport("offline".into()))
            .unwrap();
        assert!(!witness_or_degrade(&rekor, &sample_entry("a")).await.is_witnessed());
        rekor.clear_fault().unwrap();
        assert!(witness_or_degrade(&rekor, &sample_entry("a")).await.is_witnessed());
    }
}
