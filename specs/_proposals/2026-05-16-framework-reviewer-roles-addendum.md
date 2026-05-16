---
id: "PROPOSAL-2026-05-16-FRAMEWORK-REVIEWER-ROLES-ADDENDUM"
type: "governance"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "0.1.1"
created: "2026-05-16"
updated: "2026-05-16"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["governance", "reviewers", "staffing", "proposal", "framework-freeze", "wave-22", "lote-7", "ga", "addendum", "dual-hat"]
references:
  - "specs/00_framework.md"
  - "specs/_proposals/2026-05-16-framework-reviewer-roles.md"
  - "specs/_audits/2026-05-15-framework-v1-0-0-ga-audit.md"
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
5. Audit baseline `specs/_audits/2026-05-15-framework-v1-0-0-ga-audit.md` + `specs/_audits/2026-05-15-ga-readiness-consolidation-wave-13-17.md`
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

## §6 Conflict-of-interest declaration

The original proposal does not address the case where a FW-H-* reviewer is also the author of a framework section, ADR, or canonical source under review. This section closes that gap.

### §6.1 Declaration requirement

At onboarding, each nominated FW-H-* reviewer declares (in the §3 acceptance deliverable comments doc, frontmatter section labeled `conflicts_declared:`) the framework §-sections, ADRs, or canonical sources they personally authored or substantively co-authored. Examples:

- "I co-authored ADR-0017 in 2026-Q2; I will RECUSE on any decision row in §6 RACI that touches ADR-0017's subject matter."
- "I authored §22 SLO methodology in framework v0.7.0; I do NOT recuse on §22 because authorship is > 12 months old and ≥ 2 subsequent maintenance edits by others have superseded my contribution."

### §6.2 Recusal scope

A reviewer MUST recuse from approving (i.e., shift their RACI status from R/A to ABSTAIN with `recusal_reason: <text>`) on any framework decision that:

1. Promotes their own un-reviewed work to FROZEN status (authorship < 6 months ago and < 2 maintenance edits since).
2. Adjudicates a dispute where they are also a stakeholder (e.g., they authored an ADR that another reviewer is BLOCKing).
3. Modifies a section they wrote IF the modification is non-trivial (more than typo/link-fix) and authored within the last 6 months.

### §6.3 Recusal mechanics

A recused reviewer:

- Records the recusal in the §43.1 sign-off line as `RECUSED — <reason> — <date>`.
- May still participate as **C** (Consulted) in the original proposal §6 RACI rows, contributing factual input but NOT signing.
- Their slot's quorum requirement (§2.4 of this addendum) is satisfied by their backup (§7 of the original proposal) if the backup is available; otherwise the quorum drops to "2 of 3 effective seats" for that decision only.

### §6.4 Owner conflict of interest

Because the Owner is always Final Approver (always **A**), Owner recusal is structurally impossible at the framework freeze tier (there is no escalation above Final Approver). The dual-hat fallback (§1 of this addendum) intensifies this — when Owner holds 2 slots, conflicts are correspondingly more likely.

Mitigation: when the Owner has a personal conflict on a decision (e.g., the Owner authored the section under review AND is one of the dual-hat slots signing off), the §42 change-log entry MUST disclose the conflict explicitly with `owner_conflict_disclosed: <reason>`. This is an audit-trail requirement, not a recusal — the Owner does NOT recuse, but the conflict is on-record for future external auditors (e.g., during the SOC2 readiness exercise referenced in `specs/_audits/2026-05-14-soc2-readiness-score.md`).

### §6.5 Auditor review on conflicts

The quarterly framework review (§5 of this addendum) inspects the list of `conflicts_declared:` and `owner_conflict_disclosed:` entries from the prior 90 days and flags any patterns (e.g., a single reviewer consistently recusing on a high-traffic §-section, suggesting a different reviewer should take the slot). Persistent patterns are an agenda item for the annual deep review.

---

## §7 Operating-policy summary table

For quick reference, the six clauses of this addendum collapse to:

| § | Clause | What it adds | Pre-existing in original proposal? |
|---|---|---|---|
| §1 | Dual-hat fallback | Names Pairing-Alpha / Pairing-Beta; auto-expiration triggers; mechanics for §43.1 sign-off block | Hinted at in OQ-1 Option C; this addendum formalizes |
| §2 | Cross-veto rule | `vote: BLOCK / APPROVE / ABSTAIN / WITHDRAW` semantics; 3-of-4 quorum; Owner tie-break | Not in original |
| §3 | Sign-off SLA | STANDARD = 3 BD / HIGH_RISK = 7 BD; lane routing; exception types; tracking | Not in original |
| §4 | Reviewer training pack | 10-hour floor on existing §4 onboarding read order | Read order pre-existed; budget number is new |
| §5 | Quarterly cadence (90-day rolling) | Anchor on FROZEN cut date; first-review scope; chair rotation | Original §5.2 said "Q1/Q2/Q3/Q4"; this anchors to commit date |
| §6 | Conflict-of-interest | Declaration at onboarding; recusal scope + mechanics; Owner disclosure | Not in original |

---

## §8 Acceptance of this addendum

This addendum, like the original proposal it extends, does NOT change `00_framework.md` semantics. Acceptance pathway:

1. **Owner reviews this addendum** as part of the same governance checkpoint that reads the original proposal.
2. **Owner ratifies** the dual-hat fallback policy (§1) by authoring the ADR required by §1.1 item 4 (analogous to ADR-0034) at the time of invoking the fallback. If Option A (4 distinct staffing) is selected instead, §1 is inert but §2–§6 still apply.
3. **§2–§6 are active immediately on framework v1.0.0 FROZEN cut** — they do not require separate ratification; they describe operating policy that takes effect with the freeze.
4. **This addendum transitions** `doc_status: DRAFT → ACTIVE` together with the original proposal at the time of §43.1-populated FROZEN cut.

Until then, the addendum sits as DRAFT alongside the original proposal and `reviewer_staffing_strategy.md`. It does NOT add new gates — it specifies how the existing gate operates in the constrained regime.

---

## §9 Change log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 0.1.0 | 2026-05-16 | Gustavo Schneiter (via Claude Opus 4.7, wave-22 Lote 7 absorption) | Initial addendum. Extends `specs/_proposals/2026-05-16-framework-reviewer-roles.md` v0.1.0 with §1 dual-hat fallback policy (Pairing-Alpha / Pairing-Beta), §2 cross-veto rule + 3-of-4 quorum, §3 sign-off SLA (STANDARD 3 BD / HIGH_RISK 7 BD), §4 Reviewer Training Pack 10-hour floor, §5 90-day rolling quarterly cadence anchored on FROZEN cut, §6 conflict-of-interest declaration + recusal. Does not modify FW-H-* role scopes; specifies operating mechanics for small-org case. |
| 0.1.1 | 2026-05-16 | Claude Opus 4.7 (wave-24 ADR-0034b cross-ref) | Adds §1 wave-24 cross-ref callout pointing at the newly-authored `specs/03_architecture/adrs/ADR-0034b-framework-reviewer-dual-hat-fallback.md` (the §1.1 item-4-required authorization artifact). Adds ADR-0034b to the closing cross-references list. No semantic change to operating policy. |

---

**Cross-references:**

- `specs/_proposals/2026-05-16-framework-reviewer-roles.md §10` — wave-20 proposal cross-refs back to this addendum (wave-22 update adds §10 cross-ref line pointing here).
- `specs/_audits/2026-05-15-framework-v1-0-0-ga-audit.md §11` — wave-20 Lote 7 audit update notes that this addendum extends the proposal to handle the small-org case.
- `specs/00_framework.md §43.1` — slot labels reference role IDs FW-H-1..4 defined in the original proposal; this addendum specifies how those slots are filled, vetoed, scheduled, and trained.
- `specs/_governance/reviewer_staffing_strategy.md` — multi-tier strategy SOT; this addendum is downstream operating policy for the Tier 1 framework-freeze gate specifically.
- `specs/03_architecture/adrs/ADR-0034-prr-staffing-waiver-solo-tier.md` — prior-art for the §1.1-mandated dual-hat ADR pattern.
- `specs/03_architecture/adrs/ADR-0034b-framework-reviewer-dual-hat-fallback.md` — wave-24 ADR that authorizes the §1 dual-hat fallback at the framework-freeze tier (analog of ADR-0034 at the PRR tier); status PROPOSED, transitions to ACCEPTED on Owner invocation.
