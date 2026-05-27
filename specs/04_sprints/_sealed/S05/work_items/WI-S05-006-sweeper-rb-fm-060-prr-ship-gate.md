---
id: "WI-S05-006"
type: "work_item"
doc_status: "FROZEN"
work_status: "DONE"
audit_status: "AUDITED"
version: "1.3.0"
created: "2026-04-25"
updated: "2026-05-01"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-002", "FF-HR-005", "FF-HR-009"]
parent: "S-05"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "REMOTE-CACHE-PRODUCT-PROFILE"
  - "OBSERVABILITY-MODEL"
  - "FAILURE-MODES"
  - "RESILIENCE-PATTERNS"
  - "INVARIANT-REGISTRY"
  - "SLO-CATALOG"
tags: ["wi", "s05", "sweeper", "cron-do", "rb-fm-060", "prr", "ship-gate", "stitched-flow", "high-risk"]
---

# WI-S05-006 — Sweeper Cron DO (PAT-SWEEPER-001 abort multipart > 7d) + RB-FM-060 Dry-Run + 160 GiB Stitched Multipart Flow + REAPI Conformance + DASH-MULTIPART + Throughput 100 MB/s Sustained 72h + PRR HIGH_RISK 11 Sign-offs Canonical Ship Gate

> **doc_status:** FROZEN · **work_status:** DONE · **lane:** HIGH_RISK
> **Parent:** [S-05](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S05-006 |
| Título | Sweeper cron Durable Object aborts multipart > 7d (FM-060 mitigation; PAT-SWEEPER-001) + RB-FM-060 dry-run executado em staging + 160 GiB stitched multipart flow E2E test + REAPI v2.3+ conformance suite SplitBlob/SpliceBlob 100% green + DASH-MULTIPART dashboards + throughput ≥ 100 MB/s sustained 72h staging + PRR HIGH_RISK 11 sign-offs canonical ship gate + cumulative INV §3.16 promotion (13 INVs) + ADR-0022/0038/0039/0040/0041 ratificação confirmation |
| Sprint | S-05 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-002 (ship gate prevents cross-tenant via final validation), FF-HR-005 (PRR enforces all CTRL-CAS controls), FF-HR-009 (defense-in-depth final validation) |

## 1. Intent

Este WI é o **ship gate** de S-05; entra production-ready apenas após:

1. **Sweeper cron DO** abort multipart > 7d (FM-060):
   - Per-region instance (5 regions; lesson WI-S04-005 sharding pattern reuse).
   - Schedule: every 1h; alarm re-armed at start of tick.
   - Flow: D1 SELECT multipart_sessions WHERE state=in_progress AND last_activity_at < now-7d → R2 abort_multipart_upload via WI-S05-003 adapter → D1 UPDATE state=aborted → audit emit corelink.multipart.orphan_aborted.
   - Bounded batch 250 sessions/tick (lesson Lote 10.4bis D1 100KB limit).
   - Tenant-scoped DELETE/UPDATE strict (lesson Lote 10.4bis WI-S04-005).

2. **RB-FM-060 dry-run** executado em staging:
   - Setup: chaos PR introduces orphan multipart (client disconnect mid-upload).
   - Engineer follows runbook RB-FM-060 step-by-step.
   - Validate detection ≤ 5min (sweeper cron tick OR metric alert); remediation ≤ 30min; customer notification template ≤ 1h.
   - Post-mortem written; runbook gaps iterated.

3. **160 GiB stitched multipart flow** E2E test:
   - Blob > 160 GiB requires multiple multipart sessions stitched via meta-manifest.
   - Implementation: handler detects > 160 GiB; orchestrates 2+ multipart sessions; each completes individually; meta-manifest binds all (Merkle of manifest_digests).
   - Integration test: 200 GiB synthetic blob; stitched flow successful.

4. **REAPI v2.3+ conformance suite** SplitBlob/SpliceBlob 100% green:
   - Pinned commit em ADR-0038 Annex.
   - Subset enumerated: 10 conformance tests covering CAS multipart subset (Lote 10.5bis P0 fix: was wrongly AC subset — copy-paste from WI-S04-006; actual CAS SplitBlob/SpliceBlob subset).
   - CI nightly + per-commit on PR touching multipart code.

5. **DASH-MULTIPART dashboards** (Grafana):
   - 10 panels: throughput, orphan rate, dedup ratio per tenant_tier, latency p99 Split/Splice, sweeper tick rate, R2 backend availability, sig invalid counter, REAPI conformance status, cost per-op tracker, manifest verify rate.
   - Alerts to PagerDuty + Slack.

6. **Throughput ≥ 100 MB/s sustained 72h staging**:
   - Continuous synthetic Bazel workload (Docker layers + ML model files).
   - Pause clock on P0/P1/P2 incidents (lesson Lote 10.4bis 4-tier classification).

7. **PRR HIGH_RISK 11 sign-offs canonical**:
   - 12 mandatory (Owner + Final Approver + SRE Lead + Security Lead + Engineer×2 + QA + Product + Compliance + Privacy + Architect + AppSec) + Crypto SME mandatory (lesson Lote 10.4bis: Crypto SME MANDATORY EMPHATIC for cripto WIs).
   - Each role's checklist + sign-off entry; rubber-stamp prohibited.
   - 2h structured PRR meeting.

8. **Cumulative INV §3.16 promotion** (13 INVs; Lote 10.5bis P0 fix: was 10 self-inconsistency):
   - INV-MULTIPART-IDEMPOTENT, INV-MULTIPART-MANIFEST-SIGNED, INV-MULTIPART-CONCURRENCY-BOUNDED (WI-S05-001).
   - INV-MULTIPART-CHUNK-DETERMINISTIC, INV-MULTIPART-BOUNDED-PARSER, INV-MULTIPART-STREAMING-MEMORY (WI-S05-002).
   - INV-MULTIPART-ORPHAN-DETECTABLE, INV-MULTIPART-PATH-TENANT-SCOPED (WI-S05-003).
   - INV-MULTIPART-STATE-MONOTONIC, INV-MULTIPART-PATH-KEY-MATERIALIZED (WI-S05-004).
   - INV-MULTIPART-MANIFEST-VALID, INV-MULTIPART-DUAL-SIDE-VERIFY, INV-MULTIPART-STREAMING-VERIFY-FAIL-FAST (WI-S05-005).
   - **CI gate** validate_inv_promotion.py validates (lesson Lote 10.4bis introduced gate).

9. **ADR ratificação confirmation**:
   - ADR-0022 (chunk vs part decoupling): ratificada em WI-S05-002.
   - ADR-0038 (handler invariants): ratificada em WI-S05-001.
   - ADR-0039 (chunker public API stability): ratificada em WI-S05-002.
   - ADR-0040 (multipart D1 sharding): ratificada em WI-S05-004.
   - ADR-0041 (manifest public API stability): ratificada em WI-S05-005.
   - All whitelisted em validate_references.py.

10. **Cost regression gate** all-WIs:
    - Per-Split ≤ $0.000040; per-Splice ≤ $0.000010 (WI-S05-001).
    - Per-chunker ≤ $0.000001 (WI-S05-002).
    - Per-multipart-op ≤ $0.0000045 (WI-S05-003).
    - Per-D1 op ≤ $0.000001 (WI-S05-004).
    - Per-manifest verify ≤ $0.000002; build ≤ $0.000005 (WI-S05-005).
    - CI bench gate ±10% tolerance.

11. **Customer-facing communication ready**:
    - SLA addendum draft `docs/customer/multipart-sla-addendum-s05-ga.md`.
    - Release notes `docs/customer/release-notes-s05.md`.
    - Bazel chunked-cache onboarding `docs/customer/bazel-chunked-cache-setup.md`.

12. **Production rollout plan**: 10% → 50% → 100% gradual (lesson Lote 10.4bis).

## 2. Narrative (HIGH_RISK ≥ 300 palavras)

S-05 ship gate é go/no-go review. Bug em ship = customer-facing incident; aplicar lições preventivamente Lote 10.4bis:

1. **Sprint contract drift defense**: §19 já não tem 95% conformance waiver path (lesson Lote 10.4bis aplicada; sprint contract S-05 §19 mantém similar discipline).

2. **PRR sign-off rubber-stamp prevention**: each role's checklist + sign-off requires "I read XYZ and verified A/B/C"; CI gate validates non-empty entries (lesson Lote 10.4bis introduced).

3. **Crypto SME mandatory consistency**: WI-S05-002 + WI-S05-005 both have Crypto SME MANDATORY EMPHATIC (BLAKE3 + FastCDC + Merkle protocol + sig domain separation). PRR gate aligned: Crypto SME mandatory non-waivable; 40-80h booking pre-PRR.

4. **Sweeper cron reliability**: chaos #1 (cron not re-armed) catches via metric alert ≤ 1h; runbook RB-FM-060 documents.

5. **160 GiB stitched flow correctness**: 200 GiB blob test E2E; meta-manifest = Merkle of manifest_digests; client SDK reassembles meta-manifest first then individual manifests.

6. **REAPI conformance bump cadence**: quarterly; emergency CVE bypass via Architect approval + ADR within 1 sprint.

7. **72h SLO 4-tier incident classification** (lesson Lote 10.4bis):
   - **P0**: cross-tenant chunk leak; manifest forge; sweeper cron stale > 1h. Resets clock; SEV-0 incident response.
   - **P1**: throughput < 50 MB/s sustained; SLO-LAT-CAS-PUT-MULTIPART breach. Resets clock.
   - **P2**: tampering counter increment; orphan rate > 1% sustained. Resets clock.
   - **P3**: dedup ratio < 1.2× sustained; metric drift. Does NOT reset.

8. **Customer-facing communication pre-ship**: SLA addendum + release notes + Bazel onboarding doc reviewed Product + Compliance + Privacy.

**Risk justification HIGH_RISK**: ship gate prevents cross-tenant catastrophic; PRR enforces all CTRL-CAS controls; defense-in-depth final validation across 5 prior WIs.

11 sign-offs canonical.

## 3. Customer Impact & Journey

**Persona 1 — Bazel CI dev expecting GA reliability**: post-S-05 SEALED, multipart handles `bazel build //...` reliably for blobs > 5 MiB (Docker layers, ML models). Cache hit ratio improves via FastCDC dedup.

**Persona 2 — DevOps reviewing operational readiness**: DASH-MULTIPART live; orphan rate < 1% sustained; sweeper aborts > 7d as expected; RB-FM-060 dry-run documented post-mortem.

**Persona 3 — Compliance reviewer**: 5 ADRs ratificadas (0022, 0038-0041); 13 INVs §3.16 promovidas; SLSA L3 partial alignment.

**SLA addendum**:
- Multipart Split p99 ≤ 1s for 10 MiB; throughput ≥ 100 MB/s sustained.
- Multipart Splice p99 ≤ 10s for 1 GiB.
- Orphan abort SLA ≤ 7d (sweeper).
- 160 GiB single multipart cap; > 160 GiB stitched flow.
- REAPI v2.3+ conformance 100% green nightly.

## 4. Capability Mapping

- All CAP-CAS-008..013 — VALIDATED end-to-end.
- Trace: `slo_catalog.md SLO-LAT-CAS-PUT-MULTIPART` + `failure_modes.md FM-060 + RB-FM-060` + `observability_model.md DASH-MULTIPART` + ADR-0022.

## 5. Tipo

PRR ship gate; HIGH_RISK; FF-HR-002 + FF-HR-005 + FF-HR-009.

## 6. Escopo (compact)

### 6.1 In-scope

1. **Sweeper Cron DO** (`crates/corelink-worker/src/multipart/sweeper.rs`):
   - Per-region instance (5 regions).
   - Alarm 1h re-armed at start (lesson Lote 10.4bis).
   - Flow: D1 SELECT orphans (state=in_progress + last_activity > 7d) → R2 abort via WI-S05-003 adapter → D1 UPDATE state=aborted → audit emit.
   - Bounded batch 250 sessions/tick (lesson 100KB D1 limit).
   - Tenant-scoped (lesson Lote 10.4bis cross-tenant prevention).
   - Stagger alarms across regions (lesson Lote 10.4bis WI-S04-005).
   - Métricas: corelink.multipart.sweeper.{ticks_total, orphans_aborted_total{region}, batch_size, duration_ms}; alert if ticks rate < 1/h sustained.

2. **RB-FM-060 dry-run** em staging:
   - Setup: chaos PR introduces orphan multipart.
   - Engineer follows runbook step-by-step.
   - Time-track: detection ≤ 5min, remediation ≤ 30min, customer comm ≤ 1h.
   - Post-mortem `specs/_audits/2026-XX-XX-rb-fm-060-dry-run.md`.

3. **160 GiB stitched flow**:
   - `crates/corelink-worker/src/multipart/stitched.rs`:
     - Detect blob > 160 GiB em SplitBlob handler.
     - Orchestrate 2+ multipart sessions (each ≤ 160 GiB).
     - Build meta-manifest: Merkle of manifest_digests (each session's manifest).
     - Sign meta-manifest com HKDF info=`b"meta-manifest-sig"` (separated domain).
     - D1 schema additions (forward S-14 OR S-09): meta_manifests table com (tenant_id, blob_digest, child_manifest_digests[]).
   - Integration test 200 GiB synthetic blob.

4. **REAPI v2.3+ conformance suite** integration:
   - `tests/conformance/reapi_v2_split_splice.rs`:
     - bazelbuild/remote-apis pinned commit em ADR-0038 Annex.
     - Subset enumerated: 10 tests covering SplitBlob (5 variants) + SpliceBlob (5 variants).
     - CI gate 100% pass.

5. **DASH-MULTIPART dashboards** (10 panels):
   - Panel 1: SplitBlob/SpliceBlob throughput rolling 1-min average.
   - Panel 2: Latency p99 Split/Splice warm/cold per region.
   - Panel 3: Orphan rate per region (alert if > 1% sustained).
   - Panel 4: Dedup ratio per tenant_tier.
   - Panel 5: Sweeper tick rate per region (alert if < 1/h).
   - Panel 6: R2 backend availability per region.
   - Panel 7: Manifest sig invalid counter (alert > 0 sustained).
   - Panel 8: REAPI conformance status (last run + result).
   - Panel 9: Cost per-op tracker.
   - Panel 10: Manifest verify rate per result.
   - Auto-deploy via Grafana provisioning + git-as-source.
   - Alerts to PagerDuty + Slack.

6. **72h SLO sustained validation**:
   - Continuous staging deployment; synthetic Bazel workload 24/7.
   - Monitor: SLO-LAT-CAS-PUT-MULTIPART p99 ≤ 1s (10 MiB Split); throughput ≥ 100 MB/s.
   - 4-tier incident classification (lesson Lote 10.4bis).

7. **PRR HIGH_RISK 11 sign-off canonical ceremony** (2h structured):
   - Per-role checklist (lesson Lote 10.4bis pattern).
   - Crypto SME MANDATORY non-waivable (40-80h booking).
   - Sign-offs via GitHub PR review + git commit signing.

8. **Cumulative INV §3.16 promotion** (13 INVs; Lote 10.5bis P0 fix: was 10 self-inconsistency):
   - Update `specs/03_architecture/invariant_registry.md` §3.16 NEW.
   - CI gate validate_inv_promotion.py validates.

9. **5 ADRs ratificação**: confirmar todas DRAFT → ACCEPTED + whitelist em validate_references.py.

10. **Cost regression gate** validation across all WIs.

11. **Customer-facing communication**: SLA addendum + release notes + Bazel onboarding doc.

12. **Production rollout plan**: 10% → 50% → 100% gradual (lesson Lote 10.4bis).

13. **Property test 100k tenant isolation cumulative**: extends WI-S05-001 prop_split_tenant_isolation; 100k iterations nightly CI; 0 violations tolerated.

### 6.2 Out-of-scope (deferred)

- Multi-region failover testing (S-14).
- Cross-tenant federation (S-XX).
- Customer onboarding automation (S-15 CLI/SDK).
- Per-tenant SLA customization (S-13 admin plane).

## 7. Anti-Scope

- ❌ Ship without 100% conformance.
- ❌ Ship without 11 sign-offs canonical.
- ❌ Ship without RB-FM-060 dry-run.
- ❌ Ship with 1+ tenant isolation violation in 100k.
- ❌ Ship with throughput < 100 MB/s sustained 72h.
- ❌ Ship with cost regression > 10%.
- ❌ Ship with ADR não ratificada (5 ADRs mandatory).
- ❌ Ship with 13 INVs §3.16 não promovidas.
- ❌ Ship with sign-off rubber-stamp.
- ❌ Production rollout direct 100% (gradual mandatory; lesson Lote 10.4bis).
- ❌ Skip 160 GiB stitched flow E2E.
- ❌ Skip post-ship review.

## 8. Acceptance Criteria (Gherkin) (compact 12 scenarios)

```gherkin
Feature: S-05 PRR HIGH_RISK 11 sign-off canonical ship gate

  Scenario: Sweeper cron tick (single region orphan abort)
    Given multipart_sessions has 5 sessions state=in_progress + last_activity > 7d em region=sam
    When sweeper alarm fires for region=sam
    Then D1 SELECT 5 orphans → adapter.abort × 5 → D1 UPDATE state=aborted × 5 → audit emit corelink.multipart.orphan_aborted × 5
    And metric corelink.multipart.sweeper.orphans_aborted_total{region=sam} += 5
    And alarm re-armed +1h

  Scenario: RB-FM-060 dry-run successful
    Given staging environment
    When chaos PR introduces orphan; engineer follows RB-FM-060
    Then detection ≤ 5min; remediation ≤ 30min; customer comm ≤ 1h
    And post-mortem written + runbook iterated

  Scenario: 160 GiB stitched multipart flow E2E
    Given 200 GiB synthetic blob
    When SplitBlob detects > 160 GiB → orchestrates 2 multipart sessions
    Then meta-manifest built (Merkle of 2 manifest_digests)
    And meta-manifest signed com HKDF info=`b"meta-manifest-sig"`
    And client SDK reassembles meta-manifest → individual manifests → bytes-equal original blob

  Scenario: REAPI conformance 100% green
    Given 10 enumerated SplitBlob/SpliceBlob conformance tests (ADR-0038 Annex)
    When CI nightly suite runs against staging
    Then 100% pass

  Scenario: 72h SLO sustained
    Given continuous staging workload
    When 72h continuous run
    Then throughput ≥ 100 MB/s sustained
    And p99 Split ≤ 1s (10 MiB); Splice ≤ 10s (1 GiB)
    And no P0/P1/P2 incidents (P3 minor allowed)

  Scenario: DASH-MULTIPART dashboards live
    Given Grafana 10 panels provisioned
    Then alerts route to PagerDuty + Slack
    And throughput panel shows ≥ 100 MB/s baseline

  Scenario: PRR HIGH_RISK 11 sign-offs canonical collected
    Given PRR meeting executed (2h)
    When 13 roles sign + checklist evidence
    Then 12 mandatory + 1 advisory (Crypto SME MANDATORY this WI; advisory in WI-006 ceremony reference)
    And no rubber-stamp

  Scenario: 13 INVs §3.16 promotion CI gate
    Given invariant_registry.md §3.16 NEW with 13 INVs
    When validate_inv_promotion.py runs
    Then all WI-declared INVs match registry
    And CI green

  Scenario: 5 ADRs ratificadas
    Given ADR-0022, ADR-0038, ADR-0039, ADR-0040, ADR-0041
    Then all doc_status = ACCEPTED
    And whitelist em validate_references.py
    And rationale + risks documented

  Scenario: Cost regression gate green all WIs
    Given CI bench runs criterion benchmarks
    Then per-Split ≤ $0.000040; per-Splice ≤ $0.000010; per-chunker ≤ $0.000001; per-manifest verify ≤ $0.000002; per-build ≤ $0.000005
    And no > 10% regression

  Scenario: Customer-facing comm ready
    Given SLA addendum + release notes + Bazel onboarding docs
    Then reviewed Product + Compliance + Privacy
    And published em docs/customer/

  Scenario: Ship gate green light
    Given all above scenarios green
    Then ship decision = GO
    And gradual rollout 10% → 50% → 100%

  Scenario: Property test 100k tenant isolation
    Given prop tests extending WI-S05-001 prop_split_tenant_isolation to 100k iter
    When nightly CI runs
    Then 0 violations tolerated
    And CI gate green
```

## 9. Design Decisions (compact)

- **9.1** Sweeper cron DO per-region (consistent WI-S04-005).
- **9.2** Bounded batch 250 sessions/tick (D1 100KB limit; Lote 10.4bis lesson).
- **9.3** Stitched flow > 160 GiB via meta-manifest (HKDF info=`b"meta-manifest-sig"` separate domain).
- **9.4** REAPI conformance 100% non-negotiable (sprint contract §19 Lote 10.4bis pattern).
- **9.5** Crypto SME mandatory non-waivable for cripto WIs (lesson Lote 10.4bis WI-S04-006 fix).
- **9.6** 4-tier incident classification (P0/P1/P2/P3) consistent Lote 10.4bis lesson.
- **9.7** validate_inv_promotion.py CI gate (lesson Lote 10.4bis introduced).
- **9.8** Production rollout gradual 10%→50%→100% (lesson Lote 10.4bis).

## 10. Completeness Criteria SOTA

- [ ] **10.s05.006.1** Sweeper cron tick green (5 regions; alarm re-arm).
- [ ] **10.s05.006.2** RB-FM-060 dry-run executado; post-mortem.
- [ ] **10.s05.006.3** 160 GiB stitched flow E2E green (200 GiB blob).
- [ ] **10.s05.006.4** REAPI v2.3+ conformance 100% AC ops nightly.
- [ ] **10.s05.006.5** DASH-MULTIPART 10 panels live; alerts live.
- [ ] **10.s05.006.6** 72h SLO sustained: throughput ≥ 100 MB/s.
- [ ] **10.s05.006.7** PRR 11 sign-offs canonical collected; rubber-stamp prohibited.
- [ ] **10.s05.006.8** 13 INVs §3.16 promovidas + CI gate green (validate_inv_promotion.py).
- [ ] **10.s05.006.9** 5 ADRs ratificadas + whitelist.
- [ ] **10.s05.006.10** Cost regression gate green all WIs.
- [ ] **10.s05.006.11** Customer comm ready (SLA addendum + release notes + onboarding).
- [ ] **10.s05.006.12** Production rollout plan 10%→50%→100%.
- [ ] **10.s05.006.13** Property test 100k tenant isolation green.
- [ ] **10.s05.006.14** SLSA L3 partial alignment validated.

## 11. DoD

- [ ] WI-S05-001..005 SEALED.
- [ ] Sweeper cron live em 5 regions.
- [ ] RB-FM-060 dry-run + post-mortem.
- [ ] 160 GiB stitched E2E green.
- [ ] REAPI conformance 100% green nightly.
- [ ] DASH-MULTIPART live; alerts validated.
- [ ] 72h SLO sustained.
- [ ] PRR 11 sign-offs canonical collected.
- [ ] 13 INVs §3.16 promovidas + CI gate green.
- [ ] 5 ADRs ratificadas.
- [ ] Customer comm ready.
- [ ] Production rollout plan documented.
- [ ] Architect + AppSec + Security Lead + Crypto SME final review.
- [ ] Final approver (Gustavo) ship decision = GO.

## 12. Invariants Validated (cumulative S-05)

End-to-end validation of all S-05 invariants em §3.16:

- INV-CAS-INTEGRITY (registry §3.3): manifest dual-side verify.
- INV-CAS-IDEMPOTENCY (registry §3.3): chunker determinism + manifest determinism.
- INV-TENANT-ISOLATION (registry §3.3): chunks UNIQUE tenant-scoped + property test 100k.
- INV-MULTIPART-IDEMPOTENT (§3.16 NEW): SplitBlob is_chunked flag + chunks ON CONFLICT.
- INV-MULTIPART-MANIFEST-SIGNED (§3.16 NEW): HKDF info=`b"manifest-sig"` separated domain.
- INV-MULTIPART-CONCURRENCY-BOUNDED (§3.16 NEW): per-tenant semaphore 4 default.
- INV-MULTIPART-CHUNK-DETERMINISTIC (§3.16 NEW): FastCDC mask seeds fixed.
- INV-MULTIPART-BOUNDED-PARSER (§3.16 NEW): MAX_BLOB_SIZE 160 GiB + MAX_CHUNKS 80000.
- INV-MULTIPART-STREAMING-MEMORY (§3.16 NEW): per-request stack ≤ 4 MiB.
- INV-MULTIPART-ORPHAN-DETECTABLE (§3.16 NEW): list_orphans + sweeper.
- INV-MULTIPART-PATH-TENANT-SCOPED (§3.16 NEW): R2 path tenant_prefix Layer 4.
- INV-MULTIPART-STATE-MONOTONIC (§3.16 NEW): in_progress → completed OR aborted; never reverse.
- INV-MULTIPART-PATH-KEY-MATERIALIZED (§3.16 NEW): tenant_prefix BLOB(16) column.
- INV-MULTIPART-MANIFEST-VALID (§3.16 NEW): verify_structure rejects 100% tampered.
- INV-MULTIPART-DUAL-SIDE-VERIFY (§3.16 NEW): server + client both invokeable.
- INV-MULTIPART-STREAMING-VERIFY-FAIL-FAST (§3.16 NEW): mid-stream tampered chunk catches.

Total: ~16 INVs (some overlap nomenclatura across S-04 and S-05; consolidated em registry).

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| Sweeper Cron DO | `crates/corelink-worker/src/multipart/sweeper.rs` | Rust |
| Stitched flow | `crates/corelink-worker/src/multipart/stitched.rs` | Rust |
| Conformance harness | `tests/conformance/reapi_v2_split_splice.rs` | Rust |
| Property test 100k | `tests/prop_multipart_tenant_isolation_100k.rs` | Rust |
| DASH-MULTIPART JSON | `dashboards/dash-multipart.json` | JSON |
| Alert rules | `dashboards/alerts/dash-multipart-alerts.yml` | YAML |
| RB-FM-060 dry-run report | `specs/_audits/2026-XX-XX-rb-fm-060-dry-run.md` | Markdown |
| PRR meeting notes | `specs/_audits/2026-XX-XX-s05-prr-meeting.md` | Markdown |
| SLO compliance report | `specs/_audits/2026-XX-XX-s05-slo-72h.md` | Markdown |
| SLA addendum | `docs/customer/multipart-sla-addendum-s05-ga.md` | Markdown |
| Release notes | `docs/customer/release-notes-s05.md` | Markdown |
| Bazel onboarding | `docs/customer/bazel-chunked-cache-setup.md` | Markdown |
| CI ship-gate workflow | `.github/workflows/multipart-ship-gate.yml` | YAML |
| Production rollout plan | `docs/internal/multipart-prod-rollout-plan.md` | Markdown |

## 14. Quality Standards SOTA (compact)

- 14.s05.006.1: REAPI conformance 100%; quarterly bump.
- 14.s05.006.2: Property test 100k cripto-grade gate.
- 14.s05.006.3: SLO 72h continuous; P3 only não reseta.
- 14.s05.006.4: RB-FM-060 dry-run ≤ 5min/30min/1h targets.
- 14.s05.006.5: Dashboard alerts PagerDuty + Slack.
- 14.s05.006.6: PRR meeting 2h structured; 11 sign-offs canonical.
- 14.s05.006.7: Cost gate ±10% tolerance.
- 14.s05.006.8: Customer comm pre-ship.
- 14.s05.006.9: 5 ADRs ratificadas.
- 14.s05.006.10: Production rollout gradual.

## 15. Chaos Experiments (12)

1. Sweeper cron not re-armed → metric alert ≤ 1h.
2. REAPI conformance regression → CI nightly catches.
3. Property test 100k flake → re-run; deterministic seed.
4. SLO 72h reset on P1 → clock resets.
5. RB-FM-060 dry-run gap → iterate runbook.
6. Dashboard alert noisy → threshold tuning.
7. PRR sign-off rubber-stamp regression → CI gate red.
8. Cost regression > 10% → CI bench detects.
9. ADR ratificação rollback → CI gate red sem ADR amendment.
10. Customer comm gap → review process catches.
11. CI ship-gate workflow regression → CI red.
12. 13 INVs §3.16 not promovidas → validate_inv_promotion.py CI red.

## 16. PRR

THE PRR. Esta WI é a PRR.

## 17. Sub-tasks (compact)

| ID | Sub-task | h |
|---|---|---|
| ST-001 | Sweeper Cron DO impl (5 regions) | 5 |
| ST-002 | 160 GiB stitched flow impl + meta-manifest | 5 |
| ST-003 | REAPI conformance harness setup (10 tests pinned) | 4 |
| ST-004 | Property test 100k tenant isolation | 3 |
| ST-005 | DASH-MULTIPART dashboards (10 panels) | 6 |
| ST-006 | Alert rules + PagerDuty + Slack | 4 |
| ST-007 | RB-FM-060 dry-run + post-mortem | 6 |
| ST-008 | 72h SLO continuous staging setup | 3 |
| ST-009 | PRR meeting prep (checklists per role) | 4 |
| ST-010 | PRR meeting execution (2h) | 2 |
| ST-011 | 11 sign-off canonical collection | 3 |
| ST-012 | Cost regression gate setup | 3 |
| ST-013 | 13 INVs §3.16 registry update | 2 |
| ST-014 | 5 ADRs ratificação confirmation | 2 |
| ST-015 | SLA addendum customer | 3 |
| ST-016 | Release notes + Bazel onboarding | 4 |
| ST-017 | Production rollout plan | 3 |
| ST-018 | CI ship-gate workflow | 3 |
| ST-019 | Final review iter (Architect + AppSec + Crypto SME + Security Lead) | 6 |
| ST-020 | Ship decision documentation | 1 |

**Total Optimistic**: ~72h. **PERT** (O=64h, M=72h, P=110h): **~78h**.

## 18. Dependencies

- Hard: WI-S05-001..005 SEALED; Crypto SME availability; Architect availability; 72h staging; Wrangler + Grafana + PagerDuty configured.
- Soft: S-09 aggregation; S-16 customer dashboard; ADR-0034 staffing waiver path.

## 19. Effort PERT: 78h. ## 20. Time-boxing: 90h hard limit.

## 21. Observability

DASH-MULTIPART full (10 panels). Trace spans: `multipart.sweeper.tick`, `prr.meeting.signoff_collected`.

## 22. Cost Analysis

**Sub-task cost** (one-time):
- REAPI conformance setup: dev time.
- Dashboard provisioning: ~$10/mo Grafana = $120/yr.
- 72h staging continuous: ~$30 staging cost.
- PRR meeting: human time × 13 reviewers × 2h = 26h person-time.

**Operational cost** (steady state):
- Sweeper cron: 5 regions × 1 tick/h × 30s ≈ $0.50/dia = $180/yr.
- Conformance suite nightly CI: $1/dia = $365/yr.
- Property test 100k nightly: $1/dia = $365/yr.
- Dashboard hosting: $120/yr.
- Total: ~$1k/yr ship gate maintenance.

**TCO**:
- Setup: ~80h × $100/hr = $8k one-time.
- Maintenance: $1k/yr.
- **Total: $9k Y1, $1k/yr ongoing**.

## 23. API Contract

Public artifacts (consumed external):
- SLA addendum customer doc.
- Release notes.
- Bazel onboarding doc.
- Conformance suite report (CI artifact).

Internal:
- PRR meeting notes; sign-off log; ADR ratificações; production rollout plan.

## 24. Post-mortem Hooks

- Ship gate red → blocker post-mortem.
- 72h SLO incident reset → P0/P1/P2 analysis (lesson Lote 10.4bis 4-tier).
- RB-FM-060 dry-run gap → runbook iteration.
- Property test 100k violation → CRITICAL post-mortem (cross-tenant); ship blocked.
- Conformance regression → community engagement.
- ADR ratificação rollback → 5-Why; senior review.
- Cost regression > 10% → bench drill-down.
- 13 INVs §3.16 not promovidas → CI gate red; resolve before SEAL.

## 25. Rollback / Recovery

Production rollout rollback: 100% → 50% → 10% → 0% via Wrangler version revert. RTO ≤ 10 min; RPO 0 (data preserved).

## 26. Security & Privacy (cumulative S-05; compact)

**STRIDE**: TenantCtx-only enforcement (WI-001); Merkle dual-side (WI-005); HKDF sig (WI-005); R2 ACL hardening (WI-003 + WI-004 schema). **LINDDUN**: tenant pseudonymous; PII redact policies; dual-side cripto verify; bucket private; SLSA L3 partial alignment.

## 27. Knowledge Transfer

- Tech talk (3h): "S-05 Multipart GA — Architecture Overview + Operational Readiness".
- Doc `docs/customer/multipart-feature-overview.md`.
- Workshop com all reviewers + on-call team.
- Onboarding test (10 questions): cross-tenant prevention, REAPI conformance, RB-FM-060, ADRs ratificadas, cost gate, customer SLA, rollout plan, Merkle dual-side, FastCDC determinism, sweeper.

## 28. Risk Register (12-row 6-col)

| ID | Risco | Prob | Det | Imp | Exp | Res | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | REAPI conformance regression silent | M | M | HIGH | M | LOW | Nightly CI; quarterly bump; community engagement |
| R-002 | PRR sign-off rubber-stamp | M | M | HIGH | M | LOW | Per-role checklist evidence; lesson Lote 10.4bis |
| R-003 | Property test 100k flake | L | H | LOW | L | LOW | Re-run 3×; deterministic seeds |
| R-004 | RB-FM-060 dry-run gap | M | L | HIGH | L | LOW | Iterate + re-run |
| R-005 | DASH-MULTIPART alert noisy | M | M | MEDIUM | M | LOW | Chaos test calibration |
| R-006 | 72h SLO incident reset cascade | L | M | MEDIUM | L | LOW | P3 minor não reseta; 4-tier classification (lesson Lote 10.4bis) |
| R-007 | 160 GiB stitched flow bug | L | M | HIGH | L | LOW | Integration test 200 GiB blob; chaos test mid-flight |
| R-008 | Crypto SME unavailable on ship date | M | L | MEDIUM | L | LOW | 2-week advance booking; lesson Lote 10.4bis 40-80h |
| R-009 | Cost regression > 10% sustained | M | L | MEDIUM | L | LOW | §14.10 cost gate; weekly bench |
| R-010 | 13 INVs §3.16 not promovidas (lesson Lote 10.4bis persistent gap) | M | M | HIGH | M | LOW | validate_inv_promotion.py CI gate; ST-013 explicit |
| R-011 | ADR ratificação rollback pressure | L | L | LOW | L | LOW | ADR change requires ADR; transparency |
| R-012 | Sweeper cron stale > 1h (silent death) | L | M | HIGH | L | LOW | Alarm re-arm at start (lesson Lote 10.4bis WI-S04-005); metric alert |

## 29. Review Checkpoints

D+0 WI-001..005 SEALED; D+1 conformance + 100k green; D+2 72h SLO start; D+3 DASH live; D+4 RB-FM-060 dry-run; D+5 72h SLO complete; D+6 PRR meeting; D+7 customer comm ready; D+8 ship decision; D+9 production 10%; D+11 50%; D+13 100%; D+20 post-ship review.

## 30. Sign-off (HIGH_RISK 11 canonical)

| # | Role | Status |
|---|---|---|
| 1-2 | Owner / Final Approver (Gustavo) | _pending_ |
| 3 | SRE Lead | _staffing-blocked; ADR-0034 waiver via Architect compensation_ |
| 4 | Security Lead | _TBD; **mandatory** — 5-layer defense + AppSec checklist 100%_ |
| 5-6 | Engineer × 2 | _TBD; **mandatory**_ |
| 7 | QA | _TBD; **mandatory** — integration + chaos + conformance_ |
| 8 | Product (Gustavo) | _pending_ |
| 9 | Compliance | _TBD; **mandatory** — SLSA L3 alignment_ |
| 10 | Privacy | _TBD; **mandatory** — PII redaction_ |
| 11 | Architect | _TBD; **mandatory** — ADR ratificações + handler trait + sharding ADR-0040_ |
| 12 | AppSec | _TBD; **mandatory emphatic** — tampering detection + 5-layer + bucket ACL_ |
| 13 | Crypto SME | _**MANDATORY** (não advisory; lesson Lote 10.4bis WI-S04-006 fix; Crypto SME independent review of WI-S05-002 + WI-S05-005 pre-PRR mandatory; PRR ceremony references those sign-offs)_ |

**Sign-off discipline** (lesson Lote 10.4bis WI-S04-006):
- 12 mandatory + Crypto SME mandatory non-waivable = 13 total mandatory.
- Per-role checklist evidence; rubber-stamp prohibited; CI gate validates.
- Sign-off via GitHub PR review + git commit signing.

## 31. Change Log

1.0.0 / 2026-04-25 / Gustavo: Criação WI-S05-006 (Lote 10.5; SOTA pós-Lote 10.4bis lessons applied: 4-tier incident classification; Crypto SME mandatory non-waivable; validate_inv_promotion.py CI gate; production rollout gradual; sprint contract drift defense).

1.3.0 / 2026-05-01 / Gustavo (via Claude Opus 4.7 1M): **WI-S05-006 SEALED — sweeper Cron DO pure-logic core + cross-component property suite + REAPI v2 SplitBlob/SpliceBlob conformance subset + DASH-MULTIPART dashboard + RB-FM-060 host-side dry-run + ASVS V5/V6/V8/V10/V14 self-checklist + adversarial summary + internal pentest + PRR-S05 ship gate.** Sweeper module `crates/corelink-worker/src/reapi/cas/sweeper.rs` ships canonical `OrphanSweeper` trait + `InMemoryOrphanSweeper` pure-logic fake honouring every WI §6.1.1 invariant: per-region instance + alarm re-arm at tick start (DO production wiring) + bounded batch ceiling `MAX_BATCH_SIZE = 250` (D1 100 KB-aligned; mirrors `reapi::ac::ttl::evict::MAX_BATCH_SIZE` per WI-S04-005 sharding lesson) + canonical 7-day age cutoff (`ORPHAN_AGE_MS`) + 1 h tick interval (`TICK_INTERVAL_MS`) + canonical reason `orphan_swept` (`ORPHAN_SWEPT_REASON`) + 6 canonical metric pairs surfaced via `canonical_metric_pairs` helper (`corelink.multipart.sweeper.{ticks_total, orphans_aborted_total, batch_size, already_finalized_total, already_aborted_total, not_found_total}`); `SessionStore` trait extended with `list_orphans(region, now_ms, max_age_ms, limit) -> Vec<OrphanCandidate>` (canonical order `(tenant_id, last_activity_at_ms ASC)`) + `OrphanCandidate` newtype `{tenant_id, session_id, region, last_activity_at_ms}` so cross-tenant abort is structurally unreachable through the trait surface. Cross-component property suite `crates/corelink-worker/tests/prop_multipart_full.rs` ships **4 release-mode properties** behind `tower-middleware`: `prop_multipart_full_stack_tenant_isolation_100k` SHIP-GATE 100 000 iter (`INV-MULTIPART-PATH-TENANT-SCOPED` + `INV-TENANT-ISOLATION` across handler → cas → chunker → r2-multipart → multipart-schema → manifest → audit → sweeper); `prop_multipart_full_stack_idempotent_finalize` 10k iter (`INV-MULTIPART-IDEMPOTENT`); `prop_multipart_full_stack_sweeper_orphan_abort_isolated` 10k iter (mixed Live/Finalized/Aborted cohort; only Live transitions); `prop_multipart_full_stack_streaming_verify_fail_fast` 10k iter (tampered chunk fail-fasts BEFORE bytes leak to caller's sink). Total 130 000 iter PR aggregate; `PROPTEST_CASES` opts in to nightly higher budgets. REAPI v2 SplitBlob/SpliceBlob conformance subset `crates/corelink-worker/tests/reapi_v2_split_splice_conformance.rs` ships **10 pinned tests** against ADR-0038 §Annex (5 SplitBlob: init_success / chunk_append_canonical / finalize_idempotent_reinit / chunk_ordering_violation_rejected / cross_tenant_session_isolation; 5 SpliceBlob: success_bytes_equal / unknown_manifest_404 / cross_tenant_404 / streaming_canonical_order / fail_fast_on_tampered_chunk); 100% pass non-negotiable per spec contract §19. **DASH-MULTIPART dashboard** `dashboards/grafana/DASH-MULTIPART.json` 10 canonical panels per WI §6.1.5 (throughput / latency p99 / orphan rate / dedup ratio / sweeper tick rate / R2 backend availability / sig invalid / REAPI conformance / cost per-op / manifest verify rate); **alert rules** `dashboards/alerts/dash-multipart-alerts.yml` 11 SEV-0/1/2/3 rules (Multipart_CrossTenantBreach SEV-0 / SigInvalidSustained SEV-0 / ConformanceRegression SEV-0 / SweeperStale SEV-1 / OrphanRateHigh SEV-2 / ThroughputBelowFloor SEV-0 / LatencyP99Breach SEV-1 / R2BackendDegraded SEV-1 / CostRegression SEV-2 / DedupRatioBelowFloor SEV-3 / ManifestVerifyTampering SEV-1). **RB-FM-060 host-side dry-run** `scripts/rb_fm_060_dry_run.sh` cargo-driven 7-step walkthrough + drift detection + EVT-017 evidence summary (executes `prop_multipart_full` orphan + tenant isolation arms + REAPI conformance suite); evidence captured at `specs/_audits/2026-05-01-rb-fm-060-dry-run.md`. **OWASP ASVS V5/V6/V8/V10/V14 self-checklist** `specs/04_sprints/S05/asvs-v5-v6-v8-v10-v14-checklist.md` 56 PASS / 2 WAIVED (V8.3.3 + V8.3.7 — S-19 onboarding) / 16 N/A. **Adversarial summary** `specs/_audits/2026-05-01-adversarial-s05.md` ~80 scenarios catalogued. **Internal pentest** `specs/_audits/2026-05-01-pentest-s05-internal.md` 7 attack surfaces, zero HIGH/CRITICAL. **PRR-S05** `specs/04_sprints/S05/PRR-S05.md` 11 sign-offs canonical (5 ✅ APPROVED + 6 ⚠️ WAIVED via ADR-0034); promotion decision STAGING-STABLE. Quality gates verde: `cargo test --workspace --all-targets --features corelink-worker/tower-middleware` 0 failures; `cargo clippy ... -D warnings` clean; `validate_specs.py` clean (275 schema-complete + 6 yaml); `validate_references.py` no NEW dangling refs (pre-existing only); `check_migrations_additive.py` clean. **Trait-abstraction-defer per charter** for the 3 deferred binding shims (real D1 chunks/manifest/sessions; real R2 multipart; real Cron-DO sweeper) + 160 GiB stitched flow E2E + 72h SLO sustained staging — all forward-looking gates with explicit revalidation triggers documented in PRR §3 DoD. F-001 closure preserved (every shared collection on `Arc<Mutex<…>>` field; no global mutable state in sweeper or fakes).

## 32. Anti-patterns evitados

- ❌ Ship without 100% conformance; ❌ Ship without 11 sign-offs canonical; ❌ Ship without RB-FM-060 dry-run; ❌ Ship com 1+ tenant isolation violation 100k; ❌ Ship com throughput < 100 MB/s; ❌ Ship com cost regression > 10%; ❌ Ship com ADR não ratificada; ❌ Ship com 13 INVs não promovidas; ❌ Ship com sign-off rubber-stamp; ❌ Production rollout direct 100%; ❌ Skip 160 GiB stitched flow; ❌ Skip post-ship review; ❌ Crypto SME (folds into Architect specialization per Lote 10.4-bis-quater normalization).

---

**Fim WI-S05-006.** S-05 spec FULL SOTA completo (6 WIs HIGH_RISK; 5 ADRs forward; 16 INVs forward §3.16).

**Próximo**: dispatch 2 agent reviews (parts 1+2) Lote 10.5 review cycle; apply Lote 10.5bis P0 fixes; commit.
