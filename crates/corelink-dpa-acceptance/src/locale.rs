//! Locale-aware notice text registry + locale mismatch enforcement
//! (Lote 10.16 canonical; CTRL-PRIV-CONSENT-005).

use std::collections::HashMap;

use sha2::{Digest, Sha256};

use crate::error::DpaAcceptanceError;
use crate::schema::LocaleBcp47;

/// Compute the canonical SHA-256 hex64 of a rendered notice text.
///
/// Used both by the server-side recompute (CTRL-PRIV-CONSENT-001
/// enforcement) and by tests / fixtures. The Web Crypto API on the
/// client computes the matching value.
#[must_use]
pub fn notice_text_hash(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    let digest = hasher.finalize();
    hex::encode(digest)
}

/// Content-addressable registry of DPA notice texts per locale.
///
/// In production the registry is populated at startup by reading
/// `legal/dpa/v<semver>.<locale>.md` artefacts; the hash is recomputed
/// and compared against the client-supplied `notice_text_hash` to
/// enforce CTRL-PRIV-CONSENT-001.
#[derive(Clone, Debug, Default)]
pub struct LocaleNoticeRegistry {
    by_locale: HashMap<LocaleBcp47, String>,
}

impl LocaleNoticeRegistry {
    /// Build an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            by_locale: HashMap::new(),
        }
    }

    /// Register the canonical notice text for a locale. Replaces any
    /// previously-registered value (last-writer-wins; production
    /// startup populates each locale exactly once).
    pub fn register(&mut self, locale: LocaleBcp47, text: impl Into<String>) {
        self.by_locale.insert(locale, text.into());
    }

    /// Return the SHA-256 hex64 of the registered notice text for the
    /// locale, or `None` if the locale is not registered.
    #[must_use]
    pub fn hash_for(&self, locale: LocaleBcp47) -> Option<String> {
        self.by_locale.get(&locale).map(|t| notice_text_hash(t))
    }

    /// Return the registered notice text for the locale, if any.
    #[must_use]
    pub fn text_for(&self, locale: LocaleBcp47) -> Option<&str> {
        self.by_locale.get(&locale).map(String::as_str)
    }
}

/// Enforce that the locale on the consent payload matches the server-
/// resolved locale (Lote 10.16: cookie-derived, NOT Accept-Language).
///
/// # Errors
///
/// Returns [`DpaAcceptanceError::LocaleMismatch`] if the two locales
/// differ.
pub fn enforce_locale_match(
    server_resolved: LocaleBcp47,
    payload_locale: LocaleBcp47,
) -> Result<(), DpaAcceptanceError> {
    if server_resolved == payload_locale {
        Ok(())
    } else {
        Err(DpaAcceptanceError::LocaleMismatch {
            server: server_resolved.as_bcp47(),
            payload: payload_locale.as_bcp47(),
        })
    }
}

/// Hash an IP address with the per-region salt to obtain the value
/// persisted in `dpa_acceptances.accepted_ip_hash`. SHA-256 over
/// `ip_bytes || salt`. Never persist raw IPs (CTRL-PRIV-001).
#[must_use]
pub fn accepted_ip_hash(ip: &str, salt: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(ip.as_bytes());
    hasher.update(salt);
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_deterministic() {
        let a = notice_text_hash("hello");
        let b = notice_text_hash("hello");
        assert_eq!(a, b);
        assert_eq!(a.len(), 64);
    }

    #[test]
    fn registry_roundtrip() {
        let mut reg = LocaleNoticeRegistry::new();
        reg.register(LocaleBcp47::EnUs, "EN text");
        reg.register(LocaleBcp47::PtBr, "PT text");
        assert_eq!(
            reg.hash_for(LocaleBcp47::EnUs).as_deref(),
            Some(notice_text_hash("EN text")).as_deref()
        );
        assert!(reg.hash_for(LocaleBcp47::Es419).is_none());
    }

    #[test]
    fn locale_mismatch_rejected() {
        let r = enforce_locale_match(LocaleBcp47::PtBr, LocaleBcp47::EnUs);
        assert!(matches!(r, Err(DpaAcceptanceError::LocaleMismatch { .. })));
    }

    #[test]
    fn locale_match_ok() {
        assert!(enforce_locale_match(LocaleBcp47::PtBr, LocaleBcp47::PtBr).is_ok());
    }

    #[test]
    fn ip_hash_differs_for_different_salts() {
        let a = accepted_ip_hash("1.2.3.4", b"salt-a");
        let b = accepted_ip_hash("1.2.3.4", b"salt-b");
        assert_ne!(a, b);
    }
}
