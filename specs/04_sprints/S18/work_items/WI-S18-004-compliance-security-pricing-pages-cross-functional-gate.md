---
id: "WI-S18-004"
type: "work_item"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-29"
updated: "2026-04-29"
lane: "LOW_RISK"
parent: "S-18"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "COMPLIANCE-MATRIX"
  - "PRIVACY-MODEL"
  - "SECURITY-MODEL"
tags: ["wi", "s18", "docs", "compliance", "security", "pricing", "sbom", "cosign", "slsa-l3", "rekor", "pentest", "cross-functional-gate", "low-risk"]
---

# WI-S18-004 — Compliance + Security + Pricing Pages com **Cross-functional Review Gate Canonical Mandatory** (per Spec Contract §9.6 + §10 Anti-scope Estrito + Waiver Policy §19 Non-skippable): `/pricing` 5 Tiers (Free 10 GB / Starter 100 GB / Team 1 TB / Pro 10 TB / Enterprise unlimited per Spec Contract §5.5 R-S18-9) + Feature Matrix + Pricing Calculator Usage-based Validated 10 Sample Scenarios (per Completeness Criteria 10.s18.5 EVT-044) + **Finance + Legal Sign-off Mandatory** (R-S18-11); `/security` SBOM Downloadable Link Release SBOM (S-12 CycloneDX 1.5+) + Verification Instructions Cosign Verify + SLSA L3 Attestation Lookup via Rekor Query + Pentest Exec Summary Public-safe (High-level Findings; No Specific CVE before Disclosure) + SLA Published Terms + SOC 2 Timeline Gap Analysis Status (S-20 Deliverable) + **Security Lead + Privacy Officer Sign-off Mandatory**; `/compliance` LGPD Compliance Statement + GDPR (Lawful Basis per Processing) + CCPA/CPRA Compliance + DPA Template Link + Sub-processors List Link (S-16 Auto-generated Reuse) + Privacy Notice Link (S-16 Reuse) + **Legal + Privacy Officer Sign-off Mandatory**; Zero Customer Data Real em Screenshots/Examples (Fixture Pipeline; CTRL-PRIV-001 Enforced; CI Lint Check via Grep for Known PII Patterns)

