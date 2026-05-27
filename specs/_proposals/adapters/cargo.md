---
id: "PROPOSAL-2026-05-26-ADAPTER-CARGO"
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
tags: ["proposal", "adapters", "wave-34", "cargo", "package-manager"]
references:
---

> **DEFASADO 2026-05-27 — never landed.** This proposal/draft was scoped but did not advance to implementation. Preserved as historical record; no current code references it. See `specs/_audits/2026-05-27-specs-inventory-cleanup-map.md` §3.2 for the inventory triage decision.

# Adapter Contract — Cargo (Rust)

**Wave:** 34 (Adapter Campaign) · **Crate target:** `corelink-adapter-cargo` · **Owner:** Gustavo Schneiter · **Authored:** 2026-05-26 · **Redrafted:** 2026-05-26 v2 (inline-ports mandate)

## 0. PATTERN MANDATE — inline-ports (MUST READ FIRST)

This adapter MUST follow the **inline-ports pattern** convergently established by pip + brew
+ oci on `main` at `98170fb3` / `d1a275f9` / `3d3788dd`. **Do not bind to workspace traits
directly. Do not mutate umbrella `lib.rs` files.** Declare adapter-local port traits in
`src/ports.rs`. Provide in-memory test fakes colocated in the same file.

**Canonical templates to mirror** (look at brew first; cargo is the closest analogue — both
CAS-only, no KV):

- `crates/corelink-adapter-brew/src/ports.rs` — async `CasStore` + `TenantResolver`; ~107 LOC.
- `crates/corelink-adapter-pip/src/ports.rs` — adds `KvStore` (not needed here); ~121 LOC.
- `specs/_audits/sealed/2026-05-26-w34-adapter-brew.md` §6 + §3 — SEAL audit precedent.

Document this inline-ports decision in your SEAL audit §3 (mirrors the brew/pip/oci §3 / §6).

## 1. Headline + scope

CoreLink becomes Cargo's **remote build cache**: every `cargo build` consults CoreLink for the fingerprint→artifact mapping before recompiling locally. On hit, the artifact is downloaded; on miss, the local build proceeds and the result is uploaded for future reuse. Implementation pattern: **sccache-compatible backend** (sccache already supports HTTP storage; we conform to its wire). Bridge mode for adoption: a sidecar HTTP server exposing the CoreLink CAS through the sccache HTTP cache shape.

**In scope:** binary artifact caching for `rustc` invocations (one-line `RUSTC_WRAPPER=sccache` flip; no cargo source code changes required).
**Out of scope:** cargo registry mirror, source-level caching, cargo-lock pinning, dependency resolution.

## 2. Upstream protocol summary

sccache's `HTTP` storage backend (the canonical wire we conform to):

- `GET /<key>` — read artifact. 200 with bytes / 404 not found.
- `PUT /<key>` — write artifact. 200 / 201 on success. Body is opaque bytes.
- `HEAD /<key>` — existence check.

Key format: `blake3(rustc-cmdline + input-fingerprints)`. sccache uses BLAKE3 (same as CoreLink CAS — alignment is exact).

Canonical reference: https://github.com/mozilla/sccache/blob/main/docs/HTTP.md

## 3. Mapping to CoreLink

| sccache wire | CargoAdapter port call (adapter-local trait, async, tenant-scoped) |
|---|---|
| `GET /<key>` | `cas.get(&tenant_id, &Digest::from(key))` → `Ok(Some(bytes))` (hit) / `Ok(None)` (miss) → 200 / 404 |
| `PUT /<key>` | `cas.put(&tenant_id, &Digest::from(key), bytes)` after `auditor.emit(cargo.cache.write)` (fail-CLOSED — audit BEFORE put) |
| `HEAD /<key>` | `cas.get(&tenant_id, &Digest::from(key))` → 200 (Some) / 404 (None); discard bytes. No separate `exists()` on the port — use `get` and ignore the body. |

