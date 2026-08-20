//! F3.2 increment 3 — the digest-pinned, owner-gated **public-base allowlist**
//! trust root.
//!
//! The `_public` namespace ([`crate::adapter_cache::PUBLIC_NAMESPACE`]) dedups
//! content-addressed blobs across EVERY tenant, so the set of upstream layers
//! eligible to enter it is a security boundary (design BLOCKER-3 / Control 1+2 in
//! `docs/design/2026-08-13-f3-cross-tenant-public-layer-cache.md`). This module
//! is that boundary's read side: it loads the owner-curated
//! [`MANIFEST`](self::MANIFEST) — **baked into the container binary** — validates
//! every active entry is an immutable `sha256:` digest (TAGS BANNED, fail-CLOSED
//! on any violation), and answers [`PublicBaseAllowlist::is_allowlisted`].
//!
//! **Owner-gated by construction:** there is NO runtime mutation path. The only
//! way to change the allowlist is a reviewed code commit + a redeploy, so a
//! compromised token / D1 write cannot widen what may enter `_public` (design
//! Contradiction-2). The read side is consumed by the server-only public mirror
//! ([`crate::routes::public_mirror`]), which promotes a digest into `_public`
//! only if it `is_allowlisted`. **WP-E Roll-1:** the baked manifest carries
//! exactly the alpine pin (no longer deny-all); the mirror can now populate
//! `_public` for alpine while client dedup stays OFF, honouring the
//! server-populates-first order (design GAP-E).

use std::collections::HashSet;

/// The owner-curated allowlist manifest, embedded at build time. The comment
/// header documents the digest-pinned / populate-then-allowlist governance; this
/// module enforces only the machine-checkable half (well-formed digests).
const MANIFEST: &str = include_str!("public_base_allowlist.manifest");

/// Mandatory prefix on every entry: allowlist rows are UPSTREAM OCI CONTENT
/// DIGESTS, never tags.
const DIGEST_PREFIX: &str = "sha256:";

/// A sha256 hex digest is exactly 64 hex characters.
const DIGEST_HEX_LEN: usize = 64;

/// The parsed, validated public-base allowlist: an immutable set of
/// `sha256:<64hex>` upstream digests eligible to reside in `_public`.
#[derive(Debug, Clone, Default)]
pub struct PublicBaseAllowlist {
    digests: HashSet<String>,
}

impl PublicBaseAllowlist {
    /// Load + validate the baked-in [`MANIFEST`]. **Fail-CLOSED:** a single
    /// malformed active entry (a tag, a truncated/upper-case hex, a missing
    /// `sha256:` prefix) makes the WHOLE load return `Err`, so a caller must
    /// refuse to arm the public-base router rather than trust a partially-valid
    /// allowlist.
    ///
    /// # Errors
    /// Returns the first offending line's diagnostic if any active entry is not
    /// a canonical `sha256:<64 lowercase hex>` digest.
    pub fn from_baked_manifest() -> Result<Self, String> {
        Self::parse(MANIFEST)
    }

    /// Parse + validate an allowlist manifest body. Exposed for tests; the
    /// production path is [`Self::from_baked_manifest`].
    ///
    /// # Errors
    /// See [`Self::from_baked_manifest`].
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut digests = HashSet::new();
        for (idx, raw) in text.lines().enumerate() {
            // A `#` starts a comment (whole-line or trailing); keep only what
            // precedes it, then trim.
            let line = raw.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            // Only the leading token is the digest; the rest is human provenance.
            let token = line.split_whitespace().next().unwrap_or("");
            validate_digest(token)
                .map_err(|e| format!("public-base-allowlist: line {}: {e}", idx + 1))?;
            digests.insert(token.to_owned());
        }
        Ok(Self { digests })
    }

    /// Is this upstream OCI content digest eligible for the `_public` namespace?
    /// The argument is an upstream digest string (`sha256:<hex>`); the check is a
    /// constant-set membership test. Deny by default (empty allowlist ⇒ always
    /// `false`).
    #[must_use]
    pub fn is_allowlisted(&self, oci_digest: &str) -> bool {
        self.digests.contains(oci_digest)
    }

    /// Number of active allowlisted digests.
    #[must_use]
    pub fn len(&self) -> usize {
        self.digests.len()
    }

    /// Whether the allowlist is empty (the deny-all default state).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.digests.is_empty()
    }
}

