//! Configuration for [`crate::oci::run_oci_adapter`].
//!
//! Charter-critical defaults:
//!
//! - [`OciAdapterConfig::enable_catalog`] = **`false`** by default. The
//!   OCI `_catalog` endpoint enumerates repositories across whoever
//!   the realm token-holder can see; with our per-tenant scoping it
//!   would leak repository names across tenants. Customers MAY flip
//!   to `true` post-GA once the per-tenant catalog filter ships.
//! - [`OciAdapterConfig::blob_size_limit_bytes`] = 5 GiB. OCI image
//!   layers can be huge; ECS / GKE customer cases routinely hit 2 GiB.
//! - [`OciAdapterConfig::multipart_chunk_size_bytes`] = 16 MiB. R2
//!   multipart-upload minimum part size is 5 MiB; 16 MiB is the
//!   sweet-spot per `corelink-r2-multipart` benchmarks.
//! - [`OciAdapterConfig::token_ttl_secs`] = 3600 (1 hour). OCI clients
//!   refresh tokens implicitly on 401; longer TTLs increase the
//!   replay window if the HMAC key leaks.

use std::sync::Arc;

use corelink_audit::ports::AuditEmitter;
use corelink_core::SecretWrap;

use crate::oci::ports::{BlobStore, ManifestKvStore, TenantResolver};

/// Live configuration consumed by [`crate::oci::run_oci_adapter`].
///
/// Construction-by-struct-literal is intentional: every field is
/// public so the call site surfaces every dependency at the wiring
/// point (no hidden defaults).
#[non_exhaustive]
pub struct OciAdapterConfig {
    /// Listen socket. Production binds the CF Worker / k8s service
    /// behind a TLS-terminating proxy; the adapter itself speaks
    /// plain HTTP and TRUSTS proxy headers for `X-Forwarded-Proto`
    /// (TLS-edge attestation is a separate hardening initiative).
    pub bind_addr: std::net::SocketAddr,

    /// Bearer realm URL — the endpoint the adapter advertises in its
    /// `Www-Authenticate` header. Default wiring points at this same
    /// adapter's `/token` route so the customer experience is "talk
    /// to one host". Customers MAY override to delegate to a Clerk
    /// JWT bridge or an external OIDC IdP (post-GA).
    pub bearer_realm: String,

    /// Maximum blob size in bytes; default 5 GiB. Larger pushes
    /// receive `413 Payload Too Large` + audit emit
    /// `oci.push.blob_oversize`. The check fires both during the
    /// streaming `PATCH` (cumulative byte counter) and at the
    /// finalize `PUT` (defense-in-depth in case a buggy client
    /// understated `Content-Length`).
    pub blob_size_limit_bytes: u64,

    /// Multipart chunk size hint for the `BlobStore` upload session.
    /// 16 MiB by default. NOT a hard cap on `PATCH` body size — the
    /// OCI client controls that.
    pub multipart_chunk_size_bytes: u64,

    /// `_catalog` endpoint switch. MUST default `false` per oci.md §6;
    /// enabling without per-tenant scoping leaks cross-tenant repo
    /// names. Construct-time validation is enforced in
    /// [`Self::sanity_check`].
    pub enable_catalog: bool,

    /// Realm-token HMAC TTL in seconds. Defaults to 3600 (1h). Tokens
    /// older than this fail verification and the client is forced to
    /// re-exchange via the basic-auth → realm flow.
    pub token_ttl_secs: u64,

    /// HMAC-SHA256 signing key for realm bearer tokens. Wrapped in
    /// [`SecretWrap`] so drops zeroize and `Debug` refuses to leak.
    /// MUST be ≥32 characters of key material per [`Self::sanity_check`].
    /// Use `openssl rand -hex 32` (64 chars → 256-bit key) — the raw
    /// string bytes become the HMAC key directly; hex chars are NOT
    /// decoded at load time, so 64 hex chars = 64 key bytes = 512-bit
    /// effective entropy. Source env: `CORELINK_OCI_TOKEN_KEY`.
    pub token_signing_key: SecretWrap,

