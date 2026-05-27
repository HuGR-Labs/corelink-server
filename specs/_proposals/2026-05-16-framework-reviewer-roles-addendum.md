---
id: "PROPOSAL-2026-05-16-FRAMEWORK-REVIEWER-ROLES-ADDENDUM"
type: "governance"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "0.1.3"
created: "2026-05-16"
updated: "2026-05-16"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["governance", "reviewers", "staffing", "proposal", "framework-freeze", "wave-22", "wave-24", "wave-25", "wave-26", "lote-7", "ga", "addendum", "dual-hat", "raci", "sla"]
references:
  - "specs/00_framework.md"
  - "specs/_proposals/2026-05-16-framework-reviewer-roles.md"
  - "specs/_audits/sealed/2026-05-15-framework-v1-0-0-ga-audit.md"
  - "specs/_governance/reviewer_staffing_strategy.md"
  - "specs/03_architecture/adrs/ADR-0034-prr-staffing-waiver-solo-tier.md"
---

# Framework Reviewer Roles — Addendum (Dual-Hat Fallback + Operating Policy)

> **Purpose.** Extends `specs/_proposals/2026-05-16-framework-reviewer-roles.md` v0.1.0 (the wave-20 Lote 7 proposal) with six operating-policy clauses needed to make the FW-H-1..4 role acceptance pattern executable in a small-org regime. The original proposal documents WHAT each role is; this addendum documents HOW the gate operates when staffing is constrained, when reviewers disagree, when sign-off stalls, when a reviewer onboards, when the framework drifts between freezes, and when reviewers have personal stake in the artifact under review.
>
> **Non-goal.** This addendum does NOT redefine any FW-H-* role scope (§2.1–§2.4 of the original proposal remain canonical). It does NOT change the §7 lifecycle thaw/freeze cascade. It does NOT supersede `_governance/reviewer_staffing_strategy.md` v0.1.0.
>
> **Context.** Wave-22 follow-on to wave-20 Lote 7. The original proposal's §8 Open Questions OQ-1 (org size / dual-hatting), implicitly OQ-3 (external vs internal staffing), and the §7 backup/bus-factor policy left operating mechanics under-specified for the realistic case where the CoreLink program operates as solo Owner + AI labor + 1–2 retained external advisors. This addendum closes that gap.

---

## §1 Dual-hat fallback policy

The original proposal §8.1 OQ-1 enumerated three options (A = 4 distinct people; B = 3 with one dual-hat; C = 2 with Owner dual-hatting). This section formalizes the **Option C** fallback as a named operating mode so the gate is executable WITHOUT waiting for the goal-state hiring outcome.

> **Wave-24 cross-ref.** The §1.1 item-4-required ADR was authored as **`specs/03_architecture/adrs/ADR-0034b-framework-reviewer-dual-hat-fallback.md`** (wave-24, status PROPOSED → ACCEPTED on Owner invocation). ADR-0034b is the canonical authorization artifact for invoking the fallback; this §1 is the operating-policy specification it references.

### §1.1 When dual-hat fallback applies

The Owner MAY invoke the dual-hat fallback when ALL of the following hold:

1. The org headcount is < 10 engineers AND the SOC2 / ISO27001 separation-of-duty controls are NOT yet contractually required by a customer.
2. At least 2 external advisors are retained (per §8.3 OQ-3 Hybrid recommendation) and willing to fill the externally-staffable slots.
3. The Owner personally has working competence in the two slots they intend to dual-hat (Owner self-attests in the §42 change-log entry).
4. An ADR analogous to `ADR-0034-prr-staffing-waiver-solo-tier.md` is authored authorizing dual-hatting at the framework-freeze tier, including expiration trigger (e.g., "auto-expires when headcount ≥ 10 OR first SOC2 audit kickoff, whichever first").

### §1.2 Permitted dual-hat pairings

Only the following two pairings are permitted under this addendum. Both preserve cross-domain coverage (one technical/structural slot + one compliance/operational slot per person), avoiding the failure mode where one individual signs off on a domain they have no independent perspective on.

| Pairing | Owner takes | External advisors fill | Rationale |
|---|---|---|---|
| **Pairing-Alpha** (recommended) | FW-H-1 (Arch) + FW-H-3 (Security) | FW-H-2 (Compliance/Privacy) + FW-H-4 (Production Ops) | Founder typically authored architecture + threat model; externals attest compliance landscape (a moving regulatory target) + production reality (where founder bias is strongest). |
| **Pairing-Beta** (alternative) | FW-H-2 (Compliance/Privacy) + FW-H-4 (Production Ops) | FW-H-1 (Arch) + FW-H-3 (Security) | Aligns with `reviewer_staffing_strategy.md §7.2` Founder-only-sign-off fallback prior art; externals provide architectural + security independence. Use when Owner has stronger compliance/ops background than architecture/security. |

**Forbidden dual-hat pairings:**

- FW-H-1 (Arch) + FW-H-2 (Compliance/Privacy) — combines two consultative slots without an independent integrity check.
- FW-H-3 (Security) + FW-H-4 (Production Ops) — combines the two slots whose RACI in proposal §6 most frequently shows joint R-status; collapsing them loses adjudication signal on incident-driven re-attestation rows.
- FW-H-1 (Arch) + FW-H-3 (Security) under **non-Owner** dual-hatting — only the Owner may take Pairing-Alpha because the §42 change-log self-attestation requirement implies Final-Approver-level accountability for both seats; an external advisor cannot bind both.

### §1.3 Dual-hat sign-off mechanics

When the Owner dual-hats, the §43.1 sign-off block records the Owner's name + commit SHA twice (once per slot), with role IDs labeled (e.g., `FW-H-1 / FW-H-3 — Gustavo Schneiter — 2026-MM-DD — sha:<7-char>`). The original proposal's §3 acceptance deliverables (separate comments documents per role) are still required — two distinct comments docs even though one person authors both. This preserves the audit trail required by PRINC-005 (rastreabilidade) and avoids "single-doc rubber stamp" collapse.

