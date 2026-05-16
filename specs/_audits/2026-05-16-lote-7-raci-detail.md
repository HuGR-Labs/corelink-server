---
id: "AUDIT-2026-05-16-LOTE-7-RACI-DETAIL"
type: "audit"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-16"
updated: "2026-05-16"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inv: []
gap: null
references:
  - "specs/00_framework.md"
  - "specs/_proposals/2026-05-16-framework-reviewer-roles.md"
  - "specs/_proposals/2026-05-16-framework-reviewer-roles-addendum.md"
  - "specs/03_architecture/adrs/ADR-0034b-framework-reviewer-dual-hat-fallback.md"
  - "specs/03_architecture/adrs/ADR-0034-prr-staffing-waiver-solo-tier.md"
  - "specs/_audits/2026-05-15-framework-v1-0-0-ga-audit.md"
  - "specs/_governance/reviewer_staffing_strategy.md"
tags: ["audit", "lote-7", "wave-25", "governance", "reviewers", "raci", "framework-freeze", "ga", "dual-hat"]
---

# Lote 7 RACI matrix detail — wave-25 audit (FW-H-1..4 × 15 framework decisions)

> **Purpose.** Document the rationale behind the 15-row detailed RACI matrix that wave-25 authors into `specs/_proposals/2026-05-16-framework-reviewer-roles-addendum.md §6`. The wave-22 addendum and the wave-24 `ADR-0034b` both reference "the addendum proposal §6 RACI" but only the 13-row coarse summary in the original wave-20 proposal existed at the time. Wave-25 closes that gap with a per-decision-class detail bound to ADR-0034b's permitted dual-hat pairings, with single-A discipline enforced and a dedicated dual-hat fallback row showing A/R-cell migration when the Owner takes Pairing-Alpha or Pairing-Beta.

---

## §1 Scope

This audit doc accompanies the wave-25 addendum v0.1.2 commit (`specs/_proposals/2026-05-16-framework-reviewer-roles-addendum.md`) that:

1. **Inserts §6 RACI matrix detail** (15 decision rows × 5 cells per FW-H-* slot, plus a dual-hat fallback row).
2. **Renumbers** the addendum subsections: existing §6 (COI declaration) → §7; §7 (summary table) → §8; §8 (acceptance) → §9; §9 (changelog) → §10. The §8 summary table is updated to reflect the seven (was six) operating-policy clauses.
3. **Updates ADR-0034b cross-refs** from `addendum §6.2 / §6.4` to `§7.2 / §7.4` (COI subsections renumbered).
4. **Bumps ADR-0034b version** 0.1.0 → 0.1.1 with a context_links entry pointing at the new addendum §6, and a change log entry recording the cross-ref hygiene update.

---

## §2 Why a detailed matrix (not just the original proposal §6 summary)

### §2.1 Granularity mismatch

The original wave-20 proposal §6 RACI lists 13 high-level rows: PRINC additions, INV class additions, regulatory regime additions, BYOK changes, SLO methodology changes, etc. These are decision **classes**, not specific decisions. The wave-22 addendum §1.2 forbidden-pairing argument requires per-decision-row reasoning ("FW-H-3+FW-H-4 collapses joint R-status on the **incident-driven re-attestation** rows specifically") that the 13-row summary cannot support — the summary aggregates security + ops behavior across multiple decision types that have meaningfully different RACI shapes.

### §2.2 Operating-policy binding

Three addendum clauses depend on a concrete per-decision RACI:

- **§2 cross-veto rule** — needs to know which rows have R-status held by whom to enumerate cross-domain BLOCK paths.
- **§3 sign-off SLA** — STANDARD vs HIGH_RISK lane routing depends on which decision class the row falls into; with a flat 13-row summary, the routing is implicit.
- **§7 (post-renumber) conflict-of-interest** — recusal scope ("a reviewer recuses on rows that touch their own authored work") requires identifiable decision rows.

ADR-0034b's §Forbidden pairings rationale ("FW-H-3+FW-H-4 collapses joint R-status in proposal §6 RACI") was operating on the assumption that a per-row decomposition exists. Wave-25 makes the assumption explicit.

