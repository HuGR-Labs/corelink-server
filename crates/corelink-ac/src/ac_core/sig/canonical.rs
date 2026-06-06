//! Canonical 121-byte AC envelope preimage layout (ADR-0021 §3 +
//! Lote 10.4-tris P0-R5-003).
//!
//! Layout (big-endian, fixed 121 bytes):
//!
//! ```text
//! offset  size  field
//!   0       1   version (u8)
//!   1       4   sig_key_id (u32 BE)
//!   5      16   tenant_id (UUIDv7 raw bytes)
//!  21      32   action_digest_hash (BLAKE3-256 bytes)
//!  53       8   action_digest_size_bytes (i64 BE)
//!  61      32   result_hash (BLAKE3-256 of canonical ActionResult
//!                proto bytes per ADR-0037 v1.1.0; binds the FULL
//!                ActionResult shape — Merkle outputs + stdout/stderr
//!                + exit_code + execution_metadata — to the index
//!                column. NOT BLAKE3(merkle_root). See ADR-0037 §1.1.0
//!                amendment.)
//!  93       4   sig_key_id_repeat (u32 BE; binds the rotation
//!                version into both the salt and the body so a
//!                cross-version replay cannot pass verify)
//!  97      24   reserved padding (zeroed)
//!  total  121
//! ```
//!
//! This shape mirrors `corelink-worker::reapi::ac::sig::AcEnvelope::canonicalize`
//! 1:1 — a `const _: () = assert!(...)` parity test in the canonical
//! vectors integration test pins the byte-for-byte equivalence so a
//! refactor on either side that drifts the offsets is caught at
//! compile time.

use uuid::Uuid;

use super::error::SigError;
use super::RESERVED_SIG_KEY_ID;

/// Canonical preimage byte length. Pin per ADR-0021 v1.0.0 §3.
pub const AC_ENVELOPE_PREIMAGE_LEN: usize = 121;

/// Canonical preimage version byte (`v1`).
pub const AC_ENVELOPE_PREIMAGE_VERSION: u8 = 1;

// Layout offsets — `const` so the slice indexing arithmetic is
// provably in-range. Compile-time `assert!` below enforces that the
// sum of field widths equals [`AC_ENVELOPE_PREIMAGE_LEN`].
const VERSION_OFFSET: usize = 0;
const SIG_KEY_ID_OFFSET: usize = 1;
const TENANT_ID_OFFSET: usize = 5;
const ACTION_HASH_OFFSET: usize = 21;
const ACTION_SIZE_OFFSET: usize = 53;
const RESULT_HASH_OFFSET: usize = 61;
const SIG_KEY_ID_REPEAT_OFFSET: usize = 93;
// Used only in const assertions below; rust 1.88 clippy treats const
// assert!() consumers as "unused" → annotate to preserve documentation
// value of the layout offsets without triggering dead_code.
#[allow(dead_code)]
const PAD_OFFSET: usize = 97;
#[allow(dead_code)]
const PAD_LEN: usize = 24;

const _: () = assert!(AC_ENVELOPE_PREIMAGE_LEN == 1 + 4 + 16 + 32 + 8 + 32 + 4 + 24);
const _: () = assert!(PAD_OFFSET + PAD_LEN == AC_ENVELOPE_PREIMAGE_LEN);
const _: () = assert!(SIG_KEY_ID_REPEAT_OFFSET + 4 == PAD_OFFSET);