### §1.4 Auto-expiration of dual-hat mode

Dual-hat fallback automatically expires (i.e., the framework auto-demotes from FROZEN to THAWED per the §7 lifecycle, retriggering the freeze gate with 4-distinct-staffing required) when ANY of:

- Org headcount reaches 10 engineers.
- First contractual SOC2 / ISO27001 audit kickoff (gates separation-of-duty).
- 18 months elapsed since the dual-hat ADR was authored (hard ceiling regardless of conditions).
- The Owner steps down from Final Approver role.

The auto-expiration trigger is encoded into the dual-hat ADR's `Status` field and is monitored by the trimestral framework review (§5.2 of the original proposal).

---

## §2 Cross-veto rule

The original proposal §6 RACI defines Responsible / Accountable / Consulted / Informed per decision but does not enumerate **blocking** rights. This section formalizes them.

### §2.1 Veto authority

Any FW-H-* reviewer MAY block the v1.0.0 FROZEN cut by recording a CRITICAL or HIGH severity finding in their §3 comments document that the reviewer explicitly marks `vote: BLOCK`. The block remains effective until either:

1. The finding is remediated by changes to `00_framework.md` AND the blocking reviewer issues a follow-up `vote: WITHDRAW` line in the same comments doc.
2. The Owner (as Final Approver) overrides via the §2.3 tie-break rule below, with the override recorded in the §42 change-log entry.

### §2.2 Cross-domain veto

A reviewer MAY raise a `vote: BLOCK` finding on §-sections OUTSIDE their primary coverage IF the finding identifies a cross-cutting concern (e.g., FW-H-4 Production Ops blocking on §17 Security architecture because an SLO instrumentation gap masks a security signal). Cross-domain blocks have the same standing as in-domain blocks but require the blocking reviewer to articulate the cross-cut explicitly (one paragraph in the comments doc).

### §2.3 Tie-break / override

If a `vote: BLOCK` finding cannot be remediated AND the blocking reviewer refuses to withdraw, escalation to the Owner (Final Approver, always **A** per the proposal §6 RACI) is required. The Owner may:

- **Sustain the block**: framework remains v1.0.0-rc1; remediation work continues.
- **Override the block**: Owner records the override in the §42 change-log entry naming the blocking reviewer, the finding, and the override rationale. The blocking reviewer's `vote: BLOCK` stays in the comments-doc audit trail (NOT erased — PRINC-005). The override is recorded as a `dissent_recorded` entry under the v1.0.0 metadata.
- **Re-staff the slot**: in extreme cases (e.g., the blocker is a bad-faith actor), the Owner may invoke the §7 backup policy of the original proposal to swap the blocking reviewer with their documented `+1 backup`. This is a hard action and SHOULD be paired with a private post-mortem between Owner and the displaced reviewer.

### §2.4 Quorum

A v1.0.0 FROZEN cut requires affirmative `vote: APPROVE` from at least 3 of 4 FW-H-* slots (or 3 of 3 effective seats under dual-hat mode where one person holds 2 slots — i.e., that person APPROVEs both, plus 2 externals APPROVE). The 4th slot may be `vote: ABSTAIN` (e.g., scope outside the §-coverage for which they have meaningful judgment) but NOT silent — abstention is recorded explicitly.

The 3-of-4 minimum is the tightest quorum consistent with `00_framework.md §7` `Aprovador Final + ≥ N revisores requeridos` and provides bus-factor resilience (one reviewer can be unreachable without stalling the cut).

---

## §3 Sign-off SLA

The original proposal §5 sign-off cadence defines events but not response-time commitments. This section adds per-lane SLAs aligned with `reviewer_staffing_strategy.md` tiering conventions.

### §3.1 Lanes

Framework decisions are routed to one of two lanes based on §41.2 semver impact and §-coverage scope:

| Lane | Trigger | Examples |
|---|---|---|
| **STANDARD** | Patch / minor framework changes; trimestral review delta-doc; new EVT-type taxonomy addition; runbook discipline clarifications | Typo fix in §22; new SLI added to §35.5 inheritance pattern; clarifying note on §25 dry-run cadence |
| **HIGH_RISK** | Major (X+1.0.0) bumps; new PRINC; BYOK/key-custody model change; waiver semantics change (§35.6); regulatory regime addition; incident-driven re-attestation | Adding PRINC-039; lifting FIPS 140-3 framing; adding LFPDPPP to §21; post-SEV-1-incident framework drift |

### §3.2 SLA targets

| Lane | Initial-response SLA | Sign-off SLA | Escalation if missed |
|---|---|---|---|
| STANDARD | 1 business day (acknowledge receipt; flag if conflict) | **3 business days** from comments-doc cut | Owner escalates via §7 backup; if backup also unresponsive, slot enters "stalled" state and 14-day grace clock begins |
| HIGH_RISK | 2 business days (acknowledge; reserve calendar block) | **7 business days** from comments-doc cut | Same as STANDARD but with 21-day grace; HIGH_RISK with sustained stall after 21 days auto-triggers the §7 vacant-slot 60-day clock |

Business day = Monday–Friday excluding the reviewer's declared local holidays (declared at onboarding per §4 of the addendum).

### §3.3 SLA exceptions

- **Reviewer-declared OOO**: A reviewer may declare planned out-of-office in advance (≥ 5 business days notice); the SLA clock pauses for declared OOO windows.
- **Cross-reviewer dependency**: If FW-H-3 (Security) sign-off depends on FW-H-2 (Compliance) finishing first (compliance regime determines threat-model scope), the dependent reviewer's clock starts when the prerequisite reviewer signs.
- **Incident OOO**: A reviewer actively on-call during a SEV-1/SEV-2 incident may pause their SLA for the duration of the incident response.

### §3.4 SLA tracking

