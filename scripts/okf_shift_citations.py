#!/usr/bin/env python3
"""Renumber OKF citations after a code file moved lines — by CONTENT, never by
arithmetic (B-124).

WHY THIS IS A REPO-OWNED TOOL AND NOT A ONE-LINER PER SESSION
-------------------------------------------------------------
Every session that shifts a cited file writes its own throwaway `sed`, and every
throwaway meets the same three hazards:

1. **Offset arithmetic is not idempotent.** A shifter that adds `+1` to each
   citation computes from the ORIGINAL numbers, so a citation someone already
   corrected by hand gets moved a SECOND time. Reproduced on #1389:
   `main.rs:48-55` was hand-corrected to `49-56`, the bulk shifter then produced
   `50-57`, which points at a blank line — and nothing failed, because the
   checkpoint had been advanced in the same operation. This tool never adds an
   offset to a number. It reads the CONTENT the citation named in the base
   revision and finds where that content lives now.

2. **Replacing `file:N` corrupts the neighbouring `file:N-M`.** Token-level
   `sed` matches the prefix of a range. This tool rewrites by character SPAN,
   right-to-left, so a rewrite can never disturb a token it has not yet visited.

3. **A file in an intermediate state gets the transformation applied over the
   wreckage.** A rebase conflict is an intermediate state; a bulk replace runs
   straight over `<<<<<<<` markers and leaves the damage inside something that
   looks like a repair. This tool refuses any file carrying conflict markers.

And the fourth hazard, which is the reason the tool exists at all: **mixing a
hand fix and a programmatic one over the same file double-shifts it.** The rule
is per FILE, not per session. This tool enforces it mechanically instead of
asking people to remember: for each (concept, target file) pair it compares the
citation tokens naming that file against the base revision, and REFUSES the pair
if any of them was already edited by hand. Running it twice is therefore safe —
the second run finds its own output and declines, rather than shifting again.

WHAT IT GUARANTEES
------------------
Byte-for-byte, in both directions: a citation is rewritten to `L'` only when the
lines at `L'` in the working tree are byte-identical (trailing whitespace
stripped, as the OKF gate compares them) to the lines at `L` in the base
revision, AND that content occurs at exactly one place in the new file. Anything
ambiguous, vanished or already-correct is LEFT INTACT and reported. It never
guesses. A citation this tool cannot resolve is one a human must open the file
for — which is a smaller, honest worklist, not a silent pass.

Abbreviated continuation citations (`` `:803-831` ``, resolved against the
nearest preceding file reference) are first-class here, exactly as they are in
`validate_okf.py` — they are the form that survives every re-anchor unseen, and
6 of them were left wrong on #1410 while all 21 full-path ones were corrected.

USAGE
-----
    # dry run — prints the plan, writes nothing
    python3 scripts/okf_shift_citations.py crates/corelink-container/src/main.rs

    # apply, then let the gate confirm
    python3 scripts/okf_shift_citations.py --apply crates/…/main.rs
    python3 scripts/validate_okf.py

Exit status: 0 when every citation to the target files was either rewritten or
verifiably already correct; 1 when at least one could not be resolved (the
worklist is printed), or on a refusal.
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import validate_okf as okf  # noqa: E402

CONFLICT_RE = re.compile(r"^(<<<<<<< |=======$|>>>>>>> )", re.M)
# A backticked citation WITH its span in the document. Mirrors okf.CITE_RE /
# okf.BARE_PATH_RE so this tool and the gate can never disagree about what a
# citation is.
TOKEN_RE = re.compile(r"`([^`\n]+)`")


def _run(args: list[str], cwd: Path) -> subprocess.CompletedProcess:
    return subprocess.run(
        ["git", *args], cwd=str(cwd), capture_output=True, text=True, check=False
    )


def _repo_root() -> Path:
    cp = subprocess.run(
        ["git", "rev-parse", "--show-toplevel"], capture_output=True, text=True, check=False
    )
    if cp.returncode != 0:
        sys.exit("not inside a git repository")
    return Path(cp.stdout.strip())


def _resolve_base(root: Path, ref: str) -> str:
    for cand in (f"origin/{ref}", ref):
        cp = _run(["rev-parse", "--verify", "--quiet", f"{cand}^{{commit}}"], root)
        if cp.returncode == 0 and cp.stdout.strip():
            return cp.stdout.strip()
    sys.exit(f"base ref `{ref}` does not resolve (tried `origin/{ref}` and `{ref}`)")


def _show(root: Path, rev: str, rel: str) -> str | None:
    cp = _run(["show", f"{rev}:{rel}"], root)
    return cp.stdout if cp.returncode == 0 else None


def _norm(lines: list[str], l1: int, l2: int) -> list[str]:
    return [ln.rstrip() for ln in lines[l1 - 1 : l2]]


def _tokens_with_spans(body: str, declared: set[str]):
    """(path, l1, l2, span_start, span_end, is_abbrev) for every citation in
    `body`, in document order, with the span covering ONLY the inner token text
    (between the backticks). Referent resolution is identical to
    `validate_okf._collect_cites`."""
    out = []
    referent: str | None = None
    for m in TOKEN_RE.finditer(body):
        tok = m.group(1)
        stripped = tok.strip()
        cm = okf.CITE_RE.match(stripped)
        if not cm:
            if okf.BARE_PATH_RE.match(stripped) and stripped in declared:
                referent = stripped
            continue
        path = cm.group("path")
        abbrev = path is None
        if abbrev:
            if referent is None:
                continue
            path = referent
        else:
            referent = path
        l1 = int(cm.group("l1"))
        l2 = int(cm.group("l2")) if cm.group("l2") else l1
        if l2 < l1:
            l1, l2 = l2, l1
        # Span of the inner token, offset by any leading whitespace we stripped.
        lead = len(tok) - len(tok.lstrip())
        start = m.start(1) + lead
        out.append((path, l1, l2, start, start + len(stripped), abbrev))
    return out


def _occurrences(new_lines: list[str], block: list[str]) -> list[int]:
    """Every 1-based start line at which `block` occurs in `new_lines`."""
    k = len(block)
    n = len(new_lines)
    hits: list[int] = []
    if k == 0 or k > n:
        return hits
    for s in range(0, n - k + 1):
        if [ln.rstrip() for ln in new_lines[s : s + k]] == block:
            hits.append(s + 1)
    return hits


# Context sizes tried, in order, when the cited block alone is ambiguous.
_CONTEXT_STEPS = (0, 2, 5, 12, 30)


def _locate(base_lines: list[str], new_lines: list[str], l1: int, l2: int) -> list[int]:
    """Where the base's lines l1..l2 live in `new_lines`, as 1-based start lines.

    A single-line citation is frequently a line like `}` or `match state.write.
    write(req) {`, which occurs dozens of times — the cited block ALONE cannot
    identify it. So the block is progressively widened with the base file's own
    surrounding lines until the match is unique, and the offset is then folded
    back onto the cited range. Widening only ever narrows the candidate set, so a
    unique hit found with context is still a byte-identical match of the cited
    lines themselves; the caller re-asserts that before writing. When even the
    widest window stays ambiguous the citation is reported and LEFT INTACT — a
    plausible guess is the failure mode this whole tool exists to avoid."""
    hits = _occurrences(new_lines, _norm(base_lines, l1, l2))
    if len(hits) <= 1:
        return hits
    for ctx in _CONTEXT_STEPS[1:]:
        a = max(1, l1 - ctx)
        b = min(len(base_lines), l2 + ctx)
        wide = _norm(base_lines, a, b)
        wide_hits = _occurrences(new_lines, wide)
        if len(wide_hits) == 1:
            return [wide_hits[0] + (l1 - a)]
        if not wide_hits:
            break  # the neighbourhood changed; the tighter candidate set stands
    return hits


def main() -> int:
    ap = argparse.ArgumentParser(
        description="Renumber OKF citations to a moved code file, by content.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=__doc__,
    )
    ap.add_argument("files", nargs="+", metavar="PATH",
                    help="repo-relative code file(s) whose lines moved")
    ap.add_argument("--base-ref", default="main",
                    help="revision the citations were authored against "
                         "(default: main, resolved as origin/main first)")
    ap.add_argument("--apply", action="store_true",
                    help="write the changes (default: dry run, prints the plan)")
    ap.add_argument("--knowledge-dir", default="docs/knowledge",
                    help="OKF bundle root (default: docs/knowledge)")
    args = ap.parse_args()

    root = _repo_root()
    base = _resolve_base(root, args.base_ref)
    targets = [f.lstrip("./") for f in args.files]

    rewritten = 0
    unresolved: list[str] = []
    refused: list[str] = []
    skipped_ok = 0
    samples: list[str] = []

    for concept_path in sorted((root / args.knowledge_dir).rglob("*.md")):
        if concept_path.name in okf.RESERVED_NAMES:
            continue
        rel = concept_path.relative_to(root).as_posix()
        text = concept_path.read_text(encoding="utf-8")
        if CONFLICT_RE.search(text):
            refused.append(f"{rel}: conflict markers present — file is in an "
                           "INTERMEDIATE state; resolve the rebase first "
                           "(a bulk replace over `<<<<<<<` hides the damage inside a repair)")
            continue
        fm_block, body = okf._split(text)
        if fm_block is None:
            continue
        try:
            fm = okf.parse_frontmatter(fm_block)
        except Exception:
            continue
        declared = {s for s in (fm.get("source_files") or []) if isinstance(s, str)}
        relevant = [t for t in targets if t in declared]
        if not relevant:
            continue

        base_text = _show(root, base, rel)
        if base_text is None:
            refused.append(f"{rel}: not present at base `{args.base_ref}` — a NEW "
                           "concept has no base coordinates to shift from; write its "
                           "citations against the current tree instead")
            continue
        base_fm_block, base_body = okf._split(base_text)
        base_declared = declared
        if base_fm_block is not None:
            try:
                base_declared = {
                    s for s in (okf.parse_frontmatter(base_fm_block).get("source_files") or [])
                    if isinstance(s, str)
                }
            except Exception:
                pass

        cur_tokens = _tokens_with_spans(body, declared)
        base_tokens = _tokens_with_spans(base_body or "", base_declared)

        edits: list[tuple[int, int, str]] = []
        for target in relevant:
            base_file_text = _show(root, base, target)
            new_file = root / target
            if base_file_text is None or not new_file.exists():
                refused.append(f"{rel}: `{target}` missing at base or in the tree")
                continue
            base_lines = base_file_text.splitlines()
            new_text = new_file.read_text(encoding="utf-8", errors="replace")
            if CONFLICT_RE.search(new_text):
                refused.append(f"{target}: conflict markers present — refusing to "
                               "shift citations against a file mid-rebase")
                continue
            new_lines = new_text.splitlines()

            cur_for = [t for t in cur_tokens if t[0] == target]
            base_for = [t for t in base_tokens if t[0] == target]

            # THE PER-FILE ONE-METHOD RULE, mechanized. If the citation tokens
            # naming this file already differ from the base revision, a hand fix
            # (or an earlier run of this tool) is in flight for this file. Shifting
            # on top of that is precisely the double-shift this tool exists to
            # prevent, so the pair is refused — not silently skipped.
            if [t[1:3] for t in cur_for] != [t[1:3] for t in base_for]:
                refused.append(
                    f"{rel}: citations to `{target}` already differ from base "
                    f"({len(base_for)} at base, {len(cur_for)} now) — this file was "
                    "already edited by hand or by a previous run. Pick ONE method per "
                    f"FILE: `git checkout {args.base_ref} -- {rel}` and re-run, or "
                    "finish it by hand and do not run this tool on it."
                )
                continue

            for (path, l1, l2, s0, s1, abbrev) in cur_for:
                if l2 > len(base_lines) or l1 < 1:
                    unresolved.append(f"{rel}: `{path}:{l1}-{l2}` is out of bounds at "
                                      "base — nothing to carry forward")
                    continue
                block = _norm(base_lines, l1, l2)
                if not any(ln.strip() for ln in block):
                    unresolved.append(f"{rel}: `{path}:{l1}-{l2}` names only blank lines "
                                      "at base — no content identity to track")
                    continue
                hits = _locate(base_lines, new_lines, l1, l2)
                if not hits:
                    unresolved.append(
                        f"{rel}: `{path}:{l1}-{l2}` — the authored content no longer "
                        "exists in the file. The CODE changed, so the CLAIM must be "
                        "re-read, not renumbered. Left intact."
                    )
                    continue
                if len(hits) > 1:
                    unresolved.append(
                        f"{rel}: `{path}:{l1}-{l2}` — content occurs at "
                        f"{', '.join(str(h) for h in hits)}; ambiguous. Left intact."
                    )
                    continue
                n1 = hits[0]
                n2 = n1 + (l2 - l1)
                if (n1, n2) == (l1, l2):
                    skipped_ok += 1
                    continue
                # Post-condition, asserted before the edit is queued: the lines we
                # are about to point at must be byte-identical to the lines we are
                # pointing away from.
                assert _norm(new_lines, n1, n2) == block, "content verification failed"
                rng = f"{n1}" if l1 == l2 else f"{n1}-{n2}"
                new_tok = f":{rng}" if abbrev else f"{path}:{rng}"
                edits.append((s0, s1, new_tok))
                rewritten += 1
                if len(samples) < 8:
                    old_tok = (f":{l1}" if l1 == l2 else f":{l1}-{l2}") if abbrev else \
                              (f"{path}:{l1}" if l1 == l2 else f"{path}:{l1}-{l2}")
                    samples.append(
                        f"  {rel}\n    `{old_tok}` -> `{new_tok}`\n"
                        f"    content: {block[0].strip()[:78]!r}"
                    )

        if edits and args.apply:
            # RIGHT-TO-LEFT. Rewriting a span can only ever change offsets AFTER
            # it, so visiting in descending order keeps every remaining span
            # valid — the structural reason this cannot corrupt the neighbouring
            # `file:N-M` the way a token-level replace does.
            out = body
            for (s0, s1, new_tok) in sorted(edits, key=lambda e: -e[0]):
                out = out[:s0] + new_tok + out[s1:]
            concept_path.write_text(f"---\n{fm_block}\n---\n{out}", encoding="utf-8")

    print(f"base: {args.base_ref} -> {base[:12]}")
    print(f"targets: {', '.join(targets)}")
    print(f"rewritten: {rewritten}   already-correct: {skipped_ok}   "
          f"unresolved: {len(unresolved)}   refused: {len(refused)}")
    if samples:
        print("\nsample (content-verified):")
        for s in samples:
            print(s)
    for r in refused:
        print(f"\nREFUSED  {r}")
    for u in unresolved:
        print(f"UNRESOLVED  {u}")
    if not args.apply and rewritten:
        print("\n(dry run — pass --apply to write)")
    return 1 if (unresolved or refused) else 0


if __name__ == "__main__":
    raise SystemExit(main())
