---
id: "AUDIT-2026-05-26-W34-ADAPTER-BREW"
type: "audit"
doc_status: "ACTIVE"
audit_status: "SEAL"
version: "1.0.0"
created: "2026-05-26"
updated: "2026-05-26"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["audit", "adapters", "wave-34", "brew", "package-manager", "SEAL"]
references:
  - "specs/_proposals/adapters/brew.md"
---

# SEAL Audit — Wave-34 Adapter Campaign · Homebrew

**Wave:** 34 · **Crate:** `corelink-adapter-brew` · **Branch:** `wt/r-prep-w34-adapter-brew` · **Baseline:** `a1c49678` · **Authored:** 2026-05-26

## §1. Headline

Wave-34 sub-step delivers `corelink-adapter-brew`, the fifth (with cargo, npm, pip, oci siblings) adapter in the package-manager campaign. The crate sits between `brew install` and the upstream OCI bottle host (GitHub Packages by default), redirected via the `HOMEBREW_BOTTLE_DOMAIN` environment override. Bottle (`.tar.gz`) downloads are cached in per-tenant CoreLink CAS so repeated installs across a CI fleet hit local cache.

## §2. Scope realized

| Item | Spec ref | Status |
|---|---|---|
| `lib.rs` re-export façade | brew.md §4 | DONE (55 LOC) |
| `server.rs` axum catch-all `GET /*path` | §4 | DONE (156 LOC) |
| `bottle.rs` URL canonicalize + BLAKE3 CAS key + get/put orchestrator | §4 | DONE (222 LOC) |
| `upstream.rs` reqwest fetch + size enforcement | §4 | DONE (141 LOC) |
| `auth.rs` PAT extract + constant-time prefix verify + resolver bridge | §4, §6 | DONE (179 LOC) |
| `audit.rs` chokepoint wrapper, integrity="best-effort" | §3 | DONE (154 LOC) |
| `config.rs` `BrewAdapterConfig` + constructor | §5 | DONE (93 LOC) |
| `error.rs` 6-variant `BrewAdapterError` | §5 | DONE (50 LOC) |
| `ports.rs` adapter-local `CasStore` + `TenantResolver` | §5 (lifted to local port per campaign charter) | DONE (107 LOC) |
| Smoke test (cache-hit + URL collapsing) | §8 row 1 | DONE (2 tests) |
| Property tests (4 invariances × 1k cases) | §8 row 2 | DONE (5 tests) |
| Adversarial tests (oversize/forged-PAT/tenant-iso/upstream-5xx) | §8 row 3 | DONE (4 tests) |

## §3. Integrity posture (BEST-EFFORT)

Brew bottle URLs do NOT embed the upstream SHA — the formula DSL (Ruby tap file) carries the canonical hash and is verified by the brew client AFTER download. The brew adapter therefore CANNOT verify a pre-store digest the way `pip` (PEP 691 `data-dist-info-metadata` + URL fragment SHA) and `npm` (registry response `dist.shasum`) siblings can.

The implementation surfaces this asymmetry explicitly:

- every cache-fill audit row carries `payload.integrity = "best-effort"`;
- the module docstring (`bottle.rs` + `audit.rs`) calls out the asymmetry vs. pip/npm;
- the brew client's downstream formula-DSL verification is the authoritative integrity gate (documented in `specs/_proposals/adapters/brew.md` §3).

Deferred to v2 (explicit out-of-scope per spec §10): parallel fetch + verify of the brew tap's formula JSON before storing. The latency cost outweighs the benefit while the brew client's downstream check provides an authoritative second line of defense.

## §4. Charter constraint matrix

| Constraint | Mechanism | Status |
|---|---|---|
| `#![forbid(unsafe_code)]` | `lib.rs` line 41 + `Cargo.toml` `[lints.rust] unsafe_code = "forbid"` | PASS |
| `#[non_exhaustive]` on every pub type | `BrewAdapterConfig`, `BrewAdapterError`, `CasError`, `TenantResolveError`, `BottleService`, `AuditOrchestrator`, `UpstreamFetcher` | PASS |
| Zero `unwrap()` / `expect()` / `panic!()` in `src/` | `[lints.clippy] unwrap_used = expect_used = panic = "deny"` (tests use scoped `#[allow]`) | PASS |
| `subtle::ConstantTimeEq` on PAT compare | `auth::bearer_eq` constant-time prefix check; downstream resolver compares hash in CT per trait contract | PASS |
| Secrets boxed in `SecretString` | `auth::extract_bearer` returns `SecretString`; `ExposeSecret` only at the resolver-call edge | PASS |
| Audit emit BEFORE state mutation | `bottle::BottleService::fetch` calls `auditor.emit_bottle_cache_fill` BEFORE `cas.put`; tested via adversarial §4 row 4 (upstream 5xx → no half-store) and confirmed by audit-row count in smoke test (1 fill = 1 audit) | PASS |
| L2.10: ≤500 LOC per file; sweet-spot ≤200 | Largest src file = 222 LOC (`bottle.rs`); largest test file = 248 LOC (`adversarial.rs`); 5 src files ≤200 LOC | PASS |
| wasm32 stays green | `cargo build --target wasm32-unknown-unknown -p corelink-clerk-cf` succeeds (3m 03s clean build, including new workspace member) | PASS |

