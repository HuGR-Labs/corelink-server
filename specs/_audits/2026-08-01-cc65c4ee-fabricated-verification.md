---
id: "AUDIT-2026-08-01-CC65C4EE-FABRICATED-VERIFICATION"
type: "audit_report"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-08-01"
updated: "2026-08-01"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["audit", "process", "verification-integrity", "high-risk-lane", "wi-s14-002", "region-pinning", "ci-gate"]
---

# Process finding — WI-S14-002 (`cc65c4ee`) was SEALED on a fabricated verification result

> **Date:** 2026-08-01 · **Trigger:** independent review of PR #920, which had
> proposed the wrong root cause for a red CI gate. · **Class:** process /
> verification integrity — **not** a product defect. · **Method:** git archaeology
> over the authoring commit's own tree and diff; every claim below is reproducible
> with the command shown.

## Verdict

**A HIGH_RISK-lane work item was declared SEALED with a "VERIFY RESULTS (all
clean)" block containing at least one result that was arithmetically impossible
in the tree the commit created.** No data was exposed and no isolation control is
defective. The damage is to the trust model: the SEAL ceremony, the CI gate it
stood up, and the incident runbook it shipped were all accepted on an unverified
attestation, and the gate has been red ever since — for **~2.5 months**, not the
two weeks previously believed.

## The finding

Commit `cc65c4ee` ("WI-S14-002 SEALED: Tenant region pinning enforcement …",
authored 2026-05-14) states, under a heading that reads **`VERIFY RESULTS (all clean)`**:

```
- rb_region_leak_dry_run.sh: PASS=8 FAIL=0
```

That result was not obtainable from the tree that same commit produced.

### Proof

1. **The script and the artifacts it asserts were added in the SAME commit, already
   disagreeing.** `cc65c4ee` adds `scripts/rb_region_leak_dry_run.sh`, whose Step 2
   asserts `migrations/d1/0027_tenant_primary_region.sql` and whose Step 3 asserts
   `specs/03_architecture/adrs/ADR-S14-001-region-pinning-enforcement.md`:

   ```
   git show cc65c4ee:scripts/rb_region_leak_dry_run.sh | grep -n 'MIGRATION=\|ADR='
   ```

   The same commit's diff adds `migrations/d1/**0028**_tenant_primary_region.sql`
   and `specs/03_architecture/adrs/ADR-S14-**002**-region-pinning-enforcement.md`
   (`git show cc65c4ee --stat`).

2. **Neither asserted path existed anywhere in that tree.**
   `git ls-tree cc65c4ee migrations/d1/` lists `0025`, `0026`, `0028` — there is no
   `0027` at all; the WI skipped the number.

3. **Neither asserted path has ever existed, on any ref, at any time.**

   ```
   git log --all --diff-filter=AD -- migrations/d1/0027_tenant_primary_region.sql   # empty
   ```

4. **Therefore the only reachable score was `PASS=5 FAIL=2`.** The script has 8 pass
   points (Steps 1–7 plus the nested Step 2a). A missing migration fails Step 2 and
   skips Step 2a (nested inside its `if`); a missing ADR fails Step 3. 8 − 3 = 5
   passes, 2 failures. This is an arithmetic result read off the script's own
   accounting, not a re-execution of the 2026-05-14 tree — the two `_fail` calls
   are unconditional given (2) and (3), so no execution environment can change it.
   (For contrast, the corrected script on this branch was executed on 2026-08-01
   and prints `RB-region-leak dry-run summary: PASS=8 FAIL=0`.)

### A corroborating defect in the same commit body

The commit's own **ARTIFACTS** section names `migrations/d1/0027_tenant_primary_region.sql`
and `ADR-S14-001-region-pinning-enforcement.md` — two files the commit does **not**
add. The message describes a tree that was never built, and the script was written
against that same imagined tree. This is consistent with a verification block
composed from intent rather than from an executed run.

## What this is NOT

Reviewers of PR #920 initially concluded the paths had been **renumbered** (`0027`→`0028`,
`S14-001`→`S14-002`) by later work. That is false and the correction matters, because
"a rename drifted a gate" is an ordinary maintenance miss whereas the truth is a
verification-integrity failure.

