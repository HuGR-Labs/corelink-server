"""Concept and citation helpers for the OKF validator."""
from __future__ import annotations

from pathlib import Path
from validate_okf_core1 import *

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
        # OPTIONAL per-file BLOB anchor (§2.2): a flat list of `path@<40-hex blob>`
        # strings. Additive to `checkpoint_sha`, which stays REQUIRED — it is the
        # provenance record and the input C-AGE / C-REV / C5b read (they want a
        # commit_date, which a blob does not have). `source_blobs` names only the
        # thing C5 actually compares: the file's CONTENT at authoring time.
        #
        # SHAPE RATIONALE — flat list of scalars, not a nested mapping. This
        # frontmatter is read by FIVE independent hand-rolled parsers plus PyYAML,
        # and a nested `path: sha` mapping is the one shape they DISAGREE on:
        # PyYAML yields a dict while every hand parser here drops the indented
        # lines (their key regexes are anchored at column 0), so the same file
        # would validate differently depending on whether PyYAML happens to be
        # importable — and `.github/workflows/okf_wiki.yml` installs PyYAML into a
        # venv but invokes the gate with the SYSTEM python3, so both readers are
        # live in this repo. A flat list of strings parses identically under all
        # six. Per-FILE (not per-concept) so migration is incremental: a concept
        # may blob-address some of its sources and leave the rest on the legacy
        # commit anchor.
        sb = self.fm.get("source_blobs")
        self.source_blobs_raw = [x for x in sb if isinstance(x, str)] if isinstance(sb, list) else []
        self.source_blobs: dict[str, str] = {}
        self.source_blobs_bad: list[str] = []
        self.source_blobs_dupe: list[str] = []
        for entry in self.source_blobs_raw:
            m = SOURCE_BLOB_RE.match(entry.strip())
            if not m:
                self.source_blobs_bad.append(entry)
                continue
            path_, blob_ = m.group("path"), m.group("blob").lower()
            if path_ in self.source_blobs:
                self.source_blobs_dupe.append(path_)
                continue
            self.source_blobs[path_] = blob_
        self.is_adr = (self.type == ADR_TYPE) or self.concept_id.startswith("adr/")
        # parse cites + links
        self.cites = _collect_cites(self.body, self.source_files)  # list[(file, l1, l2)]
        self.cited_files = {c[0] for c in self.cites}
        self.links = [m for m in LINK_RE.findall(self.body)]


def _split(text: str):
    m = FRONT_MATTER_RE.match(text)
    if not m:
        return None, text
    return m.group(1), m.group(2)


def _collect_cites(body: str, source_files: "list[str] | set[str] | None" = None):
    """Collect full and abbreviated citations with concept-local inheritance."""
    declared = set(source_files or ())
    out = []
    referent: str | None = None
    for inner in BACKTICK_RE.findall(body):
        token = inner.strip()
        m = CITE_RE.match(token)
        if not m:
            if BARE_PATH_RE.fullmatch(token) and token in declared:
                referent = token
            continue
        path = m.group("path")
        if path is None:
            if referent is None:
                continue
            path = referent
        else:
            referent = path
        l1 = int(m.group("l1"))
        l2 = int(m.group("l2")) if m.group("l2") else l1
        if l2 < l1:
            l1, l2 = l2, l1
        out.append((path, l1, l2))
    return out


def _norm_block(lines: list[str], l1: int, l2: int) -> list[str]:
    """1-based inclusive slice of `lines`, each line trailing-whitespace-stripped
    (internal whitespace preserved — the comparison stays faithful)."""
    return [ln.rstrip() for ln in lines[l1 - 1 : l2]]


def cited_range_drifted(
    git: "Git",
    checkpoint_sha: str,
    path: str,
    l1: int,
    l2: int,
    blob_sha: str | None = None,
) -> bool:
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

    BLOB ANCHOR (`blob_sha`, §2.2 `source_blobs`). When the caller supplies the
    git BLOB id of the file at authoring time, that blob IS the baseline and
    `checkpoint_sha` is ignored for this comparison. This is what C5's own
    contract has always asked for — "the file's content at authoring time" — named
    directly instead of via a commit id that rebase, squash and cherry-pick all
    destroy. The comparison performed is otherwise IDENTICAL (same `_norm_block`
    slice, same working-tree HEAD side), so the check's power is unchanged: an
    in-range edit and a pure position-shift both still fire. What changes is only
    that the baseline survives history rewriting, so a re-anchor is needed when
    the CONTENT moved — never merely because the commit that named it was
    rewritten.
    """
    wt_blob = git.worktree_blob_sha(path)

    # BLOB-ANCHORED path (preferred). The baseline is named DIRECTLY by its
    # content hash, so no commit needs to be resolvable — and no fallback of any
    # kind applies. `blob_lines` returns None when the object is absent or is not
    # a blob, which is reported STALE here and hard-failed by C4b upstream; it is
    # never re-anchored to anything.
    if blob_sha:
        if wt_blob is not None and wt_blob == blob_sha:
            return False  # file byte-identical to the authored blob
        base_lines = git.blob_lines(blob_sha)
        wt_lines = git.worktree_lines(path)
        if base_lines is None or wt_lines is None:
            return True  # unresolvable anchor or vanished file — conservatively stale
        return _norm_block(base_lines, l1, l2) != _norm_block(wt_lines, l1, l2)

    # COMMIT-ANCHORED path (legacy, unchanged) — for concepts not yet migrated.
    ckpt_blob = git.file_blob_sha(checkpoint_sha, path)
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
    return any(CITE_FULL_RE.match(inner.strip()) for inner in BACKTICK_RE.findall(text))


def _block_cite_paths(text: str) -> list[str]:
    """The cited file paths (no line numbers) inside a single bullet block."""
    out: list[str] = []
    for inner in BACKTICK_RE.findall(text):
        m = CITE_FULL_RE.match(inner.strip())
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


# ---------------------------------------------------------------------------
__all__ = [name for name in globals() if not name.startswith("__")]