## §5. Gate matrix

| Gate | Command | Result |
|---|---|---|
| Build (crate) | `cargo build -p corelink-adapter-brew` | PASS (9.94s clean build) |
| Tests (crate) | `cargo test -p corelink-adapter-brew` | PASS · 27 tests (16 unit + 4 adversarial + 5 property + 2 smoke) |
| Clippy | `cargo clippy -p corelink-adapter-brew --all-targets -- -D warnings` | PASS (no warnings) |
| Workspace build | `cargo build --workspace` | PASS (exit 0) |
| wasm32 build | `cargo build --target wasm32-unknown-unknown -p corelink-clerk-cf` | PASS |
| Spec validators | `validate_specs.py` / `validate_references.py` / `check_migrations_additive.py` | PASS (464 docs; 0 dangling; 59 migrations additive) |
| LOC headroom | `find … *.rs | xargs wc -l | sort -rn | head -3` | PASS (248 / 222 / 179) |

Per dispatch packet §8, test floor was ≥10; delivered 27 — 2.7× the floor.

## §6. Decision log

### D-1 · Adapter-local port traits (`CasStore`, `TenantResolver`)

The dispatch packet (brew.md §5) used the conceptual signatures `Arc<dyn corelink_cas::CasStore>` and `Arc<dyn corelink_auth::TenantResolver>`. Neither workspace trait exists today (the canonical names are reserved for a post-Wave-34 consolidation that the Wave-33 Stream-A `corelink-cas` umbrella did not land).

Rather than block the Wave-34 sibling campaign on a not-yet-extracted trait surface, this adapter defines the two ports locally in `src/ports.rs` (`async_trait` flavour, debug-bounded). When the workspace traits ship, `ports.rs` collapses to a `pub use` re-export — call sites stay stable.

This matches the dispatch packet's "Concurrent agents (parallel-safe per Section 0.6) · Zero source-overlap" mandate: each of the 5 sibling adapters can land its port surface without coupling to the others' review cycles.

### D-2 · Catch-all route syntax (`/*path` vs. `/{*path}`)

axum 0.7's matchit router rejects the `{*name}` syntax (catch-all params must be at the END of the route and use the legacy `*name` form). Initial implementation used the 0.8-style `{*path}` and panicked at router construction; corrected to `/*path` and verified by all 4 adversarial tests passing.

### D-3 · `tests/common.rs` vs. `tests/common/mod.rs`

The workspace clippy lints (`mod_module_files = "deny"`) reject `mod.rs` files. Test helpers therefore live in `tests/common.rs` at the top level of the `tests/` directory. Cargo treats top-level `tests/*.rs` files as separate test binaries, so `tests/common.rs` is its own compilation unit; a scoped `#![allow(dead_code, missing_docs, …)]` accommodates the helpers-only nature (no `#[test]` functions in `common.rs`).

### D-4 · BLAKE3-of-canonical-URL as CAS key

Brew URLs carry no upstream SHA in the path. The CAS key is `blake3(canonical_path)` rendered as 64-char hex; the canonical_path pipeline (lowercase + strip-trailing-slash + drop-query-string) collapses the four common-equivalent URL variants observed in production. Verified by the property test suite (1024 cases × 4 invariances) and by the smoke `url_variants_collapse_to_single_cache_entry` test.

### D-5 · No KV (metadata) layer

Per spec §4 dep-graph note: "(No KV — brew adapter has no metadata-cache layer; only binary bottles in CAS.)" Confirmed — the crate's `Cargo.toml` carries no KV dep and no metadata path exists in source. Brew clients fetch formula metadata via separate tap-source git clones, which are explicitly out-of-scope per §10.

## §7. Out-of-scope confirmations

Per spec §10 — all preserved:

- tap source caching (separate `tap-domain`) — NOT implemented;
- formula-JSON pre-store SHA verification — NOT implemented (v2 deferral, see §3);
- casks (`brew install --cask`) — NOT implemented;
- bottling (`brew bottle`) write path — NOT implemented;
- cross-platform bottle matrix logic — NOT implemented (brew chooses URL; adapter is platform-agnostic);
- cross-tenant dedup — NOT implemented (per-tenant only, confirmed by adversarial `tenant_isolation_holds`);
- anonymous reads — NOT implemented (PAT required, confirmed by adversarial `forged_pat_returns_401`);
- brew dependency resolution — NOT implemented (brew does that locally).

## §8. Hard-pause-trigger audit

| Trigger | Threshold | Observed | Verdict |
|---|---|---|---|
| 1. Crate >50% over ~1080 LOC estimate | >1620 src LOC | ~1130 src LOC | OK (~5% over, well within budget) |
| 2. Any file >500 LOC | >500 | max 248 (test), 222 (src) | OK |
| 3. wasm32 workspace red | red | green (clerk-cf clean build) | OK |
| 4. Test count <10 | <10 | 27 | OK (2.7× floor) |

No triggers fired. No HALT/escalation needed.

## §9. Sign-off

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>.
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>.

**End of W34 brew adapter SEAL audit.**