`CasStore` is the **adapter-local trait** in `crates/corelink-adapter-cargo/src/ports.rs` (mirror brew's shape). Production wiring (bridging cargo's `CasStore` to workspace `CasReadHandler` + `CasWriteHandler` from `corelink-handler-cas`) is **Wave 35** — out of scope here.

sccache's BLAKE3 keys ARE valid `Digest` values for CoreLink (both 32-byte BLAKE3). Zero translation cost.

## 4. Crate structure

```
crates/corelink-adapter-cargo/
├── Cargo.toml
├── src/
│   ├── lib.rs                  # public surface re-exports (~50 LOC)
│   ├── server.rs               # HTTP server (axum) bridging sccache wire ↔ CoreLink (~250 LOC)
│   ├── translate.rs            # sccache key ↔ CoreLink Digest (~80 LOC)
│   ├── auth.rs                 # tenant resolution from token header (~100 LOC)
│   ├── audit.rs                # audit emit per cache hit/miss/write (~80 LOC)
│   ├── config.rs               # config struct (HUGR_CARGO_ADAPTER_*) (~80 LOC)
│   ├── error.rs                # error enum (~60 LOC)
│   └── tests/
│       ├── smoke.rs            # real sccache + adapter + CoreLink CAS end-to-end (~150 LOC)
│       ├── prop_translate.rs   # property tests on key translation (~80 LOC)
│       └── adversarial.rs      # forged tokens, replay, oversize bodies (~120 LOC)
```

Estimated total: ~1050 LOC across 9-10 files. Per L2.10: every file ≤500 LOC. Largest expected: `server.rs` (250 LOC).

### Dep graph (lean; inline-ports = no workspace SPI deps)

```
corelink-adapter-cargo
  ├── corelink-core             (TenantId, Digest, SecretWrap)
  ├── corelink-audit            (only the AuditEmitter trait + AuditEvent type — fail-CLOSED contract)
  ├── corelink-telemetry        (tracing instrumentation)
  ├── async-trait               (port traits declared in src/ports.rs)
  ├── axum                      (HTTP server)
  ├── secrecy                   (SecretString for PAT plaintext)
  ├── subtle                    (ConstantTimeEq for PAT compare)
  └── thiserror                 (error enum)
```

**NO deps on `corelink-cas`, `corelink-auth`, `corelink-reapi`, `corelink-handler-cas`,
`corelink-worker`, `corelink-adapters-cloud`.** Production binding to those Stage-1 surfaces
happens in the Wave 35 `corelink-adapter-host` crate, not here.

## 5. Trait interface (public Rust)

Inline-ports pattern: declare `CasStore` + `TenantResolver` as **adapter-local async traits**
in `crates/corelink-adapter-cargo/src/ports.rs`. Mirror brew's shape.

```rust
// crates/corelink-adapter-cargo/src/ports.rs
use std::sync::Arc;
use async_trait::async_trait;
use corelink_core::types::{digest::Digest, tenant::TenantId};
use crate::error::CargoAdapterError;

#[async_trait]
pub trait CasStore: Send + Sync + std::fmt::Debug {
    async fn get(&self, tenant: &TenantId, digest: &Digest)
        -> Result<Option<Vec<u8>>, CargoAdapterError>;
    async fn put(&self, tenant: &TenantId, digest: &Digest, bytes: Vec<u8>)
        -> Result<(), CargoAdapterError>;
}

#[async_trait]
pub trait TenantResolver: Send + Sync + std::fmt::Debug {
    async fn resolve(&self, pat_plaintext: &str) -> Result<TenantId, CargoAdapterError>;
}

pub type CasStoreHandle = Arc<dyn CasStore>;
pub type TenantResolverHandle = Arc<dyn TenantResolver>;
```

Adapter config wires these handles + the `AuditEmitter` from `corelink-audit` (the one
workspace trait the adapter consumes directly — see Stage 0 SEAL §4 chokepoint rationale).

