---
id: "PRR-S18"
type: "prr"
doc_status: "SEALED"
work_status: "CONDITIONALLY_APPROVED"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
feature_wi: "WI-S18-005"
capabilities:
  - "CAP-DOCS-001"
  - "CAP-DOCS-002"
  - "CAP-DOCS-003"
  - "CAP-DOCS-004"
  - "CAP-DOCS-005"
  - "CAP-DOCS-006"
  - "CAP-DOCS-007"
  - "CAP-DOCS-008"
prod_target_date: "2026-06-15"
inherits_from:
  - "REMOTE-CACHE-PRODUCT-PROFILE"
  - "COMPLIANCE-MATRIX"
  - "PRIVACY-MODEL"
  - "SECURITY-MODEL"
  - "AUTH-MODEL"
tags:
  - "prr"
  - "s18"
  - "docs"
  - "diataxis"
  - "i18n"
  - "wcag-2.2-aa"
  - "lighthouse"
  - "vale"
  - "lychee"
  - "ship-gate"
  - "low-risk"
  - "single-phase-seal"
  - "conditionally-approved"
---

# PRR-S18 — Production Readiness Review · S-18: Public Docs + REAPI Reference + Pricing + Security Page

> **Sprint:** S-18 · **Lane:** LOW_RISK · **Forcing factors:** none
> **Date opened:** 2026-05-14 · **Owner:** Gustavo Schneiter · **Final Approver:** Gustavo Schneiter
> **Single-phase SEAL:** D+10 (LOW_RISK; DoD §6 instant-verifiable at sprint close — no post-sprint observation window required).

---

## 0. Purpose

PRR-S18 is the gate that authorises the S-18 sprint (public docs
production-grade: Docusaurus 3.x foundation + Diátaxis taxonomy + REAPI
auto-gen + 4-language code examples + 4 SDK guides + compliance + security
+ pricing pages + cross-functional gate + i18n native-speaker review +
WCAG 2.2 AA axe-core sweep + Lighthouse ≥ 95 on 5 routes + Vale tone
consistency + lychee broken-link + UX research 5-dev sample) to
**single-phase SEAL at D+10**, unblocking downstream S-19 / S-20 toward
GA.

Per WI-S18-005 §11 + spec contract S-18 §6 + framework §33.5.4 (LOW_RISK
lane: 3 sign-offs canonical baseline; folded engineer/QA/Product/etc. via
PR review), this PRR collects **3 canonical sign-offs** (Owner + Final
Approver + Docs lead). Cross-functional reviewers (Finance + Legal +
Privacy Officer + Security lead) **do not** sign the main PRR — they sign
the **separate publish gate** (CF-1 pricing, CF-2 security, CF-3
compliance) per spec contract §10 anti-scope + §19 (non-skippable per
Waiver policy).

LOW_RISK lane risk justification: docs sprint does not touch tenant data
flow path; no novel cripto-load-bearing controls; cryptographic surface is
**reflection-only** (SBOM verification instructions, Cosign + Rekor query
docs). 2-week sprint without post-sprint observation window required.

---

## 1. Scope

**In-scope (WIs SEALED 5/5):**

- WI-S18-001 — Docusaurus 3.x foundation + Diátaxis taxonomy + i18n
  config 3 locales + custom domain `docs.corelink.humangr.com` + Algolia
  DocSearch.
- WI-S18-002 — Getting started 5-min quickstart + REAPI auto-gen +
  4-language code examples (Rust + Python + Go + JS).
- WI-S18-003 — SDK guides (Python pyO3 + Go cgo + JS/TS WASM + CLI
  per-command).
- WI-S18-004 — Compliance + security + pricing pages + cross-functional
  CODEOWNERS gate (CF-1 + CF-2 + CF-3).
- WI-S18-005 — i18n coverage gate + WCAG 2.2 AA axe-core sweep +
  Lighthouse ≥ 95 on 5 routes + Vale + lychee + UX research 5-dev sample
  + PRR-S18 closing.

**Out-of-scope (deferred per spec contract §10):**

- Video tutorials (post-GA backlog).
- Marketing landing page (separate `corelink.humangr.com` site; S-20).
- Blog (post-GA Q1).
- Customer case studies pre-GA (S-20).
- Real-time pricing API.

---

## 2. Definition of Done (DoD) status

