//! `/v2/*` + `/token` — OCI Distribution Spec v1.1 registry surface.
//!
//! Mounts the `corelink_adapter_host::oci` adapter (a full OCI
//! Distribution Spec v1.1 registry: `docker` / `podman` / `buildah` /
//! `containerd` / `crane` / Kubernetes image-pull / BuildKit cache /
//! Helm OCI all speak to it) into the container router. Blob bytes go
//! through the shared 2-level content-dedup [`MoatCache`]; manifests +
//! tag lists go through an in-process KV shell (see the KV seam below).
//!
//! # Path shape — why `.merge`, NOT `nest_service`
//!
//! The Worker forwards the request path UNCHANGED (`pathSuffix: path`,
//! `worker/src/index.ts` `matchRoute` `oci_v2` arm — it slices `/v2`
//! only to derive a routing key, it does NOT rewrite the forwarded
//! path). The adapter's router already registers the exact public OCI
//! paths (`/v2/`, `/v2`, `/v2/_catalog`, `/v2/*rest`, `/token`), so we
//! mount it with [`Router::merge`] and add only a scope-gate layer.
//!
//! This is the load-bearing deviation from `routes/brew.rs`:
//!
//! * brew uses `nest_service("/brew", …)` + strips a leading
//!   `<tenant>` segment, because brew's wire path is
//!   `/brew/<tenant>/<bottle-path>` and the bottle path must be
//!   tenant-free before the upstream fetch.
//! * OCI has NO tenant path segment. The first segment after `/v2/`
//!   is the OCI *repository name* (`/v2/alpine/blobs/…`). Stripping it
//!   would corrupt the repo. The tenant is carried inside the
//!   adapter's HMAC bearer token (minted at `/token`), never in the
//!   path. So the OCI gate does pure per-op scope enforcement and NO
//!   path surgery.
//!
//! # Trust + storage model
//!
//! OCI uses a two-leg auth flow (OCI Distribution Spec v1.1 §auth):
//!
//! 1. The client `GET /token` with `Authorization: Basic
//!    base64(user:<pat>)`. The adapter resolves the PAT via its
//!    [`TenantResolver`] port — here [`OciPatResolver`], a thin shell
//!    over the shared [`crate::adapter_pat::PatVerifier`] (Option B:
//!    full re-verify incl. Argon2id, mapping [`VerifyError`] →
//!    [`crate::oci::ports`]'s `String` `PortResult` error). On success
//!    the adapter mints an HMAC bearer token carrying `(tenant, scope,
//!    expiry)` signed with its OWN `token_signing_key`.
//! 2. The client retries `/v2/*` ops with `Authorization: Bearer
//!    <hmac-token>`. The adapter verifies the HMAC locally and reads
//!    the tenant out of the token — the PAT is NOT re-presented and
//!    the shared verifier is NOT hit on the data plane.
//!
//! Consequently the Option-B PAT re-verify runs once per token
//! exchange, exactly at the [`OciPatResolver`] seam. The per-op
//! `x-corelink-scope` gate ([`oci_gate`]) is defence-in-depth on top of
//! the adapter's own bearer-scope checks.
//!
//! Blob bytes are stored via the shared [`MoatCache`] keyed by the OCI
//! digest string (`sha256:<hex>`) as the moat `url_hash`; the bytes are
//! content-addressed by blake3 INSIDE the moat (the OCI sha256 digest
//! is NOT the CoreLink content hash — the moat map provides exactly the
//! `OCI-digest → blake3-content-hash` indirection). Images are stored
//! under the per-tenant namespace (isolated) — see the public-dedup
//! seam in the module-level OPEN DECISIONS.

#![allow(clippy::too_many_arguments)]

use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use axum::Router;
use bytes::Bytes;
use uuid::Uuid;

use corelink_adapter_host::oci::config::defaults;
use corelink_adapter_host::oci::digest::OciDigest;
use corelink_adapter_host::oci::ports::{
    BlobStore, ManifestKvStore, ManifestResolver, PortResult, ResolvedPat, TenantResolver,
};
use corelink_adapter_host::oci::{router as oci_router, AppState, OciAdapterConfig};
use corelink_audit::ports::{AuditEmitter, InMemoryAuditEmitter};
use corelink_core::{SecretWrap, TenantId};
use corelink_handler_cas::{CasReadHandler, CasWriteHandler};

use crate::adapter_cache::{MoatCache, MoatError, UrlMapStore, PUBLIC_NAMESPACE};
use crate::adapter_pat::{PatVerifier, VerifyError};
use crate::public_base_allowlist::PublicBaseAllowlist;

// B126-M2 REANCHOR MANIFEST (routes/oci).
// Ordered include fragments below are the sole composition point; this keeps
// the module namespace/API and execution order unchanged while preventing
// recomposition into a god-file. Symbols moved: OCI_SERVICE_PRINCIPAL, OCI_MAX_OPEN_SESSIONS_PER_TENANT, OCI_MAX_INFLIGHT_BYTES, OCI_MAX_INFLIGHT_BYTES_PER_TENANT, OCI_SESSION_IDLE_TIMEOUT_MS, OCI_BEARER_REALM, OCI_TOKEN_KEY_ENV, OCI_TOKEN_KEY_ENV_LEGACY, UploadSession, OciMoatStore, now_unix_ms, upload_uuid_belongs_to, OciPatResolver, router, OciCostGate, oci_bearer_tenant, oci_quota_gate.
include!("oci/b126_m2_impl_01.rs");
include!("oci/b126_m2_impl_01_part2.rs");
include!("oci/b126_m2_impl_02.rs");

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
