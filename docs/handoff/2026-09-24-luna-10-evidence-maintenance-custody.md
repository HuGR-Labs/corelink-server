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
worktree based on the remote main baseline above. The preserved source patch SHA-256 is
`5a8abb759e562beefda5f01309e8d24152eec3c18f39d198ab122e8ffff9a8e5`. The two B-098
count edits in that source patch were rejected after hosted evidence showed they were
incorrect; `main` already records the correct 488/499 population, so they have no net
delta in this PR. One focused regression test was added in isolation for B-083.

| Issue | File(s) | Acceptance criterion |
|---|---|---|
| #1657 | No file delta; `CLAUDE.md` and the B-098 evidence packet already match 488/499 on `main`. | Hosted B-098 receipt confirms main's 488 full-schema / 499 total count; retain the external release blocker. The dirty 489/500 edits are excluded as incorrect. |
| #1676 | `scripts/verify_b154_instrument_claims.py` | B-154 fail-closed verifier recognizes the current B-083 lifecycle receipt v2 shape while keeping the legal/capability claim unresolved. |
| #1700 | `scripts/verify_staging_target.py` | Staging duration verifier accepts the current bounded `DURATION=30s` declaration while retaining the 2h dispatch default check. |
| #2165 | `scripts/verify_b083_data_plane_wiring.py`; `tests/test_verify_b083_data_plane_wiring.py` | B-083 wiring verifier checks the real-provider feature where it is declared (`crates/corelink-container/Cargo.toml`) and keeps the runtime wiring assertions; a regression test proves the manifest feature is required. |

All other dirty and untracked paths in the shared checkout were excluded. The five
original diffs remain there unchanged. No worktree cleanup or shared-checkout cleanup
was performed.

## Work tracking

- Initial pull request: [#2403](https://github.com/HuGR-dev/corelink-server/pull/2403).
- Initial reviewed head: `2f12de257765678b0712b45c768e8f1893af1bad` (superseded by same-PR rework for a concrete cold-review finding).
- Initial hosted runs: B-098 `35953475267`, Python `35953475360`, Docs Reality `35953475279`, rustfmt `35953475317`, DCO `35953475288`, CodeQL `35953475292`; results are tracked on the PR for the current head.
- Review sequence: Luna returned `FIX-FIRST` with one consolidated finding. The same PR now parses the active TOML feature declaration and has a comment-only negative control. The bounded final review approved head `0f1bd0762549235790de761710c0463ec0dae8d2`. A later documentation-only commit corrected the B-098 matrix after GitHub's PR file list confirmed there is no B-098 file delta; no implementation changed after the reviewed head.
- No local verifiers or tests were run.
- Hosted B-098 runs `35953475267` and `35953726628` produced receipts reporting 488/499; run `35954256961` passed at the initial final head with the correct baseline records unchanged. The source patch's 489/500 values were wrong and are not part of the PR diff. Hosted Python runs `35953475360`, `35953726605`, and `35954153028` stopped before pytest on the existing WP-150 population drift (223 expected vs 224 found); no claim is made that the new regression test passed until a hosted run reaches it.
- The four linked issues remain open with `EXTERNAL_ACTION_REQUIRED`; these repository
  fixes do not provide legal approval, release authority, staging provisioning, or
  real AWS KMS lifecycle evidence.
