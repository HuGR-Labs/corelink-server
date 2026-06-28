#!/usr/bin/env python3
"""
Validate the OKF-CoreLink knowledge bundle against the FROZEN profile contract.

This is the completeness oracle for the knowledge wiki (W-VALIDATOR). It is a
TRANSCRIPTION of the frozen contract at
`docs/internal/okf-wiki/01-okf-corelink-profile.contract.md` — §2 (frontmatter
schema), §3 (body conventions), §4 (checks C1-C10b + secondary C-AGE/C-REV),
§4.1 (ADR/doc sub-profile), §5 (invariants). It does NOT design beyond it.

All STRUCTURAL checks validate the working tree at HEAD (the thing being gated);
`checkpoint_sha` is used ONLY as the content baseline for the C5 freshness check.

C5 freshness mechanism (CONTENT-ANCHOR): for each cited `path:Lx-Ly`, compare the
CONTENT of lines Lx..Ly of `path` between the concept's `checkpoint_sha` and the
on-disk WORKING TREE (each line trailing-whitespace-stripped, internal whitespace
preserved). If the content the author cited no longer occupies those exact lines,
the concept is STALE on that cite. This catches an in-range edit AND — critically
— a pure POSITION-SHIFT (code inserted ABOVE the cited range), which slides the
cite off its authored content WITHOUT the cited line numbers ever appearing in a
diff hunk (the blind spot of the old two-tree `git diff` ∩ cited-range mechanism
this replaces). The HEAD side is read from the WORKING TREE — the SAME tree C3
(file-exists) and C6 (line-bounds) read — so a dirty/pre-commit run is self-
consistent (worktree≠HEAD can't produce a false verdict); in CI worktree==HEAD so
behavior is unchanged. We NEVER use `git log -L` / blame. Per-file fast skip: when
the file is byte-identical at checkpoint and in the working tree, no content can
have drifted.

Usage:
    python3 scripts/validate_okf.py                       # default bundle docs/knowledge/
    python3 scripts/validate_okf.py --bundle <dir>
    python3 scripts/validate_okf.py --manifest <yaml>     # enable C10/C10b
    python3 scripts/validate_okf.py --nightly             # also run C-AGE/C-REV (WARN)

Exit contract (mirrors validate_specs.py):
    0        -> "✅ OKF-CoreLink profile valid: <N> concepts, <D> deferred, 0 stale, 0 drift"
    non-zero -> per-offender list grouped by check ID, then
                "⛔ OKF-CoreLink profile INVALID: <k> failures"

Dependencies: Python stdlib + PyYAML if available. When PyYAML is absent (or
OKF_NO_YAML=1 is set) a minimal hand-rolled YAML-subset reader is used — the
frontmatter schema only uses top-level scalars and block/inline lists, exactly
the house approach in scripts/validate_specs.py.
"""

from __future__ import annotations

import argparse
import os
import re
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

# ---------------------------------------------------------------------------
# Optional YAML — hand-rolled fallback keeps the gate self-contained on
# ubuntu-latest (no pip step required).
# ---------------------------------------------------------------------------
_USE_YAML = False
if not os.environ.get("OKF_NO_YAML"):
    try:
        import yaml as _yaml  # type: ignore

        _USE_YAML = True
    except Exception:  # pragma: no cover - environment dependent
        _USE_YAML = False


