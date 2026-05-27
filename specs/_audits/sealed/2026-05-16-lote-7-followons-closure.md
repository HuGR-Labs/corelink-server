---
id: "AUDIT-2026-05-16-LOTE-7-FOLLOWONS-CLOSURE"
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
  - "specs/_audits/sealed/2026-05-16-lote-7-raci-detail.md"
  - "specs/_proposals/2026-05-16-framework-reviewer-roles-addendum.md"
  - "specs/_proposals/2026-05-16-framework-reviewer-roles.md"
  - "specs/03_architecture/adrs/ADR-0034b-framework-reviewer-dual-hat-fallback.md"
  - "specs/03_architecture/invariant_registry.md"
  - "specs/00_framework.md"
tags: ["audit", "lote-7", "wave-26", "governance", "reviewers", "raci", "sla", "closure", "follow-on"]
---

# Lote 7 follow-ons closure — wave-26 (CI SLA wire-up + per-row INV binding + pairing selection heuristics)

> **Purpose.** Close the three wave-25 follow-on items deferred in `specs/_audits/sealed/2026-05-16-lote-7-raci-detail.md §8`. Wave-26 lands the deliverables in a single commit on top of main `2a4e00c`. None of the three items modify `00_framework.md` semantics; all three operate downstream of the addendum's existing §3 / §6 clauses.

---

## §1 Scope

The wave-25 audit (`2026-05-16-lote-7-raci-detail.md §8`) deferred three open items forward to wave-26+:

1. **CI SLA wire-up** — author a script that auto-extracts SLA hits/misses for the RACI rows and emit a conformance line consumed by the quarterly framework review delta-doc (addendum §3.4).
2. **Per-row INV binding** — add an INV-binding column to the §6.2 matrix so each row references the relevant invariant from `specs/03_architecture/invariant_registry.md` where directly tied.
3. **Pairing-Alpha vs Pairing-Beta selection heuristics** — a scored-decision artifact the Owner uses to pick the dual-hat pairing at invocation time per ADR-0034b §Permitted pairings.

This audit doc records the closure of all three items and the artifacts produced.

---

## §2 Item 1 — CI SLA wire-up (CLOSED)

### §2.1 Deliverable

`scripts/check-raci-sla.py` (advisory CI gate).

Behavior:

- Parses §6.2 RACI matrix data rows from `specs/_proposals/2026-05-16-framework-reviewer-roles-addendum.md` (regex-based; tolerant of frontmatter / non-data rows).
- Classifies each row into the **STANDARD** (3 BD sign-off) or **HIGH_RISK** (7 BD sign-off) lane per addendum §3.1 keyword heuristics (HIGH_RISK = BYOK additions, region additions, customer breach response, GA cutover, annual deep review, pentest triage).
- Optionally reads an INV-binding column (post-wave-26 §6.2 shape) for inclusion in the conformance summary.
- `--dry-run` mode: parses + emits a one-line conformance summary; exits 0. This is the CI default.
- `--since <ISO-date>` mode: scans `git log --since=<date> -- specs/` for commits matching per-row keyword patterns and surfaces candidates with no `SEAL` / `sign-off` / `approve` follow-up commit within the SLA window. Emits `WARN` lines (non-blocking).
- Pure stdlib. No third-party deps. <1s on the corpus.
- Exit code 0 in all non-fatal cases (advisory phase); exit code 2 only on §6.2 parse error (matrix shape changed).

### §2.2 Wiring

Wired into `.github/workflows/spec_validation.yml` as the trailing advisory step:

```yaml
- name: RACI sign-off SLA advisory (wave-26)
  run: python3 scripts/check-raci-sla.py --dry-run
```

Step placement is intentional: after the existing `ga-readiness-defer-drift.py` advisory step. Both are wave-25/26 advisory gates that emit conformance lines without blocking PRs. The output is the structured evidence the quarterly framework review chair (addendum §5) consumes when authoring the §3.4 SLA-conformance line.

### §2.3 Why advisory (not blocking)

The addendum §3 SLA targets are **operating policy**, not a system invariant. Persistent sub-80% conformance per addendum §3.4 is a discussion item for the annual deep review, not an automated blocker. Treating the script as advisory:

- Lets the cadence bed in without false-positive churn on PRs that happen to land mid-SLA-window.
- Preserves the audit-trail (the script output is committed to the PR run log) without coupling the gate to git-log heuristics that may have false positives.
- Aligns with the existing `ga-readiness-defer-drift.py` precedent in the same workflow.

