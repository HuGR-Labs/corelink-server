---
schema: corelink-ownership/1.1
document: reference
package: e2e-dsr
manifest: tests/e2e-dsr/Cargo.toml
source_commit: cb94e251c0f17382565bf863f517945cbb2a84d6
profile: S
state: draft
evidence_set: e2e-dsr-source-cb94e251c
---

# e2e-dsr — source ownership reference

Static source inventory at the pinned commit. The manifest description is declared intent, while local Rust source identifies the harness seam; neither is execution evidence.

[Identity](#r01) · [Boundary](#r02) · [Modules](#r03) · [Contracts](#r04) · [Axioms](#r05) · [Targets](#r06) · [Failures](#r07) · [Evidence](#r08)

<a id="r01"></a>
## R01 — Identity and purpose

| Field | SOURCE evidence |
|---|---|
| Package | `[package].name` is `e2e-dsr`; `tests/e2e-dsr/Cargo.toml:1-8` |
| Manifest library | `src/lib.rs`; manifest `:13-14` |
| Declared integration targets | Twelve `[[test]]` entries; manifest `:28-74` |
| Publish declaration | `publish = false`; manifest `:8` |
| Declared intent | Long manifest description; not evidence of a run or effect; manifest `:3` |
| Runtime observed here | None; this document inspected static source only |

The crate root describes a test harness and exports `helpers`, `policy`, and `r2`; it does not convert package naming into a production component (`src/lib.rs:1-30,40-54`).

<a id="r02"></a>
## R02 — Ownership boundary

| Local harness owns | Static boundary |
|---|---|
| Fixture composition | `TestDsrEnv`, fixed clock, test ledger handles and canonical in-memory adapters (`src/helpers.rs:86-124,126-175`) |
| Restriction and objection model | `TenantPolicyLedger` uses local mutex-backed maps (`src/policy.rs:52-58,60-150`) |
| Signed-URL stand-in | `InMemoryR2EvidenceClient` holds a local object map (`src/r2.rs:55-60,62-115`) |
| Test declarations | Six happy, five adversarial, and one property target are named in the manifest (`Cargo.toml:28-74`) |

The manifest directly declares `corelink-dsr` and `corelink-privacy-erasure-worker` (`Cargo.toml:16-18`). This records Cargo edges only. It does not establish resolved versions, selected targets, production API wiring, authentication, MFA ceremony, actual deletion or pseudonymization, durable audit, R2, provider calls, or runtime reachability.

<a id="r03"></a>
## R03 — Source and target map

| Surface | Source responsibility |
|---|---|
| Root | Public modules/reexports and clock constants; `src/lib.rs:40-63` |
| Helpers | In-memory environment, tenant/request builders, status/audit helpers; `src/helpers.rs:86-175,275-343,369-440` |
| Policy | Local restriction and per-purpose objection maps and decisions; `src/policy.rs:21-58,60-150` |
| R2 | Local map, opaque token, expiry check, and fetch; `src/r2.rs:14-29,31-53,55-115` |
| Happy targets | `happy_{access,erasure,portability,rectification,restriction,objection}.rs`; manifest `:28-50` |
| Adversarial targets | MFA, receipt signature/expiry, partial erasure, SLA; manifest `:52-70` |
| Property target | `prop_dsr_invariants.rs`; manifest `:72-74` |

<a id="r04"></a>
## R04 — Local contracts

**Index:** [API-001](#api-001) · [API-002](#api-002) · [API-003](#api-003)

<a id="api-001"></a>
### API-001 — Shared fixture assembly

`setup_test_env` returns `TestDsrEnv` with an in-memory DSR endpoint/sinks, worker/adapters, local R2 and policy values, and `TEST_NOW_MS` (`src/helpers.rs:90-124,126-175`). It constructs a `ReportSignerKey::synthetic_for_test(0x42)` (`helpers.rs:158-159`). These types and values describe a local fixture, not production wiring. [R04](#r04) · [B03](BLAST_RADIUS.md#b03)

<a id="api-002"></a>
[↩](#r01)
### API-002 — Fixture identity and audit helper

`make_test_tenant(name)` derives tenant, subject, and erasure salt from the supplied name (`src/helpers.rs:275-302,447-469`). `canonical_dsr_for` instead creates a fresh `Uuid::now_v7()` request ID; the explicit-ID builder preserves its caller's ID (`helpers.rs:304-343`). `verify_audit_chain` matches the expected events in subsequence order, allowing other events between (`helpers.rs:401-440`). [R04](#r04)

<a id="api-003"></a>
[↩](#r01)
### API-003 — Local policy and evidence stub

`TenantPolicyLedger` defaults absent restrictions to `Allowed`; a paused tenant maps `evaluate_write` to `Restricted`, while objections are keyed by tenant and exact purpose (`src/policy.rs:60-150`). `InMemoryR2EvidenceClient::get` checks URL expiry before looking up the object key (`src/r2.rs:47-53,93-115`). Neither contract authenticates a URL token or connects to a production policy table/storage service. [R04](#r04)
[↩](#r01)

<a id="r05"></a>
## R05 — Five falsifiable source axioms

**Index:** [INV-001](#inv-001) · [INV-002](#inv-002) · [INV-003](#inv-003) · [INV-004](#inv-004) · [INV-005](#inv-005)

<a id="inv-001"></a>
### INV-001 — Tenant fixture derivation

For a given name, `make_test_tenant` derives tenant UUID, subject UUID, and salt through the corresponding deterministic helper inputs; changing a derivation input or replacing a derived field falsifies the predicate. Source: `src/helpers.rs:289-302,447-469`. Scope: fixture values only, not real tenant identity. [R05](#r05)

<a id="inv-002"></a>
[↩](#r01)
### INV-002 — Clock and environment assembly

`setup_test_env` returns `now_ms = TEST_NOW_MS` and constructs the listed in-memory endpoint, worker, policy, and R2 fixture; changing that assignment or substituting a live adapter falsifies the predicate. Source: `src/helpers.rs:126-175`; `src/lib.rs:56-63`. Scope: source construction, not invocation. [R05](#r05)

<a id="inv-003"></a>
[↩](#r01)
### INV-003 — MFA fixture is synthetic

`canonical_mfa_token` returns `MfaStepUpToken::synthetic_for_test("ok")`; replacing that constructor falsifies the predicate. Source: `src/helpers.rs:362-367`. Scope: helper construction, not WebAuthn or user verification. [R05](#r05)

<a id="inv-004"></a>
[↩](#r01)
### INV-004 — Audit helper matches subsequence

For each expected event, `verify_audit_chain` advances its cursor until a match and permits arbitrary intervening events; changing it to require adjacency falsifies the predicate. Source: `src/helpers.rs:401-440`. Scope: helper algorithm, not an audit-chain integrity guarantee. [R05](#r05)

<a id="inv-005"></a>
[↩](#r01)
### INV-005 — Local URL lookup boundary

`get` returns `Expired` when `now_ms >= expires_at_ms` before map lookup; otherwise it looks up `url.object_key`. It does not compare `url.token` in this method. Any changed branch order or added token comparison falsifies this exact predicate. Source: `src/r2.rs:47-53,93-115`. Scope: local stub only. [R05](#r05)
[↩](#r01)

<a id="r06"></a>
## R06 — Targets and dependencies

| Declaration | SOURCE fact | Limit |
|---|---|---|
| Library | `src/lib.rs`; manifest `:13-14` | Not a test result |
| Six happy tests | Six paths in `:28-50` | Names do not prove execution |
| Five adversarial tests | Five paths in `:52-70` | Names do not prove outcomes |
| Property test | `prop_dsr_invariants`; `proptest` dev dependency (`:25-26,72-74`) | 500-case claim appears in source/manifest prose; no cases were run |
| Direct first-party dependencies | `corelink-dsr`, `corelink-privacy-erasure-worker`; `:16-18` | Does not prove resolution or runtime use |
| Other direct dependencies | `thiserror`, `uuid` with `v7,serde`, `blake3`, `serde`, `serde_json`; `:19-23` | Workspace versions/features were not resolved |

No package-specific feature declaration appears in this manifest excerpt. Workspace inclusion or lockfile presence would not prove target selection.

<a id="r07"></a>
## R07 — Failure surfaces

| Local signal/source | Static meaning | Limit |
|---|---|---|
| `DsrError::Mfa` assertion; `tests/adversarial_mfa_failed.rs:64-81` | Adversarial target checks a synthetic verifier error path | No real MFA ceremony |
| `ErasureWorkerError::Backend`; `tests/adversarial_erasure_partial_failure.rs:53-83` | Target inspects a local adapter failure/tombstone snapshot | No remote backend call |
| `SlaBreached`; `tests/adversarial_sla_breach.rs:26-57` | Target asserts a decision from a local verification job | No scheduler, alert, or operational SLA |
| `R2EvidenceError::{NotFound,Expired}`; `src/r2.rs:14-29,93-115` | Local object/expiry errors | No R2 provider result |

<a id="r08"></a>
## R08 — Evidence and unknowns

Evidence set: manifest, local harness modules, twelve declared test source files, and a static repository search at `cb94e251c0f17382565bf863f517945cbb2a84d6`. The search found a documentary clock reference in `tests/e2e-pilot-onboarding/src/harness.rs:15-18`; this is not a Cargo dependency. No Cargo, build, test, property-test, network, provider, CI, deployment, or runtime action was performed.

Unknowns: (1) resolved dependencies, active features, and target selection; (2) CI selectors and any execution/results; (3) live DSR endpoint, JWT key, user identity, or MFA wiring; (4) durable erasure/backend effects and legal processing state; (5) real signing keys, R2/provider use, deployment, or runtime reachability.

[Impact map](BLAST_RADIUS.md#b01) · [Maintenance](MAINTENANCE.md#m01) · [Start](#r01).
