#!/usr/bin/env python3
"""The OKF citation model — what counts as a citation, and how a path-less one
resolves.

Carved out of `validate_okf.py` rather than added to it: that file is on the
B-126 god-file baseline and the size ratchet only lets it shrink. The rule is a
good one here, because "what is a citation" is consumed by three tools now
(`validate_okf.py`, `okf_reconcile.py` through the Concept model, and
`okf_shift_citations.py`) and a shared definition is the only way they cannot
disagree about it.
"""

from __future__ import annotations

import re

# A code-anchor cite token, parsed only from inside backticks: `path:line[-line]`.
#
# ABBREVIATED CONTINUATION FORM (B-059). The path group is OPTIONAL, so the bare
# `` `:803-831` `` the wiki writes to avoid repeating a long path immediately
# after naming it is a FIRST-CLASS citation. Before this, `CITE_RE` required a
# non-empty path and `_collect_cites` silently dropped every such token: measured
# on this corpus, **107 citations across 20 concepts** were invisible to C3
# (file exists), C5 (freshness) and C6 (line bounds) — never checked, never
# counted. #1410 is the recorded proof of the consequence: a one-line shift
# corrected 21 full-path cites and left 6 abbreviated ones pointing at the wrong
# lines, with the gate green throughout.
#
# A path-less match is resolved by `_collect_cites` against the nearest PRECEDING
# backticked file reference in the same concept — either a full citation or a
# bare backticked path that the concept declares in `source_files`. The bare-path
# arm is load-bearing, not defensive: `crates/handler-trait-seam.md` writes
# ``…impl is `D1CustomerHandler` in `crates/corelink-container/src/customer_d1.rs`
# — it impls all six (`:947`, …)``, where the referent is named WITHOUT a line
# number. Resolving those six against the last full citation instead attributes
# them to a 804-line file and reports six phantom out-of-bounds failures.
# With the bare-path arm, all 107 resolve and all 107 are in bounds.
#
# `CITE_FULL_RE` keeps the OLD strict shape and is what the BLOCK-LOCAL checks
# (`_has_cite`, `_block_cite_paths` → C6c grounding) use. Those examine one
# bullet at a time, where "nearest preceding" is not available, so admitting a
# path-less token there would let a bare `:42` count as grounding for an
# invariant — a LOOSENING. This change is a strict strengthening: more citations
# validated, no check weakened.
CITE_RE = re.compile(r"^(?P<path>[A-Za-z0-9._/\-]+)?:(?P<l1>\d+)(?:-(?P<l2>\d+))?$")
CITE_FULL_RE = re.compile(r"^(?P<path>[A-Za-z0-9._/\-]+):(?P<l1>\d+)(?:-(?P<l2>\d+))?$")
# A bare backticked repo path (no line numbers) — the other thing an abbreviated
# citation can continue from. Requires a `/` and a dotted basename so prose
# tokens like `Arc` or `read_only` cannot become a citation referent.
BARE_PATH_RE = re.compile(r"^[A-Za-z0-9._\-]+(?:/[A-Za-z0-9._\-]+)+$")
BACKTICK_RE = re.compile(r"`([^`]+)`")


def _collect_cites(body: str, source_files: "list[str] | set[str] | None" = None):
    """Every citation in `body` as (path, l1, l2), abbreviated forms RESOLVED.

    Backticked tokens are scanned in document order. A full `path:N[-M]` citation
    is emitted as-is AND becomes the current referent; a bare backticked path
    that the concept declares in `source_files` becomes the current referent
    WITHOUT emitting a citation; an abbreviated `:N[-M]` is emitted against the
    current referent. An abbreviated citation with no preceding referent is
    dropped (it is unresolvable, and inventing a path would be worse than the
    silence this function used to keep) — measured over the whole corpus, that
    case does not occur: 107 of 107 resolve.
    """
    declared = set(source_files or ())
    out = []
    referent: str | None = None
    for inner in BACKTICK_RE.findall(body):
        tok = inner.strip()
        m = CITE_RE.match(tok)
        if not m:
            if BARE_PATH_RE.match(tok) and tok in declared:
                referent = tok
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
    (internal whitespace preserved — the comparison stays faithful). Lives beside
    the citation model because it defines what "the cited lines" MEANS for every
    comparison the gates make; C5, C5c and the shifter must all slice identically
    or they disagree about whether a citation is true."""
    return [ln.rstrip() for ln in lines[l1 - 1 : l2]]