FRONT_MATTER_RE = re.compile(r"^---\n(.*?)\n---\n?(.*)$", re.DOTALL)
# A code-anchor cite token, parsed only from inside backticks: `path:line[-line]`.
CITE_RE = re.compile(r"^(?P<path>[A-Za-z0-9._/\-]+):(?P<l1>\d+)(?:-(?P<l2>\d+))?$")
BACKTICK_RE = re.compile(r"`([^`]+)`")
# Bundle-relative markdown link with a leading slash: [text](/dir/x.md)
LINK_RE = re.compile(r"\[[^\]]*\]\((/[^)\s]+)\)")
HEADING_RE = re.compile(r"^(#{1,6})\s+(.*?)\s*$")
LIST_ITEM_RE = re.compile(r"^\s*(?:[-*]|\d+\.)\s+")
HEX40_RE = re.compile(r"^[0-9a-fA-F]{40}$")

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

    def run(self, args: list[str]) -> subprocess.CompletedProcess:
        return subprocess.run(
            ["git"] + args,
            cwd=str(self.repo_root),
            capture_output=True,
            text=True,
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
        key = (rev, repo_rel)
        if key in self._blob_cache:
            return self._blob_cache[key]
        cp = self.run(["rev-parse", "--verify", "--quiet", f"{rev}:{repo_rel}"])
        val = cp.stdout.strip() if cp.returncode == 0 and cp.stdout.strip() else None
        self._blob_cache[key] = val
        return val

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
        if repo_rel in self._wt_blob_cache:
            return self._wt_blob_cache[repo_rel]
        p = self.repo_root / repo_rel
        if not p.exists():
            self._wt_blob_cache[repo_rel] = None
            return None
        cp = self.run(["hash-object", "--", repo_rel])
        val = cp.stdout.strip() if cp.returncode == 0 and cp.stdout.strip() else None
        self._wt_blob_cache[repo_rel] = val
        return val

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

    def show_file(self, rev: str, repo_rel: str):
        cp = self.run(["show", f"{rev}:{repo_rel}"])
        if cp.returncode != 0:
            return None
        return cp.stdout


# ---------------------------------------------------------------------------
# Concept model
# ---------------------------------------------------------------------------
class Concept:
    def __init__(self, path: Path, bundle_root: Path):
        self.path = path
        self.bundle_root = bundle_root
        self.rel = path.relative_to(bundle_root).as_posix()  # e.g. auth/x.md
        self.concept_id = self.rel[:-3] if self.rel.endswith(".md") else self.rel
        self.text = path.read_text(encoding="utf-8")
        fm_block, body = _split(self.text)
        self.has_frontmatter = fm_block is not None
        self.parse_error: str | None = None
        self.fm: dict = {}
        if fm_block is not None:
            try:
                self.fm = parse_frontmatter(fm_block)
            except Exception as exc:  # noqa: BLE001
                self.parse_error = str(exc)
        self.body = body or ""
        self.is_deferred = bool(self.fm.get("deferred"))
        self.type = (self.fm.get("type") or "").strip() if isinstance(self.fm.get("type"), str) else self.fm.get("type")
        self.title = self.fm.get("title")
        sf = self.fm.get("source_files")
        self.source_files = [s for s in sf if isinstance(s, str)] if isinstance(sf, list) else []
        self.checkpoint_sha = self.fm.get("checkpoint_sha")
        self.is_adr = (self.type == ADR_TYPE) or self.concept_id.startswith("adr/")
        # parse cites + links
        self.cites = _collect_cites(self.body)  # list[(file, l1, l2)]
        self.cited_files = {c[0] for c in self.cites}
        self.links = [m for m in LINK_RE.findall(self.body)]


def _split(text: str):
    m = FRONT_MATTER_RE.match(text)
    if not m:
        return None, text
    return m.group(1), m.group(2)


def _collect_cites(body: str):
    out = []
    for inner in BACKTICK_RE.findall(body):
        m = CITE_RE.match(inner.strip())
        if not m:
            continue
        l1 = int(m.group("l1"))
        l2 = int(m.group("l2")) if m.group("l2") else l1
        if l2 < l1:
            l1, l2 = l2, l1
        out.append((m.group("path"), l1, l2))
    return out


def _norm_block(lines: list[str], l1: int, l2: int) -> list[str]:
    """1-based inclusive slice of `lines`, each line trailing-whitespace-stripped
    (internal whitespace preserved — the comparison stays faithful)."""
    return [ln.rstrip() for ln in lines[l1 - 1 : l2]]


def cited_range_drifted(git: "Git", checkpoint_sha: str, path: str, l1: int, l2: int) -> bool:
    """CONTENT-ANCHOR C5 predicate (shared by validate_okf and okf_reconcile so
    the two tools can never disagree on what "stale" means).

    True iff the CONTENT of lines l1..l2 of `path` differs between
    `checkpoint_sha` and the on-disk WORKING TREE (each line trailing-whitespace-
    stripped). This fires both when the cited lines were edited in place AND when
    a pure position-shift (an insertion above) slid the authored content off those
    line numbers. False (trivially fresh) when the file is byte-identical between
    the checkpoint and the working tree. A file added/removed between the two is
    incomparable -> reported drifted.

    The HEAD side is read from the WORKING TREE — the SAME tree C3 (file-exists)
    and C6 (line-bounds) validate — not `git show HEAD:path`. In CI the worktree
    equals HEAD so behavior is identical; on a dirty/pre-commit run it makes C5
    agree with C3/C6 (no false-negative when a cited source is edited-but-
    uncommitted, no false-positive when a source is added on disk but not in HEAD).
    """
    ckpt_blob = git.file_blob_sha(checkpoint_sha, path)
    wt_blob = git.worktree_blob_sha(path)
    if ckpt_blob is not None and wt_blob is not None and ckpt_blob == wt_blob:
        return False  # file unchanged checkpoint->working-tree: no content could drift
    ckpt_lines = git.show_lines(checkpoint_sha, path)
    wt_lines = git.worktree_lines(path)
    if ckpt_lines is None or wt_lines is None:
        return True  # added/removed between checkpoint and working tree — conservatively stale
    return _norm_block(ckpt_lines, l1, l2) != _norm_block(wt_lines, l1, l2)


def _sections(body: str) -> dict[str, list[str]]:
    """Map normalized level-1/2 heading title -> list of its lines."""
    sections: dict[str, list[str]] = {}
    cur = None
    for line in body.splitlines():
        h = HEADING_RE.match(line)
        if h:
            cur = h.group(2).strip().lower()
            sections[cur] = []
            continue
        if cur is not None:
            sections[cur].append(line)
    return sections


def _bullet_blocks(lines: list[str]) -> list[str]:
    blocks: list[str] = []
    cur: list[str] | None = None
    for line in lines:
        if LIST_ITEM_RE.match(line):
            if cur is not None:
                blocks.append("\n".join(cur))
            cur = [line]
        elif line.strip() == "":
            if cur is not None:
                blocks.append("\n".join(cur))
                cur = None
        else:
            if cur is not None:
                cur.append(line)
    if cur is not None:
        blocks.append("\n".join(cur))
    return blocks


def _has_cite(text: str) -> bool:
    return any(CITE_RE.match(inner.strip()) for inner in BACKTICK_RE.findall(text))


def _block_cite_paths(text: str) -> list[str]:
    """The cited file paths (no line numbers) inside a single bullet block."""
    out: list[str] = []
    for inner in BACKTICK_RE.findall(text):
        m = CITE_RE.match(inner.strip())
        if m:
            out.append(m.group("path"))
    return out


def _is_test_cite(path: str) -> bool:
    """True iff a cited path points at TEST code, not a runtime enforcer.

    Rust — any of: a `tests/` directory anywhere; the bare module files
    `test.rs` / `tests.rs`; a `*_test.rs` / `*_tests.rs` file (the underscore-
    separated suffix form, `foo_test.rs` / `foo_tests.rs`); a `test_*.rs` /
    `tests_*.rs` file (the prefix form). TS/JS — `*.test.ts` / `*.spec.ts`, or
    anything under a `__tests__/` directory. C6c uses this to forbid grounding an
    invariant SOLELY on a test (a test can be neutered later; an invariant must
    point at the non-test code that enforces it). Tests remain valid as ADDITIONAL
    cites.

    (fix #4 — audit #3 MED — a SEPARATOR BOUNDARY is now required: the old bare
    `endswith("test.rs"/"tests.rs")` matched mid-word, misclassifying real
    enforcers `attest.rs` / `latest.rs` / `contest.rs` / `protest.rs` as tests.
    A test suffix only counts when it is the WHOLE stem or follows a `_`.)
    """
    p = path.lower()
    base = p.rsplit("/", 1)[-1]
    if p.startswith("tests/") or "/tests/" in p or "/__tests__/" in p:
        return True
    if base.endswith(".rs"):
        stem = base[:-3]
        # the bare module files, exactly
        if stem in ("test", "tests"):
            return True
        # the underscore-SEPARATED suffix form: `<word>_test` / `<word>_tests`
        # (boundary required — `attest`/`latest`/`contest` have no `_` separator
        # before the suffix, so they are NOT tests).
        if stem.endswith("_test") or stem.endswith("_tests"):
            return True
        # the prefix form: `test_*` / `tests_*` inline-test modules.
        if stem.startswith("test_") or stem.startswith("tests_"):
            return True
        return False
    if base.endswith(".test.ts") or base.endswith(".spec.ts"):
        return True
    return False


def _glob_to_regex(pattern: str) -> "re.Pattern[str]":
    """Compile a path glob to a regex with PROPER segment semantics (fix #6,
    audit #3 HIGH): a single `*` matches any run of NON-`/` chars (it does NOT
    cross a path separator), `?` matches one non-`/` char, and `**` is the ONLY
    token that crosses segments (`**/` = "zero or more leading segments", a bare
    `**` = "anything including `/`"). Character classes `[...]` are passed through.

    The old matcher used Python's `fnmatch`, whose `*` matches `/`, so `**/tests/**`,
    `apps/*/src/types/**`, and `*.config.ts` silently swallowed ANY path that
    merely CONTAINED those fragments anywhere — a real handler at
    `routes/tests/realhandler.rs` would be excluded by `**/tests/**`, and a
    `*.config.ts` exclude would swallow `a/b/foo.config.ts`. Segment-aware
    matching restricts `*` to a single segment so only genuinely-matching paths
    are excluded.
    """
    i, n = 0, len(pattern)
    out = ["^"]
    while i < n:
        c = pattern[i]
        if c == "*":
            if i + 1 < n and pattern[i + 1] == "*":
                # `**` — crosses segments.
                # `**/` at a boundary = zero-or-more whole leading segments.
                if i + 2 < n and pattern[i + 2] == "/":
                    out.append("(?:[^/]+/)*")
                    i += 3
                    continue
                # a bare/trailing `**` = anything, including `/`.
                out.append(".*")
                i += 2
                continue
            # single `*` — one segment, never `/`.
            out.append("[^/]*")
            i += 1
            continue
        if c == "?":
            out.append("[^/]")
            i += 1
            continue
        if c == "[":
            j = i + 1
            if j < n and pattern[j] in ("!", "^"):
                j += 1
            if j < n and pattern[j] == "]":
                j += 1
            while j < n and pattern[j] != "]":
                j += 1
            if j >= n:
                # unterminated class — treat `[` literally
                out.append(re.escape("["))
                i += 1
                continue
            cls = pattern[i + 1 : j]
            if cls and cls[0] in ("!", "^"):
                cls = "^" + cls[1:]
            out.append("[" + cls + "]")
            i = j + 1
            continue
        out.append(re.escape(c))
        i += 1
    out.append("$")
    return re.compile("".join(out))


def _segment_glob_match(rel: str, pattern: str) -> bool:
    """True iff `rel` matches the path glob `pattern` under SEGMENT-AWARE
    semantics (see `_glob_to_regex`). `**/<tail>` also matches a bare `<tail>`
    with no leading directory (the "zero leading segments" arm of `**/`), so
    `**/*.test.ts` still matches a top-level `foo.test.ts`."""
    return _glob_to_regex(pattern).match(rel) is not None


def _line_count(path: Path) -> int:
    try:
        with path.open("rb") as fh:
            return sum(1 for _ in fh)
    except OSError:
        return 0


# ---------------------------------------------------------------------------
# Failure collection
# ---------------------------------------------------------------------------
class Failures:
    ORDER = [
        "C1", "C2", "C3", "C4", "C5", "C5b", "C6", "C6b", "C6c",
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
# Checks
# ---------------------------------------------------------------------------
def run_checks(args, git: Git, fails: Failures):
    repo_root = git.repo_root
    bundle_root = Path(args.bundle)
    if not bundle_root.is_absolute():
        bundle_root = (Path.cwd() / bundle_root).resolve()

    concepts: list[Concept] = []
    reserved_files: list[Path] = []
    deferred_ids: set[str] = set()

    if bundle_root.is_dir():
        for md in sorted(bundle_root.rglob("*.md")):
            rel = md.relative_to(bundle_root).as_posix()
            # reserved files only at bundle root
            if "/" not in rel and md.name in RESERVED_NAMES:
                reserved_files.append(md)
                continue
            concepts.append(Concept(md, bundle_root))

    # --- C9: reserved files must not be used as concepts (carry source_files) ---
    for rf in reserved_files:
        block, _ = _split(rf.read_text(encoding="utf-8"))
        if block is not None:
            try:
                fm = parse_frontmatter(block)
            except Exception:
                fm = {}
            if "source_files" in fm or "checkpoint_sha" in fm:
                fails.add(
                    "C9",
                    rf.relative_to(repo_root).as_posix() if _under(rf, repo_root) else rf.name,
                    "reserved file used as a concept (declares source_files/checkpoint_sha)",
                )

    # Per-concept structural checks.
    for c in concepts:
        loc = f"{bundle_root.name}/{c.rel}" if not _under(c.path, repo_root) else c.path.relative_to(repo_root).as_posix()

        # C9: a concept must not occupy a reserved root path (auto-excluded above,
        # but a subdir file literally named with reserved name is allowed; nothing).

        # C1: parseable frontmatter + non-empty type
        if not c.has_frontmatter:
            fails.add("C1", loc, "no parseable YAML frontmatter")
            continue
        if c.parse_error:
            fails.add("C1", loc, f"unparseable frontmatter: {c.parse_error}")
            continue
        if not c.type:
            fails.add("C1", loc, "missing/empty `type`")
            # keep going for other field checks

        if c.is_deferred:
            deferred_ids.add(c.concept_id)
            # deferred: exempt from grounding/body, but MUST have type + title
            if not c.title:
                fails.add("C2", loc, "deferred concept missing `title`")
            continue

        # C2: required fields on a non-deferred concept
        if not c.title:
            fails.add("C2", loc, "missing required field `title`")
        if not c.source_files:
            fails.add("C2", loc, "missing required field `source_files` (>=1)")
        if not c.checkpoint_sha:
            fails.add("C2", loc, "missing required field `checkpoint_sha`")

        # C3: every source_files path exists verbatim at HEAD (no rename-follow)
        missing_sources: set[str] = set()
        for sf in c.source_files:
            if not (repo_root / sf).exists():
                missing_sources.add(sf)
                fails.add("C3", loc, f"source_files path does not exist at HEAD: `{sf}`")

        # C4: checkpoint_sha 40-hex and exists in git history
        ckpt_ok = False
        if c.checkpoint_sha:
            if not isinstance(c.checkpoint_sha, str) or not HEX40_RE.match(c.checkpoint_sha):
                fails.add("C4", loc, f"checkpoint_sha is not 40-hex: `{c.checkpoint_sha}`")
            elif not git.sha_exists(c.checkpoint_sha):
                fails.add("C4", loc, f"checkpoint_sha not found in git history: `{c.checkpoint_sha}`")
            else:
                ckpt_ok = True

        # Build cited-line ranges per file (HEAD coordinates).
        ranges_by_file: dict[str, list[tuple[int, int]]] = {}
        for (f, l1, l2) in c.cites:
            ranges_by_file.setdefault(f, []).append((l1, l2))

        # C6: inline cites resolve (file exists at HEAD + lines in bounds);
        #     every source_files path is cited.
        for (f, l1, l2) in c.cites:
            if f in missing_sources:
                continue  # already reported under C3; don't double-count
            fpath = repo_root / f
            if not fpath.exists():
                fails.add("C6", loc, f"dangling citation — file does not exist: `{f}:{l1}`")
                continue
            n = _line_count(fpath)
            if l1 < 1 or l2 > n:
                fails.add("C6", loc, f"citation out of range: `{f}:{l1}-{l2}` (file has {n} lines)")
        cited_set = {f for (f, _, _) in c.cites}
        for sf in c.source_files:
            if sf in missing_sources:
                continue
            if sf not in cited_set:
                fails.add("C6", loc, f"declared source not cited under `# Citations`: `{sf}`")

        # C6b: every cited file must be declared in source_files
        for f in cited_set:
            if f not in set(c.source_files):
                fails.add("C6b", loc, f"cited file not declared in source_files: `{f}`")

        # C6c: per-claim grounding under `# How it works` and `# Invariants`
        #      (relaxed for ADRs per §4.1)
        if not c.is_adr:
            # A `testing/` concept's SUBJECT is the test harness, so grounding an
            # invariant on a test path is correct there — the test IS the enforcer
            # (parallel to C6c being relaxed for ADRs per §4.1). The exemption is
            # FILE-DIR based (fix #4): ONLY a concept whose doc physically lives
            # under `docs/knowledge/testing/` is exempt. The old `type ==
            # "TestStrategy"` escape was a taxonomy dodge — any concept anywhere
            # (e.g. under auth/) could self-declare `type: TestStrategy` and shed
            # the non-test-enforcer requirement. Anchoring on the file's directory
            # makes the exemption un-spoofable from frontmatter.
            is_testing = (c.rel == "testing" or c.rel.startswith("testing/"))
            secs = _sections(c.body)
            for title in ("how it works", "invariants"):
                if title in secs:
                    for block in _bullet_blocks(secs[title]):
                        paths = _block_cite_paths(block)
                        first = block.splitlines()[0].strip()
                        if not paths:
                            fails.add(
                                "C6c",
                                loc,
                                f"ungrounded claim under `# {title.title()}` (no path:line): {first[:70]!r}",
                            )
                        elif not is_testing and all(_is_test_cite(p) for p in paths):
                            # An invariant must be grounded on a NON-test enforcer:
                            # a test path (isolation_tests.rs:42 …) as the SOLE cite
                            # lets the grounding be neutered later by editing the
                            # test. Tests are allowed only as ADDITIONAL cites.
                            fails.add(
                                "C6c",
                                loc,
                                f"test-only grounding under `# {title.title()}` "
                                f"(an invariant must cite a non-test enforcer; sole cite(s) {paths!r} "
                                f"are all test paths): {first[:70]!r}",
                            )

        # C5: freshness — CONTENT-ANCHOR. For each cited range compare the
        # CONTENT of those exact lines between checkpoint and the working tree;
        # drift fires
        # whether the cause is an in-range edit OR a pure position-shift.
        if ckpt_ok:
            for sf in c.source_files:
                if sf in missing_sources:
                    continue
                # ADR sub-profile: an accepted ADR does not go stale on the code
                # it governs — C5 applies only to the ADR file's own content.
                if c.is_adr and not sf.endswith(".md"):
                    continue
                cranges = ranges_by_file.get(sf, [])
                if not cranges:
                    continue
                for (l1, l2) in cranges:
                    if cited_range_drifted(git, c.checkpoint_sha, sf, l1, l2):
                        fails.add(
                            "C5",
                            loc,
                            f"STALE: cited content `{sf}:{l1}-{l2}` no longer matches "
                            f"checkpoint {c.checkpoint_sha[:12]} "
                            "(in-range edit or position-shift)",
                        )
                        break

    # --- C5b: SHA advanced without a body edit (needs a 'previous' version) ---
    _check_c5b(args, git, bundle_root, concepts, fails)

    # --- C10 / C10b: manifest (skip-with-warning when absent/unparseable) ---
    # Runs before C7 because it populates the planned-id set used by C7 tolerance.
    _check_manifest(args, bundle_root, concepts, deferred_ids, fails)

    # --- C7: bundle-relative links to declared concepts resolve ---
    # --- C8: every concept reachable from index.md ---
    _check_links_and_orphans(bundle_root, concepts, deferred_ids, args, fails)

    # --- Secondary nightly WARN checks ---
    if args.nightly:
        _check_nightly(git, concepts, fails)

    # Counts for the success string.
    n_concepts = sum(1 for c in concepts if not c.is_deferred and c.has_frontmatter)
    n_deferred = sum(1 for c in concepts if c.is_deferred)
    return n_concepts, n_deferred


def _check_c5b(args, git: Git, bundle_root: Path, concepts: list[Concept], fails: Failures):
    base_bundle = Path(args.base_bundle).resolve() if args.base_bundle else None
    base_rev = None
    if base_bundle is None:
        mb = git.merge_base(args.base_ref)
        base_rev = mb

    for c in concepts:
        if c.is_deferred or not c.checkpoint_sha:
            continue
        prev_text = None
        if base_bundle is not None:
            prev_path = base_bundle / c.rel
            if prev_path.exists():
                prev_text = prev_path.read_text(encoding="utf-8")
        elif base_rev and _under(c.path, git.repo_root):
            repo_rel = c.path.relative_to(git.repo_root).as_posix()
            prev_text = git.show_file(base_rev, repo_rel)
        if prev_text is None:
            continue  # new concept — nothing was advanced
        prev_block, _ = _split(prev_text)
        if prev_block is None:
            continue
        try:
            prev_fm = parse_frontmatter(prev_block)
        except Exception:
            continue
        prev_sha = prev_fm.get("checkpoint_sha")
        if not prev_sha or prev_sha == c.checkpoint_sha:
            continue  # not advanced
        # SHA changed: require a real body edit. Compare both texts with the
        # checkpoint_sha line stripped — if identical, only the SHA moved.
        if _strip_ckpt_line(prev_text) == _strip_ckpt_line(c.text):
            loc = c.path.relative_to(git.repo_root).as_posix() if _under(c.path, git.repo_root) else f"{bundle_root.name}/{c.rel}"
            fails.add(
                "C5b",
                loc,
                f"checkpoint_sha advanced ({str(prev_sha)[:12]} -> {c.checkpoint_sha[:12]}) "
                "without any body edit (phantom reconcile)",
            )


def _strip_ckpt_line(text: str) -> str:
    return "\n".join(
        ln for ln in text.splitlines() if not ln.strip().startswith("checkpoint_sha:")
    )


def _check_links_and_orphans(
    bundle_root: Path, concepts: list[Concept], deferred_ids: set[str], args, fails: Failures
):
    repo_root = args._repo_root
    by_id = {c.concept_id: c for c in concepts}
    existing_targets = {c.rel for c in concepts}
    planned = args._planned_ids  # set populated by manifest load (may be empty)

    def loc_of(c: Concept):
        return c.path.relative_to(repo_root).as_posix() if _under(c.path, repo_root) else f"{bundle_root.name}/{c.rel}"

    # C7: each bundle-relative link to a declared concept resolves.
    index_path = bundle_root / "index.md"
    link_sources: list[tuple[str, list[str]]] = []
    if index_path.exists():
        link_sources.append(("index.md", LINK_RE.findall(index_path.read_text(encoding="utf-8"))))
    for c in concepts:
        link_sources.append((loc_of(c), c.links))

    for src, links in link_sources:
        for target in links:
            rel = target.lstrip("/")
            if not rel.endswith(".md"):
                continue
            cid = rel[:-3]
            if rel in existing_targets:
                continue  # resolves
            # tolerate links to not-yet-written / deferred / planned concepts
            if cid in deferred_ids or cid in planned:
                continue
            fails.add("C7", src, f"broken link to declared concept: `/{rel}`")

    # C8: reachability from index.md
    if concepts:
        reachable: set[str] = set()
        if index_path.exists():
            frontier = [
                t.lstrip("/")[:-3]
                for t in LINK_RE.findall(index_path.read_text(encoding="utf-8"))
                if t.lstrip("/").endswith(".md")
            ]
        else:
            frontier = []
        while frontier:
            cid = frontier.pop()
            if cid in reachable:
                continue
            reachable.add(cid)
            node = by_id.get(cid)
            if node:
                for t in node.links:
                    r = t.lstrip("/")
                    if r.endswith(".md"):
                        frontier.append(r[:-3])
        for c in concepts:
            if c.concept_id not in reachable:
                fails.add("C8", loc_of(c), "orphan concept — not reachable from index.md")


def _check_manifest(args, bundle_root: Path, concepts, deferred_ids, fails: Failures):
    manifest_path = Path(args.manifest) if args.manifest else None
    if manifest_path is None or not manifest_path.exists():
        fails.warn("C10", str(args.manifest or "concept-manifest.yaml"),
                   "manifest absent — C10/C10b skipped")
        return
    data = load_manifest(manifest_path)
    if data is None:
        fails.warn("C10", str(manifest_path), "manifest unparseable without PyYAML — C10/C10b skipped")
        return

    concept_ids = {c.concept_id for c in concepts}
    concept_by_id = {c.concept_id: c for c in concepts}

    def _concept_grounds(c: "Concept", seed: str) -> bool:
        """True iff concept `c` ACTUALLY grounds the surface `seed` — i.e. `seed`
        appears in the concept's resolved `source_files` OR in its `# Citations`
        (the inline-cited files), under one of these GENUINE-coverage relations:

          (a) verbatim — `seed` is declared/cited as-is;
          (b) `seed` is a directory containing a declared/cited path;
          (c) `seed` is a file/dir under a declared/cited directory;
          (d) crate-cluster ADOPTION — a `type: CrateCluster` concept that grounds
              a crate directory adopts the whole crate as its narrative home, so a
              file/dir under that crate is covered even without a per-file cite
              (the §1.1 taxonomy assigns `crates/` to "the 8 crate clusters"; a
              cluster's job is breadth-of-narrative, not file-granular citation).

        This is the anti-self-certification check (fix #3 / audit-round R): a
        `seed_from` entry counts as coverage ONLY when the concept it names really
        explains the surface — a manifest can no longer silently "cover" a file by
        listing it under a concept that never mentions it (the silent-gap bypass).

        REMOVED in gate v5 (audit #3 HIGH #2): the former Rust "module-parent"
        relation auto-grounded an ENTIRE module subtree from a single parent-file
        cite — a concept citing `routes.rs` (the `mod routes;` declaration site)
        auto-covered ANY `routes/*.rs` added to its `seed_from` WITHOUT the concept
        ever mentioning that handler. That is the same self-certification
        relations (a)-(c) exist to forbid, one level deeper: `X.rs` is the module
        DECLARATION, not an explanation of `X/sub.rs`'s behaviour. A file under a
        declared module is now covered ONLY when the concept declares/cites that
        file (a), or grounds a DIRECTORY that contains it (b/c — a `routes/` or
        `routes/dsr/` directory seed, an explicit narrative-home decision), or is
        the crate-cluster adopting the whole crate (d). The `X.rs`-as-module-root
        shortcut is gone — cite the file or seed its directory, do not lean on the
        `mod` declaration to self-certify the subtree.
        """
        grounded = set(c.source_files) | set(c.cited_files)
        s = seed.rstrip("/")
        is_cluster = (c.type == "CrateCluster")
        for g in grounded:
            gn = g.rstrip("/")
            if s == gn:
                return True
            # (b) seed is a directory that contains a grounded path
            if gn == s or gn.startswith(s + "/"):
                return True
            # (c) seed lives under a grounded directory
            if s.startswith(gn + "/"):
                return True
            # (d) crate-cluster adoption: a CrateCluster grounding a crate dir
            #     adopts everything under that crate (its src files + subdirs).
            if is_cluster:
                m = re.match(r"^(crates/[^/]+)/", gn + "/")
                if m:
                    crate = m.group(1)
                    if s == crate or s.startswith(crate + "/"):
                        return True
        return False

    # Real W-MANIFEST schema: top-level `candidates:` (id/type/title/status/
    # source_cluster/seed_from) + `excludes:` (surface/reason). A candidate maps
    # to repo surface via its `seed_from` paths; C10b coverage = seed_from ∪ excludes.
    # A `seed_from` path counts as COVERAGE only when the candidate's concept doc
    # actually GROUNDS that path (declares/cites it) — a seed naming a concept that
    # doesn't mention the file is self-certifying and is REJECTED (fix #3).
    entries = data.get("candidates") or data.get("concepts") or []
    seed_paths: set[str] = set()           # every declared seed (for diagnostics)
    grounded_seed_paths: set[str] = set()  # seeds a real concept grounds — THESE cover
    for e in entries:
        if not isinstance(e, dict):
            continue
        cid = e.get("id")
        status = (e.get("status") or "").strip().lower()
        if status == "planned":
            args._planned_ids.add(cid)
        c = concept_by_id.get(cid)
        for s in (e.get("seed_from") or e.get("covers") or []):
            sp = str(s)
            seed_paths.add(sp)
            # The seed grounds coverage iff its declaring concept exists AND
            # actually declares/cites the path. A planned (doc-less) candidate's
            # seed cannot self-certify coverage — there is no concept to ground it.
            if c is not None and _concept_grounds(c, sp):
                grounded_seed_paths.add(sp)
        # C10: active candidate must have a doc or a deferred marker (deferred
        # docs are scanned as concepts, so concept_ids already covers both).
        if status == "active" and cid:
            if cid not in concept_ids:
                fails.add("C10", str(manifest_path),
                          f"active candidate `{cid}` has no concept doc and no deferred marker")

    # C10b: manifest must cover the mechanically-enumerable repo surface.
    surface_root = Path(args.surface_root).resolve()
    excludes: list[str] = []
    for ex in (data.get("excludes") or []):
        if isinstance(ex, dict):
            surf = ex.get("surface") or ex.get("path")
            if surf and ex.get("reason"):
                excludes.append(str(surf))

    # NOTE: _glob_match is defined at module scope (`_segment_glob_match`) so it
    # is unit-testable and segment-aware; bound here as a local alias.
    _glob_match = _segment_glob_match

    def is_covered(rel: str, is_dir: bool) -> bool:
        # --- DIRECTORY (crate) surface: coverage = reviewed cluster MEMBERSHIP ---
        # A crate-dir surface is covered if ANY seed_from path is the dir itself or
        # lives under it. Crate-level membership is the curated, review-gated
        # cluster assignment (taxonomy §1.1: `crates/` = "the 8 crate clusters"),
        # so it self-certifies at the COARSE crate granularity by design — the
        # nightly C-REV reverse-coverage WARN is what pressures source_files
        # completeness WITHIN a member crate. Fix #3's anti-self-certification is a
        # FILE-granular guard (a handler FILE slipped under a concept that never
        # mentions it), so it does NOT apply to a whole-crate directory surface.
        if is_dir:
            if any(s == rel or s.startswith(rel.rstrip("/") + "/") for s in seed_paths):
                return True
        else:
            # --- FILE surface: coverage requires GENUINE grounding (fix #3) -------
            # A file counts as covered ONLY by a seed whose declaring concept
            # actually grounds it (`grounded_seed_paths`, not the raw seed set): a
            # manifest entry naming a FILE under a concept that never cites/declares
            # it is self-certifying and is NOT coverage — closing the silent-gap
            # bypass where a load-bearing handler hides under an unrelated concept.
            if rel in grounded_seed_paths:
                return True
            # a file is also covered when a GROUNDED directory seed contains it
            # (the concept that adopts the dir grounds the files beneath it).
            if any(
                (s.rstrip("/") != "" and rel.startswith(s.rstrip("/") + "/"))
                for s in grounded_seed_paths
            ):
                return True
        for ex in excludes:
            exn = ex.rstrip("/")
            if rel == exn or rel.startswith(exn + "/"):
                return True
            if ("*" in ex or "?" in ex or "[" in ex) and _glob_match(rel, ex):
                return True
        return False

    surface: list[tuple[str, bool]] = []
    adr_dir = surface_root / "specs" / "03_architecture" / "adrs"
    if adr_dir.is_dir():
        surface += [(p.relative_to(surface_root).as_posix(), False) for p in sorted(adr_dir.glob("*.md"))]
    routes_dir = surface_root / "crates" / "corelink-container" / "src" / "routes"
    if routes_dir.is_dir():
        # RECURSIVE: a handler dropped in routes/dsr/, routes/audit_export/, or
        # routes/audit_analytics/ (or any future subdir) must be enumerated too —
        # a non-recursive glob("*.rs") was a silent completeness hole (a new
        # handler at routes/dsr/backdoor.rs would never be gated). Genuine
        # non-handlers (tests_*.rs) are exempted via the manifest `excludes:`.
        surface += [(p.relative_to(surface_root).as_posix(), False) for p in sorted(routes_dir.rglob("*.rs"))]
    # The container crate ROOT, file-granular: a handler dropped directly in
    # crates/corelink-container/src/ (sibling to webhook.rs / native_pat_gate.rs)
    # is invisible if the crate is gated dir-granular only. Enumerate each
    # src/**/*.rs RECURSIVELY — a non-recursive glob left the src/storage/ subtree
    # (d1_http/r2_kv/r2_s3/region_map) silently un-gated (audit #2). routes/ is
    # also under src/ but already enumerated above; the dedup (dict.fromkeys on the
    # surface list) collapses the overlap. non-handlers (lib.rs is the crate wiring
    # root) are covered via the manifest like the rest.
    container_src = surface_root / "crates" / "corelink-container" / "src"
    if container_src.is_dir():
        surface += [(p.relative_to(surface_root).as_posix(), False) for p in sorted(container_src.rglob("*.rs"))]
    crates_dir = surface_root / "crates"
    if crates_dir.is_dir():
        surface += [(p.relative_to(surface_root).as_posix(), True) for p in sorted(crates_dir.iterdir()) if p.is_dir()]

    # --- Worker EDGE PLANE + apps/ (BR5 root fix) ------------------------------
    # The completeness oracle must also gate the TypeScript edge plane and the
    # apps/ workers (file-granular), not just the Rust container + ADRs — else a
    # future load-bearing worker/src or apps/*/src file with no concept stays
    # invisible. Enumerated as completeness-required surfaces; legit-exempt files
    # (types/config/test/UI/data) live in the manifest `excludes:`.
    def _add_files(base_rel: str, pattern: str, recursive: bool = False) -> None:
        base = surface_root / base_rel
        if not base.is_dir():
            return
        globber = base.rglob if recursive else base.glob
        for p in sorted(globber(pattern)):
            if p.is_file():
                surface.append((p.relative_to(surface_root).as_posix(), False))

    _add_files("worker/src", "*.ts")                 # edge plane (top-level)
    _add_files("worker/src/lib", "*.ts")             # edge plane (lib/)
    for app in ("signup-worker", "cas-worker", "analytics-worker"):
        _add_files(f"apps/{app}/src", "*.ts", recursive=True)
    mig = surface_root / "apps" / "migrate-single-to-multi-region" / "src" / "main.rs"
    if mig.is_file():
        surface.append((mig.relative_to(surface_root).as_posix(), False))

    for rel, is_dir in dict.fromkeys(surface):
        if not is_covered(rel, is_dir):
            fails.add("C10b", str(manifest_path),
                      f"repo-surface item `{rel}` has no manifest entry and no exclude (silent gap)")


def _check_nightly(git: Git, concepts: list[Concept], fails: Failures):
    now = datetime.now(timezone.utc)
    for c in concepts:
        if c.is_deferred or not c.checkpoint_sha or not isinstance(c.checkpoint_sha, str):
            continue
        if not HEX40_RE.match(c.checkpoint_sha):
            continue
        dt = git.commit_date(c.checkpoint_sha)
        if dt is None:
            continue
        if (now - dt).days > 90:
            fails.warn("C-AGE", c.rel, f"checkpoint older than 90 days ({(now - dt).days}d) — re-attest")
    # C-REV reverse-coverage (best-effort WARN): for crate dirs that changed
    # since the oldest referenced checkpoint and are covered by NO concept's
    # source_files, surface a warning (pressure toward source_files completeness;
    # cannot be proven, only surfaced — per the contract).
    declared = set()
    for c in concepts:
        for sf in c.source_files:
            declared.add(sf)
    checkpoints = [c.checkpoint_sha for c in concepts
                   if isinstance(c.checkpoint_sha, str) and HEX40_RE.match(c.checkpoint_sha)]
    if not checkpoints:
        return
    crates_root = git.repo_root / "crates"
    if not crates_root.is_dir():
        return
    for crate in sorted(p for p in crates_root.iterdir() if p.is_dir()):
        rel = crate.relative_to(git.repo_root).as_posix()
        if any(d == rel or d.startswith(rel + "/") for d in declared):
            continue
        # changed since the oldest checkpoint?
        cp = git.run(["diff", "--quiet", checkpoints[0], "HEAD", "--", rel])
        if cp.returncode == 1:  # 1 == differences
            fails.warn("C-REV", rel, "crate changed since checkpoints but no concept declares it")


# ---------------------------------------------------------------------------
def _under(p: Path, root: Path) -> bool:
    try:
        p.relative_to(root)
        return True
    except ValueError:
        return False


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bundle", default="docs/knowledge",
                        help="bundle root to validate (default: docs/knowledge)")
    parser.add_argument("--manifest", default="docs/internal/okf-wiki/concept-manifest.yaml",
                        help="concept manifest for C10/C10b (skipped-with-warning if absent)")
    parser.add_argument("--surface-root", default=None,
                        help="root for C10b repo-surface enumeration (default: repo root)")
    parser.add_argument("--base-ref", default="main",
                        help="git ref the PR forks from, for C5b (default: main)")
    parser.add_argument("--base-bundle", default=None,
                        help="a 'before' bundle dir overriding git for C5b (fixture testing)")
    parser.add_argument("--nightly", action="store_true",
                        help="also run secondary WARN checks C-AGE/C-REV (nightly)")
    parser.add_argument("--verbose", action="store_true")
    args = parser.parse_args()

    cp = subprocess.run(["git", "rev-parse", "--show-toplevel"],
                        capture_output=True, text=True)
    if cp.returncode != 0:
        sys.exit("ERROR: not inside a git repository")
    repo_root = Path(cp.stdout.strip())
    if args.surface_root is None:
        args.surface_root = str(repo_root)
    args._repo_root = repo_root  # type: ignore[attr-defined]
    args._planned_ids = set()  # type: ignore[attr-defined]

    git = Git(repo_root)
    fails = Failures()

    # Manifest is loaded inside run_checks (it also populates planned ids used
    # by C7), but C7/C8 run after C10 in the function body, so order is fine.
    n_concepts, n_deferred = run_checks(args, git, fails)

    if fails.warnings and args.verbose:
        print("⚠️  WARNINGS (non-blocking):")
        for check, loc, msg in fails.warnings:
            print(f"  [{check}] {loc}: {msg}")

    if len(fails) == 0:
        print(
            f"✅ OKF-CoreLink profile valid: {n_concepts} concepts, "
            f"{n_deferred} deferred, 0 stale, 0 drift"
        )
        return 0

    # group by check ID, in canonical order
    print("OKF-CoreLink profile FAILURES (grouped by check):\n")
    by_check: dict[str, list[tuple[str, str]]] = {}
    for check, loc, msg in fails.items:
        by_check.setdefault(check, []).append((loc, msg))
    ordered = [c for c in Failures.ORDER if c in by_check] + [
        c for c in by_check if c not in Failures.ORDER
    ]
    for check in ordered:
        print(f"[{check}]")
        for loc, msg in by_check[check]:
            print(f"  • {loc}: {msg}")
        print()
    print(f"⛔ OKF-CoreLink profile INVALID: {len(fails)} failures")
    return 1


if __name__ == "__main__":
    sys.exit(main())
# marker-test
