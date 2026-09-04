#!/usr/bin/env python3
"""Capacity guard for heavy local/CI work without destructive housekeeping.

Build capacity is a correctness prerequisite: a compiler or package manager
that runs out of space can leave incomplete artifacts which later look like a
source failure.  This tool reports the evidence before a heavy gate starts and
refuses that gate below a deliberate floor.  It never removes anything unless
an operator supplies both a fixed cache *name* and an explicit execution
acknowledgement; ordinary cleanup is a dry-run.

Exit status:
  0  report/gate passed, or a safe dry-run was printed
  1  capacity is below the requested floor
  2  evidence/arguments are unsafe or unavailable (do not claim a green gate)
"""

from __future__ import annotations

import argparse
import contextlib
import dataclasses
import fcntl
import math
import os
import shutil
import stat
import subprocess
import sys
import time
from pathlib import Path
from typing import Iterator


DEFAULT_FLOOR_MIB = 5 * 1024
"""Five GiB: techlead L10's documented critical floor for heavy Rust gates."""
DEFAULT_MIN_FREE_INODES = 1
MAX_LOCK_TIMEOUT_SECONDS = 24 * 60 * 60
MAX_CACHE_SCAN_ENTRIES = 100_000
MAX_CACHE_REPORT_BYTES = 1 << 40  # Keep inventory bounded at 1 TiB per cache.
GIT_COMMAND_TIMEOUT_SECONDS = 5
MAX_REPORT_SECONDS = 90
MAX_IGNORED_PATHS = 100_000

# These names are deliberately not paths.  Accepting arbitrary paths, globs,
# environment expansion or ``..`` here would turn a diagnostic into a delete
# primitive.  All are regenerable build/dependency caches directly below a
# checked-out worktree; source, .git, Docker, and any shared HOME cache are out.
REGENERABLE_WORKTREE_CACHES = ("target", "node_modules", ".turbo", ".pnpm-store")


class GuardError(RuntimeError):
    """The guard cannot make a safe claim about a requested operation."""


@dataclasses.dataclass(frozen=True)
class FilesystemCapacity:
    free_bytes: int
    free_inodes: int


@dataclasses.dataclass(frozen=True)
class WorktreeReport:
    path: Path
    branch: str | None
    states: tuple[str, ...]
    caches: tuple[tuple[str, int], ...]


def mib(value: int) -> int:
    return value // (1024 * 1024)


def below_floor(capacity: FilesystemCapacity, floor_mib: int, min_free_inodes: int) -> bool:
    return (
        capacity.free_bytes < floor_mib * 1024 * 1024
        or capacity.free_inodes < min_free_inodes
    )


def parse_floor(value: str) -> int:
    try:
        parsed = int(value)
    except ValueError as error:
        raise argparse.ArgumentTypeError("floor must be an integer MiB") from error
    if not 1 <= parsed <= 1_048_576:
        raise argparse.ArgumentTypeError("floor must be between 1 MiB and 1 TiB")
    return parsed


def parse_nonnegative_int(value: str) -> int:
    try:
        parsed = int(value)
    except ValueError as error:
        raise argparse.ArgumentTypeError("value must be an integer") from error
    if parsed < 0:
        raise argparse.ArgumentTypeError("value must be non-negative")
    return parsed


def parse_timeout(value: str) -> float:
    try:
        parsed = float(value)
    except ValueError as error:
        raise argparse.ArgumentTypeError("lock timeout must be a number of seconds") from error
    if not math.isfinite(parsed) or not 0 <= parsed <= MAX_LOCK_TIMEOUT_SECONDS:
        raise argparse.ArgumentTypeError(
            f"lock timeout must be finite and between 0 and {MAX_LOCK_TIMEOUT_SECONDS} seconds"
        )
    return parsed


