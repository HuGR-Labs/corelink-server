#!/usr/bin/env python3
"""C5c — `anchor_content_reverify`: a blob anchor MOVED in a change must be paid
for with the renumbering it is standing in for (B-123).

Carved out of `validate_okf.py` rather than added to it: that file is on the
B-126 god-file baseline and the size ratchet only lets it shrink. `_check_c5c`
is called from `validate_okf`'s check sequence and shares its `Git`, `Concept`
and `Failures` objects — the import is deferred into the function body so the
two modules can reference each other without a cycle.

THE INSTRUMENT FIX THAT CAME WITH IT, recorded here because `Git.merge_base` no
longer has room to carry it. That method used to resolve the BARE base ref only,
and was wrong in both directions at once, silently:

  • In CI (`actions/checkout` at `fetch-depth: 0`, detached HEAD) a bare `main`
    does not resolve at all, so it returned None — and every check that needs a
    'previous version' of a concept, C5b's anti-phantom reconcile guard and C4c's
    blob ratchet included, degraded to a NO-OP without saying so.
  • On a developer clone or a long-lived worktree, the opposite: a stale local
    `main` resolves fine, so those checks compare against a base from weeks ago.
    Measured while building this check: local `main` was 100+ commits behind
    `origin/main`, and C5c reported 47 findings about anchors nobody had touched.

The fork point of a PR is the REMOTE branch it merges into; a local branch of the
same name is a private bookmark. `origin/<ref>` is therefore tried first, bare
`<ref>` kept as the fallback for a clone with no `origin`.
"""

from __future__ import annotations

from pathlib import Path


def _block_starts(new_lines: list[str], block: list[str]) -> list[int]:
    """Every 1-based start line at which `block` occurs in `new_lines`."""
    n, k = len(new_lines), len(block)
    if k == 0 or k > n:
        return []
    return [
        s + 1
        for s in range(0, n - k + 1)
        if [ln.rstrip() for ln in new_lines[s : s + k]] == block
    ]


# Context sizes tried, in order, when the cited block alone is ambiguous.
_C5C_CONTEXT_STEPS = (2, 5, 12, 30)


def _unique_shift(old_lines: list[str], new_lines: list[str], l1: int, l2: int):
    """The single offset `d` at which the base's lines l1..l2 reappear in
    `new_lines`, or None when that is not unique.

    The cited block alone is usually NOT enough to identify a position: a
    one-line citation of `}` or `match state.write.write(req) {` occurs dozens of
    times in the same file (measured on `routes/cas.rs`: 61 and 44 occurrences).
    So when the block is ambiguous it is widened with the base file's own
    surrounding lines until exactly one match survives, and the offset is folded
    back onto the cited range. Widening only ever narrows the candidate set, so a
    hit found with context is still a byte-identical match of the cited lines
    themselves. Still ambiguous at the widest window -> None, and the caller says
    nothing: this check reports the shortcut it can PROVE, never a suspicion."""
    block = _norm_block(old_lines, l1, l2)
    hits = _block_starts(new_lines, block)
    if len(hits) == 1:
        return hits[0] - l1
    if not hits:
        return None
    for ctx in _C5C_CONTEXT_STEPS:
        a = max(1, l1 - ctx)
        b = min(len(old_lines), l2 + ctx)
        wide = _block_starts(new_lines, _norm_block(old_lines, a, b))
        if len(wide) == 1:
            return (wide[0] + (l1 - a)) - l1
        if not wide:
            return None  # the neighbourhood changed — undecidable, stay silent
    return None


