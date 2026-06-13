//! [`RekorWitnessRecord`] — the `{log_index, inclusion_proof}` CoreLink records
//! alongside a witnessed entry (ADR-0066 §Consequences), plus the parser that
//! lifts a raw Rekor `LogEntry` response into it.

use serde::{Deserialize, Serialize};

use crate::error::TransparencyLogError;

/// The public-witness handle CoreLink persists next to a witnessed entry.
///
/// This is the durable proof that "entry X is in the public Rekor log at index
/// N with inclusion proof P". A relying party fetches this record + the entry,
/// then independently re-verifies the inclusion proof against the public Rekor
/// checkpoint — *without trusting CoreLink*. That is the transparency property
/// ADR-0066 buys: verifiable AGAINST CoreLink, not VIA CoreLink.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RekorWitnessRecord {
    /// Rekor log UUID (the `entries` map key in the API response) — the stable
    /// handle to fetch this entry from the public log.
    pub entry_uuid: String,
    /// The integer index of this entry in the public Rekor transparency log.
    pub log_index: u64,
    /// The Rekor log ID (the log's public-key hash, `logID`) — identifies
    /// *which* Rekor instance witnessed the entry.
    pub log_id: String,
    /// The Merkle inclusion proof returned by Rekor.
    pub inclusion_proof: InclusionProof,
    /// Millisecond timestamp at which CoreLink recorded the witness (the
    /// submitter-observed instant; distinct from Rekor's `integratedTime`).
    pub witnessed_at_ms: u64,
}

/// The Merkle inclusion proof Rekor returns for a witnessed entry — enough to
/// recompute the log's `root_hash` from the entry's leaf and the audited
/// checkpoint, offline.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InclusionProof {
    /// The leaf index of the entry within the tree (Rekor `logIndex` within
    /// the proof; may differ from the global `log_index` on sharded logs).
    pub checkpoint_log_index: u64,
    /// Total tree size at the time the proof was generated.
    pub tree_size: u64,
    /// The Merkle root hash (hex) the proof recomputes to.
    pub root_hash: String,
    /// Ordered sibling hashes (hex) on the path from the leaf to the root.
    pub hashes: Vec<String>,
    /// The signed Rekor checkpoint (note) binding the `root_hash` at
    /// `tree_size` — the public anchor a verifier checks the proof against.
    pub checkpoint: String,
}

impl RekorWitnessRecord {
    /// Parse a raw Rekor `POST /api/v1/log/entries` (or `GET .../{uuid}`)
    /// response body into a witness record.
    ///
    /// The Rekor response is a JSON object keyed by entry UUID, each value
    /// carrying `logIndex`, `logID`, and a `verification.inclusionProof`. We
    /// take the single entry CoreLink submitted.
    ///
    /// # Errors
    ///
    /// Returns [`TransparencyLogError::ResponseParse`] when the body is not the
    /// expected shape, carries zero entries, or is missing the inclusion proof
    /// (Rekor returns the proof on submission for the public good instance).
    pub fn from_rekor_response_json(
        body: &str,
        witnessed_at_ms: u64,
    ) -> Result<Self, TransparencyLogError> {
        let value: serde_json::Value = serde_json::from_str(body)
            .map_err(|e| TransparencyLogError::ResponseParse(e.to_string()))?;
        let entries = value
            .as_object()
            .ok_or_else(|| TransparencyLogError::ResponseParse("response is not an object".into()))?;
        let (uuid, entry) = entries.iter().next().ok_or_else(|| {
            TransparencyLogError::ResponseParse("response contains zero log entries".into())
        })?;

        let log_index = entry
            .get("logIndex")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| TransparencyLogError::ResponseParse("missing logIndex".into()))?;
        let log_id = entry
            .get("logID")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| TransparencyLogError::ResponseParse("missing logID".into()))?
            .to_owned();

        let proof_value = entry
            .get("verification")
            .and_then(|v| v.get("inclusionProof"))
            .ok_or_else(|| {
                TransparencyLogError::ResponseParse("missing verification.inclusionProof".into())
            })?;
        let inclusion_proof = InclusionProof::from_rekor_value(proof_value)?;

        Ok(Self {
            entry_uuid: uuid.clone(),
            log_index,
            log_id,
            inclusion_proof,
            witnessed_at_ms,
        })
    }
}