def git(root: Path, *args: str) -> str:
    try:
        completed = subprocess.run(
            ["git", "-C", os.fspath(root), *args],
            text=True,
            capture_output=True,
            check=False,
            timeout=GIT_COMMAND_TIMEOUT_SECONDS,
            errors="surrogateescape",
        )
    except subprocess.TimeoutExpired as error:
        raise GuardError(f"git command timed out after {GIT_COMMAND_TIMEOUT_SECONDS}s") from error
    if completed.returncode:
        detail = completed.stderr.strip().splitlines()
        raise GuardError(detail[0] if detail else "git command failed")
    return completed.stdout


def validated_root(raw_root: str) -> Path:
    """Return an existing physical git worktree root, never a broad/symlink root."""
    try:
        supplied = Path(raw_root).expanduser()
    except (OSError, ValueError) as error:
        raise GuardError(f"root cannot be resolved: {error}") from error
    if any(char in raw_root for char in "*?[]"):
        raise GuardError("root must be a literal worktree path, never a glob")
    if not supplied.is_absolute():
        supplied = Path.cwd() / supplied
    if supplied == Path("/") or supplied.is_symlink():
        raise GuardError("root must be a non-symlink git worktree, never filesystem root")
    try:
        physical = supplied.resolve(strict=True)
    except (OSError, ValueError) as error:
        raise GuardError(f"cannot resolve root: {error}") from error
    if physical == Path("/"):
        raise GuardError("root must not resolve to filesystem root")
    try:
        top = Path(git(physical, "rev-parse", "--show-toplevel").strip()).resolve(strict=True)
    except (GuardError, OSError) as error:
        raise GuardError(f"root is not a usable git worktree: {error}") from error
    if physical != top:
        raise GuardError("root must name the worktree root exactly, not a parent or subdirectory")
    return physical


def filesystem_capacity(path: Path) -> FilesystemCapacity:
    stats = os.statvfs(path)
    free_inodes = stats.f_favail if stats.f_favail >= 0 else -1
    return FilesystemCapacity(stats.f_bavail * stats.f_frsize, free_inodes)


def directory_size(path: Path) -> int:
    """Size regular files below a cache, with an explicit read-only scan bound.

    A cache can contain millions of files. Bounded traversal keeps this guard
    observable on a nearly-full disk; reaching a bound is indeterminate rather
    than an inaccurate size claim.
    """
    try:
        metadata = path.lstat()
        if stat.S_ISLNK(metadata.st_mode) or not stat.S_ISDIR(metadata.st_mode):
            return 0
    except FileNotFoundError:
        return 0
    except OSError as error:
        raise GuardError(f"cannot read cache metadata: {path}: {error}") from error
    total = 0
    scanned = 0
    pending = [path]
    while pending and scanned < MAX_CACHE_SCAN_ENTRIES and total < MAX_CACHE_REPORT_BYTES:
        directory = pending.pop()
        try:
            entries = os.scandir(directory)
        except OSError as error:
            raise GuardError(f"cannot read cache directory: {directory}: {error}") from error
        with entries:
            for entry in entries:
                scanned += 1
                if scanned > MAX_CACHE_SCAN_ENTRIES:
                    raise GuardError(
                        f"cache scan exceeded {MAX_CACHE_SCAN_ENTRIES} entries: {path}"
                    )
                try:
                    if entry.is_symlink():
                        continue
                    if entry.is_dir(follow_symlinks=False):
                        pending.append(Path(entry.path))
                    elif entry.is_file(follow_symlinks=False):
                        total = min(total + entry.stat(follow_symlinks=False).st_size,
                                    MAX_CACHE_REPORT_BYTES)
                except OSError as error:
                    raise GuardError(f"cannot read cache entry: {entry.path}: {error}") from error
                if total >= MAX_CACHE_REPORT_BYTES:
                    raise GuardError(
                        f"cache size exceeded {MAX_CACHE_REPORT_BYTES} bytes: {path}"
                    )
    if pending:
        raise GuardError(f"cache scan exceeded {MAX_CACHE_SCAN_ENTRIES} entries: {path}")
    return total


