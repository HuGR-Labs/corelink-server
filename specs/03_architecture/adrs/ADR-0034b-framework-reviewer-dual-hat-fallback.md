---
id: "ADR-0034b"
type: "adr"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "0.1.1"
created: "2026-05-16"
updated: "2026-05-16"
title: "Framework Reviewer Dual-Hat Fallback Policy (FW-H-1..4 small-org operating mode)"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
deciders: ["Gustavo Schneiter (Owner / Final Approver)"]
context_links:
  - "specs/_proposals/2026-05-16-framework-reviewer-roles.md §8 OQ-1 (org size / dual-hatting)"
  - "specs/_proposals/2026-05-16-framework-reviewer-roles-addendum.md §1 (dual-hat fallback policy)"
  - "specs/_proposals/2026-05-16-framework-reviewer-roles-addendum.md §6 (detailed RACI matrix — wave-25)"
  - "specs/03_architecture/adrs/ADR-0034-prr-staffing-waiver-solo-tier.md (prior-art at PRR tier)"
  - "specs/00_framework.md §43.1 (Revisores requeridos para freeze)"
  - "specs/_governance/reviewer_staffing_strategy.md §7.2 (Founder-only-sign-off fallback prior art)"
  - "specs/_audits/2026-05-15-framework-v1-0-0-ga-audit.md §11 (unblock-path-C-progress)"
tags: ["adr", "governance", "reviewers", "staffing", "framework-freeze", "dual-hat", "small-org", "ga", "wave-22", "wave-24", "wave-25", "raci"]
---

# ADR-0034b — Framework Reviewer Dual-Hat Fallback Policy

## Status

**PROPOSED** — 2026-05-16. Transitions to **ACCEPTED** when the Owner explicitly invokes the dual-hat fallback (recorded in the framework §42 change-log entry that ratifies the v1.0.0 FROZEN cut under Pairing-Alpha or Pairing-Beta). Revisits required at org-size threshold transition (auto-expiration triggers in §Decision below).

`doc_status: DRAFT` in front matter reflects the schema enum (the schema does not enumerate `PROPOSED`); the operational semantics are PROPOSED as documented in this Status section. The doc transitions to `doc_status: ACTIVE` simultaneously with Owner invocation.

## Context

### Problem

The wave-20 Lote 7 reviewer-roles proposal (`specs/_proposals/2026-05-16-framework-reviewer-roles.md`) defines four canonical framework-freeze reviewer slots:

- **FW-H-1** — Software Architecture Lead
- **FW-H-2** — Compliance & Privacy Lead
- **FW-H-3** — Security Lead
- **FW-H-4** — Production Operations Lead

The proposal's §8 Open Question OQ-1 enumerated three staffing options:

- **Option A** — 4 distinct people (goal-state).
- **Option B** — 3 people with one slot dual-hatted.
- **Option C** — 2 people (Owner dual-hats two slots; 2 externals fill remaining slots).

CoreLink operates in a small-org regime today: solo Owner + AI labor + 1–2 retained external advisors. Hiring 4 distinct senior reviewers (FW-H-1..4 each requires a senior IC or equivalent) is **not contractually feasible** prior to the first customer-driven SOC2 / ISO27001 mandate. Without a formal fallback, the framework v1.0.0 FROZEN cut stays indefinitely DEFERRED (per `specs/_audits/2026-05-15-framework-v1-0-0-ga-audit.md §6.3 Path A/B/C`), even though the engineering side is READY (wave-20 Lote 7 absorption documented in audit §11).

### Why an ADR (not just the addendum)

The wave-22 Lote 7 framework-reviewer-roles addendum §1 specifies the dual-hat fallback as **operating policy**. ADR-0034 (PRR staffing waiver) establishes the **pattern** that solo-tier waiver decisions deserve their own ADR with explicit boundaries, audit trail, and auto-expiration triggers. This ADR (0034b) is the analogous artifact at the **framework-freeze tier** — the addendum §1.1 item 4 explicitly mandates "an ADR analogous to ADR-0034 is authored authorizing dual-hatting at the framework-freeze tier".

ADR-0034 governs **PRR sign-off** waivers (per-sprint, 11 slots). ADR-0034b governs **framework FROZEN cut** sign-offs (one-time-per-major-version, 4 slots). The two ADRs share the spirit (named waiver paths > silent skips) but operate at different tiers and over different cadences.

### Why now (wave-24)

