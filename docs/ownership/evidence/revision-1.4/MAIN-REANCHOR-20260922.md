# Reanchor delta — campaign branch versus current `main`

Observed remote `main`: `f5c963f22d2c8f5a332f6f0e970b397da09adebd`
(`test(dsr): assert DLQ tenant privacy and routing (#2078)`). Campaign branch
head before this record: `a7429e35408f5ad137ac58df70aca40d17e657fa`.

## Findings

- The campaign branch and current `main` share the historical campaign baseline
  but are materially diverged. The campaign branch is 557 commits ahead and 317
  behind the current remote tip.
- Current `main` does not contain the v1.4 ownership standard, registry, tools,
  schemas, issue drafts, package skills, or pilot artifact set. It contains a
  separate preparation bundle under `docs/ownership/preparation/2026-09-19/`.
- Tracked Cargo population remains 107 manifests: one virtual root, 95 workspace
  packages, 10 independent fuzz packages, and one historical archive. No package
  identity add/remove/rename was found; nine manifest contents changed and require
  metadata refresh.
- The `corelink-server` pilot source tree has material changes on current `main`.
  Its pinned readback at `140e16eab6315bfdec1a0e4a9892d8557a071781` is therefore
  historical source evidence, not current-main proof. The hash, cf-bindings, and
  e2e pilot source trees were unchanged in the compared range; billing had a test
  source change.

## Consequence

Do not merge or rebase this 557/317-diverged campaign branch wholesale. The
ownership artifacts remain unmerged campaign work and require a targeted
current-main reconciliation before integration, freeze, or issue publication.

## Subsequent readback

`origin/main` subsequently advanced to
`4d5d191cfd09e1a9c78b0f7e2ac672317dd41c44` (`fix(cli): probe cargo-zigbuild
directly (#2080)`) and then to
`0389714d9f5408f744e17227b82d795fff245a32` (`fix(audit): retain sanitized B-125
provider diagnostics (#2083)`). Direct diffs of the Cargo root and all five
pilot source trees across these readbacks found no changes. The latest pin is
the current observed main; the substantive reanchor findings above remain valid.