    /// CAS-side blob store port.
    pub cas: Arc<dyn BlobStore>,

    /// Manifest + tag-list KV port.
    pub metadata_kv: Arc<dyn ManifestKvStore>,

    /// PAT → tenant resolver.
    pub tenant_resolver: Arc<dyn TenantResolver>,

    /// Audit emitter — every state mutation emits BEFORE returning
    /// success to the client per INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER.
    pub auditor: Arc<dyn AuditEmitter>,
}

impl std::fmt::Debug for OciAdapterConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Manual impl so `token_signing_key` redaction is preserved
        // even if a future field accidentally includes `Debug`-leaky
        // bytes. Trait ports impl `Debug` via dyn.
        f.debug_struct("OciAdapterConfig")
            .field("bind_addr", &self.bind_addr)
            .field("bearer_realm", &self.bearer_realm)
            .field("blob_size_limit_bytes", &self.blob_size_limit_bytes)
            .field(
                "multipart_chunk_size_bytes",
                &self.multipart_chunk_size_bytes,
            )
            .field("enable_catalog", &self.enable_catalog)
            .field("token_ttl_secs", &self.token_ttl_secs)
            .field("token_signing_key", &"<redacted>")
            .finish_non_exhaustive()
    }
}

/// Default sentinel values referenced by [`OciAdapterConfig`].
///
/// Centralized defaults so production wiring + tests + audit
/// review share one source of truth.
pub mod defaults {

    /// 5 GiB.
    pub const BLOB_SIZE_LIMIT_BYTES: u64 = 5 * 1024 * 1024 * 1024;
    /// 16 MiB.
    pub const MULTIPART_CHUNK_SIZE_BYTES: u64 = 16 * 1024 * 1024;
    /// 1h.
    pub const TOKEN_TTL_SECS: u64 = 3600;
    /// `_catalog` MUST be off by default — cross-tenant leak risk.
    pub const ENABLE_CATALOG: bool = false;
    /// Minimum acceptable HMAC key bytes (raw).
    pub const MIN_TOKEN_KEY_BYTES: usize = 32;
}

impl OciAdapterConfig {
    /// Construct an [`OciAdapterConfig`] from explicit parts.
    ///
    /// Needed because the struct is `#[non_exhaustive]` and therefore
    /// cannot be built with struct-literal syntax from outside this
    /// crate. Defaults for the numeric / boolean fields land via the
    /// [`defaults`] sentinel module so call sites surface the intent
    /// explicitly.
    #[must_use]
    #[allow(
        clippy::too_many_arguments,
        reason = "wiring constructor — every dep is explicit by design"
    )]
    pub fn new(
        bind_addr: std::net::SocketAddr,
        bearer_realm: String,
        blob_size_limit_bytes: u64,
        multipart_chunk_size_bytes: u64,
        enable_catalog: bool,
        token_ttl_secs: u64,
        token_signing_key: SecretWrap,
        cas: Arc<dyn BlobStore>,
        metadata_kv: Arc<dyn ManifestKvStore>,
        tenant_resolver: Arc<dyn TenantResolver>,
        auditor: Arc<dyn AuditEmitter>,
    ) -> Self {
        Self {
            bind_addr,
            bearer_realm,
            blob_size_limit_bytes,
            multipart_chunk_size_bytes,
            enable_catalog,
            token_ttl_secs,
            token_signing_key,
            cas,
            metadata_kv,
            tenant_resolver,
            auditor,
        }
    }

    /// Verify charter-critical invariants at adapter start.
    ///
    /// # Errors
    ///
    /// - [`ConfigError::CatalogEnabledWithoutScoping`] if
    ///   `enable_catalog` is `true` (until per-tenant catalog scoping
    ///   is implemented, this is structurally rejected).
    /// - [`ConfigError::TokenKeyTooShort`] if the HMAC signing key has
    ///   fewer than 32 raw bytes.
    /// - [`ConfigError::TokenTtlZero`] if `token_ttl_secs == 0`.
    pub fn sanity_check(&self) -> Result<(), ConfigError> {
        use secrecy::ExposeSecret as _;
        if self.enable_catalog {
            return Err(ConfigError::CatalogEnabledWithoutScoping);
        }
        let key_bytes = self.token_signing_key.as_secret_string().expose_secret();
        if key_bytes.len() < defaults::MIN_TOKEN_KEY_BYTES {
            return Err(ConfigError::TokenKeyTooShort {
                got: key_bytes.len(),
                min: defaults::MIN_TOKEN_KEY_BYTES,
            });
        }
        if self.token_ttl_secs == 0 {
            return Err(ConfigError::TokenTtlZero);
        }
        Ok(())
    }
}