Wave-22 absorbed the addendum into `_proposals/`. The addendum §1.1 item 4 forward-referenced an ADR that did not yet exist. Wave-24 (this ADR) closes that forward reference so that the Owner can invoke the dual-hat fallback with a complete artifact chain (proposal → addendum → ADR) rather than authoring the ADR at invocation time under decision pressure.

## Decision

The Owner MAY invoke a **dual-hat fallback** for the framework v1.0.0 FROZEN cut, taking two FW-H-* slots personally with the remaining two slots filled by retained external advisors, **iff all conditions below hold**.

### §Eligibility conditions

All five conditions are mutually required:

1. **Org headcount** is strictly less than **N = 12 engineers** (where engineer = full-time technical IC or equivalent). N=12 is the threshold proposed by this ADR; it sits above the addendum §1.1's "headcount < 10" trigger because the auto-expiration trigger (§1.4) is the lower bound and the eligibility ceiling is the upper bound, giving a 2-engineer hysteresis band to avoid oscillation around the threshold.
2. **SOC2 / ISO27001 separation-of-duty controls** are NOT yet contractually required by a customer or regulator. Once contractually required, dual-hat collapses to 4 distinct staffing or a documented external-auditor compensating control.
3. **At least 2 external advisors** are retained (per `_governance/reviewer_staffing_strategy.md §7.2` and the addendum §8.3 OQ-3 Hybrid recommendation) and willing to fill the two externally-staffed slots under a written engagement (retainer, scope-of-work, or comparable).
4. **The Owner self-attests** working competence in the two slots they intend to dual-hat (recorded in the §42 change-log entry that ratifies the FROZEN cut).
5. **This ADR is referenced** in the §42 change-log entry as the authorizing decision (`authorizing_adr: ADR-0034b`), with the chosen pairing named (`pairing: alpha | beta`).

### §Permitted pairings

Only two pairings are permitted. Both preserve cross-domain coverage (one structural/technical slot + one compliance/operational slot per person), avoiding the failure mode where one individual signs off on a domain with no independent perspective.

| Pairing | Owner takes | External advisors fill | When to choose |
|---|---|---|---|
| **Pairing-Alpha** (recommended) | FW-H-1 (Architecture) + FW-H-3 (Security) | FW-H-2 (Compliance/Privacy) + FW-H-4 (Production Ops) | Founder authored the architecture and threat model; externals attest the regulatory landscape (which the founder is least current on) + production-ops reality (where founder bias is strongest). |
| **Pairing-Beta** (alternative) | FW-H-2 (Compliance/Privacy) + FW-H-4 (Production Ops) | FW-H-1 (Architecture) + FW-H-3 (Security) | Founder has stronger compliance/ops background than architecture/security; aligns with `reviewer_staffing_strategy.md §7.2` Founder-only-sign-off prior-art shape. |

### §Forbidden pairings

Three pairings are explicitly forbidden, with rationale rooted in conflict-of-interest theory and the addendum §7 recusal mechanics (renumbered from §6 in addendum v0.1.2 / wave-25 RACI detail; the recusal scope rows are now in §7.2):

| Forbidden pairing | Owner would take | Why forbidden |
|---|---|---|
| **FW-H-1 + FW-H-2** | Architecture + Compliance | Architecture **writes** the canonical sections that Compliance **reviews** for regulatory fit. Collapsing both into one person creates a conflict of interest (COI) — the author validates their own work against compliance criteria. Mirrors `_proposals/...-addendum.md §1.2` first forbidden row + §7.2 recusal trigger #1 (renumbered from §6.2 in addendum v0.1.2 / wave-25 RACI detail). |
| **FW-H-3 + FW-H-4** | Security + Production Ops | These two slots most frequently show **joint R-status** in the addendum proposal §6 RACI (security + ops are co-Responsible on incident response, on-call posture, runbook discipline). Collapsing them loses adjudication signal on incident-driven re-attestation rows. Mirrors addendum §1.2 second forbidden row. |
| **FW-H-1 + FW-H-3** under non-Owner dual-hatting | Architecture + Security (taken by an external advisor, not the Owner) | Only the Owner may take Pairing-Alpha because the addendum §1.3 sign-off mechanics + framework §43.1 imply Final-Approver-level accountability for both seats; an external advisor cannot bind Final-Approver authority across both. (Pairing-Alpha is permitted ONLY when the Owner is the dual-hatter.) |

### §Cross-veto rights under dual-hat

Per the addendum §2, **any reviewer (including a dual-hatted Owner)** can record a `vote: BLOCK` finding. Specifically:

