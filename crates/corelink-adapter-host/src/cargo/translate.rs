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

/// Normalize an sccache webdav key from the URL path into a safe storage key.
///
/// The real sccache HTTP backend sends TWO key shapes, not just one:
///   1. 64-hex BLAKE3 object keys (the build artifacts); and
///   2. NON-hex control keys — notably `.sccache_check`, the startup health
///      probe sccache PUTs+GETs to verify the backend before using it.
///
/// The original impl assumed every key was 64-hex and `None`-rejected everything
/// else → the cargo route 400'd `.sccache_check` → sccache deemed the backend
/// unusable and disabled it (real client never worked, even though hex round-
/// trips did). Since the cargo surface now content-addresses the BYTES via the
/// 2-level [`crate::cargo`] → MoatCache (the key is ONLY the per-tenant url-map
/// lookup label, never the CAS digest), the key no longer has to be a digest. It
/// must only be a SAFE, bounded, single path segment.
///
/// # Errors
///
/// Returns `None` for empty, over-long (>256), traversal (`.`/`..`/contains `/`),
/// or non-graphic keys.
#[must_use]
pub fn normalize_key(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() || raw.len() > 256 {
        return None;
    }
    // Single safe path segment: no traversal, no separators.
    if raw == "." || raw == ".." || raw.contains('/') {
        return None;
    }
    // sccache's vocabulary: hex digests + control keys like `.sccache_check`.
    if !raw
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
    {
        return None;
    }
    // Canonicalize the 64-hex object keys to lowercase (sccache emits lowercase;
    // defensive). Control keys (e.g. `.sccache_check`) are preserved verbatim so
    // the same key round-trips on the subsequent read.
    if raw.len() == DIGEST_HEX_LEN && raw.bytes().all(|b| b.is_ascii_hexdigit()) {
        Some(raw.to_ascii_lowercase())
    } else {
        Some(raw.to_owned())
    }
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
    fn normalize_accepts_sccache_health_probe_verbatim() {
        // The real sccache client PUTs/GETs `.sccache_check` on startup; the surface
        // MUST accept it (verbatim, so it round-trips) or sccache disables the cache.
        assert_eq!(
            normalize_key(".sccache_check"),
            Some(".sccache_check".to_owned())
        );
    }

    #[test]
    fn normalize_accepts_short_and_nonhex_safe_keys() {
        // The key is now the per-tenant url-map label (bytes are content-addressed),
        // so short / non-hex sccache keys are valid as long as they are safe.
        assert_eq!(normalize_key("deadbeef"), Some("deadbeef".to_owned()));
        let z = "z".repeat(DIGEST_HEX_LEN);
        assert_eq!(normalize_key(&z), Some(z));
    }

    #[test]
    fn normalize_rejects_unsafe_keys() {
        assert_eq!(normalize_key(""), None); // empty
        assert_eq!(normalize_key("."), None); // self
        assert_eq!(normalize_key(".."), None); // traversal
        assert_eq!(normalize_key("a/b"), None); // separator
        assert_eq!(normalize_key("../secret"), None); // traversal path
        assert_eq!(normalize_key("k ey"), None); // space (not in the safe set)
        assert_eq!(normalize_key(&"a".repeat(257)), None); // over-long
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
    fn key_from_path_rejects_empty_and_traversal() {
        assert_eq!(key_from_path("/"), None); // empty last segment
        assert_eq!(key_from_path("/.."), None); // traversal
    }

    #[test]
    fn normalize_is_idempotent() {
        let k = valid_key();
        let once = normalize_key(&k).unwrap();
        let twice = normalize_key(&once).unwrap();
        assert_eq!(once, twice);
    }
}
