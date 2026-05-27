---
id: "AUDIT-2026-05-27-SPECS-WAVE-A-SEAL"
type: "audit"
doc_status: "ACTIVE"
audit_status: "SEALED"
version: "1.0.0"
created: "2026-05-27"
updated: "2026-05-27"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["audit", "specs-cleanup", "wave-a", "archival", "seal"]
references:
  - "specs/_audits/2026-05-27-specs-inventory-cleanup-map.md"
---

# Specs cleanup Wave A — archival SEAL

## §1 Scope

Wave A executed the mechanical archival prescribed by the 2026-05-27 specs inventory
cleanup map (`specs/_audits/2026-05-27-specs-inventory-cleanup-map.md`). 514 SEALED
files were relocated to `_sealed/` subdirectories. Zero spec body content was modified;
the only edits outside the move operation were hard-coded path references in tracked
files, rewritten string-for-string to point at the new locations.

## §2 Results — files moved

| Group                     | Count | Source                                                                 | Destination                                                |
|---------------------------|------:|------------------------------------------------------------------------|------------------------------------------------------------|
| SEALED-HISTORICAL         |   365 | `specs/_audits/*.md` + `specs/_pentest/*.md` + `specs/_postmortems/*.md` | `specs/_audits/sealed/` (with `pentest/` and `postmortems/` subdirs for the 6 files originating from `_pentest/` and `_postmortems/`) |
| SPRINT-WI-SEALED          |    93 | `specs/04_sprints/S<NN>/work_items/WI-*.md` (14 sprints)               | `specs/04_sprints/_sealed/S<NN>/work_items/`               |
| SPRINT-CONTRACT-SEALED    |    56 | `specs/04_sprints/S<NN>/<sprint.md|_spec_contract.md|PRR-*.md|...>.md` | `specs/04_sprints/_sealed/S<NN>/`                          |
| **Total**                 | **514** |                                                                       |                                                            |

Filenames preserved verbatim throughout. Sprint numbers preserved for the 149 files
in groups 2+3.

Sprints affected by groups 2+3: S01, S03, S04, S05, S06, S07, S09, S12, S15, S16,
S17, S18, S19, S20.

## §3 Reference updates

A string-replace rewrite pass updated 710 tracked files (2,746 substitutions; equal
insertions/deletions confirms pure path swap, no semantic drift). File-type breakdown:
markdown specs, narrative docs (CLAUDE.md, README.md, ROADMAP-TO-GA.md, CHANGELOG.md,
RELEASE-NOTES, CONTRIBUTING.md), `Cargo.toml` comment blocks, GitHub Actions YAML,
`scripts/*.sh` and `scripts/*.py`, source-tree `*.rs` documentation comments, and JSON
report fixtures.

Substitution rules (deterministic 1-to-1 mapping):

| Old path pattern                                  | New path pattern                                           |
|---------------------------------------------------|------------------------------------------------------------|
| `specs/_audits/<2026-X>.md`                       | `specs/_audits/sealed/<2026-X>.md`                         |
| `specs/04_sprints/S<NN>/<X>.md`                   | `specs/04_sprints/_sealed/S<NN>/<X>.md`                    |
| `specs/_pentest/<X>.md`                           | `specs/_audits/sealed/pentest/<X>.md`                      |
| `specs/_postmortems/<X>.md`                       | `specs/_audits/sealed/postmortems/<X>.md`                  |

Intentionally NOT rewritten:
- `specs/_audits/2026-05-27-specs-inventory-cleanup-map.md` itself — it documents the
  pre-Wave-A corpus state as a historical snapshot; its 486 internal path references
  are part of that snapshot.

No `references:` frontmatter array required ambiguous resolution; every cross-link
mapped deterministically via the rules above.

Validator scripts (`scripts/validate_specs.py`, `scripts/validate_references.py`) did
NOT require source modifications. Both use `Path.rglob("*.md")`, which naturally
discovers files under `_sealed/` subdirs. `validate_specs.py`'s existing `SKIP_ALL`
rule for `_audits/` correctly continues to skip everything under `_audits/sealed/` as
well (semantically appropriate — sealed audit history should not be schema-validated).

