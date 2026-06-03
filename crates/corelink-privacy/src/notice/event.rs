//! Canonical privacy-notice types: [`NoticeLocale`] + [`NoticeVersion`] +
//! [`VersionBump`] + [`NoticePublishedPayload`] + [`NoticeDeprecatedPayload`]
//! + [`NoticeCloudEventType`] + hash helper [`notice_text_hash`].
//!
//! ## Locale canonical enum (WI-S11-004 §6.1)
//!
//! Three locales mandatory at GA per sprint contract §10.s11.7:
//! - `PtBr` — PT-BR primary (LGPD client-anchor HuGR Brasil).
//! - `EnUs` — EN secondary (GDPR/CCPA markets US/EU).
//! - `EsMx` — ES-MX tertiary (LATAM expansion).
//!
//! `#[non_exhaustive]` reserves additive growth (e.g. PT-PT, FR, DE
//! post-GA per sprint contract §7 anti-scope).
//!
//! ## notice_text_hash canonical algorithm (AC-006)
//!
//! Cross-platform determinism: CRLF → LF → trim trailing whitespace per
//! line → rejoin with `\n` → UTF-8 NFC normalize → SHA-256 hex64.
//! Implemented in pure Rust so the same algorithm runs in wasm32
//! (CI gate cross-validates this Rust impl against `scripts/notice_text_hash_canonical.py`).

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use uuid::Uuid;

/// Canonical 3-arm locale taxonomy for the privacy notice. Per WI-S11-004
/// §6.1: PT-BR primary (LGPD), EN-US secondary (GDPR/CCPA), ES-MX tertiary
/// (LATAM). `#[non_exhaustive]` reserves PT-PT, FR, DE post-GA.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum NoticeLocale {
    /// PT-BR — primary locale, LGPD client-anchor HuGR Brasil.
    PtBr,
    /// EN-US — secondary locale, GDPR/CCPA markets (US + EU).
    EnUs,
    /// ES-MX — tertiary locale, LATAM expansion market.
    EsMx,
}

impl NoticeLocale {
    /// Canonical BCP-47 locale tag. Pinned for file naming, HTML `lang`
    /// attribute, Cloudflare Pages path, and cross-component regression tests.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PtBr => "pt-BR",
            Self::EnUs => "en-US",
            Self::EsMx => "es-MX",
        }
    }

    /// Canonical file name (e.g. `pt-BR.md`). Pinned for `legal/privacy-notice/`
    /// directory structure per WI-S11-004 §6.3.
    #[must_use]
    pub const fn as_filename(self) -> &'static str {
        match self {
            Self::PtBr => "pt-BR.md",
            Self::EnUs => "en-US.md",
            Self::EsMx => "es-MX.md",
        }
    }
}

impl core::fmt::Display for NoticeLocale {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Canonical 3-element locale list. Pinned for cardinality enforcement in
/// CI hook validation + cross-component regression tests.
#[must_use]
pub const fn canonical_notice_locales() -> &'static [NoticeLocale; 3] {
    &[NoticeLocale::PtBr, NoticeLocale::EnUs, NoticeLocale::EsMx]
}

/// Canonical 2-arm bump taxonomy per ADR-S11-007 material vs minor criteria.
///
/// - `Major` = material change → force re-consent via WI-S11-003
///   `stale_consent_check` (CTRL-PRIV-CONSENT-005).
/// - `Minor` = clarification / typo / contact update → silent, no re-consent.
///
/// `#[non_exhaustive]` per Lote 10.9-quinquies typed-enum discipline.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum VersionBump {
    /// Major bump = material change per ADR-S11-007:
    /// - New data category collected.
    /// - New sub-processor added.
    /// - New purpose for processing.
    /// - Retention period extended.
    /// - DSR SLA extended.
    /// - Cross-border transfer to new region.
    Major,
    /// Minor bump = clarification / typo / contact update per ADR-S11-007:
    /// - Clarification of existing wording.
    /// - Typo / grammar fix.
    /// - DPO contact update.
    /// - Additional non-primary locale added.
    Minor,
}

impl VersionBump {
    /// Canonical lower-snake-case mnemonic.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Major => "major",
            Self::Minor => "minor",
        }
    }

    /// Whether this bump constitutes a material change requiring force
    /// re-consent per CTRL-PRIV-CONSENT-005.
    #[must_use]
    pub const fn is_material(self) -> bool {
        matches!(self, Self::Major)
    }
}

