//! Versioned audit-chain link algorithms and forward-only epoch state.
//!
//! The original audit chain is already deployed with algorithm `0`:
//! `BLAKE3(prev_hash || canonical_jcs)`.  This module makes the cutover
//! explicit instead of changing that formula in place.  Algorithm `1` is a
//! domain-separated keyed BLAKE3 link and may only be used by an epoch whose
//! predecessor is an earlier epoch in the same partition.
//!
//! A link key is deliberately an in-memory value.  It has no `Serialize`
//! implementation and its `Debug` representation is redacted.  Registries
//! should persist only the commitment and key id; the key itself must come
//! from a write-only deployment secret/keyring.

use core::fmt;
use std::collections::BTreeMap;

use blake3::Hasher;
use zeroize::{Zeroize, Zeroizing};

use crate::chain::link_chain_hash_from_canonical;
use crate::event::ChainHash;

/// Historical, deployed unkeyed BLAKE3 link algorithm.
pub const UNKEYED_ALGORITHM_ID: u8 = 0;
/// Domain-separated keyed BLAKE3 link algorithm.
pub const KEYED_ALGORITHM_ID: u8 = 1;
/// Exact domain separator for [`KEYED_ALGORITHM_ID`], including its NUL byte.
pub const KEYED_LINK_DOMAIN: &[u8] = b"corelink/audit-chain/link/v2\0";
/// Domain separator for durable link-key commitments.
pub const LINK_KEY_COMMITMENT_DOMAIN: &[u8] = b"corelink/audit-chain/link-key-commitment/v1\0";

/// Algorithm selected for one immutable chain epoch.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum LinkAlgorithm {
    /// Existing rows and epoch zero.  Never requires a key.
    UnkeyedV1,
    /// New epochs only.  Requires the epoch's registered key.
    KeyedV2,
}

impl LinkAlgorithm {
    /// Durable numeric identifier used by D1/archive rows.
    #[must_use]
    pub const fn id(self) -> u8 {
        match self {
            Self::UnkeyedV1 => UNKEYED_ALGORITHM_ID,
            Self::KeyedV2 => KEYED_ALGORITHM_ID,
        }
    }

    /// Parse a durable identifier.  Unknown versions fail closed.
    pub const fn from_id(id: u8) -> Result<Self, EpochError> {
        match id {
            UNKEYED_ALGORITHM_ID => Ok(Self::UnkeyedV1),
            KEYED_ALGORITHM_ID => Ok(Self::KeyedV2),
            other => Err(EpochError::UnknownAlgorithm { id: other }),
        }
    }
}

/// An in-memory 32-byte link key.  Only its commitment belongs in durable
/// state; this type intentionally cannot be serialized or displayed.
#[non_exhaustive]
pub struct LinkKey(Zeroizing<[u8; 32]>);

impl LinkKey {
    /// Construct from exactly 32 secret bytes.
    #[must_use]
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(Zeroizing::new(bytes))
    }

    /// Borrow the key for the BLAKE3 keyed primitive.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Compute the public, domain-separated commitment stored in the key
    /// registry.  The key bytes never appear in the returned value or logs.
    #[must_use]
    pub fn commitment(&self) -> ChainHash {
        let mut hasher = Hasher::new();
        hasher.update(LINK_KEY_COMMITMENT_DOMAIN);
        hasher.update(self.as_bytes());
        ChainHash(*hasher.finalize().as_bytes())
    }
}

impl Clone for LinkKey {
    fn clone(&self) -> Self {
        Self::from_bytes(*self.as_bytes())
    }
}

impl fmt::Debug for LinkKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("LinkKey(<redacted>)")
    }
}

/// Write-only deployment keyring indexed by the registered `link_key_id`.
/// Key bytes never appear in serialized state or debug output and are wiped on
/// drop. The signed epoch/registry runtime remains responsible for validating
/// the selected id and commitment before cutover.
#[derive(Clone, Default)]
pub struct LinkKeyring(BTreeMap<u64, LinkKey>);

