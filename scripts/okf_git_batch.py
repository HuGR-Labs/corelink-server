"""Small batched Git helpers used by the OKF validator.

Keeping these mechanics out of the validator's godfile lets the gate retain
its one-file policy implementation while avoiding one Git process per cite.
"""

from __future__ import annotations

import hashlib
import subprocess
from pathlib import Path


def line_count(path: Path, cache: dict[Path, tuple[int, int, int, int, int]] | None = None) -> int:
    """Count lines using a caller-scoped cache invalidated by file edits."""
    path = path.resolve()
    try:
        stat = path.stat()
        fingerprint = (stat.st_dev, stat.st_ino, stat.st_size, stat.st_mtime_ns)
    except OSError:
        fingerprint = None
    if cache is not None and fingerprint is not None:
        cached = cache.get(path)
        if cached is not None and cached[:4] == fingerprint:
            return cached[4]
    try:
        with path.open("rb") as fh:
            count = sum(1 for _ in fh)
    except OSError:
        count = 0
    if cache is not None and fingerprint is not None:
        cache[path] = (*fingerprint, count)
    return count


def preload_sha_exists(git, shas: list[str]) -> None:
    pending = [sha for sha in dict.fromkeys(shas) if sha not in git._sha_cache]
    if not pending:
        return
    cp = subprocess.run(
        ["git", "cat-file", "--batch-check=%(objectname) %(objecttype)"],
        cwd=str(git.repo_root), capture_output=True,
        input=("\n".join(pending) + "\n").encode(),
    )
    for sha, row in zip(pending, cp.stdout.decode("utf-8", errors="replace").splitlines()):
        parts = row.split()
        git._sha_cache[sha] = len(parts) >= 2 and parts[1] == "commit"


def file_blob_sha(git, rev: str, repo_rel: str):
    key = (rev, repo_rel)
    if key in git._blob_cache:
        return git._blob_cache[key]
    trees = getattr(git, "_tree_blob_cache", {})
    if rev not in trees:
        cp = git.run(["ls-tree", "-r", rev])
        tree = {}
        if cp.returncode == 0:
            for line in cp.stdout.splitlines():
                head, sep, path = line.partition("\t")
                fields = head.split()
                if sep and len(fields) >= 3 and fields[1] == "blob":
                    tree[path] = fields[2]
        trees[rev] = tree
        git._tree_blob_cache = trees
    val = trees[rev].get(repo_rel)
    git._blob_cache[key] = val
    return val


def worktree_blob_sha(git, repo_rel: str):
    if repo_rel in git._wt_blob_cache:
        return git._wt_blob_cache[repo_rel]
    path = git.repo_root / repo_rel
    if not path.exists():
        git._wt_blob_cache[repo_rel] = None
        return None
    try:
        content = path.read_bytes()
    except OSError:
        val = None
    else:
        val = hashlib.sha1(b"blob " + str(len(content)).encode() + b"\0" + content).hexdigest()
    git._wt_blob_cache[repo_rel] = val
    return val


def preload_show_files(git, rev: str, paths: list[str]) -> None:
    pending = [p for p in dict.fromkeys(paths) if (rev, p) not in git._show_file_cache]
    if not pending:
        return
    cp = subprocess.run(
        ["git", "cat-file", "--batch"], cwd=str(git.repo_root),
        capture_output=True, input="".join(f"{rev}:{p}\n" for p in pending).encode(),
    )
    data, pos = cp.stdout, 0
    for path in pending:
        end = data.find(b"\n", pos)
        if end < 0:
            git._show_file_cache[(rev, path)] = None
            continue
        header = data[pos:end].split()
        pos = end + 1
        if len(header) < 3 or header[1] != b"blob":
            git._show_file_cache[(rev, path)] = None
            continue
        try:
            size = int(header[2])
        except ValueError:
            git._show_file_cache[(rev, path)] = None
            continue
        payload, pos = data[pos : pos + size], pos + size
        if pos < len(data) and data[pos : pos + 1] == b"\n":
            pos += 1
        try:
            git._show_file_cache[(rev, path)] = payload.decode("utf-8")
        except UnicodeDecodeError:
            git._show_file_cache[(rev, path)] = None
