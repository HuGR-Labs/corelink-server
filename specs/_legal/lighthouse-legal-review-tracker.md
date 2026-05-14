---
id: "LIGHTHOUSE-LEGAL-REVIEW-TRACKER"
type: "audit"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers:
  - role: "legal"
    name: "Cooley / DLA Piper / Bird & Bird (multi-jurisdictional)"
  - role: "privacy_lead"
    name: "TBD (DPO)"
supersedes: null
superseded_by: null
parent: "S-20"
inherits_from:
  - "PRIVACY-MODEL"
  - "COMPLIANCE-MATRIX"
tags: ["legal", "s20", "ga", "dpa", "sla", "lighthouse", "wi-s20-005"]
---

# Lighthouse Customers — Legal Review Tracker (WI-S20-005)

> **Status:** DRAFT (tracker live; per-customer status fields advance as signing closes).
> **Origin:** WI-S20-005.
> **Cross-references:** WI-S20-004 (lighthouse customer attestations), WI-S19-002
> (DPA template), `legal/sla/v1.0.0.md`, `legal/dpa/v1.0.0.*.md`.

## 0. Purpose

Single source of truth for the contractual legal review of CoreLink's three
GA lighthouse customers. Each customer must complete **five gates** before
sign-off counts toward the GA promotion gate in WI-S20-001:

1. **SLA reviewed** — customer Legal has reviewed `legal/sla/v1.0.0.md`.
2. **DPA reviewed** — customer Legal has reviewed `legal/dpa/v1.0.0.*.md`
   (locale-specific copy delivered).
3. **Redlines exchanged** — any redlines exchanged + resolved + per-customer
   waiver ADR raised if a clause is altered.
4. **Final signed copy archived** — counter-signed PDFs filed in
   `docs/legal/signed/<customer>/<artifact>-vX.Y.Z.pdf` (LFS-tracked, not
   committed in plain).
5. **AML / sanctions screening completed** — beneficial-ownership and
   sanctions-list (OFAC SDN, EU, UK, ANPD-affiliated) screening completed
   and archived.

## 1. Lighthouse customer roster

| # | Codename | Tier | Region | ICP | Status |
|---|---|---|---|---|---|
| 1 | **Forge (customer-zero)** | Pro (internal) | WNAM | HuGR Forge dev-tools self-customer | active |
| 2 | **OSS-Maintainer** | Starter (external OSS) | WEUR | OSS project under SoftwareFund umbrella | active |
| 3 | **Enterprise-BYOK** | Enterprise (BYOK) | ENAM + WEUR | regulated industries (initial fintech ICP) | active |

(Codenames are placeholders; actual customer names recorded in the private
addendum at `docs/legal/lighthouse-customer-roster-private.md`, LFS-tracked.)

## 2. Per-customer five-gate matrix

| Customer | (1) SLA reviewed | (2) DPA reviewed | (3) Redlines resolved | (4) Final signed archived | (5) AML/sanctions completed | Status |
|---|---|---|---|---|---|---|
| Forge (customer-zero) | n/a — self-signed by HuGR Labs Legal Counsel acting for both sides | n/a — self-signed (precedent record) | n/a | `docs/legal/signed/forge/dpa-v1.0.0.pdf`, `docs/legal/signed/forge/sla-v1.0.0.pdf` | n/a (intra-org; UBO already known) | **SIGNED** |
| OSS-Maintainer | _pending Legal Counsel sign-off — target 2026-05-21_ | _pending — locale `en-US`, target 2026-05-21_ | _expected: zero redlines (template DPA)_ | _pending — `docs/legal/signed/oss-maintainer/`_ | _pending — sanctions list screen scheduled 2026-05-18_ | **IN-REVIEW** |
| Enterprise-BYOK | _pending Legal Counsel sign-off — target 2026-05-28_ | _pending — locale `en-US` + Pricing Addendum + Schrems II TIA — target 2026-05-28_ | _expected: ≤ 5 redlines (BYOK key-management indemnity; service-credit cap)_ | _pending — `docs/legal/signed/enterprise-byok/`_ | _pending — UBO + OFAC SDN + EU + UK sanctions screen scheduled 2026-05-20_ | **IN-REVIEW** |

## 3. Per-customer evidence detail

### 3.1 Forge (customer-zero)

- Role: HuGR Labs is both controller and processor (intra-org). Used to
  establish precedent and exercise the click-through flow end-to-end.
- DPA executed under WI-S19-002 (click-through 6-field consent), counter-
  signed by HuGR Labs General Counsel.