Sign-off SLA conformance is reported in each quarterly framework review (§5.2 of the original proposal) as a single line in the delta-doc: `SLA conformance Q<N>: STANDARD <X>/<Y> (<%>), HIGH_RISK <X>/<Y> (<%>)`. Persistent sub-80% conformance on either lane is a discussion item for the annual deep review (§5.3 of the original proposal).

---

## §4 Reviewer training pack

The original proposal §4 (Onboarding checklist) enumerates the read order for a nominated reviewer. This section names that read order as the **Reviewer Training Pack** and pins a budget number so the time commitment is concrete during candidate negotiation.

### §4.1 Training Pack contents

The Reviewer Training Pack is the existing §4 onboarding read order (no new documents authored — it points at canonical sources already in the corpus). For convenience, the items are:

1. `specs/00_framework.md` (framework canonical, 3217 lines, ~4–6h)
2. ADR index `specs/03_architecture/adrs/` (27 ADRs, ~2–4h)
3. Invariant registry `specs/03_architecture/invariant_registry.md` (192 INVs, ~1–2h)
4. Section-specific canonical sources per `reviewer_staffing_strategy.md §2` mapping
5. Audit baseline `specs/_audits/sealed/2026-05-15-framework-v1-0-0-ga-audit.md` + `specs/_audits/sealed/2026-05-15-ga-readiness-consolidation-wave-13-17.md`
6. Runbook index `specs/_runbooks/` (124 runbooks, ~1–2h skim)
7. (Optional) one sprint contract + one PRR

### §4.2 Budget commitment

**Total budget: 10 reading hours minimum**, distributed across the items above. The original proposal §4 cited "~12–20 working hours over 2–3 weeks" as a thorough envelope; this addendum sets **10 hours as the contractual floor** so that:

- A retained external advisor on a small monthly retainer (e.g., 4–10h/month per the original proposal §8.3 Hybrid recommendation) can complete onboarding within the first 1–2 months without burning their whole monthly allocation on reading.
- The candidate-conversation collapse to "yes / no / negotiate scope" (original proposal §1 goal) has a concrete number to negotiate against.

The 10-hour floor is a MINIMUM; reviewers are encouraged to invest more time on items overlapping their §-coverage (FW-H-3 Security typically needs deeper invariant-registry time than FW-H-2 Compliance, for example).

### §4.3 Pre-existing artifacts

All seven items in the Training Pack pre-exist in the spec corpus as of wave-20. No new documentation is required to enable the Training Pack — it is purely a curated read order over existing canonical sources. This is intentional: the Training Pack is a **contract** over corpus state, not a new deliverable.

### §4.4 Training Pack maintenance

The trimestral framework review (§5.2 of the original proposal) refreshes the Training Pack pointers if any of the canonical sources move or are renamed. New ADRs landing between reviews are auto-included (the read item points at the ADR index directory, not a fixed file list).

---

## §5 Quarterly framework review cadence

The original proposal §5.2 specified a quarterly cadence aligned to calendar Q1/Q2/Q3/Q4. This section concretizes the cadence to a 90-day rolling clock anchored on the v1.0.0 FROZEN cut date, which is more robust to off-quarter freeze dates.

### §5.1 Cadence anchor

The quarterly review clock starts on the v1.0.0 FROZEN cut commit date. Subsequent reviews are due every **90 calendar days** (not "next calendar quarter") from that anchor. This avoids the failure mode where a v1.0.0 cut on (e.g.) 2026-06-15 would otherwise demand a Q3 review only 15 days later.

### §5.2 First review scope

The **first** quarterly review (T+90 days from FROZEN cut) has an explicit scope:

- Framework v1.0.1 candidate delta (any typo / link-fix / clarifying-note backlog accumulated since freeze).
- Inventory of new ADRs landed in the 90-day window (each tagged by which FW-H-* §-coverage it intersects).
- Inventory of new INV additions / promotions to `tla_verified` status.
- Inventory of incident-driven re-attestation triggers (SEV-1/SEV-2 in window).
- SLA conformance summary per §3.4 of this addendum.

Output artifact: `specs/_audits/YYYY-MM-DD-framework-quarterly-review-Q1.md` (delta-only audit doc per the original proposal §5.2 output spec).

### §5.3 Subsequent quarterly reviews

Each subsequent 90-day review follows the same template as §5.2 above, with running counters (cumulative since v1.0.0 cut) and trend signals (e.g., "ADR landing rate Q1: 5 → Q2: 3 → Q3: 4; stable").

### §5.4 Quarterly chair rotation

The chair role rotates: Q1 = FW-H-1, Q2 = FW-H-2, Q3 = FW-H-3, Q4 = FW-H-4, then cycle repeats. The chair is responsible for authoring the delta-doc and circulating it for the async/synchronous round.

### §5.5 Annual deep review interleaving

The 4th quarterly review (T+360 days) is the **anchor for the annual deep review** (§5.3 of the original proposal) — the two events MAY be combined into a single audit doc if the framework state is stable, OR run as separate documents if substantial drift accumulated. The chair of the 4th quarterly review is the chair of the annual deep review.

---

## §6 RACI matrix detail (per major framework decision)

The original proposal §6 contains a 13-row RACI summary scoped to framework-level decisions (PRINC additions, new invariant classes, regulatory regime additions, BYOK changes, SLO methodology changes, etc.). The wave-22 addendum §1.2 (`FW-H-3+FW-H-4` forbidden dual-hat pairing) and wave-24 `ADR-0034b` (§Forbidden pairings + §Cross-veto rights) both reference "the addendum proposal §6 RACI" as the canonical source of joint-R-status reasoning. This §6 of the addendum **expands** the proposal's coarse-grained 13-row table to a per-decision-class detailed matrix covering 15 framework decision rows plus an explicit **dual-hat fallback row** showing how RACI cells migrate when the Owner takes Pairing-Alpha (FW-H-1 + FW-H-3) or Pairing-Beta (FW-H-2 + FW-H-4) per ADR-0034b §Permitted pairings.