def _check_anchor_content_reverify(
    args, git: Git, bundle_root: Path, concepts: list[Concept], fails: Failures
):
    """C5c — `anchor_content_reverify`: a blob anchor MOVED in this change must be
    paid for with the renumbering it is standing in for (B-123).

    THE DEFECT THIS CLOSES. `cited_range_drifted` short-circuits the moment the
    working tree's blob equals the `source_blobs` anchor — so the instant an
    author re-points the anchor at the file as it is NOW, every citation to that
    file is fresh by construction, whatever line it names. C5 stops comparing the
    tree to the authored baseline and starts comparing it to ITSELF. That is not
    a gap in the author's discipline; it is a defect of INCENTIVE in the gate:
    advancing the anchor is one command and green immediately, renumbering the
    citations is a script plus byte-for-byte verification plus a manual sweep, and
    both end green. Measured 2026-08-30 on two independent branches whose authors
    BOTH knew the caveat: #1389 shipped 17 of 18 `main.rs` citations pointing at
    wrong lines and #1393 shipped 16 more, `validate_okf` reporting `0 stale`
    throughout. A cost ratio like that does not get fixed by asking harder.

    WHAT IT CHECKS. Only when a `source_blobs` anchor for a path is ADDED or
    CHANGED between the base version of the concept and this one — i.e. exactly
    the operation that buys the vacuous green. For each citation to that path in
    the PREVIOUS body, the check reads the authored CONTENT at the OLD numbers
    from the OLD baseline, then locates that byte-identical content in the NEW
    blob:

      • found at offset d, and this concept now cites (l1+d, l2+d) — PASS. The
        renumbering was done. d == 0 with the citation unchanged is the ordinary
        no-shift case and passes through the same arm.
      • found at offset d, and nothing cites it there — FAIL, naming the exact
        lines to write. This is the shortcut, and it is also how a DOUBLE SHIFT
        surfaces (B-124): a citation hand-corrected 48-55 -> 49-56 and then moved
        again by a bulk shifter to 50-57 does not sit on its own content, so the
        one candidate offset is reported against it.
      • found nowhere — SKIP, no failure. The cited code was genuinely rewritten,
        so the anchor advance is real re-authoring and there is no old position to
        renumber to. Refusing here would punish the honest case and the gate would
        be routed around within a week.

    WHAT IT DOES NOT DO. It does not remove the anchor and it does not replace
    renumbering — the two solve different problems (the anchor makes the gate
    compare against the right tree; renumbering makes the citation TRUE) and the
    whole failure is that they collapse into one in a hurry. It is silent on
    `main` and on any branch that does not move an anchor: the teeth are aimed at
    one operation.
    """
    # Deferred so the two modules can reference each other without a cycle:
    # by the time any check runs, `validate_okf` is fully loaded.
    global _norm_block
    from validate_okf import (
        HEX40_RE,
        SOURCE_BLOB_RE,
        _collect_cites,
        _norm_block,
        _split,
        _under,
        parse_frontmatter,
    )

    base_bundle = Path(args.base_bundle).resolve() if args.base_bundle else None
    base_rev = None if base_bundle is not None else git.merge_base(args.base_ref)

    for c in concepts:
        if c.is_deferred or not c.source_blobs:
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
            continue  # new concept — no anchor was moved, nothing was bypassed
        prev_block, prev_body = _split(prev_text)
        if prev_block is None:
            continue
        try:
            prev_fm = parse_frontmatter(prev_block)
        except Exception:
            continue

        prev_sources = [s for s in (prev_fm.get("source_files") or []) if isinstance(s, str)]
        prev_blobs: dict[str, str] = {}
        for entry in prev_fm.get("source_blobs") or []:
            if not isinstance(entry, str):
                continue
            m = SOURCE_BLOB_RE.match(entry.strip())
            if m:
                prev_blobs[m.group("path")] = m.group("blob").lower()
        prev_ckpt = prev_fm.get("checkpoint_sha")

        loc = (
            c.path.relative_to(git.repo_root).as_posix()
            if _under(c.path, git.repo_root)
            else f"{bundle_root.name}/{c.rel}"
        )

        prev_cites = _collect_cites(prev_body or "", prev_sources)

        for path, new_blob in sorted(c.source_blobs.items()):
            old_blob = prev_blobs.get(path)
            if old_blob is not None and old_blob == new_blob.lower():
                continue  # anchor did not move — C5 still has its real baseline

            # The baseline the citations were written against. A CHANGED anchor
            # names it directly; a NEWLY ADDED anchor inherits the commit anchor
            # the concept used before, which is what C5 would have compared to.
            if old_blob is not None:
                old_lines = git.blob_lines(old_blob)
            elif isinstance(prev_ckpt, str) and HEX40_RE.match(prev_ckpt) and git.sha_exists(prev_ckpt):
                old_lines = git.show_lines(prev_ckpt, path)
            elif base_rev:
                old_lines = git.show_lines(base_rev, path)
            else:
                old_lines = None
            # The NEW side is the WORKING TREE, not the anchored blob — the same
            # tree C3/C6 validate and C5's HEAD side reads. A citation's job is to
            # name a line of the code as it now stands; if the freshly written
            # anchor and the tree have already diverged, the tree is what the
            # reader will open. Falls back to the blob only when the path is gone
            # from the tree (C3 reports that separately).
            new_lines = git.worktree_lines(path) or git.blob_lines(new_blob)
            if old_lines is None or new_lines is None:
                # Baseline unresolvable. C4b already hard-fails an unresolvable
                # NEW anchor; an unresolvable OLD one means there is nothing to
                # re-verify against, so stay silent rather than invent a claim.
                continue

            prev_for = [(a, b) for (p, a, b) in prev_cites if p == path]
            cur_for = [(a, b) for (p, a, b) in c.cites if p == path]
            # A RENUMBER preserves the citation count for the file; a change that
            # adds or drops citations to it is a re-AUTHORING, where "the content
            # that used to be cited moved to line X" is not a defect — the author
            # deliberately cites something else now. Restricting the check to the
            # equal-count case removes that entire false-positive class, and costs
            # nothing against the operation it is aimed at: the shortcut touches
            # the anchor and leaves the prose alone by definition.
            if len(prev_for) != len(cur_for):
                continue
            cur_ranges = set(cur_for)
            for (l1, l2) in dict.fromkeys(prev_for):
                if l2 > len(old_lines) or l1 < 1:
                    continue  # citation was already out of bounds in the base — C6's job
                if not any(ln.strip() for ln in _norm_block(old_lines, l1, l2)):
                    continue  # blank range carries no identity to track
                d = _unique_shift(old_lines, new_lines, l1, l2)
                if d is None:
                    continue  # rewritten or ambiguous — undecidable, stay silent
                if (l1 + d, l2 + d) in cur_ranges:
                    continue  # renumbered (or never moved) — paid for
                fails.add(
                    "C5c",
                    loc,
                    f"`source_blobs` anchor for `{path}` was "
                    + ("advanced" if old_blob is not None else "added")
                    + f" but the citation `{path}:{l1}-{l2}` was NOT renumbered: its "
                    f"authored content is byte-identical at `{l1 + d}-{l2 + d}` in the "
                    "working tree. Advancing an anchor makes C5 compare the file with "
                    "itself — it does NOT re-verify the citations it covers (B-123). "
                    "`python3 scripts/okf_shift_citations.py --apply "
                    f"{path}` renumbers by content; then re-anchor.",
                )
