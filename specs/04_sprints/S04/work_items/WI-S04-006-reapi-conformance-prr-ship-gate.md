---
id: "WI-S04-006"
type: "work_item"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-25"
updated: "2026-04-25"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-002", "FF-HR-005", "FF-HR-009"]
parent: "S-04"
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
tags: ["wi", "s04", "ac", "conformance", "reapi", "dashboard", "rb-fm-303", "prr", "ship-gate", "high-risk"]
---

# WI-S04-006 — REAPI v2 Conformance Test Suite + DASH-AC Dashboards + Cache Hit Ratio Business Métrica + RB-FM-303 Dry-Run + Property Test 100k Tenant Isolation + PRR HIGH_RISK 13 Sign-offs Ship Gate

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
> **Parent:** [S-04](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S04-006 |
| Título | bazelbuild/remote-apis conformance suite 100% AC ops; DASH-AC dashboards (cache hit ratio + latency + tenant isolation alert); cache hit ratio customer-visible business métrica; RB-FM-303 (AC cross-tenant) dry-run executado em staging; property test 100k tenant isolation; PRR HIGH_RISK 13 sign-offs ship gate; ADR-0021 ratificada confirmação; cost regression gate green; SLO 72h sustained; ship/no-ship decision |
| Sprint | S-04 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-002 (ship gate prevents FM-303 catastrophic; cross-tenant validation primary), FF-HR-005 (PRR enforces all CTRL-AC controls), FF-HR-009 (defense-in-depth final validation) |

## 1. Intent

Este WI é o **ship gate** de S-04: AC entra em production-ready state somente após PRR HIGH_RISK 13 sign-offs aprovados + REAPI v2 conformance 100% + 72h SLO sustained + RB-FM-303 dry-run executado + property test 100k cross-tenant 0 violations + custom dashboard live + cost gate green.

```text
Ship Gate Components:

1. **REAPI v2 Conformance Suite**:
   - Source: bazelbuild/remote-apis @ pinned commit (vendored em WI-S04-001 build.rs).
   - Subset: AC operations only (GetActionResult, UpdateActionResult, BatchUpdateActionResult).
   - Run: nightly CI against staging environment.
   - Gate: 100% pass; PR red if regression.
   - Quarterly bump cadence (community engagement).

2. **DASH-AC Dashboards** (Grafana ou CF Analytics):
   - Cache hit ratio per (tenant_tier, region) — customer-visible business métrica.
   - Latency p99 GET/UPDATE warm/cold per region.
   - Tenant isolation alert (any cross-tenant request detected — should be 0).
   - Sig invalid rate (alert if > 5/h).
   - TTL eviction rate per region.
   - Negative cache hit/populate/invalidate rates.
   - REAPI conformance status (last run timestamp + result).
   - Cost per-op tracker (regression alert).

3. **Cache hit ratio business métrica** (customer-visible):
   - Emitted via S-09 aggregation: `corelink.cache.hit_ratio{type=ac, tenant_tier, region}`.
   - Customer dashboard S-16 displays.
   - Customer-facing SLA target: ≥ 70% staging workload (Bazel synthetic).
   - Per-tenant breakdown forward S-13 admin plane.

4. **RB-FM-303 (AC cross-tenant) dry-run**:
   - Runbook RB-FM-303 already published (Lote 5.7).
   - Dry-run: simulated incident em staging — engineer follows runbook step-by-step.
   - Validates: detection ≤ 5min via property test 100k cron + DASH-AC alert; remediation procedure (rollback Worker version + flush KV cache + audit chain reconstruction); customer notification template.
   - Outcome: post-mortem written; runbook gaps documented + iterated.

5. **Property test 100k tenant isolation**:
   - Extends WI-S04-001 prop_ac_get_tenant_isolation.
   - 100k iterations nightly CI.
   - 0 violations tolerated (CRITICAL gate).
   - Coverage: random tenant pairs × random action_digests × random ActionResults.

6. **PRR HIGH_RISK 13 sign-offs**:
   - Owner + Final Approver (Gustavo).
   - SRE Lead (staffing waiver per ADR-0034 forward).
   - Security Lead, Engineer×2, QA, Product, Compliance, Privacy, Architect, AppSec, Crypto SME (advisory).
   - 12 mandatory + 1 advisory = 13 total.
   - Each role signs gates: their domain checks complete.

7. **72h SLO sustained**:
   - SLO-AVAIL-AC ≥ 99.9% sustained 72h staging.
   - SLO-LAT-AC-HIT p99 ≤ 150ms warm; UPDATE p99 ≤ 300ms; sustained 72h.
   - Continuous staging deployment with synthetic Bazel workload (10 actions × 5 rebuilds × hourly cycle).

8. **Cost regression gate**:
   - Per-op cost ≤ targets (verify $0.000002, sign $0.000001, GET warm $0.000005, UPDATE $0.000015, eviction $0.000003).
   - CI bench fails if exceed; CI gate.
```

**Constraint cripto-driven**:

1. **REAPI conformance is non-negotiable**: 100% pass; <100% blocks ship; quarterly bumps via ADR.
2. **PRR sign-offs documented**: each role's checklist + sign-off entry in WI-S04-006 §30.
3. **RB-FM-303 dry-run mandatory**: cannot ship without execution + post-mortem.
4. **Property test 100k = 0 violations**: cripto-grade gate; even 1 violation blocks ship.
5. **72h SLO sustained**: continuous staging; pause-on-incident reset clock.

## 2. Narrative (HIGH_RISK ≥ 300 palavras + risk justification)

S-04 ship gate é o "go/no-go" review antes de promoting AC para production-ready (next phase: real customer traffic). Bug em ship gate = AC enters prod com defect → customer-facing incident → reputational damage + churn. Most prior WIs (001..005) handle individual cripto/infra concerns; this WI is the **end-to-end validation** que ALL pieces work together + edge cases covered + operational readiness real.

HIGH_RISK em N dimensões:

1. **REAPI conformance regression silent**: nightly CI passes 100% in staging; new Bazel client version (8.x) introduces edge case not in conformance suite; production breaks for tenant on Bazel 8. Mitigação: conformance suite covers Bazel current + next major (Bazel 7+8 in S-04 GA); quarterly bump cadence; community engagement with Bazel maintainers; ADR upgrade policy.

2. **PRR sign-off rubber-stamp**: roles sign without thorough review (time pressure, staffing-blocked role waived). Mitigação: each role has explicit checklist + sign-off requires "I read XYZ and verified A/B/C"; ADR-0034 documents staffing waivers (Architect compensates for SRE Lead in solo-tier); CI gate validates sign-off entries non-empty.

3. **Property test 100k flake (1-em-1B false-positive)**: extremely rare but possible; would block ship despite clean code. Mitigação: re-run on flake (3× retry policy); deterministic seeds for reproducibility; ADR-0035 forward documents flake handling.

4. **RB-FM-303 dry-run gap**: dry-run reveals procedure incomplete; engineer cannot execute remediation < 30min. Mitigação: dry-run is gate; gap → iterate runbook → re-run; cannot ship until clean execution; post-mortem documents iteration.

5. **DASH-AC alert noisy** (false-positive cross-tenant alerts due to metric calc bug): on-call burnout; real incident missed. Mitigação: dashboard tested with synthetic data 7 days prior; alert thresholds calibrated; chaos test simulates incident vs noise.

6. **Cache hit ratio business métrica deviation**: customer expects 70% but staging workload = 50%; customer SLA breach if S-04 GA promises 70%. Mitigação: synthetic workload calibrated against real Bazel projects (Bazel reference projects); ADR-0019 documents per-tier expected hit ratio (free tier short TTL = lower hit ratio expected).

7. **72h SLO incident reset clock**: minor incident at 71h reset; ship delayed +72h. Mitigação: incident classification (P1/P2/P3); P3 minor (e.g., metric drift) does not reset; P1/P2 reset; pre-defined per ADR.

8. **Cost regression gate flake**: bench latency varies ±15%; gate fails ±10%. Mitigação: bench warm-up + multiple runs (5×); take median; flake retry 3×.

9. **Crypto SME availability** for final review: SME unavailable last week; ship blocked. Mitigação: book 2 weeks advance; backup pool documented (external advisor S-XX forward); ADR-0034 advisory waiver path.

**Atacante adversarial scenarios**:

- **Conformance suite fork attack**: attacker submits PR adjusting bazelbuild/remote-apis pinned commit to a fork that hides issues. Mitigação: CI uses upstream community-vetted commit; ADR for bumps; review required.

- **Dashboard metric injection**: attacker (insider) injects fake metrics into Grafana to hide real incident. Mitigação: metrics ingestion auth (S-09 forward); anomaly detection on metric drops/spikes; multiple sources cross-correlation.

- **PRR sign-off forge**: attacker forges sign-off entry. Mitigação: GitHub PR review + git commit signing; sign-off in PR description; cannot bypass review.

- **Synthetic workload bias**: attacker designs workload to inflate hit ratio. Mitigação: workload defined in committed config; reviewed by Product + Architect; reproducible by external audit.

**Risk justification HIGH_RISK**:

- **FF-HR-002**: ship gate prevents cross-tenant catastrophic; final defense before prod.
- **FF-HR-005**: PRR enforces all CTRL-AC controls (CTRL-AC-001 Merkle + CTRL-AC-002 sig).
- **FF-HR-009**: defense-in-depth final validation across 5 prior WIs.
- **Reversibility**: ship decision is binary; rollback via revert deploy; minor blast radius if caught early.
- **Customer impact**: ship-broken = immediate customer perception of unreliability; trust erosion.

13 sign-offs — this WI's defining moment.

## 3. Customer Impact & Journey

**Persona 1 — Bazel CI dev expecting GA reliability**:
- After S-04 SEALED + ship gate green: AC handles `bazel build //...` reliably with cache hit ratio > 70% staging baseline.
- Customer dashboard S-16 shows cache hit ratio per region; customer can compare expected vs actual.
- SLO baseline: 99.9% availability; p99 ≤ 150ms; published in customer-facing SLA addendum.

**Persona 2 — DevOps reviewing operational readiness**:
- Reviews DASH-AC dashboards: cache hit ratio + latency + cost.
- Reviews PagerDuty integrations: alerts fire on tenant isolation breach + sig invalid spike.
- Reviews runbooks: RB-FM-303 (AC cross-tenant) executable in < 30min.
- Reviews REAPI conformance suite history: 100% pass rate.

**Persona 3 — Compliance reviewer (LGPD/GDPR + SLSA)**:
- Reviews ADR-0021 ratificada (HKDF signing decision) + ADR-0019 (TTL handoff).
- Reviews PRR sign-offs (12+1 advisory complete).
- Reviews cripto chain: Merkle dual-side + HKDF sig + tenant_scoped storage.
- Reviews audit chain (S-09): all AC ops emit audit events.

**SLA addendum**:
- AC GET p99 ≤ 150ms warm; cold ≤ 280ms; sustained 72h.
- AC UPDATE p99 ≤ 300ms; sustained 72h.
- Cache hit ratio business métrica: ≥ 70% staging Bazel workload (calibrated synthetic).
- REAPI conformance: 100% AC ops sustained CI nightly.
- Cross-tenant alert threshold: 0 (any breach = SEV-0).
- Cost per-op gates green: GET ≤ $0.000010; UPDATE ≤ $0.000020; eviction ≤ $0.000003.

## 4. Capability Mapping

- All CAP-AC-001..006 — VALIDATED end-to-end.
- Trace: `remote_cache_product_profile.md §6 (REAPI conformance)` + `slo_catalog.md SLO-AVAIL-AC + SLO-LAT-AC-HIT` + `failure_modes.md FM-303 + RB-FM-303` + `observability_model.md DASH-AC`.

## 5. Tipo

PRR ship gate; HIGH_RISK; FF-HR-002 + FF-HR-005 + FF-HR-009.

## 6. Escopo

### 6.1 In-scope

1. **REAPI v2 Conformance Suite Integration**:
   - Pinned commit of bazelbuild/remote-apis test suite.
   - CI integration `tests/conformance/reapi_v2_ac.rs`:
     - Spawns staging environment via wrangler dev OR connects to staging.corelink.dev.
     - Runs subset: AC operations only.
     - Validates: GetActionResult NotFound + GetActionResult Hit + UpdateActionResult success + UpdateActionResult idempotent + BatchUpdate (100 cap) + edge cases.
   - Result artifact: JSON report + HTML view; uploaded to CI artifacts.
   - Gate: 100% pass; PR red if regression.
   - Schedule: nightly + per-commit.

2. **DASH-AC Dashboards**:
   - Grafana JSON specifications (or CF Analytics equivalent):
     - Panel 1: Cache hit ratio per (tenant_tier, region) — line graph + per-tier breakdown.
     - Panel 2: Latency p99 GET/UPDATE warm/cold per region — heatmap.
     - Panel 3: Tenant isolation alert (cross-tenant request count) — single-stat; should always = 0.
     - Panel 4: Sig invalid rate — alert ≥ 5/h.
     - Panel 5: TTL eviction rate per region.
     - Panel 6: Negative cache hit/populate/invalidate rates.
     - Panel 7: REAPI conformance status (last run + result).
     - Panel 8: Cost per-op tracker — alert if > 110% of target.
   - Auto-deploy via Grafana provisioning + git-as-source.
   - Alert routes to PagerDuty + Slack.

3. **Cache hit ratio business métrica**:
   - Emit via S-09 aggregation; export to customer dashboard S-16.
   - Definition: `(GET ok count) / (GET ok + GET miss count)` per (tenant_tier, region) per 5min window.
   - Customer-facing display in dashboard: real-time + 24h trend.
   - SLO baseline: ≥ 70% staging.

4. **RB-FM-303 dry-run execution**:
   - Setup: simulated incident em staging (chaos PR introduces tenant_id confusion bug; AC entry leaks).
   - Engineer follows RB-FM-303 step-by-step.
   - Time-track: detection (≤ 5min target), remediation (≤ 30min target), customer notification (≤ 1h target).
   - Post-mortem: gaps documented; runbook iterated; re-run if changes substantial.
   - Artifact: `specs/_audits/2026-XX-XX-rb-fm-303-dry-run.md` post-mortem report.

5. **Property test 100k tenant isolation**:
   - Module: `tests/prop_ac_tenant_isolation_100k.rs`.
   - Generator: random TenantA + TenantB pairs; random action_digests; random ActionResults.
   - Iterations: 100k per nightly CI run.
   - Assertion: 0 cross-tenant access (TenantB GET on TenantA's entry → 404).
   - CI gate: 100% pass; ANY violation blocks ship.

6. **PRR HIGH_RISK 13 sign-off ceremony**:
   - PRR review meeting: 2-hour structured review.
   - Each role's checklist:
     - **Owner / Final Approver** (Gustavo): all WIs SEALED; ship readiness.
     - **SRE Lead** (waiver per ADR-0034): on-call playbook updated; PagerDuty integrations validated.
     - **Security Lead**: 5-layer defense validated; AppSec checklist 100%.
     - **Engineer (peer 1+2)**: code review depth; unit/integration tests green; rustdoc complete.
     - **QA**: integration tests green; conformance suite passes; chaos suite executed.
     - **Product**: customer-facing features (cache hit ratio dashboard) work; SLA addendum reviewed.
     - **Compliance**: LGPD Art. 38 + GDPR Art. 32 alignment; SLSA L3 partial.
     - **Privacy**: PII redaction in audit chain; ActionResult metadata redaction policy customer-side.
     - **Architect**: handler trait composability; non-exhaustive evolution; ADR-0021/0035/0036/0037 ratificadas.
     - **AppSec**: tampering detection (Merkle + sig); session cache poisoning prevention; bucket ACL hardening.
     - **Crypto SME (advisory)**: HKDF sig protocol independent verification; constant-time discipline; key rotation.
   - Sign-offs collected via GitHub PR review + git commit signing.
   - Final sign-off → ship green light.

7. **72h SLO sustained validation**:
   - Continuous staging deployment; synthetic Bazel workload runs 24/7.
   - Monitoring: SLO-AVAIL-AC ≥ 99.9%; SLO-LAT-AC-HIT p99 ≤ 150ms; UPDATE p99 ≤ 300ms.
   - Pause clock on P1/P2 incidents; resume after fix; require continuous 72h pre-ship.
   - Artifact: SLO compliance report.

8. **Cost regression gate validation**:
   - CI bench runs criterion benchmarks against baseline.
   - Targets: verify $0.000002, sign $0.000001, GET warm $0.000005, UPDATE $0.000015, eviction $0.000003.
   - Tolerance: ±10% baseline.
   - Gate: PR red if > 10% regression.

9. **ADR ratificação confirmation**:
   - ADR-0021 (HKDF vs Ed25519): ratificada in WI-S04-004; this WI confirms.
   - ADR-0035 (handler invariants): ratificada in WI-S04-001.
   - ADR-0036 (schema migration governance): ratificada in WI-S04-002.
   - ADR-0037 (Merkle protocol): ratificada in WI-S04-003.
   - All four whitelisted in `validate_references.py` (forward → ACCEPTED).

10. **Customer-facing communication preparation**:
    - SLA addendum draft: `docs/customer/ac-sla-addendum-s04-ga.md`.
    - Release notes draft: `docs/customer/release-notes-s04.md` — features, SLOs, known issues, customer SDK requirements (Rust v1.0 corelink-ac client).
    - Bazel customer onboarding doc: `docs/customer/bazel-remote-cache-setup.md`.

11. **REAPI conformance baseline**:
    - Pinned commit recorded in ADR-0036 §change_log.
    - Bazel client version compatibility: 7.x + 8.x covered.
    - Buck2 client compatibility: documented; conformance subset exec.

12. **CI integration final**:
    - `.github/workflows/ac-ship-gate.yml`: combined REAPI conformance + property test 100k + cost regression + SLO compliance.
    - PR red if any sub-gate fails.
    - Nightly + on-demand triggers.

13. **Documentation final**:
    - `docs/customer/ac-feature-overview.md`.
    - Internal runbook index updated.
    - All ADRs ratificadas referenced.

### 6.2 Out-of-scope (deferred)

- **Multi-region failover testing** (S-14).
- **Cross-tenant federation** (S-XX forward).
- **Customer onboarding automation** (S-15 CLI/SDK).
- **Per-tenant SLA customization** (S-13 admin plane).
- **Bazel community contribution** (post-GA roadmap).
- **External pen-test** (S-XX forward; security audit).

## 7. Anti-Scope

- ❌ Ship without 100% conformance.
- ❌ Ship without 13 sign-offs.
- ❌ Ship without RB-FM-303 dry-run completion.
- ❌ Ship with 1+ tenant isolation violation in 100k test.
- ❌ Ship with SLO < 99.9% sustained.
- ❌ Ship with cost regression > 10%.
- ❌ Ship with ADR not ratificada.
- ❌ Ship with sign-off rubber-stamp (no checklist evidence).
- ❌ Ship with synthetic workload not aligned to real Bazel patterns.
- ❌ Ship with documentation gap (customer-facing SLA missing).
- ❌ Ship with PRR meeting < 2h (insufficient depth).
- ❌ Ship with chaos suite not executed.

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: S-04 PRR HIGH_RISK 13 sign-off ship gate

  Background:
    Given WI-S04-001..005 SEALED
    Given staging environment with full S-04 stack deployed
    Given synthetic Bazel workload running 24/7

  Scenario: REAPI v2 conformance suite 100% pass
    Given bazelbuild/remote-apis test suite pinned commit
    When `tests/conformance/reapi_v2_ac.rs` runs against staging
    Then 100% AC ops pass (GetActionResult, UpdateActionResult, BatchUpdateActionResult)
    And conformance report uploaded to CI artifacts
    And no edge cases failed

  Scenario: Property test 100k tenant isolation
    Given `tests/prop_ac_tenant_isolation_100k.rs`
    When 100k iterations run in nightly CI
    Then 0 cross-tenant violations detected
    And property test green
    And CI gate green

  Scenario: 72h SLO sustained
    Given continuous staging workload
    When 72h continuous run with monitoring
    Then SLO-AVAIL-AC ≥ 99.9% sustained
    And SLO-LAT-AC-HIT p99 ≤ 150ms warm
    And UPDATE p99 ≤ 300ms
    And No P1/P2 incidents
    (P3 minor incidents acceptable; do not reset clock)

  Scenario: RB-FM-303 dry-run successful
    Given staging environment
    When chaos PR introduces tenant_id confusion bug
    And on-call engineer follows RB-FM-303 runbook
    Then incident detected ≤ 5min via DASH-AC alert
    And remediation completed ≤ 30min
    And customer notification template ready ≤ 1h
    And post-mortem written
    And runbook gaps iterated (if any)

  Scenario: DASH-AC dashboards live + alerts validated
    Given Grafana dashboards provisioned
    When metrics flow from staging
    Then 8 panels display correctly
    And alerts route to PagerDuty + Slack
    And cache hit ratio panel shows ≥ 70% baseline

  Scenario: Cache hit ratio business métrica customer-visible
    Given S-09 aggregation emits corelink.cache.hit_ratio{type=ac, tenant_tier, region}
    When customer dashboard S-16 queries
    Then real-time hit ratio displayed
    And 24h trend rendered

  Scenario: PRR HIGH_RISK 13 sign-offs collected
    Given PRR review meeting executed (2h structured)
    When each role completes checklist + signs
    Then 12 mandatory + 1 advisory = 13 sign-offs collected
    And sign-offs evidenced in PR review + git commit signing
    And no rubber-stamp (each sign-off has checklist evidence)

  Scenario: Cost regression gate green
    Given CI bench runs criterion benchmarks
    When per-op cost measured
    Then verify ≤ $0.000002, sign ≤ $0.000001
    And GET warm ≤ $0.000005, UPDATE ≤ $0.000015, eviction ≤ $0.000003
    And no > 10% regression vs baseline
    And gate green

  Scenario: All 4 ADRs ratificadas
    Given ADR-0021 (HKDF), ADR-0035 (handler), ADR-0036 (schema), ADR-0037 (Merkle)
    Then all doc_status = ACCEPTED
    And whitelist em validate_references.py
    And rationale + risks + mitigations documented

  Scenario: Customer-facing communication ready
    Given docs/customer/ac-sla-addendum-s04-ga.md
    Given docs/customer/release-notes-s04.md
    Given docs/customer/bazel-remote-cache-setup.md
    Then all drafts complete
    And reviewed by Product + Compliance + Privacy

  Scenario: Ship gate green light
    Given all above scenarios green
    Then ship decision = GO
    And S-04 promoted to production-ready state
    And handler enabled for prod traffic gradual rollout (10% → 50% → 100%)

  Scenario: Ship gate red light (1 sub-gate fails)
    Given conformance suite returns 95% (regression)
    Then ship decision = NO-GO
    And blocker documented
    And iteration → fix → re-test
    And no production rollout

  Scenario: ADR-0034 staffing waiver path
    Given SRE Lead role staffing-blocked (solo-tier)
    When PRR review missing SRE Lead sign-off
    Then ADR-0034 waiver invoked: Architect compensates with on-call playbook review
    And sign-off marked "_staffing-blocked_waiver_per_ADR-0034_"
    And ship still proceeds (12 mandatory + 1 advisory still met via Architect compensation)

  Scenario: Crypto SME advisory waiver (acceptable per ADR-0034)
    Given Crypto SME unavailable on ship date
    When sign-off pending > 1 week
    Then advisory waiver: Architect + AppSec covenant; Crypto SME post-ship review committed
    And sign-off marked "_advisory_post_ship_"
    And ship proceeds with 12 mandatory complete

  Scenario: Quarterly conformance bump cadence
    Given REAPI v2 spec evolves (new Bazel client version)
    When quarter elapses
    Then ADR for bump submitted
    And conformance suite updated
    And re-run validates compatibility
```

## 9. Design Decisions

### 9.1 Why ship gate as separate WI (não em-line)

- Discrete validation milestone; clear go/no-go decision.
- Separation of concerns: WI-001..005 implement; WI-006 validates.
- Audit trail: ship decision documented in WI's change log.

### 9.2 Why 100% REAPI conformance (não 95%)

- Bazel/Buck2 ecosystem is unforgiving; clients break on edge cases.
- 95% leaves 5% surface for customer incidents post-ship.
- 100% is achievable; conformance suite well-defined; customer trust requires.

### 9.3 Why property test 100k (não 1M)

- 100k provides 3 orders of magnitude beyond typical workload (10-100 actions per build).
- 1M = 10× cost without proportional confidence increase.
- Lote 10.3 calibrated: 100k nightly + 10k PR; cripto-grade confidence.

### 9.4 Why 72h SLO sustained (não 24h or 1week)

- 24h: inadequate; misses weekend traffic patterns + overnight batch cycles.
- 1 week: too long; ship delayed without proportional confidence gain.
- 72h: balances; covers weekday + weekend; allows incident reset without major delay.

### 9.5 Why RB-FM-303 dry-run mandatory

- Cross-tenant catastrophic = highest-impact failure mode.
- Runbook untested = paper exercise; real incident response unproven.
- Dry-run validates: detection mechanism, remediation steps, customer comm template.

### 9.6 Why ADR-0034 staffing waiver path documented

- Solo-tier reality: SRE Lead may not exist.
- Waiver allows progress; ensures compensating control (Architect handles SRE concerns).
- Documented in ADR; transparent to reviewers.

### 9.7 Why customer-facing communication pre-ship (não post)

- SLA addendum + release notes set customer expectations correctly pre-rollout.
- Avoid post-ship clarifications; reduce customer support burden.
- Customer onboarding doc: enables self-serve setup.

### 9.8 Why cost regression gate (not just SLO)

- SLO measures availability + latency; not cost.
- Cost regression hidden = financial loss compounds.
- ±10% tolerance balances flake vs real regression.

### 9.9 Why advisory + mandatory split (Crypto SME advisory)

- Crypto SME deeply review during S-04 implementation (WI-S04-003 + WI-S04-004); ship gate is final pass.
- Advisory permits ship if Crypto SME unavailable on ship date but post-ship review committed.
- Documented in ADR-0034 / S-04 sign-off table.

### 9.10 ADR potencial?

- All 4 ADRs already ratificadas in WI-001..004; this WI confirms + closes.
- No new ADR for ship gate itself; convention.

## 10. Completeness Criteria SOTA

- [ ] **10.s04.006.1** REAPI v2 conformance suite 100% AC ops pass nightly CI (EVT-002 + EVT-018).
- [ ] **10.s04.006.2** Property test 100k tenant isolation: 0 violations (EVT-002).
- [ ] **10.s04.006.3** 72h SLO sustained staging: SLO-AVAIL-AC ≥ 99.9%; p99 latency targets met (EVT-021).
- [ ] **10.s04.006.4** RB-FM-303 dry-run executed; post-mortem written (EVT-017).
- [ ] **10.s04.006.5** DASH-AC dashboards live; 8 panels deployed; alerts to PagerDuty + Slack (EVT-021).
- [ ] **10.s04.006.6** Cache hit ratio business métrica emit + customer dashboard S-16 displays (EVT-021).
- [ ] **10.s04.006.7** PRR HIGH_RISK 12 mandatory + 1 advisory = 13 sign-offs collected (EVT-031).
- [ ] **10.s04.006.8** Cost regression gate green: per-op costs within targets (Lote 9.4 §14.10).
- [ ] **10.s04.006.9** ADR-0021, ADR-0035, ADR-0036, ADR-0037 all ratificadas + whitelisted (EVT-027).
- [ ] **10.s04.006.10** Customer-facing communication ready: SLA addendum + release notes + Bazel onboarding doc (EVT-027).
- [ ] **10.s04.006.11** SLSA Level 3 partial alignment validated (Merkle + HKDF sig + audit chain).
- [ ] **10.s04.006.12** Cargo-audit + cargo-deny + clippy `-D warnings` clean across all S-04 crates.
- [ ] **10.s04.006.13** All Gherkin scenarios green em integration test.
- [ ] **10.s04.006.14** Production rollout plan: 10% → 50% → 100% gradual; monitoring + rollback procedure documented.

## 11. DoD

- [ ] WI-S04-001..005 SEALED (cumulative gate).
- [ ] REAPI conformance 100% green nightly.
- [ ] Property test 100k tenant isolation green.
- [ ] 72h SLO sustained.
- [ ] RB-FM-303 dry-run + post-mortem complete.
- [ ] DASH-AC dashboards live; alerts validated.
- [ ] Customer dashboard S-16 cache hit ratio emit OK.
- [ ] PRR meeting (2h) executado; 13 sign-offs collected.
- [ ] Cost regression gate green.
- [ ] All 4 ADRs ratificadas.
- [ ] Customer-facing comm ready.
- [ ] CI ship-gate workflow active.
- [ ] Production rollout plan documented.
- [ ] Architect + AppSec + Security Lead + Crypto SME final review.
- [ ] Final approver (Gustavo) ship decision = GO.

## 12. Invariants Validated (cumulative S-04)

End-to-end validation of all S-04 invariants:

- **INV-AC-TENANT-SCOPED** (CRITICAL, registry §3.3): 100k property test 0 violations.
- **INV-AC-OUTPUTS-VALID** (HIGH, registry §3.3): builder strict + reconcile diário + chaos test.
- **INV-AC-MERKLE-VALID** (CRITICAL, §3.15 NEW): WI-S04-003 dual-side; conformance + chaos.
- **INV-AC-DIGEST-SIGNED** (CRITICAL, §3.15 NEW): WI-S04-004 HKDF sig; cripto-grade Mann-Whitney.
- **INV-AC-MERKLE-DETERMINISTIC** (CRITICAL, §3.15 NEW): WI-S04-003 1000× build determinism.
- **INV-AC-IDEMPOTENT** (HIGH, §3.15 NEW): WI-S04-001 ON CONFLICT; conformance edge case.
- **INV-AC-RESULT-HASH-IMMUTABLE** (HIGH, §3.15 NEW): WI-S04-001 409 mismatch enforcement.
- **INV-AC-EVICT-TENANT-SCOPED** (CRITICAL, §3.15 NEW): WI-S04-005 strict tenant filter.
- **INV-AC-TTL-MONOTONIC** (HIGH, §3.15 NEW): WI-S04-005 refresh logic.
- **INV-AC-NEG-CACHE-INVALIDATED-ON-UPDATE** (HIGH, §3.15 NEW): WI-S04-001 atomic invalidate.
- **INV-AC-BOUNDED-PARSER** (HIGH, §3.15 NEW): WI-S04-003 bounds enforced.
- **INV-AC-CYCLE-FREE** (HIGH, §3.15 NEW): WI-S04-003 visited set.
- **INV-AC-SIG-CONSTANT-TIME** (HIGH, §3.15 NEW): WI-S04-004 Mann-Whitney 3-prong cripto-grade.
- **INV-AC-SIG-INFO-FIXED** (HIGH, §3.15 NEW): WI-S04-004 CI gate.
- **INV-AC-KEY-ROTATION-GRACE** (HIGH, §3.15 NEW): WI-S04-004 grace period test.
- **INV-AC-TDK-ZEROIZED** (MEDIUM, §3.15 NEW): WI-S04-004 chaos test.
- **INV-AC-CANONICAL-BYTES-STABLE** (HIGH, §3.15 NEW): WI-S04-004 layout fixed.
- **INV-AC-DUAL-SIDE-VERIFY** (HIGH, §3.15 NEW): WI-S04-003 server + client.
- **INV-AC-EVICT-CONSISTENCY** (HIGH, §3.15 NEW): WI-S04-005 R2-then-D1.

Total: 19 INVs in §3.15 to be promovidas in Lote 10.4bis (P0 fix).

TLA+ alignment: tenant_isolation.tla AC variant; cas_integrity.tla extension for AC (sig + Merkle dual layer).

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| Conformance harness | `tests/conformance/reapi_v2_ac.rs` | Rust |
| Property test 100k | `tests/prop_ac_tenant_isolation_100k.rs` | Rust |
| DASH-AC Grafana JSON | `dashboards/dash-ac.json` | JSON |
| Alert rules | `dashboards/alerts/dash-ac-alerts.yml` | YAML |
| RB-FM-303 dry-run report | `specs/_audits/2026-XX-XX-rb-fm-303-dry-run.md` | Markdown |
| PRR meeting notes | `specs/_audits/2026-XX-XX-s04-prr-meeting.md` | Markdown |
| SLO compliance report | `specs/_audits/2026-XX-XX-s04-slo-72h.md` | Markdown |
| SLA addendum customer | `docs/customer/ac-sla-addendum-s04-ga.md` | Markdown |
| Release notes | `docs/customer/release-notes-s04.md` | Markdown |
| Bazel onboarding doc | `docs/customer/bazel-remote-cache-setup.md` | Markdown |
| CI ship-gate workflow | `.github/workflows/ac-ship-gate.yml` | YAML |
| Production rollout plan | `docs/internal/ac-prod-rollout-plan.md` | Markdown |
| Conformance pinned commit | ADR-0036 update | Markdown |

## 14. Quality Standards SOTA

- **14.s04.006.1** REAPI conformance: 100% AC ops; quarterly bump cadence.
- **14.s04.006.2** Property test 100k: cripto-grade gate; 0 violations.
- **14.s04.006.3** SLO 72h: continuous; P3 only does not reset.
- **14.s04.006.4** RB-FM-303 dry-run: detection ≤ 5min; remediation ≤ 30min; customer comm ≤ 1h.
- **14.s04.006.5** Dashboard alerts: PagerDuty + Slack; tested via chaos.
- **14.s04.006.6** PRR meeting: 2h structured; 13 sign-offs documented.
- **14.s04.006.7** Cost regression gate: ±10% tolerance.
- **14.s04.006.8** Customer comm: SLA addendum + release notes + onboarding doc.
- **14.s04.006.9** ADR ratificadas: all 4 ACCEPTED.
- **14.s04.006.10** Production rollout plan: 10% → 50% → 100% gradual.

## 15. Chaos Experiments

1. **REAPI conformance regression**: chaos PR introduces subtle drift (e.g., metadata field swap); CI nightly catches; PR red.

2. **Property test 100k flake**: simulate 1-em-100k violation; CI red; investigation triggered; deterministic seed for repro.

3. **SLO 72h reset on P1**: simulate P1 incident at 71h; clock resets; ship delayed +72h.

4. **RB-FM-303 dry-run gap detected**: dry-run fails customer notification step; iterate runbook; re-run.

5. **Dashboard alert noisy**: chaos test simulates spurious cross-tenant metric; alert fires; threshold tuned.

6. **PRR sign-off rubber-stamp regression**: chaos PR submits with empty sign-off entries; CI gate red.

7. **Cost regression > 10%**: chaos PR introduces inefficient sig path; CI bench detects; PR red.

8. **ADR ratificação rollback**: chaos PR reverts ADR-0021 to DRAFT; whitelist validation fails; PR red.

9. **Customer comm gap**: chaos PR ships without SLA addendum; review process catches; ship blocked.

10. **CI ship-gate workflow regression**: chaos PR removes conformance step; CI gate red.

11. **Crypto SME advisory waiver edge**: chaos test SME unavailable + Architect + AppSec sign-off insufficient; ship blocked until resolved.

12. **Quarterly conformance bump regression**: chaos test simulates Bazel 8 release; pinned commit outdated; alert + ADR proposal.

## 16. PRR

THE PRR. Esta WI é a PRR.

- [ ] All Gherkin green.
- [ ] All cumulative WI invariants validated (19 INVs in §3.15).
- [ ] All 4 ADRs ratificadas.
- [ ] SLO 72h sustained.
- [ ] RB-FM-303 dry-run + post-mortem.
- [ ] 13 sign-offs collected.
- [ ] Cost gate green.
- [ ] Customer comm ready.
- [ ] Ship rollout plan documented.

## 17. Sub-tasks

| ID | Sub-task | Estimativa |
|---|---|---|
| ST-001 | REAPI conformance harness setup | 4h |
| ST-002 | Property test 100k tenant isolation | 3h |
| ST-003 | DASH-AC Grafana dashboards (8 panels) | 6h |
| ST-004 | Alert rules + PagerDuty + Slack integration | 4h |
| ST-005 | Cache hit ratio S-09 aggregation hook | 3h |
| ST-006 | Customer dashboard S-16 integration | 2h |
| ST-007 | RB-FM-303 dry-run execution + post-mortem | 6h |
| ST-008 | 72h SLO continuous staging setup + monitoring | 3h |
| ST-009 | Synthetic Bazel workload calibration | 4h |
| ST-010 | PRR meeting prep (checklists per role) | 4h |
| ST-011 | PRR meeting execution (2h) | 2h |
| ST-012 | 13 sign-off collection (PR review + git signing) | 3h |
| ST-013 | Cost regression gate setup + bench baseline | 3h |
| ST-014 | ADR ratificação confirmation (all 4 ACCEPTED) | 2h |
| ST-015 | SLA addendum customer doc | 3h |
| ST-016 | Release notes + Bazel onboarding doc | 4h |
| ST-017 | Production rollout plan | 3h |
| ST-018 | CI ship-gate workflow integration | 3h |
| ST-019 | Final review iteration (Architect + AppSec + Crypto SME + Security Lead) | 6h |
| ST-020 | Ship decision documentation + go/no-go | 1h |
| ST-021 | INV registry §3.15 update (19 INVs) | 2h |

**Total Optimistic**: ~71h. **PERT** (O=64h, M=72h, P=110h): **~78h**.

## 18. Dependencies

### Hard blockers

- WI-S04-001..005 SEALED.
- All 4 ADRs ratificadas.
- Crypto SME availability (final review).
- Architect + AppSec + Security Lead availability.
- 72h staging continuous availability.
- Wrangler + Grafana + PagerDuty configured.

### Soft blockers

- S-09 aggregation pipeline (cache hit ratio emit) — soft; minimal hook OK.
- S-16 customer dashboard — soft; placeholder integration.
- ADR-0034 staffing waiver path documented.

### Outbound

- Production rollout (gradual 10% → 50% → 100%).
- S-13 admin plane consumes operational maturity.
- S-15 CLI/SDK consumes API contract stability.
- S-20 GA consumes ship readiness.

## 19. Effort PERT

O: 64h, M: 72h, P: 110h → PERT **78h**.

## 20. Time-boxing

**90h hard limit**. Se exceder → escalation: split em "validation gates" + "PRR ceremony" + "comm prep" sub-WIs.

## 21. Observability

DASH-AC full dashboard (8 panels). Trace spans:
- `s04.ship_gate` — sub-gate result, duration.
- `prr.meeting` — sign-off counts, role pending.

Logs:
- INFO em ship gate sub-gate green.
- WARN em sub-gate yellow.
- ERROR em sub-gate red (blocker).
- AUDIT em sign-off collected.

## 22. Cost Analysis

**Sub-task cost** (one-time):
- REAPI conformance harness setup: dev time.
- Dashboard provisioning: Grafana/CF Analytics.
- 72h staging continuous: ~$10/dia × 3 = $30 staging cost.
- PRR meeting: human time (13 reviewers × 2h = 26h person-time).

**Operational cost** (steady state post-ship):
- Conformance suite nightly CI: ~$1/dia compute = $365/yr.
- Dashboard hosting: $10/mo Grafana = $120/yr.
- Property test 100k nightly: ~$1/dia compute = $365/yr.
- Total: ~$850/yr ship gate maintenance.

**Cost regression gate target**: aggregate per-op costs within Lote 9.4 §14.10 budget; +10% tolerance.

**TCO**:
- Setup: ~80h × $100/hr = $8k one-time.
- Maintenance: $850/yr.
- **Total: $8.85k Y1, $850/yr ongoing**.

## 23. API Contract

Public artifacts (consumed by external):
- SLA addendum customer doc.
- Release notes.
- Bazel onboarding doc.
- Conformance suite report (CI artifact).
- Cache hit ratio business métrica (S-09 → S-16 customer dashboard).

Internal artifacts:
- PRR meeting notes.
- Sign-off log (PR review + git signing).
- ADR ratificações.
- Production rollout plan.

## 24. Post-mortem Hooks

- Ship gate red (any sub-gate fails) → blocker post-mortem; iterate; re-test.
- 72h SLO incident reset clock → P1 incident analysis.
- RB-FM-303 dry-run gap → runbook iteration; re-run mandatory.
- Property test 100k violation → CRITICAL post-mortem (cross-tenant); ship blocked.
- Conformance suite regression → community engagement + Bazel maintainers.
- ADR ratificação rollback → 5-Why; senior review; revert blocked.
- Cost regression > 10% → bench drill-down; per-op decomposition.

## 25. Rollback / Recovery

- Production rollout rollback: 100% → 50% → 10% → 0% via Wrangler version revert.
- Sub-gate failure: ship delayed; specific sub-gate iteration; re-test.
- ADR ratificação rollback: requires ADR (transparency); re-review.
- RTO production rollback: ≤ 10 min.
- RPO: 0 (data preserved across rollback; AC entries stay).

Fallback: if production rollout shows incident, gradual rollback per documented plan; customer comm template ready.

## 26. Security & Privacy

**STRIDE delta** (cumulative S-04):
- **Spoofing**: TenantCtx-only enforcement (WI-001) + tenant binding sig (WI-004).
- **Tampering**: Merkle dual-side (WI-003) + HKDF sig (WI-004) + R2 ACL hardening (WI-002).
- **Repudiation**: outbox audit emission (WI-001/005); audit chain S-09.
- **Information disclosure**: Mann-Whitney constant-time (WI-001/004); tenant pseudonymous; PII redact policies.
- **DoS**: bounded parser (WI-003) + batch caps (WI-001/005); rate limit S-08 forward.
- **Elevation of privilege**: scope check (WI-001) + 5-layer defense (WI-001/003); admin override S-13 forward.

**LINDDUN delta** (cumulative):
- **Linkability**: tenant_id pseudonymous (UUID v7).
- **Identifiability**: ActionResult metadata customer-side responsability.
- **Non-repudiation**: append-only audit; sig chain.
- **Detectability**: dashboard alerts + chaos validation.
- **Disclosure of information**: dual-side cripto verify; bucket private.
- **Unawareness**: customer SLA addendum + release notes; ADRs published.
- **Non-compliance**: LGPD Art. 38 + GDPR Art. 32 + SLSA L3 partial.

## 27. Knowledge Transfer

- **Tech talk** (3h): "S-04 Action Cache GA — Architecture Overview + Operational Readiness".
- **Doc** `docs/customer/ac-feature-overview.md` — customer-facing.
- **Doc** `docs/internal/s04-architecture-overview.md` — internal.
- **Doc** `docs/internal/ac-runbooks-index.md` — RB-FM-303 + RB-FM-AC-TTL-DRIFT + others.
- **Workshop** (4h): com all reviewers + on-call team — operational walkthrough.
- **Onboarding test** (10 questions): cross-tenant prevention layers, REAPI conformance scope, RB-FM-303 procedure, ADR-0021 rationale, cost gate targets, customer SLA, rollout plan, signing infra, Merkle dual-side, TTL boundary.
- **External-facing**: blog post post-S-04 SEALED — "CoreLink Action Cache GA: How We Built It Right".

## 28. Risk Register (6-col)

| ID | Risco | Prob | Det | Impacto | Exposure | Residual | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | REAPI conformance regression silent | M | M | HIGH | M | LOW | Nightly CI; quarterly bump cadence; community engagement |
| R-002 | PRR sign-off rubber-stamp | M | M | HIGH | M | LOW | Per-role checklist evidence; ADR-0034 waiver transparency |
| R-003 | Property test 100k flake | L | H | LOW | L | LOW | Re-run 3× on flake; deterministic seeds |
| R-004 | RB-FM-303 dry-run gap | M | L | HIGH | L | LOW | Iterate + re-run; gap documented |
| R-005 | DASH-AC alert noisy | M | M | MEDIUM | M | LOW | Chaos test calibration; threshold tuning |
| R-006 | Cache hit ratio business métrica deviation | M | L | MEDIUM | L | LOW | Synthetic workload calibrated against real Bazel; per-tier expected baseline ADR-0019 |
| R-007 | 72h SLO incident reset cascade | L | M | MEDIUM | L | LOW | P3 minor does not reset; clear classification policy |
| R-008 | Cost regression gate flake | M | M | LOW | M | LOW | Bench warm-up + multiple runs; median; flake retry |
| R-009 | Crypto SME unavailable on ship date | M | L | MEDIUM | L | LOW | 2-week advance booking; advisory waiver path ADR-0034 |
| R-010 | Conformance suite fork attack | L | L | HIGH | L | LOW | CI uses upstream community-vetted; ADR for bumps |
| R-011 | Dashboard metric injection | L | L | HIGH | L | LOW | Metrics auth (S-09); anomaly detection |
| R-012 | Synthetic workload bias | L | L | MEDIUM | L | LOW | Workload reviewed Product + Architect; reproducible |
| R-013 | ADR ratificação rollback pressure | L | L | LOW | L | LOW | ADR change requires ADR; transparency; senior review |
| R-014 | Production rollout incident at 50% | M | M | HIGH | M | LOW | Gradual rollout; rollback procedure documented; customer comm template ready |
| R-015 | Customer SLA expectation mismatch | M | L | MEDIUM | L | LOW | SLA addendum reviewed Product + Compliance; conservative baselines |

## 29. Review Checkpoints

1. **Pre-PRR (D+0)**: ALL WI-S04-001..005 SEALED confirmed.
2. **Conformance + property test (D+1)**: 100% conformance + 100k green.
3. **72h SLO start (D+2)**: continuous staging deploy.
4. **DASH-AC live (D+3)**: dashboards + alerts validated.
5. **RB-FM-303 dry-run (D+4)**: execution + post-mortem.
6. **72h SLO complete (D+5)**: continuous run validates.
7. **PRR meeting (D+6)**: 2h structured; 13 sign-offs.
8. **Customer comm ready (D+7)**: SLA + release notes + onboarding.
9. **Ship decision (D+8)**: GO/NO-GO; if GO, start gradual rollout.
10. **Production 10% (D+9)**: monitor 24h.
11. **Production 50% (D+11)**: monitor 24h.
12. **Production 100% (D+13)**: full rollout.
13. **Post-ship review (D+20)**: 1-week review; post-mortem if incidents.

## 30. Sign-off (HIGH_RISK 13)

| # | Role | Name | Signed Date | Status |
|---|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | _pending_ | _pending_ |
| 2 | Final Approver | Gustavo Schneiter | _pending_ | _pending_ |
| 3 | SRE Lead | _staffing-blocked; ADR-0034 waiver via Architect compensation_ | _pending_ | _pending_ |
| 4 | Security Lead | _TBD; **mandatory** — 5-layer defense + AppSec checklist 100%_ | _pending_ | _pending_ |
| 5 | Engineer (peer 1) | _TBD; **mandatory** — code review depth + tests green_ | _pending_ | _pending_ |
| 6 | Engineer (peer 2) | _TBD; **mandatory** — independent code review_ | _pending_ | _pending_ |
| 7 | QA | _TBD; **mandatory** — integration + chaos suite executed; conformance green_ | _pending_ | _pending_ |
| 8 | Product | Gustavo Schneiter | _pending_ | _pending_ |
| 9 | Compliance | _TBD; **mandatory** — LGPD + GDPR + SLSA L3 alignment_ | _pending_ | _pending_ |
| 10 | Privacy | _TBD; **mandatory** — PII redaction + tenant pseudonymity_ | _pending_ | _pending_ |
| 11 | Architect | _TBD; **mandatory** — ADR ratificações + handler trait composability + non-exhaustive evolution_ | _pending_ | _pending_ |
| 12 | AppSec | _TBD; **mandatory emphatic** — tampering detection + sig integration + bucket ACL_ | _pending_ | _pending_ |
| 13 | Crypto SME (advisory) | _**MANDATORY EMPHATIC** — HKDF sig protocol + Merkle protocol + constant-time + key rotation; ADR-0021 endorsement_ | _pending_ | _pending_ |

**Sign-off discipline**:
- 12 mandatory sign-offs required.
- 1 advisory (Crypto SME) acceptable post-ship review per ADR-0034 waiver path.
- ADR-0034 documents staffing-blocked compensation patterns (Architect compensates SRE Lead concerns).
- Sign-off entries require checklist evidence; rubber-stamp prohibited.
- Sign-off via GitHub PR review + git commit signing.

## 31. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-25 | Gustavo (via Claude Opus 4.7) | Criação WI-S04-006 (Lote 10.4); SOTA pós-Lote 10.3bis (32 seções; 13-row sign-off; 15-row risk; cumulative INV validation 19 INVs §3.15; 4 ADRs ratificadas; 12 chaos experiments; STRIDE+LINDDUN cumulative delta; ship gate + production rollout plan). |

## 32. Anti-patterns evitados

- ❌ Ship without 100% conformance.
- ❌ Ship without 13 sign-offs.
- ❌ Ship without RB-FM-303 dry-run.
- ❌ Ship with 1+ tenant isolation violation in 100k.
- ❌ Ship with SLO < 99.9%.
- ❌ Ship with cost regression > 10%.
- ❌ Ship with ADR not ratificada.
- ❌ Ship with sign-off rubber-stamp.
- ❌ Ship with documentation gap (customer-facing).
- ❌ Ship with PRR meeting < 2h.
- ❌ Ship with chaos suite not executed.
- ❌ Production rollout direct 100% (gradual mandatory).
- ❌ Skip post-ship review.

---

**Fim WI-S04-006.** S-04 spec FULL SOTA completo (6 WIs HIGH_RISK; ~4400 linhas; 4 ADRs forward; 19 INVs forward §3.15).

**Próximo**: dispatch 2 agent reviews (parts 1+2) Lote 10.4 review cycle; apply Lote 10.4bis P0 fixes; commit.
