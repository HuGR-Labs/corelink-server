---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-clerk
manifest: crates/corelink-clerk/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: H
state: draft
evidence_set: corelink-clerk-structural-normalization-20260921
---

# corelink-clerk — maintenance

These are static-source maintenance procedures for the package boundary. They
do not authorize Cargo execution, feature selection, target checks, network or
HTTP operation, Clerk/JWKS access, credential use, deployment, publication, or
claims about identity validation or cold review.

[Baseline](#m01) · [Surface](#m02) · [RS256](#m03) · [Cache](#m04) · [Features](#m05) · [Handoff](#m06)

<a id="m01"></a>
## M01 — Establish a fixed source baseline

**Mode:** STATIC_SOURCE. **Prerequisite:** requested SHA and changed paths are
known. **Predicate:** manifest, `src/lib.rs`, and changed source agree with
the review record. **Action:** record the revision and inspect only affected
modules plus their immediate exports. **Evidence:** revision output and path
inventory. **Stop/recovery:** stop on a different or dirty baseline; obtain
the requested source snapshot and do not substitute another revision.

<a id="m02"></a>
## M02 — Classify the changed contract boundary

**Mode:** STATIC_SOURCE. **Prerequisite:** a public type, function, trait,
error, configuration field, fake, or re-export changes. **Predicate:** the
changed symbol is traced from `lib.rs` to its owning local module and every
external implementation seam is identified. **Action:** inspect the matching
module among `adapter`, `config`, `env_config`, `error`, `fakes`,
`http_fetcher`, `jwks`, `jwks_cache`, `principal`, and `redact`.
**Evidence:** `src/lib.rs` and affected source files. **Stop/recovery:** stop
when backend, caller, transport, or provider behavior is needed; hand off the
precise missing implementation/consumer predicate.

<a id="m03"></a>
## M03 — Review the RS256 source predicate end to end

**Mode:** STATIC_SOURCE. **Prerequisite:** JWT parsing, JWK parsing, decoder
setup, claim conversion, or `AuthError` changes. **Predicate:** literal RS256
checks, RSA/sig JWK filtering, decoder configuration, and affected typed error
routes are mutually consistent in source. **Action:** trace header decode,
`kid` shape gate, `Jwks::parse`, `DecodingKey`, `Validation`, time/issuer/
audience source checks, and principal conversion. **Evidence:**
`src/{adapter,jwks,error,principal}.rs`. **Stop/recovery:** stop if the
decision requires a cryptographic measurement, library behavior, live JWKS,
or identity result; request separately authorized evidence without describing
it as proven here.

<a id="m04"></a>
## M04 — Review cache, TTL, and refresh changes as one relation

**Mode:** STATIC_SOURCE. **Prerequisite:** cache trait, instance hash, TTL,
clock, fetch/refresh, `kid`, fake, or counter changes. **Predicate:** config
bounds, `is_fresh`, trait calls, trigger labels, and negative-cache bounds
remain aligned. **Action:** compare `JWKS_TTL_SECS`, `instance_hash`,
`CachedJwks::stored_at`, `is_fresh`, `load_cached`, `fetch_and_cache`, and the
60-second/1,024-entry negative cache. **Evidence:**
`src/{adapter,config,jwks_cache,fakes}.rs`. **Stop/recovery:** stop if
durability, distributed coordination, expiry, upstream rotation, metrics, or
actual cache behavior is required; assign the concrete backend/provider
question to its owner.

<a id="m05"></a>
## M05 — Reconcile declared features and target artifacts

**Mode:** STATIC_MANIFEST. **Prerequisite:** a feature, optional dependency, cfg, example, test, or bench changes. **Predicate:** every declared feature maps to its corresponding manifest dependency relation, `lib.rs` gate, and target requirement. **Action:** compare `jwt-adapter`, `http-fetcher`, and `test-utils` with their cfg paths, self dev-dependency features, example requirements, explicit `[[test]]`/`[[bench]]` stanzas, and any convention-discovered integration-test source such as `mutation_kills.rs`. **Evidence:** `Cargo.toml:13-115`, `src/lib.rs:112-153`, and the referenced target source. **Stop/recovery:** stop where a conclusion needs selection/resolution,

a Cargo result, native/wasm compatibility, or target execution; record it as unknown rather than inferring it from source.

<a id="m06"></a>
## M06 — Run documentary gates and hand off

**Mode:** STATIC_HANDOFF. **Prerequisite:** exactly the four assigned ownership artifacts changed and the documentary checker is available. **Predicate:** S01–S07, R01–R08, B01–B06, and M01–M06 exist; material claims have static evidence; each artifact passes its H-profile structural checker; `git diff --check` and the owned-path inventory are clean. **Action:** run the checker once each for `skill`, `reference`, `blast_radius`, and `maintenance`, then run `git diff --check` and inspect changed paths. **Evidence:** exact commands, exit statuses,

baseline SHA, changed paths, and unknowns. **Stop/recovery:** stop on a checker/diff/scope failure or a required change outside these four paths; correct only owned artifacts and rerun the failed documentary gate, otherwise hand off the exact failure. These gates do not prove a selected feature set, compilation, target compatibility, service operation, identity validation, deployment, or cold review.

[Reference](REFERENCE.md#r01) · [Blast radius](BLAST_RADIUS.md#b01) · [Ownership guide](../../../../.claude/skills/own-corelink-clerk/SKILL.md#s01)
