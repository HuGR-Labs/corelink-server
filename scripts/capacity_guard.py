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
import selectors
import secrets
import stat
import subprocess
import sys
import time
from pathlib import Path
from typing import Iterator

from process_bounds import (BoundedOutputError, ProcessBoundsError,
                             kill_process_group as _kill_process_group)
from process_bounds import run_bounded as _run_bounded


DEFAULT_FLOOR_MIB = 5 * 1024
"""Five GiB: techlead L10's documented critical floor for heavy Rust gates."""
DEFAULT_MIN_FREE_INODES = 1
MAX_LOCK_TIMEOUT_SECONDS = 24 * 60 * 60
MAX_CACHE_SCAN_ENTRIES = 100_000
MAX_CACHE_REPORT_BYTES = 1 << 40  # Keep inventory bounded at 1 TiB per cache.
GIT_COMMAND_TIMEOUT_SECONDS = 5
MAX_REPORT_SECONDS = 90
MAX_IGNORED_PATHS = 100_000
MAX_COMMAND_TIMEOUT_SECONDS = 24 * 60 * 60
MAX_GIT_OUTPUT_BYTES = 16 * 1024 * 1024
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


def parse_command_timeout(value: str) -> float:
    try:
        parsed = float(value)
    except ValueError as error:
        raise argparse.ArgumentTypeError("command timeout must be a number of seconds") from error
    if not math.isfinite(parsed) or not 0 < parsed <= MAX_COMMAND_TIMEOUT_SECONDS:
        raise argparse.ArgumentTypeError(
            f"command timeout must be finite and between 0 and {MAX_COMMAND_TIMEOUT_SECONDS} seconds"
        )
    return parsed


def git(root: Path, *args: str, deadline: float | None = None) -> str:
    timeout = float(GIT_COMMAND_TIMEOUT_SECONDS)
    if deadline is not None:
        timeout = min(timeout, deadline - time.monotonic())
        if timeout <= 0:
            raise GuardError(f"worktree report exceeded {MAX_REPORT_SECONDS}s")
    try:
        completed = _run_bounded(["git", "-C", os.fspath(root), *args], cwd=root,
                                 timeout=timeout, capture_output=True,
                                 max_output_bytes=MAX_GIT_OUTPUT_BYTES)
    except subprocess.TimeoutExpired as error:
        if deadline is not None and time.monotonic() >= deadline:
            raise GuardError(f"worktree report exceeded {MAX_REPORT_SECONDS}s") from error
        raise GuardError(f"git command timed out after {GIT_COMMAND_TIMEOUT_SECONDS}s") from error
    except BoundedOutputError as error:
        raise GuardError(str(error)) from error
    if completed.returncode:
        detail = completed.stderr.strip().splitlines()
        raise GuardError(detail[0] if detail else "git command failed")
    return completed.stdout


