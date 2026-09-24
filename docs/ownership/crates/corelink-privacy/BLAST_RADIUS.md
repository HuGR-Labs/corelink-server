---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-privacy
manifest: crates/corelink-privacy/Cargo.toml
source_commit: 8b800acd3ffb042e5f68bedbb989415a5c5b0bbe
profile: H
state: draft
evidence_set: privacy-source-static-20260920
---

# corelink-privacy — blast radius

Atomic source/static relations for the hybrid umbrella. A dependency names a
declared edge, a flow names a source path, and an impact is a review consequence;
none proves execution, deployment, or a complete consumer set.

[Method](#b01) · [Local modules](#b02) · [DSR facade](#b03) ·
[Worker facade](#b04) · [Pseudonymize/DPA facades](#b05) · [Consumers](#b06)

<a id="b01"></a>
## B01 — Root-to-local-module relation

**Dependency / flow / impact:** `src/lib.rs` → local module declaration → local
submodule tree for `breach`, `consent`, `notice`, `residency`, `sub_processor`,
and `dpa::versioning`; changes can alter an exposed local source contract.

**Predicate:** a root declaration, local module, re-export, trait, or public type changes.

**Evidence:** `src/lib.rs`; `src/{breach,consent,notice,residency,sub_processor,dpa}.rs`.

**Unknown/stop:** an external adapter or execution outcome is required.

<a id="b02"></a>
## B02 — Local trait-to-implementation relation

**Dependency / flow / impact:** local event-sink/store/emitter trait → local
in-memory implementation → local caller/type; changing a trait or error can
affect local implementers and imports.

**Predicate:** a trait method, error, local implementation, or public export changes.

**Evidence:** `src/{breach,consent,notice,residency,sub_processor}.rs` and their
local submodules.

**Unknown/stop:** a trait or fake is not evidence of an external adapter or effect.

Bounded local contract inventory (the exact anchors and source facts are in
[R03](REFERENCE.md#r03)); each row is a change-to-review route, not a runtime
dependency edge.

| Local family | Contract anchors | Change impact / falsifiable source check |
|---|---|---|
| `breach` | `BreachNotificationDispatch`, `escalation_policy_for`, `BreachAuditSink` | payload, escalation mapping, or audit ordering changes; inspect `src/breach/{event,audit_emit}.rs` |
| `consent` | `ConsentLedger`, `ConsentProofPayload`, `ConsentHmacSigner`, `ConsentStore` | proof schema/signing or audit-before-store behavior changes; inspect `src/consent/{schema,hmac_sign,ledger}.rs` |
| `notice` | `NoticeEmitter`, `NoticeEmitDecision`, `notice_text_hash` | locale/hash/version decision or fail-closed state behavior changes; inspect `src/notice/{event,emitter}.rs` |
| `residency` | `Region`, `TenantCtx`, request/write assertions | region choice, assertion result, or audit failure behavior changes; inspect `src/residency/{region,assert_request,assert_write}.rs` |
| `sub_processor` | `SubProcessorEmitter`, `BroadcastStore`, `ObjectionStore`, `derive_dkim_key` | broadcast identity, tenant keying, or audit ordering changes; inspect `src/sub_processor/{emitter,broadcast,dkim}.rs` |
| `dpa::versioning` | `DpaVersioning`, `ReadOnlyDegradeGate`, `ReAcceptanceReceipt` | version/grace/gate transition changes; inspect `src/dpa/versioning/{version,versioning,middleware}.rs` |

These contracts cover representative local API seams, not every public
declaration. In-memory implementations remain local test/reference behavior;
external persistence, routes, sends, and deployment stay unknown.

<a id="b03"></a>
## B03 — DSR facade relation

**Dependency / flow / impact:** `corelink_privacy::dsr` → `corelink-dsr`; nested
`corelink_privacy::dsr::statuspage` → `corelink-dsr-statuspage-scheduler`.
Changing the facade path or dependency can affect imports at the umbrella path.

**Predicate:** `src/dsr.rs`, its nested module, or either dependency declaration changes.

**Evidence:** `Cargo.toml`; `src/dsr.rs`.

**Unknown/stop:** DSR and scheduler implementation ownership remains with their
defining packages; no facade edge proves execution.

<a id="b04"></a>
## B04 — Erasure-worker facade relation

**Dependency / flow / impact:** `corelink_privacy::erasure` →
`corelink-privacy-erasure-worker`; a source consumer can import the worker's
public names through the umbrella route.

**Predicate:** `src/erasure.rs` or its manifest dependency changes.

**Evidence:** `Cargo.toml`; `src/erasure.rs`.

**Unknown/stop:** the worker's implementation and execution ownership remain
outside this package.

<a id="b05"></a>
## B05 — Pseudonymize and DPA facade relations

**Dependency / flow / impact:** `corelink_privacy::pseudonymize` →
`corelink-privacy-pseudonymize`; `corelink_privacy::dpa::acceptance` →
`corelink-dpa-acceptance`. A facade change can change the two import routes.

**Predicate:** `src/{pseudonymize,dpa}.rs` or either dependency declaration changes.

**Evidence:** `Cargo.toml`; `src/{pseudonymize,dpa}.rs`.

**Unknown/stop:** neither relation transfers implementation ownership or proves
configured behavior of the defining package.

<a id="b06"></a>
## B06 — Static consumer and coverage relation

**Dependency / flow / impact:** workspace `Cargo.toml` → `corelink-privacy`
workspace dependency; package-local tests → `corelink_privacy::<module>` imports.
A public-path change requires a fresh static consumer search.

**Predicate:** public module, re-export, dependency, or a local contract anchor in R03 changes.

**Evidence:** root `Cargo.toml`; `crates/corelink-privacy/tests/*.rs`.

**Unknown/stop:** no separate non-test manifest consumer was found in this pass;
generated, feature-selected, external, and complete reverse consumers remain unknown.
For local traits/types, perform a fresh source search and consult the family
contract row above; filenames and declared test patterns do not prove coverage.

[Reference](REFERENCE.md#r01) · [Maintenance](MAINTENANCE.md#m01) · [Start](#b01)