/// Compose the canonical 121-byte AC envelope preimage from its
/// components.
///
/// # Errors
///
/// - [`SigError::KeyIdReserved`] when `sig_key_id == 0` — the
///   sentinel value per ADR-0021 §Sentinel.
#[allow(
    clippy::indexing_slicing,
    reason = "all slice indices are compile-time constants asserted above against AC_ENVELOPE_PREIMAGE_LEN; bounds are provably in-range"
)]
pub fn compose(
    version: u8,
    sig_key_id: u32,
    tenant_id: Uuid,
    action_digest_hash: &[u8; 32],
    action_digest_size_bytes: i64,
    result_hash: &[u8; 32],
) -> Result<[u8; AC_ENVELOPE_PREIMAGE_LEN], SigError> {
    if sig_key_id == RESERVED_SIG_KEY_ID {
        return Err(SigError::KeyIdReserved);
    }
    let mut buf = [0u8; AC_ENVELOPE_PREIMAGE_LEN];
    buf[VERSION_OFFSET] = version;
    buf[SIG_KEY_ID_OFFSET..SIG_KEY_ID_OFFSET + 4].copy_from_slice(&sig_key_id.to_be_bytes());
    buf[TENANT_ID_OFFSET..TENANT_ID_OFFSET + 16].copy_from_slice(tenant_id.as_bytes());
    buf[ACTION_HASH_OFFSET..ACTION_HASH_OFFSET + 32].copy_from_slice(action_digest_hash);
    buf[ACTION_SIZE_OFFSET..ACTION_SIZE_OFFSET + 8]
        .copy_from_slice(&action_digest_size_bytes.to_be_bytes());
    buf[RESULT_HASH_OFFSET..RESULT_HASH_OFFSET + 32].copy_from_slice(result_hash);
    buf[SIG_KEY_ID_REPEAT_OFFSET..SIG_KEY_ID_REPEAT_OFFSET + 4]
        .copy_from_slice(&sig_key_id.to_be_bytes());
    // Padding window stays zero-initialized.
    Ok(buf)
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code: panics surface as test failures by design"
)]
mod tests {
    use super::*;

    #[test]
    fn preimage_length_is_121_bytes() {
        let bytes = compose(1, 42, Uuid::nil(), &[0xAB; 32], 1234, &[0xCD; 32]).unwrap();
        assert_eq!(bytes.len(), AC_ENVELOPE_PREIMAGE_LEN);
        assert_eq!(bytes[0], 1);
        assert_eq!(&bytes[1..5], &42_u32.to_be_bytes());
    }

    #[test]
    fn reserved_sig_key_id_rejected() {
        let err = compose(
            1,
            RESERVED_SIG_KEY_ID,
            Uuid::nil(),
            &[0xAB; 32],
            1234,
            &[0xCD; 32],
        )
        .unwrap_err();
        assert_eq!(err, SigError::KeyIdReserved);
    }

    #[test]
    fn fields_at_canonical_offsets() {
        let tenant = Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap();
        let mut action_hash = [0u8; 32];
        for (i, b) in action_hash.iter_mut().enumerate() {
            *b = u8::try_from(i).unwrap();
        }
        let mut result_hash = [0u8; 32];
        for (i, b) in result_hash.iter_mut().enumerate() {
            *b = u8::try_from(i).unwrap_or(0).wrapping_add(0x80);
        }
        let bytes = compose(1, 7, tenant, &action_hash, 9999, &result_hash).unwrap();
        // Version
        assert_eq!(bytes[0], 1);
        // sig_key_id BE
        assert_eq!(&bytes[1..5], &7_u32.to_be_bytes());
        // Tenant
        assert_eq!(&bytes[5..21], tenant.as_bytes());
        // Action hash
        assert_eq!(&bytes[21..53], &action_hash);
        // Action size BE
        assert_eq!(&bytes[53..61], &9999_i64.to_be_bytes());
        // Result hash
        assert_eq!(&bytes[61..93], &result_hash);
        // sig_key_id repeat
        assert_eq!(&bytes[93..97], &7_u32.to_be_bytes());
        // Padding
        assert_eq!(&bytes[97..121], &[0u8; 24]);
    }

    #[test]
    fn distinct_inputs_yield_distinct_preimages() {
        let a = compose(1, 1, Uuid::nil(), &[0; 32], 0, &[0; 32]).unwrap();
        let b = compose(1, 2, Uuid::nil(), &[0; 32], 0, &[0; 32]).unwrap();
        assert_ne!(a, b);
        let c = compose(1, 1, Uuid::nil(), &[0; 32], 0, &[1; 32]).unwrap();
        assert_ne!(a, c);
    }
}