/// Fail-closed errors for the JSON keyring boundary.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum LinkKeyringError {
    #[error("audit-chain keyring must be a JSON object of link-key-id to hex key")]
    InvalidShape,
    #[error("audit-chain keyring contains invalid link-key id {0}")]
    InvalidKeyId(String),
    #[error("audit-chain keyring entry {0} is not 64 lower-case hex characters")]
    InvalidKey(String),
    #[error("audit-chain keyring must contain at least one key")]
    Empty,
}

impl LinkKeyring {
    /// Parse the write-only JSON representation atomically. Any malformed
    /// entry rejects the whole keyring; a partial map must never fall back to
    /// the legacy unkeyed epoch.
    pub fn parse_json(raw: &str) -> Result<Self, LinkKeyringError> {
        let entries = serde_json::from_str::<BTreeMap<String, String>>(raw)
            .map_err(|_| LinkKeyringError::InvalidShape)?;
        if entries.is_empty() {
            return Err(LinkKeyringError::Empty);
        }
        let mut keyring = Self::default();
        for (id_text, hex_key) in entries {
            let id = id_text
                .parse::<u64>()
                .ok()
                .filter(|id| *id > 0)
                .ok_or_else(|| LinkKeyringError::InvalidKeyId(id_text.clone()))?;
            if hex_key.len() != 64
                || !hex_key
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            {
                return Err(LinkKeyringError::InvalidKey(id_text));
            }
            let mut bytes = Zeroizing::new([0_u8; 32]);
            hex::decode_to_slice(&hex_key, bytes.as_mut())
                .map_err(|_| LinkKeyringError::InvalidKey(id_text.clone()))?;
            if keyring.0.insert(id, LinkKey::from_bytes(*bytes)).is_some() {
                return Err(LinkKeyringError::InvalidKey(id_text));
            }
        }
        Ok(keyring)
    }

    #[must_use]
    pub fn get(&self, link_key_id: u64) -> Option<&LinkKey> {
        self.0.get(&link_key_id)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Debug for LinkKeyring {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LinkKeyring")
            .field("key_count", &self.len())
            .finish()
    }
}

/// Errors raised by versioned link/epoch operations.  Every error is
/// fail-closed: callers must leave rows unsealed and must not report a clean
/// verification result.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum EpochError {
    /// A persisted algorithm id is not known to this binary.
    #[error("unknown audit-chain link algorithm id {id}")]
    UnknownAlgorithm {
        /// Unknown durable algorithm identifier.
        id: u8,
    },
    /// A keyed link was requested without key material.
    #[error("keyed audit-chain link requires link-key material")]
    MissingKey,
    /// An unkeyed link was supplied key material, which is a downgrade/shape
    /// mismatch rather than something to silently ignore.
    #[error("unkeyed audit-chain epoch must not carry a link key")]
    UnexpectedKey,
    /// Key id and key material commitment do not agree with the registry.
    #[error("link-key id/commitment does not match the configured key")]
    KeyCommitmentMismatch,
    /// Epoch zero has a fixed legacy shape.
    #[error("epoch zero must be the unkeyed genesis epoch")]
    InvalidGenesisEpoch,
    /// A successor epoch was not a strict forward transition.
    #[error("invalid audit-chain epoch transition: {reason}")]
    InvalidTransition {
        /// Stable local reason for the rejected transition.
        reason: &'static str,
    },
    /// Appending a link did not start at the current state.
    #[error(
        "audit-chain append state mismatch: expected sequence {expected_sequence}, observed {observed_sequence}"
    )]
    StateMismatch {
        /// Sequence required by the active state.
        expected_sequence: u64,
        /// Sequence supplied by the caller.
        observed_sequence: u64,
    },
}

/// One immutable `(tenant, region)` epoch descriptor.  The surrounding
/// runtime persists these facts in the signed epoch ledger/projection; the
/// pure type enforces the local invariants before any write is attempted.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ChainEpoch {
    epoch_id: u64,
    algorithm: LinkAlgorithm,
    link_key_id: Option<u64>,
    start_sequence: u64,
    start_prev_hash: ChainHash,
    predecessor_epoch_id: Option<u64>,
}

impl ChainEpoch {
    /// Construct the explicit legacy E0 epoch.  This describes existing
    /// history; it does not assert that the history is empty.
    #[must_use]
    pub const fn legacy() -> Self {
        Self {
            epoch_id: 0,
            algorithm: LinkAlgorithm::UnkeyedV1,
            link_key_id: None,
            start_sequence: 0,
            start_prev_hash: ChainHash::genesis(),
            predecessor_epoch_id: None,
        }
    }