```rust
#[non_exhaustive]
pub struct CargoAdapterConfig {
    pub bind_addr: std::net::SocketAddr,
    pub cas: crate::ports::CasStoreHandle,
    pub tenant_resolver: crate::ports::TenantResolverHandle,
    pub auditor: std::sync::Arc<dyn corelink_audit::ports::AuditEmitter>,
}

pub async fn run_cargo_adapter(config: CargoAdapterConfig) -> Result<(), CargoAdapterError>;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CargoAdapterError {
    #[error("bind failed: {0}")]
    Bind(std::io::Error),
    #[error("auth: {0}")]
    Auth(String),
    #[error("cas: {0}")]
    Cas(String),
    #[error("audit: {0}")]
    Audit(String),
}
```

**Test fakes (colocated):**

- `CasStore` fake: in-memory `HashMap<(TenantId, Digest), Vec<u8>>` impl in `tests/common.rs`.
- `TenantResolver` fake: `FixedTenant { tenant_id: TenantId }` impl returning the fixed
  tenant if PAT plaintext matches a fixture; rejecting otherwise (constant-time compare via
  `subtle::ConstantTimeEq`).
- `AuditEmitter` fake: use the existing workspace `corelink_audit::ports::InMemoryAuditEmitter`
  (sync emit; captures events in `Arc<Mutex<Vec<AuditEvent>>>`).

**Do NOT** import `corelink_cas::CasStore`, `corelink_auth::TenantResolver`,
`corelink_handler_cas::CasReadHandler`, or any other workspace SPI. Production bridges live
in the Wave 35 `corelink-adapter-host` crate.

## 6. Auth + multi-tenancy

sccache's HTTP backend supports a single `Authorization: Bearer <token>` header. We require it: every request MUST carry `Authorization: Bearer hugr-pat_<token>`. The adapter passes the PAT plaintext to its adapter-local `TenantResolver::resolve(pat_plaintext)` port (declared in `src/ports.rs`); a successful resolve returns `TenantId`, which is then passed to every `cas.get` / `cas.put` call.

Multi-tenant: the adapter passes `&tenant_id` into every `CasStore::{get,put}` call. The PRODUCTION wiring of `CasStore` (deferred to Wave 35) is responsible for enforcing tenant prefix derivation via `corelink_tenant_path::derive_prefix`. From the cargo adapter's perspective, tenancy is opaque — it just threads `TenantId` through.

PAT scopes required: `cas:read` + `cas:write` for cargo cache use. The cargo adapter does NOT validate scopes itself — that's the production `TenantResolver` impl's responsibility (returns error if PAT lacks scope). Audit emit per `(tenant_id, key, operation)` BEFORE the `cas.put` call (audit-fail-CLOSED contract).

Reject (HTTP 401 + audit emit `cargo.adapter.auth_failed` via the workspace `AuditEmitter`):
- Missing `Authorization` header.
- `TenantResolver::resolve` returns `Err(CargoAdapterError::Auth(...))` (PAT invalid / scope insufficient / revoked).

Constant-time PAT comparison happens inside the `TenantResolver` impl, not in the adapter handler.

## 7. Cache invalidation

sccache wire is **append-only** (no DELETE in the HTTP spec). CoreLink CAS is **content-addressable** (digest = content; updating a key is structurally impossible). Therefore: no invalidation is exposed to cargo. Stale artifacts age out via CoreLink's GC (per `corelink-gc::SweepPlan`), based on access timestamp.

Operator-side invalidation (admin force-delete a cache entry): out of scope for cargo adapter; happens via the admin plane (`corelink-ops::admin`).

## 8. Tests

