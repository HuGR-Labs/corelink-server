//! Strict locale enforcement (CTRL-PRIV-CONSENT-005).
//!
//! The HTTP `Accept-Language` header MUST match `payload.locale`.
//! Mismatch → 422 `locale_mismatch`. This is **strict** (DD-003):
//! permissive enforcement would be defensibility-weak.
//!
//! The data subject must have seen the notice in the locale they asserted.
//! A mismatch means either (a) the UI sent the wrong locale or (b) an
//! attacker is attempting to submit a consent proof for a locale the
//! subject did not see.

use super::schema::LocaleBcp47;

/// Error returned on locale mismatch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocaleEnforceError {
    /// The locale declared in the payload.
    pub payload_locale: LocaleBcp47,
    /// The locale parsed from the Accept-Language header.
    pub header_locale: String,
}

impl std::fmt::Display for LocaleEnforceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Accept-Language header '{}' must match payload.locale '{}'; \
             titular provavelmente viu notice em locale errado",
            self.header_locale,
            self.payload_locale.as_str()
        )
    }
}

/// Parse the primary locale from an `Accept-Language` header value.
///
/// Accepts the first non-wildcard token (e.g. `"pt-BR,en;q=0.9"` →
/// `"pt-BR"`). Case-insensitive prefix match against the 3 canonical
/// locales.
fn parse_accept_language_primary(header: &str) -> Option<LocaleBcp47> {
    let primary = header.split(',').next()?.split(';').next()?.trim();
    // Normalise to canonical BCP-47 casing.
    let lc = primary.to_lowercase();
    if lc.starts_with("pt-br") || lc.starts_with("pt_br") {
        Some(LocaleBcp47::PtBr)
    } else if lc.starts_with("en-us") || lc.starts_with("en_us") || lc == "en" {
        Some(LocaleBcp47::EnUs)
    } else if lc.starts_with("es-mx") || lc.starts_with("es_mx") {
        Some(LocaleBcp47::EsMx)
    } else {
        None
    }
}

/// Enforce strict locale match between `Accept-Language` header and
/// `payload.locale`.
///
/// Returns `Ok(())` if they match, or `Err(LocaleEnforceError)` if they
/// do not or if the header locale cannot be parsed.
///
/// # AC-007 — strict enforcement
///
/// ```text
/// Given payload.locale = 'pt-BR' and Accept-Language = 'en-US'
/// Then 422 locale_mismatch; NO consent row inserted.
/// ```
pub fn enforce_locale(
    accept_language_header: &str,
    payload_locale: LocaleBcp47,
) -> Result<(), LocaleEnforceError> {
    let header_locale = parse_accept_language_primary(accept_language_header);
    match header_locale {
        Some(parsed) if parsed == payload_locale => Ok(()),
        _ => Err(LocaleEnforceError {
            payload_locale,
            header_locale: accept_language_header.to_owned(),
        }),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic)]
    use super::*;

    #[test]
    fn matching_locale_passes() {
        assert!(enforce_locale("pt-BR,en;q=0.9", LocaleBcp47::PtBr).is_ok());
        assert!(enforce_locale("en-US", LocaleBcp47::EnUs).is_ok());
        assert!(enforce_locale("es-MX", LocaleBcp47::EsMx).is_ok());
    }

    #[test]
    fn mismatched_locale_rejects() {
        let err = enforce_locale("en-US", LocaleBcp47::PtBr).unwrap_err();
        assert_eq!(err.payload_locale, LocaleBcp47::PtBr);
        assert!(err.header_locale.contains("en-US"));
    }

    #[test]
    fn unknown_locale_header_rejects() {
        // Unsupported locale in Accept-Language → error
        let result = enforce_locale("fr-FR", LocaleBcp47::PtBr);
        assert!(result.is_err());
    }

    #[test]
    fn case_insensitive_match() {
        assert!(enforce_locale("PT-BR", LocaleBcp47::PtBr).is_ok());
        assert!(enforce_locale("en-us", LocaleBcp47::EnUs).is_ok());
    }
}