    /// Construct a keyed successor epoch at the exact current boundary.
    pub fn keyed_successor(
        epoch_id: u64,
        link_key_id: u64,
        start_sequence: u64,
        start_prev_hash: ChainHash,
        predecessor_epoch_id: u64,
    ) -> Result<Self, EpochError> {
        if epoch_id == 0 || link_key_id == 0 || predecessor_epoch_id != epoch_id - 1 {
            return Err(EpochError::InvalidTransition {
                reason: "keyed successor must have positive ids and immediate predecessor",
            });
        }
        Ok(Self {
            epoch_id,
            algorithm: LinkAlgorithm::KeyedV2,
            link_key_id: Some(link_key_id),
            start_sequence,
            start_prev_hash,
            predecessor_epoch_id: Some(predecessor_epoch_id),
        })
    }

    #[must_use]
    /// Return the immutable epoch id.
    pub const fn epoch_id(&self) -> u64 {
        self.epoch_id
    }

    #[must_use]
    /// Return the link algorithm selected by this epoch.
    pub const fn algorithm(&self) -> LinkAlgorithm {
        self.algorithm
    }

    #[must_use]
    /// Return the registered key id, if this is a keyed epoch.
    pub const fn link_key_id(&self) -> Option<u64> {
        self.link_key_id
    }

    #[must_use]
    /// Return the first sequence covered by this epoch.
    pub const fn start_sequence(&self) -> u64 {
        self.start_sequence
    }

    #[must_use]
    /// Return the predecessor head at this epoch boundary.
    pub const fn start_prev_hash(&self) -> &ChainHash {
        &self.start_prev_hash
    }

    #[must_use]
    /// Return the immediately preceding epoch id, if any.
    pub const fn predecessor_epoch_id(&self) -> Option<u64> {
        self.predecessor_epoch_id
    }

    /// Validate a descriptor loaded from a durable epoch projection.
    pub fn validate(&self) -> Result<(), EpochError> {
        match (
            self.epoch_id,
            self.algorithm,
            self.link_key_id,
            self.predecessor_epoch_id,
        ) {
            (0, LinkAlgorithm::UnkeyedV1, None, None)
                if self.start_sequence == 0 && self.start_prev_hash == ChainHash::genesis() =>
            {
                Ok(())
            }
            (0, _, _, _) => Err(EpochError::InvalidGenesisEpoch),
            (id, LinkAlgorithm::KeyedV2, Some(key_id), Some(pred))
                if id > 0 && key_id > 0 && pred == id - 1 =>
            {
                Ok(())
            }
            (_, _, _, _) => Err(EpochError::InvalidTransition {
                reason: "non-genesis epoch must be keyed with an immediate predecessor",
            }),
        }
    }

    /// Validate that `successor` begins exactly where `self` ended and is a
    /// strict forward keyed epoch.  The predecessor's closed end is supplied
    /// explicitly by the caller because it is a cross-row durable fact.
    pub fn validate_successor(
        &self,
        successor: &Self,
        predecessor_end_sequence: u64,
        predecessor_end_head: &ChainHash,
    ) -> Result<(), EpochError> {
        self.validate()?;
        successor.validate()?;
        if successor.epoch_id != self.epoch_id.saturating_add(1)
            || successor.predecessor_epoch_id != Some(self.epoch_id)
            || successor.start_sequence != predecessor_end_sequence
            || successor.start_prev_hash != *predecessor_end_head
            || successor.algorithm != LinkAlgorithm::KeyedV2
        {
            return Err(EpochError::InvalidTransition {
                reason: "successor boundary/algorithm is not forward and contiguous",
            });
        }
        Ok(())
    }
}

