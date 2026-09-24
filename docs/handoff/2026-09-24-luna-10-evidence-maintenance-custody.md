# Luna 10 Evidence Wave — Custody Record

Date: 2026-09-24  
Repository: `HuGR-dev/corelink-server`  
Remote main baseline: `5ba43b248d7b04376db465a1c99388598059882b`  
Isolated branch: `codex/luna-10-evidence-maintenance-custody-20260924`

## Handoff consistency

The campaign handoff says **“No repository mutations made”**, but its repository-fixes
table lists four applied fixes, and the shared checkout contained corresponding
uncommitted changes in five files. These statements conflict if “repository mutation”
includes a working-tree edit. The evidence supports a narrower statement: no fix was
merged or otherwise integrated into `main`; changes had been applied locally and were
left uncommitted. This PR preserves the selected changes without deleting or altering
their originals in the shared checkout.

## Preserved source and custody

The selected source was the diff in the shared checkout at `a577032ab816c1ae5debb2f6ddf29494162f1336`.
It was copied as a patch to `/tmp/luna10-evidence-custody.patch`, then applied in a new
worktree based on the remote main baseline above. The preserved patch SHA-256 is
`5a8abb759e562beefda5f01309e8d24152eec3c18f39d198ab122e8ffff9a8e5`. These five
original paths were selected from the dirty source; one focused regression test was
added in isolation for the B-083 verifier change:

| Issue | File(s) | Acceptance criterion |
|---|---|---|
| #1657 | `CLAUDE.md`; `evidence/owner-actions/B-098/release-governance-blocker-2026-09-09.json` | Recorded full-schema and total spec populations agree at 489 and 500; B-098 remains blocked until the external release evidence is satisfied. |
| #1676 | `scripts/verify_b154_instrument_claims.py` | B-154 fail-closed verifier recognizes the current B-083 lifecycle receipt v2 shape while keeping the legal/capability claim unresolved. |
| #1700 | `scripts/verify_staging_target.py` | Staging duration verifier accepts the current bounded `DURATION=30s` declaration while retaining the 2h dispatch default check. |
| #2165 | `scripts/verify_b083_data_plane_wiring.py`; `tests/test_verify_b083_data_plane_wiring.py` | B-083 wiring verifier checks the real-provider feature where it is declared (`crates/corelink-container/Cargo.toml`) and keeps the runtime wiring assertions; a regression test proves the manifest feature is required. |

All other dirty and untracked paths in the shared checkout were excluded. The five
original diffs remain there unchanged. No worktree cleanup or shared-checkout cleanup
was performed.

## Work tracking

- Pull request: pending creation.
- Head: pending commit.
- GitHub Actions: pending hosted runs; no local verifiers or tests were run.
- Cold review: pending one independent Luna review.
- The four linked issues remain open with `EXTERNAL_ACTION_REQUIRED`; these repository
  fixes do not provide legal approval, release authority, staging provisioning, or
  real AWS KMS lifecycle evidence.