def git_nul_paths(root: Path, *args: str, limit: int = MAX_IGNORED_PATHS,
                   deadline: float | None = None) -> list[str]:
    """Stream a NUL-delimited Git path list with byte and entry bounds."""
    timeout = float(GIT_COMMAND_TIMEOUT_SECONDS)
    if deadline is not None:
        timeout = min(timeout, deadline - time.monotonic())
        if timeout <= 0:
            raise GuardError(f"worktree report exceeded {MAX_REPORT_SECONDS}s")
    command = ["git", "-C", os.fspath(root), *args]
    process = subprocess.Popen(
        command,
        cwd=root,
        start_new_session=(os.name == "posix"),
        stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    assert process.stdout is not None
    assert process.stderr is not None
    selector = selectors.DefaultSelector()
    selector.register(process.stdout, selectors.EVENT_READ, "stdout")
    selector.register(process.stderr, selectors.EVENT_READ, "stderr")
    current = bytearray()
    paths: list[str] = []
    stderr = bytearray()
    total = 0
    deadline_at = time.monotonic() + timeout

    def stop(message: str) -> None:
        try:
            _kill_process_group(process)
        finally:
            # Descendants may retain these descriptors after the Git leader
            # exits.  Discard the bounded evidence rather than waiting on
            # inherited pipe handles during timeout cleanup.
            process.stdout.close()
            process.stderr.close()
        raise GuardError(message)

    try:
        while selector.get_map():
            remaining = deadline_at - time.monotonic()
            if remaining <= 0:
                stop(f"git command timed out after {GIT_COMMAND_TIMEOUT_SECONDS}s")
            events = selector.select(remaining)
            if not events:
                stop(f"git command timed out after {GIT_COMMAND_TIMEOUT_SECONDS}s")
            for key, _ in events:
                data = os.read(key.fd, 64 * 1024)
                if not data:
                    selector.unregister(key.fileobj)
                    continue
                if key.data == "stderr":
                    stderr.extend(data)
                    if len(stderr) > MAX_GIT_OUTPUT_BYTES:
                        stop(f"git error output exceeded {MAX_GIT_OUTPUT_BYTES} bytes")
                    continue
                total += len(data)
                if total > MAX_GIT_OUTPUT_BYTES:
                    stop(f"git path evidence exceeded {MAX_GIT_OUTPUT_BYTES} bytes")
                current.extend(data)
                while b"\0" in current:
                    raw, _, rest = current.partition(b"\0")
                    current = bytearray(rest)
                    paths.append(raw.decode(errors="surrogateescape"))
                    if len(paths) > limit:
                        stop(f"git path evidence exceeded {limit} entries")
        try:
            process.wait(timeout=max(0.0, deadline_at - time.monotonic()))
        except subprocess.TimeoutExpired as error:
            try:
                _kill_process_group(process)
            finally:
                process.stdout.close()
                process.stderr.close()
            raise GuardError(f"git command timed out after {GIT_COMMAND_TIMEOUT_SECONDS}s") from error
    finally:
        selector.close()
        process.stdout.close()
        process.stderr.close()
    if process.returncode:
        detail = bytes(stderr).decode(errors="surrogateescape").strip().splitlines()
        raise GuardError(detail[0] if detail else "git command failed")
    if current:
        raise GuardError("git returned a malformed NUL-delimited path list")
    return paths


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


def directory_size(path: Path, *, deadline: float | None = None) -> int:
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
        if deadline is not None and time.monotonic() >= deadline:
            raise GuardError(f"worktree report exceeded {MAX_REPORT_SECONDS}s")
        directory = pending.pop()
        try:
            entries = os.scandir(directory)
        except OSError as error:
            raise GuardError(f"cannot read cache directory: {directory}: {error}") from error
        with entries:
            for entry in entries:
                if deadline is not None and time.monotonic() >= deadline:
                    raise GuardError(f"worktree report exceeded {MAX_REPORT_SECONDS}s")
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


def tracked_cache_paths(root: Path, target: str, *, deadline: float | None = None) -> list[str]:
    """Return tracked index paths under one fixed cache name, NUL-safe."""
    return git_nul_paths(root, "ls-files", "-z", "--", target, f"{target}/",
                         limit=MAX_IGNORED_PATHS, deadline=deadline)


def ignored_non_cache_paths(root: Path, *, deadline: float | None = None) -> list[str]:
    """Return ignored paths outside the four explicitly disposable caches."""
    # Exclude disposable roots in Git itself. This prevents a huge ignored
    # target/node_modules tree from being captured before our path bound runs.
    excludes = tuple(f":(exclude){name}/**" for name in REGENERABLE_WORKTREE_CACHES)
    paths = git_nul_paths(root, "ls-files", "--others", "--ignored", "--exclude-standard", "-z",
                          "--", ".", *excludes, limit=MAX_IGNORED_PATHS,
                          deadline=deadline)
    return [path for path in paths if path.split("/", 1)[0] not in REGENERABLE_WORKTREE_CACHES]


def parse_worktree_list(root: Path, *, deadline: float | None = None) -> list[tuple[Path, str | None]]:
    records: list[tuple[Path, str | None]] = []
    path: Path | None = None
    branch: str | None = None
    for line in git(root, "worktree", "list", "--porcelain", deadline=deadline).splitlines() + [""]:
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


def unsafe_branch(root: Path, branch: str | None, *, deadline: float | None = None) -> bool:
    """Prove a checked-out branch is safe only against the production ref."""
    # Detached worktrees have no branch ref to prove stale.  They may contain
    # an operator's live checkout, so cleanup must protect them by default.
    if branch is None:
        return True
    if branch in {"main", "master"}:
        try:
            local = git(root, "rev-parse", "--verify", branch, deadline=deadline).strip()
            remote = git(root, "rev-parse", "--verify", "origin/main", deadline=deadline).strip()
        except GuardError as error:
            raise GuardError(f"cannot prove {branch} matches origin/main: {error}") from error
        return local != remote
    timeout = float(GIT_COMMAND_TIMEOUT_SECONDS)
    if deadline is not None:
        timeout = min(timeout, deadline - time.monotonic())
        if timeout <= 0:
            raise GuardError(f"worktree report exceeded {MAX_REPORT_SECONDS}s")
    try:
        completed = _run_bounded(
            ["git", "-C", os.fspath(root), "merge-base", "--is-ancestor", branch, "origin/main"],
            cwd=root, timeout=timeout, capture_output=True,
        )
    except subprocess.TimeoutExpired as error:
        if deadline is not None and time.monotonic() >= deadline:
            raise GuardError(f"worktree report exceeded {MAX_REPORT_SECONDS}s") from error
        raise GuardError(f"git command timed out after {GIT_COMMAND_TIMEOUT_SECONDS}s") from error
    if completed.returncode == 0:
        return False
    if completed.returncode == 1:
        return True
    raise GuardError("cannot determine whether worktree branch is still active")


def worktree_states(root: Path, branch: str | None, *, deadline: float | None = None) -> tuple[str, ...]:
    porcelain = git(root, "status", "--porcelain=v1", "--untracked-files=all", deadline=deadline)
    tracked_dirty = False
    untracked = False
    for line in porcelain.splitlines():
        if line.startswith("?? "):
            untracked = True
        elif line[:2] != "  ":
            tracked_dirty = True
    ignored = bool(ignored_non_cache_paths(root, deadline=deadline))
    states: list[str] = []
    if unsafe_branch(root, branch, deadline=deadline):
        states.append("active")
    if tracked_dirty:
        states.append("dirty")
    if untracked:
        states.append("untracked")
    if ignored:
        states.append("ignored")
    return tuple(states or ["clean"])


def inspect_worktree(path: Path, branch: str | None, *, deadline: float | None = None) -> WorktreeReport:
    # A missing or symlinked listed path is evidence unavailable.  Never follow
    # it to find a cache or to infer a safe candidate.
    if path.is_symlink() or not path.is_dir():
        return WorktreeReport(path, branch, ("unavailable",), ())
    physical = path.resolve()
    try:
        states = worktree_states(physical, branch, deadline=deadline)
    except GuardError:
        states = ("unavailable",)
    # Never spend unbounded time measuring a live/dirty checkout: its cache is
    # categorically ineligible for cleanup.  A zero size here means
    # "intentionally not measured", while clean candidates receive the bounded
    # byte inventory below.
    if set(states) - {"clean"}:
        return WorktreeReport(physical, branch, states, ())
    caches = tuple(
        (name, directory_size(physical / name, deadline=deadline))
        for name in REGENERABLE_WORKTREE_CACHES
        if (physical / name).exists() and not (physical / name).is_symlink()
    )
    return WorktreeReport(physical, branch, states, caches)


def collect_report(root: Path) -> list[WorktreeReport]:
    reports: list[WorktreeReport] = []
    deadline = time.monotonic() + MAX_REPORT_SECONDS
    for path, branch in parse_worktree_list(root, deadline=deadline):
        if time.monotonic() >= deadline:
            raise GuardError(f"worktree report exceeded {MAX_REPORT_SECONDS}s")
        report = inspect_worktree(path, branch, deadline=deadline)
        if time.monotonic() >= deadline:
            raise GuardError(f"worktree report exceeded {MAX_REPORT_SECONDS}s")
        reports.append(report)
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


def cache_identity(path: Path) -> tuple[int, int] | None:
    try:
        metadata = path.stat(follow_symlinks=False)
    except FileNotFoundError:
        return None
    if stat.S_ISLNK(metadata.st_mode) or not stat.S_ISDIR(metadata.st_mode):
        raise GuardError("cleanup target must remain a direct non-symlink cache directory")
    return metadata.st_dev, metadata.st_ino


def _assert_confinement(confinement: tuple[tuple[int, str, tuple[int, int]], ...]) -> None:
    """Prove every authorized directory is still linked below the worktree."""
    for parent_fd, name, identity in confinement:
        try:
            current = os.stat(name, dir_fd=parent_fd, follow_symlinks=False)
        except FileNotFoundError as error:
            raise GuardError(f"authorized cache moved during deletion: {name}") from error
        if (current.st_dev, current.st_ino) != identity:
            raise GuardError(f"quarantine replacement or authorized cache replacement during deletion: {name}")


def remove_tree_fd(directory_fd: int, *,
                   _confinement: tuple[tuple[int, str, tuple[int, int]], ...] = ()) -> None:
    """Remove descendants only while every directory remains confined."""
    if not _confinement:
        raise GuardError("refusing unconfined directory-descriptor deletion")
    _assert_confinement(_confinement)
    try:
        entries = os.scandir(directory_fd)
    except OSError as error:
        raise GuardError(f"cannot scan authorized cache descriptor: {error}") from error
    with entries:
        for entry in entries:
            name = entry.name
            _assert_confinement(_confinement)
            try:
                observed = entry.stat(follow_symlinks=False)
            except FileNotFoundError:
                continue
            except OSError as error:
                raise GuardError(f"cannot inspect cache entry: {name}: {error}") from error
            try:
                _assert_confinement(_confinement)
                current = os.stat(name, dir_fd=directory_fd, follow_symlinks=False)
            except FileNotFoundError:
                continue
            if (observed.st_dev, observed.st_ino) != (current.st_dev, current.st_ino):
                raise GuardError(f"cache entry changed during deletion: {name}")
            if stat.S_ISDIR(current.st_mode) and not stat.S_ISLNK(current.st_mode):
                flags = (os.O_RDONLY | getattr(os, "O_DIRECTORY", 0)
                         | os.O_NOFOLLOW | getattr(os, "O_CLOEXEC", 0))
                try:
                    child_fd = os.open(name, flags, dir_fd=directory_fd)
                except FileNotFoundError:
                    continue
                except OSError as error:
                    raise GuardError(f"cannot open cache child without following symlinks: {name}: {error}") from error
                try:
                    child_stat = os.fstat(child_fd)
                    if (child_stat.st_dev, child_stat.st_ino) != (observed.st_dev, observed.st_ino):
                        raise GuardError(f"cache entry changed during deletion: {name}")
                    try:
                        current_child = os.stat(name, dir_fd=directory_fd,
                                                follow_symlinks=False)
                    except FileNotFoundError as error:
                        raise GuardError(
                            f"cache directory moved during deletion: {name}"
                        ) from error
                    if (current_child.st_dev, current_child.st_ino) != (
                            child_stat.st_dev, child_stat.st_ino):
                        raise GuardError(f"cache entry changed during deletion: {name}")
                    remove_tree_fd(child_fd, _confinement=_confinement + (
                        (directory_fd, name, (child_stat.st_dev, child_stat.st_ino)),))
                finally:
                    os.close(child_fd)
                _assert_confinement(_confinement)
                try:
                    final = os.stat(name, dir_fd=directory_fd, follow_symlinks=False)
                except FileNotFoundError as error:
                    raise GuardError(
                        f"cache directory moved during deletion: {name}"
                    ) from error
                if (final.st_dev, final.st_ino) != (observed.st_dev, observed.st_ino):
                    raise GuardError(f"cache entry changed during deletion: {name}")
                try:
                    os.rmdir(name, dir_fd=directory_fd)
                except FileNotFoundError:
                    continue
            else:
                private = f".corelink-capacity-entry-{os.getpid()}-{secrets.token_hex(8)}"
                try:
                    _assert_confinement(_confinement)
                    os.rename(name, private, src_dir_fd=directory_fd,
                              dst_dir_fd=directory_fd)
                except FileNotFoundError:
                    continue
                except OSError as error:
                    raise GuardError(f"cannot quarantine cache entry: {name}: {error}") from error
                try:
                    _assert_confinement(_confinement)
                    moved = os.stat(private, dir_fd=directory_fd, follow_symlinks=False)
                    if (moved.st_dev, moved.st_ino) != (observed.st_dev, observed.st_ino):
                        try:
                            os.rename(private, name, src_dir_fd=directory_fd,
                                      dst_dir_fd=directory_fd)
                        except OSError:
                            pass
                        raise GuardError(f"cache entry changed during deletion: {name}")
                    os.unlink(private, dir_fd=directory_fd)
                except FileNotFoundError:
                    raise GuardError(f"cache entry disappeared during deletion: {name}")


def remove_cache_safely(root: Path, path: Path,
                        expected_identity: tuple[int, int] | None) -> None:
    """Remove one validated cache using a directory descriptor, or refuse.

    Python's fd-based rmtree rejects symlink traversal on supported POSIX
    platforms. The parent descriptor also keeps the operation rooted at the
    exact validated worktree instead of a re-resolved path.
    """
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
            if expected_identity is None:
                raise GuardError("cleanup target appeared after validation; refusing deletion")
            if (before.st_dev, before.st_ino) != expected_identity:
                raise GuardError("cleanup target replacement detected before deletion")
            current = os.stat(path.name, dir_fd=parent_fd, follow_symlinks=False)
            if (before.st_dev, before.st_ino) != (current.st_dev, current.st_ino):
                raise GuardError("cache changed during deletion validation")
            quarantine = f".corelink-capacity-quarantine-{os.getpid()}-{secrets.token_hex(8)}"
            try:
                # Move the exact inode to a private sibling first. If an
                # attacker renames the validated directory and replaces its
                # public name, the identity check below refuses the replacement
                # and nothing at the replacement name is deleted.
                os.rename(path.name, quarantine, src_dir_fd=parent_fd, dst_dir_fd=parent_fd)
            except FileNotFoundError:
                return
            except OSError as error:
                raise GuardError(f"cache changed during quarantine: {error}") from error
            moved = os.stat(quarantine, dir_fd=parent_fd, follow_symlinks=False)
            if (before.st_dev, before.st_ino) != (moved.st_dev, moved.st_ino):
                try:
                    os.rename(quarantine, path.name, src_dir_fd=parent_fd, dst_dir_fd=parent_fd)
                except OSError:
                    # If a replacement already occupies the public name, leave
                    # both entries untouched rather than deleting either one.
                    pass
                raise GuardError("cache replacement detected during quarantine; nothing deleted")
            # Keep the opened descriptor as the deletion capability.  A
            # replacement at the quarantine pathname cannot redirect this
            # recursive walk to another directory.
            remove_tree_fd(
                target_fd,
                _confinement=((parent_fd, quarantine, (before.st_dev, before.st_ino)),),
            )
            try:
                final = os.stat(quarantine, dir_fd=parent_fd, follow_symlinks=False)
            except FileNotFoundError:
                raise GuardError("quarantine disappeared during deletion; refusing cleanup")
            if (final.st_dev, final.st_ino) != (before.st_dev, before.st_ino):
                raise GuardError("quarantine replacement detected after deletion; original retained")
            try:
                os.rmdir(quarantine, dir_fd=parent_fd)
            except FileNotFoundError as error:
                raise GuardError("quarantine disappeared during deletion; refusing cleanup") from error
        finally:
            os.close(target_fd)
    finally:
        os.close(parent_fd)


def dry_run_cleanup(root: Path, targets: list[str], execute: bool) -> None:
    if not targets:
        raise GuardError("--cleanup requires at least one explicit --cleanup-target")
    paths = []
    seen: set[Path] = set()
    identities: dict[Path, tuple[int, int] | None] = {}
    for target in targets:
        path = cleanup_path(root, target)
        if path not in seen:
            paths.append(path)
            seen.add(path)
            identities[path] = cache_identity(path)
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
            remove_cache_safely(root, validated, identities[path])
    print("explicit regenerable caches removed; no worktree, branch, Docker, volume, or shared HOME cache was touched")


def _acquire_stable_lock(path: Path, deadline: float) -> tuple[int, os.stat_result]:
    flags = (os.O_RDWR | getattr(os, "O_CLOEXEC", 0) | os.O_NOFOLLOW
             | getattr(os, "O_NONBLOCK", 0))
    try:
        fd = os.open(path, flags | os.O_CREAT | os.O_EXCL, 0o600)
    except FileExistsError:
        try:
            fd = os.open(path, flags)
        except OSError as error:
            raise GuardError(f"cannot open stable materialization lock: {error}") from error
    except OSError as error:
        raise GuardError(f"cannot create stable materialization lock: {error}") from error
    try:
        identity = os.fstat(fd)
        current = os.stat(path, follow_symlinks=False)
        if not stat.S_ISREG(identity.st_mode) or (identity.st_dev, identity.st_ino) != (
            current.st_dev, current.st_ino
        ):
            raise GuardError("materialization lock path changed or is not a regular file")
        while True:
            try:
                fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
                break
            except BlockingIOError:
                if time.monotonic() >= deadline:
                    raise GuardError("materialization lock is held by another worktree; retry after it finishes")
                time.sleep(0.05)
        current = os.stat(path, follow_symlinks=False)
        if (identity.st_dev, identity.st_ino) != (current.st_dev, current.st_ino):
            raise GuardError("materialization lock path changed while acquiring lock")
        return fd, identity
    except BaseException:
        try:
            fcntl.flock(fd, fcntl.LOCK_UN)
        except OSError:
            pass
        os.close(fd)
        raise


def _acquire_directory_authority(path: Path, deadline: float) -> int:
    flags = os.O_RDONLY | os.O_NOFOLLOW | getattr(os, "O_DIRECTORY", 0) | getattr(os, "O_CLOEXEC", 0)
    try:
        fd = os.open(path, flags)
    except OSError as error:
        raise GuardError(f"cannot open lock authority directory: {error}") from error
    try:
        while True:
            try:
                fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
                return fd
            except BlockingIOError:
                if time.monotonic() >= deadline:
                    raise GuardError("materialization lock is held by another worktree; retry after it finishes")
                time.sleep(0.05)
    except BaseException:
        os.close(fd)
        raise


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
    # Keep named files as inspectable compatibility markers; the common-dir fd
    # below is the authority, so replacing every marker cannot split ownership.
    anchor_path = common / "corelink-capacity-materialization.anchor"
    lock_path = common / "corelink-capacity-materialization.lock"
    if not getattr(os, "O_NOFOLLOW", 0) or not getattr(os, "O_DIRECTORY", 0):
        raise GuardError("refusing lock: O_NOFOLLOW is unavailable")
    deadline = time.monotonic() + timeout_seconds
    guard_path = common / "corelink-capacity-materialization.guard"
    authority_fd = _acquire_directory_authority(common, deadline)
    try:
        anchor_fd, _ = _acquire_stable_lock(anchor_path, deadline)
        try:
            guard_fd, _ = _acquire_stable_lock(guard_path, deadline)
            try:
                lock_fd, _ = _acquire_stable_lock(lock_path, deadline)
                try:
                    yield
                finally:
                    fcntl.flock(lock_fd, fcntl.LOCK_UN)
                    os.close(lock_fd)
            finally:
                fcntl.flock(guard_fd, fcntl.LOCK_UN)
                os.close(guard_fd)
        finally:
            fcntl.flock(anchor_fd, fcntl.LOCK_UN)
            os.close(anchor_fd)
    finally:
        fcntl.flock(authority_fd, fcntl.LOCK_UN)
        os.close(authority_fd)


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
    parser.add_argument("--command-timeout-seconds", type=parse_command_timeout,
                        default=float(MAX_COMMAND_TIMEOUT_SECONDS),
                        help="hard bound for the locked command (default: 24 hours)")
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
                try:
                    completed = _run_bounded(
                        args.command, cwd=root, timeout=args.command_timeout_seconds,
                    )
                except subprocess.TimeoutExpired as error:
                    raise GuardError(
                        f"locked command exceeded {args.command_timeout_seconds:g}s; aborted"
                    ) from error
                return completed.returncode
        return 0
    except (GuardError, ProcessBoundsError, argparse.ArgumentTypeError) as error:
        print(f"CAPACITY INDETERMINATE: {error}", file=sys.stderr)
        return 2
    except OSError as error:
        print(f"CAPACITY INDETERMINATE: OS evidence unavailable: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
