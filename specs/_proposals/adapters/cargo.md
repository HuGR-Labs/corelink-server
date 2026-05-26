---
id: "PROPOSAL-2026-05-26-ADAPTER-CARGO"
type: "governance"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "0.1.0"
created: "2026-05-26"
updated: "2026-05-26"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["proposal", "adapters", "wave-34", "cargo", "package-manager"]
references:
---

# Adapter Contract — Cargo (Rust)

**Wave:** 34 (Adapter Campaign) · **Crate target:** `corelink-adapter-cargo` · **Owner:** Gustavo Schneiter · **Authored:** 2026-05-26

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

| sccache wire | CoreLink call |
|---|---|
| `GET /<key>` | `corelink_cas::CasStore::get(tenant_id, Digest(<key>))` |
| `PUT /<key>` | `corelink_cas::CasStore::put(tenant_id, Digest(<key>), bytes)` + audit emit `cargo.cache.write` |
| `HEAD /<key>` | `corelink_cas::CasStore::exists(tenant_id, Digest(<key>))` |

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

### Dep graph

```
corelink-adapter-cargo
  ├── corelink-cas              (REAPI CAS surface)
  ├── corelink-auth             (tenant resolution from PAT)
  ├── corelink-audit            (audit emit fail-CLOSED)
  ├── corelink-core             (types: TenantId, Digest, SecretWrap)
  └── corelink-telemetry        (tracing instrumentation)
```

NO direct deps on `corelink-reapi`, `corelink-billing`, etc. Lean.

## 5. Trait interface (public Rust)

```rust
#[non_exhaustive]
pub struct CargoAdapterConfig {
    pub bind_addr: std::net::SocketAddr,
    pub upstream_cas: std::sync::Arc<dyn corelink_cas::CasStore>,
    pub tenant_resolver: std::sync::Arc<dyn corelink_auth::TenantResolver>,
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

Single function entry. No factory complexity. Test fakes for `CasStore`, `TenantResolver`, `AuditEmitter` already exist in `corelink-cas` + `corelink-auth` + `corelink-audit` (`InMemory*` types).

## 6. Auth + multi-tenancy

sccache's HTTP backend supports a single `Authorization: Bearer <token>` header. We require it: every request MUST carry `Authorization: Bearer hugr-pat_<token>` where the PAT resolves to a tenant via `corelink-auth::TenantResolver`.

Multi-tenant: each tenant has its own keyspace at the wire level via the resolved `tenant_id`. The HTTP path (`/<key>`) is tenant-unaware; the adapter prefixes internally before calling `CasStore`. Cargo client doesn't know.

PAT scopes required: `cas:read` + `cas:write` for cargo cache use. Audit emit per `(tenant_id, key, operation)`.

Reject (HTTP 401 + audit emit `cargo.adapter.auth_failed`):
- Missing `Authorization` header.
- PAT signature invalid (subtle::ConstantTimeEq compare; per `corelink-auth`).
- PAT scope insufficient.
- PAT revoked.

## 7. Cache invalidation

sccache wire is **append-only** (no DELETE in the HTTP spec). CoreLink CAS is **content-addressable** (digest = content; updating a key is structurally impossible). Therefore: no invalidation is exposed to cargo. Stale artifacts age out via CoreLink's GC (per `corelink-gc::SweepPlan`), based on access timestamp.

Operator-side invalidation (admin force-delete a cache entry): out of scope for cargo adapter; happens via the admin plane (`corelink-ops::admin`).

## 8. Tests

| Tier | Tests | Acceptance |
|---|---|---|
| **Smoke (e2e)** | Real sccache binary + adapter + in-memory CoreLink. `cargo build` against a 3-crate workspace; second build should be 100% cache hit (per `sccache --stats`). | `sccache --stats` shows `cache hits: 3/3` on second run |
| **Property** | Key translation: sccache 32-byte BLAKE3 ↔ `Digest`. Roundtrip property: `digest_to_key(key_to_digest(k)) == k` for all 32-byte inputs. | 1k cases pass; 10k nightly |
| **Adversarial** | (1) Forged Bearer token → 401 + audit row. (2) Oversize PUT body (>16 MiB) → 413 + audit row. (3) Tenant A reads tenant B's key → MUST miss (different namespace). (4) Replay attack (same key, different content) → second PUT rejected (CAS immutability). | All 4 pass; 0 leaks |

Test infra: `wiremock` for HTTP-side mocking; `InMemoryCasStore` + `InMemoryTenantResolver` + `InMemoryAuditEmitter` for backends. No real sccache binary required for non-smoke tests (can stub the wire).

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
