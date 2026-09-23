---
name: own-corelink-clerk
description: Review source-grounded contract changes to the corelink-clerk adapter, configuration, JWKS/cache traits, principal types, and feature-gated helpers.
metadata:
  evidence-set: corelink-clerk-structural-normalization-20260921
  schema: corelink-ownership/1.1
  package: corelink-clerk
  manifest: crates/corelink-clerk/Cargo.toml
  source-commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
  evidence: static-source-only
  profile: H
---

# Own corelink-clerk

Use this skill for checked-in `corelink-clerk` changes. It records source and
static-manifest evidence only. It does not establish a selected feature set,
native or wasm compatibility, HTTP operation, a live Clerk/JWKS exchange,
identity validation, deployment, secret custody, or independent review.

[Baseline](#s01) · [Surface](#s02) · [RS256](#s03) · [Cache](#s04) · [Features](#s05) · [Consumers](#s06) · [Handoff](#s07)

<a id="s01"></a>
## S01 — Fix the source baseline

**Condition:** beginning a review or receiving a diff. **Action:** record the
revision, `Cargo.toml`, `src/lib.rs`, and every affected module. **Evidence:**
`git rev-parse HEAD`, changed-path inventory, and the named checked-in files.
**Stop:** the requested revision differs or the path inventory is incomplete;
obtain the intended snapshot rather than inferring from a newer checkout.

<a id="s02"></a>
## S02 — Keep the module and trait boundary explicit

**Condition:** changing an exported type, trait, config, error, fake, or
adapter path. **Action:** trace `lib.rs` re-exports to
`adapter`, `config`, `env_config`, `error`, `http_fetcher`, `jwks`,
`jwks_cache`, `principal`, and `redact`; distinguish local trait contracts
from an implementation supplied elsewhere. **Evidence:** `src/lib.rs` and the
affected module. **Stop:** a conclusion needs a fetcher, cache backend, HTTP
client, or caller behavior that is not present in these sources.

<a id="s03"></a>
## S03 — Preserve the source-level RS256 rejection path

**Condition:** changing JWT header handling, JWKS parsing, decoder setup, or
`AuthError`. **Action:** trace the input through `decode_header_json`,
`decode_header`, `Jwks::parse`, `DecodingKey`, and `Validation`; retain the
literal `RS256` checks and the typed error route. **Evidence:**
`src/adapter.rs`, `src/jwks.rs`, and `src/error.rs`. **Stop:** do not claim
cryptographic effectiveness or an identity result from source inspection; ask
for separately authorized execution evidence when that predicate is required.

<a id="s04"></a>
## S04 — Guard the cache and rotation contract

**Condition:** changing cache, `kid`, TTL, clock, refresh, or counter code.
**Action:** compare `CachedJwks::stored_at`, `is_fresh`, cache trait methods,
the 24-hour default, refresh triggers, and negative-cache constants as one
relation. **Evidence:** `src/{adapter,config,jwks_cache,fakes}.rs`.
**Stop:** do not infer KV durability, cross-isolate coordination, upstream
rotation timing, or an actual cache hit from these declarations.

<a id="s05"></a>
## S05 — Treat feature declarations as routes, not observations

**Condition:** changing `jwt-adapter`, `http-fetcher`, `test-utils`, an
optional dependency, or a `#[cfg]`. **Action:** reconcile each manifest entry
with its `lib.rs` gate and gated source path, including required example
features. **Evidence:** `Cargo.toml`, `src/lib.rs`, and the affected module.
**Stop:** no selected feature set, resolver result, compilation result, target
compatibility, or runtime path follows from a declaration or comment.

<a id="s06"></a>
## S06 — Scope consumer impact statically

**Condition:** changing public API, trait, error, feature, config, or
principal type. **Action:** inspect direct manifest/import edges before naming
consumers; hand off `corelink-worker`, `corelink-clerk-cf`, and the
`corelink-auth` facade when their cited edges are affected. **Evidence:**
consumer manifests and imports plus [B05](../../../docs/ownership/crates/corelink-clerk/BLAST_RADIUS.md#b05).
**Stop:** a text mention, re-export, or workspace membership is not a complete
reverse graph or runtime-reachability proof.

<a id="s07"></a>
## S07 — Hand off static evidence without overclaiming

**Condition:** the four ownership artifacts are ready. **Action:** supply the
baseline, changed paths, source anchors, four documentary-check verdicts,
`git diff --check`, and explicit unknowns. **Evidence:** the recorded command
outputs and [M06](../../../docs/ownership/crates/corelink-clerk/MAINTENANCE.md#m06).
**Stop:** do not self-certify cold review, live service behavior, deployment,
or any unexecuted Cargo/target gate.

[Reference](../../../docs/ownership/crates/corelink-clerk/REFERENCE.md#r01) · [Blast radius](../../../docs/ownership/crates/corelink-clerk/BLAST_RADIUS.md#b01) · [Maintenance](../../../docs/ownership/crates/corelink-clerk/MAINTENANCE.md#m01)