/// Validate a single token is a canonical `sha256:<64 lowercase hex>` digest.
/// This is the machine-enforced half of "digest-pinned, never tag-resolved":
/// a tag (`ubuntu:22.04`) has no `sha256:` prefix / wrong length / non-hex
/// chars, so it is rejected here.
fn validate_digest(token: &str) -> Result<(), String> {
    let hex = token.strip_prefix(DIGEST_PREFIX).ok_or_else(|| {
        format!(
            "entry '{token}' is not a 'sha256:' digest — TAGS ARE BANNED, pin the immutable digest"
        )
    })?;
    if hex.len() != DIGEST_HEX_LEN {
        return Err(format!(
            "entry '{token}' has a {}-char hash, expected {DIGEST_HEX_LEN}-hex sha256",
            hex.len()
        ));
    }
    // Lowercase hex only — the canonical OCI digest form. Rejecting upper-case
    // keeps membership a single unambiguous string compare (no case folding).
    if let Some(bad) = hex
        .bytes()
        .find(|b| !b.is_ascii_hexdigit() || b.is_ascii_uppercase())
    {
        return Err(format!(
            "entry '{token}' has a non-lowercase-hex character '{}'",
            char::from(bad)
        ));
    }
    Ok(())
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;

    const REAL_ALPINE: &str =
        "sha256:25f1d6b1951ac8eb3740558fe94cb83d377bdadf95fd9f98b50d2e1b96130471";
    const REAL_DEBIAN: &str =
        "sha256:039e6f9f9752f74a3ff4a6a224f64c7c864da16ed98f882107704328f41b9c42";

    // WP-G M3 base-image INDEX pins (multi-arch manifest-list digests), resolved
    // 2026-08-19. These gate the M2 `_public` closure-promote (server-side,
    // upstream-verified), NOT the client-push layer route.
    const IDX_ALPINE: &str =
        "sha256:d9e853e87e55526f6b2917df91a2115c36dd7c696a35be12163d44e6e2a4b6bc";
    const IDX_DEBIAN12: &str =
        "sha256:813017f3d62be4b5891a7acca6a01bdcd4b8513daa81b1ab99d3a50385b26931";
    const IDX_UBUNTU2404: &str =
        "sha256:33ceb71981b602c1a7443a53469e4dba065f7503eab3078a2d7a57a2ab987517";
    const IDX_NODE22SLIM: &str =
        "sha256:d649c27dae7ba0137b3cef5dd75baa422c08dc3d9e3fc0c23dfb172dc3cc6436";
    const IDX_PYTHON312SLIM: &str =
        "sha256:2c941e860699f878900b0edc2403613c234d4b32eda3cc9fa7036991a2a63c4a";

    #[test]
    fn baked_manifest_has_m3_pins_active() {
        // WP-G M3: the shipped trust root MUST parse (no malformed active entry)
        // and carries the alpine WP-E LAYER pin plus the 5 base-image INDEX pins.
        let al =
            PublicBaseAllowlist::from_baked_manifest().expect("baked manifest must be well-formed");
        assert!(!al.is_empty(), "the shipped allowlist is not deny-all");
        assert_eq!(
            al.len(),
            6,
            "alpine LAYER + 5 base-image INDEX pins after M3; had {}",
            al.len()
        );
        assert!(
            al.is_allowlisted(REAL_ALPINE),
            "alpine WP-E layer pin still active"
        );
        for (name, idx) in [
            ("alpine", IDX_ALPINE),
            ("debian12", IDX_DEBIAN12),
            ("ubuntu24.04", IDX_UBUNTU2404),
            ("node22-slim", IDX_NODE22SLIM),
            ("python3.12-slim", IDX_PYTHON312SLIM),
        ] {
            assert!(al.is_allowlisted(idx), "{name} index pin must be active");
        }
        assert!(
            !al.is_allowlisted(REAL_DEBIAN),
            "the old bookworm-slim LAYER example stays a non-active example (never a layer pin)"
        );
    }

    #[test]
    fn accepts_pinned_digests_and_ignores_comments_and_blanks() {
        let manifest = format!(
            "# header comment\n\n  {REAL_ALPINE}   docker.io/library/alpine:3.20\n\
             {REAL_DEBIAN} # trailing provenance comment\n\n"
        );
        let al = PublicBaseAllowlist::parse(&manifest).expect("valid digests parse");
        assert_eq!(al.len(), 2);
        assert!(al.is_allowlisted(REAL_ALPINE));
        assert!(al.is_allowlisted(REAL_DEBIAN));
        assert!(
            !al.is_allowlisted(
                "sha256:0000000000000000000000000000000000000000000000000000000000000000"
            ),
            "an un-listed digest is not allowlisted"
        );
    }

    #[test]
    fn rejects_a_tag_fail_closed() {
        // The core security property: a TAG can never slip into the trust root.
        let err = PublicBaseAllowlist::parse("ubuntu:22.04   a mutable tag\n")
            .expect_err("a tag must be rejected");
        assert!(
            err.contains("TAGS ARE BANNED"),
            "diagnostic names the rule: {err}"
        );
    }

    #[test]
    fn rejects_truncated_and_uppercase_and_nonhex() {
        assert!(
            PublicBaseAllowlist::parse("sha256:deadbeef\n").is_err(),
            "short hex rejected"
        );
        assert!(
            PublicBaseAllowlist::parse(&format!(
                "sha256:{}\n",
                "25F1D6B1951AC8EB3740558FE94CB83D377BDADF95FD9F98B50D2E1B96130471A"
            ))
            .is_err(),
            "uppercase hex rejected (canonical form is lowercase)"
        );
        assert!(
            PublicBaseAllowlist::parse(&format!("sha256:{}\n", "z".repeat(64))).is_err(),
            "non-hex rejected"
        );
    }

    #[test]
    fn one_bad_entry_fails_the_whole_load() {
        // Fail-CLOSED: a valid entry followed by a tag still errors — no partial
        // allowlist is trusted.
        let manifest = format!("{REAL_ALPINE}\nlatest\n");
        assert!(
            PublicBaseAllowlist::parse(&manifest).is_err(),
            "a single malformed entry must fail the entire load"
        );
    }
}