> **Relationship to original proposal §6.** The original proposal §6 RACI is the **summary view**; this §6 of the addendum is the **detailed view**. They are intentionally redundant: the summary lives in the proposal so a reviewer reading only the proposal sees the core staffing intent; the detail lives here so the operating-policy machinery (veto, SLA, quorum, COI, dual-hat) has a concrete per-row staffing target to bind against.
>
> **Single-A discipline.** Every row has **exactly one A (Accountable)** cell. Gustavo (Owner / Final Approver) is **A** on every row by `00_framework.md §43.1` (Final Approver authority) — the FW-H-* slots are **R / C / I** under the Owner. This mirrors the original proposal §6 footer rule ("Gustavo = Final Approver, always **A**") and avoids the multi-A failure mode that RACI literature flags as the most common matrix bug.

### §6.1 Legend

- **R** — *Responsible*: does the work of producing the artifact / drafting the change / authoring the finding.
- **A** — *Accountable*: single sign-off authority; one A per row (Owner-as-Final-Approver is **A** on all framework-tier rows).
- **C** — *Consulted*: input solicited **before** the decision is finalized; required to be reachable in the SLA window (addendum §3.2).
- **I** — *Informed*: notified **after** the decision is finalized; no input expected but transcript-of-record receives the notification (addendum §4.4 + §5.2 quarterly review).

### §6.2 Detailed RACI matrix — 15 framework decision rows

> **Wave-26 update.** The trailing `INV binding` column was added per `specs/_audits/sealed/2026-05-16-lote-7-followons-closure.md §3` to bind rows to the invariant-registry entries they functionally enforce. Rows whose decision class is not directly tied to a single invariant carry `—` (the binding is implicit via the broader §-coverage). The binding is **non-exclusive**: a row may touch multiple INVs at runtime; the column names the *closest* invariant whose violation would block the decision. Bindings reference `specs/03_architecture/invariant_registry.md`.

| # | Decision | Owner (Gustavo) | FW-H-1 Architecture | FW-H-2 Compliance | FW-H-3 Security | FW-H-4 Production Ops | INV binding |
|---|---|---|---|---|---|---|---|
| 1 | **ADR creation** (new ADR authored against framework §-content) | A | R if architecture; C otherwise | C if compliance-touching | C if security-touching | C if ops-touching | — |
| 2 | **ADR approval** (status DRAFT → ACCEPTED transition) | A | C | C | C | C | — |
| 3 | **INV registry promotion** (DRAFT → PROVEN / tla_verified status flip) | A | R (architecture-of-record co-signs) | I | C if security invariant; else I | C if SLI / SLO invariant; else I | INV-OBS-AUDIT-CHAIN-INTEGRITY (promotion provenance lands in audit chain) |
| 4 | **Sprint impl sign-off** (S-NN PRR gate close per `_governance/reviewer_staffing_strategy.md §2.5`) | A | C | C | C | R | — |
| 5 | **DEBT register entry** (new DEBT-NNN authored or waived per §35.6) | A | C | C if compliance-DEBT | C if security-DEBT | R (operational reality check) | — |
| 6 | **Runbook approval** (new runbook authored / existing materially edited per §25) | A | I | I | C if security-runbook | R | — |
| 7 | **TLA+ spec addition** (new formal spec landed in `specs/_proofs/`) | A | R | I | C if security-property | I | — |
| 8 | **BYOK provider addition** (§28 — new KMS / HSM provider integrated) | A | C | C (DPA review) | R (key-custody architecture co-signs) | C (operational SLO impact) | INV-BYOK-CRYPTO-SOVEREIGNTY |
| 9 | **Region addition** (new deployment region / data-residency expansion) | A | C | R (regulatory mapping per §21) | C (tenant-isolation INVs cross-region) | C (latency SLO + on-call coverage) | INV-REGION-NO-CROSS-LEAK |
| 10 | **Schema migration** (canonical-source schema or DB migration touching framework §-binding fields) | A | R (architecture-of-record) | C if PII-schema | C if security-schema | C (rollout posture) | INV-AUTH-MIGRATION-ADDITIVE |
| 11 | **Customer breach response** (SEV-1 breach, regulatory notification per §27 + §30) | A | I | R (notification timeline + DPA invocation) | C (forensics + IR coordination) | C (status-page + comms cadence) | INV-AUDIT-APPEND-ONLY (breach-timeline evidence preserved) |
| 12 | **Pentest finding triage** (external pentest report intake per `_audits/2026-05-16-pre-ga-pentest-scope.md`) | A | C (architectural remediation) | I | R (severity classification + remediation owner assignment) | C (operational mitigations) | — |
| 13 | **GA cutover sign-off** (v1.0.0 FROZEN cut + product GA gate close) | A | R | R | R | R | INV-ROLLOUT-COSIGN-GATE (cutover artifact provenance) |
| 14 | **Quarterly framework review** (addendum §5 delta-doc cadence) | A | R if Q1; C otherwise | R if Q2; C otherwise | R if Q3; C otherwise | R if Q4; C otherwise | — |
| 15 | **Annual deep review** (addendum §5.5 — T+360 days from v1.0.0 cut) | A | R | R | R | R | — |

### §6.3 Dual-hat fallback row (per ADR-0034b §Permitted pairings)

When the Owner invokes the dual-hat fallback per ADR-0034b, the **A cells migrate accordingly** because the Owner-as-dual-hatter is simultaneously Final Approver (always A) AND occupies two FW-H-* seats whose addendum §1.3 sign-off mechanics require **per-slot comments docs** (two distinct authorship trails even though one person authors both). The R/C/I cells in the §6.2 matrix do NOT change — only the **interpretation** of the dual-hat slots' R/C status shifts to "Owner acting in capacity X" for traceability.

