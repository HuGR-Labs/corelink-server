# `main` readback — 2026-09-22 (`122515e8`)

Observed remote tip after a fresh `git fetch origin main`:
`122515e808158c55b7bff8ecda6b1f760a9439c9`.

Delta from the prior readback `41c89d72` is 11 files (`+1080/-24`), limited to
Buck2/CLI release workflows and verification scripts, `BACKLOG.md`, `CLAUDE.md`,
one owner-action evidence JSON and an example benchmark script. No Cargo
manifest, lockfile, five-pilot source, migration or worker source changed in
this delta. The new/changed backlog and release-governance surfaces keep the
deduplication/backlog gate stale until their contents are re-read; CI and
verification scripts remain SOURCE/UNKNOWN without execution.

This is source-drift evidence only. It does not alter the 105-package census,
approve artifacts, freeze the standard or authorize publication.
