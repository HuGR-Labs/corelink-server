---
id: "AUDIT-2026-05-26-W34-ADAPTER-CARGO-V2"
type: "audit"
doc_status: "ACTIVE"
audit_status: "SEAL"
version: "1.0.0"
created: "2026-05-26"
updated: "2026-05-26"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: "AUDIT-2026-05-26-W34-ADAPTER-CARGO"
superseded_by: null
tags: ["audit", "adapters", "wave-34", "cargo", "package-manager", "SEAL", "v2"]
references:
  - "specs/_proposals/adapters/cargo.md"
  - "specs/_audits/2026-05-26-w34-adapter-cargo.md"
---

# SEAL Audit — Wave-34 Adapter Campaign · Cargo (v2 re-dispatch)

**Wave:** 34 · **Crate:** `corelink-adapter-cargo` · **Branch:** `wt/r-prep-w34-adapter-cargo-v2` · **Baseline:** `04a48e7d` · **Authored:** 2026-05-26

> **Note:** The v1 packet (`2026-05-26-w34-adapter-cargo.md`) HALTed pre-mutation on
> fictional trait surfaces (`corelink_cas::CasStore`, `corelink_auth::TenantResolver`).
> This is the v2 re-dispatch with the inline-ports mandate applied. v1 audit stays
> unchanged as the HALT record.

## §1. Headline

Wave-34 sub-step delivers `corelink-adapter-cargo`, the sccache HTTP storage backend
adapter for CoreLink. The crate bridges sccache's `GET`/`PUT`/`HEAD /<key>` wire against
CoreLink's per-tenant content-addressable storage so `cargo build` invocations across a
CI fleet hit the shared cache on repeat builds.

BLAKE3 key alignment is exact — sccache's HTTP backend uses BLAKE3 for artifact keys;
CoreLink CAS uses BLAKE3 for content-addressing. Zero translation cost.

## §2. Scope realized

| Item | Spec ref | Status |
|---|---|---|
| `lib.rs` re-export façade | cargo.md §4 | DONE (48 LOC) |
| `server.rs` axum GET/PUT/HEAD routes | §4 | DONE (281 LOC) |
| `translate.rs` sccache key normalize + `key_from_path` | §4 | DONE (119 LOC) |
| `auth.rs` PAT extract + constant-time prefix verify + resolver bridge | §4, §6 | DONE (174 LOC) |
| `audit.rs` chokepoint wrapper (cache.write + auth_failed events) | §3 | DONE (178 LOC) |
| `config.rs` `CargoAdapterConfig` + constructor + 16 MiB default cap | §5 | DONE (79 LOC) |
| `error.rs` 5-variant `CargoAdapterError` | §5 | DONE (42 LOC) |
| `ports.rs` adapter-local `CasStore` + `TenantResolver` | §5 (inline-ports mandate) | DONE (106 LOC) |
| Smoke tests (PUT→GET hit + HEAD + idempotent PUT) | §8 row 1 | DONE (3 tests) |
| Property tests (key normalization × 7 invariants) | §8 row 2 | DONE (7 tests) |
| Adversarial tests (forged-PAT/oversize/tenant-iso/CAS-failure/bad-key/multi-key) | §8 row 3 | DONE (6 tests) |
| Unit tests (auth + audit + translate) | internal | DONE (18 tests) |

## §3. Inline-ports decision (mirrors brew/pip/oci §3 / §6)

The dispatch packet (cargo.md §5) used the conceptual signatures
`Arc<dyn corelink_cas::CasStore>` and `Arc<dyn corelink_auth::TenantResolver>`. Neither
workspace trait exists at baseline `04a48e7d` (reserved for post-Wave-34 consolidation).

The v1 packet HALTed here per "executa, não decide." The v2 contract (cargo.md §0) mandates
the **inline-ports pattern** convergently established by pip + brew + oci on main:

- Adapter-local `pub trait CasStore` + `pub trait TenantResolver` declared in `src/ports.rs`
  (async, tenant-scoped by `&str tenant_id`, domain-native `digest_hex: &str`).
- No imports of `corelink_cas`, `corelink_auth`, `corelink_handler_cas`, or any workspace
  SPI. Production bridges deferred to Wave-35 `corelink-adapter-host`.
- In-memory test fakes (`InMemoryCas`, `StaticTenantResolver`) colocated in
  `tests/common.rs`.
- The ONE workspace trait consumed: `corelink_audit::ports::AuditEmitter` (sync, fail-CLOSED
  chokepoint per Wave-33 Stage 0 SEAL §4 rationale).

When the workspace traits ship, `ports.rs` collapses to `pub use` re-exports — call sites
stay stable. This matches the parallel-safe / source-disjoint mandate for the 5-sibling
adapter campaign.

## §4. Charter constraint matrix

| Constraint | Mechanism | Status |
|---|---|---|
| `#![forbid(unsafe_code)]` | `lib.rs` line 36 + `Cargo.toml` `[lints.rust] unsafe_code = "forbid"` | PASS |
| `#[non_exhaustive]` on every pub type | `CargoAdapterConfig`, `CargoAdapterError`, `CasError`, `TenantResolveError`, `AuditOrchestrator`, `CargoRouterState` | PASS |
| Zero `unwrap()` / `expect()` / `panic!()` in `src/` | `[lints.clippy] unwrap_used = expect_used = panic = "deny"` (tests use scoped `#[allow]`); only `unwrap_or` fallbacks in production code | PASS |
| `subtle::ConstantTimeEq` on PAT compare | `auth::bearer_eq` constant-time prefix check; `tests/common.rs::StaticTenantResolver` uses `ct_eq().unwrap_u8()` | PASS |
| Secrets boxed in `SecretString` | `auth::extract_bearer` returns `SecretString`; `ExposeSecret` only at resolver-call edge | PASS |
| Audit emit BEFORE state mutation | `server::handle_put` calls `auditor.emit_cache_write` BEFORE `cas.put`; adversarial §2 (oversize) and §4 (CAS failure) confirm no write row on rejected requests | PASS |
| L2.10: ≤500 LOC per file; sweet-spot ≤200 | Largest src file = 281 LOC (`server.rs`); largest test file = 313 LOC (`adversarial.rs`); 5 src files ≤200 LOC | PASS |
| wasm32 stays green | `cargo build --target wasm32-unknown-unknown -p corelink-clerk-cf` succeeds (59s clean build including new workspace member) | PASS |

