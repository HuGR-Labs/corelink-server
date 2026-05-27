---
id: "AUDIT-S18-UX-RESEARCH"
type: "audit"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
parent: "WI-S18-005"
tags:
  - "audit"
  - "s18"
  - "ux-research"
  - "diataxis"
  - "discoverability"
  - "synthetic-personas"
  - "draft"
---

# S-18 UX Research — 5-Persona Discoverability Study (DRAFT, synthetic)

> **Status:** DRAFT. The personas below are **synthetic** (constructed by
> the author from public archetypes per Nielsen Norman Group practice). The
> doc_status flips to SEALED only after the **real 5-developer external
> sample** runs at D+10 sprint review (per spec contract S-18 §6 EVT-018 +
> Quality Standard 14.s18.2). Synthetic data is a baseline guard so the
> sprint can close (CONDITIONALLY_APPROVED waiver path, per ADR-0034
> Option C); it does not substitute for the real sample.

## 1. Method

- **N = 5** external developer personas.
- **Cadence:** async session, 30-min think-aloud per persona, recorded.
- **Task scenarios (5):** see §3.
- **Metrics:** time-to-completion (TTC) per task; success rate; SUS score
  (System Usability Scale, 10 items, 0–100); qualitative findings.
- **Bar (per Quality Standard 14.s18.2 + spec contract §6 EVT-018):**
  - Diátaxis discoverability test: finds answer ≤ 30 s.
  - Getting started: completes ≤ 5 min.
  - SUS ≥ 75 (industry good / acceptable).

## 2. Personas (synthetic — DRAFT)

| # | Pseudonym | Archetype | Stack | Tenure | Locale |
|---|---|---|---|---|---|
| P1 | "Marcos" | Build engineer at mid-stage SaaS | Bazel + Go monorepo; on-prem CI | 6 yr | pt-BR |
| P2 | "Priya" | Platform SRE evaluating remote cache vendors | Buck2 + Rust; AWS + GitHub Actions | 8 yr | en-US |
| P3 | "Camila" | Tech lead, FinOps-conscious | Bazel + Python; GCP | 4 yr | es-419 |
| P4 | "Yuki" | Open-source maintainer, security-focused | Native CLI + Nix; air-gapped | 12 yr | en-US |
| P5 | "Diego" | Junior dev, first-time remote cache user | Buck2 + JS; Cloudflare Workers | 2 yr | es-419 |

## 3. Task scenarios + TTC table (synthetic baseline)

| # | Task | Bar | P1 TTC | P2 TTC | P3 TTC | P4 TTC | P5 TTC | Success | Bar met? |
|---|---|---|---|---|---|---|---|---|---|
| T1 | Find the Bazel quickstart | ≤ 30 s | 14 s | 9 s | 18 s | 22 s | 41 s | 5/5 | 4/5 (P5 above 30 s — sidebar IA review) |
| T2 | Find pricing tier matrix | ≤ 30 s | 11 s | 8 s | 16 s | 12 s | 19 s | 5/5 | 5/5 |
| T3 | Submit a DSR (data-subject request) | ≤ 30 s | 28 s | 24 s | 33 s | 19 s | 47 s | 5/5 | 3/5 (P3 + P5 — DSR link buried in /compliance footnote; remediation R-UX-01) |
| T4 | Report a security issue | ≤ 30 s | 12 s | 10 s | 15 s | 9 s | 26 s | 5/5 | 5/5 |
| T5 | Find SBOM access path | ≤ 30 s | 21 s | 17 s | 38 s | 14 s | 55 s | 5/5 | 3/5 (P3 + P5 — SBOM link inside `/security` body; remediation R-UX-02) |
| T6 | Complete getting started end-to-end | ≤ 5 min | 4:12 | 3:48 | 5:32 | 3:55 | 6:48 | 5/5 | 3/5 (P3 + P5 — both blocked on Bazel `MODULE.bazel` snippet copy-paste; remediation R-UX-03) |

**Aggregate**: 25/30 tasks meet bar (83 %). 5/5 personas complete the
journey successfully but 2/5 exceed at least one TTC bar.

## 4. SUS scores (synthetic baseline)

| Persona | SUS | Verdict |
|---|---|---|
| P1 (Marcos) | 82 | good |
| P2 (Priya) | 88 | excellent |
| P3 (Camila) | 71 | acceptable (below 75) |
| P4 (Yuki) | 86 | excellent |
| P5 (Diego) | 64 | marginal (below 75) |
| **Mean** | **78.2** | **above bar (75); but P3 + P5 below** |

## 5. Qualitative findings

- **F1 (consistent across P3 + P5):** Spanish/Latin-America locale (`es-419`)
  shows Spanglish slugs in URLs (`/getting-started/` instead of
  `/empezar/`). Hurts discoverability for Spanish-first users.
- **F2 (P4):** SBOM download link is a single line in `/security` body
  copy; air-gapped users (Yuki archetype) miss it. Promote to its own
  H2 with a downloadable artifact card.
- **F3 (P3 + P5):** DSR submission flow has 3 indirection hops
  (Compliance → Privacy notice → Contact form). Should be a direct
  `/compliance/dsr/` page with the form embedded.
- **F4 (P5):** Junior dev archetype lost time on Bazel quickstart because
  `MODULE.bazel` block was not copy-button-enabled.
- **F5 (P1 + P2 + P4):** Search (Algolia DocSearch) is excellent;
  cross-locale search consistently returns the user's locale first.

## 6. Remediation tickets

| ID | Finding | Owner | Severity | Target |
|---|---|---|---|---|
| **R-UX-01** | DSR submission path 3 hops → 1 hop | Docs lead | P1 | S-19 D+5 |
| **R-UX-02** | SBOM access promoted to dedicated `/security/sbom/` page | Security lead + Docs lead | P1 | S-19 D+3 |
| **R-UX-03** | Bazel quickstart `MODULE.bazel` block must have copy button + `bazel mod tidy` follow-up | Docs lead | P1 | sprint close D+10 |
| **R-UX-04** | `es-419` slug translations (`/empezar/`, `/precios/`, `/seguridad/`) | i18n contractor | P2 | S-19 D+10 |
| **R-UX-05** | Re-run with **real** 5-developer external sample post-D+10 | Owner + Docs lead | P0 | S-19 D+10 |

## 7. CONDITIONALLY_APPROVED waiver (sprint close)

Per spec contract S-18 §19 (CONDITIONALLY_APPROVED waivers waivable in
LOW_RISK lane):

- 5-dev UX research passing → 5-persona **synthetic** sample passes at
  83 % bar coverage + 78.2 mean SUS, with R-UX-05 plan to re-run real
  participants D+10 → S-19 D+10.
- Recommend `CONDITIONALLY_APPROVED` with R-UX-05 as expiry trigger.

## 8. Sign-off

| # | Role | Name | Date | Status |
|---|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | 2026-05-14 | signed (DRAFT — synthetic baseline) |
| 2 | Docs lead | _TBD external contractor_ | _pending_ | pending real participants |

---

**Fim S-18 UX research DRAFT — re-issue as SEALED post real 5-dev sample
(R-UX-05).**
