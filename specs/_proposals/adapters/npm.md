---
id: "PROPOSAL-2026-05-26-ADAPTER-NPM"
type: "governance"
doc_status: "DEFASADO"
audit_status: "ACTIVE"
version: "0.2.0"
created: "2026-05-26"
updated: "2026-05-27"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["proposal", "adapters", "wave-34", "npm", "package-manager"]
references:
---

> **DEFASADO 2026-05-27 — never landed.** This proposal/draft was scoped but did not advance to implementation. Preserved as historical record; no current code references it. See `specs/_audits/2026-05-27-specs-inventory-cleanup-map.md` §3.2 for the inventory triage decision.

# Adapter Contract — npm (Node.js)

**Wave:** 34 · **Crate target:** `corelink-adapter-npm` · **Owner:** Gustavo Schneiter · **Authored:** 2026-05-26 · **Redrafted:** 2026-05-26 v2 (inline-ports mandate)

## 0. PATTERN MANDATE — inline-ports (MUST READ FIRST)

This adapter MUST follow the **inline-ports pattern** convergently established by pip + brew
+ oci on `main` at `98170fb3` / `d1a275f9` / `3d3788dd`. **Do not bind to workspace traits
directly. Do not mutate umbrella `lib.rs` files.** Declare adapter-local port traits
(`CasStore` + `KvStore` + `TenantResolver`) in `src/ports.rs`. Provide in-memory test fakes
colocated.

**Canonical template to mirror:** pip is the closest analogue — both have CAS + KV. See
`crates/corelink-adapter-pip/src/ports.rs` (121 LOC; 3 async traits + handle type aliases).
SEAL audit precedent: `specs/_audits/sealed/2026-05-26-w34-adapter-pip.md` §3.

Document this inline-ports decision in your SEAL audit §3 (mirror pip/brew/oci §3 / §6).

## 1. Headline + scope

CoreLink becomes a **caching npm registry proxy**: `npm install` (and `pnpm` / `yarn` / `bun` — all speak the npm registry wire) consults CoreLink as a transparent caching mirror in front of `registry.npmjs.org`. On miss, the adapter fetches upstream and stores the tarball in CoreLink CAS; on hit, the tarball serves directly from cache. Implementation: HTTP proxy implementing the npm registry API subset that clients actually use (~6 endpoints).

**In scope:** package metadata + tarball caching. Read path (`install`). Tenant-scoped cache namespaces.
**Out of scope:** publishing (write to upstream), private package registry (workspace-internal pkgs), org-permission enforcement, npm audit, deprecation notices.

## 2. Upstream protocol summary

npm registry API (subset used by client install):

- `GET /<package>` — package metadata (versions, dist tags, dependencies). Content-Type `application/json` (or `application/vnd.npm.install-v1+json` for newer clients — slim metadata).
- `GET /<package>/<version>` — single-version metadata.
- `GET /<package>/-/<tarball>.tgz` — package tarball (binary).
- `GET /-/v1/search?text=...` — search (often skipped by `npm install`; can 501).
- `GET /-/ping` — health probe.

Canonical reference: https://github.com/npm/registry/blob/main/docs/REGISTRY-API.md

Clients send `Authorization: Bearer <token>` for private packages; public reads tolerate missing auth.

## 3. Mapping to CoreLink

| npm wire | NpmAdapter port call (all adapter-local async ports) |
|---|---|
| `GET /<pkg>` | `kv.get(&tenant, &format!("meta:{pkg}"))` → on `None` or stale (per TTL): upstream fetch + `kv.put(&tenant, key, json_bytes, now_ms)`; hit fresh → serve cached JSON |
| `GET /<pkg>/-/<tarball>.tgz` | `cas.get(&tenant, &Digest::from(blake3(tarball_url)))` → on `None`: upstream fetch + verify tarball SHA matches metadata-published shasum → `auditor.emit(npm.tarball.cache_write)` (BEFORE put) → `cas.put(...)` |
| `GET /-/ping` | `200 {}` direct, no upstream |
| `GET /-/v1/search` | `501 Not Implemented` (out of scope) |

