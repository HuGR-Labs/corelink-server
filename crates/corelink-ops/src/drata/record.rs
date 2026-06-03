//! Canonical evidence record envelope + SHA-256 idempotency hash.
//!
//! Every record materialises into the same envelope shape regardless of
//! source. Metadata is a `BTreeMap` so JSON serialisation orders keys
//! deterministically — without this, two semantically identical records
//! would hash differently and idempotency would break.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::stream::EvidenceStream;

/// Canonical evidence envelope POSTed to Drata.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceRecord {
    /// Target Drata stream.
    pub stream: EvidenceStream,
    /// Source-system identifier (e.g. `audit-outbox#42`, `pat#pat_xxx`,
    /// `gh-pr#123`). Opaque to Drata; meaningful only inside the
    /// CoreLink audit chain.
    pub source_id: String,
    /// Event timestamp (ms since epoch).
    pub occurred_at_ms: i64,
    /// Metadata key/value pairs — CTRL-PRIV-001 forbids PII; callers
    /// pass hashes / opaque ids only.
    pub metadata: BTreeMap<String, String>,
}

impl EvidenceRecord {
    /// Construct a record. `metadata` may be any iterable of key/value
    /// pairs; it will be normalised into a `BTreeMap` to keep hashing
    /// deterministic.
    pub fn new<I, K, V>(
        stream: EvidenceStream,
        source_id: impl Into<String>,
        occurred_at_ms: i64,
        metadata: I,
    ) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        let metadata = metadata
            .into_iter()
            .map(|(k, v)| (k.into(), v.into()))
            .collect::<BTreeMap<_, _>>();
        Self {
            stream,
            source_id: source_id.into(),
            occurred_at_ms,
            metadata,
        }
    }

    /// Stable JSON serialisation used for hashing + transport. Returns
    /// an error only if a metadata value is somehow not JSON-encodable
    /// (cannot happen for `BTreeMap<String, String>` but the function
    /// surfaces a `serde_json::Error` so callers don't have to
    /// `unwrap`).
    ///
    /// # Errors
    ///
    /// Propagates `serde_json::Error` from the encoder.
    pub fn to_canonical_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}

/// SHA-256 hex digest of the canonical JSON encoding. Used as both the
/// idempotency-ledger key and the `Idempotency-Key` HTTP header value
/// on the POST to Drata.
///
/// # Errors
///
/// Propagates the `serde_json::Error` from
/// [`EvidenceRecord::to_canonical_json`].
pub fn record_sha256(record: &EvidenceRecord) -> Result<String, serde_json::Error> {
    let json = record.to_canonical_json()?;
    let mut hasher = Sha256::new();
    hasher.update(json.as_bytes());
    let digest = hasher.finalize();
    Ok(hex::encode(digest))
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

    fn fixture() -> EvidenceRecord {
        EvidenceRecord::new(
            EvidenceStream::AuditLogs,
            "audit-outbox#1",
            1_700_000_000_000,
            [
                ("event_type", "corelink.auth.signin"),
                ("tenant_hash", "abcd1234"),
            ],
        )
    }

    #[test]
    fn hash_is_deterministic() {
        let a = record_sha256(&fixture()).unwrap();
        let b = record_sha256(&fixture()).unwrap();
        assert_eq!(a, b);
        assert_eq!(a.len(), 64); // sha256 hex
    }

    #[test]
    fn hash_changes_with_payload() {
        let mut r = fixture();
        r.source_id = "audit-outbox#2".into();
        assert_ne!(
            record_sha256(&r).unwrap(),
            record_sha256(&fixture()).unwrap()
        );
    }

    #[test]
    fn metadata_key_order_does_not_affect_hash() {
        let r1 = EvidenceRecord::new(EvidenceStream::AuditLogs, "x", 0, [("a", "1"), ("b", "2")]);
        let r2 = EvidenceRecord::new(EvidenceStream::AuditLogs, "x", 0, [("b", "2"), ("a", "1")]);
        assert_eq!(record_sha256(&r1).unwrap(), record_sha256(&r2).unwrap());
    }

    #[test]
    fn canonical_json_round_trips() {
        let r = fixture();
        let j = r.to_canonical_json().unwrap();
        let r2: EvidenceRecord = serde_json::from_str(&j).unwrap();
        assert_eq!(r, r2);
    }
}
