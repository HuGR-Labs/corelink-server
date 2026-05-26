//! sccache key ↔ CoreLink CAS digest translation.
//!
//! sccache's HTTP storage backend identifies build artifacts by a key
//! derived from `blake3(rustc-cmdline + input-fingerprints)`. The key
//! is transmitted in the URL path as a hex string.
//!
//! CoreLink's CAS also uses BLAKE3 for content-addressing. The alignment
//! is exact: sccache's 32-byte BLAKE3 digest, rendered as 64-char
//! lowercase hex, IS a valid CoreLink CAS key. Zero translation cost.
//!
//! ## Invariants
//!
//! - A valid sccache key is exactly 64 lowercase hex characters.
//! - `normalize_key(k)` lowercases the input (sccache emits lowercase;
//!   we normalize defensively so the property tests can range over
//!   arbitrary ASCII hex).
//! - Roundtrip: `normalize_key(normalize_key(k)) == normalize_key(k)`.

/// Canonical key length (64 hex chars = 32 bytes BLAKE3).
pub const DIGEST_HEX_LEN: usize = 64;

/// Normalize a sccache key from the URL path into a canonical lowercase
/// hex string. Returns `None` if the key is not exactly 64 hex chars
/// (upper or lower case).
///
/// # Errors
///
/// Returns `None` for keys that are not valid 64-char hex strings.
#[must_use]
pub fn normalize_key(raw: &str) -> Option<String> {
    if raw.len() != DIGEST_HEX_LEN {
        return None;
    }
    if !raw.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    Some(raw.to_ascii_lowercase())
}

/// Extract the sccache key from a URL path segment (strips a leading
/// `/` if present). Returns `None` for malformed or missing keys.
#[must_use]
pub fn key_from_path(path: &str) -> Option<String> {
    let stripped = path.strip_prefix('/').unwrap_or(path);
    // Only the last path segment is the key (sccache sends `/<key>`).
    let segment = stripped.rsplit('/').next().unwrap_or(stripped);
    normalize_key(segment)
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::uninlined_format_args
)]
mod tests {
    use super::*;

    fn valid_key() -> String {
        "a".repeat(DIGEST_HEX_LEN)
    }

    #[test]
    fn normalize_accepts_64_hex_lowercase() {
        let k = valid_key();
        assert_eq!(normalize_key(&k), Some(k));
    }

    #[test]
    fn normalize_accepts_64_hex_uppercase() {
        let upper = "A".repeat(DIGEST_HEX_LEN);
        let lower = "a".repeat(DIGEST_HEX_LEN);
        assert_eq!(normalize_key(&upper), Some(lower));
    }

    #[test]
    fn normalize_rejects_too_short() {
        assert_eq!(normalize_key("deadbeef"), None);
    }

    #[test]
    fn normalize_rejects_non_hex() {
        let bad = "z".repeat(DIGEST_HEX_LEN);
        assert_eq!(normalize_key(&bad), None);
    }

    #[test]
    fn normalize_rejects_too_long() {
        let long = "a".repeat(DIGEST_HEX_LEN + 1);
        assert_eq!(normalize_key(&long), None);
    }

    #[test]
    fn key_from_path_strips_leading_slash() {
        let k = valid_key();
        assert_eq!(key_from_path(&format!("/{k}")), Some(k));
    }

    #[test]
    fn key_from_path_without_slash() {
        let k = valid_key();
        assert_eq!(key_from_path(&k), Some(k));
    }

    #[test]
    fn key_from_path_rejects_invalid() {
        assert_eq!(key_from_path("/short"), None);
    }

    #[test]
    fn normalize_is_idempotent() {
        let k = valid_key();
        let once = normalize_key(&k).unwrap();
        let twice = normalize_key(&once).unwrap();
        assert_eq!(once, twice);
    }
}