`CasStore` + `KvStore` + `TenantResolver` are **adapter-local async traits** in
`crates/corelink-adapter-npm/src/ports.rs` (mirror pip's shape). Production wiring deferred
to Wave 35.

**Metadata vs binary blobs:** metadata is short JSON, mutable upstream (new versions get published). Binary tarballs are immutable (npm enforces "no republish under same name+version"). Therefore:

- Tarballs: cache forever under `blake3(url)` (content-addressable; immutable). **Stored in CAS port.**
- Metadata: cache with TTL (5min default; configurable per request via `Cache-Control: max-age` upstream). **Stored in KV port** (returns `(value, inserted_at_unix_ms)` so adapter checks freshness in pure logic, like pip).

## 4. Crate structure

```
crates/corelink-adapter-npm/
├── Cargo.toml
├── src/
│   ├── lib.rs                  # ~50 LOC re-exports
│   ├── server.rs               # axum router + 6 endpoints (~280 LOC)
│   ├── metadata.rs             # GET /<pkg> metadata cache logic (~180 LOC)
│   ├── tarball.rs              # GET /<pkg>/-/<x>.tgz CAS-backed cache (~160 LOC)
│   ├── upstream.rs             # reqwest client for registry.npmjs.org (~120 LOC)
│   ├── auth.rs                 # PAT → tenant resolver (~100 LOC)
│   ├── audit.rs                # audit emit per cache op (~90 LOC)
│   ├── config.rs               # config struct (HUGR_NPM_ADAPTER_*) (~80 LOC)
│   ├── error.rs                # error enum (~60 LOC)
│   └── tests/
│       ├── smoke.rs            # npm install run against adapter (~180 LOC)
│       ├── prop_metadata.rs    # property tests on metadata cache TTL (~80 LOC)
│       └── adversarial.rs      # tarball integrity, forged auth, replay (~140 LOC)
```

Estimated: ~1300 LOC. Per L2.10: every file ≤500 LOC; largest expected `server.rs` (280).

### Dep graph (lean; inline-ports = no workspace SPI deps)

```
corelink-adapter-npm
  ├── corelink-core             (TenantId, Digest, SecretWrap)
  ├── corelink-audit            (only the AuditEmitter trait + AuditEvent type — fail-CLOSED chokepoint)
  ├── corelink-telemetry        (tracing)
  ├── async-trait               (port traits in src/ports.rs)
  ├── axum                      (HTTP server)
  ├── reqwest                   (upstream registry.npmjs.org client)
  ├── secrecy                   (SecretString for PAT)
  ├── subtle                    (ConstantTimeEq for PAT compare)
  └── thiserror                 (error enum)
```

**NO deps on `corelink-cas`, `corelink-auth`, `corelink-handler-cas`, `corelink-worker`,
`corelink-adapters-cloud`, `corelink-reapi`.** Production binding to those Stage-1 surfaces
happens in the Wave 35 `corelink-adapter-host` crate.

## 5. Trait interface

Inline-ports pattern: declare 3 async traits in
`crates/corelink-adapter-npm/src/ports.rs`. Mirror pip's `src/ports.rs` shape exactly (commit
`98170fb3`).

