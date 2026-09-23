---
schema: corelink-ownership/1.1
document: blast_radius
package: chaos-campaign
manifest: tests/chaos/Cargo.toml
source_commit: cb94e251c0f17382565bf863f517945cbb2a84d6
profile: S
state: candidate
evidence_set: chaos-campaign-static-cb94e251c
---

# chaos-campaign — blast radius

Static boundary census for package `chaos-campaign`, repository ID `1232040291`. Its models are self-contained and tests are default-off sources. A relation here does not establish target selection, test execution, production wiring, delivery, or runtime. The [OKF profile](../../../../docs/internal/okf-wiki/01-okf-corelink-profile.contract.md) is route/reference only and is not duplicated.

[Scope](#b01) · [Method](#b02) · [Relations](#b03) · [Propagation](#b04) · [Change impact](#b05) · [Coverage](#b06).

<a id="b01"></a>
## B01 — Scope and boundary

In scope: root workspace membership, `tests/chaos/Cargo.toml`, its library target and 11 named test targets, plus each test source's local import/assertion boundary to the package library. Package declares no dependency tables. Every scenario is an in-memory model. Builds, target/feature resolution, CI, current execution, production activity, and reverse runtime consumers are not observed.

**Builds evaluated:** none. No package/target/features/artifact selection or command execution was performed. **Environment:** static checkout at source commit `cb94e251c0f17382565bf863f517945cbb2a84d6` only.

<a id="b02"></a>
## B02 — Discovery method and populations

Method: read root and package Cargo manifests, every declaration in `tests/chaos/Cargo.toml`, `src/lib.rs`, and all 11 package test sources. Search tracked checkout text for exact package/crate identifiers and `tests/chaos`; inspect `.rs`, `Cargo.toml`, ownership/runbook/audit populations separately. Count unique files/manifest declarations in the bounded paths, not search hits as edges. No Cargo metadata/tree or runtime instrumentation was run.

| Population | Search/review and count | Meaning / limit |
|---|---|---|
| Direct Cargo dependency declarations | Complete package manifest: 0 normal, 0 build, 0 dev, 0 target-specific tables. | No declared direct package dependency; not a resolved graph. |
| Inverse exact-name Cargo declarations | Workspace manifests searched for `chaos-campaign` / `chaos_campaign`: 0 consumer dependency declarations. | Root workspace membership and own `package.name` are not inverse dependencies. Resolved external/global graph UNKNOWN. |
| Workspace membership | 1 root member entry at `Cargo.toml:329`. | One membership relation, not an invocation or target selection. |
| Internal source clients | 11 files importing `chaos_campaign`, all under this package's `tests/chaos/tests/`; 16 source `#[test]` functions. | All client files mapped below; none executed. |
| External Rust clients | 0 `.rs` files outside `tests/chaos/**` matching the exact package/crate/source identifiers. | Static text search only; generated/dynamic/other-repository clients UNKNOWN. |
| Runbook prose | 1 exact runbook match: `specs/_runbooks/RB-CHAOS-CAMPAIGN.md`. | Operational instructions/history, not a current package call edge or execution record. |
| Ownership/census prose | 4 matching files: `docs/ownership/CARGO_CENSUS.md`, `docs/ownership/ROLLOUT.md`, `docs/ownership/WAVE_014_PLAN.md`, and `docs/ownership/crates/corelink-signup/REFERENCE.md`. | Documentary mentions only; owned docs are excluded to prevent self-reference. |
| Sealed audit prose | 16 matching files under `specs/_audits/sealed/` (listed in B06). | Immutable historical text only; it does not prove current execution/state. |
| Other tracked lexical records | 5 unique files, itemized as LEX-001–005 below; counts are files, not token occurrences. | SBOM/lock inventory and handoff/release/audit-index text; no source call, manifest edge, or current execution is inferred. |

**Direct/inverse/transitive result:** direct declared dependencies `0`; inverse exact-name dependency declarations `0`; statically declared internal Cargo dependency paths to/from this package `0`. A resolved graph outside this bounded declaration search remains UNKNOWN. The 11 intra-package test-source imports are enumerated as `test` relations, not misreported as Cargo dependency-table edges.

Read-only search commands used:

```sh
rg -n 'chaos-campaign|chaos_campaign|tests/chaos' --glob 'Cargo.toml' .
rg -l 'use chaos_campaign' tests/chaos/tests
rg -l 'chaos-campaign|chaos_campaign|tests/chaos' --glob '*.rs' --glob '!tests/chaos/**' .
rg -l 'chaos-campaign|chaos_campaign|tests/chaos|chaos campaign' docs/ownership specs/_runbooks specs/_audits/sealed
rg -n -i 'chaos-campaign|chaos_campaign|tests/chaos|chaos campaign' .sbom/cyclonedx-rust.json docs/handoff/2026-06-28-enterprise-dd-RESPONSE.md docs/release/v1.0.0-GA-tag-draft.txt specs/_audits/2026-05-27-specs-inventory-cleanup-map.md Cargo.lock
```

Classify directory populations separately and exclude these owned artifacts from documentary-text counts; matches are not inferred as edges.

<a id="b03"></a>
## B03 — Complete direct relation register

`REL-001` is root workspace membership. `REL-002`–`REL-012` are one relation per declared test target/source to its local library surface. Shared keys use repository ID `1232040291`. For every row: dependency direction is consumer→provider; flow is compile-time-only/not-applicable (no runtime data flow); impact direction is the source seam change toward the named target. No owner outside this local package is verified.

| ID | Type / direction | Surface | Activation | Contract owner |
|---|---|---|---|---|
| [REL-001](#rel-001) | build-deploy / root config → member | Root `members` entry → `tests/chaos` | Workspace manifest inclusion; command selection unknown | Workspace owner unknown |
| [REL-002](#rel-002) | test / target → library | Network-partition model and assertions | Declared target; source `cfg(feature="chaos")` | Local package source |
| [REL-003](#rel-003) | test / target → library | D1 pool model and assertions | Declared target; source cfg | Local package source |
| [REL-004](#rel-004) | test / target → library | Dual-write model and assertions | Declared target; source cfg | Local package source |
| [REL-005](#rel-005) | test / target → library | RLS model and assertions | Declared target; source cfg | Local package source |
| [REL-006](#rel-006) | test / target → library | BYOK model and assertions | Declared target; source cfg | Local package source |
| [REL-007](#rel-007) | test / target → library | Webhook model and assertions | Declared target; source cfg | Local package source |
| [REL-008](#rel-008) | test / target → library | JWKS model and assertions | Declared target; source cfg | Local package source |
| [REL-009](#rel-009) | test / target → library | Multipart model and assertions | Declared target; source cfg | Local package source |
| [REL-010](#rel-010) | test / target → library | Failover + BYOK local composition | Declared target; source cfg | Local package source |
| [REL-011](#rel-011) | test / target → library | D1 + webhook local composition | Declared target; source cfg | Local package source |
| [REL-012](#rel-012) | test / target → library | Dual-write + private export fixture | Declared target; source cfg | Local package source |

<a id="rel-001"></a>
### REL-001 — Root workspace membership
**Identity:** `repo:1232040291:boundary:chaos-campaign-workspace-member`.
**Dependency / flow / impact:** Not a Cargo dependency / not applicable / root membership edit changes workspace inventory.
**Surface:** `Cargo.toml:329` entry `tests/chaos` → `tests/chaos/Cargo.toml`.
**Activation:** Root workspace declaration; command/target selection unknown.
**Contract:** Member path included in declared workspace; no package call follows.
**State / effect:** Manifest text only; no runtime state.

**Failure / propagation:** Removing path changes workspace membership; effect on commands is unmeasured.
**Containment:** Stops at member declaration.
**Validation:** Static line inspection only; no Cargo metadata.
**Coordination / evidence:** Root manifest; workspace owner unverified. [Relation index](#b03)

<a id="rel-002"></a>
### REL-002 — Network-partition target imports library
**Identity:** `repo:1232040291:boundary:chaos-campaign-test-network-partition`.
**Dependency / flow / impact:** Test consumer→library / compile-only / library API change can affect target compile/assertions.
**Surface:** `campaign_network_partition_failover.rs` imports `assert_alert_fired`, `assert_audit_emitted_once`, `CampaignFailoverModel`, `CampaignRegion`, `RouteOutcome`.
**Activation:** `Cargo.toml:21–23`; file cfg gate; resolved feature unknown.
**Contract:** Local model calls/assertions; no router binding.
**State / effect:** New fixture state per test; source-defined only.

**Failure / propagation:** API/assertion mismatch can fail selected test; none run.
**Containment:** One declared target.
**Validation:** Source import and target path inspected; NOT_EXECUTED.
**Coordination / evidence:** `tests/chaos/tests/campaign_network_partition_failover.rs`; local owner unknown. [Relation index](#b03)

<a id="rel-003"></a>
### REL-003 — D1 target imports library
**Identity:** `repo:1232040291:boundary:chaos-campaign-test-d1-pool`.
**Dependency / flow / impact:** Test consumer→library / compile-only / API change can affect local pool assertions.
**Surface:** `campaign_d1_pool_exhaustion_degrades_gracefully.rs` imports `assert_alert_fired`, `assert_audit_emitted_once`, `CampaignD1Pool`, `D1AcquireOutcome`.
**Activation:** `Cargo.toml:25–27`; file cfg gate; selection unknown.
**Contract:** Counter/map fixture only; no database pool call.
**State / effect:** Per-test in-memory pool model.

**Failure / propagation:** Compile/assert failure stays in this target; none observed.
**Containment:** One target; no runtime consumer established.
**Validation:** Source path/import inspected; NOT_EXECUTED.
**Coordination / evidence:** matching source file; local owner unknown. [Relation index](#b03)

<a id="rel-004"></a>
### REL-004 — Dual-write target imports library
**Identity:** `repo:1232040291:boundary:chaos-campaign-test-audit-dual-write`.
**Dependency / flow / impact:** Test consumer→library / compile-only / API change can affect local vector assertions.
**Surface:** `campaign_neon_shadow_silent_failure_alerts.rs` imports `assert_alert_fired`, `assert_audit_emitted_once`, `CampaignAuditDualWrite`, `SinkPersistResult`.
**Activation:** `Cargo.toml:29–31`; file cfg gate; selection unknown.
**Contract:** Two in-memory vectors; not R2/Neon clients.
**State / effect:** Test-local fixture state.

**Failure / propagation:** API/assertion mismatch in target; no sink effect.
**Containment:** One target.
**Validation:** Source path/import inspected; NOT_EXECUTED.
**Coordination / evidence:** matching source file; local owner unknown. [Relation index](#b03)

<a id="rel-005"></a>
### REL-005 — RLS target imports library
**Identity:** `repo:1232040291:boundary:chaos-campaign-test-rls`.
**Dependency / flow / impact:** Test consumer→library / compile-only / API change can affect local tenant assertions.
**Surface:** `campaign_rls_guc_dropout_rejects_insert.rs` imports `assert_alert_fired`, `assert_audit_emitted_once`, `CampaignRlsTable`, `RlsInsertOutcome`.
**Activation:** `Cargo.toml:33–35`; file cfg gate; selection unknown.
**Contract:** Local option/map checks only; no SQL session.
**State / effect:** Test-local rows and vectors.

**Failure / propagation:** API/assertion mismatch in target; no DB effect.
**Containment:** One target.
**Validation:** Source path/import inspected; NOT_EXECUTED.
**Coordination / evidence:** matching source file; local owner unknown. [Relation index](#b03)

<a id="rel-006"></a>
### REL-006 — BYOK target imports library
**Identity:** `repo:1232040291:boundary:chaos-campaign-test-byok`.
**Dependency / flow / impact:** Test consumer→library / compile-only / API change can affect local map assertions.
**Surface:** `campaign_byok_provider_503_fails_closed.rs` imports `assert_alert_fired`, `assert_audit_emitted_once`, `CampaignByokModel`, `ByokProvider`, `ByokOutcome`.
**Activation:** `Cargo.toml:37–39`; file cfg gate; selection unknown.
**Contract:** Availability map only; no key/provider call.
**State / effect:** Test-local map/vectors.

**Failure / propagation:** API/assertion mismatch in target; no key effect.
**Containment:** One target.
**Validation:** Source path/import inspected; NOT_EXECUTED.
**Coordination / evidence:** matching source file; local owner unknown. [Relation index](#b03)

<a id="rel-007"></a>
### REL-007 — Webhook target imports library
**Identity:** `repo:1232040291:boundary:chaos-campaign-test-webhook`.
**Dependency / flow / impact:** Test consumer→library / compile-only / API change can affect local timestamp assertions.
**Surface:** `campaign_stripe_webhook_timestamp_drift_rejected.rs` imports `assert_alert_fired`, `assert_audit_emitted_once`, `CampaignWebhookVerifier`, `WebhookOutcome`.
**Activation:** `Cargo.toml:41–43`; file cfg gate; selection unknown.
**Contract:** Caller bool/timestamps; no signature or webhook handler.
**State / effect:** Per-test scalar inputs and vectors.

**Failure / propagation:** Assertion result local; integer extremes are not evidenced by tests.
**Containment:** One target.
**Validation:** Source path/import inspected; NOT_EXECUTED.
**Coordination / evidence:** matching source file; local owner unknown. [Relation index](#b03)

<a id="rel-008"></a>
### REL-008 — JWKS target imports library
**Identity:** `repo:1232040291:boundary:chaos-campaign-test-jwks`.
**Dependency / flow / impact:** Test consumer→library / compile-only / API change can affect local string assertions.
**Surface:** `campaign_clerk_jwks_rotation_recovers.rs` imports `assert_alert_fired`, `assert_audit_emitted_once`, `CampaignClerkJwks`, `JwksOutcome`.
**Activation:** `Cargo.toml:45–47`; file cfg gate; selection unknown.
**Contract:** String equality and local cache update; no JWKS request.
**State / effect:** Test-local key strings/vectors.

**Failure / propagation:** Assertion mismatch local; no auth request.
**Containment:** One target.
**Validation:** Source path/import inspected; NOT_EXECUTED.
**Coordination / evidence:** matching source file; local owner unknown. [Relation index](#b03)

<a id="rel-009"></a>
### REL-009 — Multipart target imports library
**Identity:** `repo:1232040291:boundary:chaos-campaign-test-multipart`.
**Dependency / flow / impact:** Test consumer→library / compile-only / API change can affect local state/queue assertions.
**Surface:** `campaign_cas_multipart_abort_cleanup.rs` imports `assert_alert_fired`, `assert_audit_emitted_once`, `CampaignMultipart`, `MultipartState`.
**Activation:** `Cargo.toml:49–51`; file cfg gate; selection unknown.
**Contract:** Local state machine and queue; no CAS/GCS call.
**State / effect:** Test-local staged part numbers.

**Failure / propagation:** Assertion mismatch local; no object cleanup.
**Containment:** One target.
**Validation:** Source path/import inspected; NOT_EXECUTED.
**Coordination / evidence:** matching source file; local owner unknown. [Relation index](#b03)

<a id="rel-010"></a>
### REL-010 — Combined partition/BYOK target imports library
**Identity:** `repo:1232040291:boundary:chaos-campaign-test-combined-failover-byok`.
**Dependency / flow / impact:** Test consumer→library / compile-only / either API change can affect sequential fixture assertions.
**Surface:** `campaign_combined_partition_plus_byok_503.rs` imports both assertion helpers, `ByokOutcome`, `ByokProvider`, `CampaignByokModel`, `CampaignFailoverModel`, `CampaignRegion`, `RouteOutcome`.
**Activation:** `Cargo.toml:56–58`; file cfg gate; selection unknown.
**Contract:** Two locally composed models; calls are sequential.
**State / effect:** Independent fixture maps and vectors.

**Failure / propagation:** Target assertion/compile only; no shared router/provider.
**Containment:** One target; not concurrency evidence.
**Validation:** Source path/import inspected; NOT_EXECUTED.
**Coordination / evidence:** matching source file; local owner unknown. [Relation index](#b03)

<a id="rel-011"></a>
### REL-011 — Combined D1/webhook target imports library
**Identity:** `repo:1232040291:boundary:chaos-campaign-test-combined-d1-webhook`.
**Dependency / flow / impact:** Test consumer→library / compile-only / API change can affect two fixture assertions.
**Surface:** `campaign_combined_d1_exhaustion_plus_stripe_drift.rs` imports both assertion helpers, `CampaignD1Pool`, `CampaignWebhookVerifier`, `D1AcquireOutcome`, `WebhookOutcome`.
**Activation:** `Cargo.toml:60–62`; file cfg gate; selection unknown.
**Contract:** Sequential local calls with supplied values; no DB or handler.
**State / effect:** Independent model state.

**Failure / propagation:** Compile/assert only; time/input ordering is fixture-defined.
**Containment:** One target.
**Validation:** Source path/import inspected; NOT_EXECUTED.
**Coordination / evidence:** matching source file; local owner unknown. [Relation index](#b03)

<a id="rel-012"></a>
### REL-012 — Combined audit/export target imports library
**Identity:** `repo:1232040291:boundary:chaos-campaign-test-combined-audit-export`.
**Dependency / flow / impact:** Test consumer→library / compile-only / model API changes can affect fixture/export assertions.
**Surface:** `campaign_combined_neon_shadow_failure_plus_audit_export.rs` imports both assertion helpers, `CampaignAuditDualWrite`, `SinkPersistResult`; private `ExportTrailerModel` is test-local.
**Activation:** `Cargo.toml:64–66`; file cfg gate; selection unknown.
**Contract:** Test copies local rows to private fixture; no export endpoint.
**State / effect:** Local vectors and boolean only.

**Failure / propagation:** Compile/assert in one target; no audit exporter call.
**Containment:** Fixture does not leave test process.
**Validation:** Source path/import inspected; NOT_EXECUTED.
**Coordination / evidence:** matching source file; local owner unknown. [Relation index](#b03)

<a id="b04"></a>
## B04 — Transitive propagation census

| Destination | Witness / RELs | Condition | Effect / containment | Validation status |
|---|---|---|---|---|
| Package test targets (internal) | `chaos-campaign` library → one of REL-002–REL-012 | Only if target and `chaos` feature are selected; unresolved | Source/API edits may alter local test compilation/assertions; containment is the target process | 11 imports statically mapped; no test executed |
| Other workspace packages via Cargo | No exact inverse manifest declaration; no declared package edge | No statically declared path found | No declared transitive internal Cargo path established; resolved global graph remains unknown | Manifest text search only; Cargo resolution not run |
| Production analogues named by model docs | Comments only; no Rust import or manifest edge | No activation/wiring established | No causal path from this model to production established | Static source only; current operation unknown |

The declared dependency graph has zero incoming or outgoing exact-name package dependency edges in inspected manifests, so its declared internal transitive path count is zero. This is not a result for external registry/git resolution, dynamic/generated use, another repository, runtime, or CI.

<a id="b05"></a>
## B05 — Change impact and validation

| Change | Affected IDs | Consumer/effect | Validation and coordination |
|---|---|---|---|
| Root workspace membership or package identity | REL-001; R01 | Changes workspace inventory or package key, not proven build invocation. | Reconcile root + package manifests; owner unknown; see PROC-001/002. |
| Add/remove dependency declaration | R01; direct/inverse census; no existing dependency REL | Changes declared package graph and all relevant features/targets. | Enumerate normal/build/dev/target sections and inverse matches; resolve only under authorized scope; see PROC-001. |
| Change feature or target declaration | R01, REL-002–012, A2 | Alters possible target compilation surface; selection unknown. | Reconcile exact target rows and all file cfg gates; source-level check then optional local test procedure. |
| Change model outcome, state, event string, or failure arithmetic | matching `API`/`INV` in REFERENCE; relevant REL(s) | Could change fixture assertions; vectors remain local. | Inspect source test assertions; include signed integer extremes for webhook contract review; coordinate independent reviewer. |
| Change combined test composition/private export fixture | REL-010/011/012 | Affects sequential test-local model interactions. | Keep each imported model and private fixture explicit; do not infer concurrency or integration. |

<a id="b06"></a>
## B06 — Coverage, exclusions, and unknowns

| Population | Discovered | Documented | Excluded with reason | Unknown |
|---|---:|---:|---:|---:|
| Package direct dependency tables | 0 | 0 | 0 | 0 |
| Inverse exact-name manifest deps | 0 | 0 | 0 | 1 resolved external/global graph unmeasured |
| Root workspace membership | 1 | 1 (`REL-001`) | 0 | 1 command/target use unmeasured |
| Internal Rust imports | 11 | 11 (`REL-002–012`) | 0 | 1 generated/dynamic population unmeasured |
| External Rust source matches | 0 | 0 | 0 | 1 other-repo/runtime population unmeasured |
| Runbook mentions | 1 | 0 | 1 documentary-only | 0 |
| Ownership/census mentions | 4 | 0 | 4 documentary-only | 0 |
| Sealed audit mentions | 16 | 0 | 16 historical-only | 0 |
| Other tracked lexical records | 5 (`LEX-001–005`) | 5 | 5 inventory/documentary-only; no consumer relation | 1 current SBOM freshness / lock-resolution state unknown |

**Explicit lexical exclusions:** `tests/e2e-chaos/Cargo.toml` is distinct package `e2e-chaos` with 12 named test targets; none is a `chaos-campaign` consumer. `crates/corelink-r2-multipart/Cargo.toml` is package `corelink-r2-multipart` with a separate `chaos_r2_multipart` target. Other unrelated chaos-named path matches are `corelink-gc/tests/chaos_gc_scheduler.rs`, `corelink-signup/tests/chaos_stripe_outage.rs`, and `corelink-privacy-erasure-worker/tests/chaos_per_backend_failure.rs`. These are not edges to the owned package.

**LEX-001–005 — tracked lexical records, excluded from consumer edges:**

| ID | File and observed record | Classification / exclusion |
|---|---|---|
| LEX-001 | `.sbom/cyclonedx-rust.json:1721,1727,1734`; three occurrences in one SBOM file. | Generated CycloneDX package inventory record; no call/manifest edge. Generation freshness unknown. |
| LEX-002 | `docs/handoff/2026-06-28-enterprise-dd-RESPONSE.md:38`; one test-inventory mention. | Handoff narrative, not a source consumer or execution record. |
| LEX-003 | `docs/release/v1.0.0-GA-tag-draft.txt:58–59`; draft names sealed audit artifacts. | Release-draft/history text, not source use or current execution. |
| LEX-004 | `specs/_audits/2026-05-27-specs-inventory-cleanup-map.md:358–359`; entries for two sealed campaign audits. | Historical index record, not a consumer or evidence those scenarios ran now. |
| LEX-005 | `Cargo.lock:1029`; package stanza has no dependency list. | Lockfile package node/inventory, not an inverse dependency declaration; resolution/use is unmeasured. |

16 sealed audit files match the B02 literal search. They are documentary/historical text, not current execution proof.

| Sealed audit path | Sealed audit path |
|---|---|
| `2026-05-14-adversarial-summary-s12.md` | `2026-05-14-region-outage-chaos-s14.md` |
| `2026-05-14-s19-sprint-preflight-review.md` | `2026-05-16-chaos-campaign-harness.md` |
| `2026-05-16-chaos-combined-failures.md` | `2026-05-16-final-cutover-readiness.md` |
| `2026-05-16-ga-final-checklist.md` | `2026-05-16-ga-readiness-final.md` |
| `2026-05-16-pre-cutover-state-snapshot.md` | `2026-05-16-pre-ga-security-attestation.md` |
| `2026-05-16-prod-deploy-dressrun.md` | `2026-05-16-wave22-adversarial-review.md` |
| `2026-05-16-wave22-closure.md` | `2026-05-16-wave23-adversarial-review.md` |
| `2026-05-16-wave23-closure.md` | `stride-per-crate/STRIDE-corelink-privacy-consent-ledger.md` |

Unknown: feature/target resolution; current execution/build/CI; external dependency resolution; generated/other-repository consumers; production model compatibility; all provider/database/storage/network/alert/audit activity; deployed state; runtime effects; signed integer overflow configuration; responsible implementation/operator/reviewer. The census equality is bounded to its named populations and does not prove complete system discovery.

[Reference](REFERENCE.md#r01) · [Maintenance](MAINTENANCE.md#m01) · [Start](#b01)