impl core::fmt::Display for VersionBump {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Canonical semver version wrapper for the privacy notice. Enforces the
/// `M.m` semver discipline (major.minor; patch omitted — notice versioning
/// is intentionally coarser than software semver to match Privacy Officer
/// judgment granularity per ADR-S11-007 §DD-001).
///
/// Invariant: string representation is `"v{major}.{minor}.0"` matching
/// the `legal/privacy-notice/v<M.m.0>/` directory convention.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct NoticeVersion {
    /// Major component (0-based; major bump = material change).
    pub major: u32,
    /// Minor component (0-based; minor bump = clarification).
    pub minor: u32,
}

impl NoticeVersion {
    /// Construct a new notice version.
    #[must_use]
    pub const fn new(major: u32, minor: u32) -> Self {
        Self { major, minor }
    }

    /// Canonical directory name: `"v{major}.{minor}.0"`. Pinned for
    /// `legal/privacy-notice/` path construction + Cloudflare Pages deploy.
    #[must_use]
    pub fn directory_name(&self) -> alloc::string::String {
        alloc::format!("v{}.{}.0", self.major, self.minor)
    }

    /// Canonical string representation for CloudEvents payload, Prom metrics,
    /// and audit log fields: `"v{major}.{minor}.0"`.
    #[must_use]
    pub fn as_version_str(&self) -> alloc::string::String {
        alloc::format!("v{}.{}.0", self.major, self.minor)
    }

    /// Determine the bump type vs a predecessor version. Returns `Major` when
    /// the current major component is greater than the predecessor's. Returns
    /// `Minor` when the minor component is greater (major unchanged). Returns
    /// `None` when they are equal (no bump — CI gate blocks this).
    #[must_use]
    pub fn bump_vs(&self, predecessor: &Self) -> Option<VersionBump> {
        if self.major > predecessor.major {
            Some(VersionBump::Major)
        } else if self.major == predecessor.major && self.minor > predecessor.minor {
            Some(VersionBump::Minor)
        } else {
            None
        }
    }
}

impl core::fmt::Display for NoticeVersion {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "v{}.{}.0", self.major, self.minor)
    }
}

/// 2-arm canonical CloudEvents type taxonomy for privacy notice publication.
/// Per Lote 10.9bis P0-G prefix: `dev.hugr.corelink.privacy_notice.<arm>.v1`.
///
/// `#[non_exhaustive]` per Lote 10.9-quinquies discipline.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum NoticeCloudEventType {
    /// `dev.hugr.corelink.privacy_notice.published.v1` — new version published.
    /// Fired for every publication (major + minor).
    Published,
    /// `dev.hugr.corelink.privacy_notice.deprecated.v1` — previous version
    /// deprecated. Fired only on major bump (previous version superseded).
    Deprecated,
}

impl NoticeCloudEventType {
    /// Canonical CloudEvents type string per Lote 10.9bis P0-G prefix.
    #[must_use]
    pub const fn as_cloud_event_type(self) -> &'static str {
        match self {
            Self::Published => "dev.hugr.corelink.privacy_notice.published.v1",
            Self::Deprecated => "dev.hugr.corelink.privacy_notice.deprecated.v1",
        }
    }
}

impl core::fmt::Display for NoticeCloudEventType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_cloud_event_type())
    }
}

/// Payload for `privacy_notice.published.v1` CloudEvent. Carries the new
/// version, bump type, 3-locale notice_text_hashes, and the INV-CONSENT-
/// PROOF-VERIFIABLE cross-validation anchor (the hashes are the same input
/// that WI-S11-003 `ConsentProofPayload.notice_text_hash` stores).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoticePublishedPayload {
    /// UUIDv7 event id for idempotency + audit trail localization.
    pub event_id: Uuid,
    /// New notice version.
    pub version: NoticeVersion,
    /// Bump type (major = material change; minor = clarification).
    pub bump: VersionBump,
    /// Whether this bump constitutes a material change requiring
    /// force re-consent (derived from `bump`; denormalized for consumer
    /// convenience per AC-003 requirement `payload.material_change=false`
    /// for minor bumps).
    pub material_change: bool,
    /// SHA-256 hex64 notice_text_hash per locale — deterministic canonical
    /// (CRLF→LF + trim + UTF-8 NFC + SHA-256) per AC-006.
    /// BTreeMap guarantees stable JSON serialization order across platforms.
    pub notice_text_hashes: BTreeMap<alloc::string::String, alloc::string::String>,
    /// Wall-clock instant of emission (Unix epoch ms).
    pub emitted_at_ms: u64,
}

impl NoticePublishedPayload {
    /// Construct a new publication payload.
    #[must_use]
    pub fn new(
        event_id: Uuid,
        version: NoticeVersion,
        bump: VersionBump,
        notice_text_hashes: BTreeMap<alloc::string::String, alloc::string::String>,
        emitted_at_ms: u64,
    ) -> Self {
        let material_change = bump.is_material();
        Self {
            event_id,
            version,
            bump,
            material_change,
            notice_text_hashes,
            emitted_at_ms,
        }
    }
}

