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

SQUASH-MERGE RESILIENCE (C4/C5): a concept may pin `checkpoint_sha` at a PR's
pre-merge branch tip that git rewrites at squash/rebase merge, ORPHANING the
commit — unreachable from `main` and absent from the `fetch-depth: 0` CI clone.
That is not drift (the squash landing preserves the cited source byte-for-byte)
and no longer hard-fails C4; instead C4 warns and C5 re-anchors freshness to the
reachable base ref (the fork point), so a genuinely drifted cite STILL fails but
the recurring "checkpoint not found in git history" false-positive is gone.

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
        """Resolve the PR's base ref to a concrete commit id, tolerating the CI
        checkout layout (`actions/checkout` at `fetch-depth: 0`, detached HEAD)
        where the base branch exists ONLY as a remote-tracking ref
        `origin/<ref>` and NOT as a local branch — so a bare `main` would not
        resolve. Tries the ref verbatim first, then `origin/<ref>`. Returns the
        40-hex commit id, or None when neither resolves. Used to anchor the C5
        squash-orphan content fallback (below) to a REACHABLE base commit even
        when the checkout never materialised a local base branch."""
        for cand in (ref, f"origin/{ref}"):
            cp = self.run(["rev-parse", "--verify", "--quiet", f"{cand}^{{commit}}"])
            sha = cp.stdout.strip()
            if cp.returncode == 0 and sha:
                return sha
        return None

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


def _basename_is_test_or_config(rel: str) -> bool:
    """True iff the BASENAME of `rel` is GENUINELY a test or config file by its
    OWN name — independent of any ancestor directory (gate v7 fix #1).

    `_is_test_cite` answers a DIFFERENT question: it returns True for ANY file
    that merely LIVES under a `tests/`/`__tests__/` directory, regardless of the
    file's own name. That directory-membership semantics is exactly the
    exclude-glob escape hatch: a REAL request-reachable handler named
    `poison.rs` dropped under `routes/tests/` is swept up by the broad
    `**/tests/**` manifest exclude even though `poison.rs` is not itself a test.

    This predicate looks ONLY at the basename, so it answers "is THIS FILE,
    by its own name, a test/config artifact?" — which is the only thing a BROAD
    pattern exclude (a dir-name or extension glob) is allowed to auto-exclude
    inside a strict tree. A real handler name fails it and must therefore carry
    an EXPLICIT per-file `excludes:` entry (an exact, non-glob surface) or be
    covered by a grounded concept.
    """
    base = rel.rsplit("/", 1)[-1].lower()
    # Rust test-named files (stem-exact / `_test(s)` suffix / `test(s)_` prefix).
    if base.endswith(".rs"):
        stem = base[:-3]
        if stem in ("test", "tests"):
            return True
        if stem.endswith("_test") or stem.endswith("_tests"):
            return True
        if stem.startswith("test_") or stem.startswith("tests_"):
            return True
        return False
    # TS/JS test + ambient-type + tooling-config artifacts (by their own name).
    if base.endswith(".test.ts") or base.endswith(".spec.ts"):
        return True
    if base.endswith(".d.ts") or base.endswith(".config.ts") or base.endswith(".config.js"):
        return True
    if base in ("wrangler.toml", "package.json", "tsconfig.json"):
        return True
    return False


# A line is GROUNDING CODE only if it is non-blank AND not a comment/doc-comment
# line. Comment leaders we reject: Rust `//` and `//!`/`///` doc-comments, the
# `#` shell/yaml/python comment, and a `*` continuation line (the body of a
# `/* … */` or `/** … */` block, conventionally column-aligned with a leading
# `*`). A cite that resolves ONLY to such lines points at the file's narrative
# header, not at the executed behaviour it claims to ground (gate v7 fix #2).
_COMMENT_LEADERS = ("//", "#", "*", "/*")


def _line_is_code(line: str) -> bool:
    """True iff `line` is a NON-COMMENT, NON-BLANK source line (gate v7 fix #2)."""
    s = line.strip()
    if not s:
        return False
    # `//`, `///`, `//!` all start with `//`; `/*`, `/**` start with `/*`;
    # `*` is a block-comment continuation; `#` is the shell/yaml/py comment.
    for lead in _COMMENT_LEADERS:
        if s.startswith(lead):
            return False
    return True


# gate v8 fix #2 (HIGH — raise-the-bar): `_line_is_code` accepts ANY non-comment
# line as a covering cite, including pure BOILERPLATE that substantiates NOTHING —
# a `use crate::auth;` / `mod x;` / `impl X {` / a bare `}` / `});`. A backdoor
# handler cited at its `use` line therefore shipped GREEN. A SUBSTANTIVE line must
# carry actual statement/expression content, not pure import/declaration-header/
# brace scaffolding. (This only RAISES the bar — see the contract: a backdoor cited
# at a real-but-misdescribed enforcer line is STILL the C5 freshness≠correctness
# structural residual the human/panel deep-audit owns. This stops the trivial
# boilerplate-line bypass only.)
_BOILERPLATE_LEADERS = (
    # Rust imports / module wiring.
    "use ", "pub use ", "mod ", "pub mod ", "pub(crate) mod ", "extern crate ",
    # TS/JS imports + re-exports.
    "import ", "export *", "export {", "export type {", "export default {",
    "from ", "require(",
)
# A line that is ONLY a bracket/paren/brace (block scaffolding) substantiates
# nothing. Compared after stripping whitespace.
_PURE_SCAFFOLD = {
    "{", "}", "(", ")", "[", "]",
    ");", "})", "});", "],", "},", "),", ");}", "});}",
    ",", ";", "=> {", "{}", "})", "}),",
}
# TYPE / BLOCK declaration-HEADER leaders: a bare `impl X {` / `struct X {` /
# `enum X {` / `trait X {` / a bare `match …{`/`if …{`/`for …{` block opener with
# NO body on the line is pure container scaffolding, not the enforcing statement.
# We accept such a line only when it carries content BEYOND the opening brace (a
# single-line body). A NAMED FUNCTION signature (`fn NAME(…)`) is treated as
# substantive even when its body opens on the next line — it names the actual
# callable enforcer (its identity + interface), which is what a per-file cite is
# anchoring; that is NOT the boilerplate bypass this fix closes (the bypass is the
# `use`/`mod`/import/brace line, which names nothing). A backdoor cited at its own
# real handler signature line remains the C5 freshness≠correctness structural
# residual the human/panel deep-audit owns (see the contract).
_TYPE_HEADER_LEADERS = (
    "impl ", "impl<", "struct ", "pub struct ", "pub(crate) struct ",
    "enum ", "pub enum ", "pub(crate) enum ",
    "trait ", "pub trait ", "pub(crate) trait ",
    "match ", "if ", "else", "for ", "while ", "loop", "unsafe {",
    "class ", "export class ", "interface ", "export interface ",
    "namespace ", "module ",
)
# A NAMED function signature opener: substantive (names the callable enforcer)
# even when the body opens on a following line. Must carry the `(` argument list.
_FN_SIG_LEADERS = (
    "fn ", "pub fn ", "pub(crate) fn ", "async fn ", "pub async fn ",
    "pub(crate) async fn ", "const fn ", "pub const fn ", "unsafe fn ",
    "pub unsafe fn ", "function ", "export function ", "async function ",
    "export async function ",
)


