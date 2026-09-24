---
name: own-corelink-auth
description: Review source-grounded ownership and contract changes to the corelink-auth hybrid authentication facade.
metadata:
  evidence-set: auth-source-static-20260920
  source-commit: b64025a5ef6295c1f23614339b6bb5a1e909f793
  manifest: crates/corelink-auth/Cargo.toml
  package: corelink-auth
  scope: corelink-auth
  evidence: static-source-only
  profile: H
---

# Own corelink-auth

Use this skill for `crates/corelink-auth`: a hybrid facade with four external re-export modules, inline schema and WebAuthn implementation, and feature-gated worker re-exports. It records source/static evidence only; it does not establish runtime reachability, deployment safety, database state, target builds, or independent review.

[Baseline](#s01) · [Ownership](#s02) · [Features](#s03) · [Schema](#s04) · [WebAuthn](#s05) · [Consumers](#s06) · [Handoff](#s07)

<a id="s01"></a>
## S01 — Establish the source baseline

**Condition:** beginning a change or receiving a diff. **Action:** record the revision, manifest, `src/lib.rs`, and every changed module. **Evidence:** `crates/corelink-auth/Cargo.toml`, `src/lib.rs`, and the changed-path inventory. **Stop:** the revision or path set is unavailable or differs; do not replace it with a historical absorption narrative.

<a id="s02"></a>
## S02 — Separate facade and implementation ownership

**Condition:** a public auth path changes. **Action:** classify `clerk`, `clerk_cf`, `pat`, and `tenant_path` as `pub use` facades; classify `schema` and `webauthn` as inline modules. Keep the implementation owner for each re-export outside this package. **Evidence:** `src/{clerk,clerk_cf,pat,tenant_path}.rs`, `src/{schema,webauthn}.rs`, and manifest dependencies. **Stop:** a change assumes a re-export transfers implementation ownership or API compatibility without tracing its defining crate.

<a id="s03"></a>
## S03 — Preserve feature-gated public paths

**Condition:** changing features, middleware, worker session, or a forwarded dependency feature. **Action:** trace `clerk-jwt-adapter → corelink-clerk/jwt-adapter` and `tower-middleware → corelink-worker/tower-middleware`; trace the `#[cfg(feature = "tower-middleware")]` declarations for both public modules. **Evidence:** `Cargo.toml:55-67`, `src/lib.rs:143-162`. **Stop:** a claim depends on feature resolution, native/wasm target behavior, or worker semantics not established by these declarations.

<a id="s04"></a>
## S04 — Guard the inline schema boundary

**Condition:** changing schema SQL inclusion, email hashing, pseudonymization, RLS guidance, or the simulator. **Action:** trace `MIGRATION_002_AUTH_TABLES` to `migrations/002_auth_tables.sql`; review `email_hash`, `pseudonymize`, `rls`, and `sim` together where an invariant crosses them. **Evidence:** `src/schema.rs`, its four submodules, and the migration text. **Stop:** the decision requires a live Postgres result, migration ordering, data recovery, secret provenance, or application transaction wiring.

<a id="s05"></a>
## S05 — Guard the inline WebAuthn boundary

**Condition:** changing AAGUID policy, challenge, credential, engine, origin, recovery, sign count, step-up, or store code. **Action:** trace the affected input through the named module and its exported type/trait; preserve explicit error/result paths and TTL/value predicates in source. **Evidence:** `src/webauthn.rs` plus the cited module files. **Stop:** a claim needs a browser, authenticator, production engine, persistent store, delivery channel, or measured security property.

<a id="s06"></a>
## S06 — Scope consumer impact as a static graph

**Condition:** changing a public path, feature, or manifest edge. **Action:** inspect direct manifest/source references first, then list CLI, OpenAPI, adapters, REAPI, worker, statuspage, and fuzz as review targets only when a concrete source edge is found. **Evidence:** consumer manifests/imports and the workspace manifest. **Stop:** a reverse-dependency, generated, dynamic, feature-resolved, or runtime consumer is needed but has no separately recorded graph evidence.

<a id="s07"></a>
## S07 — Hand off with falsifiable limits

**Condition:** static review is ready to hand off. **Action:** report the revision, facade/inline classification, affected feature and relation identifiers, and unknowns. **Evidence:** exact source paths, symbols, and the three ownership documents. **Stop:** do not label a source review as a target build, runtime, deployment, migration execution, or cold review; request the responsible owner and evidence for those claims.