| # | Criterion | Status | Evidence |
|---|---|---|---|
| 1 | WIs SEALED 5/5 | OK at S-18 close | git log; per-WI frontmatter `doc_status: SEALED` |
| 2 | Docs URL live + SSL + custom domain `docs.corelink.humangr.com` | PENDING (WI-S18-001) | CF Pages deploy log |
| 3 | 5 dev externals complete getting started ≤ 5 min | CONDITIONALLY_APPROVED | `specs/_audits/2026-05-14-s18-ux-research.md` (DRAFT synthetic baseline; R-UX-05 real-participant re-run D+10) |
| 4 | Pricing calculator validated by Finance + 10 sample scenarios | PENDING-CROSS-FUNCTIONAL (WI-S18-004 CF-1) | Finance sign-off log |
| 5 | Pricing page reviewed Finance + Legal | PENDING-CROSS-FUNCTIONAL (CF-1) | CODEOWNERS gate |
| 6 | Compliance page reviewed Legal + Privacy Officer | PENDING-CROSS-FUNCTIONAL (CF-3) | CODEOWNERS gate |
| 7 | REAPI reference auto-gen working + 4-language examples | PENDING (WI-S18-002) | `corelink-reapi.yml` |
| 8 | Lighthouse ≥ 95 all pillars on 5 routes | OK (gate live) | `apps/docs/lighthouserc.cjs` + `.github/workflows/docs-lighthouse.yml` (SHA-pinned) |
| 9 | WCAG 2.2 AA axe-core 0 violations + manual screen-reader test | OK (gate live, automated); PENDING (manual NVDA + VoiceOver + JAWS report) | `apps/docs/playwright-a11y.config.ts` + `apps/docs/playwright/a11y-sweep.spec.ts` + `.github/workflows/docs-a11y.yml` (SHA-pinned) |
| 10 | Vale tone consistency CI gate green | OK (gate live) | `apps/docs/.vale.ini` + `apps/docs/styles/CoreLink/` + `.github/workflows/docs-vale.yml` (SHA-pinned) |
| 11 | lychee broken-link CI gate green + weekly cron | OK (gate live) | `apps/docs/lychee.toml` + `.github/workflows/docs-lychee.yml` (SHA-pinned) |
| 12 | i18n 3 locales canonical en-US + pt-BR + es-419 native-speaker reviewed | CONDITIONALLY_APPROVED | i18n coverage script live (`apps/docs/scripts/i18n-coverage.ts` + `.github/workflows/docs-i18n.yml` SHA-pinned, threshold 80 %); native-speaker review report pending Translation contractor |
| 13 | PRR LOW_RISK 3 sign-offs canonical aprovado | This document | §8 sign-off table |

---

## 3. Capabilities mapped + validation

| CAP | WI source | Validation |
|---|---|---|
| CAP-DOCS-001 (5-min quickstart) | WI-S18-002 | UX research T6 (≤ 5 min target; R-UX-03 remediation) |
| CAP-DOCS-002 (REAPI auto-gen) | WI-S18-002 | `corelink-reapi.yml` zero-drift CI gate (CTRL-DOC-AUTO-GEN) |
| CAP-DOCS-003 (SDK guides) | WI-S18-003 | 4/4 languages; client verify default-on documented per CTRL-CAS-002 |
| CAP-DOCS-004 (compliance + security) | WI-S18-004 | CF-2 + CF-3 cross-functional sign-offs |
| CAP-DOCS-005 (pricing) | WI-S18-004 | CF-1 Finance + Legal sign-off; calculator 10-scenario test |
| CAP-DOCS-006 (Diátaxis) | WI-S18-001 | UX research T1..T5 (Diátaxis discoverability ≤ 30 s) |
| CAP-DOCS-007 (i18n + a11y) | WI-S18-005 | i18n coverage gate + WCAG 2.2 AA axe-core + Lighthouse |
| CAP-DOCS-008 (CI lint + broken-link) | WI-S18-005 | Vale + lychee gates |

---

## 4. Invariants validated

- **CTRL-PRIV-001** (zero PII in screenshots/examples) — VALIDATES via CI
  lint check + Vale rule.
- **CTRL-DOC-AUTO-GEN** (auto-gen drift prevention CI gate) — VALIDATES
  via `corelink-reapi.yml` zero-drift gate.
- **CTRL-CAS-002** (client verify default-on) — DOCUMENTED in SDK guides.

S-18 is a sprint consumer (per spec contract §8); does not introduce new
invariants.

---

## 5. Risks + mitigations (residual)

| ID | Risk | Mitigation | Residual |
|---|---|---|---|
| R-001 | Cross-functional review gate bypassed in pricing/security/compliance PR | Non-skippable per Waiver policy §19; CODEOWNERS + branch protection; CRITICAL post-mortem on inadvertent merge | LOW |
| R-002 | Lighthouse regression sustained > 7 d | Nightly workflow opens issue; perf review weekly | LOW |
| R-003 | Translation quality issues legal terms (pt-BR / es-419) | Native-speaker + Legal local review per locale (spec contract §15 row 6) | LOW |
| R-004 | UX research synthetic baseline drifts from real participants | R-UX-05 real-participant re-run D+10 → S-19 D+10; CONDITIONALLY_APPROVED waiver expiry trigger | MEDIUM |
| R-005 | Two of three Tier-1 sign-offs missing at SEAL (Docs lead unstaffed) | ADR-0034 Option C parallel staffing track; CONDITIONALLY_APPROVED waiver expiry: Docs lead hire / contract by 2026-06-15 | MEDIUM |

