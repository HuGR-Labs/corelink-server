---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-stripe-real
manifest: crates/corelink-stripe-real/Cargo.toml
source_commit: 16d9f0303d849a1ab3df14688bd2c7cdbfee8140
profile: S
state: author_validated
evidence_set: w011-stripe-real-source-static-20260920
---

# corelink-stripe-real — blast radius

SOURCE-only atomic relation map. Every arrow below is a declared source
relation, not proof of credentials, network activity, external effects,
configuration, deployment, or runtime behavior.

[Root gates](#b01) · [Root surface](#b02) · [Trait seam](#b03) · [Verifier](#b04) · [Dispatcher/fakes](#b05) · [Unknowns](#b06)

<a id="b01"></a>
## B01 — Manifest target block → crate-root gate relation

**Arrow:** `Cargo.toml` native target dependency block → `src/lib.rs`
`#[cfg(not(target_arch = "wasm32"))]` client module/re-exports; wasm target
dependency block → opposed clock re-export. **Mode:** SOURCE. **Evidence:**
`Cargo.toml`; `src/lib.rs`. **Boundary:** this does not select, compile, link,
or execute either target.

<a id="b02"></a>
## B02 — Crate root → public local surface relation

**Arrow:** `src/lib.rs` re-exports → local clock, DLQ, error, portal, retry,
webhook, and dispatcher surface. **Mode:** SOURCE. **Evidence:** `src/lib.rs`.
**Boundary:** a public path is not evidence of a caller, compatibility result,
or invocation.

<a id="b03"></a>
## B03 — Local dispatcher module → imported trait relation

**Arrow:** `webhook_dispatch` `pub use` →
`corelink_billing_stripe_traits` identities and port traits. **Mode:** SOURCE.
**Evidence:** direct path dependency in `Cargo.toml`; `src/webhook_dispatch.rs`.
**Boundary:** the dependency owns its definitions; this re-export neither
selects an implementation nor establishes any cross-package behavior.

<a id="b04"></a>
## B04 — Verifier inputs → local result relation

**Arrow:** byte slice, header text, supplied secret bytes, and supplied time →
`verify_webhook_signature` → `Result<(), WebhookVerifyError>`. **Mode:**
SOURCE. **Evidence:** `src/{webhook,error}.rs`. **Boundary:** its parse,
tolerance, HMAC, and comparison branches do not prove that a request supplied
the inputs, that a secret is configured, or that a clock is trusted.

<a id="b05"></a>
## B05 — Dispatcher-use and local-implementation relations

Each row is one directed SOURCE relation. The five `use` rows name only a
dispatcher field/constructor relation; the five `implementation` rows name
only an implementation relation. No row establishes construction, invocation,
persistence, audit delivery, or any external state effect.

| Atomic arrow | Evidence | Boundary |
|---|---|---|
| `WebhookDispatcher` → `IdempotencyStore` | `src/webhook_dispatch.rs` idempotency field and constructor parameter | The source use does not prove a selected store or persisted deduplication. |
| `InMemoryIdempotencyStore` → `IdempotencyStore` | `src/webhook_dispatch.rs` `impl IdempotencyStore for InMemoryIdempotencyStore` | The mutex-backed fake does not prove a selected store or persisted deduplication. |
| `WebhookDispatcher` → `StateMaterializer` | `src/webhook_dispatch.rs` materializer field and constructor parameter | The source use does not prove materialization, a consumer, or a state effect. |
| `RecordingStateMaterializer` → `StateMaterializer` | `src/webhook_dispatch.rs` `impl StateMaterializer for RecordingStateMaterializer` | The recorder does not prove materialization, a consumer, or a state effect. |
| `WebhookDispatcher` → `AuditEmitter` | `src/webhook_dispatch.rs` audit field and constructor parameter | The source use does not prove durable audit storage or delivery. |
| `RecordingAuditEmitter` → `AuditEmitter` | `src/webhook_dispatch.rs` `impl AuditEmitter for RecordingAuditEmitter` | The recorder does not prove durable audit storage or delivery. |
| `WebhookDispatcher` → `SliRecorder` | `src/webhook_dispatch.rs` SLI field and constructor parameter | The source use does not prove metric publication, collection, or observation. |
| `RecordingSliRecorder` → `SliRecorder` | `src/webhook_dispatch.rs` `impl SliRecorder for RecordingSliRecorder` | The recorder does not prove metric publication, collection, or observation. |
| `WebhookDispatcher` → `WebhookDlqStore` | `src/webhook_dispatch.rs` DLQ field and `with_dlq` parameter | The optional source use does not prove quarantine, replay, or persistence. |
| `InMemoryWebhookDlqStore` → `WebhookDlqStore` | `src/dlq.rs` `impl WebhookDlqStore for InMemoryWebhookDlqStore` | The map-backed fake does not prove quarantine, replay, or persistence outside the fake. |

<a id="b06"></a>
## B06 — Unresolved relation boundary

**Arrow:** package declarations → five unresolved domains: target/build
selection; callers/composition; credentials/network/external behavior;
clock/audit/persistence effects; deployment/runtime observation. **Mode:**
UNKNOWN. **Evidence:** [R08](REFERENCE.md#r08). **Boundary:** B01–B05 may not
be expanded into an unknown-domain claim without separate evidence.

Known reverse Cargo consumers:

| Relation | Consumer → source relation and activation | Evidence | Unknown / limit |
|---|---|---|---|
| RC-001 | `corelink-billing` → `stripe::real` publicly re-exports this crate. | `crates/corelink-billing/Cargo.toml`; `src/stripe.rs` | Facade use and target selection are unknown. |
| RC-002 | `corelink-adapters-cloud` → `stripe` publicly re-exports this crate. | `crates/corelink-adapters-cloud/Cargo.toml`; `src/stripe.rs` | Facade use and target selection are unknown. |
| RC-003 | `corelink-container` → handler/webhook and checkout modules refer to `StripeRealClient`, dispatcher, and DLQ contracts; route source constructs clients. | `crates/corelink-container/Cargo.toml`; `src/customer_d1_handler_state.rs`; `src/routes/tier_select/part-00-01.rs`; `src/webhook.rs` | Construction paths require their server routes; no network request or external result is established. |
| RC-004 | `corelink-billing-stripe-materializer` → its integration-test target constructs this crate's `WebhookDispatcher` and fakes; manifest comment marks the dependency for that test. | `crates/corelink-billing-stripe-materializer/Cargo.toml`; `tests/materializers_e2e.rs` | Test target execution is unproven; production source uses the extracted traits crate instead. |
| RC-005 | `e2e-signup-flow` → harness manifest depends on this crate for the credential-gated live Stripe variant. | `tests/e2e-signup-flow/Cargo.toml`; `src/lib.rs`; `tests/happy_path_starter_stripe_test_mode.rs` | The source states live credentials/ignored test conditions; neither test execution nor provider contact is established. |

These are the known direct Cargo edges inspected here, not an exhaustive
compiled graph. Target/feature selection, caller invocation, credentials,
network effects, and deployed behavior remain unknown.

[Reference](REFERENCE.md#r01) · [Maintenance](MAINTENANCE.md#m01) · [Ownership guide](../../../../.claude/skills/own-corelink-stripe-real/SKILL.md#s01)