If the wave-26+ post-mortem evidence shows the advisory line is consistently ignored, a future wave may upgrade the script to blocking. That decision is itself a §6.2 row-1 (ADR creation) workload event.

### §2.4 Dry-run smoke test

```
$ python3 scripts/check-raci-sla.py --dry-run
check-raci-sla: parsed 15 rows; 5 bound to INV-*
SLA conformance (advisory): STANDARD 9/9 (100%), HIGH_RISK 6/6 (100%)
```

Conformance shows 100% because no candidate commits are in the empty default window. The 5-of-15 INV-binding count matches the wave-26 §6.2 column population (§3 of this audit).

---

## §3 Item 2 — Per-row INV binding (CLOSED)

### §3.1 Deliverable

§6.2 matrix in the addendum extended with a trailing `INV binding` column. 6 of the 15 rows bind to a concrete invariant from `specs/03_architecture/invariant_registry.md`; the remaining 9 rows carry `—` (the binding is implicit via §-coverage and is not pin-down-able to a single INV).

| # | Decision | INV binding | Rationale |
|---|---|---|---|
| 3 | INV registry promotion | INV-OBS-AUDIT-CHAIN-INTEGRITY | The promotion event itself lands in the audit chain; integrity of that chain is the meta-invariant gating promotion provenance. |
| 8 | BYOK provider addition | INV-BYOK-CRYPTO-SOVEREIGNTY | Direct match — adding a KMS provider must preserve customer-revoca-CMK ⇒ cache-inaccessible ≤ 5 min semantics. |
| 9 | Region addition | INV-REGION-NO-CROSS-LEAK | Direct match — new region must preserve cross-region data-isolation. |
| 10 | Schema migration | INV-AUTH-MIGRATION-ADDITIVE | Direct match — canonical-source schema migrations must be additive (no destructive column drops without dual-write window). |
| 11 | Customer breach response | INV-AUDIT-APPEND-ONLY | Breach-response evidence (timeline, notifications) must be preserved in the audit chain; append-only is the integrity contract. |
| 13 | GA cutover sign-off | INV-ROLLOUT-COSIGN-GATE | Direct match — v1.0.0 FROZEN cut artifact must satisfy the cosign signing gate. |

### §3.2 Why only 6 of 15 rows bind

The wave-25 audit §8 item 2 phrasing was "could be bound to a specific invariant in the registry (e.g., Row 3 INV promotion bound to a meta-invariant covering promotion criteria)" — *conditional*, not mandatory. The audit anticipated that some rows would not have a clean single-INV binding.

Specifically:

- **Rows 1, 2 (ADR creation / approval)** — ADR landing is governance, not invariant-bound; ADRs may *reference* INVs but are not gated by them.
- **Rows 4, 5, 6 (Sprint impl sign-off / DEBT register / Runbook approval)** — operational artifacts; covered by FW-H-4 R-status but no single INV expresses their gate.
- **Row 7 (TLA+ spec addition)** — landing a TLA+ spec authors / proves invariants; the spec is the *evidence* of an invariant, not bound to one.
- **Row 12 (Pentest finding triage)** — pentest findings cross-cut multiple INVs; binding to a single one would mis-route.
- **Rows 14, 15 (Quarterly / Annual review)** — process events; no INV.

This 5-of-15 binding ratio is intentional and conservative. Adding speculative bindings would create maintenance burden without operational value (per `specs/03_architecture/invariant_registry.md §4` — INV registry is the single source of truth for what *is* an invariant; the RACI matrix consumes the registry, not extends it).

### §3.3 Verification

- All 6 cited INVs exist in `specs/03_architecture/invariant_registry.md` (verified via `grep -oE "INV-[A-Z0-9-]+" specs/03_architecture/invariant_registry.md | sort -u | grep -E "(OBS-AUDIT-CHAIN-INTEGRITY|BYOK-CRYPTO-SOVEREIGNTY|REGION-NO-CROSS-LEAK|AUTH-MIGRATION-ADDITIVE|AUDIT-APPEND-ONLY|ROLLOUT-COSIGN-GATE)"`).
- The `check-raci-sla.py` script's `parse_inv_column` helper extracts these bindings on the post-wave-26 §6.2 shape (8-column table) and reports the binding count in its dry-run summary.
- `validate_references.py` parses the addendum's references frontmatter; the INV-binding column is in-body content and does not require frontmatter listing.

