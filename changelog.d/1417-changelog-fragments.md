### Changed

- **`CHANGELOG.md` is no longer edited by PRs — they add a `changelog.d/`
  fragment instead.** One shared file serializes every branch by construction:
  on 2026-08-29, 10 of the 13 open PRs (#1389 #1393 #1395 #1396 #1398 #1399
  #1400 #1402 #1410 #1412) carried a `CHANGELOG.md` edit, so each merge
  invalidated the next branch's context and the queue could only drain one PR
  at a time regardless of how much work was in flight. A PR now writes
  `changelog.d/<pr-number>-<slug>.md`; two new files cannot conflict, so the
  serialization disappears rather than being managed.
  `scripts/assemble_changelog.py` groups the fragments by section, splices them
  into `## [Unreleased]` in deterministic order and deletes the ones it
  consumed — with zero fragments present it writes nothing, leaving
  `CHANGELOG.md` byte-identical. The `changelog-validate` gate accepts
  **either** form for `feat:`/`fix:` PRs, a fragment or the legacy
  `[Unreleased]` entry; dual acceptance is deliberate and stays until the
  in-flight queue drains, because a fragment-only gate would turn all ten
  in-flight PRs red on the day it landed.
