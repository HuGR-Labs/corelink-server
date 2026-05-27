//! AAGUID allowlist + denylist machinery.
//!
//! Per `WI-S03-006 §6.1.5 + §9.5` the canonical decision is "explicit
//! allowlist with a sibling denylist; FIDO MDS enriches metadata, but
//! the final accept/reject is local config" so a compromised MDS feed
//! cannot widen the policy.
//!
//! Closed-default semantics: an AAGUID **missing** from the allowlist
//! is rejected (`WebAuthnError::AaguidNotAllowed`) even if it is not
//! present on the denylist. The denylist is the explicit "this AAGUID
//! is known-bad" override.

use std::collections::BTreeSet;
use std::fmt;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::WebAuthnError;

/// 128-bit Authenticator Attestation GUID (W3C WebAuthn §6.4.1).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Aaguid(pub Uuid);

impl Aaguid {
    /// Synthetic test AAGUID for the YubiKey 5 series. Real
    /// production policies are sourced from FIDO MDS; the test
    /// fixtures hard-code the synthetic so property tests do not need
    /// network access.
    #[must_use]
    pub const fn yubikey_5() -> Self {
        Self(Uuid::from_bytes([
            0xfa, 0x2b, 0x99, 0xdc, 0x9e, 0x39, 0x42, 0x57, 0x8f, 0x92, 0x4a, 0x30, 0xd2, 0x3c,
            0x47, 0x00,
        ]))
    }

    /// Synthetic test AAGUID for Apple Touch ID / Face ID (platform
    /// authenticator).
    #[must_use]
    pub const fn touch_id() -> Self {
        Self(Uuid::from_bytes([
            0xad, 0xce, 0x00, 0x02, 0x35, 0xbc, 0xc6, 0x0a, 0x64, 0x8b, 0x0b, 0x25, 0xf1, 0xf0,
            0x55, 0x03,
        ]))
    }

    /// Synthetic test AAGUID for Windows Hello.
    #[must_use]
    pub const fn windows_hello() -> Self {
        Self(Uuid::from_bytes([
            0x08, 0x98, 0x76, 0x47, 0x1f, 0x46, 0x4c, 0xb6, 0x88, 0xb1, 0xb2, 0x4d, 0xc2, 0xed,
            0x00, 0x55,
        ]))
    }

    /// Synthetic test AAGUID for Android biometrics.
    #[must_use]
    pub const fn android_biometrics() -> Self {
        Self(Uuid::from_bytes([
            0xb9, 0x3f, 0xd9, 0x61, 0xf2, 0xe6, 0x46, 0x2f, 0xb1, 0x22, 0x82, 0x00, 0x2a, 0x44,
            0x4e, 0x32,
        ]))
    }

    /// Synthetic test AAGUID for the iCloud Keychain passkey provider.
    #[must_use]
    pub const fn icloud_passkey() -> Self {
        Self(Uuid::from_bytes([
            0xfb, 0xfc, 0x30, 0x07, 0x15, 0x4e, 0x4e, 0xcc, 0x8c, 0x0b, 0x6e, 0x02, 0x0c, 0xc6,
            0xed, 0xae,
        ]))
    }

    /// Synthetic test AAGUID for the **deprecated** YubiKey 4
    /// generation. Used by the adversarial regression test
    /// `test_aaguid_denylist`.
    #[must_use]
    pub const fn yubikey_4_deprecated() -> Self {
        Self(Uuid::from_bytes([
            0x4e, 0xb6, 0x4c, 0x29, 0x82, 0xa8, 0x4d, 0xab, 0xa9, 0x60, 0x8b, 0xab, 0xe0, 0xa4,
            0x88, 0x99,
        ]))
    }

    /// All-zero "indistinguishable AAGUID" sentinel (W3C §6.4.1) —
    /// some authenticators omit AAGUID entirely. Treated as
    /// `AaguidNotAllowed` unless explicitly allowed by config.
    #[must_use]
    pub const fn nil() -> Self {
        Self(Uuid::nil())
    }
}

impl fmt::Debug for Aaguid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Aaguid({})", self.0)
    }
}

impl fmt::Display for Aaguid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Closed-default AAGUID policy.
#[derive(Debug, Clone, Default)]
pub struct AaguidPolicy {
    allow: BTreeSet<Aaguid>,
    deny: BTreeSet<Aaguid>,
}

impl AaguidPolicy {
    /// Construct an empty (`{ }`) policy. Every AAGUID is rejected
    /// until at least one [`AaguidPolicyBuilder::allow`] entry lands.
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// Construct a fluent builder.
    #[must_use]
    pub fn builder() -> AaguidPolicyBuilder {
        AaguidPolicyBuilder::default()
    }

    /// Evaluate the policy against an AAGUID. Returns `Ok(())` only
    /// when the AAGUID is on the allowlist AND not on the denylist.
    pub fn evaluate(&self, aaguid: Aaguid) -> Result<(), WebAuthnError> {
        if self.deny.contains(&aaguid) {
            return Err(WebAuthnError::AaguidDenied);
        }
        if !self.allow.contains(&aaguid) {
            return Err(WebAuthnError::AaguidNotAllowed);
        }
        Ok(())
    }

    /// Whether `aaguid` is on the allowlist.
    #[must_use]
    pub fn is_allowed(&self, aaguid: Aaguid) -> bool {
        self.allow.contains(&aaguid)
    }

    /// Whether `aaguid` is on the denylist.
    #[must_use]
    pub fn is_denied(&self, aaguid: Aaguid) -> bool {
        self.deny.contains(&aaguid)
    }

    /// Iterator over allowlisted entries (for diagnostics + dashboard
    /// fixture population).
    pub fn allow_iter(&self) -> impl Iterator<Item = &Aaguid> {
        self.allow.iter()
    }

    /// Iterator over denylisted entries.
    pub fn deny_iter(&self) -> impl Iterator<Item = &Aaguid> {
        self.deny.iter()
    }
}

/// Fluent builder for [`AaguidPolicy`].
#[derive(Debug, Default, Clone)]
pub struct AaguidPolicyBuilder {
    allow: BTreeSet<Aaguid>,
    deny: BTreeSet<Aaguid>,
}

impl AaguidPolicyBuilder {
    /// Add an AAGUID to the allowlist.
    #[must_use]
    pub fn allow(mut self, aaguid: Aaguid) -> Self {
        self.allow.insert(aaguid);
        self
    }

    /// Add an AAGUID to the denylist. Denylist takes precedence over
    /// allowlist (a denylisted AAGUID is always rejected).
    #[must_use]
    pub fn deny(mut self, aaguid: Aaguid) -> Self {
        self.deny.insert(aaguid);
        self
    }

    /// Add an iterator of AAGUIDs to the allowlist.
    #[must_use]
    pub fn allow_many<I: IntoIterator<Item = Aaguid>>(mut self, iter: I) -> Self {
        self.allow.extend(iter);
        self
    }

    /// Add an iterator of AAGUIDs to the denylist.
    #[must_use]
    pub fn deny_many<I: IntoIterator<Item = Aaguid>>(mut self, iter: I) -> Self {
        self.deny.extend(iter);
        self
    }

    /// Finalize.
    #[must_use]
    pub fn build(self) -> AaguidPolicy {
        AaguidPolicy {
            allow: self.allow,
            deny: self.deny,
        }
    }
}