The table below shows the **net effect on the §6.2 R column** for each pairing. Rows where the Owner already held A are unchanged; the dual-hat columns absorb R-status as "Owner-in-role-X" for the §43.1 sign-off block.

| Pairing | Owner takes (dual-hat) | External advisors fill | Effect on §6.2 R-cells under Owner-as-dual-hat |
|---|---|---|---|
| **None** (4-distinct staffing) | — | All 4 FW-H-* externally / independently staffed | §6.2 matrix applies as written; Owner is A on all rows; FW-H-* slots are R/C/I per their primary coverage. |
| **Pairing-Alpha** (ADR-0034b recommended) | FW-H-1 (Architecture) + FW-H-3 (Security) | FW-H-2 (Compliance/Privacy) + FW-H-4 (Production Ops) | Row 1 (ADR creation): Owner-as-FW-H-1 holds R if architecture-touching. Row 3 (INV promotion): Owner-as-FW-H-1 holds R; Owner-as-FW-H-3 holds C on security-INVs. Row 7 (TLA+ spec): Owner-as-FW-H-1 holds R. Row 8 (BYOK): Owner-as-FW-H-3 holds R; Owner-as-FW-H-1 in C. Row 10 (Schema migration): Owner-as-FW-H-1 holds R. Row 12 (Pentest triage): Owner-as-FW-H-3 holds R. Row 13 + Row 15 (GA cutover + annual deep review): both Owner-held slots are R; the two external advisors (FW-H-2 + FW-H-4) hold the remaining R cells. Sign-off block per addendum §1.3 records Owner's commit SHA twice with role IDs labeled. |
| **Pairing-Beta** (ADR-0034b alternative) | FW-H-2 (Compliance/Privacy) + FW-H-4 (Production Ops) | FW-H-1 (Architecture) + FW-H-3 (Security) | Row 4 (Sprint impl sign-off): Owner-as-FW-H-4 holds R. Row 5 (DEBT register): Owner-as-FW-H-4 holds R. Row 6 (Runbook approval): Owner-as-FW-H-4 holds R. Row 9 (Region addition): Owner-as-FW-H-2 holds R. Row 11 (Customer breach response): Owner-as-FW-H-2 holds R. Row 13 + Row 15: both Owner-held slots are R; the two external advisors (FW-H-1 + FW-H-3) hold the remaining R cells. Same §1.3 sign-off mechanics apply. |
| **Forbidden pairings** | — | — | Per ADR-0034b §Forbidden pairings: FW-H-1+FW-H-2 (collapses Row 9/11 R-cells onto a COI-conflicted single person); FW-H-3+FW-H-4 (collapses Row 4/6/12 joint R-status with no adjudication signal); FW-H-1+FW-H-3 under non-Owner dual-hatting (only Owner may take Pairing-Alpha per Final-Approver-accountability rule). |

**Cross-veto preservation under dual-hat.** Addendum §2 cross-veto rights are preserved row-by-row. Specifically, under Pairing-Alpha, the external advisors (FW-H-2 + FW-H-4) retain `vote: BLOCK` authority on **any** row in §6.2 — including rows where the R-cell is held by an Owner-dual-hat slot. This is the addendum §2.2 cross-domain veto operating at the per-decision granularity.

### §6.4 Pairing-Alpha vs Pairing-Beta selection heuristics (wave-26)

ADR-0034b §Permitted pairings names the two permitted dual-hat pairings but does not script the **selection decision** the Owner makes at invocation time. This section provides that decision tree so the choice is reproducible / auditable rather than a one-off judgment call.

> **Source-of-truth.** This subsection complements ADR-0034b §Permitted pairings (which is normative). The heuristics below are advisory inputs to the §43.1 sign-off block authoring; the Owner retains final discretion per `00_framework.md §43.1` Final-Approver authority. The §10 change-log records which heuristic dimension dominated the decision so the rationale is preserved.

#### §6.4.1 Decision tree

```
1. Look at the trailing 90-day work mix (use the §5 quarterly delta-doc inventories):
   - Count of ADRs landed × architectural weight  → Architecture-pressure score
   - Count of canonical-source / spec-corpus edits → Architecture-pressure score
   - Count of TLA+ specs landed                   → Architecture-pressure score
   - Count of customer-breach / SEV-1 incidents   → Compliance-pressure score
   - Count of runbook edits / new RBs             → ProdOps-pressure score
   - Count of GA-cutover / region-rollout events  → Compliance-pressure score
   - Count of pentest / vulnerability triage events → Security-pressure score

2. If Architecture-pressure + Security-pressure ≥ Compliance-pressure + ProdOps-pressure
   → choose **Pairing-Alpha** (Owner = FW-H-1 + FW-H-3).
   Externals fill FW-H-2 (Compliance) + FW-H-4 (Production Ops).

3. Else if Compliance-pressure + ProdOps-pressure > Architecture-pressure + Security-pressure
   → choose **Pairing-Beta** (Owner = FW-H-2 + FW-H-4).
   Externals fill FW-H-1 (Architecture) + FW-H-3 (Security).

4. Tie-breaker (pressures within ±20% of each other):
   - If pre-GA (no v1.0.0 FROZEN cut yet OR within first 90 days post-cut)
     → **Pairing-Beta** (compliance-heavy phase: cutover, region rollouts, runbook discipline).
   - If post-GA + steady-state (≥ 90 days post-cut, no SEV-1 in window)
     → **Pairing-Alpha** (architecture-heavy phase: ADR landing, INV promotion, TLA+ deepening).
```

#### §6.4.2 Default for SaaS pre-GA (CoreLink current state)

For the CoreLink program as of wave-26 (pre-GA, S-21 sprints in flight, GA cutover decision row [§6.2 row 13] is the next major framework event), the default is **Pairing-Beta**:

- The next 90 days are dominated by §6.2 row 13 (GA cutover) and row 9 (Region addition), both of which are HIGH_RISK lane with FW-H-2 (Compliance) R-status.
- Runbook approval (row 6) and DEBT register entry (row 5) cadence is at trailing-90-day high-water-mark per `_audits/2026-05-15-debt-register.md` — both FW-H-4 R.
- Sprint impl sign-off (row 4) is FW-H-4 R; sprint cadence remains weekly until S-21 close.
- Architecture-pressure has plateaued post-S-14 (TLA+ landing rate slowing per `tla_check.yml` weekly volume); Pairing-Alpha's preferred row pattern (rows 3 / 7 / 8 / 10) is less frequently hit.

Pairing-Beta keeps the Owner on the rows that fire weekly (4 / 5 / 6 / 9 / 11) and routes the lower-cadence rows (3 / 7 / 8 / 10 / 12) to external advisors who can take a deeper-but-rarer engagement model.

#### §6.4.3 Re-pairing criteria post-GA

After the v1.0.0 FROZEN cut + product GA, the Owner SHOULD re-evaluate the pairing at the first quarterly review (T+90 days post-cut). Re-pair to **Pairing-Alpha** when ALL of the following hold:

1. ≥ 6 ADRs landed in the trailing 90 days (high architectural-work cadence).
2. ≥ 1 TLA+ spec added or promoted to GREEN in the window.
3. No SEV-1 breach response (row 11) fired in the window.
4. No new region / data-residency expansion (row 9) is scheduled in the next 90 days.
5. The DEBT register entry rate has dropped to ≤ 3 entries per 90 days (operational steady-state).

Re-pairing requires authoring a delta entry in the §1.1-mandated dual-hat ADR (ADR-0034b) recording the heuristic-dimension scores and the chosen pairing. The §43.1 sign-off block for the next quarterly review records the new pairing; mid-quarter re-pairings are discouraged because they invalidate the per-row R-cell attribution for the partial-quarter audit trail.

#### §6.4.4 Anti-patterns to avoid

- **Choosing Pairing-Alpha pre-GA to avoid compliance reading.** This inverts the workload — Compliance pressure is highest pre-GA and routing it to a thinly-engaged external advisor risks DPA / regulatory misses. The §1.2 OQ-1 rationale specifically calls out "Owner takes the slots they personally have working competence in"; an Owner without strong regulatory background should NOT take FW-H-2.
- **Choosing Pairing-Beta post-GA in steady-state.** This routes Architecture (FW-H-1) R-status to an external advisor who may have shallower familiarity with the framework's INV / TLA+ corpus than the Owner who authored most of it. The INV registry promotion row (3) and TLA+ row (7) cadence post-GA benefit from Owner-as-FW-H-1 continuity.
- **Mid-quarter re-pairing.** See §6.4.3 — invalidates the audit trail. Recover by waiting to the next quarter boundary.

#### §6.4.5 Audit trail

Each pairing decision is recorded in the §10 change-log entry that authors / re-affirms the dual-hat ADR with the format:

```
pairing_chosen: Alpha | Beta
heuristic_inputs:
  architecture_pressure: <int>  # 0-N ADRs / TLA+ specs / spec edits in trailing 90d
  security_pressure: <int>      # 0-N pentest / vuln events in trailing 90d
  compliance_pressure: <int>    # 0-N breach / region / regulatory events
  prodops_pressure: <int>       # 0-N runbook / DEBT / sprint-impl events
  tie_breaker_invoked: yes | no
  pre_or_post_GA: pre-GA | post-GA-<days>d
decision_rationale: <one-paragraph>
```

This is the §3.4 SLA-conformance line's structural sibling: a per-90-day record that the quarterly review chair can inspect to detect drift (e.g., "Pairing-Beta was chosen Q1 and Q2 but heuristic scores favored Pairing-Alpha both quarters — investigate why").

---

### §6.5 Disambiguating rules (matrix-edge cases)

1. **Multi-domain decisions.** If a decision row crosses two or more FW-H-* slots' primary §-coverage (e.g., a BYOK change that also touches a regulatory regime), the slot whose primary coverage contains the **canonical §-section authoring the rule under change** is R; the others are C. This mirrors the original proposal §6 "Disambiguating rule".
2. **C-cell SLA.** A reviewer marked C on a decision row inherits the addendum §3.2 SLA: STANDARD lane = 1 BD initial response, 3 BD sign-off; HIGH_RISK lane = 2 BD initial response, 7 BD sign-off. Silent C is a procedural defect logged in the quarterly review (addendum §3.4 SLA conformance line).
3. **I-cell receipt confirmation.** I-cells require no input but the chair of the trimestral review confirms receipt-of-notification was logged (e.g., a comment in the change-log entry, a Slack thread message ID, or equivalent). Missing receipt confirmation is a low-severity audit finding (does not block the cut, but accumulates in the §5.2 quarterly delta-doc).
4. **Tie-break.** Conflicts between R / C reviewers on a row escalate to Owner (A) per addendum §2.3. Under dual-hat, conflicts between Owner-held-slot and external advisor on the same row follow the addendum §2.3 external-advisor-as-tie-breaker rule (ADR-0034b §Cross-veto rights under dual-hat last bullet).
5. **Recusal interaction.** A reviewer who declared a §7 conflict-of-interest (this addendum, post-renumber) on a given row recuses by shifting their R/C cell to ABSTAIN; their backup (per original proposal §7) absorbs the R if they hold one, else the quorum drops by one effective seat (addendum §2.4 + this addendum §7.3 recusal mechanics).
6. **Decision-row provenance.** Each row in §6.2 maps to a `00_framework.md` section. Future framework versions may add rows; new rows MUST cite the framework §-section they bind to and MUST declare the R/A/C/I cells explicitly (no "TBD" cells permitted — single-A discipline includes "single-A-or-explicit-deferral").

### §6.6 Cross-references

