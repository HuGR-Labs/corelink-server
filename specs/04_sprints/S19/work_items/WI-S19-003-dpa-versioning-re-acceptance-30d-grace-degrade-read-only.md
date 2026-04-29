---
id: "WI-S19-003"
type: "work_item"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-29"
updated: "2026-04-29"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-009"]
parent: "S-19"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "PRIVACY-MODEL"
  - "RESILIENCE-PATTERNS"
  - "OBSERVABILITY-MODEL"
  - "FAILURE-MODES"
  - "INVARIANT-REGISTRY"
tags: ["wi", "s19", "onboarding", "dpa-versioning", "re-acceptance", "30d-grace", "degrade-read-only", "pat-degrade-001", "high-risk"]
---

# WI-S19-003 — DPA Versioning Semver Discipline (Major Bump = Material Change Requires Re-Acceptance; Minor/Patch = No Re-Acceptance) + Legal Review on Every PR Touching DPA + Quarterly Audit + Major Bump Triggers Existing Tenants Email Broadcast + 30d Grace Period to Re-Accept + In-App Banner Persistent During Grace + Re-Acceptance Failure Post-Grace → Tenant Degrade Read-Only PAT-DEGRADE-001 Reuse com Email + In-App Warning + Customer Support Escalation Path + v1→v2 Simulated Cycle Test em Staging + DPA Re-Acceptance Flow Sustained 1 Cycle GA Evidence Gate D+45 + Métricas Prometheus snake_case (re_acceptance_total{from_version, to_version, outcome} + grace_period_remaining_days_gauge + degraded_tenants_count_gauge) + RB-FM-DPA-LEGAL-CHALLENGE Stub em `specs/05_runbooks/`

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
> **Parent:** [S-19](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S19-003 |
| Título | DPA versioning semver + re-acceptance flow + 30d grace + degrade read-only post-grace + v1→v2 simulated cycle |
| Sprint | S-19 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-009 (DPA versioning é customer-facing legal contract; semver bump missed = retroactive consent invalidation) |

## 1. Intent

DPA versioning + re-acceptance flow é o **lifecycle management** do DPA legal contract entre CoreLink (Processor) e existing customers (Controller). Sem semver discipline rigorosa + re-acceptance flow para material changes, customer pode contestar legitimately em court que continuou usando service sob "old DPA" enquanto CoreLink já operava sob "new terms" sem proper notice + opt-in (GDPR Art. 7§3 freely-revoke + LGPD Art. 8§5 mudança de finalidade). Spec contract S-19 §5.2 R-S19-5 estabelece: (a) semver (major bump = material change requires re-acceptance); (b) major bump triggers existing tenants email broadcast + 30d grace period to re-accept; (c) re-acceptance failure após grace period → tenant degrade read-only com email + in-app warning. Este WI implementa todos 3 + v1→v2 simulated cycle test em staging.

```rust
// File: crates/corelink-onboarding-dpa-versioning/src/versioning.rs

#![forbid(unsafe_code)]

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum DpaVersionBumpKind {
    Major,        // Material change → re-acceptance required + 30d grace
    Minor,        // Non-material change → no re-acceptance
    Patch,        // Typo fix / clarification → no re-acceptance
}

#[async_trait]
pub trait DpaVersioning: Send + Sync {
    /// Triggered when new DPA version detected em legal/dpa/ folder via CI.
    /// Determines bump kind via semver diff; if Major → broadcast + grace.
    async fn on_dpa_version_bumped(
        &self,
        old_version: SemverVersion,
        new_version: SemverVersion,
        bump_kind: DpaVersionBumpKind,
    ) -> Result<DpaBumpReceipt, DpaVersioningError>;

    /// Email broadcast existing tenants on Major bump + 30d grace start.
    async fn broadcast_re_acceptance_required(
        &self,
        new_version: SemverVersion,
        grace_period_days: u32,             // canonical 30d
    ) -> Result<BroadcastReceipt, DpaVersioningError>;

    /// Daily cron checks tenants em grace period; degrades tenants post-expiry.
    async fn check_grace_expirations(&self) -> Result<GraceCheckReceipt, DpaVersioningError>;
}
```

## 2. Narrative (HIGH_RISK ≥ 300 palavras + risk justification)

DPA é um **versioned legal contract**, não um one-shot acceptance. CoreLink may need to update DPA terms over time (new sub-processors added; new regions; Schrems II legal landscape changes; breach notification SLA updated). Each update is one of three kinds: (a) major (material change requiring re-acceptance per GDPR Art. 7§3 + LGPD Art. 8§5); (b) minor (non-material change like adding optional clause); (c) patch (typo fix). Mistakes em determining kind = catastrophic legal exposure: shipping material change as patch = retroactive consent invalidation; shipping clarification as major = unnecessary customer friction + churn.

**Risk justification HIGH_RISK**:

- **FF-HR-009**: DPA versioning é customer-facing legal contract; semver bump missed for material change = retroactive consent invalidation; legal challenge legitimate em court.
- **Reversibility**: bug detected post-deploy (e.g., minor bump applied for material change) = catastrophic (existing customers retroactively re-prompted; customer trust loss; reputation damage; compliance audit failure).
- **Blast radius**: 100% existing tenants on major bump (could be 1k+ tenants at GA scale).

**Bugs catastróficos que este WI deve catch**:

- **Semver bump kind misidentified**: PR claims "patch" but content adds new sub-processor (material change); Legal review must catch; CI gate `dpa-bump-kind-justification` mandatory PR comment.
- **Email broadcast fail silently**: SES rate-limited ou bounced; tenants not notified; grace period starts mas customer doesn't know; degrade post-grace = surprise.
- **Grace period drift**: grace_period_days configured 30d mas cron checks expiration calculated com timezone bug (UTC vs local); tenants degraded prematurely OR late.
- **Degrade read-only too aggressive**: tenant degraded em prod traffic mid-request; in-flight CAS PUT fails; data loss potential.
- **Re-acceptance flow infinite loop**: tenant accepts new version mas backend doesn't update `dpa_acceptance_pending` flag; tenant prompted again on next request; UX broken.
- **Version skip**: tenant on v1.0 + v2.0 + v3.0 released sequentially within 30d; tenant accepts v3.0 but never v2.0; INV-CONSENT-PROOF-VERIFIABLE for v2.0 missing.

**Atacante adversarial scenarios validated**:

- **Bypass grace period**: pentester tries direct API call to mark tenant `dpa_acceptance_pending=false` without re-acceptance; verify endpoint requires explicit DPA POST.
- **Replay old DPA acceptance**: pentester captures v1.0 JWT receipt; tries submit as v2.0 acceptance; verify version match enforced.
- **Forge grace expiration ts**: pentester modifies D1 row `grace_expires_at`; verify D1 audit chain integrity catches.

**Mitigation**: Legal review on every PR touching DPA + CI gate `dpa-bump-kind-justification` mandatory; SES retry queue + bounce handling; cron timezone UTC canonical; degrade graceful (no in-flight request abort; PAT-DEGRADE-001 pattern); re-acceptance handler updates D1 row atomic; version skip prevented via "must re-accept latest available" flow; quarterly audit Legal Counsel.

## 3. Customer Impact & Journey

**Persona 1 — Existing customer (DPA re-acceptance flow)**:
- Email broadcast: "CoreLink DPA updated to v2.0; please review + re-accept by <grace_expires_at> (30 days from now)."
- In-app banner persistent: "DPA v2.0 effective <date>; please re-accept; <countdown> days remaining."
- Re-acceptance flow: same UI as initial DPA click-through (S-19 WI-S19-002 reuse); 6-field consent + JWT receipt + EVT-049 evidence.
- Failure post-grace: tenant degraded read-only; email + in-app warning + customer support escalation path.

**Persona 2 — Privacy Officer cliente / Compliance auditor**:
- DPA semver discipline audit: every PR touching DPA carries Legal review + bump kind justification + sign-off.
- Re-acceptance evidence: 6-field consent + JWT receipt + EVT-049 retention 7y.
- Quarterly audit Legal Counsel reviews DPA template lifecycle.

**Persona 3 — Internal Customer Success / Support**:
- Dashboard DASH-ONBOARDING painel "DPA Re-Acceptance Lifecycle": current version + tenants em grace + tenants degraded + re-acceptance rate per cohort.
- Customer support escalation path: degraded tenant calls support → Customer Success retrieves JWT receipt v1.0 + offers manual DPA v2.0 re-acceptance flow (assisted).

**SLA addendum**:
- Major bump email broadcast ≤ 1h after deploy (SES retry queue + bounce handling).
- Grace period 30d canonical (timezone UTC canonical).
- Degrade read-only graceful (no in-flight request abort; PAT-DEGRADE-001).
- Re-acceptance flow sustained 1 cycle (v1 → v2 mock) GA Evidence Gate D+45.

## 4. Capability Mapping

- **CAP-ONBOARD-007** (DPA versioning + re-acceptance) — IMPLEMENTA primary.
- Trace: `_spec_contract.md §5.2 R-S19-5` + `privacy_model.md §5.6 CTRL-PRIV-CONSENT-005 (notice versioning)` + `S-19 WI-S19-002 DPA click-through baseline` + `resilience_patterns.md PAT-DEGRADE-001`.

## 5. Tipo

Feature WI; HIGH_RISK; FF-HR-009; legal lifecycle.

## 6. Escopo

### 6.1 In-scope

1. **DPA semver discipline**:
   - Mdx frontmatter `notice_version` semver string canonical.
   - Bump kind taxonomy: major (material change requires re-acceptance) | minor (non-material) | patch (typo/clarification).
   - CI gate `dpa-bump-kind-justification` mandatory PR comment from Legal Counsel.
   - Quarterly audit Legal Counsel reviews DPA template lifecycle.

2. **Major bump email broadcast + 30d grace period**:
   - Trigger: `on_dpa_version_bumped(old, new, Major)` invoked via CI deploy hook.
   - Email broadcast existing tenants via SES (subject: "CoreLink DPA updated to v<new>; please review + re-accept by <date>"); body links to DPA v<new> + re-acceptance flow URL.
   - SES retry queue + bounce handling; alert > 5% bounce rate.
   - In-app banner persistent UI (S-16 frontend consume): "DPA v<new> effective <date>; please re-accept; <countdown> days remaining"; dismissible NOT (must re-accept ou degrade).
   - 30d grace period canonical (timezone UTC; configurable via DO config-singleton S-13 admin override em emergency).

3. **Re-acceptance flow** (reuse S-19 WI-S19-002):
   - User clicks "Re-accept DPA" link em email ou in-app banner.
   - Same UI as initial DPA click-through: render DPA v<new> + scroll-gate + 6-field consent capture + JWT receipt.
   - Backend updates D1 row `dpa_acceptance_pending=false` + `dpa_version_accepted=v<new>` atomic.
   - EVT-049 evidence emit retention 7y.

4. **Degrade read-only post-grace expiration** (PAT-DEGRADE-001 reuse):
   - Daily cron `check_grace_expirations` runs UTC midnight; queries `tenant WHERE dpa_acceptance_pending=true AND grace_expires_at < now()`.
   - For each expired tenant: degrade read-only (write operations rejected; read operations allowed); email + in-app warning emit; audit emit `corelink.onboarding.dpa_re_acceptance_grace_expired{tenant_id}`.
   - Customer support escalation path: degraded tenant retrieves support ticket auto-created + Customer Success contact info.
   - Recovery: tenant re-accepts DPA v<new> → degrade lifted automatic; audit emit `corelink.onboarding.dpa_re_acceptance_late`.

5. **Version skip prevention**:
   - Tenant on v1.0; v2.0 + v3.0 released sequentially within 30d.
   - Re-acceptance flow forces "must re-accept latest available" (v3.0); EVT-049 evidence emitted for v3.0 only (NOT v2.0; intentional — latest-wins canonical).
   - Audit emit `corelink.onboarding.dpa_version_skip{from, to}` for compliance trail.

6. **v1→v2 simulated cycle test em staging**:
   - Deploy v1.0 → existing tenants accept; deploy v2.0 (major bump) → broadcast + grace + re-acceptance flow.
   - Verify: 100% tenants notified; 100% accept ou degrade post-grace; recovery flow works; INV-CONSENT-PROOF-VERIFIABLE preserved.
   - Cadence: GA Evidence Gate D+45 (1 cycle); quarterly em production.
   - Output: `specs/_audits/2026-XX-XX-dpa-v1-v2-cycle-staging.md`.

7. **RB-FM-DPA-LEGAL-CHALLENGE stub** em `specs/05_runbooks/RB-FM-DPA-LEGAL-CHALLENGE.md`:
   - Scenario: customer challenges DPA term em court; Legal externo escalation; customer notify; retroactive re-acceptance broadcast se invalidated.
   - Decision tree: scope challenge (single customer vs class action) + Legal externo engagement + customer trust review + breach notification consideration.
   - Stub-only em S-19; full runbook deferred S-20 GA hardening.

8. **Métricas Prometheus snake_case underscored**:
   - `corelink_onboarding_dpa_re_acceptance_total{from_version, to_version, outcome, plan}` (outcome ∈ accepted|grace_expired_degraded|grace_expired_blocked).
   - `corelink_onboarding_dpa_grace_period_remaining_days{tenant_tier, plan}` (gauge per tier; aggregate p50/p99).
   - `corelink_onboarding_dpa_degraded_tenants_count{plan}` (gauge; alert > 5% existing tenants em prod).

### 6.2 Out-of-scope (deferred)

- DPA click-through 6-field consent (WI-S19-002).
- Tier selection + Stripe Checkout (WI-S19-004).
- Enterprise inquiry form (WI-S19-005).
- Conversion funnel instrumentation (WI-S19-006).
- Customer-managed DPA per customer (single template at GA).
- Multi-version concurrent (only latest version active; v1.0 + v2.0 + v3.0 released sequentially).

## 7. Anti-Scope

- Bump kind misidentification tolerated (Legal review CI gate mandatory).
- Email broadcast fail silently (SES retry + bounce handling required).
- Grace period < 30d (regulatory baseline).
- Degrade read-only too aggressive (in-flight request abort; PAT-DEGRADE-001 graceful required).
- Re-acceptance flow infinite loop (D1 row atomic update required).
- Version skip without "must re-accept latest" enforcement.
- Skip v1→v2 simulated cycle staging GA Evidence Gate.
- Skip quarterly audit Legal Counsel.

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: DPA versioning re-acceptance flow + 30d grace + degrade read-only post-grace

  Scenario: Major bump triggers email broadcast + 30d grace
    Given DPA v1.0.0 deployed; existing tenants accepted
    When DPA v2.0.0 major bump deployed
    Then on_dpa_version_bumped(v1.0.0, v2.0.0, Major) invoked
    And email broadcast sent to existing tenants via SES
    And in-app banner rendered persistent
    And grace_expires_at = now + 30d (UTC)

  Scenario: Re-acceptance flow within grace period
    Given tenant em grace period (15d remaining)
    When user clicks "Re-accept DPA" link
    Then DPA v2.0.0 click-through UI rendered (S-19 WI-S19-002 reuse)
    And 6-field consent captured + JWT receipt issued
    And D1 row updated dpa_acceptance_pending=false + dpa_version_accepted=v2.0.0

  Scenario: Grace period expires → tenant degrade read-only
    Given tenant did not re-accept; grace_expires_at < now()
    When daily cron check_grace_expirations runs
    Then tenant marked degraded read-only
    And email + in-app warning emit
    And customer support escalation path triggered (auto-ticket)
    And audit emit "corelink.onboarding.dpa_re_acceptance_grace_expired{tenant_id}"

  Scenario: Degrade read-only graceful (PAT-DEGRADE-001)
    Given tenant degraded mid-request
    Then in-flight CAS PUT completes successfully
    And subsequent CAS PUT requests rejected with 451 + warning message
    And read operations (CAS GET, AC READ) continue allowed

  Scenario: Recovery — tenant re-accepts post-degradation
    Given tenant degraded read-only
    When user clicks "Re-accept DPA" + completes flow
    Then degrade lifted automatic
    And audit emit "corelink.onboarding.dpa_re_acceptance_late"

  Scenario: Version skip — tenant on v1.0, v2.0 + v3.0 released sequentially
    Given tenant on v1.0; v2.0 + v3.0 released within 30d
    When user clicks "Re-accept DPA"
    Then DPA v3.0 (latest) rendered
    And EVT-049 evidence emit for v3.0 only
    And audit emit "corelink.onboarding.dpa_version_skip{from=v1.0, to=v3.0}"

  Scenario: Bump kind CI gate mandatory PR comment
    Given PR modifies legal/dpa/v1.0.0.en-US.md
    When CI runs
    Then dpa-bump-kind-justification flag triggered
    And merge blocked until Legal Counsel comments bump kind (major|minor|patch) + justification

  Scenario: SES email broadcast retry queue + bounce handling
    Given email broadcast sent to 1000 existing tenants
    When 5% bounce
    Then bounce queue processed; retry attempted; alert > 5% bounce rate

  Scenario: v1→v2 simulated cycle staging
    Given v1.0 deployed; tenants accepted
    When v2.0 major bump simulated em staging
    Then 100% tenants notified
    And 100% accept ou degrade post-grace
    And recovery flow works
    And INV-CONSENT-PROOF-VERIFIABLE preserved

  Scenario: Replay old DPA JWT receipt rejected
    Given pentester captures v1.0 JWT receipt
    When tries submit as v2.0 re-acceptance
    Then version mismatch detected
    And rejected 422 + audit emit "corelink.onboarding.dpa_version_replay"

  Scenario: Bypass grace period attempt rejected
    Given pentester tries direct API call mark dpa_acceptance_pending=false
    Then endpoint requires explicit DPA POST flow
    And rejected 401

  Scenario: Quarterly audit Legal Counsel review
    Given DPA template lifecycle 90d elapsed
    When quarterly audit runs
    Then Legal Counsel reviews DPA template + bump history
    And specs/_audits/2026-QQ-XX-dpa-quarterly-audit.md committed
```

## 9. Design Decisions

### 9.1 Why 30d grace period (não 15d ou 60d)

- 30d = balance between regulatory baseline (LGPD/GDPR not specifying minimum) and customer friction.
- 15d = aggressive; risk customer not noticing email; degrade surprise.
- 60d = lax; legal exposure window prolonged; competitor advantage if material change material.

### 9.2 Why degrade read-only graceful (PAT-DEGRADE-001 reuse)

- Hard 503 = data loss potential; in-flight CAS PUT fails; customer angry.
- Read-only graceful = read operations continue (existing data accessible); write rejected with clear 451 + warning + support escalation; customer can re-accept + continue.
- PAT-DEGRADE-001 pattern canonical (S-14 BYOK kill switch reuse).

### 9.3 Why version skip latest-wins (não require all intermediate)

- Sequential v1 → v2 → v3 within 30d uncommon; if happens, latest-wins reduces customer friction.
- INV-CONSENT-PROOF-VERIFIABLE preserved for v3 (latest); v2 EVT-049 evidence not required (intentional).
- Audit emit `dpa_version_skip` for compliance trail.

### 9.4 Why daily cron check (não real-time)

- Real-time check on every request = D1 query overhead per request; performance degraded.
- Daily cron UTC midnight = 1 query per tenant per day; scales to 100k+ tenants.
- Grace period 30d > 1 day cron cadence; no risk false-positive degradation.

### 9.5 Why CI gate dpa-bump-kind-justification mandatory

- Legal Counsel must explicitly classify bump kind (major|minor|patch) with justification.
- Prevents accidental material change shipped as patch (catastrophic legal exposure).
- Quarterly audit retroactively reviews bump history.

### 9.6 Why timezone UTC canonical

- Multi-region service; customer timezone varies; UTC canonical avoids ambiguity.
- Grace period 30d = 30 × 24h = 2,592,000 seconds (precise).

### 9.7 ADR potencial?

- Não. Patterns reused (PAT-DEGRADE-001 S-14 + semver discipline industry standard + cron daily). No novel architecture decision.

## 10. Completeness Criteria SOTA

- [ ] **10.s19.003.1** DPA semver discipline + bump kind taxonomy (major|minor|patch).
- [ ] **10.s19.003.2** Legal review CI gate `dpa-bump-kind-justification` mandatory PR comment.
- [ ] **10.s19.003.3** Major bump triggers email broadcast via SES + retry queue + bounce handling.
- [ ] **10.s19.003.4** In-app banner persistent during grace period.
- [ ] **10.s19.003.5** 30d grace period canonical UTC.
- [ ] **10.s19.003.6** Re-acceptance flow reuse S-19 WI-S19-002 (6-field consent + JWT receipt + EVT-049).
- [ ] **10.s19.003.7** Degrade read-only graceful PAT-DEGRADE-001 post-grace.
- [ ] **10.s19.003.8** Version skip latest-wins + audit emit.
- [ ] **10.s19.003.9** v1→v2 simulated cycle staging GA Evidence Gate D+45.
- [ ] **10.s19.003.10** RB-FM-DPA-LEGAL-CHALLENGE stub em `specs/05_runbooks/`.
- [ ] **10.s19.003.11** Quarterly audit Legal Counsel.
- [ ] **10.s19.003.12** Métricas Prometheus snake_case (3+ métricas).

## 11. DoD

- [ ] DPA versioning lifecycle tested staging.
- [ ] Email broadcast SES retry queue + bounce handling verified.
- [ ] Grace period UTC canonical verified.
- [ ] Degrade read-only graceful PAT-DEGRADE-001 verified.
- [ ] v1→v2 simulated cycle staging verified.
- [ ] CI gate `dpa-bump-kind-justification` enforced.
- [ ] Tests: unit (semver diff + bump kind + grace check) + integration (E2E v1→v2 cycle) + 4+ negative scenarios.

## 12. Invariants Validated

- **CTRL-PRIV-CONSENT-005** (notice versioning) — IMPLEMENTA primary; semver discipline + Legal review.
- **INV-CONSENT-PROOF-VERIFIABLE** (CRITICAL — registry §3.12 herdada S-11): preserved across version bumps.
- Não introduz INV nova (per spec contract §8 — INV-ONBOARD-DPA-FIRST + INV-ONBOARD-ATOMIC-PROVISIONING declared em WI-S19-001 + WI-S19-004).

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| DPA versioning crate | `crates/corelink-onboarding-dpa-versioning/` | Rust |
| Bump kind detection | `crates/corelink-onboarding-dpa-versioning/src/bump_kind.rs` | Rust |
| Email broadcast worker | `crates/corelink-onboarding-dpa-versioning/src/broadcast.rs` | Rust |
| Grace check cron | `crates/corelink-onboarding-dpa-versioning/src/grace_check.rs` | Rust |
| Degrade read-only | `crates/corelink-onboarding-dpa-versioning/src/degrade.rs` | Rust |
| In-app banner UI | `apps/web/src/app/dpa-banner/page.tsx` | TypeScript |
| CI gate script | `scripts/check_dpa_bump_kind.py` | Python |
| RB stub | `specs/05_runbooks/RB-FM-DPA-LEGAL-CHALLENGE.md` | Markdown |
| v1→v2 cycle staging report | `specs/_audits/2026-XX-XX-dpa-v1-v2-cycle-staging.md` | Markdown |

## 14. Quality Standards SOTA

- **14.s19.003.1** Test coverage ≥ 90% (semver diff + bump kind + grace check + degrade).
- **14.s19.003.2** SAST: cargo-audit + cargo-deny clean.
- **14.s19.003.3** SES email deliverability ≥ 95%; bounce rate < 5%.
- **14.s19.003.4** Grace period precision UTC seconds.
- **14.s19.003.5** Degrade graceful (in-flight request not aborted).
- **14.s19.003.6** v1→v2 cycle staging completed ≤ 30 dias (canonical period); GA Evidence Gate D+45.
- **14.s19.003.7** Quarterly audit Legal Counsel cadence.

## 15. Chaos Experiments

1. **v1→v2 simulated cycle staging**: full lifecycle test.
2. **Bump kind misidentification attempt**: PR ships material change as patch; CI gate catches.
3. **Email broadcast bounce 50%**: simulate SES throttle; verify retry queue + bounce handling.
4. **Grace period drift**: synthesize timezone bug; verify UTC canonical correctness.
5. **Degrade mid-request**: trigger degrade during in-flight CAS PUT; verify graceful completion.
6. **Re-acceptance infinite loop**: synthesize backend update bug; verify D1 atomic.
7. **Version skip 3 versions in 30d**: verify latest-wins canonical.
8. **Bypass grace period attempt**: pentester direct D1 update; verify audit chain integrity.

## 16. PRR (Production Readiness Review)

PRR HIGH_RISK 11 sign-offs canonical (sprint S-19 §14):

- [ ] All Gherkin green.
- [ ] DPA semver discipline + Legal review CI gate.
- [ ] v1→v2 simulated cycle staging.
- [ ] Degrade read-only graceful.
- [ ] Métricas DASH-ONBOARDING emitting.
- [ ] 11 sign-offs canonical documented.

## 17. Sub-tasks

| ID | Sub-task | Estimativa |
|---|---|---|
| ST-001 | Semver bump kind detection logic | 2h |
| ST-002 | Email broadcast worker SES + retry queue + bounce handling | 4h |
| ST-003 | In-app banner UI persistent | 2h |
| ST-004 | Grace period cron UTC + degrade logic | 4h |
| ST-005 | Re-acceptance flow integration WI-S19-002 reuse | 2h |
| ST-006 | v1→v2 simulated cycle staging | 3h |
| ST-007 | CI gate dpa-bump-kind-justification | 1h |
| ST-008 | RB-FM-DPA-LEGAL-CHALLENGE stub | 1h |

**Total Optimistic**: ~19h. **PERT** (O=12h, M=18h, P=30h per spec contract §12): **19.0h**.

## 18. Dependencies

### Hard blockers
- WI-S19-002 SEALED (DPA click-through baseline; re-acceptance flow reuse).
- S-11 SEALED (consent ledger D1 + verify endpoint).
- S-13 SEALED (admin plane DO config-singleton para grace_period_days override).
- Legal Counsel availability for CI gate review + quarterly audit.

### Soft blockers
- S-09 SEALED (audit chain R2 EVT-049).
- S-14 SEALED (PAT-DEGRADE-001 pattern canonical reuse).

### Outbound
- WI-S19-006 (closing PRR + sustained 1 cycle GA Evidence Gate D+45).

## 19. Effort PERT

O: 12h, M: 18h, P: 30h → PERT **19.0h** (per spec contract §12; semver + email broadcast + grace + degrade + cycle).

## 20. Time-boxing

**22h hard limit owner**. v1→v2 cycle staging **3h dedicated**. Se exceder: split em sub-WI (versioning logic vs lifecycle test).

## 21. Observability

Métricas Prometheus snake_case underscored:

- `corelink_onboarding_dpa_re_acceptance_total{from_version, to_version, outcome, plan}` (outcome ∈ accepted|grace_expired_degraded|grace_expired_blocked).
- `corelink_onboarding_dpa_grace_period_remaining_days{tenant_tier, plan}` (gauge per tier; aggregate p50/p99).
- `corelink_onboarding_dpa_degraded_tenants_count{plan}` (gauge; alert > 5% existing tenants).
- `corelink_onboarding_dpa_email_broadcast_total{outcome, plan}` (outcome ∈ sent|bounced|retried).
- `corelink_onboarding_dpa_version_skip_total{plan}` (counter).

Dashboard DASH-ONBOARDING painel "DPA Re-Acceptance Lifecycle" (4-panel: re-acceptance rate per cohort + grace remaining distribution + degraded tenants count + version skip events).

## 22. Cost Analysis

- SES email broadcast: ~$0.10 / 1000 emails × estimated 10k tenants = $1 per major bump (occasional).
- Cron daily check: ~$5/mês CI compute.
- Total: ~$10/mês incremental.

## 23. API Contract

- `POST /api/onboarding/dpa-re-accept` — re-acceptance flow (reuse WI-S19-002 endpoint with version-aware logic).
- Internal: `DpaVersioning::on_dpa_version_bumped` Rust trait + cron `check_grace_expirations` daily.

## 24. Post-mortem Hooks

- DPA semver bump missed for material change → CRITICAL post-mortem + Legal + retroactive re-acceptance broadcast.
- Email broadcast bounce > 5% sustained → SEV-2 + SES configuration review.
- Degrade read-only too aggressive (in-flight aborted) → SEV-2 + PAT-DEGRADE-001 review.
- Version skip event + INV violation → CRITICAL + retroactive evidence cycle.
- Quarterly audit findings P1 → fix + re-test.
- Customer challenges DPA legitimately → invoke RB-FM-DPA-LEGAL-CHALLENGE.

## 25. Rollback / Recovery

DPA versioning regression → revert via CF Workers rollback; existing acceptances retained; degrade lifted manually by Customer Success.

## 26. Security & Privacy

**STRIDE delta**:
- **Spoofing**: re-acceptance flow requires Clerk-authenticated tenant.
- **Tampering**: D1 atomic update prevents partial state; audit chain integrity.
- **Repudiation**: 6-field consent + JWT receipt + EVT-049 retention 7y per re-acceptance event.
- **Information disclosure**: email broadcast sanitized (no secrets em URL).
- **DoS**: cron daily UTC midnight = bounded load.
- **Elevation of privilege**: degrade read-only graceful (no hard 503).

**LINDDUN delta**:
- **Linkability**: tenant_id em audit (compliance accountability).
- **Identifiability**: email broadcast via tenant.user_email (intentional notification).
- **Non-repudiation**: 6-field consent + JWT receipt + audit chain trail.
- **Detectability**: bounce rate alerted; degrade events alerted.
- **Disclosure**: DPA text public per locale.
- **Unawareness**: customer notified via email + in-app banner + grace countdown.
- **Non-compliance**: GDPR Art. 7§3 + LGPD Art. 8§5 satisfied.

## 27. Knowledge Transfer

- Tech talk (1h): "CoreLink DPA Versioning Lifecycle — Semver + 30d Grace + Degrade Read-Only".
- Doc `docs/internal/dpa-versioning-lifecycle.md`.
- Onboarding test (3 questions): bump kind taxonomy + grace period + degrade graceful.

## 28. Sign-off (HIGH_RISK 11 canonical)

| # | Role | Name | Status |
|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | _pending_ |
| 2 | Final Approver | Gustavo Schneiter | _pending_ |
| 3 | Architect (Privacy + Legal SME) | _TBD_ | _pending_ |
| 4 | Privacy Officer | _TBD_ | _pending_ |
| 5 | Legal Counsel | _TBD_ | _pending_ |
| 6 | Engineer (S-19 lead) | _TBD_ | _pending_ |
| 7 | QA Lead | _TBD_ | _pending_ |
| 8 | Product | Gustavo Schneiter | _pending_ |
| 9 | SRE Lead | _TBD_ | _pending_ |
| 10 | Compliance Officer | _TBD_ | _pending_ |
| 11 | Sales lead | _TBD_ | _pending_ |

## 29. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-29 | Gustavo (via Claude Opus 4.7) | Criação WI-S19-003 (cycle 12.S19.0; DPA versioning semver + 30d grace + degrade read-only graceful + v1→v2 cycle). |

## 30. Anti-patterns evitados

- Bump kind misidentification tolerated.
- Email broadcast fail silently.
- Grace period < 30d.
- Degrade read-only too aggressive.
- Re-acceptance flow infinite loop.
- Version skip without latest-wins enforcement.
- Skip v1→v2 cycle staging.
- Skip quarterly audit.

---

**Fim WI-S19-003.**