def _line_is_substantive(line: str) -> bool:
    """True iff `line` is a SUBSTANTIVE code line (gate v8 fix #2): a non-comment,
    non-blank line that carries real statement/expression content — NOT a pure
    import (`use`/`mod`/`import`/`from`), NOT a line that is only brace/paren/
    bracket scaffolding (`{`,`}`,`);`,`});`,…), and NOT a bare TYPE/BLOCK
    declaration HEADER with no body on the line (`impl X {`, `struct X {`,
    `match … {` with nothing after the brace). A single-line body
    (`fn deny() { false }`) and a NAMED function signature opener (`fn router(…)`)
    are substantive."""
    if not _line_is_code(line):
        return False
    s = line.strip()
    if s in _PURE_SCAFFOLD:
        return False
    low = s.lower()
    for lead in _BOILERPLATE_LEADERS:
        if low.startswith(lead) or s.startswith(lead):
            return False
    # A named function signature (`fn NAME(…)` / `async fn NAME(…)`) names the
    # callable enforcer → substantive (its body may open on the next line).
    for lead in _FN_SIG_LEADERS:
        if s.startswith(lead) and "(" in s:
            return True
    # TYPE / BLOCK headers: substantive ONLY if there is content AFTER the opening
    # `{` (a single-line body). A bare `… {` (or no brace at all, a multi-line
    # opener) is pure container scaffolding.
    for lead in _TYPE_HEADER_LEADERS:
        if s.startswith(lead):
            brace = s.find("{")
            if brace == -1:
                return False  # opener with no body on the line
            tail = s[brace + 1:].strip()
            if tail in ("", "}"):
                return False
            return True
    return True


def _cite_is_code_line(file_lines: list[str], l1: int, l2: int) -> bool:
    """True iff the 1-based inclusive cited range `l1..l2` of `file_lines`
    contains AT LEAST ONE SUBSTANTIVE code line (gate v7 fix #2, RAISED by gate v8
    fix #2). A range that is entirely blank / comment / doc-comment lines (the
    `:1-2` file-header PoC) OR entirely pure-boilerplate (import / module-wiring /
    brace-scaffold / bare declaration-header — the `use`-line PoC) does NOT
    substantiate an executed handler body and so does not count as a covering
    cite in a strict tree."""
    if l1 < 1:
        l1 = 1
    if l2 > len(file_lines):
        l2 = len(file_lines)
    for i in range(l1 - 1, l2):
        if 0 <= i < len(file_lines) and _line_is_substantive(file_lines[i]):
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


# The SECURITY-CRITICAL trees the C10b file enumeration walks FILE-GRANULAR.
# A file under one of these is a request-reachable enforcer (a route handler, the
# native PAT gate, an edge auth/quota/webhook module, an app worker), so it is
# only "covered" when a concept GROUNDS that EXACT file (per-file source_files /
# `# Citations` cite) or it is an explicit `excludes:` entry. Crate-cluster /
# whole-directory ADOPTION must NOT auto-cover an individual file here.
#
# gate v6 fix #1 (audit #1 HIGH — cluster-directory-adoption neutralized the
# recursive enumeration): the `crates/container-platform` CrateCluster seeds the
# whole `crates/corelink-container` DIRECTORY, which clause (b) of
# `_concept_grounds` promotes to a grounded-directory seed; the FILE branch of
# `is_covered` then auto-covered EVERY `.rs` under that crate via the
# "rel.startswith(grounded_dir + '/')" relation — so dropping a brand-new
# `routes/poison.rs` / `src/poison_top.rs` stayed GREEN (the anti-shadow walk was
# dead). The fix CLASS: in these trees the directory-grounded-seed coverage relation
# is suppressed — a file is covered ONLY by an exact grounded seed or an exclude.
# Directory/cluster adoption still covers files OUTSIDE these trees (non-file-
# granular library crates), where the coarse crate-membership self-cert is by design.
_FILE_GRANULAR_STRICT_PREFIXES = (
    "crates/corelink-container/src/",   # the Rust container compute plane (routes/ + src/**)
    "worker/src/",                      # the TS edge plane (recursive)
)
# apps/<app>/src/** — file-granular per the BR5 edge-plane enumeration. Matched
# via a segment-aware glob so only the `src/` subtree of an app is strict (an
# app's top-level config/manifest files are not request-reachable enforcers).
_FILE_GRANULAR_STRICT_GLOBS = (
    "apps/*/src/**",
)


def _is_file_granular_strict(rel: str) -> bool:
    """True iff `rel` lives in a SECURITY-CRITICAL file-granular tree where
    directory/cluster ADOPTION must not auto-cover an individual file (gate v6
    fix #1). Such a file is covered only by an exact grounded seed or an
    `excludes:` entry."""
    if any(rel.startswith(p) for p in _FILE_GRANULAR_STRICT_PREFIXES):
        return True
    return any(_segment_glob_match(rel, g) for g in _FILE_GRANULAR_STRICT_GLOBS)


# gate v9 fix (HIGH — strict-classifier vs surface-walk EXTENSION MISALIGNMENT):
# `_is_file_granular_strict` classifies EVERY file under worker/src/** and
# apps/*/src/** as a strict, file-granular-required tree REGARDLESS of extension —
# but the C10b surface ENUMERATION only globbed `*.ts` (worker) and `*.ts`/`*.rs`
# (apps). So a real request-reachable Cloudflare-Worker backdoor handler dropped in
# those trees with ANY OTHER executable extension — `.mts`/`.mjs`/`.cts`/`.cjs`/
# `.tsx`/`.jsx`/`.js` (all of which wrangler accepts as `main`) — was classified
# strict yet NEVER enumerated, so it could never surface as a [C10b] gap and shipped
# GREEN with zero coverage (PoCs: worker/src/poison.mts + apps/runner-worker/src/
# poison.mts). The surface walk and the strict classifier MUST agree on WHAT is an
# executable source file. We factor that predicate ONCE here so both sides use the
# IDENTICAL extension set.
#   - JS/TS executable extensions (an edge/app worker entrypoint or imported
#     module): the full module-system matrix {.ts,.mts,.cts,.tsx,.js,.mjs,.cjs,.jsx}.
#   - .rs for the Rust trees.
# Genuinely-non-source files surfaced by the recursive walk (e.g. .json/.md/.css
# under src/) are NOT executable source and so are not enumerated by the JS/TS walk;
# they remain handled by the existing manifest data/config `excludes:` where they
# are intentionally surfaced as .rs/.ts today — but the closing of the JS/TS hole
# adds no spurious data-file surface (we enumerate only the executable extensions).
_EXEC_JS_TS_EXTS = (".ts", ".mts", ".cts", ".tsx", ".js", ".mjs", ".cjs", ".jsx")
_EXEC_RS_EXTS = (".rs",)