/// Config-time errors (separate from runtime [`crate::oci::error::OciAdapterError`]).
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ConfigError {
    /// Catalog endpoint was enabled but per-tenant scoping is not
    /// implemented; would leak cross-tenant repo names.
    #[error(
        "`enable_catalog = true` is rejected until per-tenant catalog scoping ships (oci.md §6)"
    )]
    CatalogEnabledWithoutScoping,

    /// HMAC signing key is too short.
    #[error("token signing key too short: got {got} bytes, need ≥{min}")]
    TokenKeyTooShort {
        /// Actual key length in bytes.
        got: usize,
        /// Minimum acceptable length.
        min: usize,
    },

    /// Token TTL is zero.
    #[error("token_ttl_secs must be > 0")]
    TokenTtlZero,
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
    use crate::oci::ports::testing::{InMemoryBlobStore, InMemoryKv, StaticTenantResolver};
    use corelink_audit::ports::InMemoryAuditEmitter;

    fn cfg(enable_catalog: bool, key: &str) -> OciAdapterConfig {
        OciAdapterConfig {
            bind_addr: ([127u8, 0, 0, 1], 0).into(),
            bearer_realm: String::from("http://localhost/token"),
            blob_size_limit_bytes: defaults::BLOB_SIZE_LIMIT_BYTES,
            multipart_chunk_size_bytes: defaults::MULTIPART_CHUNK_SIZE_BYTES,
            enable_catalog,
            token_ttl_secs: defaults::TOKEN_TTL_SECS,
            token_signing_key: SecretWrap::new(key.to_string()),
            cas: Arc::new(InMemoryBlobStore::default()),
            metadata_kv: Arc::new(InMemoryKv::default()),
            tenant_resolver: Arc::new(StaticTenantResolver::default()),
            auditor: Arc::new(InMemoryAuditEmitter::default()),
        }
    }

    #[test]
    fn catalog_default_off_passes() {
        let c = cfg(false, &"x".repeat(32));
        c.sanity_check().expect("default config passes");
    }

    #[test]
    fn catalog_enabled_rejected() {
        let c = cfg(true, &"x".repeat(32));
        match c.sanity_check() {
            Err(ConfigError::CatalogEnabledWithoutScoping) => {}
            other => panic!("expected CatalogEnabledWithoutScoping, got {other:?}"),
        }
    }

    #[test]
    fn token_key_too_short_rejected() {
        let c = cfg(false, "short");
        match c.sanity_check() {
            Err(ConfigError::TokenKeyTooShort { got: 5, min: 32 }) => {}
            other => panic!("expected TokenKeyTooShort, got {other:?}"),
        }
    }

    #[test]
    fn debug_redacts_key() {
        let c = cfg(false, &"x".repeat(32));
        let s = format!("{c:?}");
        assert!(
            s.contains("<redacted>"),
            "key must be redacted in Debug, got: {s}"
        );
        assert!(
            !s.contains('x'),
            "raw key bytes must not appear in Debug: {s}"
        );
    }
}