| Tier | Tests | Acceptance |
|---|---|---|
| **Smoke (e2e)** | Real sccache binary + adapter + in-memory CoreLink. `cargo build` against a 3-crate workspace; second build should be 100% cache hit (per `sccache --stats`). | `sccache --stats` shows `cache hits: 3/3` on second run |
| **Property** | Key translation: sccache 32-byte BLAKE3 ↔ `Digest`. Roundtrip property: `digest_to_key(key_to_digest(k)) == k` for all 32-byte inputs. | 1k cases pass; 10k nightly |
| **Adversarial** | (1) Forged Bearer token → 401 + audit row. (2) Oversize PUT body (>16 MiB) → 413 + audit row. (3) Tenant A reads tenant B's key → MUST miss (different namespace). (4) Replay attack (same key, different content) → second PUT rejected (CAS immutability). | All 4 pass; 0 leaks |

Test infra (all adapter-local or workspace-canonical):

| Role | Fake/impl | Location |
|---|---|---|
| CAS read+write | in-memory `HashMap<(TenantId, Digest), Vec<u8>>` impl of `crate::ports::CasStore` | `tests/common.rs` (colocated) |
| Tenant resolver | `FixedTenant { tenant_id: TenantId }` impl of `crate::ports::TenantResolver` (constant-time PAT compare on fixture match) | `tests/common.rs` |
| Audit emit | `corelink_audit::ports::InMemoryAuditEmitter` (sync; existing workspace fake) | workspace dep |
| HTTP wire mock | `wiremock` | dev-dep |

No real sccache binary required for non-smoke tests (can stub the wire). Reference brew's
`tests/common.rs` (commit `42a12337`) for the canonical fake layout.

## 9. Acceptance criteria

Agent has produced a SEALable crate when ALL of the following:

- [ ] `cargo build -p corelink-adapter-cargo`: green
- [ ] `cargo test -p corelink-adapter-cargo`: ≥10 tests passing (smoke + property + adversarial per §8)
- [ ] `cargo clippy -p corelink-adapter-cargo --all-targets -- -D warnings`: clean
- [ ] `cargo build --target wasm32-unknown-unknown -p corelink-clerk-cf`: green (preserved)
- [ ] Every public type has `#[non_exhaustive]`
- [ ] `#![forbid(unsafe_code)]` at crate root
- [ ] Zero `unwrap()/expect()/panic!()` in `src/` (test-mod `#[cfg(test)] #[allow(...)]` OK)
- [ ] Every state-mutation path emits audit via `corelink_audit::ports::AuditEmitter::emit` before returning success
- [ ] All credentials (PAT) wrapped in `SecretString` from `secrecy`; constant-time compare via `subtle::ConstantTimeEq`
- [ ] L2.10: no `.rs` file in the crate >500 LOC; sweet-spot ≤200 LOC per file
- [ ] SEAL audit at `specs/_audits/2026-05-2X-w34-adapter-cargo.md` per template
- [ ] Smoke test §8 row 1 reaches "cache hits: 3/3" assertion
- [ ] DCO sign-off + Co-Authored-By trailer on every commit

## 10. Out of scope / explicit deferrals

- **Cargo registry mirror.** This adapter only handles BUILD cache (compiled artifacts). Source crates still come from crates.io. A registry-mirror adapter is a separate contract (defer to Wave 35+).
- **Cross-tenant dedup.** Per-tenant only at v1. The "shared open-source cache" use case (every customer benefits from `tokio v1.40` having been built once) requires cross-tenant dedup, which is on the post-GA roadmap.
- **WASM target.** Adapter binary is native (Linux/macOS); the CAS backend it calls IS reachable via HTTPS so it works for any platform that can run sccache. WASM/edge-side adapter not needed.
- **Stale-entry warning.** Sccache uses BLAKE3 which collision-resists; we don't proactively warn about stale entries (let GC handle).
- **Per-user namespace within a tenant.** PAT resolves to tenant; CI runners within a tenant share keyspace. If per-user isolation needed, that's a future PAT-scope refinement.

---

## Sign-off

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>.
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>.

**End of Cargo Adapter Contract.**