```rust
// crates/corelink-adapter-npm/src/ports.rs
use std::sync::Arc;
use async_trait::async_trait;
use corelink_core::types::{digest::Digest, tenant::TenantId};
use crate::error::NpmAdapterError;

#[async_trait]
pub trait CasStore: Send + Sync + std::fmt::Debug {
    async fn get(&self, tenant: &TenantId, digest: &Digest)
        -> Result<Option<Vec<u8>>, NpmAdapterError>;
    async fn put(&self, tenant: &TenantId, digest: &Digest, bytes: Vec<u8>)
        -> Result<(), NpmAdapterError>;
}

#[async_trait]
pub trait KvStore: Send + Sync + std::fmt::Debug {
    // Returns (value, inserted_at_unix_ms) for freshness check in pure logic.
    async fn get(&self, tenant: &TenantId, key: &str)
        -> Result<Option<(Vec<u8>, u64)>, NpmAdapterError>;
    async fn put(&self, tenant: &TenantId, key: &str, value: Vec<u8>, inserted_at_unix_ms: u64)
        -> Result<(), NpmAdapterError>;
}

#[async_trait]
pub trait TenantResolver: Send + Sync + std::fmt::Debug {
    async fn resolve(&self, pat_plaintext: &str) -> Result<TenantId, NpmAdapterError>;
}

pub type CasStoreHandle = Arc<dyn CasStore>;
pub type KvStoreHandle = Arc<dyn KvStore>;
pub type TenantResolverHandle = Arc<dyn TenantResolver>;
```

Adapter config wires these handles + the workspace `AuditEmitter`:

```rust
#[non_exhaustive]
pub struct NpmAdapterConfig {
    pub bind_addr: std::net::SocketAddr,
    pub upstream_registry: url::Url,   // default https://registry.npmjs.org
    pub metadata_ttl_seconds: u64,     // default 300
    pub tarball_size_limit_bytes: u64, // default 256 MiB
    pub cas: crate::ports::CasStoreHandle,
    pub metadata_kv: crate::ports::KvStoreHandle,
    pub tenant_resolver: crate::ports::TenantResolverHandle,
    pub auditor: std::sync::Arc<dyn corelink_audit::ports::AuditEmitter>,
}

pub async fn run_npm_adapter(config: NpmAdapterConfig) -> Result<(), NpmAdapterError>;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum NpmAdapterError {
    #[error("bind: {0}")]
    Bind(std::io::Error),
    #[error("auth: {0}")]
    Auth(String),
    #[error("cas: {0}")]
    Cas(String),
    #[error("kv: {0}")]
    Kv(String),
    #[error("upstream: {0}")]
    Upstream(String),
    #[error("audit: {0}")]
    Audit(String),
    #[error("tarball exceeds limit: {0} bytes")]
    TarballOversized(u64),
}
```

## 6. Auth + multi-tenancy

npm clients send `Authorization: Bearer <token>`. The adapter expects `Bearer hugr-pat_<token>` (CoreLink PAT). Calls `tenant_resolver.resolve(pat_plaintext).await?` (adapter-local port from `src/ports.rs`); successful resolve returns `TenantId`.

**Multi-tenant cache:** the adapter passes `&tenant_id` into every `cas.{get,put}` and
`kv.{get,put}` call. The production wiring of these ports (Wave 35) is responsible for
enforcing tenant prefix derivation (`corelink_tenant_path::derive_prefix`). From the npm
adapter's perspective, tenancy is opaque — `TenantId` is threaded through.

PAT scopes required: `npm:read` (custom scope; the production `TenantResolver` impl
validates against the scope catalog). The adapter does NOT validate scopes itself.

**Anonymous reads (no Authorization):** REJECTED at v1. The "public shared namespace"
feature is post-GA (depends on cross-tenant dedup). Documented in §10.

Constant-time PAT comparison happens inside the `TenantResolver` impl, not in the adapter
handler.

## 7. Cache invalidation

| Cache type | Invalidation strategy |
|---|---|
| **Tarball (CAS)** | Never invalidated. npm registry enforces tarball immutability (republish blocked at upstream). Aged out via `corelink-gc`. |
| **Metadata (KV)** | TTL-based. Default 300s. Operator override via env. After TTL: next request fetches upstream, refreshes KV entry. |

Operator-side force-invalidation: out of scope; handled via admin plane.

**Stale metadata edge case:** if `lodash@4.17.21` is unpublished upstream, our cached metadata may still list it. Acceptable per §10 (best-effort cache, not source-of-truth registry).

## 8. Tests

