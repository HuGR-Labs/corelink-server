---
schema: corelink-ownership/1.1
document: reference
package: corelink-ratelimit
manifest: crates/corelink-ratelimit/Cargo.toml
source_commit: 6ed297f5b2b64cf97447985111a2ecbbaa9536bb
profile: S
state: author_validated
evidence_set: ratelimit-static-source-20260920
---

# corelink-ratelimit — ownership reference

Static-source reference for the token-bucket engine, trait boundary, and
in-memory collaborators. SOURCE != executed/runtime: no DO, D1, rate-limit,
HTTP, provider, deployment, or persistence operation is asserted here.

[Identity](#r01) · [Boundary](#r02) · [Map](#r03) · [Contracts](#r04) · [Invariants](#r05) · [Configuration](#r06) · [Failures](#r07) · [Evidence](#r08).

<a id="r01"></a>
## R01 — Identity

| Field | Static evidence |
|---|---|
| Package / manifest | `corelink-ratelimit` / `crates/corelink-ratelimit/Cargo.toml` |
| Local source inventory | `lib`, `bucket`, `config`, `key`, `limiter`, `tier`, `audit`, `metrics`, and `error` |
| Declared dependencies | `thiserror`, `uuid`, `corelink-eviction`; dev-only `proptest` |
| Source mode | SOURCE only; no Cargo resolution, build, or test result collected |

<a id="r02"></a>
## R02 — Boundary and authority

This crate owns Rust declarations, pure token-bucket source, the `RateLimiter`
trait, and in-memory implementations. `MIGRATION_0010_RATELIMIT_BUCKETS` is an
`include_str!` relation to repository SQL, not evidence that D1 exists or ran.
Verified OKF canonical routing remains external: this set neither copies,
redefines, nor revalidates its policy.

<a id="r03"></a>
## R03 — Implementation map

| Module | Source-visible responsibility | Boundary |
|---|---|---|
| `lib.rs` | Forbids unsafe code, modules/re-exports, migration include | Local namespace/source relation |
| `bucket.rs` | State, decision, refill, acquire | Pure local algorithm source |
| `limiter.rs` | Trait and mutex/map-backed limiter | Fake/orchestrator source |
| `key.rs`, `tier.rs`, `config.rs` | Keys, tier resolver, configuration | Local value contracts |
| `audit.rs`, `metrics.rs`, `error.rs` | Observer ports, fakes, taxonomy | Source interface, not delivery |

<a id="r04"></a>
## R04 — Public contracts

[API-001](#api-001) · [API-002](#api-002) · [API-003](#api-003) · [API-004](#api-004) · [API-005](#api-005).

<a id="api-001"></a>
### API-001 — Bucket state and decision

`TokenBucketState`, `BucketDecision`, and `bucket_try_acquire` are re-exported
from `bucket`. The local function returns a decision-state pair; it exposes no
transport response. Evidence: `src/{lib,bucket}.rs`. [Contracts](#r04)

<a id="api-002"></a>
[↩](#r01)
### API-002 — Limiter port and fake

`RateLimiter` declares `try_acquire`, `update_plan`, and `snapshot_bucket`.
`InMemoryTokenBucketRateLimiter` is the local implementation exported by the
root. Neither declaration identifies a real backend. Evidence:
`src/{lib,limiter}.rs`. [Contracts](#r04)

<a id="api-003"></a>
[↩](#r01)
### API-003 — Key surface

`KeyDimension`, `BucketKey`, and `KEY_DIMENSION_LIST` are re-exported from
`key`; constructors cover three source-defined dimensions. `scope_key` is a
`String`; this crate performs no request extraction. Evidence: `src/{lib,key}.rs`.
[Contracts](#r04)

<a id="api-004"></a>
[↩](#r01)
### API-004 — Configuration and tier surface

`RateLimitConfig` and constants come from `config`; resolver and tier constants
come from `tier`. They are local source values, not independently certified
policy. Evidence: `src/{lib,config,tier}.rs`. [Contracts](#r04)

<a id="api-005"></a>
[↩](#r01)
### API-005 — Audit, metrics, and errors

The root exposes observer traits, in-memory/no-op/failing helpers, canonical
name functions, and `RateLimitError`. These interfaces do not demonstrate
persistence, alerting, or delivery. Evidence: `src/{lib,audit,metrics,error}.rs`.
[Contracts](#r04)
[↩](#r01)

<a id="r05"></a>
## R05 — Falsifiable source invariants

| Identifier | Atomic predicate | Static falsifier / evidence |
|---|---|---|
| <a id="inv-001"></a>INV-001 | Root contains `#![forbid(unsafe_code)]`. | Remove/weaken it in `src/lib.rs`. |
| <a id="inv-002"></a>INV-002 | `RateLimiter` declares the three named required methods in this baseline. | Add, remove, or rename one in `src/limiter.rs`. |
| <a id="inv-003"></a>INV-003 | `KEY_DIMENSION_LIST` contains `PerTenant`, `PerIp`, and `PerTenantPerEndpoint`. | Change members/order in `src/key.rs`. |
| <a id="inv-004"></a>INV-004 | Migration constant uses `include_str!` for `migrations/d1/0010_ratelimit_buckets.sql`. | Change include target/definition in `src/lib.rs`. |
| <a id="inv-005"></a>INV-005 | In-memory limiter declares `Arc<Mutex<HashMap<BucketKey, Bucket>>>`. | Replace the field shape in `src/limiter.rs`. |

<a id="r06"></a>
## R06 — Configuration and canonical-routing boundary

The manifest declares no crate-local feature table. Configuration and tier
symbols are source-defined. The wave plan assigns profile S and directs
canonical concepts through verified OKF routing; this document records routing
and local symbols only, not policy meaning or validity.

<a id="r07"></a>
## R07 — Failure and compatibility boundary

`RateLimitError` has audit, metrics, cost-capacity, tenant-mismatch, and
backend variants in source. Compatibility impact includes changed exported
names, trait methods, enum shapes, constants, or include paths. Source alone
does not prove HTTP mapping, provider behavior, durable storage, or callers.

<a id="r08"></a>
## R08 — Evidence, unknowns, and quality gate

Evidence: manifest, listed local modules, unexecuted test source, and wave
plan. Unknowns: resolved graph/features, build/test results, consumers, policy
conformance, DO/D1 existence or operation, migration application, telemetry or
audit delivery, network, deployment, production behavior, and cold review.

Success is source-accurate boundary; completeness is R01–R08 plus companions;
quality is checker-valid navigation and explicit unknowns. Definition of Done
is documentary validation only, never execution.

[Impact map](BLAST_RADIUS.md#b01) · [Maintenance](MAINTENANCE.md#m01) · [Start](#r01)