def nul_paths(output: str, *, limit: int = MAX_IGNORED_PATHS) -> list[str]:
    """Parse a NUL-delimited git path stream without newline ambiguity."""
    if not output:
        return []
    if not output.endswith("\0"):
        raise GuardError("git returned a malformed NUL-delimited path list")
    paths = output[:-1].split("\0")
    if len(paths) > limit:
        raise GuardError(f"git path evidence exceeded {limit} entries")
    return paths


def tracked_cache_paths(root: Path, target: str) -> list[str]:
    """Return tracked index paths under one fixed cache name, NUL-safe."""
    output = git(root, "ls-files", "-z", "--", target, f"{target}/")
    return nul_paths(output)


def ignored_non_cache_paths(root: Path) -> list[str]:
    """Return ignored paths outside the four explicitly disposable caches."""
    output = git(root, "ls-files", "--others", "--ignored", "--exclude-standard", "-z")
    paths = nul_paths(output)
    return [path for path in paths if path.split("/", 1)[0] not in REGENERABLE_WORKTREE_CACHES]


def parse_worktree_list(root: Path) -> list[tuple[Path, str | None]]:
    records: list[tuple[Path, str | None]] = []
    path: Path | None = None
    branch: str | None = None
    for line in git(root, "worktree", "list", "--porcelain").splitlines() + [""]:
        if line.startswith("worktree "):
            path = Path(line.removeprefix("worktree "))
            if not path.is_absolute():
                raise GuardError("git reported a non-absolute worktree path")
            branch = None
        elif line.startswith("branch "):
            branch = line.removeprefix("branch refs/heads/")
        elif not line and path is not None:
            records.append((path, branch))
            path = None
            branch = None
    if not records:
        raise GuardError("git reported no worktrees")
    return records


def unsafe_branch(root: Path, branch: str | None) -> bool:
    """Prove a checked-out branch is safe only against the production ref."""
    # Detached worktrees have no branch ref to prove stale.  They may contain
    # an operator's live checkout, so cleanup must protect them by default.
    if branch is None:
        return True
    if branch in {"main", "master"}:
        try:
            local = git(root, "rev-parse", "--verify", branch).strip()
            remote = git(root, "rev-parse", "--verify", "origin/main").strip()
        except GuardError as error:
            raise GuardError(f"cannot prove {branch} matches origin/main: {error}") from error
        return local != remote
    try:
        completed = subprocess.run(
            ["git", "-C", os.fspath(root), "merge-base", "--is-ancestor", branch, "origin/main"],
            capture_output=True,
            check=False,
            timeout=GIT_COMMAND_TIMEOUT_SECONDS,
        )
    except subprocess.TimeoutExpired as error:
        raise GuardError(f"git command timed out after {GIT_COMMAND_TIMEOUT_SECONDS}s") from error
    if completed.returncode == 0:
        return False
    if completed.returncode == 1:
        return True
    raise GuardError("cannot determine whether worktree branch is still active")


def worktree_states(root: Path, branch: str | None) -> tuple[str, ...]:
    porcelain = git(root, "status", "--porcelain=v1", "--untracked-files=all")
    tracked_dirty = False
    untracked = False
    for line in porcelain.splitlines():
        if line.startswith("?? "):
            untracked = True
        elif line[:2] != "  ":
            tracked_dirty = True
    ignored = bool(ignored_non_cache_paths(root))
    states: list[str] = []
    if unsafe_branch(root, branch):
        states.append("active")
    if tracked_dirty:
        states.append("dirty")
    if untracked:
        states.append("untracked")
    if ignored:
        states.append("ignored")
    return tuple(states or ["clean"])


def inspect_worktree(path: Path, branch: str | None) -> WorktreeReport:
    # A missing or symlinked listed path is evidence unavailable.  Never follow
    # it to find a cache or to infer a safe candidate.
    if path.is_symlink() or not path.is_dir():
        return WorktreeReport(path, branch, ("unavailable",), ())
    physical = path.resolve()
    try:
        states = worktree_states(physical, branch)
    except GuardError:
        states = ("unavailable",)
    # Never spend unbounded time measuring a live/dirty checkout: its cache is
    # categorically ineligible for cleanup.  A zero size here means
    # "intentionally not measured", while clean candidates receive the bounded
    # byte inventory below.
    if set(states) - {"clean"}:
        return WorktreeReport(physical, branch, states, ())
    caches = tuple(
        (name, directory_size(physical / name))
        for name in REGENERABLE_WORKTREE_CACHES
        if (physical / name).exists() and not (physical / name).is_symlink()
    )
    return WorktreeReport(physical, branch, states, caches)


