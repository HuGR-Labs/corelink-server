#!/usr/bin/env python3
"""C5c: require citation renumbering when a ``source_blobs`` anchor moves.

The ordinary C5 check compares the worktree with the selected blob.  Moving
that blob therefore makes C5 compare a file with itself.  C5c compares the
previous concept and the current concept, and follows each old cited block
into the new file; an unchanged citation is rejected when that block moved.
"""

from __future__ import annotations

import re
from pathlib import Path


_BASE_REF_RE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._/-]{0,255}$")
_CONTEXT = (2, 5, 12, 30)


def _resolve_base(git, ref: str) -> str | None:
    """Return a bounded, literal merge-base for a base ref."""
    if not isinstance(ref, str) or len(ref) > 256:
        return None
    if not re.fullmatch(r"[0-9a-fA-F]{40}", ref) and (
        not _BASE_REF_RE.fullmatch(ref)
        or any(x in ref for x in ("..", "//", "/.", "@{"))
        or ref.endswith(".")
    ):
        return None
    # ``actions/checkout`` commonly leaves only ``origin/main`` in a detached
    # worktree.  Reuse the validator's bounded resolver instead of silently
    # turning C5c into a no-op there.
    resolved = git.resolve_base(ref)
    return git.merge_base(resolved) if resolved else None


def _norm(lines: list[str], first: int, last: int) -> list[str]:
    return [line.rstrip() for line in lines[first - 1:last]]


def _starts(lines: list[str], block: list[str]) -> list[int]:
    if not block or len(block) > len(lines):
        return []
    return [
        index + 1
        for index in range(len(lines) - len(block) + 1)
        if _norm(lines, index + 1, index + len(block)) == block
    ]


def _unique_shift(old: list[str], new: list[str], first: int, last: int):
    """Find the unique line offset of an old cited block in the new file.

    Ambiguous or genuinely rewritten content is intentionally undecidable and
    returns ``None``.  Context only narrows candidates; it never changes the
    bytes that must match for the cited range itself.
    """
    block = _norm(old, first, last)
    hits = _starts(new, block)
    if len(hits) == 1:
        return hits[0] - first
    if not hits:
        return None
    for width in _CONTEXT:
        left = max(1, first - width)
        right = min(len(old), last + width)
        contextual = _starts(new, _norm(old, left, right))
        if len(contextual) == 1:
            return (contextual[0] + first - left) - first
        if not contextual:
            return None
    return None


def _check_anchor_content_reverify(args, git, bundle_root: Path, concepts, fails):
    """C5c check, called by ``validate_okf.run_checks``."""
    import sys

    validator = sys.modules.get("validate_okf") or sys.modules.get("__main__")
    if validator is None:
        return
    split = validator._split
    parse_frontmatter = validator.parse_frontmatter
    collect_cites = validator._collect_cites
    source_blob_re = validator.SOURCE_BLOB_RE
    hex40 = validator.HEX40_RE
    under = validator._under

    base_bundle = Path(args.base_bundle).resolve() if args.base_bundle else None
    base_rev = None if base_bundle else _resolve_base(git, args.base_ref)
    if base_bundle is None and base_rev is None:
        # A missing base cannot prove an anchor delta. Existing C4/C5 checks
        # remain authoritative; do not turn every old concept into noise.
        return

    for concept in concepts:
        if concept.is_deferred or not concept.source_blobs:
            continue
        if base_bundle is not None:
            previous_path = base_bundle / concept.rel
            previous_text = previous_path.read_text(encoding="utf-8") if previous_path.exists() else None
        elif under(concept.path, git.repo_root):
            previous_text = git.show_file(base_rev, concept.path.relative_to(git.repo_root).as_posix())
        else:
            previous_text = None
        if previous_text is None:
            continue
        previous_block, previous_body = split(previous_text)
        if previous_block is None:
            continue
        try:
            previous_fm = parse_frontmatter(previous_block)
        except Exception:
            continue
        previous_sources = [x for x in (previous_fm.get("source_files") or []) if isinstance(x, str)]
        previous_blobs = {}
        for entry in previous_fm.get("source_blobs") or []:
            if not isinstance(entry, str):
                continue
            match = source_blob_re.match(entry.strip())
            if match:
                previous_blobs[match.group("path")] = match.group("blob").lower()
        previous_checkpoint = previous_fm.get("checkpoint_sha")
        previous_cites = collect_cites(previous_body or "", previous_sources)
        location = (
            concept.path.relative_to(git.repo_root).as_posix()
            if under(concept.path, git.repo_root)
            else f"{bundle_root.name}/{concept.rel}"
        )

        for path, new_blob in sorted(concept.source_blobs.items()):
            old_blob = previous_blobs.get(path)
            if old_blob is not None and old_blob == new_blob.lower():
                continue
            if old_blob is not None:
                old_lines = git.blob_lines(old_blob)
            elif isinstance(previous_checkpoint, str) and hex40.fullmatch(previous_checkpoint):
                # A well-formed checkpoint may be a squash orphan.  Mirror C5's
                # fallback to the reachable base rather than silently dropping
                # the previous cited content from C5c's comparison.
                old_lines = git.show_lines(previous_checkpoint, path)
                if old_lines is None and base_rev:
                    old_lines = git.show_lines(base_rev, path)
            else:
                old_lines = git.show_lines(base_rev, path) if base_rev else None
            # C5c verifies the content represented by the NEW anchor itself.
            # Never substitute the mutable worktree: a later source edit in the
            # same PR could erase the moved block and make this check vacuous.
            new_lines = git.blob_lines(new_blob)
            if old_lines is None or new_lines is None:
                continue
            old_ranges = list(dict.fromkeys((a, b) for p, a, b in previous_cites if p == path))
            current_ranges = list(dict.fromkeys((a, b) for p, a, b in concept.cites if p == path))
            # Adding/removing citations is prose re-authoring, not a pure anchor
            # shortcut; only equal slots can be matched by ordinal.
            if len(old_ranges) != len(current_ranges):
                continue
            for ordinal, (first, last) in enumerate(old_ranges):
                if first < 1 or last > len(old_lines) or not any(_norm(old_lines, first, last)):
                    continue
                shift = _unique_shift(old_lines, new_lines, first, last)
                if shift is None:
                    continue
                expected = (first + shift, last + shift)
                if current_ranges[ordinal] == expected:
                    continue
                observed = current_ranges[ordinal]
                fails.add(
                    "C5c",
                    location,
                    f"`source_blobs` anchor for `{path}` was "
                    + ("advanced" if old_blob is not None else "added")
                    + f" but citation `{path}:{first}-{last}` was not renumbered: "
                    f"its authored content is at `{first + shift}-{last + shift}` "
                    f"but citation slot {ordinal + 1} is `{path}:{observed[0]}-{observed[1]}` "
                    "(B-123)",
                )
