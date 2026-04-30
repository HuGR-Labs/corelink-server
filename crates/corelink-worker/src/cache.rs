//! Negative-cache adapters for the CoreLink CAS hot path (WI-S02-005).
//!
//! Caches "blob `(tenant, digest)` is known-missing" results in a per-region
//! KV namespace so that probe storms from a misbehaving (or adversarial)
//! cache client short-circuit before they hit D1 + R2. The KV adapter is
//! built behind the [`kv::KvBackend`] trait abstraction (same pattern as
//! [`crate::storage::r2::R2Backend`] and `corelink-meta::MetaStore`): the
//! crate ships an [`kv::InMemoryKv`] fake for host-side tests; the real
//! Cloudflare Workers KV binding adapter lands at integration tier
//! alongside the REAPI handler wiring.
//!
//! ## Canonical key format
//!
//! Per `remote_cache_product_profile.md §7.1` (REG-NAMESPACE-001/-002 +
//! REG-NEGATIVE-002 revised Lote 10.2bis):
//!
//! ```text
//! ac_neg:<region>:<HMAC16>:<digest_hex>
//! ```
//!
//! - `<region>` ∈ `{wnam, weur, sam}` — see [`crate::Region::bucket_suffix`].
//! - `<HMAC16>` is the 16-char URL-safe-base64 HMAC prefix from
//!   [`corelink_tenant_path::derive_prefix`]. **Never** the plaintext
//!   `tenant_id`; cross-tenant negative-cache poisoning is impossible by
//!   construction.
//! - `<digest_hex>` is the 64-char lowercase hex of the BLAKE3 digest.
//!
//! ## TTL bound (PAT-KV-TTL-001)
//!
//! The default TTL is **300 seconds**. Per WI §9.3:
//! - < 60 s: cache miss rate dominates; cost benefit eliminated.
//! - 300 s: balances Bazel build storm window (~5–10 min) against the
//!   stale-window cap.
//! - > 600 s: stale-window > business tolerance.
//!
//! ## Eventual-consistency reality (CF Workers KV)
//!
//! CF Workers KV is eventually consistent globally and offers no atomic CAS
//! primitive (see WI §9.11 + linked CF docs). The negative cache is
//! therefore a **hint**, not an authoritative source-of-truth — the read
//! handler short-circuits on `GetBlob` (single-digest read; per
//! REG-NEGATIVE-002), but `FindMissingBlobs` MUST always fall through to
//! D1 + R2 HEAD per REG-NEGATIVE-001. Stale negatives are bounded by the
//! TTL (≤ 300 s defensible hard bound; typical 60 s propagation but CF
//! docs explicitly say "60 seconds OR MORE"); explicit invalidation on
//! the write path collapses the stale window further but offers no strong
//! ordering guarantee.

pub mod kv;
pub mod miss_reason;
pub mod negative;

pub use kv::{InMemoryKv, KvBackend, KvError};
pub use miss_reason::MissReason;
pub use negative::{NegativeCache, NegativeCacheError, DEFAULT_NEGATIVE_CACHE_TTL_SECS};