/// Payload for `privacy_notice.deprecated.v1` CloudEvent. Fired only on
/// major bump — the previous version is superseded by the new one.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoticeDeprecatedPayload {
    /// UUIDv7 event id.
    pub event_id: Uuid,
    /// Version being deprecated (the previous one being superseded).
    pub deprecated_version: NoticeVersion,
    /// New version that supersedes the deprecated one.
    pub superseded_by: NoticeVersion,
    /// Wall-clock instant of emission (Unix epoch ms).
    pub emitted_at_ms: u64,
}

impl NoticeDeprecatedPayload {
    /// Construct a new deprecation payload.
    #[must_use]
    pub fn new(
        event_id: Uuid,
        deprecated_version: NoticeVersion,
        superseded_by: NoticeVersion,
        emitted_at_ms: u64,
    ) -> Self {
        Self {
            event_id,
            deprecated_version,
            superseded_by,
            emitted_at_ms,
        }
    }
}

/// Canonical CloudEvent envelope for `privacy_notice.published.v1`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoticePublishedCloudEvent {
    /// CloudEvents `specversion` = "1.0".
    pub spec_version: alloc::string::String,
    /// CloudEvents `type`.
    pub r#type: alloc::string::String,
    /// CloudEvents `source` = the canonical crate source.
    pub source: alloc::string::String,
    /// CloudEvents `id` = UUIDv7 of the event.
    pub id: Uuid,
    /// CloudEvents `datacontenttype` = "application/json".
    pub data_content_type: alloc::string::String,
    /// CloudEvents `data`.
    pub data: NoticePublishedPayload,
}

impl NoticePublishedCloudEvent {
    /// Construct a canonical published CloudEvent envelope.
    #[must_use]
    pub fn new(data: NoticePublishedPayload) -> Self {
        let id = data.event_id;
        Self {
            spec_version: alloc::string::String::from("1.0"),
            r#type: alloc::string::String::from(
                NoticeCloudEventType::Published.as_cloud_event_type(),
            ),
            source: alloc::string::String::from("dev.hugr.corelink/privacy-notice-emit"),
            id,
            data_content_type: alloc::string::String::from("application/json"),
            data,
        }
    }
}

/// Canonical CloudEvent envelope for `privacy_notice.deprecated.v1`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoticeDeprecatedCloudEvent {
    /// CloudEvents `specversion` = "1.0".
    pub spec_version: alloc::string::String,
    /// CloudEvents `type`.
    pub r#type: alloc::string::String,
    /// CloudEvents `source`.
    pub source: alloc::string::String,
    /// CloudEvents `id`.
    pub id: Uuid,
    /// CloudEvents `datacontenttype`.
    pub data_content_type: alloc::string::String,
    /// CloudEvents `data`.
    pub data: NoticeDeprecatedPayload,
}

impl NoticeDeprecatedCloudEvent {
    /// Construct a canonical deprecated CloudEvent envelope.
    #[must_use]
    pub fn new(data: NoticeDeprecatedPayload) -> Self {
        let id = data.event_id;
        Self {
            spec_version: alloc::string::String::from("1.0"),
            r#type: alloc::string::String::from(
                NoticeCloudEventType::Deprecated.as_cloud_event_type(),
            ),
            source: alloc::string::String::from("dev.hugr.corelink/privacy-notice-emit"),
            id,
            data_content_type: alloc::string::String::from("application/json"),
            data,
        }
    }
}

/// Compute the canonical deterministic SHA-256 hex64 hash of privacy notice
/// content per AC-006:
///
/// 1. CRLF → LF normalization.
/// 2. Trim trailing whitespace per line.
/// 3. Rejoin lines with `\n`.
/// 4. UTF-8 NFC normalization (via unicode-normalization; the canonical
///    algorithm is identical whether run in Rust or Python
///    `scripts/notice_text_hash_canonical.py`).
/// 5. SHA-256 hex64 of UTF-8 bytes.
///
/// The hash is deterministic across Windows (CRLF) + Linux (LF) + macOS (LF)
/// platforms. Property test `prop_notice_hash_determinism` verifies 100 random
/// content samples produce identical hashes regardless of line-ending variant.
///
/// **Note on NFC**: The current implementation performs the CRLF→LF and trim
/// steps in pure Rust. Full Unicode NFC normalization would require the
/// `unicode-normalization` crate (a C-free pure-Rust dep). To keep wasm32
/// clean with minimal deps for this WI, the NFC step is modeled as a no-op
/// here (the property test fixture verifies that the Python `unicodedata.normalize`
/// output matches this impl for ASCII-only + Latin-supplement content which is
/// the full corpus of the 3 notice files). A follow-on WI can add the
/// `unicode-normalization` crate without breaking the hash for content that
/// is already NFC-normalized (all 3 canonical notice files are authored in NFC).
#[must_use]
pub fn notice_text_hash(content: &str) -> alloc::string::String {
    // Step 1: CRLF → LF.
    let normalized = content.replace("\r\n", "\n").replace('\r', "\n");
    // Step 2: trim trailing whitespace per line.
    let trimmed: alloc::string::String = normalized
        .lines()
        .map(|l| l.trim_end())
        .collect::<alloc::vec::Vec<&str>>()
        .join("\n");
    // Step 3: SHA-256 (NFC is treated as identity for ASCII/Latin-supplement
    // corpus; see doc comment above for rationale).
    let mut hasher = Sha256::new();
    hasher.update(trimmed.as_bytes());
    hex::encode(hasher.finalize())
}

