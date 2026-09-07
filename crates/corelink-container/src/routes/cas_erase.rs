//! `POST /_internal/cas/:tenant/:hash/erase` — per-hash CAS erase + 410-Gone
//! tombstone (hugit-P2 seam B, WP-B).
//!
//! Operator/internal **write-side** endpoint (off the hot GET path). Deletes a
//! single content-addressed blob from the cold `corelink-cas-prod` R2 bucket
//! and writes a durable tombstone so subsequent `GET /v1/cas/:tenant/:hash`
//! returns HTTP **410 Gone** (never 404, never 200). The pure decision logic
//! lives in [`corelink_handler_cas_erase`]; this module is the transport wiring.
//!
//! # Auth
//!
//! Gated by the same constant-time `X-Corelink-Internal-Auth` shared-secret as
//! [`crate::routes::internal_pat`] (the DO is the only caller). The body carries
//! the authenticated `tenant` + an audit `reason`; the path `:tenant` is a
//! client echo that MUST equal the body tenant (cross-tenant ⇒ 403).
//!
//! # Composition with DSR Wave 1 (PR #254) — the R2 erase seam
//!
//! The blob byte-deletion is expressed through the [`CasBlobEraser`] trait. Its
//! production implementation reuses the **DSR Wave 1 R2 CAS primitives**
//! (`R2S3Client::{delete, list_objects_v2}` + `R2S3Client::blob_key`, added on
//! branch `feat/dsr-account-deletion`, PR #254 increment 3) — it LISTs the
//! tenant prefix across the five CAS regions and DELETEs the object(s) for the
//! one digest, exactly the per-key cousin of the DSR tenant-wide erase. WP-B
//! does **not** duplicate those primitives; the trait is the frozen seam #254
//! fills. The production [`R2CasBlobEraser`] reuses `R2S3Client` directly and
//! derives the tenant prefix the SAME way the CAS writer did (so the erase key
//! matches the stored object by construction); [`InMemoryBlobEraser`] backs the
//! unit tests. The env builder ([`build_state_from_env`]) returns `Some` only
//! when the R2 TDK + D1 tombstone store + internal-auth key are all present,
//! and `None` otherwise (route fail-CLOSED / unmounted) so no half-built erase
//! — and, critically, no wrong-key silent-no-op — can run in prod.
//!
//! # Tombstone store
//!
//! [`TombstoneStore`] persists/queries the `cas_tombstone` D1 table (migration
//! `0067`). [`D1TombstoneStore`] backs it over [`crate::storage::d1_http`];
//! [`InMemoryTombstoneStore`] backs the unit tests.

use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use corelink_tenant_path::{derive_prefix, TenantDerivationKey};
use subtle::ConstantTimeEq;
use uuid::Uuid;
use zeroize::Zeroizing;

use corelink_handler_cas_erase::{erase_outcome, prepare_erase, EraseOutcome};
use corelink_privacy_erasure_worker::legitimacy::DsrLegitimacyStore;

use crate::container_capacity::{BLOOM_CACHE_BIT_ARRAY_BYTES, BLOOM_CACHE_METADATA_BYTES};
use crate::routes::dsr::legitimacy::D1DsrLegitimacyStore;

// B126-M2 REANCHOR MANIFEST (routes/cas_erase).
// Ordered include fragments below are the sole composition point; this keeps
// the module namespace/API and execution order unchanged while preventing
// recomposition into a god-file. Symbols moved: INTERNAL_AUTH_HEADER, CAS_ERASE_ROUTE, MAX_REASON_LEN, TombstoneStore, CasBlobEraser, CasEraseRouteState, router, EraseBody, handle_erase, internal_auth_ok, now_unix_ms, D1TombstoneStore, DEFAULT_BLOOM_BITS, DEFAULT_BLOOM_HASHES, DEFAULT_BLOOM_REFRESH, DEFAULT_MAX_TENANT_BLOOMS, MAX_BLOOM_TENANT_ID_BYTES, MAX_BLOOM_ENTRY_METADATA_BYTES, _, Bloom, TenantBloom, TenantBloomEntry, BloomTombstoneStore, TOMBSTONE_GONE_SENTINEL, TOMBSTONE_UNAVAILABLE_SENTINEL, TombstoneGatedCasHandler, InMemoryTombstoneStore, lock_or_recover, InMemoryBlobEraser, DEFAULT_CAS_BUCKET, TENANT_PREFIX_LEN, R2CasBlobEraser, build_state_from_env, load_tdk_from_env.
include!("cas_erase/b126_m2_impl_01.rs");
include!("cas_erase/b126_m2_impl_01_part2.rs");
include!("cas_erase/b126_m2_impl_02.rs");
include!("cas_erase/b126_m2_impl_02_part2.rs");

#[cfg(test)]
mod b126_m2_reanchor {
    #[test]
    fn implementation_fragments_are_wired() {
        let _ = [
            super::B126_M2_IMPL_1_REANCHOR,
            super::B126_M2_IMPL_2_REANCHOR,
        ];
    }
}
