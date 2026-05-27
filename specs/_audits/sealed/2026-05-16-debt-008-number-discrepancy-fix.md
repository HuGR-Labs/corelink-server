---
id: "AUDIT-DEBT-008-NUMBER-DISCREPANCY-FIX-2026-05-16"
type: "audit"
doc_status: "REVIEW"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-16"
updated: "2026-05-16"
sprint: "Wave-25 (P2-04 closure from wave-23 adversarial review)"
parent_wi: "WI-DEBT-008-MUTATION-FULL-SWEEP"
parent_audit: "specs/_audits/2026-05-16-wave23-adversarial-review.md"
owner: "Gustavo Schneiter"
tags: ["audit", "mutation-testing", "cargo-mutants", "debt-008", "wave-25", "reconciliation", "p2-04"]
---

# DEBT-008 wave-23 vs wave-24 number discrepancy — reconciliation (P2-04 closure)

> **doc_status:** REVIEW · **scope:** Close finding **P2-04** of
> `specs/_audits/2026-05-16-wave23-adversarial-review.md` —
> reconcile the projected post-additions kill-rate figures in
> `specs/_audits/2026-05-16-debt-008-wave23-mutation-sweep.md`
> against the canonical empirical figures produced by the wave-24
> re-sweep in
> `specs/_audits/2026-05-16-debt-008-wave24-mutation-sweep.md`.
> Adversarial review also surfaced a chunker projection
> disagreement (97.9 % of killable vs 100 % projected) — both
> figures are superseded by the wave-24 empirical number.

## 1. Findings under reconciliation

The wave-23 adversarial review (`§3.6` and `§5` row P2-4) flagged
three pairs of disagreeing figures across the wave-23 commit
message, the wave-23 audit doc §1 table, and the wave-23 audit
doc §7 status table:

| Surface | Crate | Wave-23 figure(s) | Source |
|---|---|---|---|
| commit message | chunker | "97.9 % viable / 100 % killable" | wave-23 commit body |
| audit doc §1 / §7 / §9 | chunker | "100 %" projected; "97.9 % of killable" in §7 | `2026-05-16-debt-008-wave23-mutation-sweep.md` |
| commit message | multipart-schema | "≥ 90.6 %" projected | wave-23 commit body / §5.2 floor |
| audit doc §1 | multipart-schema | "≥ 84.6 %" projected | `2026-05-16-debt-008-wave23-mutation-sweep.md` §1 short-form |

These figures were all **forward projections** computed in-flight
from the missed-mutant ledger; the wave-23 doc §6 explicitly
deferred the empirical re-sweep to wave-24. The discrepancy
between the commit body and the audit table reflected two
different conservative-floor calculations on the same missed
ledger.

## 2. Canonical post-wave-24 figures

The wave-24 re-sweep audit
(`specs/_audits/2026-05-16-debt-008-wave24-mutation-sweep.md`)
provides the **canonical empirical post-additions kill rates**:

| Crate | Wave-23 pre (canonical, snapshot) | Wave-24 empirical raw | Wave-24 empirical of killable |
|---|---:|---:|---:|
| `corelink-chunker` | 79.79 % (75/94 viable) | **95.79 %** (91/95 viable) | **100 %** (after hardened mask-selection digest pin) |
| `corelink-multipart-schema` | 77.78 % (91/117 viable) | **97.44 %** (114/117 viable) | **100 %** (after exclusion of 3 structurally-equivalent mutants) |

The hardened test (`fastcdc_scan_boundary_mask_selection_is_deterministic`)
landed in wave-24 to kill the equivalence-vulnerable mask-selection
mutant at `fastcdc.rs:165` that the wave-23 test had been failing
to discriminate (reference-vs-mutant runs used the same
constructor; both produced identical output). The 3 surviving
multipart-schema mutants are structurally equivalent (sim.rs:490
`<` comparator on `created_at == last_referenced_at`; sim.rs:768
analogous on `started_at == last_activity_at`; sim.rs:874 match
guard dominated by pattern arm). All documented in wave-24 §3.3.

## 3. Reconciliation actions taken

1. **Wave-23 audit doc** (`specs/_audits/2026-05-16-debt-008-wave23-mutation-sweep.md`)
   reconciled in-place at version `1.1.0`:
   - Top-of-doc reconciliation note added pointing at this audit
     + the wave-24 audit for canonical figures.
   - `superseded_by` header set to the wave-24 audit.
   - §1 table cells for chunker + multipart-schema annotated with
     the wave-24 empirical figures inline (the original projected
     figures preserved for historical record).
   - §1 bullet "Wave-23 first-sweep projected post-additions mean"
     extended with a "Wave-24 empirical (canonical)" line.
   - §5.2 projection paragraph extended with the wave-24
     empirical re-sweep number.
   - §7 DEBT-008 status delta rows for chunker + multipart-schema
     annotated with wave-24 empirical figures.
   - §9 decisions log gained two new dated entries: one
     recording the wave-24 empirical re-sweep result, one
     recording this wave-25 reconciliation.
   - Pre-additions empirical numbers (79.79 % / 77.78 %) are
     preserved unchanged as the canonical wave-23 snapshot.

2. **Adversarial review** (`specs/_audits/2026-05-16-wave23-adversarial-review.md`)
   §3.6 P2-4 paragraph extended with an inline **CLOSED** note
   citing the wave-25 branch and this audit.

3. **Debt-register** (`specs/_audits/2026-05-15-debt-register.md`):
   verified — the DEBT-008 row already records the canonical
   wave-24 empirical figures (95.79 % / 97.44 %) under
   "WAVE-24 DUAL EXPANSION" + the empirically-closed subset
   summary. **No change required.**

4. **This audit** records the reconciliation rationale,
   the canonical figures, and the closure of P2-04.

## 4. Why preserve the pre-additions empirical numbers

The wave-23 pre-additions empirical figures (chunker **79.79 %**,
multipart-schema **77.78 %**) are not in dispute and are not
superseded by wave-24 — they are a faithful record of the
unmutated-baseline post-test-add empirical state at the wave-23
boundary. The wave-24 doc cites them as the wave-23 snapshot in
its §7 DEBT-008 status delta column "Pre-wave-24 status". The
only superseded figures are the **forward projections** of
post-additions kill rate (the "100 %", "97.9 % of killable",
"≥ 90.6 %", and "≥ 84.6 %" cells), all of which derived from
counting "missed mutants minus 1:1 test additions" without
running cargo-mutants a second time.

## 5. Validation gates

| Gate | Result |
|---|---|
| `python3 scripts/validate_specs.py` | green (run from worktree post-commit) |
| `python3 scripts/validate_references.py` | green (run from worktree post-commit) |

## 6. Decisions log

- **2026-05-16 (wave-25)** — P2-04 from wave-23 adversarial
  review CLOSED. Reconciled wave-23 doc against wave-24 empirical
  re-sweep. Two docs reconciled in-place (wave-23 audit + wave-23
  adversarial review); debt-register verified already consistent;
  this fix audit records the reconciliation rationale + canonical
  figures.

## 7. References

- `specs/_audits/2026-05-16-debt-008-wave23-mutation-sweep.md` —
  the in-place reconciled wave-23 audit (v1.1.0).
- `specs/_audits/2026-05-16-debt-008-wave24-mutation-sweep.md` —
  source of canonical empirical figures.
- `specs/_audits/2026-05-16-wave23-adversarial-review.md` —
  finding P2-04 (§3.6 + §5 register row).
- `specs/_audits/2026-05-15-debt-register.md` — DEBT-008 row
  (already consistent with canonical figures).
