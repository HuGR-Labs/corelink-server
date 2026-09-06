"""YAML/frontmatter, git, concept, and failure primitives for OKF."""
from __future__ import annotations

import os
import re
import subprocess
from pathlib import Path
from okf_git_batch import file_blob_sha as _file_blob_sha, line_count as _line_count, preload_sha_exists as _preload_sha_exists, preload_show_files as _preload_show_files, worktree_blob_sha as _worktree_blob_sha

_USE_YAML = False
if not os.environ.get("OKF_NO_YAML"):
    try:
        import yaml as _yaml  # type: ignore

        _USE_YAML = True
    except Exception:  # pragma: no cover - environment dependent
        _USE_YAML = False


FRONT_MATTER_RE = re.compile(r"^---\n(.*?)\n---\n?(.*)$", re.DOTALL)
# Code-anchor cite token, parsed only from inside backticks; path is optional for
# abbreviated continuations, while C6c deliberately uses the strict full form.
CITE_RE = re.compile(r"^(?P<path>[A-Za-z0-9._/\-]+)?:(?P<l1>\d+)(?:-(?P<l2>\d+))?$")
CITE_FULL_RE = re.compile(r"^(?P<path>[A-Za-z0-9._/\-]+):(?P<l1>\d+)(?:-(?P<l2>\d+))?$")
# Path-only anchors are accepted only from the concept's exact declared sources.
BARE_PATH_RE = re.compile(r"^[A-Za-z0-9._/\-]+$")
BACKTICK_RE = re.compile(r"`([^`]+)`")
# Bundle-relative markdown link with a leading slash: [text](/dir/x.md)
LINK_RE = re.compile(r"\[[^\]]*\]\((/[^)\s]+)\)")
HEADING_RE = re.compile(r"^(#{1,6})\s+(.*?)\s*$")
LIST_ITEM_RE = re.compile(r"^\s*(?:[-*]|\d+\.)\s+")
HEX40_RE = re.compile(r"^[0-9a-fA-F]{40}$")
# A `source_blobs` entry: `<repo-relative path>@<40-hex blob object id>`.
# `@` is NOT in the citation path charset above, so the split is unambiguous;
# the shape mirrors the house `grounded@sha` idiom used across the repo.
SOURCE_BLOB_RE = re.compile(
    r"^(?P<path>[A-Za-z0-9._/\-]+)@(?P<blob>[0-9a-fA-F]{40})$"
)

RESERVED_NAMES = {"index.md", "log.md"}
ADR_TYPE = "ADR"


# ---------------------------------------------------------------------------
# Minimal YAML-subset reader (used when PyYAML is unavailable).
# ---------------------------------------------------------------------------
def _scalar_from_value(v: str):
    v = v.strip()
    if v and v[0] in "\"'":
        q = v[0]
        end = v.find(q, 1)
        if end != -1:
            return v[1:end]
        return v[1:]
    if "#" in v:
        v = v.split("#", 1)[0].strip()
    if v.lower() in ("true", "false"):
        return v.lower() == "true"
    return v


def _hand_parse(block: str) -> dict:
    data: dict = {}
    cur_key = None
    for raw in block.split("\n"):
        if not raw.strip() or raw.lstrip().startswith("#"):
            continue
        m_item = re.match(r"^(\s+)-\s+(.*)$", raw)
        if m_item and cur_key is not None and isinstance(data.get(cur_key), list):
            data[cur_key].append(_scalar_from_value(m_item.group(2)))
            continue
        m_kv = re.match(r"^([A-Za-z0-9_\-]+):\s*(.*)$", raw)
        if m_kv:
            key, val = m_kv.group(1), m_kv.group(2)
            if val.strip() == "":
                data[key] = []
                cur_key = key
            else:
                v = val.strip()
                if v.startswith("[") and v.endswith("]"):
                    inner = v[1:-1].strip()
                    data[key] = (
                        [_scalar_from_value(x) for x in inner.split(",") if x.strip()]
                        if inner
                        else []
                    )
                else:
                    data[key] = _scalar_from_value(v)
                cur_key = None
            continue
    return data