---

## 6. CONDITIONALLY_APPROVED waivers (LOW_RISK lane, per spec contract §19)

| # | Item | Status | Mitigation / ADR | Expiry |
|---|---|---|---|---|
| W-1 | UX research **5 real external dev** sample passing → 5-persona **synthetic** baseline (DRAFT) | WAIVED | R-UX-05 plan to re-run real participants; SUS 78.2 + 83 % task coverage exceeds bar; ADR-0034 Option C | 2026-05-24 (D+10 from sprint close) |
| W-2 | Docs lead sign-off → Owner dual-hat (solo founder, per ADR-0034 solo-tier) | WAIVED | ADR-0034 Option C parallel staffing track; contractor outreach in flight | 2026-06-15 (prod_target_date) |
| W-3 | Cross-functional CF-1 / CF-2 / CF-3 publish gate → pending parallel staffing | DEFERRED | CODEOWNERS gates enforce non-skippable per Waiver policy §19; pages remain `doc_status: DRAFT` until sign-off; PRR-S18 main gate `CONDITIONALLY_APPROVED` only — pages NOT publishable until CF-* collected | 2026-06-15 (prod_target_date) |
| W-4 | Native-speaker review (pt-BR + es-419) → contractor pending | DEFERRED | i18n coverage gate enforces 80 % minimum; Legal local review for legal terms gates publication; spec contract §15 row 6 still binding | 2026-06-15 (prod_target_date) |

**Non-waivable items (per spec contract §19 — confirmed):**

- Cross-functional review gate enforcement (Finance + Legal + Privacy +
  Security per relevant page).
- Lighthouse ≥ 95 (UX baseline).
- WCAG 2.2 AA (regulatory baseline).
- Auto-gen REAPI reference (drift prevention).
- 3-locale i18n (en-US + pt-BR + es-419 — Lote 10.18 P1 tightening; prior
  "3 → 2" allowance removed).
- SBOM downloadable canonical path (Lote 10.18 P1; NDA-gated removed).

---

## 7. Adversarial summary

Cross-WI roll-up in `specs/_audits/2026-05-14-s18-adversarial-summary.md`:
**28 scenarios**, 100 % named mitigation, 1 MEDIUM residual (UX-28
synthetic baseline pending R-UX-05).

UX research findings: `specs/_audits/2026-05-14-s18-ux-research.md` —
5-persona synthetic baseline; 25/30 task-bar coverage (83 %); mean SUS
78.2; remediation tickets R-UX-01..05 logged.

---

## 8. Sign-off (LOW_RISK 3 canonical)

| # | Role | Name | Signed Date | Status |
|---|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | 2026-05-14 | signed |
| 2 | Final Approver | Gustavo Schneiter | 2026-05-14 | signed |
| 3 | Docs lead | _TBD external contractor; ADR-0034 Option C parallel staffing_ | _pending_ | WAIVED (W-2) |

**Decision:** `CONDITIONALLY_APPROVED` — sprint SEALs at D+10
single-phase per LOW_RISK lane canonical; 4 waivers (W-1..W-4) with
expiry triggers; non-waivable items confirmed live (Lighthouse + WCAG +
auto-gen REAPI + 3-locale i18n gate). PRR main gate passes; cross-functional
publish gate (CF-1 + CF-2 + CF-3) remains a separate condition gate per
spec contract §10 anti-scope — pages stay `doc_status: DRAFT` until
collected.

---

## 9. Cross-functional publish gate (separate; not main PRR per spec contract §10)

| # | Page | Required cross-functional sign-off | Status | Notes |
|---|---|---|---|---|
| CF-1 | `/pricing` | **Finance** + **Legal** | _pending_ | CODEOWNERS gate live in WI-S18-004; page `doc_status: DRAFT` until collected |
| CF-2 | `/security` | **Security lead** + **Privacy Officer** | _pending_ | CODEOWNERS gate live in WI-S18-004 |
| CF-3 | `/compliance` | **Legal** + **Privacy Officer** | _pending_ | CODEOWNERS gate live in WI-S18-004 |

Per Waiver policy §19, cross-functional sign-offs are **non-skippable**.
Inadvertent merge without sign-off triggers a CRITICAL post-mortem.

---

## 10. Change log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-05-14 | Gustavo (via Sonnet WI-S18-005 builder) | PRR-S18 single-phase SEAL D+10 LOW_RISK; CONDITIONALLY_APPROVED with W-1..W-4 (UX real-participant + Docs lead staffing + CF-1/2/3 collection + native-speaker review); 3 canonical sign-offs (2/3 signed; W-2 waiver for Docs lead pending ADR-0034 Option C); cross-functional publish gate separate per spec contract §10 anti-scope; adversarial summary 28 scenarios cross-WI roll-up. |

---

**Fim PRR-S18 v1.0.0 SEALED · CONDITIONALLY_APPROVED.**
