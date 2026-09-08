//! `GET/PUT /v8/artifacts/:hash` + `POST /v8/artifacts/events` +
//! `POST /v8/artifacts/status` — Vercel Turborepo remote-cache protocol.
//!
//! # Why this module exists
//!
//! Turborepo (by Vercel) supports any HTTP server that implements the
//! Vercel Remote Cache `/v8/artifacts` API. Wiring CoreLink as that server
//! lets teams replace Vercel's paid remote cache with their own CAS-backed
//! store by setting:
//!
//! ```text
//! TURBO_API=https://corelink-api.humangr.com
//! TURBO_TOKEN=<their CoreLink PAT>
//! ```
//!
//! # Backing store
//!
//! The route state is backed by the **durable**, per-tenant R2-backed
//! [`R2KvStore`](crate::storage::r2_kv::R2KvStore) whenever storage
//! credentials are configured (`StorageEnv::from_env()`), so artifacts
//! persist across container restarts. Only the no-creds dev / CI path
//! falls back to the in-RAM [`InMemoryKvStore`]; if creds ARE present but
//! `R2KvStore` refuses to build, the handler fails CLOSED (503s every verb)
//! rather than silently losing durability. The backing store is a swappable
//! port trait ([`CasReadStore`] / [`CasWriteStore`]); the route handlers and
//! audit surface are identical across both backings. See `build_handlers`.
//!
//! # Tenant isolation
//!
//! The isolation tenant is the DO-injected, PAT-resolved authenticated tenant
//! (`AuthTenant` / `x-corelink-tenant-id`), threaded in as `caller_tenant`. The
//! `teamId` query parameter is required on PUT and GET (missing `teamId` → 400)
//! but is NOT a security boundary: it is a Turborepo team label demoted to a
//! logical sub-namespace WITHIN the authenticated tenant. Storage is keyed
//! `tenant = auth.0`, `key = "<teamId>/<hash>"`, so teams under one tenant stay
//! partitioned while cross-tenant access is impossible (no authenticated tenant
//! ⇒ 401 fail-CLOSED at the extractor).
//!
//! # Hash semantics
//!
//! Turbo's artifact hash is OPAQUE. It is stored verbatim as the KV key without
//! any hash-integrity verification. Hashes longer than
//! [`corelink_turbo_bridge::MAX_HASH_LEN`] (128 chars) are rejected with 400
//! as a DoS guard — see [`corelink_turbo_bridge::TurboBridgeError::HashTooLong`].
//!
//! # Route constants
//!
//! MUST use the matchit-0.7 `:name` capture syntax.  `{name}` form is silently
//! treated as a literal path segment in matchit 0.7.3 — see DEBT-029 comment in
//! `routes/cas.rs` for the full rationale.

#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex as AsyncMutex, OwnedSemaphorePermit, Semaphore};

use crate::wall_clock::{SystemWallClock, WallClock};
use corelink_turbo_bridge::{
    adapter::{CasAdapterTurboHandler, InMemoryKvStore},
    audit::InMemoryTurboAuditSink,
    error::validate_team_id,
    TurboArtifactHandler, TurboBridgeError, TurboEventsRequest, TurboEventsResponse,
    TurboGetRequest, TurboGetResponse, TurboPutRequest, TurboPutResponse, TurboStatusRequest,
    TurboStatusResponse,
};

// ── Route constants ───────────────────────────────────────────────────────────

// B126-M2 REANCHOR MANIFEST (routes/turbo_v8).
// Ordered include fragments below are the sole composition point; this keeps
// the module namespace/API and execution order unchanged while preventing
// recomposition into a god-file. Symbols moved: TURBO_GET_ROUTE, TURBO_PUT_ROUTE, TURBO_EVENTS_ROUTE, TURBO_STATUS_ROUTE, ARTIFACT_TAG_HEADER, TURBO_BODY_LIMIT_BYTES, TURBO_PUT_CONCURRENCY_LIMIT, TURBO_GET_CONCURRENCY_LIMIT, EVENTS_BODY_LIMIT_BYTES, GLOBAL_TURBO_PUT_PERMITS, GLOBAL_TURBO_GET_PERMITS, GLOBAL_TURBO_EVENTS_PERMITS, EVENTS_CONCURRENCY_LIMIT, EVENTS_BODY_READ_TIMEOUT, GLOBAL_PUT_PERMIT_WAIT, TURBO_WRITE_LOCK_SHARDS, ArtifactQuery, PutArtifactResponse, TurboRouteState, global_turbo_put_budget, global_turbo_get_budget, global_turbo_events_budget, new_write_locks, write_lock_shard, PutSlot, PutConcurrencyGuard, GlobalPutBudgetGuard, GetSlot, GetConcurrencyGuard, GlobalGetBudgetGuard, EventsBudgetGuard, EventsSlot, EventsConcurrencyGuard, build_handlers, router, handle_get, handle_put, handle_events, handle_status, TURBO_STORAGE_UNAVAILABLE_SENTINEL, UnavailableTurboHandler, map_err.
include!("turbo_v8/b126_m2_impl_01.rs");
include!("turbo_v8/b126_m2_impl_01_part2.rs");
include!("turbo_v8/b126_m2_impl_02.rs");
include!("turbo_v8/b126_m2_impl_02_part2.rs");

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