impl InclusionProof {
    /// Parse the `verification.inclusionProof` sub-object of a Rekor response.
    ///
    /// # Errors
    ///
    /// Returns [`TransparencyLogError::ResponseParse`] when required proof
    /// fields (`logIndex`, `treeSize`, `rootHash`, `hashes`, `checkpoint`) are
    /// absent or malformed.
    pub fn from_rekor_value(
        value: &serde_json::Value,
    ) -> Result<Self, TransparencyLogError> {
        let checkpoint_log_index = value
            .get("logIndex")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| {
                TransparencyLogError::ResponseParse("inclusionProof missing logIndex".into())
            })?;
        let tree_size = value
            .get("treeSize")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| {
                TransparencyLogError::ResponseParse("inclusionProof missing treeSize".into())
            })?;
        let root_hash = value
            .get("rootHash")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                TransparencyLogError::ResponseParse("inclusionProof missing rootHash".into())
            })?
            .to_owned();
        let hashes_arr = value
            .get("hashes")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| {
                TransparencyLogError::ResponseParse("inclusionProof missing hashes".into())
            })?;
        let mut hashes = Vec::with_capacity(hashes_arr.len());
        for h in hashes_arr {
            let s = h.as_str().ok_or_else(|| {
                TransparencyLogError::ResponseParse("inclusionProof hash is not a string".into())
            })?;
            hashes.push(s.to_owned());
        }
        let checkpoint = value
            .get("checkpoint")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                TransparencyLogError::ResponseParse("inclusionProof missing checkpoint".into())
            })?
            .to_owned();

        Ok(Self {
            checkpoint_log_index,
            tree_size,
            root_hash,
            hashes,
            checkpoint,
        })
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "unit tests may use unwrap/expect"
)]
mod tests {
    use super::*;

    fn sample_response(log_index: u64) -> String {
        format!(
            r#"{{
              "uuid-abc": {{
                "logIndex": {log_index},
                "logID": "c0d23d6ad406973f9559f3ba2d1ca01f84147d8ffc5b8445c224f98b9591801d",
                "verification": {{
                  "inclusionProof": {{
                    "logIndex": {log_index},
                    "treeSize": 9001,
                    "rootHash": "deadbeef",
                    "hashes": ["aa11", "bb22"],
                    "checkpoint": "rekor.sigstore.dev\n9001\nzm9v\n"
                  }}
                }}
              }}
            }}"#
        )
    }

    #[test]
    fn parses_well_formed_response() {
        let rec = RekorWitnessRecord::from_rekor_response_json(&sample_response(42), 1_700_000).unwrap();
        assert_eq!(rec.log_index, 42);
        assert_eq!(rec.entry_uuid, "uuid-abc");
        assert_eq!(rec.witnessed_at_ms, 1_700_000);
        assert_eq!(rec.inclusion_proof.tree_size, 9001);
        assert_eq!(rec.inclusion_proof.hashes, vec!["aa11", "bb22"]);
        assert_eq!(rec.inclusion_proof.root_hash, "deadbeef");
    }

    #[test]
    fn rejects_empty_object() {
        let err = RekorWitnessRecord::from_rekor_response_json("{}", 0).unwrap_err();
        assert!(matches!(err, TransparencyLogError::ResponseParse(_)));
    }

    #[test]
    fn rejects_missing_inclusion_proof() {
        let body = r#"{"u":{"logIndex":1,"logID":"x"}}"#;
        let err = RekorWitnessRecord::from_rekor_response_json(body, 0).unwrap_err();
        assert!(matches!(err, TransparencyLogError::ResponseParse(_)));
    }

    #[test]
    fn rejects_non_json() {
        let err = RekorWitnessRecord::from_rekor_response_json("not json", 0).unwrap_err();
        assert!(matches!(err, TransparencyLogError::ResponseParse(_)));
    }
}