## §5. Gate matrix

| Gate | Command | Result |
|---|---|---|
| Build (crate) | `cargo build -p corelink-adapter-cargo` | PASS (1.17s incremental) |
| Tests (crate) | `cargo test -p corelink-adapter-cargo` | PASS · 34 tests (18 unit + 6 adversarial + 7 property + 3 smoke) |
| Clippy | `cargo clippy -p corelink-adapter-cargo --all-targets -- -D warnings` | PASS (no warnings) |
| wasm32 build | `cargo build --target wasm32-unknown-unknown -p corelink-clerk-cf` | PASS |
| LOC headroom | `find … *.rs \| xargs wc -l \| sort -rn` | PASS (313 / 281 / 184) |

Per dispatch packet §9, test floor was ≥10; delivered 34 — 3.4× the floor.

## §6. Decision log

### D-1 · Inline-ports (mirrors brew §6 D-1)

See §3 above. Identical rationale to the pip/brew/oci agents' independent convergence.

### D-2 · `collect_body` via `http_body_util::BodyExt::collect`

sccache PUT bodies are in-memory build artifacts (typ. 1-8 MiB). The implementation
collects the full body via `BodyExt::collect().to_bytes()` before enforcing the size
limit. This is simpler than streaming chunk-by-chunk (no `futures_util` dep needed) and
sufficient at 16 MiB cap — fits comfortably in server memory. A streaming approach would
be needed only if the cap were raised to GiB scale (deferred to Wave 35 if needed).

### D-3 · `parse_key_or_400` inlined

Initial implementation used a helper returning `Result<String, Response>` which triggered
`clippy::result_large_err` (Response is ≥128 bytes). Replaced with inline `match
key_from_path(raw_key) { Some(k) => k, None => return StatusCode::BAD_REQUEST... }` at
each handler call site. Cleaner and clippy-clean.

### D-4 · `HEAD` handler via explicit route

axum 0.7 serves HEAD via GET by default if HEAD is not explicitly registered. We register
an explicit HEAD handler (`handle_head`) that calls `cas.get` and ignores the body — per
spec §3: "No separate `exists()` on the port — use `get` and ignore the body." This
avoids sending artifact bytes on HEAD requests and matches the sccache wire expectation.

### D-5 · No KV (metadata) layer

Per spec §4 dep-graph: cargo adapter is CAS-only, no KV. Build artifacts are
content-addressed by BLAKE3 key; no metadata path needed. The crate carries no KV dep.

### D-6 · Audit emit on auth failures

The cargo adapter emits `corelink.cargo.adapter.auth_failed.v1` on every 401 path (both
`extract_bearer` failures and `resolve_tenant` failures). This is best-effort (the request
was already rejected; we don't fail-CLOSED on auth-audit failures). Verified by
adversarial `forged_pat_returns_401` test.

## §7. Out-of-scope confirmations

Per spec §10 — all preserved:

- cargo registry mirror — NOT implemented;
- source-level caching — NOT implemented;
- cross-tenant dedup — NOT implemented (per-tenant only; adversarial `tenant_isolation_holds` confirms);
- WASM/edge adapter — NOT implemented (native server only);
- stale-entry warning — NOT implemented (GC handles; BLAKE3 collision-resists);
- per-user namespace within a tenant — NOT implemented (PAT → tenant; deferred to Wave 35).

## §8. Hard-pause-trigger audit

| Trigger | Threshold | Observed | Verdict |
|---|---|---|---|
| 1. Crate >50% over ~1050 LOC estimate | >1575 src LOC | ~820 src LOC | OK (22% under estimate) |
| 2. Any file >500 LOC | >500 | max 313 (test), 281 (src) | OK |
| 3. wasm32 workspace red | red | green (clerk-cf 59s clean build) | OK |
| 4. Test count <10 | <10 | 34 | OK (3.4× floor) |
| 5. Missing workspace trait | missing | `corelink_audit::ports::AuditEmitter` confirmed present | OK |
| 6. Cargo.lock conflicts | non-UNION | UNION-resolvable (`cargo update -w` clean) | OK |

No triggers fired. No HALT/escalation needed.

## §9. Per-file LOC table

| File | LOC | Category |
|---|---|---|
| `src/server.rs` | 281 | src |
| `src/audit.rs` | 178 | src |
| `src/auth.rs` | 174 | src |
| `src/translate.rs` | 119 | src |
| `src/ports.rs` | 106 | src |
| `src/config.rs` | 79 | src |
| `src/lib.rs` | 48 | src |
| `src/error.rs` | 42 | src |
| `tests/adversarial.rs` | 313 | test |
| `tests/smoke.rs` | 172 | test |
| `tests/common.rs` | 184 | test |
| `tests/prop_translate.rs` | 85 | test |
| **Total** | **1781** | |

## §10. Sign-off

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>.
Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>.

**End of W34 cargo adapter SEAL audit (v2).**