def _is_exec_source(rel: str, *, js_ts: bool = True, rust: bool = True) -> bool:
    """True iff `rel` names an EXECUTABLE source file under the shared predicate
    used by BOTH the strict file-granular classification and the C10b surface walk
    (gate v9). `js_ts` covers the full Cloudflare-Worker module matrix; `rust`
    covers `.rs`. The two callers MUST pass the same flags for the same tree so the
    classifier and the enumeration never disagree on what is request-reachable."""
    suffix = "." + rel.rsplit(".", 1)[-1].lower() if "." in rel else ""
    if js_ts and suffix in _EXEC_JS_TS_EXTS:
        return True
    if rust and suffix in _EXEC_RS_EXTS:
        return True
    return False


def _iter_exec_sources(base: Path, surface_root: Path, *, js_ts: bool = True,
                       rust: bool = True):
    """Yield (rel, False) for every RECURSIVE executable source file under `base`
    matching the shared `_is_exec_source` predicate — the surface-walk half of the
    gate-v9 strict-classifier == surface-walk unification."""
    if not base.is_dir():
        return
    for p in sorted(base.rglob("*")):
        if not p.is_file():
            continue
        rel = p.relative_to(surface_root).as_posix()
        if _is_exec_source(rel, js_ts=js_ts, rust=rust):
            yield (rel, False)


# gate v10 fix #2 (HIGH — app ENTRYPOINT outside src/): the apps surface walk +
# the strict classifier are hardcoded to `apps/*/src/**`, but a Cloudflare Worker's
# wrangler `main` can point ANYWHERE — `apps/sneaky-worker/index.ts` at app-root,
# `apps/x/dist/worker.mjs`, etc. (admin-ui's real main is `.open-next/worker.js`).
# An app whose deploy entrypoint lives OUTSIDE `src/` therefore had its actual
# request-reachable handler NEVER enumerated → it could ship GREEN with zero
# coverage. We parse each app's `wrangler.toml`/`wrangler.jsonc` `main = "..."` and
# ALSO enumerate that entrypoint file as a required surface (in addition to
# `apps/*/src/**`). A main at app-root (or any non-src dir) now surfaces as a
# [C10b] gap unless a concept grounds it or an explicit `excludes:` entry waives it.
# (The existing wholesale `apps/admin-ui` / `apps/get-corelink-worker` exact-prefix
# excludes still cover THEIR mains — `.open-next/worker.js` / `src/index.ts` both
# live under the excluded app dir — so a build-output/presentation main stays
# review-waived, as designed.)
#
# gate v11 fix (HIGH — env-OVERRIDE main shipped GREEN; PoC-proven): a wrangler
# config may declare a top-level `main` AND a PER-ENVIRONMENT override — TOML
# `[env.prod]\nmain = "build/worker-prod.mjs"` or JSONC `"env": {"prod": {"main":
# ...}}`. `wrangler deploy --env prod` deploys the ENV main, not the top-level one.
# The old `_wrangler_main` used `.search()` (FIRST match only), so it saw ONLY the
# top-level `main` — a decoy `main = "src/index.ts"` over an `[env.prod]` override
# pointing at `build/worker-prod.mjs` made the REAL prod entrypoint ship GREEN with
# zero coverage (PoC: apps/poc-envmain). We now parse EVERY `main` declaration in
# the file (`findall`, not `search`) — the top-level main AND every `[env.<name>]`
# (TOML) / `"env": {"<name>": {…}}` (JSONC) override — and enumerate EACH distinct
# entrypoint as a required surface (each strict). An env-override main outside src/
# now surfaces as a [C10b] gap unless a concept grounds it or an `excludes:` entry
# waives it. The matcher also tolerates single quotes and a TOML/JSONC ARRAY form
# (`main = ["a", "b"]` / `"main": ["a", "b"]`) if the wrangler schema ever allows it,
# enumerating every element. (Real apps declare only a top-level `src/index.ts` main
# with no env override — and the excluded apps' env mains, if any, stay covered by
# their wholesale exact-prefix excludes — so the widened net stays selective.)
_WRANGLER_MAIN_RE = re.compile(
    r"""["']?main["']?\s*[:=]\s*(\[[^\]]*\]|["'][^"']+["'])""", re.MULTILINE
)
_WRANGLER_MAIN_ELEM_RE = re.compile(r"""["']([^"',\[\]]+)["']""")


