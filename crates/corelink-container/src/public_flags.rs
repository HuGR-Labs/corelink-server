//! Boot-read feature flags for the F3.2 cross-tenant public OCI base-layer
//! cache (increment-6 activation).
//!
//! These are read ONCE, at process start, and injected as plain `bool`s into
//! the stores that consume them (e.g. [`crate::routes::oci`]'s `OciMoatStore`).
//! A running Durable-Object container does NOT pick up an env change without a
//! fresh image roll (an env-only redeploy never restarts a live container), so
//! **activation is a repin, never an env flip** — the campaign's WP-E ships the
//! new pin with the flag baked ON.

/// Env var gating cross-tenant `_public` routing of allowlisted OCI base
/// layers (increment 6). Absent / anything but `"1"` ⇒ disabled.
const OCI_PUBLIC_DEDUP_ENV: &str = "OCI_PUBLIC_DEDUP_ENABLED";

/// Is cross-tenant `_public` routing of allowlisted OCI base layers enabled?
///
/// Default **OFF** (fail-safe): the flag must be exactly `"1"` to enable. Every
/// other state — unset, empty, `"0"`, `"true"`, whitespace — is `false`, so a
/// typo can never silently turn cross-tenant sharing on. Boot-read; see the
/// module doc on why activation is a repin, not a live env flip.
#[must_use]
pub fn oci_public_dedup_enabled() -> bool {
    std::env::var(OCI_PUBLIC_DEDUP_ENV).ok().as_deref() == Some("1")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_off_semantics_are_exact() {
        // Pure string logic — assert the enable set is exactly {"1"} without
        // mutating process env (parallel-test-safe): mirror the check.
        let enabled = |v: Option<&str>| v == Some("1");
        assert!(enabled(Some("1")));
        for off in [
            None,
            Some(""),
            Some("0"),
            Some("true"),
            Some(" 1"),
            Some("1 "),
        ] {
            assert!(!enabled(off), "value {off:?} must be OFF");
        }
    }

    #[test]
    fn reads_the_documented_env_var_name() {
        assert_eq!(OCI_PUBLIC_DEDUP_ENV, "OCI_PUBLIC_DEDUP_ENABLED");
        // Whatever the ambient env is, the reader must not panic.
        let _ = oci_public_dedup_enabled();
    }
}