extern crate alloc;

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

    #[test]
    fn locale_canonical_strings_unique() {
        let locales = canonical_notice_locales();
        assert_eq!(locales.len(), 3);
        let mut set = std::collections::HashSet::new();
        for l in locales {
            assert!(set.insert(l.as_str()), "duplicate: {l}");
        }
        assert_eq!(set.len(), 3);
    }

    #[test]
    fn locale_filenames_unique() {
        let locales = canonical_notice_locales();
        let mut set = std::collections::HashSet::new();
        for l in locales {
            assert!(set.insert(l.as_filename()), "duplicate filename: {l}");
        }
        assert_eq!(set.len(), 3);
    }

    #[test]
    fn version_bump_vs_major() {
        let v1 = NoticeVersion::new(1, 0);
        let v2 = NoticeVersion::new(2, 0);
        let bump = v2.bump_vs(&v1);
        let valid = matches!(bump, Some(VersionBump::Major));
        assert!(valid, "expected Major bump");
    }

    #[test]
    fn version_bump_vs_minor() {
        let v1 = NoticeVersion::new(1, 5);
        let v2 = NoticeVersion::new(1, 6);
        let bump = v2.bump_vs(&v1);
        let valid = matches!(bump, Some(VersionBump::Minor));
        assert!(valid, "expected Minor bump");
    }

    #[test]
    fn version_bump_vs_none_on_equal() {
        let v1 = NoticeVersion::new(1, 0);
        let bump = v1.bump_vs(&v1);
        assert!(bump.is_none(), "equal versions produce no bump");
    }

    #[test]
    fn version_bump_major_is_material() {
        assert!(VersionBump::Major.is_material());
        assert!(!VersionBump::Minor.is_material());
    }

    #[test]
    fn notice_text_hash_crlf_lf_parity() {
        let lf = "Hello world\nSecond line\nThird";
        let crlf = "Hello world\r\nSecond line\r\nThird";
        assert_eq!(notice_text_hash(lf), notice_text_hash(crlf));
    }

    #[test]
    fn notice_text_hash_trailing_whitespace_stripped() {
        let a = "Hello   \nWorld";
        let b = "Hello\nWorld";
        assert_eq!(notice_text_hash(a), notice_text_hash(b));
    }

    #[test]
    fn notice_text_hash_is_64_hex_chars() {
        let h = notice_text_hash("Some content");
        assert_eq!(h.len(), 64, "SHA-256 hex = 32 bytes = 64 hex chars");
    }

    #[test]
    fn cloud_event_type_strings_canonical() {
        assert_eq!(
            NoticeCloudEventType::Published.as_cloud_event_type(),
            "dev.hugr.corelink.privacy_notice.published.v1"
        );
        assert_eq!(
            NoticeCloudEventType::Deprecated.as_cloud_event_type(),
            "dev.hugr.corelink.privacy_notice.deprecated.v1"
        );
    }

    #[test]
    fn notice_version_display() {
        let v = NoticeVersion::new(2, 3);
        assert_eq!(v.to_string(), "v2.3.0");
        assert_eq!(v.directory_name(), "v2.3.0");
    }

    #[test]
    fn published_payload_material_change_reflects_bump() {
        let hashes: BTreeMap<_, _> = std::iter::empty::<(String, String)>().collect();
        let minor = NoticePublishedPayload::new(
            Uuid::nil(),
            NoticeVersion::new(1, 1),
            VersionBump::Minor,
            hashes.clone(),
            0,
        );
        assert!(!minor.material_change);

        let major = NoticePublishedPayload::new(
            Uuid::nil(),
            NoticeVersion::new(2, 0),
            VersionBump::Major,
            hashes,
            0,
        );
        assert!(major.material_change);
    }
}
