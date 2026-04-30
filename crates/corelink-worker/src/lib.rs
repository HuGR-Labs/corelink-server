//! CoreLink Worker storage adapters (S-01 / WI-S01-003).
//!
//! This crate hosts the storage adapters that sit on the CAS write/read hot
//! path. The first deliverable, landed in WI-S01-003, is the **R2 single-blob
//! adapter**: an [`storage::r2::R2Writer`] / [`storage::r2::R2Reader`] pair
//! that encapsulate every interaction with Cloudflare R2 buckets for blobs
//! ≤ 5 MiB.
//!
//! ## Why a trait abstraction over the real Cloudflare binding
//!
//! Cloudflare's native R2 binding (`worker::R2Bucket`) only exists inside the
//! Workers runtime — it is unavailable in host-side `cargo test` builds. To
//! keep the unit/integration test suite real (i.e. exercising the same code
//! paths that run in production) we abstract the bucket behind an
//! [`storage::r2::R2Backend`] trait. The crate ships an
//! [`storage::r2::InMemoryR2`] fake for tests; the real CF binding adapter
//! lands alongside the REAPI handler in **WI-S01-005** (miniflare-backed
//! integration tier). The trait keeps the tenant-path / `If-None-Match` /
//! error-mapping logic identical between the test fake and the production
//! binding.
//!
//! ## CAS-integrity seam
//!
//! Per [`corelink_hash::BlobStoreWrite`], writes accept only `&VerifiedBody`.
//! [`storage::r2::R2Writer::for_tenant`] returns a [`storage::r2::ScopedR2Writer`] that
//! implements `BlobStoreWrite`, so any code path that wants to write to R2
//! *physically* cannot do so without first running `VerifiedBody::new` —
//! enforcing CTRL-CAS-001 / INV-CAS-INTEGRITY at the type level.
//!
//! ## Tenant isolation seam
//!
//! Per [`corelink_tenant_path::derive_prefix`], every R2 key is prefixed by
//! `HMAC16 = b64url_no_pad(HMAC_SHA256(TDK, tenant_id))[..16]`. The
//! adapter never accepts a tenant_id in plaintext — the only way to obtain a
//! `ScopedR2Writer` is via [`storage::r2::R2Writer::for_tenant`] which derives the prefix
//! internally. Bypassing the HMAC derivation is impossible (REG-NAMESPACE-001
//! / REG-NAMESPACE-002).
//!
//! # Quickstart
//!
//! ```rust
//! use bytes::Bytes;
//! use corelink_hash::{Digest, VerifiedBody};
//! use corelink_tenant_path::TenantDerivationKey;
//! use corelink_worker::{Region, TenantCtx};
//! use corelink_worker::storage::r2::{InMemoryR2, R2Reader, R2Writer};
//! use std::sync::Arc;
//! use uuid::Uuid;
//! use zeroize::Zeroizing;
//!
//! # async fn ex() -> Result<(), Box<dyn std::error::Error>> {
//! let tdk = TenantDerivationKey::from_bytes(Zeroizing::new([0u8; 32]));
//! let backend = Arc::new(InMemoryR2::new());
//! let writer = R2Writer::new(Region::Wnam, Arc::clone(&backend));
//! let reader = R2Reader::new(Region::Wnam, backend);
//!
//! let tenant_id = Uuid::parse_str("01938af0-abcd-7123-8456-000000000001")?;
//! // Prefix is derived internally — caller cannot forge it.
//! let ctx = TenantCtx::new(&tdk, tenant_id, Region::Wnam);
//!
//! let body = Bytes::from_static(b"hello world");
//! let claimed = Digest::from_hex(
//!     "d74981efa70a0c880b8d8c1985d075dbcbf679b99a5f9914e5aaf96b831a9e24",
//! )?;
//! let vb = VerifiedBody::new(body.clone(), claimed)?;
//!
//! writer.put(&ctx, &vb).await?;
//! let got = reader.get(&ctx, &claimed).await?;
//! assert_eq!(got, body);
//! # Ok(()) }
//! ```
//!
//! # Anti-patterns (do NOT)
//!
//! - **Do not** construct R2 keys from `tenant_id` plaintext anywhere outside
//!   this crate; always go through [`storage::r2::R2Writer::for_tenant`] /
//!   [`storage::r2::R2Writer::put`], which route through
//!   `corelink_tenant_path::derive_prefix`.
//! - **Do not** bypass [`storage::r2::R2Backend::put_if_none_match`] by adding
//!   a side-door PUT method that overwrites; CAS immutability
//!   (INV-CAS-IMMUTABILITY) is enforced through the `If-None-Match: *`
//!   contract at the trait level.
//! - **Do not** add an `R2Writer::put_unverified(body, digest)` shortcut — the
//!   `&VerifiedBody` requirement is the load-bearing seam.

#![forbid(unsafe_code)]

pub mod cache;
#[cfg(feature = "tower-middleware")]
pub mod middleware;
mod region;
pub mod storage;
mod tenant;

pub use region::Region;
pub use tenant::TenantCtx;