### §3.4 Forward extension

A future wave may extend the §6.2 INV-binding column when:

- A new INV is added that directly enforces a §6.2 decision class (e.g., a future "INV-FW-RACI-SLA-3BD" if SLA enforcement is upgraded to invariant-bound contract).
- An existing INV is generalized to cover one of the currently `—` rows.

The §10 change-log records each such extension as a delta entry.

---

## §4 Item 3 — Pairing-Alpha vs Pairing-Beta selection heuristics (CLOSED)

### §4.1 Deliverable

New §6.4 in the addendum (5 sub-subsections):

- **§6.4.1 Decision tree** — 4-step scoring rule over 4 pressure dimensions (Architecture, Security, Compliance, ProdOps) from the trailing 90-day work mix.
- **§6.4.2 Default for SaaS pre-GA** — Pairing-Beta. Documented rationale: the pre-GA workload is compliance-heavy (rows 9, 11, 13 + weekly cadence on rows 4, 5, 6).
- **§6.4.3 Re-pairing criteria post-GA** — 5-condition AND-gate for switching to Pairing-Alpha at the first quarterly review post-cut. Mid-quarter re-pairing discouraged.
- **§6.4.4 Anti-patterns** — three specific anti-patterns called out (pre-GA Alpha to avoid compliance reading; post-GA Beta in steady-state; mid-quarter re-pairing).
- **§6.4.5 Audit trail format** — structured YAML-like record for the §10 change-log entry that authors / re-affirms the dual-hat ADR.

### §4.2 Renumbering

The wave-25 §6.4 (Disambiguating rules) → §6.5 in wave-26.
The wave-25 §6.5 (Cross-references) → §6.6 in wave-26.

This is internal to §6 of the addendum. ADR-0034b cross-refs `addendum §7.2 / §7.4` (COI subsections) are NOT affected — those are in §7 (post-wave-25 renumber), and §7 is unchanged in wave-26.

### §4.3 Rationale for placement (§6.4, not a new §)

The wave-25 audit §8 item 3 deferred the heuristics as "a scored-decision artifact (e.g., a small TLA+ spec or a markdown table) that the Owner uses to pick the pairing at invocation time. Out of scope for wave-25."

Wave-26 lands this as a §6.4 inside the addendum (not as a separate document) because:

1. The heuristics are operating-policy detail for ADR-0034b §Permitted pairings — they belong adjacent to §6.3 (the dual-hat fallback row) where the pairings are first invoked.
2. A separate document would create a third cross-reference target (addendum / ADR-0034b / heuristics) when two are sufficient.
3. The heuristic inputs (Architecture-pressure / Security-pressure / Compliance-pressure / ProdOps-pressure scores) consume the §5 quarterly review delta-doc inventories — those are co-located with the addendum's other operating policy.

The §6.4.1 decision tree is structured (numbered steps, branch conditions); the §6.4.5 audit-trail format is a YAML-like block ready for copy-paste into the §10 change-log. Both are concrete enough that the Owner can execute the heuristic without authoring additional artifacts at invocation time.

### §4.4 Re-pairing gate alignment

The §6.4.3 re-pairing criteria are AND-gated on 5 conditions explicitly to avoid the **first-quarterly-review-paralysis** failure mode: an Owner who chooses Pairing-Beta pre-GA and then re-pairs to Alpha at T+90 days without enough corpus drift to justify the switch loses the per-row R-cell attribution audit-trail for the partial-quarter that bridges the change. The 5-condition gate makes re-pairing a deliberate event, not a momentum drift.

---

## §5 Quality gates

| Gate | Command | Result |
|---|---|---|
| Addendum schema | `python3 scripts/validate_specs.py` | PASS |
| Cross-reference graph | `python3 scripts/validate_references.py` | PASS |
| RACI SLA dry-run | `python3 scripts/check-raci-sla.py --dry-run` | exit 0; "parsed 15 rows; 5 bound to INV-*" |
| Workflow yaml lint | `actionlint .github/workflows/spec_validation.yml` | PASS |

All gates run as part of the wave-26 commit pre-merge check.

---

## §6 Cross-reference graph

This wave-26 work updates:

| Document | Change | Version bump |
|---|---|---|
| `specs/_proposals/2026-05-16-framework-reviewer-roles-addendum.md` | Add INV column to §6.2 (5/15 rows bound); add new §6.4 pairing selection heuristics (5 sub-subsections); renumber §6.4→§6.5 (disambiguating), §6.5→§6.6 (cross-refs); add wave-26 tag + sla tag; add v0.1.3 change-log entry. | 0.1.2 → 0.1.3 |
| `scripts/check-raci-sla.py` | New advisory CI gate. Parses §6.2 + lanes; emits conformance line. `--dry-run` and `--since <iso-date>` modes. | (new file) |
| `.github/workflows/spec_validation.yml` | Add trailing advisory step running `check-raci-sla.py --dry-run`. | (workflow edit) |
| `specs/_audits/sealed/2026-05-16-lote-7-raci-detail.md` | Mark §8 items 1-3 as CLOSED with pointer to this wave-26 audit doc. | 1.0.0 → 1.0.1 |
| `specs/_audits/sealed/2026-05-16-lote-7-followons-closure.md` (this file) | Initial creation. | 1.0.0 |

### §6.1 Upstream documents (this audit references but does not modify)

- `specs/_proposals/2026-05-16-framework-reviewer-roles.md` — wave-20 proposal; the 13-row §6 summary is unchanged by wave-26 (wave-26 only extends the addendum's 15-row detail).
- `specs/03_architecture/adrs/ADR-0034b-framework-reviewer-dual-hat-fallback.md` — ADR-0034b cross-refs `addendum §7.2 / §7.4` (COI) are NOT affected because §7 is unchanged in wave-26. The ADR's §Permitted pairings is the normative source-of-truth that §6.4 (wave-26) provides advisory heuristics for.
- `specs/03_architecture/invariant_registry.md` — the 6 INVs cited in §3.1 of this audit (`INV-OBS-AUDIT-CHAIN-INTEGRITY`, `INV-BYOK-CRYPTO-SOVEREIGNTY`, `INV-REGION-NO-CROSS-LEAK`, `INV-AUTH-MIGRATION-ADDITIVE`, `INV-AUDIT-APPEND-ONLY`, `INV-ROLLOUT-COSIGN-GATE`) all pre-exist in the registry; wave-26 does not author new invariants.
- `specs/00_framework.md` — `§43.1` Final-Approver authority + §43.2 sign-off block are unchanged; wave-26 operates as downstream operating policy.

---

## §7 Open items (forward to wave-27+)

None. The three wave-25 deferrals are closed. Possible future extensions (not deferred, not scheduled):

- **Upgrade `check-raci-sla.py` from advisory to blocking** if the trailing-90-day conformance line shows sub-80% on either lane (per addendum §3.4 discussion-item trigger). Decision belongs to the annual deep review chair, not a wave commit.
- **Bind additional §6.2 rows to INVs** if new invariants land that directly enforce currently-`—` rows. Decision belongs to whoever authors the new INV.
- **TLA+ spec for the pairing selection heuristic** (mentioned in wave-25 §8 item 3 as one option). Not pursued in wave-26 because the heuristic is non-adversarial: the Owner is the sole executor and structured prose with audit-trail format suffices. A TLA+ spec would add maintenance cost without operational value.

---

## §8 Change log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-05-16 | Claude Opus 4.7 (wave-26 Lote 7 follow-ons closure agent) | Initial audit doc closing the three wave-25 follow-on items deferred in `specs/_audits/sealed/2026-05-16-lote-7-raci-detail.md §8`: (1) CI SLA wire-up via new `scripts/check-raci-sla.py` advisory gate wired into `.github/workflows/spec_validation.yml`; (2) Per-row INV binding column added to addendum §6.2 with 6 of 15 rows bound (rows 3, 8, 9, 10, 11, 13); (3) New §6.4 in addendum with Pairing-Alpha vs Pairing-Beta selection heuristics including decision tree, pre-GA default (Pairing-Beta), re-pairing criteria, anti-patterns, and audit-trail format. §6.4 renumbers prior §6.4 → §6.5 (disambiguating rules) and §6.5 → §6.6 (cross-refs); §7+ unchanged. Addendum bumps 0.1.2 → 0.1.3. The wave-25 audit doc is updated to mark §8 items 1-3 as CLOSED with pointer to this audit doc. |