def parse_frontmatter(block: str) -> dict:
    if _USE_YAML:
        data = _yaml.safe_load(block)
        if data is None:
            return {}
        if not isinstance(data, dict):
            raise ValueError(f"front matter is not a mapping (is {type(data).__name__})")
        return data
    return _hand_parse(block)


# ---------------------------------------------------------------------------
# git helpers
# ---------------------------------------------------------------------------
class Git:
    def __init__(self, repo_root: Path):
        self.repo_root = repo_root
        self._sha_cache: dict[str, bool] = {}
        self._show_cache: dict[tuple[str, str], list[str] | None] = {}
        self._blob_cache: dict[tuple[str, str], str | None] = {}
        self._wt_blob_cache: dict[str, str | None] = {}
        self._wt_lines_cache: dict[str, list[str] | None] = {}
        self._blob_present_cache: dict[str, bool] = {}
        self._blob_lines_cache: dict[str, list[str] | None] = {}
        self._show_file_cache: dict[tuple[str, str], str | None] = {}
        self._path_blobs_cache: dict[str, set[str]] = {}

    def run(self, args: list[str], stdin: str | None = None) -> subprocess.CompletedProcess:
        return subprocess.run(
            ["git"] + args,
            cwd=str(self.repo_root),
            capture_output=True,
            text=True,
            input=stdin,
        )

    def sha_exists(self, sha: str) -> bool:
        if sha in self._sha_cache:
            return self._sha_cache[sha]
        cp = self.run(["rev-parse", "--verify", "--quiet", f"{sha}^{{commit}}"])
        ok = cp.returncode == 0 and bool(cp.stdout.strip())
        self._sha_cache[sha] = ok
        return ok

    def file_blob_sha(self, rev: str, repo_rel: str):
        """The git blob object id of `repo_rel` at `rev` (None if the path does
        not exist at that rev). Used for the C5 per-file trivially-fresh skip."""
        return _file_blob_sha(self, rev, repo_rel)

    def show_lines(self, rev: str, repo_rel: str):
        """Lines of `repo_rel` at `rev` (line terminators stripped), or None if
        the path does not exist at that rev. Cached per (rev, path)."""
        key = (rev, repo_rel)
        if key in self._show_cache:
            return self._show_cache[key]
        content = self.show_file(rev, repo_rel)
        lines = content.splitlines() if content is not None else None
        self._show_cache[key] = lines
        return lines

    def worktree_blob_sha(self, repo_rel: str):
        """The git blob object id the on-disk WORKING-TREE file would hash to
        (None if the path does not exist on disk). The working-tree analogue of
        `file_blob_sha`, used for the C5 per-file trivially-fresh skip so the
        skip stays correct in a dirty tree (where worktree != HEAD)."""
        return _worktree_blob_sha(self, repo_rel)

    def worktree_lines(self, repo_rel: str):
        """Lines of the on-disk WORKING-TREE file `repo_rel` (line terminators
        stripped), or None if the path does not exist on disk. This is the SAME
        tree C3 (file-exists) and C6 (line-bounds) validate, so the C5 HEAD-side
        baseline stays consistent with them on a dirty/pre-commit run. Cached."""
        if repo_rel in self._wt_lines_cache:
            return self._wt_lines_cache[repo_rel]
        p = self.repo_root / repo_rel
        if not p.exists():
            self._wt_lines_cache[repo_rel] = None
            return None
        try:
            content = p.read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError):
            content = None
        lines = content.splitlines() if content is not None else None
        self._wt_lines_cache[repo_rel] = lines
        return lines

    def blob_is_present(self, sha: str) -> bool:
        """True iff `sha` names a BLOB object present in THIS clone's object
        database. Deliberately strict about the object TYPE: a commit id pasted
        into `source_blobs` must be rejected, not silently peeled.

        This is the fail-CLOSED half of blob addressing. Blob ids are content
        hashes, immutable under rebase,
        squash and cherry-pick, so a blob that was ever pushed stays reachable
        from the rewritten history and IS in the gate's `fetch-depth: 0` clone.
        An absent blob is therefore never "orphaned" — it is a typo or a forgery,
        and it FAILS (C4b) instead of falling back to anything.

        Caveat recorded honestly: locally, `git hash-object -w` can plant a loose
        blob that resolves in the author's clone only. That is why the CI gate —
        which clones fresh and has only pushed objects — is the enforcing
        instance; a fabricated blob RESOLVES for the author and FAILS in CI,
        which is the fail-loud direction."""
        if not sha:
            return False
        if sha in self._blob_present_cache:
            return self._blob_present_cache[sha]
        cp = self.run(["cat-file", "-t", sha])
        ok = cp.returncode == 0 and cp.stdout.strip() == "blob"
        self._blob_present_cache[sha] = ok
        return ok

    def path_blob_history(self, repo_rel: str) -> set[str]:
        """Every blob id `repo_rel` has EVER had at a commit REACHABLE FROM HEAD,
        plus HEAD's own. This is the durable-reachability oracle behind C4b's
        second half (`blob_reachable_for_path`).

        Cost is bounded and lazy: `git rev-list HEAD -- <path>` (one process) fed
        into ONE `git cat-file --batch-check` (a second process) per path, and the
        whole thing is skipped for the overwhelmingly common case where the anchor
        already equals HEAD's blob. Nothing here walks the object database."""
        if repo_rel in self._path_blobs_cache:
            return self._path_blobs_cache[repo_rel]
        blobs: set[str] = set()
        head_blob = self.file_blob_sha("HEAD", repo_rel)
        if head_blob:
            blobs.add(head_blob)
        cp = self.run(["rev-list", "HEAD", "--", repo_rel])
        revs = cp.stdout.split() if cp.returncode == 0 else []
        if revs:
            probe = "".join(f"{r}:{repo_rel}\n" for r in revs)
            out = self.run(["cat-file", "--batch-check=%(objectname) %(objecttype)"], stdin=probe)
            for line in out.stdout.splitlines():
                parts = line.split()
                if len(parts) == 2 and parts[1] == "blob":
                    blobs.add(parts[0])
        self._path_blobs_cache[repo_rel] = blobs
        return blobs

    def blob_reachable_for_path(self, repo_rel: str, sha: str) -> bool:
        """True iff `sha` is the content `repo_rel` had at SOME commit reachable
        from HEAD — i.e. the anchored content DURABLY LANDED on this history.

        `blob_is_present` above is NOT sufficient, and the gap is not theoretical:
        it cost us a false-green on `push:main`. A concept authored mid-PR can
        anchor an INTERMEDIATE commit's blob; a later commit on the same PR branch
        supersedes it, so the squash-merge lands the branch TIP's blob and the
        anchored one never reaches `main` at all. The anchor is then reachable only
        from the PR head ref — which GitHub auto-DELETES at merge. The `push:main`
        run clones seconds after the merge and still fetches that dying ref, so
        `cat-file -t` resolves and C4b passes; every later clone lacks the ref, the
        object is gone, and the identical check REDs. Presence in the object
        database is therefore a TIMING artifact of when the gate happened to clone.
        Reachability from HEAD is not: it is a property of the history being gated,
        so it returns the same verdict at merge time and forever after.

        Deliberately permissive about AGE — an anchor may name any historical
        version of the file, not just HEAD's. That is the point of a content anchor
        (C5 is what decides whether the cited lines have since drifted); this check
        only rejects content that was never on this history at all."""
        if not sha or not repo_rel:
            return False
        if self.file_blob_sha("HEAD", repo_rel) == sha:
            return True  # fast path: the anchor IS HEAD's content
        return sha in self.path_blob_history(repo_rel)

    def blob_lines(self, sha: str):
        """Lines of the blob object `sha` (line terminators stripped), or None
        when the object is absent or is not a blob. Cached per sha."""
        if sha in self._blob_lines_cache:
            return self._blob_lines_cache[sha]
        if not self.blob_is_present(sha):
            self._blob_lines_cache[sha] = None
            return None
        cp = self.run(["cat-file", "blob", sha])
        lines = cp.stdout.splitlines() if cp.returncode == 0 else None
        self._blob_lines_cache[sha] = lines
        return lines

    def commit_date(self, sha: str):
        cp = self.run(["show", "-s", "--format=%cI", sha])
        if cp.returncode != 0:
            return None
        try:
            return datetime.fromisoformat(cp.stdout.strip())
        except Exception:
            return None

    def merge_base(self, ref: str):
        cp = self.run(["merge-base", "HEAD", ref])
        if cp.returncode != 0:
            return None
        return cp.stdout.strip() or None

    def is_ancestor(self, sha: str, ref: str) -> bool:
        """True iff `sha` is an ancestor of `ref` (reachable on that history).
        False when it is not — INCLUDING when `sha` is not a resolvable commit
        in this checkout (an ORPHANED pointer: a pre-merge branch tip that was
        rewritten/replayed at merge and never landed on the mainline). git's
        `merge-base --is-ancestor` exits 0 for ancestor, 1 for non-ancestor,
        and 128 when a commit is unknown — all non-zero cases are `not an
        ancestor` for our purposes (a shallow CI clone simply lacks the orphan)."""
        if not sha:
            return False
        return self.run(["merge-base", "--is-ancestor", sha, ref]).returncode == 0

    def resolve_base(self, ref: str):
        """Resolve a named base ref for auxiliary diff tools.

        This helper is retained for C5c's previous-version comparison and other
        tooling. It is deliberately NOT used to replace a concept's checkpoint:
        C4/C5 require the checkpoint itself to be reachable from ``HEAD``.
        """
        for cand in (ref, f"origin/{ref}"):
            cp = self.run(["rev-parse", "--verify", "--quiet", f"{cand}^{{commit}}"])
            sha = cp.stdout.strip()
            if cp.returncode == 0 and sha:
                return sha
        return None

    def show_file(self, rev: str, repo_rel: str):
        """Contents of `repo_rel` at `rev`, or None. CACHED: C5b and C4c both read
        the SAME previous-version text for every concept (one `git show` per
        concept each), and on a 160-concept corpus that is 160 redundant
        subprocesses. One cache keyed on (rev, path) makes the second reader free."""
        key = (rev, repo_rel)
        if key in self._show_file_cache:
            return self._show_file_cache[key]
        cp = self.run(["show", f"{rev}:{repo_rel}"])
        out = cp.stdout if cp.returncode == 0 else None
        self._show_file_cache[key] = out
        return out

