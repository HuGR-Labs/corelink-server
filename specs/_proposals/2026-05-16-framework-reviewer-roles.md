---
id: "PROPOSAL-2026-05-16-FRAMEWORK-REVIEWER-ROLES"
type: "governance"
doc_status: "DEFASADO"
audit_status: "ACTIVE"
version: "0.2.0"
created: "2026-05-16"
updated: "2026-05-27"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["governance", "reviewers", "staffing", "proposal", "framework-freeze", "wave-20", "wave-22", "lote-7", "ga"]
references:
  - "specs/00_framework.md"
  - "specs/_audits/sealed/2026-05-15-framework-v1-0-0-ga-audit.md"
  - "specs/_governance/reviewer_staffing_strategy.md"
  - "specs/03_architecture/adrs/ADR-0034-prr-staffing-waiver-solo-tier.md"
  - "specs/_proposals/2026-05-16-framework-reviewer-roles-addendum.md"
---

> **DEFASADO 2026-05-27 — never landed.** This proposal/draft was scoped but did not advance to implementation. Preserved as historical record; no current code references it. See `specs/_audits/2026-05-27-specs-inventory-cleanup-map.md` §3.2 for the inventory triage decision.

# Framework Reviewer Roles — Lote 7 Staffing Proposal (FW-H-1..4)

> **Purpose.** The wave-18 Lote 6 audit (`specs/_audits/sealed/2026-05-15-framework-v1-0-0-ga-audit.md`) DEFERRED `00_framework.md` v1.0.0-rc1 → v1.0.0 GA promotion because §43.1 contains 4 reviewer slots, 3 of which are `(a nomear)`. This document proposes **canonical role definitions** for each of those slots so the Owner can pattern-match real organization members (or external advisors) to slots, sign off §43.1, and close the framework freeze gate (unblock Path A in audit §6.3).
>
> **Non-goal.** This proposal does NOT nominate individuals; it does NOT supersede `specs/_governance/reviewer_staffing_strategy.md` v0.1.0 (which remains the operative multi-tier staffing strategy). It NARROWS the framework-freeze gate to four concretely-scoped role definitions whose intersection produces full §1–§43 coverage.

---

## §1 Goal

For each of the 4 reviewer slots in `specs/00_framework.md §43.1`, define:

1. **What job function** the slot represents (single, unambiguous responsibility scope).
2. **Which framework §-sections** the slot is the canonical sign-off authority for.
3. **Background / experience profile** the candidate must satisfy.
4. **Acceptance deliverables** the candidate commits to producing before framework promotion to FROZEN.

The proposal's output is a 1-page summary the Owner can hand to a candidate (internal hire, external advisor, or peer-network contact) so the conversation collapses to "yes / no / negotiate scope" rather than "what is the role".

Aligns with `reviewer_staffing_strategy.md §2` (which defines 6 roles across multiple tiers); this proposal specifically maps Tier 1 framework-freeze sign-off into the 4 numbered slots that `§43.1` actually carries.

---

## §2 Proposed roles (4)

### §2.1 FW-H-1 — Software Architecture Lead

| Field | Value |
|---|---|
| Slot in §43.1 | "Revisor Técnico (a nomear)" |
| Job function | Architecture-of-record reviewer for the framework |
| Framework §-coverage | §1 (objetivos) · §2 (princípios, 38 PRINCs) · §3 (escopo) · §4 (estrutura corpus) · §5 (frontmatter) · §6 (identificadores) · §7 (lifecycle two-axis) · §8–§14 (architecture method, C4, INVs, ADRs, formal methods) · §15–§16 (NFR + capability) · §17 (security architecture intersection) · §20 (data architecture) · §41 (meta-rules / evolution) |
| Required background | ≥ 5 years distributed-systems architecture experience. Working knowledge of: Rust, Cloudflare Workers / edge-runtime constraints, REAPI / remote-cache protocols (or analogous protocol-design space), C4 model, TLA+ familiarity (read level minimum), ADR practice, semver discipline. Comfortable challenging PRINC-level claims. |
| Sign-off scope | Approves architectural decisions enumerated in §2 PRINCs and §8–§14 method clauses; co-signs major-version (X.0.0) framework changes per §41.2. |
| Time commitment | ~6–10h for initial v1.0.0 freeze review; ~2–4h per trimestral cycle (§41.5); ad-hoc on major-bump events. |
| Maps to `reviewer_staffing_strategy.md` | §2.1 Architecture Reviewer (Tier 1 P0). |