### §2.3 Single-A enforcement at scale

RACI matrix literature (e.g., Hunter & Westerveld "Project Communication") flags multi-A as the most common matrix defect. The original 13-row summary has Owner = A on every row (good); but as the matrix scales to 15+ rows with mixed Owner-dual-hat and external-advisor cells, the discipline becomes harder to enforce without an explicit policy. The §6 of the addendum reasserts the rule with a footnote ("single-A discipline includes 'single-A-or-explicit-deferral' — no TBD cells").

---

## §3 The 15 decision rows: per-row rationale

This section documents why each row's R / C / I cells are assigned as they are. The matrix itself is in addendum §6.2; this section is the **rationale-of-record** for future auditors / contributors who might re-staff or re-RACI.

### §3.1 Row 1 — ADR creation

Owner = A (Final Approver always). R varies by ADR subject: an architecture ADR (e.g., sprint contract pattern ADRs) is FW-H-1 R; a security ADR (e.g., BYOK provider ADRs) is FW-H-3 R; a compliance ADR is FW-H-2 R; an ops ADR is FW-H-4 R. The C cells fire on whichever slots are cross-cut. Conditional R-cell wording ("R if architecture; C otherwise") preserves single-A discipline because only one R cell fires per concrete ADR — the matrix expresses a **decision-class** with conditional dispatch into specific rows at runtime.

### §3.2 Row 2 — ADR approval