- Each external advisor has full cross-veto authority on cross-cutting findings (addendum §2.2), including blocks on sections OUTSIDE their primary §-coverage.
- The Owner under dual-hat MAY record BLOCK on either of their two slots; if the Owner records BLOCK in one slot and APPROVE in the other, the BLOCK stands until withdrawn (addendum §2.1).
- **Tie-break under dual-hat**: if an external advisor records `vote: BLOCK` that the Owner refuses to remediate, the addendum §2.3 Owner-tie-break clause still applies — the Owner may sustain, override, or re-staff. Overrides are recorded as `dissent_recorded` in the v1.0.0 metadata (PRINC-005 audit trail; reviewer's BLOCK stays in the comments-doc).
- **External advisor as tie-breaker**: when both external advisors APPROVE and the Owner's two slots split (one APPROVE, one BLOCK), the dispute escalates to an **external advisor (not the dual-hat-conflicted ones)** acting as a neutral arbitrator per the §-coverage of the disputed finding. If no such advisor is retained, the cut defers and the slot is re-staffed (addendum §2.3 last bullet).

### §Quorum under dual-hat

Per the addendum §2.4: a v1.0.0 FROZEN cut under dual-hat requires affirmative `vote: APPROVE` from **3 of 3 effective seats** — i.e., the Owner APPROVEs **both** of their slots (counted as 2 of the 3 effective seats) AND the 2 external advisors both APPROVE. ABSTAIN by either external advisor reduces the quorum below the threshold and the cut defers. (Under 4-distinct-staffing, the addendum permits 3-of-4 APPROVE with one ABSTAIN; under dual-hat, the tighter 3-of-3 effective-seats rule applies because the dual-hatter cannot ABSTAIN — silence on either slot would collapse the framework's own §7 Aprovador-Final-plus-N-reviewers rule.)

### §Sign-off mechanics under dual-hat

Per the addendum §1.3:

- The framework §43.1 sign-off block records the Owner's name + commit SHA **twice** (once per slot), with role IDs labeled (e.g., `FW-H-1 / FW-H-3 — Gustavo Schneiter — 2026-MM-DD — sha:<7-char>`).
- The §3 acceptance deliverable (separate comments documents per role) is still required — **two distinct comments docs** even when one person authors both, to preserve PRINC-005 rastreabilidade and avoid single-doc rubber-stamp collapse.
- The §42 change-log entry MUST disclose the conflict explicitly per the addendum §7.4 (`owner_conflict_disclosed: dual-hat under ADR-0034b Pairing-<Alpha|Beta>`; addendum subsection renumbered from §6.4 in v0.1.2 / wave-25 RACI detail).

### §Auto-expiration triggers

Dual-hat mode auto-expires (framework auto-demotes FROZEN → THAWED per §7 lifecycle, retriggering the freeze gate with 4-distinct-staffing) when ANY of:

| Trigger | Threshold | Source |
|---|---|---|
| Headcount growth | **≥ 10 engineers** (note: lower than the §Eligibility ceiling of N=12; hysteresis band 10–12 prevents oscillation) | Addendum §1.4 |
| First contractual SOC2 / ISO27001 audit kickoff | Customer or regulator triggers separation-of-duty | Addendum §1.4 |
| Time ceiling | **18 months** elapsed since this ADR's `created` date OR Owner-invocation date (whichever later) | Addendum §1.4 hard ceiling |
| Owner role transition | Owner steps down from Final Approver | Addendum §1.4 |

Triggers are monitored by the **trimestral framework review** (addendum §5.2) — the chair inspects org headcount, customer-contract pipeline, and elapsed time at each quarterly review.

### §Status transition

| Field | Initial value (this commit) | Transitions on |
|---|---|---|
| `doc_status` | `DRAFT` (schema-bound; semantically PROPOSED) | `ACTIVE` when Owner invokes the fallback in a framework §42 change-log entry. |
| `audit_status` | `ACTIVE` | unchanged on invocation; flips to `AUDIT_PENDING` at first quarterly framework review post-invocation. |
| Operational status (this §) | **PROPOSED** | **ACCEPTED** on Owner invocation. **SUPERSEDED-EXPIRED** on any auto-expiration trigger firing. **REVISITED** at each org-size / SOC2 / 18-month / Owner-transition review checkpoint. |

## Alternatives considered

### Alternative 1 — Hire 4 distinct senior reviewers (Option A from proposal §8 OQ-1)

**Cost.** Four senior ICs at framework-tier seniority is ~USD 1.2–1.8M/yr fully-loaded. CoreLink pre-GA does not have revenue to support this hiring envelope.

**Time.** Each FW-H-* slot requires the §4 onboarding read (10-hour floor per addendum §4.2 + addendum proposal §4 originally cited 12–20 hours) plus §3 acceptance deliverables (~5 BD each) — minimum 6–8 weeks per nominee to onboarding completion. Four parallel nominees push v1.0.0 cut to T+8 weeks at best, and the program cannot tolerate that latency at the current GA-readiness inflection point.

**Verdict.** Goal-state, not present-state. Documented in the §Eligibility auto-expiration ladder as the post-dual-hat state.

### Alternative 2 — External rotating reviewers (no dual-hat, no permanent staffing)

**Mechanism.** Each FW-H-* slot filled by a different external advisor per cut, drawn from a pool.

**Continuity risk.** Framework decisions span quarterly cadence (addendum §5); rotating reviewers lose context between cuts. The addendum §4 Reviewer Training Pack (10-hour floor) assumes a stable reviewer who can amortize onboarding across multiple decisions. Rotating reviewers repay the onboarding cost on every cut — economically and practically prohibitive.

**Audit trail risk.** SOC2 readiness expects identifiable reviewer continuity (per `specs/_audits/2026-05-14-soc2-readiness-score.md`). Rotating reviewers fragment the audit trail.

**Verdict.** Rejected — addendum §4 + §5 explicitly assume continuity.

### Alternative 3 — Single-reviewer simple-majority (Owner + 1 external advisor only)

**Mechanism.** Collapse to 2 effective seats (Owner as 1, external as 1); simple-majority requires 2/2 APPROVE (i.e., unanimous between 2 people).

**Defense-in-depth shortfall.** The framework §7 rule (Aprovador Final + ≥ N revisores requeridos) implies N ≥ 1 distinct from Final Approver; collapsing to a single external advisor satisfies the letter of the rule but loses the **cross-domain coverage** that 4 FW-H-* slots provide (architecture / compliance / security / production-ops are each substantive distinct concerns).

**Risk concentration.** A single external advisor concentrates audit-trail risk on one individual — bus-factor 1 on the entire external review side.

**Verdict.** Rejected — fails the addendum §2.4 quorum logic (3-of-4 or 3-of-3-effective minimum) AND the addendum §1.2 cross-domain-coverage requirement.

### Alternative 4 — Indefinite DEFER (status quo)

**Mechanism.** Keep framework at v1.0.0-rc1 indefinitely; revisit only when the org grows to 12+ engineers.

**Cost.** GA verdict stuck on DEFER indefinitely; downstream sprint-contract / runbook / ADR work continues to reference an unreleased framework. Audit `specs/_audits/2026-05-15-framework-v1-0-0-ga-audit.md` §6.3 explicitly rejects this as the steady-state.

**Verdict.** Rejected — the addendum and this ADR exist specifically to terminate indefinite-DEFER.

## Consequences

### Positive

- **Small-org GA viable.** With Pairing-Alpha or Pairing-Beta invocable, the framework v1.0.0 FROZEN cut becomes achievable without waiting for the goal-state 4-distinct hire — closing the audit §6.3 Path A engineering-side-READY gap that wave-20 Lote 7 and wave-22 Lote 7 narrowed but did not eliminate.
- **Audit trail preserved.** Owner conflict disclosure (addendum §7.4 — renumbered from §6.4 in wave-25), per-slot comments docs (addendum §1.3), and explicit pairing-name + authorizing-ADR reference in the §42 change-log entry produce a SOC2-readable trail of WHO signed WHAT under WHICH waiver.
- **Hysteresis band prevents oscillation.** Eligibility ceiling (N=12) above auto-expiration trigger (≥10) gives the Owner a 2-engineer buffer to plan a graceful transition to 4-distinct staffing rather than thrashing on the threshold.
- **Pattern reuse.** This ADR mirrors ADR-0034's structure at a different tier; future framework-tier waiver decisions can follow the same pattern (named pairings + forbidden pairings + auto-expiration + cross-veto preservation).

### Negative

- **More single-points-of-failure on dual-hat slots.** The two slots the Owner holds carry bus-factor 1 (the Owner). If the Owner is unavailable for an incident-driven re-attestation, those two §-coverages have no backup. (Addendum §7 backup policy applies to the externally-staffed slots, not the Owner-held slots.)
- **Conflict of interest concentrated on the Owner.** The addendum §7.4 (renumbered from §6.4 in wave-25) acknowledges this is structural (Final Approver cannot recuse). The disclosure requirement mitigates the audit-trail risk but does not eliminate the substantive COI risk.
- **Cross-veto effectiveness depends on external advisor courage.** External advisors hired on retainer face a structural incentive to APPROVE rather than BLOCK (preserving the engagement). The addendum §2.1 + §2.3 cannot fully neutralize this. Mitigation: written engagement scope MUST include explicit BLOCK authority (per `_governance/reviewer_staffing_strategy.md` retainer-template).
- **Auto-expiration to 4-distinct may be late.** At org-size 10–12 (the hysteresis band), the Owner has limited time to hire and onboard 4 distinct reviewers before the next framework cut (90-day quarterly cadence). Mitigation: the quarterly review chair flags approaching expiration triggers at least one quarter in advance.

### Neutral

- The §43.1 line count under dual-hat is unchanged (still 4 FW-H-* entries, plus the Owner-as-Final-Approver line) — the names just collapse.
- The §3 acceptance deliverable doc count under dual-hat is unchanged (still 4 distinct comments docs) — the authorship just collapses (Owner authors 2 of the 4).
- Framework §7 lifecycle is unchanged. THAWED → FROZEN cascade fires identically regardless of staffing mode.

## CI / validation hooks

This ADR does not introduce a new CI gate. The existing `validate_specs.py` + `validate_references.py` (run by `make validate-canonical` per the canonical-consistency baseline) cover schema conformance.

Forward-looking: when a framework v1.0.0 FROZEN cut is authored under this ADR, the change-log entry SHOULD include the fields enumerated in §Eligibility item 5 (`authorizing_adr: ADR-0034b`, `pairing: alpha | beta`, `owner_conflict_disclosed: ...`). A future CI gate (out of scope for this ADR) MAY parse these fields from the change-log entry.

## References

- `specs/_proposals/2026-05-16-framework-reviewer-roles.md` — wave-20 proposal defining FW-H-1..4 (the slots this ADR governs the staffing of).
- `specs/_proposals/2026-05-16-framework-reviewer-roles-addendum.md §1` — wave-22 addendum dual-hat fallback (this ADR is the §1.1 item-4-required artifact).
- `specs/03_architecture/adrs/ADR-0034-prr-staffing-waiver-solo-tier.md` — PRR-tier prior-art pattern (ADR-0034b is the analogous artifact at framework-freeze tier).
- `specs/00_framework.md §43.1` — slot labels the dual-hat applies to.
- `specs/_governance/reviewer_staffing_strategy.md §7.2` — Founder-only-sign-off prior art.
- `specs/_audits/2026-05-15-framework-v1-0-0-ga-audit.md §11` — `unblock-path-C-progress` (this ADR + the wave-22 addendum together complete the engineering-side preparation for Path A).

## Change log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 0.1.0 | 2026-05-16 | Claude Opus 4.7 (wave-24 ADR-0034b authoring agent) | Initial ADR. Formalizes the dual-hat fallback policy at the framework-freeze tier (analog of ADR-0034 at PRR tier). §Eligibility (5 conditions; N=12 ceiling with 10-engineer auto-expiration hysteresis); §Permitted pairings (Alpha + Beta) and §Forbidden pairings (FW-H-1+FW-H-2 COI; FW-H-3+FW-H-4 RACI collapse; non-Owner Alpha); §Cross-veto under dual-hat preserves addendum §2 rights; §Quorum 3-of-3-effective seats; §Sign-off mechanics per addendum §1.3 (two-comments-docs + Owner conflict disclosure); §Auto-expiration ladder mirroring addendum §1.4; §Status PROPOSED (DRAFT in schema) → ACCEPTED on Owner invocation. §Alternatives 1–4 documented (4-distinct hire / rotating / single-reviewer simple-majority / indefinite DEFER) with explicit-cost rejection rationale. |
| 0.1.1 | 2026-05-16 | Claude Opus 4.7 (wave-25 Lote 7 RACI detail cross-ref sync) | Updates addendum cross-refs from `§6.2 / §6.4` (COI subsections) to `§7.2 / §7.4` after the wave-25 addendum v0.1.2 renumber that inserted detailed RACI matrix as new §6. Adds context_links pointer to addendum §6 (RACI detail). No semantic change to this ADR's decision content (eligibility / pairings / quorum / auto-expiration unchanged); cross-ref hygiene only. |