def _wrangler_mains(app_dir: Path) -> list[str]:
    """ALL app-relative `main` entrypoints declared in ANY of `app_dir`'s wrangler
    config files — the top-level `main` AND every per-environment override
    (`[env.<name>]` in TOML, `"env": {…}` in JSONC), UNIONED across EVERY
    `wrangler*.{toml,jsonc,json}` file in the dir. Returns a de-duplicated,
    order-preserving list (possibly empty).

    gate v12 (LOW, two same-root holes closed):
      1. ALT-NAMED / per-env config — `wrangler deploy --config wrangler.prod.toml`
         (or the convention `wrangler.<env>.toml`) deploys a main declared in a
         NON-canonical-named config. The old loop checked ONLY the three canonical
         names (`wrangler.toml`/`.jsonc`/`.json`), so a main declared solely in
         `wrangler.prod.toml` was NEVER read → could ship GREEN uncovered. We now
         GLOB every `wrangler*.{toml,jsonc,json}` in the app dir.
      2. DUAL-CONFIG precedence — the old loop was FIRST-FILE-WINS (it `return`ed on
         the first existing canonical name, `wrangler.toml` before `.jsonc`), but
         modern wrangler prefers `.jsonc`/`.json` over `.toml`. So a decoy
         `wrangler.toml` (cited `src/index.ts`) over a `wrangler.jsonc` (real,
         uncited `build/*.mjs`) hid the real entrypoint. We no longer first-file-win:
         we UNION the mains from EVERY config file, so neither a precedence decoy nor
         an alt-named config can hide the real entrypoint. (Over-enumerating a decoy
         main is conservative/safe — it merely demands that main be concept-covered
         or excluded; never HIDING a real one is the goal.)

    Parsed with a tolerant regex so the SAME matcher handles TOML (`main = "x"`) and
    JSONC (`"main": "x"`, possibly with `//` comments) without a TOML/JSONC
    dependency, and `findall` (NOT `search`) so an env-override main is not masked by
    a top-level decoy (gate v11). Single quotes and an array form (`main = ["a",
    "b"]`) are tolerated — every element of an array is enumerated."""
    mains: list[str] = []
    cfgs = sorted(
        p
        for p in app_dir.glob("wrangler*")
        if p.is_file() and p.suffix.lower() in (".toml", ".jsonc", ".json")
    )
    for cfg in cfgs:
        try:
            text = cfg.read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError):
            continue
        for raw in _WRANGLER_MAIN_RE.findall(text):
            raw = raw.strip()
            if raw.startswith("["):
                # array form: enumerate every quoted element.
                for elem in _WRANGLER_MAIN_ELEM_RE.findall(raw):
                    elem = elem.strip()
                    if elem and elem not in mains:
                        mains.append(elem)
            else:
                val = raw.strip("\"'").strip()
                if val and val not in mains:
                    mains.append(val)
    return mains


# gate v14: build-output / vendor dirs the recursive wrangler-config walk MUST
# skip — generated bundles + dependency trees are not authored deploy surface, and
# scanning them would (a) be slow and (b) re-discover the very build outputs the
# other gates already exclude (same spirit as validate_secrets_matrix / the
# secrets-checklist: never scan `.open-next`/`.wrangler`/generated bundles). Any
# directory whose NAME is in this set (at any depth) prunes that subtree.
_WRANGLER_WALK_EXCLUDE_DIRS = frozenset({
    ".wrangler", ".open-next", "node_modules", "target", ".git",
    "dist", "build",
})


