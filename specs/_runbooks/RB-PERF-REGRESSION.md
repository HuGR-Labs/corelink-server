---
id: "RB-PERF-REGRESSION"
type: "runbook"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
owner: "Engineering Lead"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["runbook", "r-prep", "performance", "criterion", "ci-gate"]
---

# RB-PERF-REGRESSION — Perf-Regression CI Gate Triage

> **Status:** ACTIVE. Owned by Engineering Lead. Triggered by a failing
> `perf-regression` workflow on a PR. Companion docs:
> `docs/internal/PERFORMANCE-PLAYBOOK.md` §"How regression gates work",
> `scripts/perf-regression-check.py`, `reports/perf/README.md`.

## 1. Purpose

The `perf-regression` workflow runs the criterion bench suite for every
PR that touches a perf-critical crate (or carries the `perf-sensitive`
label) and fails closed when any tracked bench regresses beyond the
configured threshold (**default: 10% on p99**). This runbook is the
triage path when that workflow fails.

The gate is a follow-up to DEBT-013 (perf-optimization audit) — it
exists so that the hot-path improvements landed in DEBT-013-PERF-OPT
do not silently erode in subsequent PRs.

## 2. Trigger

- **Automatic:** GitHub PR check `criterion regression gate` fails red.
- **Manual:** Engineering Lead requests a re-run via `workflow_dispatch`
  with an override threshold (rare; requires written justification in
  the PR thread).

## 3. Pre-flight

- [ ] Confirm you are the PR author or assigned reviewer.
- [ ] Pull the **`criterion-report-<run_id>`** artifact from the failed
      workflow run (Actions tab → failed run → artifacts).
- [ ] Open `perf-regression-report.json` from the artifact — it lists
      every bench, baseline, current, and delta_pct.
- [ ] Open the criterion HTML for the offending bench(es)
      (`target/criterion/<bench>/report/index.html`) to inspect the
      raw distribution.

## 4. 3-Step Diagnosis

### Step 1 — Is the regression real?

Look at the criterion HTML report for the offending bench. Three signs
the failure is genuine signal (not CI noise):

1. **Median moved with the p99**, not just the tail. CI jitter inflates
   the tail; a real regression shifts the whole distribution.
2. **The change is reproducible locally**:
   `cargo bench -p <crate> --bench <bench>` on your laptop reproduces
   a delta in the same direction (sign matters; magnitude may differ).
3. **The PR touches the bench's hot path** (or a transitive dep). If
   the regression is in `corelink-hash::blake3` but you only edited
   docs, the regression is almost certainly noise — re-run the workflow.

If steps 1–3 do not converge on "real", treat as noise. Re-run the
workflow once; if the second run is green, proceed normally.

### Step 2 — Where is the regression?

If real, narrow it down:

- `git log --oneline -p crates/<crate>` since the bench's last green
  baseline (`reports/perf/baseline-<crate>-<bench>.json` → `commit`
  field).
- `cargo bench -p <crate> --bench <bench> -- --save-baseline pr` then
  `git checkout main && cargo bench -p <crate> --bench <bench> -- --baseline pr`
  on a clean tree to bisect.
- Profile with `perf record` / `cargo flamegraph` against the offending
  bench: look for new allocations (see PLAYBOOK Pattern A), new
  synchronous I/O on the hot path (Pattern C), or unbounded cloning
  (Pattern B).

### Step 3 — Cross-link to DEBT-013

Every regression on a tracked bench is — by definition — eroding a
DEBT-013 optimization. Open the related ticket in
`specs/_audits/perf-optimization-followup-tickets.md` and add a comment
that the regression was observed. If the WI is closed, file a new
follow-up ticket under the same DEBT-013 umbrella tagged
`debt-013-regression`.

## 5. Options (pick one)

After diagnosis, the PR author picks **one** of three resolutions and
records the choice in the PR description under a `## Perf` heading:

### 5.1 Optimize (preferred)

Apply the fix; re-run the workflow; merge once green. This is the
default — the gate exists to force this option.

### 5.2 Revert

If the regression is a side-effect of an unrelated change (e.g. a
dependency bump or a refactor that proved more costly than expected),
revert the offending hunk. Re-run; merge.

### 5.3 Accept regression with sign-off

Only when a deliberate trade-off was made (e.g. a security hardening
that costs 12% on p99 but closes a CVE). Requires:

- [ ] **Engineering Lead** sign-off in the PR thread.
- [ ] **Quality Lead** sign-off in the PR thread.
- [ ] PR description includes:
  - The exact regression observed (bench, baseline, current, delta_pct).
  - The rationale (security, correctness, feature delivery).
  - A follow-up WI in `specs/_audits/perf-optimization-followup-tickets.md`
    with a target date for restoring the baseline.
- [ ] A baseline refresh PR follows the merge:
  `scripts/refresh-perf-baseline.sh && git commit -m "chore(perf): accept regression for <PR>"`.
  This refresh PR is reviewed by Engineering Lead.

> **Hard rule:** Accept-with-sign-off requires two approvals AND a
> follow-up WI. No exceptions. If you cannot get two approvals, you
> must optimize or revert.

## 6. After resolution

- [ ] PR description updated under `## Perf` with the chosen option.
- [ ] If option 5.3: follow-up baseline-refresh PR opened and linked.
- [ ] Workflow re-run is green (or workflow is explicitly bypassed via
      5.3's documented process).

## 7. Bypass procedure (emergency only)

If the gate itself is broken (false positives blocking unrelated work),
Engineering Lead may temporarily mark the workflow as non-required in
branch protection. **This is logged as an incident**
(`specs/_incidents/`) and the gate must be restored within 24h.

The bypass procedure is **never** the right call for "we don't have
time to fix the regression" — use option 5.3 instead.

## 8. References

- `docs/internal/PERFORMANCE-PLAYBOOK.md` — patterns + bench evidence.
- `specs/_audits/2026-05-14-perf-baseline.md` — original criterion baseline.
- `specs/_audits/2026-05-15-perf-optimization-audit.md` — DEBT-013 audit.
- `specs/_audits/perf-optimization-followup-tickets.md` — DEBT-013 backlog.
- `scripts/perf-regression-check.py` — gate implementation.
- `scripts/refresh-perf-baseline.sh` — operator refresh script.
- `reports/perf/README.md` — committed baseline manifest format.
- `.github/workflows/perf-regression.yml` — CI workflow.
- `specs/_runbooks/RB-GA-CUTOVER.md` §4 G1 + §0.2.12 + §5.1 RB-T3 — GA cutover greenlight criterion G1 (P99 latency ≤ SLO 5 regions) + rollback trigger RB-T3 (P99 SLO breach >15min) hand off to this runbook.
