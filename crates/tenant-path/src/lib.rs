//! CoreLink tenant prefix derivation (S-01 / WI-S01-001).
//!
//! Single canonical entry point for deriving the per-tenant path prefix used in
//! R2 / KV / D1 keys. Implements layer 5 of `INV-TENANT-ISOLATION` (see
//! `auth_model.md §8.1`) via HMAC-SHA256 over the tenant UUID, base64
//! URL-safe encoded and truncated to 16 ASCII characters per
//! `remote_cache_product_profile.md §7.1` and the S-01 spec contract.
//!
//! # Quickstart
//!
//! ```rust
//! use corelink_tenant_path::{derive_prefix, TenantDerivationKey};
//! use uuid::Uuid;
//! use zeroize::Zeroizing;
//!
//! # fn ex() -> Result<(), Box<dyn std::error::Error>> {
//! let tdk = TenantDerivationKey::from_bytes(Zeroizing::new([0u8; 32]));
//! let tenant_id = Uuid::parse_str("01938af0-abcd-7123-8456-000000000001")?;
//! let prefix = derive_prefix(&tdk, tenant_id);
//! assert_eq!(prefix.as_str().len(), 16);
//! # Ok(())
//! # }
//! ```
//!
//! # Anti-patterns (do NOT)
//!
//! - **Do not** construct [`TenantPrefix`] from raw bytes outside this crate.
//!   The newtype field is private precisely so that only [`derive_prefix`]
//!   can produce a valid prefix.
//! - **Do not** log or `Debug`-print [`TenantDerivationKey`]. The `Debug` impl
//!   intentionally redacts every byte.
//! - **Do not** add a `derive_prefix_for_testing` or `derive_prefix_raw` public
//!   API. Single function, single trust boundary.

#![forbid(unsafe_code)]

pub mod error;

mod cache;
mod prefix;

pub use cache::{TdkVersion, TenantPrefixCache, CACHE_CAPACITY};
pub use error::DeriveError;
pub use prefix::{derive_prefix, TenantDerivationKey, TenantPrefix, TENANT_PREFIX_LEN};