- `specs/_proposals/2026-05-16-framework-reviewer-roles.md §6` — the summary view (13 rows; same staffing intent, coarser grain).
- `specs/03_architecture/adrs/ADR-0034b-framework-reviewer-dual-hat-fallback.md §Permitted pairings + §Forbidden pairings + §Cross-veto rights under dual-hat` — authorization for the §6.3 dual-hat row of this matrix.
- `specs/00_framework.md §43.1` — sign-off block where R / A cell holders record commit SHAs.
- `specs/_audits/sealed/2026-05-16-lote-7-raci-detail.md` — audit doc explaining the per-row rationale (e.g., why Row 4 sprint impl sign-off is FW-H-4 R rather than FW-H-1 R; why Row 8 BYOK is FW-H-3 R rather than FW-H-1 R).
- This addendum §1 (dual-hat policy) + §2 (cross-veto) + §3 (SLA) + §7 (COI / recusal — post-renumber) — the operating-policy clauses that bind against the §6.2 matrix.

---

## §7 Conflict-of-interest declaration

The original proposal does not address the case where a FW-H-* reviewer is also the author of a framework section, ADR, or canonical source under review. This section closes that gap.

### §7.1 Declaration requirement

At onboarding, each nominated FW-H-* reviewer declares (in the §3 acceptance deliverable comments doc, frontmatter section labeled `conflicts_declared:`) the framework §-sections, ADRs, or canonical sources they personally authored or substantively co-authored. Examples:

- "I co-authored ADR-0017 in 2026-Q2; I will RECUSE on any decision row in §6 RACI that touches ADR-0017's subject matter."
- "I authored §22 SLO methodology in framework v0.7.0; I do NOT recuse on §22 because authorship is > 12 months old and ≥ 2 subsequent maintenance edits by others have superseded my contribution."

### §7.2 Recusal scope

A reviewer MUST recuse from approving (i.e., shift their RACI status from R/A to ABSTAIN with `recusal_reason: <text>`) on any framework decision that:

1. Promotes their own un-reviewed work to FROZEN status (authorship < 6 months ago and < 2 maintenance edits since).
2. Adjudicates a dispute where they are also a stakeholder (e.g., they authored an ADR that another reviewer is BLOCKing).
3. Modifies a section they wrote IF the modification is non-trivial (more than typo/link-fix) and authored within the last 6 months.

### §7.3 Recusal mechanics

A recused reviewer:

- Records the recusal in the §43.1 sign-off line as `RECUSED — <reason> — <date>`.
- May still participate as **C** (Consulted) in the original proposal §6 RACI rows, contributing factual input but NOT signing.
- Their slot's quorum requirement (§2.4 of this addendum) is satisfied by their backup (§7 of the original proposal) if the backup is available; otherwise the quorum drops to "2 of 3 effective seats" for that decision only.

### §7.4 Owner conflict of interest

Because the Owner is always Final Approver (always **A**), Owner recusal is structurally impossible at the framework freeze tier (there is no escalation above Final Approver). The dual-hat fallback (§1 of this addendum) intensifies this — when Owner holds 2 slots, conflicts are correspondingly more likely.

Mitigation: when the Owner has a personal conflict on a decision (e.g., the Owner authored the section under review AND is one of the dual-hat slots signing off), the §42 change-log entry MUST disclose the conflict explicitly with `owner_conflict_disclosed: <reason>`. This is an audit-trail requirement, not a recusal — the Owner does NOT recuse, but the conflict is on-record for future external auditors (e.g., during the SOC2 readiness exercise referenced in `specs/_audits/sealed/2026-05-14-soc2-readiness-score.md`).

### §7.5 Auditor review on conflicts

The quarterly framework review (§5 of this addendum) inspects the list of `conflicts_declared:` and `owner_conflict_disclosed:` entries from the prior 90 days and flags any patterns (e.g., a single reviewer consistently recusing on a high-traffic §-section, suggesting a different reviewer should take the slot). Persistent patterns are an agenda item for the annual deep review.

---

## §8 Operating-policy summary table

For quick reference, the seven clauses of this addendum collapse to:

| § | Clause | What it adds | Pre-existing in original proposal? |
|---|---|---|---|
| §1 | Dual-hat fallback | Names Pairing-Alpha / Pairing-Beta; auto-expiration triggers; mechanics for §43.1 sign-off block | Hinted at in OQ-1 Option C; this addendum formalizes |
| §2 | Cross-veto rule | `vote: BLOCK / APPROVE / ABSTAIN / WITHDRAW` semantics; 3-of-4 quorum; Owner tie-break | Not in original |
| §3 | Sign-off SLA | STANDARD = 3 BD / HIGH_RISK = 7 BD; lane routing; exception types; tracking | Not in original |
| §4 | Reviewer training pack | 10-hour floor on existing §4 onboarding read order | Read order pre-existed; budget number is new |
| §5 | Quarterly cadence (90-day rolling) | Anchor on FROZEN cut date; first-review scope; chair rotation | Original §5.2 said "Q1/Q2/Q3/Q4"; this anchors to commit date |
| §6 | RACI matrix detail | 15 framework decision rows × 5 cells; single-A discipline; dual-hat fallback row migrating A/R cells per ADR-0034b | Original §6 has 13-row summary; this expands to 15-row detail bound to ADR-0034b pairings |
| §7 | Conflict-of-interest | Declaration at onboarding; recusal scope + mechanics; Owner disclosure | Not in original |

---

## §9 Acceptance of this addendum

This addendum, like the original proposal it extends, does NOT change `00_framework.md` semantics. Acceptance pathway:

1. **Owner reviews this addendum** as part of the same governance checkpoint that reads the original proposal.
2. **Owner ratifies** the dual-hat fallback policy (§1) by authoring the ADR required by §1.1 item 4 (analogous to ADR-0034) at the time of invoking the fallback. If Option A (4 distinct staffing) is selected instead, §1 is inert but §2–§6 still apply.
3. **§2–§6 are active immediately on framework v1.0.0 FROZEN cut** — they do not require separate ratification; they describe operating policy that takes effect with the freeze.
4. **This addendum transitions** `doc_status: DRAFT → ACTIVE` together with the original proposal at the time of §43.1-populated FROZEN cut.

