//! Azure Key Vault key resource URI validation + parsing.
//!
//! Canonical forms accepted:
//!
//! ```text
//! https://{vault-name}.vault.azure.net/keys/{key-name}
//! https://{vault-name}.vault.azure.net/keys/{key-name}/{key-version}
//! https://{vault-name}.managedhsm.azure.net/keys/{key-name}
//! https://{vault-name}.managedhsm.azure.net/keys/{key-name}/{key-version}
//! ```
//!
//! - `vault-name`: 3–24 alphanumeric/hyphen, must start with a letter (Azure naming rules).
//! - `key-name`: 1–127 alphanumeric/hyphen.
//! - `key-version`: 32-char hex (Azure assigns).
//!
//! Sovereign / government clouds (`vault.azure.cn`, `vault.usgovcloudapi.net`,
//! `vault.microsoftazure.de`) are also accepted but transparently — the
//! provider just uses whatever host the URI carries.
//!
//! Use [`is_valid_kv_resource`] for reject-malformed validation and
//! [`parse_kv_resource`] to extract the (`base_url`, `key_name`, `key_version`)
//! triple needed by the REST client.

#[cfg(feature = "production-azure")]
use std::sync::OnceLock;

/// Parsed components of a Key Vault key URI.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(feature = "production-azure"), allow(dead_code))]
pub(crate) struct KvKeyResource {
    /// e.g. `https://myvault.vault.azure.net`
    pub base_url: String,
    /// Key name segment.
    pub key_name: String,
    /// Optional key version (32 hex chars).
    pub key_version: Option<String>,
}

#[cfg(feature = "production-azure")]
fn full_regex() -> &'static regex::Regex {
    static R: OnceLock<regex::Regex> = OnceLock::new();
    R.get_or_init(|| {
        // Anchored on both ends. Vault host pattern matches both classic
        // Key Vault (`*.vault.azure.net`) and Managed HSM (`*.managedhsm.azure.net`)
        // plus the well-known sovereign-cloud TLDs.
        #[allow(clippy::expect_used)]
        regex::Regex::new(
            r"^https://(?P<vault>[a-zA-Z][a-zA-Z0-9-]{1,22}[a-zA-Z0-9])\.(?P<svc>vault\.azure\.net|managedhsm\.azure\.net|vault\.azure\.cn|managedhsm\.azure\.cn|vault\.usgovcloudapi\.net|managedhsm\.usgovcloudapi\.net|vault\.microsoftazure\.de)/keys/(?P<key>[a-zA-Z0-9-]{1,127})(?:/(?P<ver>[a-f0-9]{32}))?$",
        )
        // nosemgrep: corelink.rust.no-expect-in-byok-src  # reason: compile-time-constant regex literal; failure here would be a build-time bug (caught by tests), not a runtime key-path panic. Owner-approved per R-prep static-analysis triage 2026-05-15.
        .expect("static regex compiles")
    })
}

/// Return `true` iff `s` matches a canonical Azure Key Vault key URI.
#[cfg(feature = "production-azure")]
#[must_use]
#[allow(dead_code)] // exposed for tests + future direct-validation call sites
pub(crate) fn is_valid_kv_resource(s: &str) -> bool {
    full_regex().is_match(s)
}

/// Parse the URI into base URL + key name + optional version.
///
/// Returns `None` if the URI is malformed.
#[cfg(feature = "production-azure")]
#[must_use]
pub(crate) fn parse_kv_resource(s: &str) -> Option<KvKeyResource> {
    let caps = full_regex().captures(s)?;
    let vault = caps.name("vault")?.as_str();
    let svc = caps.name("svc")?.as_str();
    let key = caps.name("key")?.as_str();
    let ver = caps.name("ver").map(|m| m.as_str().to_string());
    Some(KvKeyResource {
        base_url: format!("https://{vault}.{svc}"),
        key_name: key.to_string(),
        key_version: ver,
    })
}

/// Fallback validator when the `production` feature is off (used by the
/// stub for hygiene). Pure-Rust shape check; less strict than the regex.
#[cfg(not(feature = "production-azure"))]
#[allow(dead_code)]
#[must_use]
pub(crate) fn is_valid_kv_resource(s: &str) -> bool {
    // Minimum: https://<host>/keys/<name>
    let Some(rest) = s.strip_prefix("https://") else {
        return false;
    };
    let Some((host, path)) = rest.split_once('/') else {
        return false;
    };
    if !host.contains(".vault.azure.")
        && !host.contains(".managedhsm.azure.")
        && !host.contains(".vault.usgovcloudapi.")
        && !host.contains(".managedhsm.usgovcloudapi.")
        && !host.contains(".vault.microsoftazure.")
    {
        return false;
    }
    let parts: Vec<&str> = path.split('/').collect();
    matches!(parts.as_slice(), ["keys", _] | ["keys", _, _]) && parts.iter().all(|p| !p.is_empty())
}

#[cfg(all(test, feature = "production-azure"))]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn valid_resource_paths() {
        assert!(is_valid_kv_resource(
            "https://myvault.vault.azure.net/keys/mykey"
        ));
        assert!(is_valid_kv_resource(
            "https://myvault.vault.azure.net/keys/mykey/0123456789abcdef0123456789abcdef"
        ));
        assert!(is_valid_kv_resource(
            "https://corp-hsm.managedhsm.azure.net/keys/customer-cmk"
        ));
        assert!(is_valid_kv_resource(
            "https://gov-vault.vault.usgovcloudapi.net/keys/k1"
        ));
        assert!(is_valid_kv_resource(
            "https://china-vault.vault.azure.cn/keys/k1"
        ));
    }

    #[test]
    fn invalid_resource_paths() {
        // Wrong scheme.
        assert!(!is_valid_kv_resource(
            "http://myvault.vault.azure.net/keys/mykey"
        ));
        // Wrong host.
        assert!(!is_valid_kv_resource("https://example.com/keys/mykey"));
        // Missing path segment.
        assert!(!is_valid_kv_resource(
            "https://myvault.vault.azure.net/secrets/mykey"
        ));
        // Empty.
        assert!(!is_valid_kv_resource(""));
        // AWS ARN format.
        assert!(!is_valid_kv_resource("arn:aws:kms:us-east-1:123:key/abc"));
        // Trailing slash.
        assert!(!is_valid_kv_resource(
            "https://myvault.vault.azure.net/keys/mykey/"
        ));
        // Vault name too short.
        assert!(!is_valid_kv_resource(
            "https://ab.vault.azure.net/keys/mykey"
        ));
    }

    #[test]
    fn parse_with_version() {
        let r = parse_kv_resource(
            "https://myvault.vault.azure.net/keys/mykey/0123456789abcdef0123456789abcdef",
        )
        .unwrap();
        assert_eq!(r.base_url, "https://myvault.vault.azure.net");
        assert_eq!(r.key_name, "mykey");
        assert_eq!(
            r.key_version.as_deref(),
            Some("0123456789abcdef0123456789abcdef")
        );
    }

    #[test]
    fn parse_without_version() {
        let r = parse_kv_resource("https://myvault.vault.azure.net/keys/mykey").unwrap();
        assert_eq!(r.base_url, "https://myvault.vault.azure.net");
        assert_eq!(r.key_name, "mykey");
        assert!(r.key_version.is_none());
    }

    #[test]
    fn parse_managed_hsm() {
        let r =
            parse_kv_resource("https://corp-hsm.managedhsm.azure.net/keys/customer-cmk").unwrap();
        assert_eq!(r.base_url, "https://corp-hsm.managedhsm.azure.net");
        assert_eq!(r.key_name, "customer-cmk");
    }
}
