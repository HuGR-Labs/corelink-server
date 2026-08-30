# `changelog.d/` — changelog fragments

**Do not edit `CHANGELOG.md` in a PR. Add a file here instead.**

`CHANGELOG.md` is one shared file. On 2026-08-29, **10 of the 13 open PRs edited
it**, which serializes the whole merge queue by construction: every merge
invalidates the next branch's context. A new file per PR cannot conflict with
another new file, so the conflict disappears instead of being managed.

## How to add an entry

Create `changelog.d/<pr-number>-<short-slug>.md`:

```markdown
### Fixed

- **One-line lead in bold, naming the defect and its ID (B-0NN / WI-S__-___).**
  Then the prose: what was actually wrong, what the fix does, and the evidence
  you measured — same style as the entries already in `CHANGELOG.md`.
```

Rules (enforced by `scripts/assemble_changelog.py --lint`, run by the
`changelog-validate` workflow on every PR that adds a fragment):

- First non-blank line is exactly one of `### Added`, `### Changed`,
  `### Deprecated`, `### Removed`, `### Fixed`, `### Security`.
- The entry text follows and must be a Markdown list item (`- …`), non-empty.
- One fragment per PR is the norm; more than one is fine (a PR that both adds
  and fixes something writes two files).

If you do not know the PR number yet, open the PR first and then add the
fragment — the number is what keeps filenames unique across concurrent
branches.

## Release time

```
python3 scripts/assemble_changelog.py            # splice + delete fragments
python3 scripts/assemble_changelog.py --dry-run  # preview only
python3 scripts/assemble_changelog.py --check    # exit 1 if any fragment is unassembled
```

The assembler groups fragments by section, orders them deterministically (by
filename within each section, Keep-A-Changelog order across sections), inserts
them at the top of the `## [Unreleased]` block — where every existing entry was
added — and deletes the fragments it consumed. With zero fragments present it
writes nothing, so `CHANGELOG.md` is left byte-identical.

## Transition

The `changelog-validate` gate accepts **either** form for `feat:`/`fix:` PRs: a
new fragment here, **or** a legacy `## [Unreleased]` entry in `CHANGELOG.md`.
Dual acceptance stays until the in-flight queue drains, so no open PR breaks.
New PRs should use fragments.