| Tier | Tests | Acceptance |
|---|---|---|
| **Smoke (e2e)** | (1) Spin adapter on localhost. (2) `npm install lodash` with `npm config set registry http://localhost:<port>`. (3) Re-run install → should hit cache (`npm install --offline` succeeds). | npm install completes both runs; second run faster + no upstream HTTP traffic |
| **Property** | Metadata TTL: cached entry served fresh under TTL; expired entry triggers upstream refresh; offline upstream with cached entry serves cached. | Round-trip + boundary + offline cases |
| **Adversarial** | (1) Forged Bearer token → 401 + audit. (2) Tarball larger than `tarball_size_limit_bytes` → 413 + audit. (3) Upstream returns tampered tarball (hash mismatch vs metadata) → reject + audit. (4) Tenant A cannot access tenant B's tarball even if both downloaded same package version. (5) Replay attack: same tarball URL, different bytes upstream → upstream change detected via metadata hash; refresh. | All 5 pass; 0 leaks |

Test infra (all adapter-local or workspace-canonical):

| Role | Fake/impl | Location |
|---|---|---|
| CAS read+write | in-memory `HashMap<(TenantId, Digest), Vec<u8>>` impl of `crate::ports::CasStore` | `tests/common.rs` (colocated) |
| Metadata KV | in-memory `HashMap<(TenantId, String), (Vec<u8>, u64)>` impl of `crate::ports::KvStore` (returns `(value, ts)`) | `tests/common.rs` |
| Tenant resolver | `FixedTenant { tenant_id }` impl of `crate::ports::TenantResolver` (constant-time PAT compare on fixture) | `tests/common.rs` |
| Audit emit | `corelink_audit::ports::InMemoryAuditEmitter` (sync; existing workspace fake) | workspace dep |
| Upstream registry | `wiremock` (mock HTTP server) | dev-dep |

Reference pip's `tests/common.rs` (commit `e279b296`) for the canonical fake layout. No real
npm binary required except in smoke tier.

## 9. Acceptance criteria

- [ ] `cargo build -p corelink-adapter-npm`: green
- [ ] `cargo test -p corelink-adapter-npm`: ≥12 tests passing (smoke + property + adversarial)
- [ ] `cargo clippy -p corelink-adapter-npm --all-targets -- -D warnings`: clean
- [ ] wasm32 workspace build still green
- [ ] Every public type `#[non_exhaustive]`
- [ ] `#![forbid(unsafe_code)]`
- [ ] Zero `unwrap/expect/panic!` in `src/`
- [ ] Every state-mutation emits audit via `AuditEmitter` before returning success
- [ ] PAT wrapped in `SecretString`; constant-time compare
- [ ] Tarball integrity verified post-download (hash matches metadata-published hash) BEFORE storing in CAS
- [ ] L2.10: no `.rs` file >500 LOC; sweet-spot ≤200
- [ ] SEAL audit at `specs/_audits/2026-05-2X-w34-adapter-npm.md`
- [ ] Smoke test §8 row 1 reaches second-run cache hit assertion
- [ ] DCO + Co-Authored-By

## 10. Out of scope / explicit deferrals

- **Publishing (`npm publish`).** Read-only cache mirror only. Customers publish to upstream npmjs.org directly.
- **Private package registries.** Custom `@scope/pkg` namespace mirroring requires a separate config layer; defer to v2.
- **Cross-tenant deduplication.** Each tenant has its own `lodash@4.17.21` cached blob even when bytes identical. Post-GA roadmap.
- **Anonymous reads / public namespace.** v1 requires PAT. The "public shared cache for open-source packages" is a future iteration tied to cross-tenant dedup.
- **`npm audit` API.** Vulnerability metadata is upstream-live; not cached.
- **Deprecation/unpublish propagation.** Cached metadata can be stale; documented as best-effort.
- **Sub-second metadata refresh on dist-tag change.** TTL is coarse (5 min default); good enough for build reproducibility.

---

## Sign-off

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>.
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>.

**End of npm Adapter Contract.**