All FW-H-* slots are C (consulted) regardless of subject. Owner is A. Rationale: ADR approval is a procedural gate (does the ADR meet the framework's ADR template + has the relevant slot signed off?). The substantive sign-off happens in Row 1 R-cell authorship; Row 2 is the formal status transition. Making it all-C avoids double-A vs Row 1.

### §3.3 Row 3 — INV registry promotion (DRAFT → PROVEN / tla_verified)

FW-H-1 = R because invariant promotion is fundamentally an architecture-of-record claim ("this invariant holds in the system as designed"). Owner is A. Security and Ops are C **only when the INV touches their domain** (a security INV gets FW-H-3 C; an SLI/SLO INV gets FW-H-4 C; an architecture-only INV gets only FW-H-1 R). FW-H-2 = I always (compliance does not gatekeep INV promotions; it consumes the registry as evidence).

### §3.4 Row 4 — Sprint impl sign-off

FW-H-4 = R because the sprint impl gate is fundamentally about PRR (`specs/_governance/reviewer_staffing_strategy.md §2.5`) — observability, SLO instrumentation, runbook readiness, on-call posture. Architecture / Compliance / Security are C as cross-cut reviewers but the **owner of the gate** is operations. This is one of the rows where wave-25 disambiguates a pattern the wave-22 forbidden-pairing rationale relied on (collapsing FW-H-3 + FW-H-4 would lose the FW-H-4 R-cell here, with FW-H-3 absorbing it under a security frame, mis-routing PRR signal).

### §3.5 Row 5 — DEBT register entry

FW-H-4 = R because the DEBT register (`_audits/2026-05-15-debt-register.md`) is fundamentally an ops-reality artifact (which planned work is provisionally accepted as DEBT and which is being actively worked). Compliance / Security become C **only when the DEBT entry touches their domain** (a compliance DEBT entry like DEBT-016-statuspage-urls gets FW-H-2 C if PII-touching; a security DEBT gets FW-H-3 C).

### §3.6 Row 6 — Runbook approval

FW-H-4 = R (canonical SRE responsibility per `00_framework.md §25`). FW-H-3 = C only for security-runbooks (e.g., key rotation runbook, breach-response runbook). FW-H-1 and FW-H-2 = I — runbook authorship is operational, not architectural or compliance-driven.

### §3.7 Row 7 — TLA+ spec addition

FW-H-1 = R because formal methods spec authorship lives in architecture. FW-H-3 = C only when the spec encodes a security property (e.g., audit-chain monotonicity, BYOK key-custody invariant). FW-H-2 and FW-H-4 = I.

### §3.8 Row 8 — BYOK provider addition

FW-H-3 = R (key-custody architecture is fundamentally a security concern per `00_framework.md §28`). FW-H-2 = C (DPA review needed for new sub-processor / KMS provider). FW-H-1 = C (architectural fit). FW-H-4 = C (operational SLO impact — new provider may shift latency / availability). This is one of the rows where the **FW-H-1+FW-H-3 forbidden-pairing under non-Owner dual-hatting** rationale (ADR-0034b §Forbidden pairings) bites: collapsing architecture + security onto a non-Owner advisor would absorb both the C-cell and the R-cell here without Final-Approver authority.

### §3.9 Row 9 — Region addition

FW-H-2 = R because new region = new regulatory regime + data-residency / DPA implications. FW-H-1 = C (architectural fit; cross-region tenant-isolation INVs). FW-H-3 = C (security implications of new region's regulatory boundary, e.g., LGPD vs LFPDPPP threat-model deltas). FW-H-4 = C (latency SLO, on-call coverage).

### §3.10 Row 10 — Schema migration

FW-H-1 = R (architecture-of-record on canonical-source schemas). FW-H-2 = C for PII-schema migrations (e.g., adding a new PII field, changing retention semantics). FW-H-3 = C for security-schema migrations (e.g., adding an audit-chain field). FW-H-4 = C for migration rollout posture (zero-downtime vs maintenance-window).

### §3.11 Row 11 — Customer breach response

FW-H-2 = R because customer breach response is fundamentally a regulatory-notification artifact (GDPR Art. 33/34 timelines; LGPD ANPD notification; per `00_framework.md §27 + §30`). FW-H-3 = C (forensics + incident response coordination). FW-H-4 = C (status-page + comms cadence). FW-H-1 = I.

### §3.12 Row 12 — Pentest finding triage

FW-H-3 = R (canonical security responsibility — severity classification + remediation owner assignment). FW-H-1 = C for architectural remediation. FW-H-4 = C for operational mitigations (WAF rules, rate-limit tightening). FW-H-2 = I.

### §3.13 Row 13 — GA cutover sign-off

All four FW-H-* slots = R; Owner = A. This is the v1.0.0 FROZEN cut row (original proposal §5.1 "Initial v1.0.0 FROZEN cut" event). The original proposal already encodes this as all-R (proposal §6 first row "v1.0.0 FROZEN cut"); wave-25 preserves the encoding and adds it as Row 13 of the detailed matrix.

### §3.14 Row 14 — Quarterly framework review

R rotates by quarter per addendum §5.4 (Q1 = FW-H-1, Q2 = FW-H-2, Q3 = FW-H-3, Q4 = FW-H-4). The non-chair slots are C (must be reachable for the async/sync round). Owner = A.

### §3.15 Row 15 — Annual deep review

All four FW-H-* slots = R; Owner = A. Same pattern as Row 13 — annual deep review is the framework-tier equivalent of a v1.0.0 cut (addendum §5.5 explicitly interleaves the annual deep review with the 4th quarterly review).

---

## §4 The dual-hat fallback row (§6.3) — per-pairing migration semantics

The §6.3 of the addendum encodes how A/R cells migrate when the Owner invokes a dual-hat pairing under ADR-0034b. The migration is **purely interpretive** — the §6.2 matrix cells are not rewritten; instead, the R-cells held by the dual-hat-slot in §6.2 are re-attributed to "Owner-in-role-X" for §43.1 sign-off block traceability.

### §4.1 Why this design (rather than a separate dual-hat matrix)

Three reasons:

1. **Decision-class invariance.** The R/C/I assignment for a given decision class (e.g., "BYOK addition → FW-H-3 R") is invariant under staffing changes. Only the **identity of the R-cell holder** changes. Maintaining a separate full-matrix for each pairing would triple the surface area and require parallel maintenance.
2. **Sign-off mechanics already disambiguate.** The addendum §1.3 sign-off mechanics require two distinct comments documents under dual-hat (one per slot), so the audit trail records "Owner acting as FW-H-3 on BYOK row" without needing a separate matrix.
3. **Cross-veto preserves the A-cell semantics.** Owner = A on every row regardless of pairing; this means dual-hat pairings cannot accidentally move A-status to a non-Final-Approver, which would violate `00_framework.md §43.1`.

### §4.2 Pairing-Alpha (Owner = FW-H-1 + FW-H-3) — affected rows

Rows where FW-H-1 or FW-H-3 hold R in §6.2:
- Row 1 (ADR creation, if architecture-touching)
- Row 3 (INV promotion — FW-H-1 R)
- Row 7 (TLA+ spec — FW-H-1 R)
- Row 8 (BYOK — FW-H-3 R)
- Row 10 (Schema migration — FW-H-1 R)
- Row 12 (Pentest triage — FW-H-3 R)
- Row 13 (GA cutover — FW-H-1 + FW-H-3 R, plus FW-H-2 + FW-H-4 R from externals)
- Row 14 (Q1 chair = FW-H-1 R; Q3 chair = FW-H-3 R; both Owner-held under Alpha)
- Row 15 (Annual deep review — FW-H-1 + FW-H-3 R from Owner; FW-H-2 + FW-H-4 R from externals)

The Owner authoring two distinct comments docs (per addendum §1.3) on each of these rows means the audit trail records 2 of the 4 R-cell signatures as "Owner under Pairing-Alpha" for §43.1 sign-off block.

### §4.3 Pairing-Beta (Owner = FW-H-2 + FW-H-4) — affected rows

Rows where FW-H-2 or FW-H-4 hold R in §6.2:
- Row 4 (Sprint impl sign-off — FW-H-4 R)
- Row 5 (DEBT register — FW-H-4 R)
- Row 6 (Runbook approval — FW-H-4 R)
- Row 9 (Region addition — FW-H-2 R)
- Row 11 (Customer breach response — FW-H-2 R)
- Row 13 (GA cutover — FW-H-2 + FW-H-4 R from Owner; FW-H-1 + FW-H-3 R from externals)
- Row 14 (Q2 chair = FW-H-2 R; Q4 chair = FW-H-4 R; both Owner-held under Beta)
- Row 15 (Annual deep review — FW-H-2 + FW-H-4 R from Owner; FW-H-1 + FW-H-3 R from externals)

### §4.4 Cross-veto preservation under dual-hat

Independent of which pairing is invoked, addendum §2 cross-veto rights are preserved row-by-row. The external advisors (FW-H-2 + FW-H-4 under Alpha; FW-H-1 + FW-H-3 under Beta) retain BLOCK authority on **any** row in §6.2, including rows where the R-cell is held by an Owner-dual-hat slot. This is critical for COI mitigation — the Owner cannot self-approve under dual-hat without external advisor concurrence (addendum §2.4 quorum 3-of-3-effective seats).

---

## §5 Single-A discipline verification

Reviewed every row of §6.2 and §6.3 for single-A:

| # | Row | A cell | Other cells with A? |
|---|---|---|---|
| 1 | ADR creation | Owner | No |
| 2 | ADR approval | Owner | No |
| 3 | INV registry promotion | Owner | No |
| 4 | Sprint impl sign-off | Owner | No |
| 5 | DEBT register entry | Owner | No |
| 6 | Runbook approval | Owner | No |
| 7 | TLA+ spec addition | Owner | No |
| 8 | BYOK provider addition | Owner | No |
| 9 | Region addition | Owner | No |
| 10 | Schema migration | Owner | No |
| 11 | Customer breach response | Owner | No |
| 12 | Pentest finding triage | Owner | No |
| 13 | GA cutover sign-off | Owner | No |
| 14 | Quarterly framework review | Owner | No |
| 15 | Annual deep review | Owner | No |

**Result.** Single-A discipline enforced across all 15 rows. Owner = A on every row by structural design (`00_framework.md §43.1` Final Approver authority); no FW-H-* slot ever holds A. This matches the original proposal §6 footer rule and is the desired invariant.

The dual-hat fallback row (§6.3) does NOT introduce new A-cells — it only re-attributes R-cell **authorship identity** for rows where the dual-hat slot already holds R in §6.2. Owner remains A on every row regardless of pairing.

---

## §6 Cross-reference graph

This wave-25 work updates the following documents:

| Document | Change | Version bump |
|---|---|---|
| `specs/_proposals/2026-05-16-framework-reviewer-roles-addendum.md` | Insert §6 RACI matrix detail (3 subsections: legend / 15-row matrix / dual-hat fallback row / disambiguating rules / cross-refs). Renumber §6→§7 (COI), §7→§8 (summary), §8→§9 (acceptance), §9→§10 (changelog). Update §8 summary to reflect 7 clauses. | 0.1.1 → 0.1.2 |
| `specs/03_architecture/adrs/ADR-0034b-framework-reviewer-dual-hat-fallback.md` | Migrate cross-refs from `addendum §6.2/§6.4` to `§7.2/§7.4` (COI subsections renumbered). Add context_links pointer to addendum §6. | 0.1.0 → 0.1.1 |
| `specs/_audits/2026-05-16-lote-7-raci-detail.md` (this file) | Initial creation — audit doc for the wave-25 RACI detail. | 1.0.0 |

### §6.1 Upstream documents (this audit references but does not modify)

- `specs/00_framework.md §43.1` — sign-off block where the R / A cell holders record commit SHAs. Already cross-references the addendum (line 3210). No changes needed.
- `specs/_proposals/2026-05-16-framework-reviewer-roles.md §6` — the 13-row summary that this wave-25 detail expands. Wave-25 leaves the summary intact (it remains the proposal's coarse-grain view; the detail lives in the addendum).
- `specs/_audits/2026-05-15-framework-v1-0-0-ga-audit.md §11` — the Lote 6 audit baseline. The audit's `unblock-path-C-progress` already records wave-22 addendum + wave-24 ADR-0034b absorption; wave-25 RACI detail closes the forward reference. No mutation required to the Lote 6 audit doc (it points at the addendum, which now contains the detailed matrix).
- `specs/_governance/reviewer_staffing_strategy.md` — multi-tier strategy SOT. The RACI detail does not redefine staffing; it operates downstream.

---

## §7 Quality gates

- `python3 scripts/validate_specs.py` — schema conformance for the addendum + ADR-0034b + this audit doc.
- `python3 scripts/validate_references.py` — cross-reference graph integrity (verifies the addendum §7.2 / §7.4 refs in ADR-0034b resolve to existing anchors post-renumber).

Both gates run as part of the wave-25 commit pre-merge check.

---

## §8 Open items (forward to wave-26+)

1. **Wire RACI per-row SLA into a CI gate?** The addendum §3.4 says SLA conformance is reported as a single line in the quarterly review delta-doc. Wave-26 may consider authoring a script that auto-extracts SLA hits/misses from change-log entries (out of scope for wave-25).
2. **Per-row INV binding?** Each row in §6.2 could be bound to a specific invariant in the registry (e.g., Row 3 INV promotion bound to a meta-invariant covering promotion criteria). Wave-25 leaves this as an open extension; doing so would tighten the RACI from "decision class" to "invariant-bound contract".
3. **Pairing-Alpha vs Pairing-Beta selection heuristics.** ADR-0034b §Permitted pairings already provides "when to choose" guidance. Wave-26 may consider a scored-decision artifact (e.g., a small TLA+ spec or a markdown table) that the Owner uses to pick the pairing at invocation time. Out of scope for wave-25.

---

## §9 Change log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-05-16 | Claude Opus 4.7 (wave-25 Lote 7 RACI detail authoring agent) | Initial audit doc documenting the wave-25 addendum v0.1.2 §6 RACI matrix detail authoring. §2 explains why a detailed matrix was needed beyond the original 13-row summary. §3 documents per-row rationale for all 15 decision rows. §4 explains the dual-hat fallback row design and per-pairing affected rows. §5 verifies single-A discipline across all rows. §6 enumerates the cross-reference graph changes (addendum 0.1.1→0.1.2; ADR-0034b 0.1.0→0.1.1; this audit doc 1.0.0). §7 names quality gates. §8 forward-references wave-26 open items (CI SLA wire-up, INV binding, pairing selection heuristics). |
