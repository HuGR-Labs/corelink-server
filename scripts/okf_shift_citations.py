#!/usr/bin/env python3
"""Move OKF citation ranges by matching their authored content.

The command deliberately works from the base coordinates.  It never adds a
numeric offset: a citation is moved only when the exact lines it named in the
base occur exactly once in the current source file.  A file whose citations
already differ from the base is refused as a mixed hand-edit/programmatic
operation; this is the guard against the B-124 double shift.
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import validate_okf as okf  # noqa: E402


CONFLICT_RE = re.compile(r"^(?:<<<<<<< |=======|>>>>>>> )", re.MULTILINE)
TOKEN_RE = re.compile(r"`([^`\n]+)`")


@dataclass(frozen=True)
class Citation:
    path: str
    start: int
    end: int
    span_start: int
    span_end: int
    abbreviated: bool

    @property
    def range(self) -> tuple[int, int]:
        return self.start, self.end


def _run(root: Path, *args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["git", *args], cwd=root, text=True, capture_output=True, check=False
    )


def _repo_root() -> Path:
    cp = subprocess.run(
        ["git", "rev-parse", "--show-toplevel"],
        text=True,
        capture_output=True,
        check=False,
    )
    if cp.returncode:
        raise SystemExit("not inside a git repository")
    return Path(cp.stdout.strip())


def _resolve_base(root: Path, ref: str) -> str:
    # origin/<ref> is the PR target in CI; bare <ref> is the fallback for a
    # clone with no remote-tracking ref.  A full SHA works in either arm.
    for candidate in (f"origin/{ref}", ref):
        cp = _run(root, "rev-parse", "--verify", "--quiet", f"{candidate}^{{commit}}")
        if cp.returncode == 0 and cp.stdout.strip():
            return cp.stdout.strip()
    raise SystemExit(f"base ref {ref!r} does not resolve")


def _show(root: Path, revision: str, path: str) -> str | None:
    cp = _run(root, "show", f"{revision}:{path}")
    return cp.stdout if cp.returncode == 0 else None


def _lines(text: str) -> list[str]:
    return text.splitlines()


def _norm(lines: list[str], start: int, end: int) -> list[str]:
    return [line.rstrip() for line in lines[start - 1 : end]]


def _citations(body: str, declared: set[str]) -> list[Citation]:
    """Parse citations and abbreviated continuations with source-local spans."""
    result: list[Citation] = []
    referent: str | None = None
    for match in TOKEN_RE.finditer(body):
        token = match.group(1)
        stripped = token.strip()
        parsed = okf.CITE_RE.fullmatch(stripped)
        if parsed is None:
            if okf.BARE_PATH_RE.fullmatch(stripped) and stripped in declared:
                referent = stripped
            continue
        path = parsed.group("path")
        abbreviated = path is None
        if abbreviated:
            if referent is None:
                continue
            path = referent
        else:
            referent = path
        start = int(parsed.group("l1"))
        end = int(parsed.group("l2") or start)
        if end < start:
            start, end = end, start
        lead = len(token) - len(token.lstrip())
        span_start = match.start(1) + lead
        result.append(Citation(path, start, end, span_start, span_start + len(stripped), abbreviated))
    return result


def _occurrences(lines: list[str], block: list[str]) -> list[int]:
    if not block or len(block) > len(lines):
        return []
    width = len(block)
    return [
        offset + 1
        for offset in range(len(lines) - width + 1)
        if _norm(lines, offset + 1, offset + width) == block
    ]


def _locate(base: list[str], current: list[str], start: int, end: int) -> list[int]:
    """Find the unique current start for a base range, widening context safely."""
    block = _norm(base, start, end)
    hits = _occurrences(current, block)
    if len(hits) <= 1:
        return hits
    # A one-line `}` is commonly repeated.  Context can disambiguate it, but
    # ambiguity remains an explicit unresolved result rather than a guess.
    for context in (2, 5, 12, 30):
        left = max(1, start - context)
        right = min(len(base), end + context)
        wide = _occurrences(current, _norm(base, left, right))
        if len(wide) == 1:
            return [wide[0] + (start - left)]
        if not wide:
            return []
    return hits


def _ranges(citations: list[Citation], path: str) -> list[tuple[int, int]]:
    return [citation.range for citation in citations if citation.path == path]


def _format_range(start: int, end: int) -> str:
    return str(start) if start == end else f"{start}-{end}"


def _shift_concept(
    root: Path,
    concept: Path,
    base: str,
    targets: set[str],
    apply: bool,
) -> tuple[int, list[str], list[str]]:
    """Return (rewritten, unresolved, refused), atomically per concept."""
    current_text = concept.read_text(encoding="utf-8")
    if CONFLICT_RE.search(current_text):
        return 0, [], [f"{concept.relative_to(root)}: conflict markers present"]
    frontmatter, body = okf._split(current_text)
    if frontmatter is None or body is None:
        return 0, [], []
    try:
        metadata = okf.parse_frontmatter(frontmatter)
    except Exception as exc:
        return 0, [], [f"{concept.relative_to(root)}: invalid frontmatter ({exc})"]
    declared = {value for value in (metadata.get("source_files") or []) if isinstance(value, str)}
    relevant = targets & declared
    if not relevant:
        return 0, [], []
    base_text = _show(root, base, concept.relative_to(root).as_posix())
    if base_text is None:
        return 0, [], [f"{concept.relative_to(root)}: concept is absent at base {base[:12]}"]
    base_frontmatter, base_body = okf._split(base_text)
    if base_frontmatter is None or base_body is None:
        return 0, [], [f"{concept.relative_to(root)}: base has no usable frontmatter"]
    try:
        base_metadata = okf.parse_frontmatter(base_frontmatter)
    except Exception:
        base_metadata = metadata
    base_declared = {
        value for value in (base_metadata.get("source_files") or []) if isinstance(value, str)
    }
    current_cites = _citations(body, declared)
    base_cites = _citations(base_body, base_declared)
    edits: list[tuple[int, int, str]] = []
    unresolved: list[str] = []
    refused: list[str] = []
    for path in sorted(relevant):
        current_ranges = _ranges(current_cites, path)
        base_ranges = _ranges(base_cites, path)
        # Any existing difference means somebody already selected a method for
        # this file.  Refuse the whole pair so one hand edit cannot be shifted
        # again while untouched citations are processed.
        if current_ranges != base_ranges:
            refused.append(
                f"{concept.relative_to(root)}: citations to {path!r} already differ from base; "
                "choose hand editing OR this tool for the whole file"
            )
            continue
        source = root / path
        base_source = _show(root, base, path)
        if base_source is None or not source.is_file():
            refused.append(f"{concept.relative_to(root)}: source {path!r} is missing at base or in tree")
            continue
        current_source_text = source.read_text(encoding="utf-8", errors="replace")
        if CONFLICT_RE.search(current_source_text):
            refused.append(f"{concept.relative_to(root)}: source {path!r} has conflict markers")
            continue
        base_lines, current_lines = _lines(base_source), _lines(current_source_text)
        for citation in [item for item in base_cites if item.path == path]:
            if citation.start < 1 or citation.end > len(base_lines):
                unresolved.append(
                    f"{concept.relative_to(root)}: {path}:{_format_range(citation.start, citation.end)} "
                    "is out of bounds in base"
                )
                continue
            block = _norm(base_lines, citation.start, citation.end)
            if not any(line.strip() for line in block):
                unresolved.append(f"{concept.relative_to(root)}: {path}:{_format_range(citation.start, citation.end)} is blank")
                continue
            hits = _locate(base_lines, current_lines, citation.start, citation.end)
            if len(hits) != 1:
                detail = "not found" if not hits else "ambiguous at " + ", ".join(map(str, hits))
                unresolved.append(
                    f"{concept.relative_to(root)}: {path}:{_format_range(citation.start, citation.end)} {detail}"
                )
                continue
            new_start = hits[0]
            new_end = new_start + citation.end - citation.start
            if _norm(current_lines, new_start, new_end) != block:
                unresolved.append(f"{concept.relative_to(root)}: content verification failed for {path}:{new_start}")
                continue
            if (new_start, new_end) == citation.range:
                continue
            token = f":{_format_range(new_start, new_end)}" if citation.abbreviated else (
                f"{path}:{_format_range(new_start, new_end)}"
            )
            edits.append((citation.span_start, citation.span_end, token))
    if refused or unresolved:
        # Do not partially rewrite a concept when one target cannot be proven.
        return 0, unresolved, refused
    if apply and edits:
        rewritten_body = body
        for start, end, token in sorted(edits, key=lambda item: item[0], reverse=True):
            rewritten_body = rewritten_body[:start] + token + rewritten_body[end:]
        concept.write_text(f"---\n{frontmatter}\n---\n{rewritten_body}", encoding="utf-8")
    return len(edits), unresolved, refused


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("files", nargs="+", help="repo-relative source files whose lines moved")
    parser.add_argument("--base-ref", default="main", help="citation authoring revision (default: main)")
    parser.add_argument("--knowledge-dir", default="docs/knowledge", help="OKF bundle (default: docs/knowledge)")
    parser.add_argument("--apply", action="store_true", help="write verified changes; default is dry-run")
    args = parser.parse_args(argv)
    root = _repo_root()
    base = _resolve_base(root, args.base_ref)
    targets = {str(Path(name)) for name in args.files}
    bundle = root / args.knowledge_dir
    if not bundle.is_dir():
        raise SystemExit(f"knowledge directory does not exist: {bundle}")
    rewritten = 0
    unresolved: list[str] = []
    refused: list[str] = []
    for concept in sorted(bundle.rglob("*.md")):
        if concept.name in okf.RESERVED_NAMES:
            continue
        count, errors, refusals = _shift_concept(root, concept, base, targets, args.apply)
        rewritten += count
        unresolved.extend(errors)
        refused.extend(refusals)
    print(f"base: {base[:12]}; targets: {', '.join(sorted(targets))}")
    print(f"rewritten: {rewritten}; unresolved: {len(unresolved)}; refused: {len(refused)}")
    for message in refused:
        print(f"REFUSED: {message}")
    for message in unresolved:
        print(f"UNRESOLVED: {message}")
    if not args.apply and rewritten:
        print("dry run: pass --apply to write verified changes")
    return 1 if unresolved or refused else 0


if __name__ == "__main__":
    raise SystemExit(main())