Until then, the addendum sits as DRAFT alongside the original proposal and `reviewer_staffing_strategy.md`. It does NOT add new gates — it specifies how the existing gate operates in the constrained regime.

---

## §10 Change log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 0.1.0 | 2026-05-16 | Gustavo Schneiter (via Claude Opus 4.7, wave-22 Lote 7 absorption) | Initial addendum. Extends `specs/_proposals/2026-05-16-framework-reviewer-roles.md` v0.1.0 with §1 dual-hat fallback policy (Pairing-Alpha / Pairing-Beta), §2 cross-veto rule + 3-of-4 quorum, §3 sign-off SLA (STANDARD 3 BD / HIGH_RISK 7 BD), §4 Reviewer Training Pack 10-hour floor, §5 90-day rolling quarterly cadence anchored on FROZEN cut, §6 conflict-of-interest declaration + recusal. Does not modify FW-H-* role scopes; specifies operating mechanics for small-org case. |
| 0.1.1 | 2026-05-16 | Claude Opus 4.7 (wave-24 ADR-0034b cross-ref) | Adds §1 wave-24 cross-ref callout pointing at the newly-authored `specs/03_architecture/adrs/ADR-0034b-framework-reviewer-dual-hat-fallback.md` (the §1.1 item-4-required authorization artifact). Adds ADR-0034b to the closing cross-references list. No semantic change to operating policy. |
| 0.1.2 | 2026-05-16 | Claude Opus 4.7 (wave-25 Lote 7 RACI detail) | Adds **§6 RACI matrix detail** authoring the 15-row per-decision matrix (ADR creation, ADR approval, INV registry promotions, sprint impl sign-off, DEBT register entries, runbook approvals, TLA+ spec additions, BYOK provider additions, region additions, schema migrations, customer breach response, pentest finding triage, GA cutover, quarterly review, annual deep review) with explicit R/A/C/I cells per FW-H-* slot, single-A discipline enforced (Owner = A always), and a dedicated **§6.3 dual-hat fallback row** showing per-pairing A/R-cell migration under ADR-0034b Pairing-Alpha (FW-H-1+FW-H-3) and Pairing-Beta (FW-H-2+FW-H-4). Renumbers existing §6 (COI) → §7, §7 (summary table) → §8, §8 (acceptance) → §9, §9 (changelog) → §10. The §8 summary table is updated to reflect the seven (was six) clauses. Cross-veto preservation under dual-hat is reasserted in §6.3 last paragraph. Closes the wave-22 / wave-24 forward reference where both documents pointed at "the addendum proposal §6 RACI" without an authored detail. ADR-0034b cross-refs `addendum §6.2 / §6.4` (COI subsections) are migrated to `§7.2 / §7.4` in the same wave-25 commit. |
| 0.1.3 | 2026-05-16 | Claude Opus 4.7 (wave-26 Lote 7 follow-ons closure) | Closes the three wave-25 follow-on items deferred in `specs/_audits/sealed/2026-05-16-lote-7-raci-detail.md §8`: (1) Adds **INV binding column** to the §6.2 matrix — 6 of 15 rows bind to a concrete INV (Row 3 → INV-OBS-AUDIT-CHAIN-INTEGRITY; Row 8 → INV-BYOK-CRYPTO-SOVEREIGNTY; Row 9 → INV-REGION-NO-CROSS-LEAK; Row 10 → INV-AUTH-MIGRATION-ADDITIVE; Row 11 → INV-AUDIT-APPEND-ONLY; Row 13 → INV-ROLLOUT-COSIGN-GATE). Rows with no direct INV binding carry `—`. (2) Inserts new **§6.4 Pairing-Alpha vs Pairing-Beta selection heuristics** with decision tree, default-for-SaaS-pre-GA (Pairing-Beta), re-pairing criteria post-GA, anti-patterns, and audit-trail format. Renumbers the previous §6.4 (Disambiguating rules) → §6.5 and the previous §6.5 (Cross-references) → §6.6. (3) **CI SLA wire-up** lands `scripts/check-raci-sla.py` parsing §6.2 + §3.2 lanes + emitting an advisory conformance line; wired into `.github/workflows/spec_validation.yml` as an advisory step (never blocks). Closure audit doc: `specs/_audits/sealed/2026-05-16-lote-7-followons-closure.md`. No semantic change to §1–§5 / §7–§9 operating-policy clauses. |

---

**Cross-references:**

- `specs/_proposals/2026-05-16-framework-reviewer-roles.md §10` — wave-20 proposal cross-refs back to this addendum (wave-22 update adds §10 cross-ref line pointing here).
- `specs/_audits/sealed/2026-05-15-framework-v1-0-0-ga-audit.md §11` — wave-20 Lote 7 audit update notes that this addendum extends the proposal to handle the small-org case.
- `specs/00_framework.md §43.1` — slot labels reference role IDs FW-H-1..4 defined in the original proposal; this addendum specifies how those slots are filled, vetoed, scheduled, and trained.
- `specs/_governance/reviewer_staffing_strategy.md` — multi-tier strategy SOT; this addendum is downstream operating policy for the Tier 1 framework-freeze gate specifically.
- `specs/03_architecture/adrs/ADR-0034-prr-staffing-waiver-solo-tier.md` — prior-art for the §1.1-mandated dual-hat ADR pattern.
- `specs/03_architecture/adrs/ADR-0034b-framework-reviewer-dual-hat-fallback.md` — wave-24 ADR that authorizes the §1 dual-hat fallback at the framework-freeze tier (analog of ADR-0034 at the PRR tier); status PROPOSED, transitions to ACCEPTED on Owner invocation.
