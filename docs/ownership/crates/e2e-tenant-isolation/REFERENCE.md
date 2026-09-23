---
schema: corelink-ownership/1.1
document: reference
package: e2e-tenant-isolation
manifest: tests/e2e-tenant-isolation/Cargo.toml
source_commit: 1177dad2ca2a9f21c29b5a118aa7944b77147798
profile: S
state: draft
evidence_set: e2e-tenant-isolation-static-1177dad2c
---

# e2e-tenant-isolation — ownership reference

Static source record for a test harness. A declaration, source assertion, or manifest description does not establish a test run, production path, or runtime effect.

[Identity](#r01) · [Boundary](#r02) · [Modules](#r03) · [Contracts](#r04) ·
[Axioms](#r05) · [Targets](#r06) · [Failures](#r07) · [Evidence](#r08).

<a id="r01"></a>
## R01 — Identity and role

| Field | Pinned source fact |
|---|---|
| Cargo package / manifest | e2e-tenant-isolation / [Cargo.toml](../../../../tests/e2e-tenant-isolation/Cargo.toml), package name at lines 1–2 |
| Workspace member | Root [Cargo.toml](../../../../Cargo.toml), member at line 312 |
| Targets | Library at manifest lines 14–15; one explicit integration target adversarial at lines 31–33; autotests=false at line 9 |
| Publish / features | publish=false; no package features table in the manifest |
| Role | Static adversarial harness. The library exports fixtures/fakes; the declared test target contains 25 scenario functions across adversarial.rs and its included adversarial_tail module |
| Execution observed | None. No Rust/Cargo/test command was run for this ownership record |

The package declares version, edition, rust-version, license, and lints through workspace values. This document does not resolve those values or Cargo target/features.

<a id="r02"></a>
## R02 — Ownership boundary

| Surface | Implementation owner | Contract owner | Boundary |
|---|---|---|---|
| Tenant fixture and local fake stores | This package | This package's source assertions | In-memory UUID-keyed/prefix-keyed fixtures; not production stores |
| Prefix derivation | corelink-tenant-path | corelink-tenant-path | Direct manifest dependency and source call; production request reachability unknown |
| Audit event types/emitter | corelink-audit | corelink-audit | Uses InMemoryEmitter and local capture; durable outbox behavior is not implemented here |
| Envelope encryption API | corelink-byok | corelink-byok | Adversarial target uses a local StubKms; no remote KMS selection/contact is shown |
| Production composition | Not this package | Relevant runtime owners not established by this harness | Container/worker wiring, DB, R2, KV, and deployment are outside evidence inspected here |

Canonical routing: the [OKF tenancy isolation reference](../../../knowledge/tenancy/isolation.md) and [invariant registry](../../../../specs/03_architecture/invariant_registry.md). Do not copy their system-wide claims into harness coverage.

<a id="r03"></a>
## R03 — Source map

| Source | Static responsibility |
|---|---|
| [src/lib.rs](../../../../tests/e2e-tenant-isolation/src/lib.rs) | Forbids unsafe code; exports fakes and TenantCtx; describes 25 scenarios and six invariant layers |
| [src/tenants.rs](../../../../tests/e2e-tenant-isolation/src/tenants.rs) | Fixed TENANT_A/TENANT_B UUIDs, shared test TDK, real derive_prefix call, TenantCtx accessors, constant-time UUID comparison helper |
| [src/fakes.rs](../../../../tests/e2e-tenant-isolation/src/fakes.rs), fakes/foundation.rs | Public fake reexports; DenyKind, AuditAttempt, AuditCapture, FakeError; wraps corelink-audit InMemoryEmitter |
| [src/fakes/stores.rs](../../../../tests/e2e-tenant-isolation/src/fakes/stores.rs) | In-memory CAS/R2, D1, PAT, idempotency, quota, and rate-limit behavior |
| [src/fakes/extended.rs](../../../../tests/e2e-tenant-isolation/src/fakes/extended.rs), extended/core.rs | Stripe replay ledger, timing probe, CMK rotation, PAT revoke, region routing, DSR, and audit chain fakes |
| [src/fakes/extended/storage.rs](../../../../tests/e2e-tenant-isolation/src/fakes/extended/storage.rs) | Multipart, child quota, replicated PAT, and audit query fakes |
| [tests/adversarial.rs](../../../../tests/e2e-tenant-isolation/tests/adversarial.rs) | Scenarios S01–S17, KMS test stub, and module inclusion of adversarial_tail |
| [tests/adversarial_tail.rs](../../../../tests/e2e-tenant-isolation/tests/adversarial_tail.rs) | Scenarios S18–S25; included as a module, not another declared Cargo target |

Inventory at the pin: one manifest, one README, eight library source files, and two test source files. The two test files contain 25 named scenario functions; this is a source count, not an execution count.

<a id="r04"></a>
## R04 — Local contracts

**Index:** [API-001](#api-001) · [API-002](#api-002) · [API-003](#api-003) · [API-004](#api-004).

<a id="api-001"></a>
### API-001 — Deterministic tenant fixture

`TenantCtx::tenant_a() -> Result<Self, TenantCtxError>` and `tenant_b() -> Result<Self, TenantCtxError>` parse fixed UUIDv7 strings, create a `TenantDerivationKey` from the same constant test bytes, and derive a `TenantPrefix` through `corelink_tenant_path::derive_prefix`. `tenant_id(&self) -> Uuid`, `prefix(&self) -> &TenantPrefix`, and `audit_tenant_id(&self) -> corelink_audit::TenantId` expose the fixture fields. `ct_tenant_eq(a: &Uuid, b: &Uuid) -> bool` compares the 16 UUID bytes with `subtle::ConstantTimeEq`. These are synthetic fixture identities and key bytes, not tenant credentials or key material. Source: tenants.rs lines 17–21, 43–48, 50–115. [R04](#r04) · [B03](BLAST_RADIUS.md#b03)

<a id="api-002"></a>
[↩](#r01)
### API-002 — In-memory fake surface

Crate-root reexports are `{AuditAttempt, AuditCapture, AuditChain, AuditQueryEngine, CasStore, CmkRotationLedger, ConstantTimeAuthProbe, DenyKind, DsrIntake, HierarchicalQuotaStore, IdempotencyStore, KvReplicatedPatStore, MultipartBroker, PatRevokeLedger, PatStore, QuotaStore, RateLimiter, RegionRouter, StripeWebhookLedger}` (`lib.rs:78–87`). Nested `fakes` additionally reexports `{D1Row, D1Store, FakeError}` (`fakes.rs:9–15`); `D1Row` has `tenant_id: Uuid` and `payload: Vec<u8>`, and `D1Store::{new(audit: AuditCapture)->Self, insert(&self, Uuid, Uuid, &str, Vec<u8>)->Result<(), FakeError>}` (`stores.rs:142–200`). `AuditCapture` methods and `AuditAttempt::new` are exact in API-004. Mutating/validation methods generally return `Result<_, FakeError>`; introspection returns scalars/tuples. All are local fakes, not production adapters. [R04](#r04)

<a id="api-003"></a>
[↩](#r01)
### API-003 — Audit capture ordering

AuditCapture::record_deny constructs an AuthEvent, calls the in-memory emitter, then appends the AuditAttempt. An emitter error returns FakeError::Audit before the attempt is stored. Scenario sources inspect capture counts and selected deny fields. This order is local to the fake; the crate does not own or demonstrate the durable production outbox. Source: fakes/foundation.rs lines 124–178, 280–305. [R04](#r04) · [INV-002](#inv-002)

<a id="api-004"></a>
[↩](#r01)
### API-004 — Public fake and constant-time helper signatures

The crate-root public surface is `pub mod fakes`, `pub mod tenants`, and the reexports listed in API-002; `TenantCtx` is reexported from `tenants`. The fake capture API is `AuditCapture::new() -> Self`, `emitter(&self) -> &InMemoryEmitter`, `count(&self) -> usize`, `snapshot(&self) -> Vec<AuditAttempt>`, `last_deny(&self) -> Option<AuditAttempt>`, and `record_deny(&self, AuditAttempt) -> Result<(), FakeError>`. `AuditAttempt::new(kind: DenyKind, requester: Uuid, resource_owner: Uuid) -> Self` is the constructor for its non-exhaustive record. `ct_tenant_eq` is a free public function, not a `TenantCtx` method. Source: lib.rs:76–87; tenants.rs:43–115; fakes/foundation.rs:92–120, 129–178. [R04](#r04)
[↩](#r01)

<a id="r05"></a>
## R05 — Five falsifiable source axioms

**Index:** [INV-001](#inv-001) · [INV-002](#inv-002) · [INV-003](#inv-003) · [INV-004](#inv-004) · [INV-005](#inv-005).

<a id="inv-001"></a>
### INV-001 — Fixed test identities

TenantCtx uses the two fixed UUID strings and shared TDK bytes to derive each fixture prefix through derive_prefix. Changing either input or replacing the derivation call changes this source predicate. It does not prove a production tenant lookup or uniqueness guarantee. Source: tenants.rs lines 20–54. [R05](#r05)

<a id="inv-002"></a>
[↩](#r01)
### INV-002 — Emitter before capture append

record_deny calls InMemoryEmitter::emit before pushing the matching AuditAttempt. Reordering those operations falsifies this source predicate. It does not prove durable audit, and an enclosing operation may emit later than a cryptographic error. Source: fakes/foundation.rs lines 280–305. [R05](#r05)

<a id="inv-003"></a>
[↩](#r01)
### INV-003 — BYOK AAD rejection is a caller-boundary check

S04 expects BYOKError::AadMismatch for decrypt under the wrong tenant context. S05 also receives AadMismatch, then explicitly records a deny through the test AuditCapture. S04 has no audit assertion; S05 records after decrypt returns. Therefore these scenarios do not establish a universal emit-before-crypto-rejection predicate. Source: adversarial.rs lines 229–299. [R05](#r05)

<a id="inv-004"></a>
[↩](#r01)
### INV-004 — D1 fake takes tenant from the JWT argument

D1Store::insert compares jwt_tenant and body_tenant; on mismatch it records a deny and returns before inserting. On the matching branch the row key and stored tenant use jwt_tenant. S07 asserts the conflicting-body rejection. This is a fake contract, not proof of a deployed JWT middleware path. Source: fakes/stores.rs lines 142–199; adversarial.rs lines 391–418. [R05](#r05)

<a id="inv-005"></a>
[↩](#r01)
### INV-005 — The target declares 25 scenarios in one module tree

The manifest disables automatic test discovery and declares one adversarial target. adversarial.rs defines S01–S17 and includes adversarial_tail; that module defines S18–S25. Removing the declaration or inclusion changes the source/target census, but this alone proves neither selection nor execution. Source: Cargo.toml lines 9, 31–33; adversarial.rs lines 1–12, 802; adversarial_tail.rs lines 1–25. [R05](#r05)
[↩](#r01)

<a id="r06"></a>
## R06 — Targets and direct dependencies

| Manifest declaration | Source role | Limit |
|---|---|---|
| Library: src/lib.rs | Fixture/fake API consumed by the adversarial target | Declared target, not proof of selection |
| Test: adversarial at tests/adversarial.rs | 25 scenario functions; tail is a submodule | No run result observed |
| Normal dependency: corelink-tenant-path | TenantPrefix / derive_prefix used by fixture and fake storage source | Direct declaration/source use; no production reachability |
| Normal dependency: corelink-audit | Audit event/emitter source used by fake capture | Direct declaration/source use; no durable event claim |
| Normal dependency: corelink-byok | EnvelopeEncryptor API used by S04/S05 | Direct declaration/source use; StubKms is local |
| Normal dependency: async-trait | Implements the test-local StubKms KmsProvider trait | Workspace version/features not resolved |
| Normal dependency: serde_json | Supplies the KmsProvider encryption-context value type in the test stub | Workspace version/features not resolved |
| Normal dependency: thiserror | Derives local TenantCtxError and FakeError types | Workspace version/features not resolved |
| Normal dependency: subtle | Constant-time UUID-byte equality helper | No timing/runtime measurement |
| Normal dependency: uuid | Fixed fixture IDs and tenant-keyed fake maps | No production identity resolution |
| Normal dependency: zeroize | Wraps fixed fixture TDK bytes | Bytes are test values, not production secrets |
| Dev dependency: tokio | Async test runtime for BYOK scenarios S04/S05 | Declared dev edge; no test ran |

The package manifest has no feature table or binary target. Workspace-inherited version/edition/rust-version/license and dependency feature resolution remain unresolved because Cargo was not run. “No feature flag in this manifest” does not mean no features are selected in dependencies.

<a id="r07"></a>
## R07 — Failure and evidence boundaries

| Source result | What it means locally | What it does not establish |
|---|---|---|
| FakeError::Deny(DenyKind) | A named fake rejection predicate; taxonomy has 17 variants | HTTP status, production auth behavior, or live audit delivery |
| FakeError::Audit / MutexPoisoned / NotFound / Crypto | Local emitter, mutex, lookup, or crypto-fake error surface | Service recovery or provider errors |
| BYOKError::AadMismatch in S04/S05 | Encryptor call returns the asserted error under a wrong tenant AAD | KMS behavior; StubKms wraps bytes locally and performs no network IO |
| Assertion failure or compilation error | Would indicate a source or API mismatch if the target were built | No failure or pass was observed in this record |

Do not cite the crate comment or manifest description alone for every-rejection audit ordering. INV-003 records a concrete scenario-level limitation.

<a id="r08"></a>
## R08 — Evidence and unknowns

Evidence is the exact source snapshot 1177dad2ca2a9f21c29b5a118aa7944b77147798 plus the package paths and line anchors listed above. The assigned integration baseline and pinned source have no content diff in root Cargo.toml or tests/e2e-tenant-isolation. No source edit, Cargo metadata, Cargo build, test, clippy, fuzz, network, provider, GitHub, deployment, or runtime operation was performed.

Unknown: resolved dependency graph/features; actual target selection and CI results; production caller/composition; live database/storage/provider effects; whether each comment-level claim is exercised by a test assertion. The two residual scripts named in [B03](BLAST_RADIUS.md#b03) contain import expectations that differ from the pinned adversarial_tail source; this remains unresolved and unexecuted.

[Impact map](BLAST_RADIUS.md#b01) · [Maintenance](MAINTENANCE.md#m01) · [Start](#r01).