/// Compute one link under an explicitly selected epoch algorithm.
pub fn link_for_epoch(
    epoch: &ChainEpoch,
    prev_hash: &ChainHash,
    canonical_jcs: &[u8],
    key: Option<&LinkKey>,
) -> Result<ChainHash, EpochError> {
    epoch.validate()?;
    match (epoch.algorithm, key) {
        (LinkAlgorithm::UnkeyedV1, None) => {
            Ok(link_chain_hash_from_canonical(prev_hash, canonical_jcs))
        }
        (LinkAlgorithm::UnkeyedV1, Some(_)) => Err(EpochError::UnexpectedKey),
        (LinkAlgorithm::KeyedV2, None) => Err(EpochError::MissingKey),
        (LinkAlgorithm::KeyedV2, Some(link_key)) => {
            let mut hasher = Hasher::new_keyed(link_key.as_bytes());
            hasher.update(KEYED_LINK_DOMAIN);
            hasher.update(prev_hash.as_bytes());
            hasher.update(canonical_jcs);
            Ok(ChainHash(*hasher.finalize().as_bytes()))
        }
    }
}

/// Compute the durable commitment used to match key material to a registered
/// key id before sealing/verifying.  The commitment is safe to persist.
#[must_use]
pub fn link_key_commitment(key: &LinkKey) -> ChainHash {
    key.commitment()
}

/// Check configured key material against the commitment authenticated by the
/// epoch/key registry.  A false result must abort sealing or verification;
/// callers must not try another key or silently use E0.
#[must_use]
pub fn key_matches_commitment(key: &LinkKey, expected: &ChainHash) -> bool {
    key.commitment() == *expected
}

/// Mutable pure-logic state machine for one active epoch.  It cannot rewind,
/// relabel a v1 segment, or transition without the exact current boundary.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct EpochChainState {
    epoch: ChainEpoch,
    head: ChainHash,
    next_sequence: u64,
}

impl EpochChainState {
    /// Start at an epoch's declared boundary.
    pub fn new(epoch: ChainEpoch) -> Result<Self, EpochError> {
        epoch.validate()?;
        Ok(Self {
            head: *epoch.start_prev_hash(),
            next_sequence: epoch.start_sequence(),
            epoch,
        })
    }

    #[must_use]
    /// Return the active epoch descriptor.
    pub const fn epoch(&self) -> &ChainEpoch {
        &self.epoch
    }

    #[must_use]
    /// Return the current chain head.
    pub const fn head(&self) -> &ChainHash {
        &self.head
    }

    #[must_use]
    /// Return the next sequence number to append.
    pub const fn next_sequence(&self) -> u64 {
        self.next_sequence
    }

    /// Append canonical bytes and advance exactly one sequence position.
    pub fn append(
        &mut self,
        sequence: u64,
        prev_hash: &ChainHash,
        canonical_jcs: &[u8],
        key: Option<&LinkKey>,
    ) -> Result<ChainHash, EpochError> {
        if sequence != self.next_sequence || *prev_hash != self.head {
            return Err(EpochError::StateMismatch {
                expected_sequence: self.next_sequence,
                observed_sequence: sequence,
            });
        }
        let next = link_for_epoch(&self.epoch, prev_hash, canonical_jcs, key)?;
        self.head = next;
        self.next_sequence = self.next_sequence.saturating_add(1);
        Ok(next)
    }

    /// Open a keyed successor only at this state's current head/sequence.
    pub fn transition(self, successor: ChainEpoch) -> Result<Self, EpochError> {
        self.epoch
            .validate_successor(&successor, self.next_sequence, &self.head)?;
        Self::new(successor)
    }
}

// Keep the secret zeroing guarantee explicit if the implementation changes
// the backing type later; `Zeroizing` currently performs this on drop.
#[allow(dead_code)]
fn _zeroize_link_key(key: &mut LinkKey) {
    key.0.zeroize();
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests intentionally exercise fail-closed error paths"
)]
mod tests {
    use super::*;

    const KEY_BYTES: [u8; 32] = [0x42; 32];

    #[test]
    fn legacy_formula_remains_byte_for_byte_unchanged() {
        let epoch = ChainEpoch::legacy();
        let canonical = br#"{"event":"old"}"#;
        let got = link_for_epoch(&epoch, &ChainHash::genesis(), canonical, None).unwrap();
        let expected = link_chain_hash_from_canonical(&ChainHash::genesis(), canonical);
        assert_eq!(got, expected);
    }