> **doc_status:** DRAFT · **work_status:** READY · **lane:** LOW_RISK
> **Parent:** [S-18](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S18-004 |
| Título | Compliance + security + pricing pages com cross-functional review gate canonical mandatory. |
| Sprint | S-18 |
| Lane | LOW_RISK |
| Forcing factors | none (LOW_RISK; compliance page LINKS canonical sources não introduz claims novos; SBOM access reuses S-12 SEALED CycloneDX 1.5+; sub-processors + privacy notice S-16 SEALED auto-generated reuse; cross-functional review gate process discipline non-skippable mas não cripto-load-bearing) |

## 1. Intent

Entrega **3 páginas sensíveis** em `apps/docs/docs/` com **cross-functional review gate canonical mandatory** (per spec contract §9.6 + §10 anti-scope estrito + Waiver policy §19 non-skippable):

1. **`/pricing`** com 5 tiers (Free 10 GB / Starter 100 GB / Team 1 TB / Pro 10 TB / Enterprise unlimited per spec contract §5.5 R-S18-9) + feature matrix + pricing calculator usage-based input GB/mo + tier → estimated cost validated 10 sample scenarios (per Completeness Criteria 10.s18.5 EVT-044) + **Finance + Legal sign-off mandatory** antes publish (R-S18-11; non-skippable per Waiver policy §19).

2. **`/security`** SBOM downloadable link release SBOM (S-12 CycloneDX 1.5+) + verification instructions Cosign verify + SLSA L3 attestation lookup via Rekor query + pentest exec summary public-safe (high-level findings; no specific CVE before disclosure per spec contract §15 row 8) + SLA published terms + SOC 2 timeline gap analysis status (S-20 deliverable per spec contract §5.4 R-S18-7) + **Security lead + Privacy Officer sign-off mandatory** antes publish.

3. **`/compliance`** LGPD compliance statement + GDPR compliance statement (lawful basis per processing) + CCPA/CPRA compliance + DPA template link + sub-processors list link (S-16 auto-generated reuse) + privacy notice link (S-16 reuse per spec contract §5.4 R-S18-8) + **Legal + Privacy Officer sign-off mandatory** antes publish.

Zero customer data real em screenshots/examples (per Quality Standard 14.s18.9; fixture pipeline ensures no real PII; test fixtures only; CI lint check via grep for known PII patterns; reuse from WI-S18-002 + WI-S18-003).

```typescript
// File: apps/docs/src/components/PricingCalculator.tsx (excerpt)
const TIERS = {
  free: { storage_gb: 10, egress_gb: 100, pats: 1, sla: null, byok: false, price_usd: 0 },
  starter: { storage_gb: 100, egress_gb: 1000, pats: 5, sla: 'basic', byok: false, price_usd: 'X' },
  team: { storage_gb: 1000, egress_gb: 10000, pats: 20, sla: '99.9%', byok: false, price_usd: 'Y' },
  pro: { storage_gb: 10000, egress_gb: 100000, pats: 'unlimited', sla: '99.95%', byok: true, price_usd: 'Z' },
  enterprise: { storage_gb: 'unlimited', egress_gb: 'unlimited', pats: 'unlimited', sla: 'custom', byok: true, dpa: true, sso: true, price_usd: 'Contact' },
};

// Validated 10 scenarios per Completeness Criteria 10.s18.5 EVT-044 + Finance sign-off
function calculateCost(usageGB: number, tier: keyof typeof TIERS): number | string {
  // 10 sample scenarios: small dev (10 GB) / medium team (500 GB) / large team (5 TB) / enterprise (50 TB) / etc
  // ... Finance sign-off mandatory before publish
}
```

```markdown
<!-- File: apps/docs/docs/security.mdx (excerpt) -->
# Security

## SBOM Access (Public)

Download CoreLink Server SBOM (CycloneDX 1.5+ per S-12 SEALED):
- [Latest SBOM (.cdx.json)](https://releases.corelink.humangr.com/latest/sbom.cdx.json)

### Verification Instructions

```bash
# Cosign verify (signed SBOM; SLSA L3 attestation)
cosign verify-blob \
  --signature sbom.cdx.json.sig \
  --certificate sbom.cdx.json.cert \
  --certificate-identity-regexp "^https://github.com/humangr-labs/corelink-server" \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com \
  sbom.cdx.json
```

### SLSA L3 Attestation Lookup via Rekor

```bash
# Query Rekor transparency log
rekor-cli search --artifact sbom.cdx.json
rekor-cli get --uuid <uuid>
```

## Pentest Exec Summary (Public-safe)

Last pentest: 2026-Q1 (high-level findings; no specific CVE before disclosure per S-18 spec contract §15 row 8):
- 0 critical findings
- 2 medium findings (remediated; CVE disclosed responsibly)
- ...
```

## 2. Narrative

Cross-functional review gate canonical é o discipline-strict pattern para pages com regulatory/financial/security implications. Per spec contract §9.6 + §10 anti-scope estrito + Waiver policy §19 non-skippable: **qualquer page tocando pricing requires Finance + Legal review; security/compliance requires Privacy Officer + Security lead. Sem review = não merge**. Process discipline strict não skippable.

3 páginas sensíveis:
- **`/pricing`** = 5 tiers + feature matrix + calculator validated 10 scenarios + Finance + Legal sign-off.
- **`/security`** = SBOM downloadable + Cosign verify + SLSA L3 Rekor lookup + pentest exec summary + SLA + SOC 2 timeline + Security lead + Privacy Officer sign-off.
- **`/compliance`** = LGPD + GDPR + CCPA + DPA + sub-processors + privacy notice + Legal + Privacy Officer sign-off.

Compliance page LINKS canonical sources (não introduz claims novos): privacy_model.md + compliance_matrix.md + sub-processors auto-generated S-16 SEALED reuse. Avoids drift via single source of truth canonical.

SBOM access SOTA-baseline rare em competitors (BuildBuddy email request com NDA only; CoreLink openly downloadable + verification instructions Cosign verify + SLSA L3 attestation via Rekor query). Vantagem competitive strict.

Pentest exec summary public-safe (high-level findings; no specific CVE before disclosure per spec contract §15 row 8). Security lead review antes publish.

Pricing calculator validated 10 sample scenarios (per Completeness Criteria 10.s18.5 EVT-044): small dev (10 GB) + medium team (500 GB) + large team (5 TB) + enterprise (50 TB) + etc. Finance sign-off mandatory before publish (R-S18-11; non-skippable per Waiver policy §19).

Zero customer data real em screenshots/examples per Quality Standard 14.s18.9 (fixture pipeline ensures no real PII; test fixtures only; CI lint check via grep for known PII patterns; reuse from WI-S18-002 + WI-S18-003).

**Risk justification LOW_RISK lane (zero forcing factors)**:
- Compliance page LINKS canonical sources (não introduz claims novos); compliance_matrix.md + privacy_model.md já SEALED.
- SBOM access reuses S-12 SEALED CycloneDX 1.5+ + Cosign verify + SLSA L3 attestation Rekor.
- Sub-processors + privacy notice S-16 SEALED auto-generated reuse.
- Cross-functional review gate process discipline non-skippable mas não cripto-load-bearing.
- Não introduz tenant data flow path novo.

## 3. Customer Impact & Journey

**Persona — Procurement / Compliance auditor (enterprise prospect)**:
- SBOM downloadable (S-12 CycloneDX 1.5+) + Cosign verify instructions + SLSA L3 attestation lookup via Rekor query (rare em competitors; vantagem strict).
- Pentest exec summary public-safe (high-level findings; no specific CVE before disclosure).
- SOC 2 timeline gap analysis status (S-20 deliverable; transparent communication).
- LGPD + GDPR (lawful basis per processing) + CCPA/CPRA compliance statements aligned canonical sources.
- DPA template link + sub-processors list link (S-16 auto-generated reuse) + privacy notice link.

**Persona — Finance / Legal counsel**:
- Pricing transparency 5 tiers (Free/Starter/Team/Pro/Enterprise) + feature matrix + calculator validated 10 scenarios.
- Pricing changes via PR + cross-functional review only (not real-time API; intentional).
- Cross-functional review gate canonical: Finance + Legal sign-off pricing; Legal + Privacy Officer compliance; Security lead + Privacy Officer security — non-skippable per Waiver policy §19.

## 4. Capability Mapping

- **CAP-DOCS-004** (compliance & security page) — IMPLEMENTA primary.
- **CAP-DOCS-005** (pricing page) — IMPLEMENTA primary.
- Trace: `_spec_contract.md §4` + `compliance_matrix.md` (SOC 2 timeline + LGPD/GDPR/CCPA statements) + `privacy_model.md` (privacy notice + sub-processors) + `security_model.md` (SBOM + pentest exec summary).

## 5. Tipo

Content WI sensitive cross-functional gate; LOW_RISK lane.

## 6. Escopo

### 6.1 In-scope

1. **`/pricing` page** em `apps/docs/docs/pricing.mdx`:
   - **5 tiers** (per spec contract §5.5 R-S18-9):
     - **Free**: 10 GB storage, 100 GB egress, 1 PAT, no SLA.
     - **Starter** ($X/mo; Finance sign-off): 100 GB, 1 TB egress, 5 PATs, basic SLA.
     - **Team** ($Y/mo; Finance sign-off): 1 TB, 10 TB egress, 20 PATs, 99.9% SLA.
     - **Pro** ($Z/mo; Finance sign-off): 10 TB, 100 TB egress, unlimited PATs, 99.95% SLA, BYOK.
     - **Enterprise** (Contact): unlimited, custom SLA, BYOK, DPA, SSO, dedicated support.
   - **Feature matrix**: tier × feature comparison (storage / egress / PATs / SLA / BYOK / DPA / SSO / dedicated support).
   - **Pricing calculator** usage-based: input GB/mo + tier → estimated cost (R-S18-10).
   - **Validated 10 sample scenarios** (per Completeness Criteria 10.s18.5 EVT-044):
     - Scenario 1: small dev solo (10 GB) → Free.
     - Scenario 2: dev team 5 (50 GB) → Starter.
     - Scenario 3: medium team 20 (500 GB) → Team.
     - Scenario 4: large team 50 (5 TB) → Pro.
     - Scenario 5: enterprise 200 (50 TB) → Enterprise.
     - Scenario 6: CI-heavy team (high egress 5 TB/mo) → Pro com egress overage.
     - Scenario 7: BYOK requirement (custom KMS) → Pro/Enterprise.
     - Scenario 8: regional residency (EU only) → Enterprise.
     - Scenario 9: SOC 2 procurement requirement → Enterprise (SLA + DPA + audit-ready).
     - Scenario 10: high availability requirement (99.99%) → Enterprise (custom SLA).
   - **Finance + Legal sign-off mandatory** antes publish (R-S18-11; non-skippable per Waiver policy §19).
   - **Cross-functional gate CF-1** (per sprint.md §6): Finance (5 tiers + calculator validated 10 scenarios + EVT-044) + Legal (terms + DPA reference).

2. **`/security` page** em `apps/docs/docs/security.mdx`:
   - **SBOM access** (per spec contract §5.4 R-S18-7):
     - Link para release SBOM (S-12 CycloneDX 1.5+ SEALED reuse).
     - Verification instructions: Cosign verify (`cosign verify-blob` com `--certificate-identity-regexp` + `--certificate-oidc-issuer`).
     - SLSA L3 attestation lookup via Rekor query (`rekor-cli search --artifact` + `rekor-cli get --uuid`).
   - **Pentest exec summary** public-safe (high-level findings; no specific CVE before disclosure per spec contract §15 row 8):
     - Last pentest date + scope.
     - Findings count by severity (critical / high / medium / low).
     - Remediation status (no specific CVE before disclosure).
     - Security lead review antes publish (Quality Standard 14.s18.6).
   - **SLA** published terms (uptime + latency + support response time).
   - **SOC 2 timeline gap analysis status** (S-20 deliverable; transparent communication).
   - **Security lead + Privacy Officer sign-off mandatory** antes publish.
   - **Cross-functional gate CF-2** (per sprint.md §6): Security lead (SBOM + Cosign verify + SLSA L3 + pentest summary) + Privacy Officer (CTRL-PRIV-001 enforced + zero PII em examples).

3. **`/compliance` page** em `apps/docs/docs/compliance.mdx`:
   - **LGPD compliance statement** (Brazil regulatory; LGPD primary per S-11 + S-16 alignment).
   - **GDPR compliance statement** (lawful basis per processing).
   - **CCPA/CPRA compliance statement** (California).
   - **DPA template link** (Data Processing Addendum download or request).
   - **Sub-processors list link** (S-16 auto-generated reuse).
   - **Privacy notice link** (S-16 reuse).
   - **Legal + Privacy Officer sign-off mandatory** antes publish.
   - **Cross-functional gate CF-3** (per sprint.md §6): Legal (LGPD + GDPR + CCPA statements aligned canonical compliance_matrix.md) + Privacy Officer (privacy notice + sub-processors S-16 auto-generated reuse).
   - LINKS canonical sources (não introduz claims novos; alignment com compliance_matrix.md + privacy_model.md SEALED).

4. **Zero customer data em screenshots/examples** (per Quality Standard 14.s18.9):
   - Fixture pipeline ensures no real PII (test fixtures only; reuse from WI-S18-002 + WI-S18-003).
   - CI lint check via grep for known PII patterns (reuse).
   - PAT format placeholder `corelink_dev_t_xxx.xxx.xxx` (S-03 decision (a)).
   - Tenant_id placeholder `acme-corp` (never real tenant ID).

5. **Categorization Diátaxis discipline**:
   - `/pricing` → standalone navbar link (não Diátaxis category; pricing/legal/security são standalone pages).
   - `/security` → standalone navbar link.
   - `/compliance` → standalone navbar link.
   - PR review checks (Docs lead + cross-functional sign-off per page).

### 6.2 Out-of-scope (deferred)

- WCAG 2.2 AA axe-core CI gate (WI-S18-005).
- Lighthouse ≥ 95 CI gate (WI-S18-005).
- Vale tone consistency lint CI (WI-S18-005).
- lychee broken-link CI (WI-S18-005).
- UX research session 5 dev sample (WI-S18-005).
- i18n native speaker review (WI-S18-005).
- Real-time pricing API (anti-scope per spec contract §10; pricing changes via PR + cross-functional review only).
- Customer case studies (anti-scope pre-GA; S-20).

## 7. Anti-Scope

- Skip cross-functional review gate (CRITICAL gap; non-skippable per Waiver policy §19; per spec contract §10 anti-scope estrito).
- Pricing claim sem Finance + Legal sign-off (CRITICAL gap; per spec contract §10 + R-S18-11).
- Security claim sem Security lead + Privacy Officer sign-off (CRITICAL gap; Quality Standard 14.s18.6).
- Compliance claim sem Legal + Privacy Officer sign-off (CRITICAL gap; alignment com compliance_matrix.md + privacy_model.md).
- Real customer data em screenshots/examples (CTRL-PRIV-001 violation; fixture pipeline + CI lint check).
- SBOM com sensitive vendor info inadvertent (per spec contract §15 row 7; Security lead review antes publish).
- Pentest exec summary com specific CVE before disclosure (per spec contract §15 row 8; Security lead review).
- Real-time pricing API (anti-scope per spec contract §10; pricing changes via PR + cross-functional review only).

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: Compliance + security + pricing pages com cross-functional review gate canonical mandatory

  Scenario: Pricing page 5 tiers + feature matrix + calculator validated 10 scenarios
    Given apps/docs/docs/pricing.mdx published
    When user views pricing page
    Then 5 tiers documented (Free/Starter/Team/Pro/Enterprise)
    And feature matrix tier × feature comparison rendered
    And pricing calculator usage-based (input GB/mo + tier → estimated cost)
    And 10 sample scenarios validated (per Completeness Criteria 10.s18.5 EVT-044)

  Scenario: Pricing page Finance + Legal sign-off mandatory (cross-functional gate CF-1; Lote 10.18 codex P0 hard merge control)
    Given pricing page PR submitted (touches `apps/docs/docs/pricing.mdx` ou `PricingCalculator.tsx`)
    When PR review
    Then GitHub CODEOWNERS file (`.github/CODEOWNERS`) requires `@finance-team` + `@legal-team` review for paths `apps/docs/docs/pricing.mdx` + `apps/docs/src/components/PricingCalculator.tsx`
    And GitHub branch protection rule on `main` requires CODEOWNERS approval (cannot merge sem matching reviewer)
    And CI check `validate_cross_functional_signoffs.py` parses `specs/04_sprints/S18/cross-functional-signoffs.md` para confirm both sign-offs entries presentes; missing = exit 1 = PR fails
    And both controls (CODEOWNERS + CI) hard-fail mergeability — markdown log alone (codex P0 finding) é insufficient; combined enforcement é canonical Lote 10.18 codex P0 fix

  Scenario: Security page SBOM downloadable + Cosign verify + SLSA L3 Rekor
    Given apps/docs/docs/security.mdx published
    When user views security page
    Then SBOM link to S-12 CycloneDX 1.5+ release
    And verification instructions Cosign verify present
    And SLSA L3 attestation lookup via Rekor query present

  Scenario: Security page pentest exec summary public-safe
    Given security page pentest section
    When user reads
    Then high-level findings present (count by severity)
    And no specific CVE before disclosure (per spec contract §15 row 8)
    And Security lead review required antes publish

  Scenario: Security page Security lead + Privacy Officer sign-off mandatory (CF-2)
    Given security page PR submitted
    When PR review
    Then Security lead sign-off required (SBOM + pentest summary)
    And Privacy Officer sign-off required (CTRL-PRIV-001 enforced)
    And PR fails until both sign-offs collected

  Scenario: Compliance page LGPD + GDPR + CCPA + DPA + sub-processors + privacy notice
    Given apps/docs/docs/compliance.mdx published
    When user views compliance page
    Then LGPD compliance statement present
    And GDPR compliance statement (lawful basis per processing) present
    And CCPA/CPRA compliance statement present
    And DPA template link present
    And sub-processors list link (S-16 auto-generated reuse) present
    And privacy notice link (S-16 reuse) present

  Scenario: Compliance page Legal + Privacy Officer sign-off mandatory (CF-3)
    Given compliance page PR submitted
    When PR review
    Then Legal sign-off required (LGPD + GDPR + CCPA aligned canonical compliance_matrix.md)
    And Privacy Officer sign-off required (privacy notice + sub-processors)
    And PR fails until both sign-offs collected

  Scenario: Zero customer data em screenshots/examples (CTRL-PRIV-001)
    Given any screenshot or example em compliance/security/pricing pages
    When CI lint check via grep runs
    Then no real PII detected
    And no real PAT detected (CTRL-CRED-001)
    And fixture pipeline placeholder format enforced

  Scenario: Cross-functional review gate bypassed attempt (waiver attempt)
    Given pricing/security/compliance page PR sem cross-functional sign-off
    When developer attempts merge
    Then PR fails (non-skippable per Waiver policy §19)
    And CRITICAL post-mortem trigger se merge inadvertent (per spec contract §18)

  Scenario: SBOM access verification instructions tested (per Completeness Criteria 10.s18.4)
    Given user follows SBOM verification instructions
    When cosign verify-blob runs
    Then signature valid + certificate identity matches
    And rekor-cli search succeeds
    And SLSA L3 attestation found
```

## 9. Design Decisions

### 9.1 Why cross-functional review gate canonical mandatory non-skippable

- Per spec contract §9.6 + §10 anti-scope estrito + Waiver policy §19 non-skippable.
- Pricing/security/compliance claims com regulatory/financial/legal implications direct.
- Process discipline strict: Finance/Legal/Privacy Officer/Security lead per relevant page.
- Sem review = não merge (PR fails; CRITICAL post-mortem trigger se merge inadvertent).

### 9.2 Why 3 separate pages (não single legal page)

- Stripe + Linear + GOV.UK pattern: separate pricing + security + compliance pages.
- Cada page targets distinct persona: Finance/Legal (pricing) + Security/Privacy (security) + Compliance/Legal (compliance).
- Cross-functional sign-off per page per relevant role.

### 9.3 Why SBOM downloadable + Cosign verify + SLSA L3 Rekor (não NDA-gated email request)

- SOTA-baseline rare em competitors (BuildBuddy email request com NDA only).
- CoreLink openly downloadable + verification instructions Cosign verify + SLSA L3 attestation via Rekor query.
- Vantagem competitive strict (rare em open SBOM access).
- S-12 SEALED reuse (CycloneDX 1.5+ + Cosign sign + SLSA L3 attestation Rekor).

### 9.4 Why pentest exec summary public-safe (não full pentest report)

- Per spec contract §15 row 8: pentest exec summary public-safe (high-level findings; no specific CVE before disclosure).
- Security lead review antes publish (Quality Standard 14.s18.6).
- Full pentest report = NDA-gated enterprise procurement only (S-19 customer onboarding).

### 9.5 Why pricing calculator validated 10 scenarios (não unlimited)

- 10 sample scenarios cobrem common use cases (small dev solo + dev team + medium team + large team + enterprise + CI-heavy + BYOK + EU residency + SOC 2 procurement + high availability).
- Finance sign-off antes publish (R-S18-11; non-skippable per Waiver policy §19).
- Quality Standard 14.s18.5 canonical: pricing calculator validated 10 scenarios + Finance sign-off (EVT-044).

### 9.6 Why compliance page LINKS canonical sources (não introduz claims novos)

- Alignment com compliance_matrix.md + privacy_model.md SEALED.
- LGPD + GDPR + CCPA statements canonical em compliance_matrix.md (single source of truth).
- Sub-processors + privacy notice S-16 SEALED auto-generated reuse.
- Avoids drift via single source of truth canonical.

### 9.7 Why pricing changes via PR + cross-functional review only (não real-time API)

- Anti-scope per spec contract §10: real-time pricing API.
- Pricing changes versioned em git + cross-functional review only.
- Email broadcast 30d antes (per spec contract §15 row 2 mitigation).

### 9.8 ADR potencial?

- Não — cross-functional review gate canonical é canonical em S-18 spec contract §9.6 + §10 + Waiver policy §19; SBOM access reuses S-12 SEALED; sub-processors + privacy notice S-16 SEALED reuse (no novel decision; ADR não necessário em S-18 docs sprint).

## 10. Completeness Criteria

- [ ] **10.s18.004.1** `/pricing` page 5 tiers + feature matrix + pricing calculator validated 10 scenarios (per Completeness Criteria 10.s18.5 EVT-044).
- [ ] **10.s18.004.2** Pricing page Finance + Legal sign-off (cross-functional gate CF-1; non-skippable per Waiver policy §19).
- [ ] **10.s18.004.3** `/security` page SBOM downloadable + Cosign verify + SLSA L3 attestation Rekor lookup instructions (per Completeness Criteria 10.s18.4 EVT-010).
- [ ] **10.s18.004.4** Security page pentest exec summary public-safe + SLA + SOC 2 timeline.
- [ ] **10.s18.004.5** Security page Security lead + Privacy Officer sign-off (cross-functional gate CF-2).
- [ ] **10.s18.004.6** `/compliance` page LGPD + GDPR + CCPA + DPA + sub-processors + privacy notice.
- [ ] **10.s18.004.7** Compliance page Legal + Privacy Officer sign-off (cross-functional gate CF-3).
- [ ] **10.s18.004.8** Zero customer data em screenshots/examples (fixture pipeline + CI lint check; CTRL-PRIV-001 enforced; reuse from WI-S18-002 + WI-S18-003).

## 11. DoD

- [ ] 3 pages published (`/pricing` + `/security` + `/compliance`).
- [ ] Cross-functional sign-offs collected (Finance + Legal pricing; Security lead + Privacy Officer security; Legal + Privacy Officer compliance).
- [ ] Pricing calculator validated 10 scenarios + Finance sign-off.
- [ ] SBOM downloadable + Cosign verify + SLSA L3 Rekor lookup instructions tested.
- [ ] Pentest exec summary Security lead reviewed.
- [ ] Compliance LINKS canonical sources (alignment compliance_matrix.md + privacy_model.md).
- [ ] Zero customer data em screenshots/examples (CI lint check).
- [ ] Adversarial scenarios 4+ documented.

## 12. Invariants Validated

- **CTRL-PRIV-001** (zero PII em screenshots/examples) — IMPLEMENTA reflection em fixture pipeline + CI lint check (reuse from WI-S18-002 + WI-S18-003).
- **CTRL-CRED-001** (no secrets em CLI output / docs) — IMPLEMENTA reflection em PAT format placeholder (reuse from WI-S18-002).
- **Não introduz INVs novas** (sprint consumer; per spec contract §8 mantidas only).

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| Pricing page | `apps/docs/docs/pricing.mdx` | MDX |
| Pricing calculator component | `apps/docs/src/components/PricingCalculator.tsx` | TypeScript / React |
| Security page | `apps/docs/docs/security.mdx` | MDX |
| Compliance page | `apps/docs/docs/compliance.mdx` | MDX |
| Cross-functional sign-off log | `specs/04_sprints/S18/cross-functional-signoffs.md` | Markdown |
| **CODEOWNERS file** (Lote 10.18 codex P0 hard merge control) | `.github/CODEOWNERS` | Plain text — paths-to-team mapping |
| **Sign-off validator CI script** (Lote 10.18 codex P0) | `scripts/validate_cross_functional_signoffs.py` | Python; parses log + matches CODEOWNERS team membership; exit 1 se mismatch |
| **GitHub branch protection rule** (Lote 10.18 codex P0) | configured via `gh api` em deploy README | enforces CODEOWNERS approval para paths em §10 anti-scope sensitive |
| Pricing 10 scenarios validation | `specs/_audits/2026-XX-XX-pricing-calculator-10-scenarios.md` | Markdown |
| SBOM verification test report | `specs/_audits/2026-XX-XX-sbom-verification-test.md` | Markdown |

## 14. Quality Standards

- **14.s18.004.1** Cross-functional review gate para pricing + security + compliance pages: Finance + Legal + Privacy Officer + Security lead sign-off antes publish (per Quality Standard 14.s18.6; non-skippable per Waiver policy §19).
- **14.s18.004.2** SBOM access: SBOM downloadable + verification instructions; pentest exec summary public-safe (per Quality Standard 14.s18.7).
- **14.s18.004.3** Examples sanitization: fixture pipeline ensures no real customer data; test fixtures only (per Quality Standard 14.s18.9; reuse from WI-S18-002 + WI-S18-003).
- **14.s18.004.4** Cost regression gate: zero adicional cost (compliance/security/pricing pages reuse S-12 SEALED SBOM + S-16 SEALED sub-processors + privacy notice).

## 15. Test Plan

### Unit tests
- Pricing calculator function returns correct cost per tier per usage GB.
- 10 sample scenarios validated (assertion-based; expected cost per scenario).
- Cross-functional sign-off log schema valid.

### Integration tests
- Pricing page renders 5 tiers + feature matrix + calculator interactive.
- Security page SBOM download link valid + Cosign verify instructions tested (`cosign verify-blob` smoke test).
- Security page Rekor lookup instructions tested (`rekor-cli search` + `rekor-cli get` smoke test).
- Compliance page LINKS resolve (DPA + sub-processors + privacy notice).
- **Hard merge control test (Lote 10.18 codex P0 canonical fix)**: PR touching `apps/docs/docs/pricing.mdx` SEM `@finance-team` review → GitHub branch protection rejects merge (test via dry-run com placeholder PR); CI script `validate_cross_functional_signoffs.py` parses log + verifies entries; markdown log alone (prior version) era bypassable, agora combined CODEOWNERS + CI enforcement makes gate non-skippable.
- Cross-functional sign-off log committed before merge.
- CI lint check enforces zero customer data em screenshots/examples (reuse from WI-S18-002 + WI-S18-003).

### Adversarial scenarios (4+)
1. Pricing page merge attempt sem Finance sign-off → PR fails (cross-functional gate CF-1; non-skippable per Waiver policy §19).
2. Security page merge attempt sem Security lead review → PR fails (cross-functional gate CF-2; CRITICAL post-mortem trigger se merge inadvertent).
3. Pentest exec summary com specific CVE before disclosure inadvertent (developer mistake) → Security lead review catches; PR fails.
4. SBOM com sensitive vendor info inadvertent (developer mistake) → Security lead review catches; PR fails (per spec contract §15 row 7 mitigation).
5. Compliance claim out of sync com canonical compliance_matrix.md (developer mistake) → Legal review catches; PR fails (per spec contract §15 row 3 mitigation).

## 16. Failure Modes

- **FM-DOC-CROSS-FUNCTIONAL-BYPASS** (cross-functional review gate bypassed): non-skippable per Waiver policy §19; CRITICAL post-mortem trigger se merge inadvertent (per spec contract §18).
- **FM-DOC-COMPLIANCE-DRIFT** (compliance claim out of sync com canonical compliance_matrix.md): Legal review enforces alignment.
- **FM-DOC-SBOM-LEAK** (SBOM com sensitive vendor info inadvertent): Security lead review antes publish (per spec contract §15 row 7).
- **FM-DOC-PENTEST-CVE-LEAK** (pentest exec summary com specific CVE before disclosure): Security lead review high-level only (per spec contract §15 row 8).
- **FM-DOC-PRICING-DRIFT** (pricing claim out-of-sync com Stripe billing): Finance review enforces alignment + versioned pricing pages + email broadcast 30d (per spec contract §15 row 2).

## 17. Controls

- **CTRL-PRIV-001** (zero PII em screenshots/examples) — IMPLEMENTA reflection em fixture pipeline + CI lint check (reuse from WI-S18-002 + WI-S18-003).
- **CTRL-CRED-001** (no secrets em CLI output / docs) — IMPLEMENTA reflection em PAT format placeholder.
- **Cross-functional review gate canonical** (Finance + Legal + Privacy Officer + Security lead per relevant page) — IMPLEMENTA primary em WI-S18-004 (process discipline non-skippable).

## 18. Resilience Patterns

- N/A (docs sprint consumer; non-cripto-load-bearing).

## 19. Observability

- Cross-functional sign-off log status (PR fails se sign-off missing).
- CI lint check status (PR fails se PII or real PAT detected em screenshots/examples).
- Pricing calculator 10 scenarios validation CI status.
- SBOM verification test CI status.

Não introduz métricas Prometheus per-tenant em content WI (per observability_model §3.1; sprint consumer).

## 20. Security & Privacy

**STRIDE delta**:
- **Spoofing**: PAT format examples placeholder (never real PAT; CTRL-CRED-001 reflection).
- **Tampering**: docs source git-tracked + reviewer Docs lead + cross-functional sign-off per page; SBOM signed Cosign + SLSA L3 attestation Rekor (S-12 SEALED reuse).
- **Repudiation**: GitHub Actions CI lint check log + git commit history + cross-functional sign-off log.
- **Information disclosure**: SBOM scrubbed of internal-only deps (Security lead review antes publish per spec contract §15 row 7); pentest exec summary high-level only (no specific CVE before disclosure per row 8); zero customer data em screenshots/examples (CTRL-PRIV-001 reflection).
- **DoS**: nenhuma (docs content WI).
- **Elevation of privilege**: docs publish requires PR review + Docs lead sign-off + cross-functional sign-off per page (Finance/Legal/Privacy Officer/Security lead) + CI lint check pass.

**LINDDUN delta**:
- Linkability: nenhuma (docs content WI; no per-tenant identifiers em examples).
- Identifiability: PAT format placeholder + tenant_id placeholder + zero customer data em screenshots (CTRL-PRIV-001 reflection).
- Non-repudiation: GitHub Actions CI lint check log + cross-functional sign-off log.
- Detectability: CI lint check via grep for known PII patterns; cross-functional sign-off enforcement.
- Disclosure: SBOM Security lead review; pentest exec summary high-level only; compliance claims aligned canonical sources.
- Unawareness: cross-functional review gate canonical (Finance + Legal + Privacy Officer + Security lead) per page sensible non-skippable.
- Non-compliance: LGPD + GDPR (lawful basis per processing) + CCPA/CPRA statements aligned canonical compliance_matrix.md.

## 21. Dependencies

### Hard blockers
- **WI-S18-001 SEALED** (Docusaurus 3.x foundation + navbar links).
- **S-12 SEALED** (SBOM published CycloneDX 1.5+ + Cosign signed + SLSA L3 attestation Rekor).
- **S-16 SEALED** (privacy notice + sub-processors list auto-generated; docs link reuse).

### Soft blockers
- **compliance_matrix.md** SEALED canonical (LGPD + GDPR + CCPA statements canonical source).
- **privacy_model.md** SEALED canonical (privacy notice + sub-processors canonical source).
- **security_model.md** SEALED canonical (SBOM access + pentest summary canonical source).

### Cross-functional reviewers (mandatory; cross-functional gate canonical)
- **Finance** (pricing CF-1 sign-off mandatory).
- **Legal** (pricing CF-1 + compliance CF-3 sign-off mandatory).
- **Privacy Officer** (security CF-2 + compliance CF-3 sign-off mandatory).
- **Security lead** (security CF-2 sign-off mandatory).

### Outbound
- WI-S18-005 (closing ship gate verifies cross-functional sign-offs collected; PRR LOW_RISK + cross-functional publish gate separate).

## 22. Effort PERT

O: 8h, M: 14h, P: 22h → PERT **14.3h** (per spec contract §12; compliance + security + pricing pages + cross-functional review).

## 23. Cost Analysis

**Direct cost**:
- Zero adicional cost (compliance/security/pricing pages reuse S-12 SEALED SBOM + S-16 SEALED sub-processors + privacy notice).
- GitHub Actions free tier (CI lint check): $0/mês.

**Total**: ~$0/mês adicional.

**Indirect cost**: 0 enterprise procurement friction (SBOM downloadable + cross-functional review gate) + 0 regulatory exposure (compliance LINKS canonical sources) = priceless.

## 24. Post-mortem Hooks

- Cross-functional review bypassed em pricing/compliance/security PR → CRITICAL post-mortem + process reinforce (non-skippable gate violated; per spec contract §18).
- Pricing claim out-of-sync com Stripe billing → CRITICAL post-mortem (legal + customer trust per spec contract §18).
- Compliance claim incorrect → 5-Why + Legal review + immediate fix.
- SBOM disclosure inadvertent (sensitive vendor info) → 5-Why + Security lead review reinforce.
- Pentest exec summary leaks attack details (specific CVE before disclosure) → 5-Why + Security lead review reinforce.

## 25. Rollback / Recovery

Docs rollback: revert PR + redeploy CF Pages previous build. RTO ≤ 5min. Cross-functional sign-off retroactive enforcement via PR re-run + sign-off log re-collected.

## 26. Risk Register (6-col)

| ID | Risco | Prob | Det | Impacto | Exposure | Residual | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | Cross-functional review bypassed (pricing/security/compliance PR) | L | H (PR review) | HIGH (regulatory + financial + customer trust) | M | LOW | Non-skippable per Waiver policy §19; PR fails se sign-off missing; CRITICAL post-mortem trigger se merge inadvertent |
| R-002 | Pricing claim out-of-sync com Stripe billing | L | M | HIGH (legal + customer trust) | L | LOW | Finance review enforces alignment; versioned pricing pages; email broadcast 30d antes (per spec contract §15 row 2) |
| R-003 | Compliance claim out of sync com canonical compliance_matrix.md | M | M | HIGH (regulatory) | M | LOW | Legal review enforces alignment; LINKS canonical sources (não introduz claims novos) |
| R-004 | SBOM disclosure inadvertent (sensitive vendor info) | L | L | MEDIUM | L | LOW | Security lead review antes publish; SBOM scrubbed of internal-only deps (per spec contract §15 row 7) |
| R-005 | Pentest exec summary leaks attack details (specific CVE before disclosure) | L | L | MEDIUM | L | LOW | Security lead review high-level only; no specific CVE before disclosure (per spec contract §15 row 8) |

## 27. Knowledge Transfer

- Tech talk (1h): "Cross-functional review gate canonical — Finance/Legal/Privacy Officer/Security lead per relevant page; SBOM access SOTA-baseline; pricing calculator validated 10 scenarios".
- Doc `docs/internal/s18-cross-functional-gate.md` — cross-functional review procedure.
- Onboarding test (4 questions): cross-functional gate non-skippable rationale + SBOM access SOTA + pentest exec summary public-safe rationale + compliance LINKS canonical sources rationale.

## 28. Sign-off (LOW_RISK 3 canonical + cross-functional gate)

3 roles canonical para LOW_RISK lane:

| # | Role | Name | Signed Date | Status |
|---|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | _pending_ | _pending_ |
| 2 | Final Approver | Gustavo Schneiter | _pending_ | _pending_ |
| 3 | Docs lead | _TBD; emphatic — compliance + security + pricing pages content + Diátaxis discipline_ | _pending_ | _pending_ |

**Cross-functional review gate canonical (separate publish gate; non-skippable per Waiver policy §19)**:

| # | Page | Required cross-functional sign-off | Status |
|---|---|---|---|
| CF-1 | `/pricing` | **Finance** (5 tiers + calculator validated 10 scenarios + EVT-044) + **Legal** (terms + DPA reference) | _pending_ |
| CF-2 | `/security` | **Security lead** (SBOM access + pentest exec summary public-safe + Cosign verify + SLSA L3 Rekor instructions) + **Privacy Officer** (CTRL-PRIV-001 enforced + zero PII em examples) | _pending_ |
| CF-3 | `/compliance` | **Legal** (LGPD + GDPR + CCPA statements aligned canonical compliance_matrix.md) + **Privacy Officer** (privacy notice + sub-processors S-16 auto-generated reuse) | _pending_ |

> Cross-functional sign-offs count toward separate publish gate (não main PRR per spec contract §10 anti-scope; non-skippable per Waiver policy §19). PRR LOW_RISK 3 sign-offs canonical em WI-S18-005 closing ship gate.

## 29. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-29 | Gustavo (via Claude Opus 4.7) | Criação WI-S18-004 (cycle 12.S18.0; LOW_RISK lane; compliance + security + pricing pages com cross-functional review gate canonical mandatory Finance/Legal/Privacy Officer/Security lead non-skippable per Waiver policy §19; SBOM downloadable + Cosign verify + SLSA L3 Rekor; pentest exec summary public-safe; pricing calculator validated 10 scenarios + Finance sign-off). |

## 30. Anti-patterns evitados

- Skip cross-functional review gate (CRITICAL gap; non-skippable per Waiver policy §19).
- Pricing claim sem Finance + Legal sign-off (CRITICAL gap; per spec contract §10 + R-S18-11).
- Security claim sem Security lead + Privacy Officer sign-off (CRITICAL gap; Quality Standard 14.s18.6).
- Compliance claim sem Legal + Privacy Officer sign-off (CRITICAL gap).
- Real customer data em screenshots/examples (CTRL-PRIV-001 violation).
- SBOM com sensitive vendor info inadvertent (Security lead review antes publish).
- Pentest exec summary com specific CVE before disclosure (Security lead review high-level only).
- Real-time pricing API (anti-scope per spec contract §10; pricing changes via PR + cross-functional review only).

---

**Fim WI-S18-004.**
