# Capacity guard for heavy local and CI gates

`scripts/capacity_guard.py` treats build capacity as correctness evidence. A
full disk or inode table can produce partial artifacts, misleading compiler
errors, and a false conclusion about the change under test. It therefore
measures available bytes and inodes before a heavy gate and inventories the
largest **worktree-local, regenerable** caches.

The default heavy-gate floor is **5 GiB** (`5120 MiB`). This is not a promise
that a complete workspace build fits in 5 GiB; it is the documented critical
floor in the tech-lead maintenance checklist, below which Rust/Node materialize
work has already failed. Configure a higher floor for a known larger operation:

```bash
CORELINK_CAPACITY_FLOOR_MIB=12288 python3 scripts/capacity_guard.py --gate
python3 scripts/capacity_guard.py --gate --floor-mib 8192
```

Exit codes are deliberate: `0` means the report was obtained (and, with
`--gate`, the byte and inode floors held); `1` means capacity is below the
requested floor; `2` means arguments or evidence were unsafe/unavailable.
`--gate` requires at least one available inode by default; use
`--min-free-inodes N` for an explicit larger bound. Inode evidence is always
printed, but a filesystem which cannot report it is not silently interpreted as
infinite capacity.

## Parallel worktrees and shared caches

Each worktree retains its own `target/`, `node_modules/`, `.turbo/`, and
`.pnpm-store/`; those are the only paths the guard will describe as removable.
The report labels each worktree independently as `active` (unmerged or
detached branch), `dirty`, `untracked`, `ignored`, `clean`, or `unavailable`.
Active, dirty, untracked, or ignored worktrees are never cleanup candidates.
Cache byte totals are measured only for `clean` worktrees; live/dirty/untracked/ignored
worktrees are intentionally not traversed, so reporting cannot turn their
contents into a cleanup suggestion or an unbounded scan.
Any cache read error or bounded-scan overflow is indeterminate (exit `2`), not
reported as a zero-byte cache. `main` and `master` are trusted only when their
local commit exactly equals `origin/main`; other branches are stale candidates
only after Git proves ancestry into `origin/main`.

Cargo registries, `sccache`, package-manager stores under `$HOME`, Docker
images/volumes, branches, and worktrees may be shared or contain owner data.
They are intentionally **not** targets of this tool. The capacity report can
mention no cleanup recommendation for them; their lifecycle remains an explicit
operator decision.

Serialize only the write/materialization phase that shares a cache, not all
tests. The lock lives in git's common directory, so it coordinates separate
worktrees without relying on `/tmp`:

```bash
python3 scripts/capacity_guard.py --lock -- pnpm install --frozen-lockfile
python3 scripts/capacity_guard.py --lock -- cargo fetch
```

The lock file is created/opened with `O_NOFOLLOW`, exclusive creation, and
device/inode checks; a symlink or replacement lock path fails closed. The
command fails with exit `2` if another materialization holder exists (or
use a finite `--lock-timeout-seconds N`, bounded to 24 hours). The locked
command also has a hard, configurable `--command-timeout-seconds N` bound
(default 24 hours), so a hung Git/process cannot leave the guard waiting
forever. Compilation/test execution remains parallel after materialization.
Callers should invoke
`--gate` only for the heavy mode that needs the precondition; validators-only
work should not become red because of an unrelated capacity check.
The child command's non-zero status, including an `ENOSPC` failure, is
propagated and never turned into a green result.

## Cleanup is opt-in and narrow

Ordinary cleanup is a dry-run and requires each fixed cache name explicitly:

```bash
python3 scripts/capacity_guard.py --cleanup --cleanup-target target
```

It accepts only `target`, `node_modules`, `.turbo`, and `.pnpm-store`, direct
non-symlink directories below the exact current git worktree root. Before any
delete it proves, with a NUL-delimited `git ls-files` query, that the cache has
no tracked paths; ignored data outside those four exact cache roots also
refuses cleanup. Paths, globs, environment expansion, `..`, symlinks,
filesystem root, and worktree subdirectories are refused. Deletion requires a
no-follow directory descriptor rooted at the validated worktree. The validated
inode is first renamed to a private sibling quarantine; a rename/replacement
race is detected and restored/refused before any deletion. A
symlink-resistant `rmtree` then removes only that quarantined inode; if the
platform cannot provide the descriptor guarantee the guard refuses. Ignored
cache roots are excluded by Git at the source of the NUL-delimited query, so
large dependency trees are never captured as cleanup evidence. It never
automatically deletes code, untracked files, dirty
work, branches, a worktree, Docker, a Docker volume, or shared `$HOME` caches.

Actual removal additionally requires `--execute` and the exact acknowledgement
`CORELINK_CAPACITY_ALLOW_DELETE=delete-regenerable-cache`; inspect the dry-run
first. This two-step operation is for a human who has verified the target, not
for CI or an agent's automatic recovery loop.
