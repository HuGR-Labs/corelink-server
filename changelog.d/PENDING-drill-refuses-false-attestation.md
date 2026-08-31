### Fixed

- **The BYOK kill-switch drill manufactured a passing SLA attestation from a
  `sleep` (B-084).** Every phase of `scripts/byok_kill_switch_drill.sh` assigned
  a hardcoded constant — `TENANT_STATUS="active"`, `DETECTED=true`,
  `TENANT_STATUS_POST="degraded_read_only"`, `RECOVERY_STATUS="active"` — and
  `SLA_RESULT` was pinned to `"PASS"`. The real KMS calls are comments, and the
  workflow's provider credentials are commented out. The report it wrote to
  `specs/_audits/` is forwarded to customer SRE as SLA evidence by
  `marketing/lighthouse-kit/05-sla-attestation-instructions.md`.

  This is categorically worse than a missing control (B-083 tracks *building*
  it): a missing control leaves a gap, this manufactured proof the gap was
  closed.

  Each stubbed phase now registers in a ledger, and the script refuses **before**
  creating the file: no report, non-zero exit, all five stub phases named. The
  file is never written rather than written with a warning banner — a banner does
  not survive a copy-paste into a customer form. There is deliberately no
  override flag. Implementing a phase means writing its provider call and
  deleting its `drill_stub` line; the attestation becomes reachable when the
  ledger empties, and not one phase before.

### Changed

- **`byok_kill_switch_drill_weekly.yml` is dispatch-only.** With the refusal in
  place a weekly cron is a chronic red nobody can act on — the lesson B-058 already
  paid for. It also fails this repo's own cron rule: the drill's outcome is decided
  entirely by constants in the script, so it cannot change without a commit.

### Notes

- Two measured corrections to B-084's own body, recorded in the item:
  - **"Automatic generator of false evidence" overstated the reach.** The lane
    reports `success` weekly (12 of the last 19 runs, latest 2026-08-30) but no
    report has *ever* reached the repo: the "Commit drill report" step runs
    `git add` + `git commit` and never **pushes**, so the commit dies with the
    ephemeral checkout. The only drill attestation in the tree,
    `specs/_audits/sealed/2026-05-14-byok-kill-switch-drill-aws.md`, was committed
    by hand at the WI-S14-006 SEAL. Population is **1**, not a growing pile.
  - Fixing that missing push *before* implementing the phases would have started
    the false attestations actually landing. Recorded in the workflow header.