### §2.2 FW-H-2 — Compliance & Privacy Lead

| Field | Value |
|---|---|
| Slot in §43.1 | (NEW slot — extracted from "Revisor de Produto" scope; see §8 OQ-1) |
| Job function | Compliance / privacy / legal-adjacent reviewer for the framework |
| Framework §-coverage | §18 (privacy by design) · §19 (LINDDUN threat modeling) · §20 (Data Governance, in compliance dimension) · §21 (regulatory compliance frame: GDPR / LGPD / LFPDPPP / CCPA) · §27 (DSR semantics) · §29 (audit-chain semantics from compliance POV) · §30 (legal / DPA / Schrems II) · §32 (data retention) · cross-cuts on PRINC-014 (compliance not afterthought) |
| Required background | ≥ 3 years privacy operations or compliance engineering. Working knowledge of: GDPR (EU), LGPD (Brazil), LFPDPPP (Mexico), CCPA / CPRA (California); DPIA / LIA / TIA practice; DPO certification preferred (IAPP CIPP/E or equivalent). Comfortable with audit-chain immutability semantics and tamper-evident logs from a regulatory-evidence POV (not crypto details — that's FW-H-3). |
| Sign-off scope | Approves compliance matrix delta logic in §21 / §35.5 inheritance pattern; signs off LINDDUN coverage in §19; signs DSR semantics in §27; approves the framework's stance on "privacy violation is fail-CLOSED" rule. |
| Time commitment | ~4–6h for initial v1.0.0 freeze review; ~1–3h per trimestral cycle; ad-hoc on regulatory-landscape change events. |
| Maps to `reviewer_staffing_strategy.md` | §2.3 Privacy / Compliance Reviewer (Tier 2 P0). |

### §2.3 FW-H-3 — Security Lead

| Field | Value |
|---|---|
| Slot in §43.1 | "Revisor de Segurança (a nomear)" |
| Job function | Security-architecture-of-record reviewer for the framework |
| Framework §-coverage | §17 (Zero Trust + secure-by-default; security architecture; tenant-isolation invariants) · §19 (STRIDE + LINDDUN threat modeling from attacker-POV side) · §28 (BYOK / KMS / key custody architecture) · §29 (audit-chain integrity + fail-CLOSED on tamper) · §31 (supply-chain security / SLSA) · §33 (Row-Level Security and analogous boundary enforcement) · §35 (controls / waiver crypto-grade compensation logic) · cross-cuts on PRINC-014 (security not afterthought) and PRINC-038 (waiver expiration as compensating control) |
| Required background | ≥ 5 years application security or security engineering. STRIDE + LINDDUN fluent. OWASP ASVS familiar (level-2 minimum). Cryptography literacy (HMAC, BLAKE3, Argon2id, Ed25519, FIPS 140-3 framing — not necessarily implementation depth). Threat-model authorship experience. Pentest / red-team exposure preferred. Comfortable adjudicating "fail-OPEN vs fail-CLOSED" decisions and articulating compensating controls. |
| Sign-off scope | Approves §17 architecture; signs STRIDE applied to framework invariants; signs §28 BYOK reasoning; signs §29 audit-chain semantics from integrity-attacker POV; co-signs every major-version bump that touches security PRINCs. |
| Time commitment | ~6–10h for initial v1.0.0 freeze review; ~2–4h per trimestral cycle; ad-hoc on new threat-model findings or incident post-mortems that imply framework drift. |
| Maps to `reviewer_staffing_strategy.md` | §2.2 Security Reviewer (Tier 1 P0). |

### §2.4 FW-H-4 — Production Operations Lead

| Field | Value |
|---|---|
| Slot in §43.1 | "Revisor de Produto (a nomear)" *(re-scoped from "Product" to "Production Ops" — see §8 OQ-2)* |
| Job function | Production-readiness / SRE-discipline reviewer for the framework |
| Framework §-coverage | §22 (SRE methodology: SLI / SLO / Error Budget) · §23 (resilience patterns) · §24 (failure modes / FMEA) · §25 (runbook discipline) · §26 (incident response / post-mortem culture) · §38 (adversarial / chaos / pentest cadence) · §39 (PRR gate matrix) · §40 (operational vocabulary) · §41 (trimestral / annual review cadence — operational side) · §43 (revisão / freeze) |
| Required background | ≥ 5 years SRE or production-systems operations. SLO discipline (Google SRE book practice). On-call rotation experience with real incidents. Runbook authorship + dry-run cadence familiarity. Comfortable challenging "shipped" claims when SLO instrumentation or runbook coverage is shallow. Observability stack fluent (OpenTelemetry / Prometheus / structured logs). |
| Sign-off scope | Approves §22 SRE methodology; signs SLO catalog inheritance pattern in §35.5; signs runbook coverage and dry-run cadence requirement in §25; approves PRR gate matrix in §39; signs trimestral / annual review process in §41.5. |
| Time commitment | ~4–6h for initial v1.0.0 freeze review; ~2–4h per trimestral cycle; on-call escalation tier (advisory) on framework-affecting production incidents. |
| Maps to `reviewer_staffing_strategy.md` | §2.4 SRE / Operations Reviewer (Tier 2 P0, elevated to Tier 1 for framework freeze). |

---

## §3 Acceptance criteria per role

Each reviewer commits to a concrete deliverable **before** the framework can promote `v1.0.0-rc1 → v1.0.0` FROZEN. Deliverables are durable artifacts (not slack threads / verbal approvals), per PRINC-005 (rastreabilidade).

### §3.1 FW-H-1 — Software Architecture Lead

- [ ] Comments document `specs/_audits/YYYY-MM-DD-framework-v1-0-0-architecture-review.md` enumerating findings (severity-classified: CRITICAL / HIGH / MEDIUM / LOW / INFO) against §1–§16 + §41.
- [ ] Sign-off line in `specs/00_framework.md §43.1` with date + commit SHA.
- [ ] One-line rationale in the §42 change-log entry for the v1.0.0 bump.
- [ ] Commitment to attend trimestral framework review per §41.5 (or delegate to a named backup).

### §3.2 FW-H-2 — Compliance & Privacy Lead

- [ ] Comments document `specs/_audits/YYYY-MM-DD-framework-v1-0-0-compliance-review.md` enumerating findings (same severity grading) against §18–§21 + §27 + §29 + §30 + §32.
- [ ] Explicit attestation that §19 LINDDUN treatment is sufficient for a v1.0.0 FROZEN framework (or list specific deltas required first).
- [ ] Sign-off line in `specs/00_framework.md §43.1` with date + commit SHA.
- [ ] Commitment to re-attest on every regulatory-landscape change (e.g., new state privacy law landing) OR delegate to named backup.

### §3.3 FW-H-3 — Security Lead

- [ ] Comments document `specs/_audits/YYYY-MM-DD-framework-v1-0-0-security-review.md` enumerating findings (same severity grading) against §17 + §19 (attacker POV) + §28 + §29 + §31 + §33 + §35.
- [ ] STRIDE table covering at least the 12 framework-level INVs (or signed delta against existing per-component STRIDE matrices in `specs/_audits/stride-per-crate/`).
- [ ] Sign-off line in `specs/00_framework.md §43.1` with date + commit SHA.
- [ ] Commitment to incident-driven re-attestation if any SEV-1/SEV-2 production incident implies framework drift.

### §3.4 FW-H-4 — Production Operations Lead

- [ ] Comments document `specs/_audits/YYYY-MM-DD-framework-v1-0-0-production-review.md` enumerating findings (same severity grading) against §22–§26 + §38–§40 + §43.
- [ ] Explicit attestation that §22 SLO methodology + §25 runbook discipline + §39 PRR gate matrix are operationally sound (NOT just doctrinally sound).
- [ ] Sign-off line in `specs/00_framework.md §43.1` with date + commit SHA.
- [ ] Commitment to participate in quarterly framework review per §41.5 (or delegate to named backup).

---

## §4 Onboarding checklist (read order)

A nominated reviewer of any FW-H-* slot follows this read order before producing their comments doc. The order is intentional: framework canonical FIRST (the META-doc), then the ADRs that shaped its evolution, then the invariant registry (which encodes the immutable contracts), then the broader spec corpus, then the runbook index (operational reality check).

1. **Framework canonical** — `specs/00_framework.md` (~3217 lines; 43 sections; budget 4–6h read).
2. **ADR index** — `specs/03_architecture/adrs/` (27 ADRs; budget 2–4h skim, deeper on the ones touching your §-coverage).
3. **Invariant registry** — `specs/03_architecture/invariant_registry.md` (192 INVs, 76 tla_verified; budget 1–2h skim, deep on the INVs your §-coverage signs off).
4. **Section-specific canonical sources** — see `reviewer_staffing_strategy.md §2` mapping table for which canonical sources each role overlaps with.
5. **Audit baseline** — `specs/_audits/sealed/2026-05-15-framework-v1-0-0-ga-audit.md` (this is the DEFER doc you are unblocking) + `specs/_audits/sealed/2026-05-15-ga-readiness-consolidation-wave-13-17.md` (wave-17 baseline).
6. **Runbook index** — `specs/_runbooks/` (124 runbooks; budget 1–2h skim — confirms that the framework's runbook-discipline claims in §25 match operational reality).
7. **(Optional, recommended for FW-H-1 + FW-H-4)** — sample one sprint contract (suggested: `specs/02_sprints/S-01/`) and one PRR (`specs/02_sprints/S-20/prr/`) to ground the meta-doc in actual artifact shapes.

Total budget for a thorough first review: ~12–20 working hours over 2–3 weeks.

---

## §5 Sign-off cadence

### §5.1 Per-section sign-off events

| Event | Trigger | Required signers |
|---|---|---|
| Initial v1.0.0 FROZEN cut | This proposal accepted + 4 reviewers nominated + each completes §3 deliverables | FW-H-1, FW-H-2, FW-H-3, FW-H-4, Gustavo Schneiter (Final Approver) |
| Patch (X.Y.Z+1) | Typo / link-fix; no semantic change per §41.2 | Final Approver only (per §41.2 patch row) |
| Minor (X.Y+1.0) | Adds principle / rule / section per §41.2 | Final Approver + at least the FW-H-* slot whose §-coverage the change touches |
| Major (X+1.0.0) | Inviolable rule change per §41.2 | ALL 4 FW-H-* slots + Final Approver (full re-attestation; §7 thaw cascade) |

### §5.2 Quarterly framework review (per §41.5)

- Cadence: 4× per year (Q1 / Q2 / Q3 / Q4 of each calendar year).
- Format: 60-min synchronous call OR async written round (whichever fits the reviewer roster).
- Output: short delta-only audit doc (`specs/_audits/YYYY-QN-framework-quarterly-review.md`).
- Required attendees: at least 2 of 4 FW-H-* (rotating chair) + Final Approver.

### §5.3 Annual deep review (per §41.5)

- Cadence: once per year (anniversary of v1.0.0 cut).
- Format: full adversarial review per §38 against the framework's own promises.
- Output: full audit doc (length ≥ that of the v1.0.0 freeze audit).
- Required: ALL 4 FW-H-* slots + Final Approver.

---

## §6 RACI matrix (FW-H-* roles × major framework decisions)

Legend: **R** = Responsible (does the work); **A** = Accountable (owns the outcome); **C** = Consulted (input solicited before decision); **I** = Informed (notified after decision). Gustavo = Final Approver, always **A**.

| Decision | Gustavo | FW-H-1 Arch | FW-H-2 Compliance | FW-H-3 Security | FW-H-4 Ops |
|---|---|---|---|---|---|
| v1.0.0 FROZEN cut | A | R | R | R | R |
| Add a new PRINC (PRINC-039+) | A | R | C | C | C |
| Add a new INV class | A | R | C | C | C |
| Add a regulatory regime to §21 | A | C | R | I | I |
| Add a new EVT-type to taxonomy §35.7 | A | C | C | R | C |
| Change BYOK / key-custody model (§28) | A | C | C | R | C |
| Change SLO methodology / PRR gate matrix | A | C | I | C | R |
| Change waiver semantics (§35.6) | A | C | C | R | C |
| New ADR touching framework §-content | A | R if architecture; else C | varies | varies | varies |
| Trimestral review delta-doc | A | R/C | R/C | R/C | R/C |
| Annual deep review | A | R | R | R | R |
| Incident-driven framework re-attestation | A | C | C | R if security-touching; else C | R if ops-touching; else C |
| Major-version bump (X+1.0.0) | A | R | R | R | R |

**Disambiguating rule.** If a decision crosses two or more FW-H-* slots' §-coverage, the slot whose coverage contains the canonical §-section authoring the rule is **R**; the others are **C**. Conflict resolution escalates to Gustavo (A).

---

## §7 Backup / secondary recommendation (bus-factor mitigation)

Each FW-H-* slot SHOULD have a documented `+1 backup` reviewer (a secondary individual capable of stamping in absence of the primary). The backup MUST satisfy the same `Required background` floor and complete the §4 onboarding checklist before they can stamp.

| Slot | Primary | Backup (recommended profile) |
|---|---|---|
| FW-H-1 Arch | (nominate) | A senior staff engineer with 3+ years on the CoreLink codebase OR a peer architect from an adjacent system (cache / build-graph / distributed-blob storage). |
| FW-H-2 Compliance | (nominate) | A fractional DPO retained on advisory contract; OR external counsel with privacy-engineering depth. |
| FW-H-3 Security | (nominate) | A retained AppSec advisor (e.g., the firm engaged for the pre-GA pentest per `specs/_audits/sealed/2026-05-16-pre-ga-pentest-scope.md`). |
| FW-H-4 Ops | (nominate) | A senior on-call engineer once the 24/7 rota lands (see `specs/_audits/sealed/2026-05-14-s20-oncall-24-7-readiness.md`); interim = Gustavo as Owner. |

**Open seat policy.** If a primary departs, the backup becomes acting primary; a new backup nomination is required within 30 days. If a slot is vacant for > 60 consecutive days, the framework auto-demotes from FROZEN to THAWED per §7 lifecycle, triggering the freeze gate again.

---

## §8 Open questions for the Owner

Three decisions are intentionally left to the Owner because they depend on org structure / hiring constraints that an agent cannot observe:

### §8.1 OQ-1 — Org size: 4 distinct people, or some dual-hat?

The `reviewer_staffing_strategy.md §3` baseline assumes Tier 1 = 2 reviewers (Arch + Security). This proposal narrows the framework freeze gate to 4 numbered slots. The Owner must decide:

- **Option A — 4 distinct people**: Strongest separation of concerns; matches §43.1 literal slot count. Recommended for any org > 10 engineers and for orgs operating under SOC2 / ISO27001 separation-of-duty controls.
- **Option B — 3 people with one dual-hat**: e.g., FW-H-1 (Arch) and FW-H-4 (Ops) collapsed into a single "Architecture + SRE" reviewer. Acceptable IF that individual genuinely has both skill profiles; document the dual-hatting in the §42 change-log entry.
- **Option C — 2 people with the Owner dual-hatting two slots**: Owner takes FW-H-2 (Compliance/Privacy) + FW-H-4 (Ops) as interim DPO + interim SRE Lead; external advisors fill FW-H-1 + FW-H-3. Aligns with the existing `reviewer_staffing_strategy.md §7.2` Founder-only-sign-off fallback. Requires an ADR analogous to ADR-0034 explicitly authorizing dual-hatting at the framework-freeze tier.

**Recommendation:** Option A as goal-state; Option C as interim if Option A is not achievable within 60 days post wave-20.

### §8.2 OQ-2 — Re-scope "Revisor de Produto" → "Revisor de Operações"?

The literal text in `00_framework.md §43.1` (unchanged since v0.3.x) names the 3rd "(a nomear)" slot "Revisor de Produto". This proposal argues that for a **framework freeze** specifically, "Product" is the wrong frame: the framework is the META-doc governing engineering process, and its operational surface (SLOs, runbooks, PRR matrix, incident discipline) is exactly what a Production Ops / SRE Lead is qualified to attest — not a Product Manager.

Product input (PRFAQ alignment, capability catalog ergonomics) is genuinely valuable but belongs at **canonical-source** sign-off (`personas.md`, `jtbd.md`, `capabilities.md`) and at **sprint contract** level (per `reviewer_staffing_strategy.md §2.5`), not at framework freeze.

- **Option A (proposed)**: Re-scope slot to "Production Ops Lead" as defined in §2.4 above. Edit §43.1 label accordingly when the slot is populated.
- **Option B**: Keep "Revisor de Produto" literal, add a 5th slot "Revisor de Operações"; renumber FW-H-* accordingly. (Increases the freeze gate from 4 to 5 reviewers.)
- **Option C**: Keep "Revisor de Produto" literal, define it as a Product Manager role; absorb Production Ops attestation into FW-H-1 (Arch) via dual-hatting.

**Recommendation:** Option A. The framework's content profile (3217 lines; 14 of 43 sections are SRE/ops-shaped vs ~2 sections that are product-shaped) supports the re-scope.

### §8.3 OQ-3 — Externally-staffed vs internally-staffed?

For an org of the CoreLink program's current size (solo founder + AI labor), all 4 slots may need external staffing for the foreseeable future. The §3 acceptance criteria are designed to be **executable by an external advisor** on retainer (4–10h/month commitments). The Owner must decide:

- **All internal**: requires hiring 4 specialized engineers in advance of GA; likely infeasible.
- **Hybrid (recommended)**: 1–2 external retained advisors (FW-H-1 + FW-H-3 ideal external candidates) + Owner dual-hatting FW-H-2 / FW-H-4 as interim per OQ-1 Option C, with a 12-month plan to backfill internally.
- **All external**: feasible; risk = advisor turnover; mitigated by §7 backup requirement.

**Recommendation:** Hybrid. Most actionable given current resource envelope.

---

## §9 Acceptance of this proposal

This proposal itself does NOT change `00_framework.md` semantics — it only references roles and provides §43.1 with a forward-pointer to a defined role catalog. Acceptance pathway:

1. **Owner reviews this doc** and selects choices for OQ-1, OQ-2, OQ-3.
2. **Owner nominates** 4 individuals (or 3 with documented dual-hat per OQ-1) matching the §2 role profiles.
3. **Each nominee** completes §4 onboarding read order, produces §3 deliverable, signs §43.1.
4. **Framework promotes** v1.0.0-rc1 → v1.0.0 FROZEN in a follow-up commit; §42 change-log entry attests to nominee names + SHAs.
5. **This proposal** transitions `doc_status: DRAFT → ACTIVE` once it is referenced by the §43.1-populated FROZEN cut.

Until then, the proposal sits as DRAFT alongside `reviewer_staffing_strategy.md` and unblocks Path A in the wave-18 Lote 6 audit §6.3.

---

## §10 Change log + addendum cross-reference

### §10.1 Wave-22 addendum (operating policy)

The operating mechanics for executing this proposal in a small-org regime — dual-hat fallback formalization (OQ-1 Option C), cross-veto rule, sign-off SLA, reviewer training pack budget, 90-day rolling quarterly cadence, conflict-of-interest declaration — are captured in the wave-22 follow-on addendum:

- `specs/_proposals/2026-05-16-framework-reviewer-roles-addendum.md` v0.1.0 (DRAFT alongside this proposal; transitions ACTIVE jointly at FROZEN cut).

The addendum does NOT redefine FW-H-1..4 role scopes (§2.1–§2.4 of this proposal remain canonical); it specifies HOW the gate operates when staffing is constrained, when reviewers disagree, when sign-off stalls, when a reviewer onboards, when the framework drifts between freezes, and when reviewers have personal stake in artifacts under review. Owner reads both documents together at the same governance checkpoint.

### §10.2 Change log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 0.1.0 | 2026-05-16 | Gustavo Schneiter (via Claude Opus 4.7, wave-20 Lote 7) | Initial proposal. Defines 4 canonical roles for `00_framework.md §43.1` slots FW-H-1..4 to unblock Path A in wave-18 Lote 6 GA-freeze audit. Complements (does not supersede) `_governance/reviewer_staffing_strategy.md` v0.1.0. |
| 0.1.0+wave22 | 2026-05-16 | Gustavo Schneiter (via Claude Opus 4.7, wave-22 Lote 7 absorption) | Added §10.1 cross-reference to addendum `specs/_proposals/2026-05-16-framework-reviewer-roles-addendum.md` v0.1.0. No semantic change to FW-H-* role scopes; addendum operates downstream of this proposal as operating-policy layer. |

---

**Cross-references:**

- `specs/00_framework.md §43.1` — slots updated in same wave-20 commit to link here.
- `specs/_proposals/2026-05-16-framework-reviewer-roles-addendum.md` — wave-22 addendum specifying operating policy (dual-hat fallback, veto, SLA, training, cadence, COI).
- `specs/_audits/sealed/2026-05-15-framework-v1-0-0-ga-audit.md §unblock-path-C-progress` — wave-20 update logs landing of this proposal as engineering-side READY; wave-22 update notes the addendum extends coverage to small-org case.
- `specs/_governance/reviewer_staffing_strategy.md` — multi-tier strategy SOT; this proposal narrows Tier 1 framework-freeze gate.
- `specs/03_architecture/adrs/ADR-0034-prr-staffing-waiver-solo-tier.md` — analogous waiver pattern (PRR-scoped); informs OQ-1 Option C fallback; addendum §1.1 mandates an analogous ADR when dual-hat mode is invoked.