- SLA executed alongside DPA at v1.0.0 effective date.
- AML/sanctions: n/a — intra-org.
- Evidence: `docs/legal/signed/forge/`.

### 3.2 OSS-Maintainer (external, Starter tier)

- Legal Counsel engagement: external counsel (target firm: Cooley); 6-week
  lead reuses the S-14 engagement path.
- Review scope: SLA v1.0.0 + DPA v1.0.0 (en-US locale). No Pricing Addendum
  (Starter tier).
- Expected redlines: none (template DPA). Any non-zero redlines trigger an
  ADR per the WI-S20-005 acceptance criteria.
- AML / sanctions screening: corporate-entity verification only; check OSS
  parent foundation against OFAC SDN list and EU consolidated list; archive
  screening output as `docs/legal/sanctions-screening/oss-maintainer-2026-05.pdf`.

### 3.3 Enterprise-BYOK

- Legal Counsel engagement: external counsel multi-jurisdictional (US +
  EU + Brazil) — Cooley (US), DLA Piper (EU/Schrems II), Bird & Bird (UK).
  Total engagement budget $15–30k (reuse S-14 path).
- Review scope: SLA v1.0.0 + DPA v1.0.0 + Pricing Addendum (Enterprise) +
  Schrems II TIA (`legal/lia/tia-template.md`) + Sub-Processor Commitments
  (`legal/dpa/SUB-PROCESSOR-COMMITMENTS.md`) + EU SCCs reference
  (`legal/dpa/STANDARD-CONTRACTUAL-CLAUSES-EU.md`).
- Expected redlines: ≤ 5 (key management indemnity on BYOK kill switch;
  service-credit cap exception for catastrophic breach; SCC docking-clause
  language; pricing-addendum SLA enhancement to 99.95%).
- Each redline opens an ADR at `specs/_audits/2026-XX-XX-dpa-waiver-enterprise-byok-<topic>.md`
  with Legal Counsel + customer Legal + Founder/CEO sign-off and an expiry
  + revalidation trigger.
- AML / sanctions screening: full UBO chain plus OFAC SDN + EU + UK
  + Brazilian sanctions lists; archive output as
  `docs/legal/sanctions-screening/enterprise-byok-2026-05.pdf`.
- Breach-notification SLA: customer-specific contact channel established
  (24h Slack-shared incident channel + 72h GDPR Art. 33-aligned formal
  notice). Recorded in `legal/breach-notification/<customer>/contacts.md`.

## 4. Sign-off gates

A customer "counts" for GA only when all five gates are GREEN:

```
gate_status = SIGNED if (
    SLA_reviewed AND DPA_reviewed AND redlines_resolved
    AND final_signed_archived AND aml_sanctions_completed
) else IN-REVIEW
```

| Gate state | Counts toward GA gate WI-S20-001? |
|---|---|
| SIGNED | yes |
| IN-REVIEW | no |
| BLOCKED | no — waiver ADR + Founder/CEO escalation required |

## 5. Negative-path waivers (per WI-S20-005 §6.2)

If 3 → 2 lighthouse signings occur (Enterprise-BYOK redlines extend beyond
GA-cutover): waiver ADR at
`specs/_audits/2026-XX-XX-dpa-waiver-2-of-3-lighthouse.md` with explicit
plan to add the third within 60 days post-GA, Legal Counsel + Founder/CEO
sign-off, and an expiry trigger.

## 6. Audit evidence chain

- `specs/_audits/2026-XX-XX-dpa-signed-3-lighthouse.md` — signed-customer
  evidence (filled at sealing of WI-S20-005 with attached final-PDF refs).
- `specs/_audits/2026-XX-XX-legal-review-dpa-v1.md` — multi-jurisdictional
  Legal review record.
- `specs/_audits/2026-XX-XX-legal-review-sla-v1.md` — SLA Legal review
  record.

## 7. Compliance mapping

| Framework | Section in this tracker |
|---|---|
| GDPR Art. 28 (processor obligations evidenced via DPA + signed copy) | §§1–4 |
| LGPD Art. 39 (operador evidenced) | §§1–4 |
| AML — US Bank Secrecy Act / EU 6AMLD / Brazilian Lei 9613/98 | §3 (sanctions screening) |
| OFAC SDN, EU consolidated, UK Sanctions List, ANPD-affiliated | §3 |
| SOC 2 CC9.2 (vendor / customer management — bidirectional) | §§3–5 |

Canonical control IDs in `specs/03_architecture/compliance_matrix.md`.

## 8. Version history

| Version | Date | Author | Change |
|---|---|---|---|
| 1.0.0 | 2026-05-14 | Gustavo Schneiter | Tracker created under WI-S20-005. |