def _wrangler_config_dirs(surface_root: Path) -> list[Path]:
    """EVERY directory in the repo that hosts a `wrangler*.{toml,jsonc,json}`
    config whose declared `main` is the real deploy surface — found by a RECURSIVE
    walk of `surface_root` (build-output / vendor dirs pruned), NOT a fixed
    location allowlist.

    gate v14 (MATERIAL — config LOCATION could still hide a deploy entrypoint):
      v13 enumerated wrangler mains over a FIXED 3-class location set — the repo
      ROOT + ONE level into `apps/*` + ONE level into `crates/*` — yet its
      docstring claimed "EVERY directory that hosts a wrangler config." So a
      wrangler config whose LOCATION fell OUTSIDE that set shipped GREEN with only
      coarse crate-dir / cluster adoption, and a real edge entrypoint (a `main`
      pointing outside `*/src/**`) declared there rode that adoption uncovered.
      Three PoC-proven location bypasses (all reverted):
        * crate-NESTED — `crates/corelink-clerk-cf/cf/wrangler.toml`
          (the one-level-only walk misses `crates/<x>/cf/`);
        * sibling top-level dir — `services/edge/wrangler.toml`
          (only root + apps/ + crates/ were considered);
        * `worker/` alt config — `worker/wrangler.staging.toml`
          (the `worker/` dir itself was never in the set).
      We now RECURSIVELY `rglob("wrangler*.{toml,jsonc,json}")` over the surface
      root, so config LOCATION can no longer hide a deploy entrypoint — matching
      what v10–v13 already do for config NAME (`wrangler*` glob) and env-override
      SYNTAX (`findall` over `[env.*]`). Build-output / vendor dirs
      (`_WRANGLER_WALK_EXCLUDE_DIRS`) are PRUNED so we never scan generated
      bundles (same exclusion the secrets gates apply). Every config found, in ANY
      dir, contributes its mains (top-level + `[env.*]` + array, via the existing
      `_wrangler_mains`) as STRICT required surfaces (concept cite or explicit
      exclude), exactly as before.

      (The `crates/corelink-clerk-cf` dormant POC stays GREEN because its declared
      `main = "build/worker/shim.mjs"` is a build artifact that does not exist on
      disk — the `_mp.is_file()` guard at the call site drops a non-existent main;
      if it ever materialises it surfaces as a strict [C10b] gap requiring a
      precise concept cite or an explicit exclude.)
    """
    dirs: list[Path] = []
    seen: set[Path] = set()

    def _hosts_config(d: Path) -> bool:
        try:
            return any(
                p.is_file() and p.suffix.lower() in (".toml", ".jsonc", ".json")
                for p in d.glob("wrangler*")
            )
        except OSError:
            return False

    def _add(d: Path) -> None:
        rd = d.resolve()
        if rd in seen:
            return
        if _hosts_config(d):
            seen.add(rd)
            dirs.append(d)

    if not surface_root.is_dir():
        return dirs

    # RECURSIVE walk, pruning build-output / vendor subtrees by directory NAME.
    stack = [surface_root]
    while stack:
        cur = stack.pop()
        _add(cur)
        try:
            children = sorted(p for p in cur.iterdir() if p.is_dir())
        except OSError:
            continue
        for child in children:
            if child.name in _WRANGLER_WALK_EXCLUDE_DIRS:
                continue
            if child.is_symlink():
                continue
            stack.append(child)
    # Deterministic order (the recursive pop order is not lexicographic).
    dirs.sort()
    return dirs


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

    # C5 SQUASH-ORPHAN fallback anchor: the merge-base of HEAD with the PR's base
    # ref (the fork point). When a concept's `checkpoint_sha` is squash-orphaned
    # (its commit was rewritten at merge and is unreachable in this clone — see C4
    # below), C5 re-anchors freshness to THIS reachable commit instead of the dead
    # checkpoint. Computed once here (not per-concept). None when no base ref is
    # resolvable (a degenerate detached checkout) — an orphaned concept then has no
    # anchor and C5 is skipped for it (warned under C4), which only happens where
    # the checkpoint object is also present anyway (local full clone ⇒ ckpt_ok).
    _resolved_base = git.resolve_base(args.base_ref) or args.base_ref
    base_rev_for_c5 = git.merge_base(_resolved_base)

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

        # C4: checkpoint_sha is 40-hex and its commit is resolvable in this clone.
        #
        # SQUASH-MERGE RESILIENCE (content-reachability, not commit-reachability).
        # When a feature PR whose concept pins `checkpoint_sha` at its OWN pre-merge
        # branch tip is SQUASH- or rebase-merged, git rewrites that tip into a new
        # commit; the original tip becomes ORPHANED — unreachable from `main` and,
        # in the gate's `fetch-depth: 0` CI clone, never fetched at all. The old C4
        # then hard-failed "checkpoint_sha not found in git history" on EVERY
        # subsequent PR (it passed LOCALLY, where the loose object still lingers in
        # the author's clone, but RED in CI) until a manual #688/#690 repoint. That
        # recurring false-positive is the trap this closes.
        #
        # An orphaned checkpoint is NOT drift and NOT a code bug: the squash LANDING
        # preserves the concept's cited source BYTE-FOR-BYTE, so the content the
        # author cited is intact — only the commit *name* (the id) died. So instead
        # of red-failing, we TOLERATE the orphan (a non-blocking C4 warning) and
        # re-anchor C5 freshness to the base ref (`base_rev_for_c5`, the reachable
        # fork point whose cited content equals the dead checkpoint's). Freshness is
        # still fully enforced against that reachable equivalent, so a genuinely
        # DRIFTED citation STILL fails C5 — only the dead-commit-name alarm is gone.
        # (This mirrors the C5b orphan-repair carve-out, which already treats a
        # non-ancestor checkpoint as an orphaned pointer rather than an error. The
        # residual — an orphaned SHA is now indistinguishable from a never-existed
        # typo — is accepted: content is still gated against the base ref either way,
        # and a fat-fingered SHA is caught in that concept's own PR review; only the
        # 40-hex FORMAT violation remains a hard C4 failure.)
        #
        # KNOWN TRADE-OFF (accepted, human-review-guarded — cold-review noted): a
        # FORGED well-formed-hex checkpoint is indistinguishable from a genuine
        # squash-orphan, so an author could in principle write a bogus unreachable
        # SHA to launder an ALREADY-LANDED drift past C5's reconcile-nag (the
        # re-anchor compares to the base ref, which already contains that landed
        # drift, so it reads "fresh"). This is NARROW: NEW drift a PR introduces
        # still fails C5 (working tree ≠ base ref), and freshness-when-base-resolves
        # + the fail-closed no-base path are both enforced below. It is the same
        # freshness ≠ authoring-correctness residual the contract already assigns to
        # human/panel review (a hand-picked unreachable SHA is review-visible), so
        # no code change is warranted — logged here so it is never mistaken for a
        # silently-closed hole.
        ckpt_ok = False
        ckpt_orphaned = False
        if c.checkpoint_sha:
            if not isinstance(c.checkpoint_sha, str) or not HEX40_RE.match(c.checkpoint_sha):
                fails.add("C4", loc, f"checkpoint_sha is not 40-hex: `{c.checkpoint_sha}`")
            elif git.sha_exists(c.checkpoint_sha):
                ckpt_ok = True
            else:
                # Object absent from this (full-history) clone ⇒ unreachable ⇒
                # squash-orphaned. Warn, don't fail; C5 re-anchors to the base ref.
                ckpt_orphaned = True
                fails.warn(
                    "C4", loc,
                    f"checkpoint_sha `{c.checkpoint_sha[:12]}` is orphaned "
                    "(squash/rebase rewrote its commit; unreachable in this clone) — "
                    f"C5 freshness re-anchored to base ref `{args.base_ref}`; "
                    "cited content still gated",
                )

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
            # The test-only-grounding exemption is for a concept whose SUBJECT *is*
            # the test harness — there the test IS the enforcer, so grounding an
            # invariant on a test path is correct (parallel to C6c being relaxed for
            # ADRs per §4.1).
            #
            # gate v6 fix #2 (audit #2 MED — the exemption was defeated by file
            # PLACEMENT): the old gate keyed the exemption on the doc living under
            # `docs/knowledge/testing/` ALONE. An author could file a SECURITY /
            # auth / compliance invariant at `docs/knowledge/testing/sneaky.md`
            # grounded SOLELY on a test and inherit the exemption — a neuterable
            # grounding masquerading as enforced. (The earlier `type ==
            # "TestStrategy"` ALONE was the symmetric dodge — self-declare the type
            # under auth/ and shed the requirement.) The exemption now requires
            # BOTH axes to agree: the doc lives under `testing/` AND its declared
            # `type` is a testing type (`TestStrategy`). A concept ABOUT the harness
            # satisfies both; a security/auth/compliance concept satisfies neither —
            # so it cannot inherit the test-enforcer exemption by placement OR by a
            # frontmatter relabel alone (it would have to mislabel BOTH the folder
            # and the type, which is review-visible and miscategorizes it in the
            # taxonomy/index). This binds the exemption to the concept being a
            # genuine test-harness concept, closing the placement bypass without
            # breaking the 3 legit testing/ concepts (all `type: TestStrategy`).
            _TESTING_TYPES = {"TestStrategy"}
            _in_testing_dir = (c.rel == "testing" or c.rel.startswith("testing/"))
            _is_testing_type = (
                isinstance(c.type, str) and c.type.strip() in _TESTING_TYPES
            )
            is_testing = _in_testing_dir and _is_testing_type
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
        # CONTENT of those exact lines between the baseline and the working tree;
        # drift fires whether the cause is an in-range edit OR a pure position-shift.
        # Baseline = the `checkpoint_sha` when it is resolvable (ckpt_ok); for a
        # squash-ORPHANED checkpoint (C4 above), the reachable base ref
        # (`base_rev_for_c5`), whose cited content is byte-identical to the dead
        # checkpoint's — so freshness is enforced against a REACHABLE equivalent and
        # a genuinely drifted cite still fails, without the orphan false-positive.
        # FAIL-CLOSED (cold-review MUST-FIX): a gate whose job is to be
        # unbypassable must never let its freshness check silently VANISH. When a
        # checkpoint is orphaned we re-anchor to `base_rev_for_c5`; but if that is
        # None (the base ref is unresolvable OR shares no common ancestor with
        # HEAD), there is NO reachable anchor to compare against — freshness is
        # UNVERIFIABLE, so we HARD-FAIL rather than skip. (Latent in practice: CI
        # always resolves `origin/<base_ref>` with a common ancestor. But a missing
        # anchor is a fail-open footgun, so it errors, never passes.)
        c5_baseline = None
        anchor_kind = "checkpoint"
        if ckpt_ok:
            c5_baseline = c.checkpoint_sha
            anchor_kind = "checkpoint"
        elif ckpt_orphaned:
            if base_rev_for_c5 is None:
                fails.add(
                    "C5",
                    loc,
                    f"orphaned checkpoint `{c.checkpoint_sha[:12]}` and NO reachable "
                    f"base anchor (base ref `{args.base_ref}` unresolvable / no common "
                    "ancestor with HEAD) — freshness UNVERIFIABLE (fail-closed; "
                    "re-anchor impossible)",
                )
            else:
                c5_baseline = base_rev_for_c5
                anchor_kind = "base-ref anchor"
        if c5_baseline:
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
                    if cited_range_drifted(git, c5_baseline, sf, l1, l2):
                        fails.add(
                            "C5",
                            loc,
                            f"STALE: cited content `{sf}:{l1}-{l2}` no longer matches "
                            f"{anchor_kind} {c5_baseline[:12]} "
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
        # ORPHANED-CHECKPOINT REPAIR EXEMPTION. If the PREVIOUS checkpoint_sha is
        # not an ancestor of the base ref, it never landed on the mainline — it is
        # an orphaned pointer (classic case: a PR's pre-merge branch tip that git
        # rewrote/replayed at merge, so the checkpoint the PR wrote points at a
        # commit `main` cannot reach; e.g. #677's G4 commit `73419d59`). Repointing
        # such a checkpoint to its real main-history landing SHA is a MANDATORY C4
        # repair, not a reconcile: the cited source is byte-identical (nothing to
        # re-read), so demanding a body edit would force exactly the phantom edit
        # C5b exists to reject. A genuine phantom advance always has an on-main
        # prev_sha and therefore stays fully gated below.
        if not git.is_ancestor(str(prev_sha), args.base_ref):
            continue
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
    surface_root = Path(args.surface_root).resolve()

    # gate v7 fix #2: a strict-tree file is GENUINELY grounded only when the
    # covering concept cites that exact file at a range that contains a real CODE
    # line (not a blank / `//`-`//!`-`///` doc-comment / `#`-`*` comment line).
    # A cite that resolves only to the file's leading doc/license header (the
    # `:1-2` PoC) does NOT substantiate the executed handler body, so it does not
    # code-ground a strict-tree seed. Cached per (path) since file content is
    # shared across concepts.
    _strict_file_lines_cache: dict[str, list[str] | None] = {}

    def _strict_file_lines(rel: str):
        if rel not in _strict_file_lines_cache:
            p = surface_root / rel
            try:
                _strict_file_lines_cache[rel] = p.read_text(encoding="utf-8").splitlines()
            except (OSError, UnicodeDecodeError):
                _strict_file_lines_cache[rel] = None
        return _strict_file_lines_cache[rel]

    def _concept_code_grounds(c: "Concept", sp: str) -> bool:
        """True iff concept `c`'s grounding of the EXACT strict-tree file `sp`
        is GENUINE for gate v7 fix #2.

        The hole fix #2 closes is the per-file CITE that points at a non-code
        line: a concept lists `sp` in `source_files` + cites it under
        `# Citations`, but the cite resolves only to the file's `//`/`//!`
        doc-comment header (the `:1-2` PoC), leaving the executed handler body
        uncited. So when `sp` IS cited by `c`, AT LEAST ONE of its cite ranges
        must contain a real CODE line.

        A strict file grounded by CrateCluster ADOPTION (relation (d): an
        explicit `seed_from` entry under a `type: CrateCluster` concept, NOT a
        per-file cite) is a DIFFERENT, already review-gated coverage path — the
        cluster adopts the crate as its narrative home and is not expected to
        cite every file. That path is unaffected: `sp` is code-grounded when it
        is NOT cited by `c` at all (pure adoption) OR it is cited with a code
        line. It is NOT code-grounded only when it IS cited yet EVERY cite is a
        comment/blank line — the exact PoC shape."""
        if sp not in c.cited_files:
            return True  # adoption-only (or source_files-only) — not a cite hole
        lines = _strict_file_lines(sp)
        if lines is None:
            return False
        for (f, l1, l2) in c.cites:
            if f == sp and _cite_is_code_line(lines, l1, l2):
                return True
        return False

    entries = data.get("candidates") or data.get("concepts") or []
    seed_paths: set[str] = set()           # every declared seed (for diagnostics)
    grounded_seed_paths: set[str] = set()  # seeds a real concept grounds — THESE cover
    # strict-tree seeds whose covering concept cites a real CODE line (fix #2).
    code_grounded_seed_paths: set[str] = set()
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
                if _concept_code_grounds(c, sp):
                    code_grounded_seed_paths.add(sp)
        # C10: active candidate must have a doc or a deferred marker (deferred
        # docs are scanned as concepts, so concept_ids already covers both).
        if status == "active" and cid:
            if cid not in concept_ids:
                fails.add("C10", str(manifest_path),
                          f"active candidate `{cid}` has no concept doc and no deferred marker")

    # C10b: manifest must cover the mechanically-enumerable repo surface.
    # NOTE: _glob_match is defined at module scope (`_segment_glob_match`) so it
    # is unit-testable and segment-aware; bound here as a local alias.
    _glob_match = _segment_glob_match

    # gate v7 fix #3 — a one-line `excludes:` entry with a fabricated reason can
    # hide a backdoor. We (a) require every exclude reason to be non-trivial (a
    # real sentence, not a placeholder), and (b) SURFACE-via-WARN every exclude
    # entry that targets a SECURITY-CRITICAL strict tree, so a human reviewer
    # sees exactly which strict-tree surfaces were waived (the sanctioned
    # alternative to a per-file cite must remain review-visible). A trivially-
    # reasoned strict-tree exclude is a hard [C10b] failure; an honest one is a
    # surfaced WARN.
    _PLACEHOLDER_REASONS = {
        "todo", "tbd", "fixme", "n/a", "na", "none", "test", "wip",
        "placeholder", "exclude", "excluded", "skip", "ignore", "-", "x",
    }

    def _reason_is_trivial(reason: str) -> bool:
        r = (reason or "").strip()
        if len(r) < 12:
            return True
        if r.lower().rstrip(".") in _PLACEHOLDER_REASONS:
            return True
        return False

    def _exclude_can_hit_strict(surf: str) -> bool:
        """True iff this exclude surface (exact or glob) can match SOME path in a
        strict tree — i.e. it waives strict-tree surface and must be reviewed."""
        s = surf.rstrip("/")
        # exact / prefix entry pointing into a strict tree
        if _is_file_granular_strict(s) or _is_file_granular_strict(s + "/x.rs"):
            return True
        # a broad pattern (e.g. **/tests/**) — does it match any strict prefix?
        if "*" in surf or "?" in surf or "[" in surf:
            probes = [
                "crates/corelink-container/src/routes/tests/x.rs",
                "crates/corelink-container/src/x.config.ts",
                "crates/corelink-container/src/x.d.ts",
                "worker/src/__tests__/x.ts",
                "worker/src/x.test.ts",
                "worker/src/x.config.ts",
                "apps/signup-worker/src/e2e/x.ts",
                "apps/signup-worker/src/x.test.ts",
            ]
            return any(_glob_match(p, surf) for p in probes)
        return False

    excludes: list[str] = []
    for ex in (data.get("excludes") or []):
        if isinstance(ex, dict):
            surf = ex.get("surface") or ex.get("path")
            reason = ex.get("reason")
            if surf and reason:
                surf = str(surf)
                excludes.append(surf)
                if _exclude_can_hit_strict(surf):
                    if _reason_is_trivial(str(reason)):
                        fails.add(
                            "C10b", str(manifest_path),
                            f"strict-tree exclude `{surf}` has a trivial/placeholder "
                            f"reason ({str(reason).strip()[:40]!r}) — a strict-tree waiver "
                            "must carry a real justification",
                        )
                    else:
                        fails.warn(
                            "C10b", str(manifest_path),
                            f"strict-tree exclude (review): `{surf}` — {str(reason).strip()[:80]}",
                        )

    # gate v10 fix #2 (HIGH): the set of wrangler-`main` entrypoints that live
    # OUTSIDE the conventional `apps/*/src/**` strict tree. Such a main IS the real
    # request-reachable deploy surface, so it must be treated as STRICT — directory/
    # cluster ADOPTION must NOT auto-cover it; it needs an exact grounded seed or an
    # explicit `excludes:` entry, the same as any other request-reachable enforcer.
    # gate v14 (MATERIAL): enumerate over EVERY wrangler config in the repo via a
    # RECURSIVE walk of the surface root (`_wrangler_config_dirs`, build-output /
    # vendor dirs pruned), not a fixed root+apps/*+crates/* location set. A wrangler
    # `main` pointing OUTSIDE `*/src/**` declared in ANY dir — a crate-NESTED
    # `crates/<x>/cf/`, a sibling top-level `services/edge/`, a `worker/` alt config,
    # or anywhere else — is now classified STRICT, so config LOCATION can no longer
    # hide a deploy entrypoint behind coarse crate-dir/cluster ADOPTION.
    wrangler_main_strict: set[str] = set()
    for _cfg_dir in _wrangler_config_dirs(surface_root):
        # gate v11: EVERY declared main — top-level AND every [env.*] override.
        for _main_rel in _wrangler_mains(_cfg_dir):
            _mp = (_cfg_dir / _main_rel).resolve()
            try:
                _r = _mp.relative_to(surface_root).as_posix()
            except ValueError:
                continue
            if _mp.is_file() and _is_exec_source(_r, js_ts=True, rust=True):
                # only the ones NOT already strict by the file-granular rule
                # (a main INSIDE the owning crate's src/** is already covered).
                if not _is_file_granular_strict(_r):
                    wrangler_main_strict.add(_r)

    def _strict(rel: str) -> bool:
        """`_is_file_granular_strict`, EXTENDED with the gate-v10/v13 wrangler-`main`
        entrypoints that live outside `*/src/**` — a non-conventional but
        request-reachable deploy surface declared by ANY wrangler config under
        apps/, crates/, or the repo root."""
        return _is_file_granular_strict(rel) or rel in wrangler_main_strict

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
            # gate v7 fix #2 — in a strict tree, a verbatim grounded seed is
            # coverage ONLY when the covering cite resolves to a real CODE line.
            # A backdoor handler cited solely at its `:1-2` doc-comment header is
            # NOT grounded (the cited line doesn't substantiate the executed
            # body), so it falls through to the exclude check and REDs unless it
            # carries an explicit per-file exclude. Outside strict trees the
            # verbatim grounded seed covers unchanged.
            if rel in grounded_seed_paths:
                if not _strict(rel) or rel in code_grounded_seed_paths:
                    return True
            # a file is also covered when a GROUNDED directory seed contains it
            # (the concept that adopts the dir grounds the files beneath it) —
            # EXCEPT in the SECURITY-CRITICAL file-granular trees (gate v6 fix #1):
            # there, directory/cluster ADOPTION must NOT auto-cover an individual
            # file, or a new request-reachable handler (routes/poison.rs,
            # src/poison_top.rs, a new worker/src/** or apps/*/src/** module) would
            # ride the whole-crate `crates/corelink-container` seed and stay GREEN —
            # the dead anti-shadow enumeration. Those files are covered only by an
            # exact grounded seed (above) or an explicit `excludes:` entry (below).
            if not _strict(rel) and any(
                (s.rstrip("/") != "" and rel.startswith(s.rstrip("/") + "/"))
                for s in grounded_seed_paths
            ):
                return True
        strict = (not is_dir) and _strict(rel)
        for ex in excludes:
            exn = ex.rstrip("/")
            # An EXACT (or exact-prefix) exclude entry is an EXPLICIT, review-
            # visible per-file/per-dir decision — always honored, in or out of a
            # strict tree.
            if rel == exn or rel.startswith(exn + "/"):
                return True
            # A BROAD PATTERN exclude (dir-name glob like `**/tests/**`, or an
            # extension glob like `*.config.ts`) is a bulk rule. gate v7 fix #1:
            # inside a SECURITY-CRITICAL strict tree, a broad pattern may exclude
            # a file ONLY when that file is GENUINELY a test/config artifact by
            # its OWN basename. A real handler name (`poison.rs`) dropped under a
            # `tests/` dir matches `**/tests/**` but is NOT a test by its name —
            # it must NOT be auto-swallowed; it requires an explicit per-file
            # `excludes:` entry (handled above) or grounded coverage, else it
            # REDs. Outside strict trees the bulk rule applies unchanged.
            if "*" in ex or "?" in ex or "[" in ex:
                if not _glob_match(rel, ex):
                    continue
                if strict and not _basename_is_test_or_config(rel):
                    continue
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
        # gate v10 fix #1 (MED — CASE-SENSITIVE rglob vs case-insensitive
        # classifier): `rglob("*.rs")` is case-SENSITIVE while `_is_exec_source`
        # lowercases the suffix, so a `Poison.RS` (pulled via `#[path]`) was
        # classified strict yet NEVER enumerated → shipped GREEN. Enumerate via
        # the SAME `_is_exec_source` predicate (case-insensitive) so the surface
        # walk and the strict classifier AGREE on what is a Rust source file.
        surface += list(_iter_exec_sources(routes_dir, surface_root, js_ts=False, rust=True))
    # The container crate ROOT, file-granular: a handler dropped directly in
    # crates/corelink-container/src/ (sibling to webhook.rs / native_pat_gate.rs)
    # is invisible if the crate is gated dir-granular only. Enumerate each
    # src/**/*.rs RECURSIVELY — a non-recursive glob left the src/storage/ subtree
    # (d1_http/r2_kv/r2_s3/region_map) silently un-gated (audit #2). routes/ is
    # also under src/ but already enumerated above; the dedup (dict.fromkeys on the
    # surface list) collapses the overlap. non-handlers (lib.rs is the crate wiring
    # root) are covered via the manifest like the rest.
    # gate v10 fix #1 (MED): same case-insensitive unification — enumerate via the
    # shared `_is_exec_source` predicate (which lowercases the suffix) instead of a
    # case-sensitive `rglob("*.rs")`, so a `src/Poison.RS` (classified strict by the
    # lowercasing classifier) is also ENUMERATED and surfaces as a [C10b] gap.
    container_src = surface_root / "crates" / "corelink-container" / "src"
    surface += list(_iter_exec_sources(container_src, surface_root, js_ts=False, rust=True))
    crates_dir = surface_root / "crates"
    if crates_dir.is_dir():
        surface += [(p.relative_to(surface_root).as_posix(), True) for p in sorted(crates_dir.iterdir()) if p.is_dir()]

    # --- Worker EDGE PLANE + apps/ (BR5 root fix) ------------------------------
    # The completeness oracle must also gate the TypeScript edge plane and the
    # apps/ workers (file-granular), not just the Rust container + ADRs — else a
    # future load-bearing worker/src or apps/*/src file with no concept stays
    # invisible. Enumerated as completeness-required surfaces; legit-exempt files
    # (types/config/test/UI/data) live in the manifest `excludes:`. The recursive
    # executable-source enumeration uses the shared `_iter_exec_sources` helper so
    # the surface walk and `_is_file_granular_strict` agree on the extension set
    # (gate v9).
    # gate v6 fix #3 (audit #3 LOW — worker/src was enumerated NON-recursively):
    # the old gate globbed `worker/src/*.ts` + a HARDCODED `worker/src/lib/*.ts`,
    # so a new `worker/src/<subdir>/*.ts` (any future subdir other than lib/)
    # escaped the surface entirely. Enumerate `worker/src/**` RECURSIVELY.
    # gate v9 fix (HIGH — extension misalignment): enumerate the FULL executable-
    # extension set via the shared `_is_exec_source` predicate (so the surface walk
    # and `_is_file_granular_strict` AGREE) — a `.mts`/`.mjs`/`.cts`/`.cjs`/`.tsx`/
    # `.jsx`/`.js` edge handler (all wrangler-`main`-eligible) is now enumerated, not
    # just `.ts`. Genuine non-handler subdirs surface as [C10b] and get an honest
    # manifest `excludes:` entry, the same as the container tree.
    worker_src = surface_root / "worker" / "src"
    surface += list(_iter_exec_sources(worker_src, surface_root, js_ts=True, rust=False))
    # gate v8 fix #1 (MEDIUM — app enumeration was a HARDCODED 3-app allowlist):
    # the surface walk only enumerated ("signup-worker","cas-worker","analytics-
    # worker"), but `_is_file_granular_strict` classifies EVERY `apps/*/src/**` as
    # strict — MISALIGNED. A brand-new app (e.g. `apps/runner-worker/src/poison.ts`)
    # was classified strict yet never ENUMERATED, so it could never surface as a
    # [C10b] gap → a new app's request-reachable handler shipped GREEN with zero
    # coverage. We now enumerate ALL `apps/*/src/**/*.{ts,rs}` DYNAMICALLY by
    # globbing the apps/ dir for any app that has a `src/`, so strict-classification
    # and the surface walk AGREE. Genuinely-non-app dirs under apps/ stay out of the
    # surface via the existing manifest `excludes:` (apps/admin-ui, apps/get-corelink-
    # worker are exact-prefix excludes honored by is_covered) — and any future
    # non-handler app that is wholesale-waived gets the same honest exclude entry.
    # gate v9 fix (HIGH): same extension unification for apps/*/src/** — enumerate
    # the FULL executable matrix ({.ts,.mts,.cts,.tsx,.js,.mjs,.cjs,.jsx} + .rs) via
    # the shared `_is_exec_source` predicate, not just `.ts`/`.rs`. A new app's
    # `.mts`/`.mjs` request-reachable handler is classified strict, so it must be
    # ENUMERATED too (PoC apps/runner-worker/src/poison.mts now REDs [C10b]).
    apps_dir = surface_root / "apps"
    if apps_dir.is_dir():
        for app in sorted(p for p in apps_dir.iterdir() if p.is_dir()):
            app_src = app / "src"
            if app_src.is_dir():
                surface += list(_iter_exec_sources(app_src, surface_root, js_ts=True, rust=True))

    # gate v10 fix #2 (HIGH): ALSO enumerate every wrangler `main` entrypoint —
    # the real deploy surface — wherever it lives (app-root, a non-src dir, build
    # output). An executable main OUTSIDE src/ surfaces as a required [C10b]
    # surface; an excluded surface stays covered by its existing exact-prefix
    # exclude.
    # gate v11 fix (HIGH): enumerate EVERY declared main — the top-level main AND
    # every per-environment `[env.<name>]` override — so an env-override
    # entrypoint (e.g. `[env.prod] main = "build/worker.mjs"`) that the top-level
    # decoy used to mask is now a required surface.
    # gate v14 fix (MATERIAL): enumerate over EVERY wrangler config found by a
    # RECURSIVE walk of the surface root (`_wrangler_config_dirs`, build-output /
    # vendor dirs pruned) — not a fixed root+apps/*+crates/* location set. A wrangler
    # main pointing OUTSIDE `*/src/**` declared in ANY dir — crate-NESTED
    # (`crates/<x>/cf/`), a sibling top-level dir (`services/edge/`), a `worker/` alt
    # config, etc. — now surfaces as a required [C10b] surface instead of riding
    # coarse crate-dir/cluster adoption from a hidden config LOCATION.
    for cfg_dir in _wrangler_config_dirs(surface_root):
        for main_rel in _wrangler_mains(cfg_dir):
            main_path = (cfg_dir / main_rel).resolve()
            try:
                rel = main_path.relative_to(surface_root).as_posix()
            except ValueError:
                rel = None
            if (
                rel
                and main_path.is_file()
                and _is_exec_source(rel, js_ts=True, rust=True)
            ):
                surface.append((rel, False))

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