## §4 Validation

| Check                              | Pre-Wave-A baseline                         | Post-Wave-A          | Delta                           |
|------------------------------------|---------------------------------------------|----------------------|---------------------------------|
| `validate_specs.py` total scanned  | 465 (`455 schema + 10 yaml-only`)*          | 458 (`449 + 9`)      | −6 ; intentional                |
| `validate_specs.py` failures       | 0                                           | 0                    | unchanged GREEN                 |
| `validate_references.py` analyzed  | 520 docs                                    | 514 docs             | −6 ; intentional                |
| `validate_references.py` dangling  | 0                                           | 0                    | unchanged GREEN                 |
| Conflict markers (`<<<<<<<` etc.)  | 0                                           | 0                    | unchanged                       |
| `cargo metadata --no-deps`         | OK                                          | OK                   | unchanged                       |

*Pre-baseline was measured by re-checking out commit `4c5e36e7` (the merge that landed
the inventory map) in a temp worktree and running `python3 scripts/validate_specs.py --verbose`.

Explanation of the −6 delta: the 6 files originating from `specs/_pentest/` and
`specs/_postmortems/` were previously visible to `validate_specs.py` (those dirs were
NOT in `SKIP_ALL`). The inventory map prescribed moving them under
`specs/_audits/sealed/pentest/` and `specs/_audits/sealed/postmortems/`, which places
them under the `_audits/` skip rule. Semantically correct — they are sealed audit
artifacts now — and intentional. Same explanation applies to the −6 delta on
`validate_references.py`. No content errors were masked: a manual stale-ref grep across
the entire tracked tree post-rewrite shows ZERO references to any moved old path
(outside the deliberately preserved inventory map).

## §5 Commits

| SHA       | Subject                                                                     |
|-----------|-----------------------------------------------------------------------------|
| `689e5960`| chore(specs): archive 365 SEALED-HISTORICAL audits to specs/_audits/sealed/ |
| `942208d9`| chore(specs): fix mispathed pentest+postmortem files under sealed/          |
| `ad13e2ea`| chore(specs): archive 93 SPRINT-WI-SEALED work items to specs/04_sprints/_sealed/ |
| `282a41f0`| chore(specs): archive 56 SPRINT-CONTRACT-SEALED docs to specs/04_sprints/_sealed/ |
| `3dd43b5a`| chore(specs): rewrite 2,746 hard-coded references to point at _sealed/ paths |
| (this doc)| docs(seal): Wave A archival audit                                            |

Note on commit `942208d9`: the initial archival commit `689e5960` had a prefix-stripping
bug in the move script — 6 files originating from `specs/_pentest/` and
`specs/_postmortems/` had `'specs/_audits/'` literally stripped from their source paths
(yielding garbage substrings like `tems/PM-...md`) instead of being mapped to the
prescribed `_audits/sealed/{pentest,postmortems}/` destinations. A follow-up commit
(`942208d9`) relocated those 6 files to the correct subdirs via `git mv` and the
rewrite pass picked up the corrected mapping. No --amend was used (charter
constraint); the fix lives in its own commit for review-ability.

## §6 Charter compliance

- [x] No spec body content modified; moves only via `git mv`.
- [x] No validator LOGIC changed (only verified the existing logic + rglob behavior).
- [x] The 376 non-archived files were NOT moved. The 393 of them that contained
      hard-coded paths to moved files had ONLY those paths string-replaced; no other
      edits.
- [x] No `--no-verify` used. All hooks ran on every commit.
- [x] Three primary commits + one fix-up + one rewrite commit + this seal — five
      commits total before the seal, each independently reviewable.
- [x] No frontmatter `references:` array required ambiguous resolution; every
      cross-link mapped 1-to-1 via the rules in §3.
- [x] Disk headroom verified before start (36 GiB free; threshold 3 GiB).

## §A DCO sign-off

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>.
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>.
