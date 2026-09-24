---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-ratelimit
manifest: crates/corelink-ratelimit/Cargo.toml
source_commit: 6ed297f5b2b64cf97447985111a2ecbbaa9536bb
profile: S
state: author_validated
evidence_set: ratelimit-static-source-20260920
---

# corelink-ratelimit — blast radius

Static relation map for a token-bucket crate. Each relation is a source or
manifest fact, not a claim that a dependency, consumer, D1, DO, or rate limit
runs.

[Local](#b01) · [Root](#b02) · [Dependencies](#b03) · [Migration text](#b04) · [Tests](#b05) · [Unknowns](#b06).

<a id="b01"></a>
## B01 — Local source relation

[REL-001](#rel-001).

<a id="rel-001"></a>
### REL-001 — Root to modules

`src/lib.rs` declares the eight named public modules and re-exports selected
symbols from them. A changed declaration or root export can change local
source-path availability. Evidence: `src/lib.rs`. [Local](#b01) [Relation index](#b03)

<a id="b02"></a>
## B02 — Public limiter relation

[REL-002](#rel-002). [Relation index](#b03)

<a id="rel-002"></a>
### REL-002 — Root to limiter contract

`src/lib.rs` re-exports `RateLimiter` and `InMemoryTokenBucketRateLimiter`
from `limiter`. Altering an export or defining name can affect source callers
using the crate-root path. Evidence: `src/{lib,limiter}.rs`. [Root](#b02) [Relation index](#b03)

<a id="b03"></a>
## B03 — Declared dependency and reverse-consumer relations

[REL-003](#rel-003) · [REL-007](#rel-007). [Relation index](#b03)

<a id="rel-003"></a>
### REL-003 — Manifest to direct crates

The manifest declares `thiserror`, `uuid`, `corelink-eviction`, and dev-only
`proptest`. Changing these declarations changes static manifest edges only;
resolution and selected features remain unknown. Evidence: `Cargo.toml`.
[Dependencies](#b03)

<a id="rel-007"></a>
### REL-007 — e2e-resilience limiter scenario consumer

**Shared fingerprint:** `repo:1232040291:boundary:e2e-resilience-scenarios->corelink-ratelimit-api-v1`. **Consumer arrow:** [e2e-resilience](../e2e-resilience/BLAST_RADIUS.md#rel-002) `tests/e2e-resilience/tests/scenarios.rs[rate-limit tests]` → `corelink_ratelimit::{BucketKey,InMemoryRateLimitAuditSink,InMemoryRateLimitMetrics,InMemoryTokenBucketRateLimiter,RateLimitConfig,RateLimitDecision,RateLimitEventType,RateLimiter,TokenBucketState}`; source `:37-40,60-72,145-216`. **Surface/data:** `canonical`, `new`, `seed_bucket`, `try_acquire`, `snapshot_of`; tenant/key/tokens/time → decision/audit. **Activation:** source callsite only; target selection, compilation, execution and runtime unknown. **Effect/failure:** API or decision changes may break the harness or assertions; no runtime effect is claimed. **Contract owner:** `corelink-ratelimit`. **Validation/coordination:** compare the exact imports/calls and scenario assertions; coordinate this package and e2e-resilience owners. Evidence: `tests/e2e-resilience/Cargo.toml`, `tests/e2e-resilience/tests/scenarios.rs` at source `6ed297f5b2b64cf97447985111a2ecbbaa9536bb`. [Back to B03](#b03)

<a id="b04"></a>
## B04 — Migration text relation

[REL-004](#rel-004).

<a id="rel-004"></a>
### REL-004 — Constant to repository SQL path

`MIGRATION_0010_RATELIMIT_BUCKETS` references
`migrations/d1/0010_ratelimit_buckets.sql` through `include_str!`. Editing
either path can alter embedded source text; it cannot evidence D1 availability
or migration execution. Evidence: `src/lib.rs`. [Migration text](#b04) [Relation index](#b03)

<a id="b05"></a>
## B05 — Declared test relation

[REL-005](#rel-005). [Relation index](#b03)

<a id="rel-005"></a>
### REL-005 — Manifest to test targets

The manifest names `prop_ratelimit` and `migration_canonical_0010` test
targets. Their presence identifies source files for review, not a run result,
coverage result, or runtime conformance. Evidence: `Cargo.toml` and `tests/`.
[Tests](#b05) [Relation index](#b03)

<a id="b06"></a>
## B06 — External and unknown relation boundary

[REL-006](#rel-006). [Relation index](#b03)

<a id="rel-006"></a>
### REL-006 — Source references do not establish operation

Comments and identifiers may mention DO, D1, middleware, audit, metrics, or
tiers. They are not evidence of external bindings, reverse consumers, policy
approval, network access, deployment, or production rate limiting. Evidence:
static source only. [Unknowns](#b06)

Success is seven atomic, source-evidenced relations. Completeness is B01–B06
plus REL-007;
quality is no inferred operational edge. Definition of Done is static review
and documentary checks only.

[Reference](REFERENCE.md#r01) · [Maintenance](MAINTENANCE.md#m01) · [Start](#b01) [Relation index](#b03)
