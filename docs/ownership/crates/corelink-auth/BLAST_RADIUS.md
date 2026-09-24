---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-auth
manifest: crates/corelink-auth/Cargo.toml
source_commit: b64025a5ef6295c1f23614339b6bb5a1e909f793
profile: H
state: draft
evidence_set: auth-source-static-20260920
---

# corelink-auth — blast radius

Atomic source/static relations only. A dependency is a declared/import edge; a flow is a source value or name route; an impact is a review consequence. None establishes invocation, deployment, or runtime reachability.

[Facade](#b01) · [Features](#b02) · [Schema inclusion](#b03) · [Schema data](#b04) · [WebAuthn](#b05) · [Consumers](#b06)

<a id="b01"></a>
## B01 — Facade-to-implementation relation

**Dependency / flow / impact:** `corelink_auth::{clerk, clerk_cf, pat, tenant_path}` → respective declared dependency; consumer import → local submodule → `pub use` target; a changed facade path can rename or remove a public route without moving its implementation ownership.

**Predicate:** a facade module, dependency name, or `pub use` changes.

**Evidence:** `Cargo.toml:13-30`; `src/{clerk,clerk_cf,pat,tenant_path}.rs`.

**Unknown/stop:** the imported crate’s semantic behavior, feature resolution, and runtime use require its own source/graph evidence.

<a id="b02"></a>
## B02 — Feature-to-worker re-export relation

**Dependency / flow / impact:** `tower-middleware` → `corelink-worker/tower-middleware`; feature selection → cfg-gated `middleware` and `worker_session` paths → worker public symbols; changing either feature/cfg route can alter consumer compilation paths.

**Predicate:** feature forwarding, `#[cfg]`, or worker module re-export changes.

**Evidence:** `Cargo.toml:55-67`; `src/lib.rs:143-162`.

**Unknown/stop:** a feature declaration does not show a resolved build, wasm behavior, or an active worker middleware/session flow.

<a id="b03"></a>
## B03 — Schema-source-to-migration relation

**Dependency / flow / impact:** `schema.rs` → `include_str!("../../../migrations/002_auth_tables.sql")`; checked-in SQL bytes → `MIGRATION_002_AUTH_TABLES`; a migration-text or include-path change changes the embedded string contract.

**Predicate:** the include path, schema version, or migration SQL changes.

**Evidence:** `src/schema.rs:45-70`; `migrations/002_auth_tables.sql:1-39`.

**Unknown/stop:** embedded text does not prove migration execution, order, database compatibility, or recovery.

<a id="b04"></a>
## B04 — Schema helper and simulator relation

**Dependency / flow / impact:** email input → canonicalization/HMAC → `[u8; 32]` hash; account/user UUID plus distinct domain → truncated HMAC pseudonym; row input → `AuthSchema` checks/maps → result or `SimError`. A change can alter lookup identity, pseudonym collision/domain separation, or simulator outcomes.

**Predicate:** hash constants/normalization, pseudonym domains/width, or simulator validation/cascade logic changes.

**Evidence:** `src/schema/email_hash.rs:34-152`; `src/schema/pseudonymize.rs:26-58`; `src/schema/sim.rs:192-529`.

**Unknown/stop:** the simulator does not parse SQL; source parity with Postgres RLS, transaction behavior, encryption, or persisted rows is unproven.

<a id="b05"></a>
## B05 — WebAuthn ceremony-to-store relation

**Dependency / flow / impact:** ceremony request → `WebAuthnEngine` → challenge/credential/origin/AAGUID/sign-count/recovery/step-up types and stores → typed outcome/error; changing an input rule, trait, or store contract can change a public ceremony boundary.

**Predicate:** a WebAuthn module, re-export, TTL, trait, or input-validation path changes.

**Evidence:** `src/webauthn.rs:118-165`; `src/webauthn/{aaguid,challenge,credential,engine,origin,recovery,sign_count,step_up,store}.rs`.

**Unknown/stop:** real authenticators, browsers, production engine, persistence, recovery delivery, and security effectiveness are outside this static relation.

<a id="b06"></a>
## B06 — Static consumer-review relation

**Dependency / flow / impact:** `corelink-reapi` manifest → `corelink-auth` plus `tower-middleware`; REAPI imports → `corelink_auth::middleware`; a changed optional path can affect that direct static consumer. CLI, OpenAPI, adapters, worker, statuspage, and fuzz remain review categories until a concrete source edge is recorded.

**Predicate:** public module, feature, dependency, or public symbol changes.

**Evidence:** root `Cargo.toml:588-591`; `crates/corelink-reapi/Cargo.toml:42-68`; `crates/corelink-reapi/src/{http_read,timing_padding_wiring}.rs`; `crates/corelink-adapters-cloud/src/lib.rs:17-40`.

**Unknown/stop:** this is not a complete reverse-dependency graph and cannot prove the listed consumers execute or are exhaustive.

[Reference](REFERENCE.md#r01) · [Maintenance](MAINTENANCE.md#m01) · [Start](#b01)