# ---------------------------------------------------------------------------

# Failure collection
# ---------------------------------------------------------------------------
class Failures:
    ORDER = [
        "C1", "C2", "C3", "C4", "C4b", "C4c", "C5", "C5b", "C5c", "C6", "C6b", "C6c",
        "C7", "C8", "C9", "C10", "C10b",
    ]

    def __init__(self):
        self.items: list[tuple[str, str, str]] = []  # (check, location, message)
        self.warnings: list[tuple[str, str, str]] = []

    def add(self, check: str, location: str, message: str):
        self.items.append((check, location, message))

    def warn(self, check: str, location: str, message: str):
        self.warnings.append((check, location, message))

    def __len__(self):
        return len(self.items)


# ---------------------------------------------------------------------------
# Manifest
# ---------------------------------------------------------------------------
def load_manifest(path: Path):
    block = path.read_text(encoding="utf-8")
    if _USE_YAML:
        data = _yaml.safe_load(block) or {}
    else:
        # Manifest is structured nested YAML; require PyYAML for it. If absent,
        # signal "can't parse" so the caller treats C10/C10b as skipped-warn.
        return None
    return data if isinstance(data, dict) else {}


# ---------------------------------------------------------------------------

__all__ = [name for name in globals() if not name.startswith("__")]