    #[test]
    fn keyed_formula_is_domain_separated_and_requires_key() {
        let epoch = ChainEpoch::keyed_successor(1, 7, 3, ChainHash([1; 32]), 0).unwrap();
        let key = LinkKey::from_bytes(KEY_BYTES);
        let canonical = br#"{"event":"new"}"#;
        let got = link_for_epoch(&epoch, epoch.start_prev_hash(), canonical, Some(&key)).unwrap();
        let mut h = Hasher::new_keyed(&KEY_BYTES);
        h.update(KEYED_LINK_DOMAIN);
        h.update(epoch.start_prev_hash().as_bytes());
        h.update(canonical);
        assert_eq!(got, ChainHash(*h.finalize().as_bytes()));
        assert_eq!(
            link_for_epoch(&epoch, epoch.start_prev_hash(), canonical, None),
            Err(EpochError::MissingKey)
        );
    }

    #[test]
    fn unknown_version_and_downgrade_fail_closed() {
        assert_eq!(
            LinkAlgorithm::from_id(99),
            Err(EpochError::UnknownAlgorithm { id: 99 })
        );
        let legacy = ChainEpoch::legacy();
        let key = LinkKey::from_bytes(KEY_BYTES);
        assert_eq!(
            link_for_epoch(&legacy, &ChainHash::genesis(), b"x", Some(&key)),
            Err(EpochError::UnexpectedKey)
        );
    }

    #[test]
    fn transition_is_forward_only_and_preserves_old_head() {
        let mut state = EpochChainState::new(ChainEpoch::legacy()).unwrap();
        let old_head = state
            .append(0, &ChainHash::genesis(), b"legacy", None)
            .unwrap();
        let successor = ChainEpoch::keyed_successor(1, 7, 1, old_head, 0).unwrap();
        let mut keyed = state.transition(successor).unwrap();
        let key = LinkKey::from_bytes(KEY_BYTES);
        let new_head = keyed.append(1, &old_head, b"keyed", Some(&key)).unwrap();
        assert_ne!(new_head, old_head);
        assert_eq!(keyed.epoch().algorithm(), LinkAlgorithm::KeyedV2);
        assert_eq!(keyed.epoch().start_sequence(), 1);
    }

    #[test]
    fn wrong_boundary_and_cross_partition_style_predecessor_are_rejected() {
        let previous = ChainEpoch::legacy();
        let wrong = ChainEpoch::keyed_successor(1, 7, 0, ChainHash::genesis(), 0).unwrap();
        assert_eq!(
            previous.validate_successor(&wrong, 4, &ChainHash([2; 32])),
            Err(EpochError::InvalidTransition {
                reason: "successor boundary/algorithm is not forward and contiguous"
            })
        );
        assert!(ChainEpoch::keyed_successor(2, 7, 1, ChainHash::genesis(), 0).is_err());
    }

    #[test]
    fn key_commitment_is_stable_but_debug_never_exposes_key() {
        let key = LinkKey::from_bytes(KEY_BYTES);
        assert_eq!(key.commitment(), link_key_commitment(&key));
        assert!(key_matches_commitment(&key, &key.commitment()));
        assert!(!key_matches_commitment(&key, &ChainHash([0; 32])));
        let debug = format!("{key:?}");
        assert!(debug.contains("redacted"));
        assert!(!debug.contains("42"));
    }

    #[test]
    fn keyring_parsing_is_atomic_and_redacted() {
        let keyring = LinkKeyring::parse_json(&format!(r#"{{"7":"{}"}}"#, "ab".repeat(32)))
            .expect("valid link-key id map");
        assert_eq!(keyring.len(), 1);
        assert!(keyring.get(7).is_some());
        assert!(format!("{keyring:?}").contains("key_count"));
        assert!(!format!("{keyring:?}").contains("abab"));
        assert!(matches!(
            LinkKeyring::parse_json(r#"{"7":"not-hex"}"#),
            Err(LinkKeyringError::InvalidKey(_))
        ));
        assert!(matches!(
            LinkKeyring::parse_json(
                r#"{"0":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}"#
            ),
            Err(LinkKeyringError::InvalidKeyId(_))
        ));
        assert!(matches!(
            LinkKeyring::parse_json(
                r#"{"7":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"}"#
            ),
            Err(LinkKeyringError::InvalidKey(_))
        ));
    }
}