def collect_report(root: Path) -> list[WorktreeReport]:
    reports: list[WorktreeReport] = []
    deadline = time.monotonic() + MAX_REPORT_SECONDS
    for path, branch in parse_worktree_list(root):
        if time.monotonic() >= deadline:
            raise GuardError(f"worktree report exceeded {MAX_REPORT_SECONDS}s")
        reports.append(inspect_worktree(path, branch))
    return reports


def print_report(root: Path, floor_mib: int) -> FilesystemCapacity:
    capacity = filesystem_capacity(root)
    print(f"capacity root: {root}")
    print(f"free: {mib(capacity.free_bytes)} MiB; free inodes: {capacity.free_inodes}")
    print(f"heavy-gate floor: {floor_mib} MiB (configurable CORELINK_CAPACITY_FLOOR_MIB/--floor-mib)")
    reports = collect_report(root)
    candidates = {"active": 0, "dirty": 0, "untracked": 0, "clean": 0, "unavailable": 0}
    cache_rows: list[tuple[int, str, str, tuple[str, ...]]] = []
    for report in reports:
        for state in report.states:
            candidates[state] = candidates.get(state, 0) + 1
        for name, size in report.caches:
            cache_rows.append((size, os.fspath(report.path), name, report.states))
    print("worktrees: " + ", ".join(f"{name}={count}" for name, count in candidates.items()))
    if cache_rows:
        print("largest regenerable worktree caches (report only; no shared HOME/Docker targets):")
        for size, path, name, states in sorted(cache_rows, reverse=True)[:10]:
            print(f"  {mib(size):>7} MiB  {path}/{name}  [{','.join(states)}]")
    else:
        print("largest regenerable worktree caches: none found")
    unavailable = [os.fspath(report.path) for report in reports if "unavailable" in report.states]
    if unavailable:
        raise GuardError("worktree evidence unavailable: " + ", ".join(unavailable))
    return capacity


def cleanup_path(root: Path, target: str) -> Path:
    root = validated_root(os.fspath(root))
    if target not in REGENERABLE_WORKTREE_CACHES:
        raise GuardError("cleanup target must be one fixed regenerable cache name: "
                         + ", ".join(REGENERABLE_WORKTREE_CACHES))
    tracked = tracked_cache_paths(root, target)
    if tracked:
        raise GuardError("refusing cleanup: tracked files under cache " + ", ".join(tracked[:5]))
    ignored = ignored_non_cache_paths(root)
    if ignored:
        raise GuardError("refusing cleanup with ignored data outside disposable caches: "
                         + ", ".join(ignored[:5]))
    states = worktree_states(root, git(root, "branch", "--show-current").strip() or None)
    unsafe = set(states) - {"clean"}
    if unsafe:
        raise GuardError("refusing cleanup in " + ",".join(sorted(unsafe))
                         + " worktree; no source/dirty/live candidate is disposable")
    target_path = root / target
    try:
        target_path.lstat()
    except FileNotFoundError:
        return target_path
    if target_path.is_symlink() or not target_path.is_dir():
        raise GuardError("cleanup target must be a direct non-symlink cache directory")
    # Parent and lexical relation protect against a future target-list mistake.
    if target_path.parent != root or target_path.resolve().parent != root:
        raise GuardError("cleanup target escaped the validated worktree")
    return target_path


