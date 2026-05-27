---
id: "RB-CANONICAL-DRIFT"
type: "runbook"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
sprint: "R-PREP"
parent_wi: "WT-R-PREP-CANONICAL-LINT"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["runbook", "spec-hygiene", "canonical-consistency", "INV", "drift"]
---

# RB-CANONICAL-DRIFT — Triaging INV registry × TLA × code × test drift

> **Trigger:** `validate_canonical_consistency.py` (or
> `canonical-consistency.yml` CI workflow) fails on a pull request, or the
> weekly digest surfaces a regression vs baseline
> (`specs/_audits/sealed/2026-05-15-canonical-consistency-baseline.md`).
>
> **Severity:** SEV-3 (process / hygiene). No customer impact; blocks PR
> merge until resolved.
>
> **SLA:** Triage within the same PR (no carry-over). Code-owner of the
> failing crate owns triage; if ambiguous, fall back to author of the PR.

## 1. What the validator detects

`scripts/validate_canonical_consistency.py` cross-checks four sources:

1. **Declared** — `specs/03_architecture/invariant_registry.md §3` rows.
2. **TLA-verified** — `INV-*` mentions in `specs/tla/*.tla` (canonical) and
   `specs/03_architecture/tla+/runbooks/*.tla` (runbook).
3. **Code-referenced** — `INV-*` mentions in `crates/*/src/**.rs`.
4. **Test-referenced** — `INV-*` mentions in `crates/*/tests/**.rs` and in
   `#[cfg(test)]` blocks inside `src/`.

Four drift classes are surfaced. Triage flow per class:

```
+----------------------+------------------------------------------------+
| Drift class          | Triage path (see §2..§5)                       |
+----------------------+------------------------------------------------+
| orphan-ref           | §2 — code mentions an undeclared INV           |
| declared-no-code     | §3 — INV in registry, never referenced in src/ |
| test-without-code    | §4 — test names an INV; no src/ pointer        |
| critical-no-TLA      | §5 — CRITICAL severity; no .tla proof          |
+----------------------+------------------------------------------------+
```

## 2. orphan-ref — INV in code, NOT in registry

### Decision tree

1. **Was the INV recently renamed?**
   - Yes → search registry §5 aliases; either add an alias row mapping
     the legacy name to the canonical, OR (preferred) rename the code
     reference. Re-run validator.
   - No → continue.
2. **Is the INV forward-looking** (a sprint WI declared it but it has
   not yet been promoted to registry §3)?
   - Yes → either (a) promote it to the appropriate §3 sub-section
     **in this PR**, or (b) add it to the validator's WHITELIST_NON_IDS
     with a TODO referencing the sprint WI that will promote it.
   - No → continue.
3. **Is it a typo?** Compare to existing IDs (`grep -i INV- registry`).
   - Yes → fix the code reference. Re-run validator.
4. **Is it a NEW domain not yet represented in the registry?** (Common
   when a new crate ships first INVs ahead of registry update.)
   - Yes → open a new sub-section in registry §3 (`### 3.N <Domain>
     (domain <DOMAIN>)`) with at minimum the INV row + severity +
     enforcement. Commit registry + code together.

### Example (BACKUP domain, 2026-05-15)

`INV-BACKUP-FRESH` appears in `corelink-backup-verify/src/lib.rs` but no
`§3.X Backup` section exists. Open `§3.20 Backup` with rows for
INV-BACKUP-FRESH (HIGH), INV-BACKUP-RESTORE-EPHEMERAL (HIGH),
INV-BACKUP-INTEGRITY-SAMPLE-CAP (HIGH). Cross-reference
`RB-BACKUP-VERIFICATION.md`.

## 3. declared-no-code — INV in registry, no src/ reference

This is **expected** for forward-looking invariants whose implementation
sprint has not yet landed. The validator warns but does not fail unless
the BASELINE rollup count rises above the floor pinned in
`specs/_audits/sealed/2026-05-15-canonical-consistency-baseline.md`.

### Decision tree

1. **Is the INV scheduled for an unfinished sprint** (S-12..S-21 still
   open)?
   - Yes → leave as-is; the sprint impl WI will close it. Note the
     INV-ID in the sprint preflight review checklist.
2. **Has the sprint shipped but no code/test references the INV?**
   - Yes → either (a) add a comment referencing the INV in the relevant
     handler (the cheap fix; document where it is enforced), or (b)
     delete the INV from registry §3 and add to §5 with `(deprecated:
     removed in <date>)`.
3. **Is the INV a high-level policy** (e.g. process / runbook controls)
   with no direct code surface?
   - Yes → move from §3 to `compliance_matrix.md` as a CTRL row; remove
     from invariant_registry.

## 4. test-without-code — INV named in test, no src/ pointer

Usually fine: the test asserts behaviour in a crate that doesn't yet
have the comment marker. Fix: add `// INV-<ID>: <one-liner>` comment in
the production code path the test exercises. Costs nothing; ratchets the
`code_referenced` baseline up.

## 5. critical-no-TLA — CRITICAL severity, no .tla proof

This validator duplicates `check_tla_obligations.py`'s §4.2 PLANNED
matrix check. If the two disagree (this script flags more than the
obligations check), the registry §4.2 row likely lists a TLA filename
that doesn't actually contain the INV mention. Fix: either update the
.tla header to include the INV-ID, or update the §4.2 row to reflect
the actual INV-IDs the spec proves.

## 6. Baseline ratchet handling

The baseline file pins floors via embedded HTML comments:

```
<!-- BASELINE code_referenced=77 -->
<!-- BASELINE test_referenced=65 -->
<!-- BASELINE critical_referenced=29 -->
<!-- BASELINE orphan_refs=15 -->
```

- `code_referenced`/`test_referenced`/`critical_referenced` — MUST go UP
  or stay flat across PRs. Any regression fails CI.
- `orphan_refs` — MUST go DOWN or stay flat. Any increase fails CI.
- When a sprint closes its INV cluster, update the baseline in the
  same PR that lands the implementation. Baseline edits go through PR
  review like any spec change.

## 7. Quick commands

```bash
# Local report (human-readable):
python3 scripts/validate_canonical_consistency.py

# Machine-readable JSON for digest tooling:
python3 scripts/validate_canonical_consistency.py --json --out /tmp/cc.json

# Show only failures, exit non-zero on regression:
python3 scripts/validate_canonical_consistency.py 2>&1 | tail -30

# Inspect orphan refs by source:
python3 scripts/validate_canonical_consistency.py --json | \
  python3 -c 'import json,sys; d=json.load(sys.stdin); print(json.dumps(d["orphan_refs"], indent=2))'
```

## 8. Cross-references

- Baseline: `specs/_audits/sealed/2026-05-15-canonical-consistency-baseline.md`
- Validator chain doc: `docs/internal/CODE-REVIEW-CHECKLIST.md` §L6
- TLA obligation gate: `scripts/check_tla_obligations.py` + registry §4
- SOC 2 evidence: `specs/_compliance/SOC2-EVIDENCE-ROLLUP-2026-05-15.md`
  CC4.1 (continuous compliance monitoring)
- GA gate: `ROADMAP-TO-GA.md` §7 R-7 (evidence pack includes canonical
  consistency JSON report)
- `specs/_runbooks/RB-GA-CUTOVER.md` §1.2 + §2.2 + §0.2.11 — GA cutover migration-additivity gate + proptest gate consume this runbook; pre-cutover sweep ≤ 24h mandatory.