- `migrations/d1/0027_region_provisioning.sql` and
  `specs/03_architecture/adrs/ADR-S14-001-multi-region-terraform-module.md` were
  introduced by a **different work item**, `96d9ae28` (WI-S14-001, R2+D1+DO 4-region
  Terraform provisioning). Different content, different WI, no rename relationship.
- `cc65c4ee` is **not** an ancestor of `96d9ae28` (`git merge-base --is-ancestor`
  returns non-zero). The number `0027` was simply skipped — presumably reserved for
  the parallel WI-S14-001 branch — while the script and commit body kept referring
  to it.

## Blast radius

| Artifact | State | Consequence |
|---|---|---|
| `scripts/rb_region_leak_dry_run.sh` | red since birth (2026-05-14) | The `RB-region-leak dry-run (INV-REGION-NO-CROSS-LEAK forensic)` job failed on **every** nightly cron and every path-matching PR for ~2.5 months. |
| `specs/05_quality/runbooks/RB-region-leak.md` §Pre-conditions | wrong migration id since birth | An on-call running a **Schrems II / LGPD Art. 33** incident would have verified `0027_region_provisioning.sql` — which carries neither `primary_region` nor `trg_tenant_primary_region_immutable` — and could have concluded the enforcement was absent (or present) on the wrong evidence. |
| The script's header index | desynchronised from its own code in numbering **and** content | Header claimed a "Step 6: Drift findings committed" check that the code never implemented. |
| `.github/workflows/region_pinning.yml` | job timeouts never calibrated | 3 of the 4 substantive jobs were cancelled on timeout the first time anyone actually watched them run (2026-08-01, run `30715557213`). No number in that workflow had ever been observed green. |

**Not affected:** the isolation controls themselves. The 10 adversarial cross-region
scenarios and the 30k property test are real and assert real behaviour; they have
never reported a leak. The gate was crying wolf, which is strictly worse than no
gate — the fleet learns to ignore it — but the wolf was never there.

## Unattested claims

Because the one line in `cc65c4ee`'s `VERIFY RESULTS (all clean)` block that is
independently checkable today is **false**, the remaining lines carry no residual
credibility and are recorded here as **unattested** — not as disproven:

- `58 tests total PASS (12 prop 30k + 10 adversarial + 18 integration + 9 regression + 9 existing)`
- `clippy -D warnings: CLEAN`
- `migrations additive: 30 files scanned OK`
- `YAML smoke: OK`

They may well all have been true. There is no evidence either way, and none of them
was gated by CI at the time — the workflow that would have checked them was
introduced *by this very commit*, and its first real execution was 2026-08-01.

## Remediation

Shipped in PR #920 (branch `fix/region-pinning-gate-stale-paths`):

1. Script Steps 2/3 repointed to the artifacts that actually exist (`0028`, `ADR-S14-002`).
2. Script header index rewritten to match the code it documents, step for step.
3. Runbook §Pre-conditions corrected to `0023` (column + `CHECK`) **and** `0028`
   (backfill + immutable trigger), with an inline note recording the correction.
4. `region_pinning.yml`: the stale `0027` comment fixed; the three self-hosted-Mac
   job timeouts raised against **observed** run durations; the `--all-targets` test
   step capped at smoke proptest density so it stops re-running the mandated 30k
   density a third time in debug.

## Process recommendation (open)

The gap is that a `SEALED` commit body is free-text prose that nothing verifies. A
`VERIFY RESULTS` block asserting a script's exit score is indistinguishable, to any
reviewer or gate, from a `VERIFY RESULTS` block that was typed. Two candidate
mitigations, neither yet adopted — deliberately left as a decision for the owner
rather than pre-empted here:

- **Gate-first sealing:** a HIGH_RISK WI may not claim a CI result the WI itself
  introduces; the gate must have executed on the PR before SEAL.
- **Machine-checkable attestation:** verification blocks carry the run URL / job id
  the number came from, and a lint rejects a bare number.

## Reproduce

```bash
git show cc65c4ee --stat
git show cc65c4ee:scripts/rb_region_leak_dry_run.sh | grep -n 'MIGRATION=\|ADR='
git ls-tree cc65c4ee migrations/d1/ --name-only
git log --all --diff-filter=AD -- migrations/d1/0027_tenant_primary_region.sql   # empty
git merge-base --is-ancestor cc65c4ee 96d9ae28; echo $?                          # non-zero
git show cc65c4ee | git stripspace | grep 'PASS=8'
```