def remove_cache_safely(root: Path, path: Path) -> None:
    """Remove one validated cache using a directory descriptor, or refuse.

    Python's fd-based rmtree rejects symlink traversal on supported POSIX
    platforms. The parent descriptor also keeps the operation rooted at the
    exact validated worktree instead of a re-resolved path.
    """
    if not getattr(shutil.rmtree, "avoids_symlink_attacks", False):
        raise GuardError("refusing deletion: fd-safe symlink-resistant rmtree unavailable")
    flags = (getattr(os, "O_RDONLY", 0) | getattr(os, "O_DIRECTORY", 0)
             | getattr(os, "O_NOFOLLOW", 0) | getattr(os, "O_CLOEXEC", 0))
    if not getattr(os, "O_DIRECTORY", 0) or not getattr(os, "O_NOFOLLOW", 0):
        raise GuardError("refusing deletion: directory no-follow descriptors unavailable")
    try:
        parent_fd = os.open(os.fspath(root), flags)
    except OSError as error:
        raise GuardError(f"cannot open validated worktree descriptor: {error}") from error
    try:
        try:
            target_fd = os.open(path.name, flags, dir_fd=parent_fd)
        except FileNotFoundError:
            return
        except OSError as error:
            raise GuardError(f"cannot open cache descriptor without following symlinks: {error}") from error
        try:
            before = os.fstat(target_fd)
            if not stat.S_ISDIR(before.st_mode):
                raise GuardError("cleanup target must remain a directory")
            current = os.stat(path.name, dir_fd=parent_fd, follow_symlinks=False)
            if (before.st_dev, before.st_ino) != (current.st_dev, current.st_ino):
                raise GuardError("cache changed during deletion validation")
            try:
                shutil.rmtree(path.name, dir_fd=parent_fd)
            except (FileNotFoundError, NotADirectoryError):
                # A concurrent remover is an idempotent success only when the
                # entry is gone; a symlink or replacement is refusal below.
                pass
            try:
                os.stat(path.name, dir_fd=parent_fd, follow_symlinks=False)
            except FileNotFoundError:
                return
            raise GuardError("cache changed during deletion; refusing replacement")
        finally:
            os.close(target_fd)
    finally:
        os.close(parent_fd)


def dry_run_cleanup(root: Path, targets: list[str], execute: bool) -> None:
    if not targets:
        raise GuardError("--cleanup requires at least one explicit --cleanup-target")
    paths = []
    seen: set[Path] = set()
    for target in targets:
        path = cleanup_path(root, target)
        if path not in seen:
            paths.append(path)
            seen.add(path)
    for path in paths:
        print(f"cleanup {'EXECUTE' if execute else 'DRY-RUN'}: {path} ({mib(directory_size(path))} MiB)")
    if not execute:
        print("no files removed: repeat with --execute and "
              "CORELINK_CAPACITY_ALLOW_DELETE=delete-regenerable-cache only after review")
        return
    if os.environ.get("CORELINK_CAPACITY_ALLOW_DELETE") != "delete-regenerable-cache":
        raise GuardError("execution requires CORELINK_CAPACITY_ALLOW_DELETE=delete-regenerable-cache")
    root = validated_root(os.fspath(root))
    with materialization_lock(root, 0):
        for path in paths:
            # Revalidate immediately before descriptor-based deletion: a cache
            # swapped for a symlink or tracked path is refused, never followed.
            validated = cleanup_path(root, path.name)
            remove_cache_safely(root, validated)
    print("explicit regenerable caches removed; no worktree, branch, Docker, volume, or shared HOME cache was touched")


