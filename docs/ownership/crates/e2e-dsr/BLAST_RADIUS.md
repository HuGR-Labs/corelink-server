---
schema: corelink-ownership/1.1
document: blast_radius
package: e2e-dsr
manifest: tests/e2e-dsr/Cargo.toml
source_commit: cb94e251c0f17382565bf863f517945cbb2a84d6
profile: S
state: draft
evidence_set: e2e-dsr-source-cb94e251c
---

# e2e-dsr — blast radius

Static package and source relationships. A manifest edge is a declaration; a test assertion is a source-level check; neither proves execution or downstream production behavior.

[Scope](#b01) · [Method](#b02) · [Atomic relations](#b03) · [Propagation](#b04) · [Change map](#b05) · [Coverage and unknowns](#b06)

<a id="b01"></a>
## B01 — Scope

This map covers `tests/e2e-dsr/Cargo.toml`, `src/{lib,helpers,policy,r2}.rs`, its twelve declared test sources, and the one statically found documentary clock reference. It does not claim a complete resolved dependency graph, CI selection, runtime graph, provider contact, or production impact.

<a id="b02"></a>
## B02 — Method and populations

| Population | Evidence | Limit |
|---|---|---|
| Package identity and direct declarations | `tests/e2e-dsr/Cargo.toml:1-74` | Not resolved dependency selection |
| Local composition | `src/helpers.rs:86-175`; local policy/R2 modules | Not external wiring |
| Test call sites | Twelve manifest paths; imports and calls in each path | Declared source, not execution |
| Documentary reference | `tests/e2e-pilot-onboarding/src/harness.rs:15-18` | Not a Cargo edge |
| CI, resolved reverse graph, external effects | UNKNOWN | No runtime or CI census performed |

<a id="b03"></a>
## B03 — Atomic relations

**Index:** [REL-001](#rel-001) · [REL-002](#rel-002) · [REL-003](#rel-003) · [REL-004](#rel-004) · [REL-005](#rel-005) · [REL-006](#rel-006) · [REL-007](#rel-007)

<a id="rel-001"></a>
### REL-001 — DSR dependency fixture

**Arrow:** `e2e-dsr` manifest → `corelink-dsr` package. **Surface:** direct path dependency; helpers construct `InMemoryDsrEndpoint` and tests call its trait surface. **Activation:** source path `tests/e2e-dsr/src/helpers.rs:22-26,126-141`; happy test `tests/e2e-dsr/tests/happy_access.rs:36-65`. **Failure boundary:** an incompatible type/API can prevent the harness from compiling. **Limit:** no endpoint, JWT, identity, or runtime effect is established. [B03](#b03)

<a id="rel-002"></a>
### REL-002 — Erasure worker fixture

**Arrow:** `e2e-dsr` manifest → `corelink-privacy-erasure-worker` package. **Surface:** direct path dependency; helper builds an in-memory worker from canonical in-memory adapters. **Activation:** `Cargo.toml:16-18`; `src/helpers.rs:27-30,143-159`; the erasure target explicitly calls `process_erasure` at `tests/e2e-dsr/tests/happy_erasure.rs:103-116`. **Failure boundary:** API/constructor incompatibility reaches this harness surface. **Limit:** no real backend call or deletion is shown. [B03](#b03)

<a id="rel-003"></a>
### REL-003 — Library fixture to integration targets

**Arrow:** `e2e_dsr` library exports → twelve package test targets. **Surface:** root reexports helpers, policy, and R2 types. **Activation:** `src/lib.rs:44-54`; manifest targets `Cargo.toml:28-74`; tests import `e2e_dsr`, for example `happy_access.rs:19-22`. **Failure boundary:** changed exports can break importing targets. **Limit:** target declaration/import does not establish selection or execution. [B03](#b03)

<a id="rel-004"></a>
### REL-004 — Restriction target to local ledger

**Arrow:** `happy_restriction` target → local `TenantPolicyLedger`. **Surface:** the test explicitly calls `set_restriction` after `dsr.submit`, then inspects `evaluate_write`. **Activation:** `tests/e2e-dsr/tests/happy_restriction.rs:33-55`. **Failure boundary:** changed local flag/decision behavior can falsify these assertions. **Limit:** source does not show automatic event-to-policy wiring or a production table. [B03](#b03)

<a id="rel-005"></a>
### REL-005 — Objection target to local ledger

**Arrow:** `happy_objection` target → local `TenantPolicyLedger`. **Surface:** explicit `record_objection` for one tenant-purpose pair and checks of matching/unrelated purposes. **Activation:** `tests/e2e-dsr/tests/happy_objection.rs:49-75`. **Failure boundary:** changed map key or decision logic can falsify the assertions. **Limit:** no production pipeline consumer is shown. [B03](#b03)

<a id="rel-006"></a>
### REL-006 — Portability target to local R2 stub

**Arrow:** `happy_portability` target → `InMemoryR2EvidenceClient`. **Surface:** serializes a fixture export, stores by a constructed key, and fetches within the local TTL. **Activation:** `tests/e2e-dsr/tests/happy_portability.rs:54-82`. **Failure boundary:** local key, expiry, or stored bytes mismatch fails the source assertion. **Limit:** payload is a fixture stand-in; no R2 request or customer download is established. [B03](#b03)

<a id="rel-007"></a>
### REL-007 — Onboarding clock reference

**Arrow:** `e2e-pilot-onboarding` harness source → `e2e-dsr` clock declaration. **Surface:** a comment says the local fixed clock mirrors the DSR harness; both visible constants are `1_700_000_000_000`. **Activation:** `tests/e2e-pilot-onboarding/src/harness.rs:15-18`; `tests/e2e-dsr/src/lib.rs:56-59`. **Failure boundary:** changing one pin can invalidate this documentary correspondence. **Limit:** not a Cargo edge or shared runtime fixture. [B03](#b03)

<a id="b04"></a>
## B04 — Propagation and composition boundaries

An upstream API change can break this harness at the two direct dependency surfaces. A local assertion change can reduce what its declared target would detect. Restriction and Objection targets manually mutate the policy ledger; the erasure target manually calls the worker. These source paths do not show an event queue connecting those operations. No direction from a passing test to live DSR, data erasure, storage, audit, or provider state is justified.

<a id="b05"></a>
## B05 — Change to impact to static validation

| Source change | Review surface | Static evidence to re-read |
|---|---|---|
| Package name, targets, or dependencies | Identity/selection claims | `Cargo.toml`; R01/R06 |
| Fixture or exported helper | Imports and fixture invariants | `src/lib.rs`, `src/helpers.rs`; INV-001–004 |
| Policy helper | Manual Restriction/Objection assertions | `src/policy.rs`; REL-004/005 |
| R2 stub | Expiry/key lookup and local callers | `src/r2.rs`; INV-005; REL-006 |
| Upstream type/API | Direct dependency relationship and test call sites | REL-001/002; stop if behavior needs execution proof |

<a id="b06"></a>
## B06 — Coverage and unknowns

The static inventory records two direct first-party manifest edges, three local public modules, twelve declared integration targets, and one documentary cross-package clock reference. It does not prove transitive consumers, a selected CI job, or a reverse runtime graph.

Unknowns: (1) dependency resolution/features/target selection; (2) CI selectors and run results; (3) live DSR, JWT, identity, or MFA wiring; (4) durable erasure/backend effects or legal processing state; (5) external signing keys, R2/provider activity, deployment, or runtime reachability. Route canonical OKF context only through the [SRE operations hub](../../../knowledge/ops/sre-operations-hub.md).

[Reference](REFERENCE.md#r01) · [Maintenance](MAINTENANCE.md#m01) · [Start](#b01).
