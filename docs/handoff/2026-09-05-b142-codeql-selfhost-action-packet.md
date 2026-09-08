# B-142 CodeQL self-hosting action packet

Status: CodeQL and secrets-drift engineering wiring complete; external owner
and run evidence remain.

## Landed in this work package

- `.github/workflows/codeql.yml` runs the closed Rust, JavaScript/TypeScript,
  and Python matrix on `corelink`, not `ubuntu-*`.
- The matrix is serialized (`max-parallel: 1`), CodeQL is limited to two
  threads/4 GiB, Rust uses the workspace-pinned host toolchain, and CodeQL
  dependency caching is enabled.
- CodeQL still runs when Advanced Security is unavailable. Each leg fails
  closed when it emits no SARIF, retains the SARIF plus a run manifest for 90
  days, and preserves the Security-tab `upload-sarif` action.
- The `init`, `analyze`, and `upload-sarif` actions all use the verified
  CodeQL Action `v4.37.1` commit
  `7188fc363630916deb702c7fdcf4e481b751f97a`; friendly version, env, and SHA
  comments are one ADR + Security review unit.
- `codeql-evidence-watchdog.yml` independently checks the latest scheduled run,
  exact three-job population, successful conclusions, and all three retained
  artifacts. A missing/failed/stale/unexpected result opens one stable issue
  and fails the watchdog.
- `.github/workflows/secrets-drift.yml` runs its scheduled and PR drift gate on
  `corelink`, retains the primary report plus a run manifest, and fails closed
  when the primary report is absent. Its independent watchdog defaults to
  `corelink` and only permits the owner-controlled hosted fallback when
  `vars.HOSTED_ACTIONS_AVAILABLE == 'true'`.

## External actions before claiming full operational closure

1. The owner must enable GitHub Advanced Security/code scanning for
   `HuGR-Labs/corelink-server`. Until then, SARIF is retained as an artifact
   and the Security-tab upload is explicitly reported unavailable; this is not
   represented as a successful Security-tab upload.
2. The owner/operator must confirm the `corelink` runner fabric is online, then
   retain a successful scheduled CodeQL run (all three jobs/artifacts) and a
   successful scheduled secrets-drift run (the primary report). The watchdogs
   cannot observe a workflow while the entire runner fabric is offline;
   `runner-fleet-health.yml` is the separate fleet signal.
3. Re-run `python3 -S scripts/verify_b142_workflows.py`, the focused tests,
   focal actionlint, and both watchdogs after any runner label, workflow, or
   GHAS change.

The secrets-drift implementation remains separately owned and tracked by B-132;
this packet records its current runner wiring only and does not claim B-132
closure or external evidence.