@contextlib.contextmanager
def materialization_lock(root: Path, timeout_seconds: float) -> Iterator[None]:
    """A shared git-common-dir advisory lock for package/cache materialization."""
    if not math.isfinite(timeout_seconds) or not 0 <= timeout_seconds <= MAX_LOCK_TIMEOUT_SECONDS:
        raise GuardError("lock timeout must be finite and bounded to 24 hours")
    root = validated_root(os.fspath(root))
    common = Path(git(root, "rev-parse", "--git-common-dir").strip())
    if not common.is_absolute():
        common = (root / common).resolve()
    try:
        common = common.resolve(strict=True)
    except OSError as error:
        raise GuardError(f"git common directory unavailable: {error}") from error
    if not common.is_dir():
        raise GuardError("git common directory is not a directory")
    lock_path = common / "corelink-capacity-materialization.lock"
    with lock_path.open("a+", encoding="utf-8") as lock:
        deadline = time.monotonic() + timeout_seconds
        while True:
            try:
                fcntl.flock(lock.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
                break
            except BlockingIOError:
                if time.monotonic() >= deadline:
                    raise GuardError("materialization lock is held by another worktree; retry after it finishes")
                time.sleep(0.05)
        try:
            yield
        finally:
            fcntl.flock(lock.fileno(), fcntl.LOCK_UN)


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", default=".", help="literal git worktree root (default: .)")
    parser.add_argument("--floor-mib", type=parse_floor,
                        default=parse_floor(os.environ.get("CORELINK_CAPACITY_FLOOR_MIB", str(DEFAULT_FLOOR_MIB))))
    parser.add_argument("--gate", action="store_true", help="exit 1 below floor; default is diagnostic only")
    parser.add_argument("--cleanup", action="store_true", help="explicit cache cleanup (dry-run unless --execute)")
    parser.add_argument("--cleanup-target", action="append", default=[], metavar="NAME",
                        help="one of target, node_modules, .turbo, .pnpm-store; repeatable")
    parser.add_argument("--execute", action="store_true", help="perform reviewed explicit cleanup (requires acknowledgement env)")
    parser.add_argument("--lock", action="store_true", help="hold shared materialization lock around command after --")
    parser.add_argument("--lock-timeout-seconds", type=parse_timeout, default=0.0,
                        help="lock wait bound (default: fail immediately)")
    parser.add_argument("--min-free-inodes", type=parse_nonnegative_int,
                        default=DEFAULT_MIN_FREE_INODES,
                        help="minimum available inodes required by --gate (default: 1)")
    parser.add_argument("command", nargs=argparse.REMAINDER, help="command to run while --lock is held")
    args = parser.parse_args(argv)
    if args.execute and not args.cleanup:
        parser.error("--execute requires --cleanup")
    if args.cleanup and args.lock:
        parser.error("--cleanup and --lock are separate operations")
    if args.lock:
        if not args.command or args.command[0] != "--" or len(args.command) == 1:
            parser.error("--lock requires a command after --")
        args.command = args.command[1:]
    elif args.command:
        parser.error("a command is valid only with --lock")
    return args


def main(argv: list[str] | None = None) -> int:
    try:
        args = parse_args(list(argv if argv is not None else sys.argv[1:]))
        root = validated_root(args.root)
        capacity = print_report(root, args.floor_mib)
        if args.cleanup:
            dry_run_cleanup(root, args.cleanup_target, args.execute)
        if args.gate and below_floor(capacity, args.floor_mib, args.min_free_inodes):
            print(f"CAPACITY: only {mib(capacity.free_bytes)} MiB free; heavy gate needs "
                  f"{args.floor_mib} MiB and {args.min_free_inodes} free inode(s); "
                  "inspect the report, serialize materialization with --lock, then use "
                  "reviewed explicit cache cleanup if appropriate.", file=sys.stderr)
            return 1
        if args.lock:
            with materialization_lock(root, args.lock_timeout_seconds):
                # Recheck after acquiring the shared lock: another materializer
                # may have consumed the reported headroom while we waited.
                if args.gate:
                    locked_capacity = filesystem_capacity(root)
                    if below_floor(locked_capacity, args.floor_mib, args.min_free_inodes):
                        print(
                            f"CAPACITY: only {mib(locked_capacity.free_bytes)} MiB free after lock; "
                            f"heavy gate needs {args.floor_mib} MiB and {args.min_free_inodes} "
                            "free inode(s); command not started.",
                            file=sys.stderr,
                        )
                        return 1
                completed = subprocess.run(args.command, cwd=root, check=False)
                return completed.returncode
        return 0
    except (GuardError, argparse.ArgumentTypeError) as error:
        print(f"CAPACITY INDETERMINATE: {error}", file=sys.stderr)
        return 2
    except OSError as error:
        print(f"CAPACITY INDETERMINATE: OS evidence unavailable: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
