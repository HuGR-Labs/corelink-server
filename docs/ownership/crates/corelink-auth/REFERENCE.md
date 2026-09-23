---
schema: corelink-ownership/1.1
document: reference
package: corelink-auth
manifest: crates/corelink-auth/Cargo.toml
source_commit: b64025a5ef6295c1f23614339b6bb5a1e909f793
profile: H
state: draft
evidence_set: auth-source-static-20260920
---

# corelink-auth — ownership reference

This H-profile reference is limited to checked-in manifest and source evidence at `b64025a5ef6295c1f23614339b6bb5a1e909f793`. It does not prove feature resolution, compilation, target-specific behavior, database execution, deployment, runtime use, or cold review.

[Identity](#r01) · [Surface](#r02) · [Features](#r03) · [Schema](#r04) · [WebAuthn](#r05) · [Declared checks](#r06) · [Consumers](#r07) · [Unknowns](#r08)

<a id="r01"></a>
## R01 — Package identity and hybrid ownership

`crates/corelink-auth/Cargo.toml` names `corelink-auth`; the source tree contains 37 Rust files (26 below `src`). `src/lib.rs` declares the six canonical submodule paths. Four are facades over declared dependencies; two are inline implementations. This means a public path can be owned here while its behavior remains owned by an external crate. Falsifier: a module declaration, dependency, or `pub use` below changes. Evidence: `Cargo.toml:1-67`, `src/lib.rs:133-162`, `src/{clerk,clerk_cf,pat,tenant_path,schema,webauthn}.rs`.

<a id="r02"></a>
## R02 — Public surface and ownership split

| Path | Static construction | Implementation ownership conclusion |
|---|---|---|
| `clerk` | `pub use corelink_clerk::*` | remains with `corelink-clerk` |
| `clerk_cf` | `pub use corelink_clerk_cf::*` | remains with `corelink-clerk-cf` |
| `pat` | `pub use corelink_pat::*` | remains with `corelink-pat` |
| `tenant_path` | `pub use corelink_tenant_path::*` | remains with `corelink-tenant-path` |
| `schema` | local module plus four local submodules | implementation is in this package |
| `webauthn` | local module plus 14 local submodules | implementation is in this package |

The root forbids unsafe code and denies missing docs. Re-export syntax establishes a public name route, not execution or semantic parity of every source crate. Evidence: `src/lib.rs:133-162`; `src/{clerk,clerk_cf,pat,tenant_path}.rs:1-15`; `src/schema.rs:43-62`; `src/webauthn.rs:116-163`.

<a id="r03"></a>
## R03 — Feature-forwarding contract

The default feature list is empty. `clerk-jwt-adapter` forwards only to `corelink-clerk/jwt-adapter`. `tower-middleware` forwards only to `corelink-worker/tower-middleware`; when that feature is selected, `middleware` re-exports `corelink_worker::middleware::*` and `worker_session` re-exports `corelink_worker::auth::*`.

Falsifiable invariant: without a change to the relevant `Cargo.toml` feature entry or `#[cfg]` declaration, this package exposes neither optional worker module through its own source. The declarations do not prove a resolved feature set, a native or wasm target result, or worker runtime behavior. Evidence: `Cargo.toml:55-67`; `src/lib.rs:143-162`.

<a id="r04"></a>
## R04 — Inline schema contract

`schema.rs` embeds `migrations/002_auth_tables.sql` through `include_str!`, exposes `email_hash`, `pseudonymize`, `rls`, and `sim`, and returns schema version `2`. `EmailHashKey` is a 32-byte non-`Clone`/non-`Copy` value with `ZeroizeOnDrop`; `compute_email_hash` applies `trim().to_ascii_lowercase()` before HMAC-SHA256. Account and user pseudonyms use distinct literal domains and encode the first 16 HMAC bytes. `AuthSchema` models checks, uniqueness, foreign keys, revocation idempotency, and selected account-deletion cascades in memory; it is not a SQL parser.

Falsifiable invariant: altering an included migration byte, normalization/domain byte, or simulator predicate changes this source-level contract and requires a schema review. SQL execution, RLS enforcement by Postgres, secret provisioning, and production cascade behavior are unknown. Evidence: `src/schema.rs:45-70`; `src/schema/email_hash.rs:34-152`; `src/schema/pseudonymize.rs:26-58`; `src/schema/rls.rs:1-42`; `src/schema/sim.rs:192-516`; `migrations/002_auth_tables.sql:1-39,119-254`.

<a id="r05"></a>
## R05 — Inline WebAuthn contract

`webauthn.rs` exports modules for AAGUID policy, challenge, clock, COSE, credential, engine, errors, flags, metrics, origin, recovery, sign count, step-up, store, and types. Its public surface includes `WebAuthnEngine`, `ChallengeStore`, `CredentialStore`, `RecoveryOtpStore`, in-memory variants, and `ProductionEngineNotConfigured`. Source constants set a 32-byte challenge, 600-second default recovery OTP TTL, and 300-second default step-up TTL; `RecoveryChannel` and sign-count assessment are separately represented types.

Falsifiable invariant: a change to a named module or its re-export can change the corresponding public source contract and must be traced through the owning inline source. The declarations do not prove authenticator interoperability, browser behavior, production-engine configuration, durable-store semantics, OTP delivery, or a measured security outcome. Evidence: `src/webauthn.rs:118-165`; `src/webauthn/{aaguid,challenge,credential,engine,origin,recovery,sign_count,step_up,store}.rs`.

<a id="r06"></a>
## R06 — Declared examples and tests

The manifest explicitly names four WebAuthn examples, three WebAuthn test targets, and four schema test targets. Their names identify intended static coverage areas (property, adversarial, canonical-vector, migration, DSR/PAT export, and mutation-kill); no execution result is recorded here. Evidence: `Cargo.toml:79-126`; matching paths under `examples/` and `tests/`.

<a id="r07"></a>
## R07 — Static consumer census

The root manifest makes `corelink-auth` a workspace member and workspace dependency. This revision directly declares `corelink-reapi → corelink-auth`, enables its `tower-middleware` feature there, and imports `corelink_auth::middleware` in two REAPI sources. `corelink-adapters-cloud` source names the `clerk_cf` canonical path as an architectural relation, but its manifest does not declare `corelink-auth` here.

CLI, OpenAPI, adapters, REAPI, worker, statuspage, and fuzz are the required downstream review categories for a change; this package-only census does not substantiate every category as a direct consumer. Evidence: root `Cargo.toml:34-40,588-591`; `crates/corelink-reapi/Cargo.toml:42-68`; `crates/corelink-reapi/src/{http_read,timing_padding_wiring}.rs`; `crates/corelink-adapters-cloud/src/lib.rs:17-40`.

<a id="r08"></a>
## R08 — Explicit unknowns and limits

Unknown from this static snapshot: complete reverse-dependency graph; CLI/OpenAPI/worker/statuspage/fuzz semantic use; feature-resolved dependencies; wasm/native target results; Clerk, PAT, tenant-path, and Clerk-CF implementation behavior; live Postgres/RLS/migration outcomes; WebAuthn browser/authenticator behavior; secret custody; deployment; and runtime reachability. A re-export or a wasm-related comment is not runtime proof.

[Ownership guide](../../../../.claude/skills/own-corelink-auth/SKILL.md#s01) · [Blast radius](BLAST_RADIUS.md#b01) · [Maintenance](MAINTENANCE.md#m01)
