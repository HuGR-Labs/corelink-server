//! `corelink-cli` library facade — exposes the CLI's pure-parsing and
//! security-sensitive surface to integration / fuzz harnesses
//! (WI-S15-006 ship-gate gate).
//!
//! The actual user-facing binary (`corelink`) lives in `main.rs`. This library
//! only re-exports modules that are deliberately fuzz-targetable:
//!
//! - [`auth`] — PAT shape validation (CTRL-CRED-001 enforcement).
//! - [`config`] — TOML config load + key apply (input parsing surface).
//! - [`error`] — error enums (verifies redaction of sensitive values in `Display`).
//!
//! Plus a small [`fuzz_api`] module with helpers that wrap the parser surface
//! in a panic-safe, allocation-light way for cargo-fuzz / property-test
//! consumers.
//!
//! Spec traceability: `specs/04_sprints/S15/work_items/WI-S15-006-*.md` §6
//! "cargo-fuzz 1M random inputs em CLI subcommands surface".

#![forbid(unsafe_code)]
#![deny(missing_docs, missing_debug_implementations)]
// Mirror main.rs allowances for the binary's intentional stdout/stderr use.
// The library surface itself does not print; these allowances are a no-op here.

pub mod auth;
pub mod config;
pub mod error;

/// Pure helpers exposed for fuzz harnesses. None of these perform network I/O
/// or touch the filesystem; they are deterministic transformations of
/// user-controlled input, suitable for libFuzzer / AFL++ targets.
pub mod fuzz_api {
    use crate::config::CorelinkConfig;
    use crate::error::ConfigError;

    /// Parse arbitrary bytes as a TOML config document. Mirrors
    /// `corelink config` load semantics without touching the filesystem.
    ///
    /// Returns `Err(ConfigError::Parse(_))` for malformed TOML; never panics.
    pub fn parse_config_toml(data: &[u8]) -> Result<CorelinkConfig, ConfigError> {
        let s = match std::str::from_utf8(data) {
            Ok(s) => s,
            // Non-UTF8 is rejected as a parse error (TOML is UTF-8 by spec).
            Err(_) => return Err(ConfigError::UnknownKey("<non-utf8>".to_owned())),
        };
        let cfg: CorelinkConfig = toml::from_str(s)?;
        Ok(cfg)
    }

    /// Apply a `key=value` pair to an in-memory config, exercising the same
    /// dispatch table the `corelink config set` command uses. Filesystem-free.
    pub fn apply_config_key(
        cfg: &mut CorelinkConfig,
        key: &str,
        value: &str,
    ) -> Result<(), ConfigError> {
        crate::config::apply_key_to_cfg(cfg, key, value)
    }

    /// Validate the structural shape of a candidate PAT, without performing
    /// any network or cryptographic check. Returns `Ok(())` iff the input
    /// matches `corelink_<env>_<token_id>.<random_secret>.<hmac_sig>`.
    pub fn validate_pat_shape(pat: &str) -> Result<(), crate::error::CliError> {
        crate::auth::validate_pat_shape(pat)
    }

    /// Scan a text buffer (typically stderr captured during a CLI run) for
    /// any substring that matches the canonical PAT shape. Used by the
    /// `secret_redaction_check` fuzz harness to assert CTRL-CRED-001
    /// (no secrets leaked via error paths).
    ///
    /// Returns the number of PAT-shaped matches found (must be 0 for
    /// fuzz invariants to hold).
    #[must_use]
    pub fn count_pat_leaks(haystack: &str) -> usize {
        let mut count = 0_usize;
        // Naive O(n*m) scan; fuzz inputs are bounded.
        for (idx, _) in haystack.match_indices("corelink_") {
            let candidate = &haystack[idx..];
            // PAT total length is 95 or 96; clip to 96 + safety margin.
            let end = candidate.len().min(128);
            let window = &candidate[..end];
            // Try increasing lengths matching the spec (95 / 96 chars).
            for needed in [95_usize, 96] {
                if window.len() >= needed {
                    let attempt = &window[..needed];
                    if validate_pat_shape(attempt).is_ok() {
                        count += 1;
                        break;
                    }
                }
            }
        }
        count
    }
}

#[cfg(test)]
mod lib_tests {
    use super::fuzz_api;

    #[test]
    fn count_pat_leaks_finds_well_formed_pat_in_text() {
        let token_id = "ABCDEFGH01234567";
        let secret = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
        let sig = "AAAAAAAAAAAAAAAAAAAAAA";
        let pat = format!("corelink_pat_{token_id}.{secret}.{sig}");
        let buf = format!("error: failed for token {pat} — see logs");
        assert_eq!(fuzz_api::count_pat_leaks(&buf), 1);
    }

    #[test]
    fn count_pat_leaks_zero_on_clean_text() {
        let buf = "error: PAT must NOT be passed as a CLI argument";
        assert_eq!(fuzz_api::count_pat_leaks(buf), 0);
    }

    #[test]
    fn parse_config_toml_rejects_garbage() {
        let r = fuzz_api::parse_config_toml(b"\x00\xff\xff not toml");
        assert!(r.is_err(), "garbage bytes must not parse as TOML");
    }

    #[test]
    fn parse_config_toml_accepts_empty() {
        let r = fuzz_api::parse_config_toml(b"");
        assert!(r.is_ok(), "empty doc should yield default config");
    }
}
