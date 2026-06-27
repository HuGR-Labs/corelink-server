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
CONTENT of lines Lx..Ly of `path` between the concept's `checkpoint_sha` and HEAD
(each line trailing-whitespace-stripped, internal whitespace preserved). If the
content the author cited no longer occupies those exact lines, the concept is
STALE on that cite. This catches an in-range edit AND — critically — a pure
POSITION-SHIFT (code inserted ABOVE the cited range), which slides the cite off
its authored content WITHOUT the cited line numbers ever appearing in a diff hunk
(the blind spot of the old two-tree `git diff` ∩ cited-range mechanism this
replaces). We NEVER use `git log -L` / blame. Per-file fast skip: when the file
blob is byte-identical at checkpoint and HEAD, no content can have drifted.

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
    `checkpoint_sha` and HEAD (each line trailing-whitespace-stripped). This
    fires both when the cited lines were edited in place AND when a pure
    position-shift (an insertion above) slid the authored content off those line
    numbers. False (trivially fresh) when the file blob is byte-identical at both
    revs. A file added/removed between the revs is incomparable -> reported drifted.
    """
    ckpt_blob = git.file_blob_sha(checkpoint_sha, path)
    head_blob = git.file_blob_sha("HEAD", path)
    if ckpt_blob is not None and head_blob is not None and ckpt_blob == head_blob:
        return False  # file unchanged checkpoint->HEAD: no content could drift
    ckpt_lines = git.show_lines(checkpoint_sha, path)
    head_lines = git.show_lines("HEAD", path)
    if ckpt_lines is None or head_lines is None:
        return True  # added/removed between revs — conservatively stale
    return _norm_block(ckpt_lines, l1, l2) != _norm_block(head_lines, l1, l2)


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
            secs = _sections(c.body)
            for title in ("how it works", "invariants"):
                if title in secs:
                    for block in _bullet_blocks(secs[title]):
                        if not _has_cite(block):
                            first = block.splitlines()[0].strip()
                            fails.add(
                                "C6c",
                                loc,
                                f"ungrounded claim under `# {title.title()}` (no path:line): {first[:70]!r}",
                            )

        # C5: freshness — CONTENT-ANCHOR. For each cited range compare the
        # CONTENT of those exact lines between checkpoint and HEAD; drift fires
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
    # Real W-MANIFEST schema: top-level `candidates:` (id/type/title/status/
    # source_cluster/seed_from) + `excludes:` (surface/reason). A candidate maps
    # to repo surface via its `seed_from` paths; C10b coverage = seed_from ∪ excludes.
    entries = data.get("candidates") or data.get("concepts") or []
    seed_paths: set[str] = set()
    for e in entries:
        if not isinstance(e, dict):
            continue
        cid = e.get("id")
        status = (e.get("status") or "").strip().lower()
        if status == "planned":
            args._planned_ids.add(cid)
        for s in (e.get("seed_from") or e.get("covers") or []):
            seed_paths.add(str(s))
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

    def is_covered(rel: str, is_dir: bool) -> bool:
        if rel in seed_paths:
            return True
        # a directory surface (a crate) is covered if any seed_from path is the
        # dir itself or lives under it
        if is_dir and any(s == rel or s.startswith(rel.rstrip("/") + "/") for s in seed_paths):
            return True
        if any(rel == ex or rel.startswith(ex.rstrip("/") + "/") for ex in excludes):
            return True
        return False

    surface: list[tuple[str, bool]] = []
    adr_dir = surface_root / "specs" / "03_architecture" / "adrs"
    if adr_dir.is_dir():
        surface += [(p.relative_to(surface_root).as_posix(), False) for p in sorted(adr_dir.glob("*.md"))]
    routes_dir = surface_root / "crates" / "corelink-container" / "src" / "routes"
    if routes_dir.is_dir():
        surface += [(p.relative_to(surface_root).as_posix(), False) for p in sorted(routes_dir.glob("*.rs"))]
    crates_dir = surface_root / "crates"
    if crates_dir.is_dir():
        surface += [(p.relative_to(surface_root).as_posix(), True) for p in sorted(crates_dir.iterdir()) if p.is_dir()]

    for rel, is_dir in surface:
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
