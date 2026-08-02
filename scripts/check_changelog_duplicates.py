#!/usr/bin/env python3
"""Fail when the `## [Unreleased]` CHANGELOG section carries the same entry twice.

WHY THIS EXISTS (observed defect, 2026-08-01, PR #955)
-----------------------------------------------------
A rebase turned a `1 insertion(+), 1 deletion(-)` CHANGELOG commit into an
insertion-only one: upstream had moved the context the deletion was anchored
to, so `git` dropped the deletion and kept the insertion. Two entries for the
same change survived, and the STALE one carried a factual claim that the newer
one existed specifically to retract. `changelog-validate` stayed green, because
it only asserts that a `feat:`/`fix:` PR HAS an `[Unreleased]` entry -- never
that it has exactly one.

This is not hypothetical and it is not rare. Measured on `origin/main`
@3960abf7 (578 comparable `[Unreleased]` entries, 166 753 pairs) this detector
finds THREE surviving duplicate pairs:

  * L47/L48   -- byte-identical `fix(docs+marketing): 51 more ... invocations`
  * L78/L79   -- byte-identical `fix(ci): the region_pinning ... forensic gate`
  * L63/L64   -- `fix(ci): 8 crons re-proved an UNCHANGED tree ...` where the
                 second entry RETRACTS the first ("now weekly, but gated on the
                 PR that could actually break them" vs. "Six of them have also
                 never been able to pass, which is why they do NOT get a PR
                 trigger")
  * L77/L78   -- the incident pair itself: "paths that no longer existed -- RED
                 since at least 2026-07-15" vs. the retraction "paths that have
                 NEVER existed -- RED since the day it was authored"

DETECTION
---------
Duplication is judged on the entry's BOLDED LEAD SENTENCE -- house style puts
the whole claim in the leading `**...**` span -- and never on whole-body
equality: in the real incident the two entries diverged in their tails while
sharing a near-identical opening claim, so a body/exact comparison would have
missed it.

Two orthogonal signals, both computed on the first LEAD_TOKENS(=24) normalised
tokens of that lead:

  1. LEAD-PREFIX: length of the common token prefix.  >= PREFIX_TOKENS(=12).
  2. LEAD-DICE:   Sorensen-Dice over adjacent token bigrams. >= DICE(=0.85).

THRESHOLDS -- CHOSEN FROM THE MEASURED CORPUS, NOT GUESSED
----------------------------------------------------------
Scored over every pair on `origin/main` @3960abf7, hand-classified:

  signal        honest siblings (max)                 real duplicates (min)
  ----------------------------------------------------------------------------
  lead-prefix   9  (L100 vs L102: MKCOL fix vs        14 (L63 vs L64)
                    PROPFIND/DELETE fix -- same
                    surface, two distinct changes)
  lead-dice     0.800 (L3971 vs L3984: two distinct   0.652 (L77 vs L78)
                    DSR erase adapters)

=> `lead-prefix` is the DISCRIMINATING signal: honest siblings top out at 9 and
   real duplicates start at 14, an empty band of 4. The threshold is 12 -- the
   middle of that band, >= 3 tokens of margin on BOTH sides. Tighter (>= 15)
   misses the L63/L64 retraction pair; looser (<= 9) fires on the honest
   `fix(container): the /cargo/<tenant>/<key> sccache/cargo WebDAV surface now
   ...` siblings, and this repo genuinely ships several such related entries per
   release (L971/L980, L1108/L1119, L100/L102 are all legitimate).

=> `lead-dice` ALONE CANNOT separate the two classes (0.652 duplicate sits
   BELOW the 0.800 honest maximum) -- documented here so nobody "simplifies"
   this file down to a single ratio. It is kept only as a high-confidence OR:
   0.85 is above the measured honest maximum (0.800) with margin, and catches
   the clone whose first tokens were edited (scope renamed, "(#123)" moved to
   the front) but whose lead is otherwise a copy.

SCOPE
-----
Only `## [Unreleased]`. Released sections are frozen history and may legitimately
repeat wording across versions.

By default only pairs where AT LEAST ONE member was ADDED by the diff under
review are reported, because (a) `main` today already carries the pre-existing
duplicates listed above and a whole-file gate would wedge every unrelated PR on
inherited debt, and (b) the failure mode being closed is a PR *introducing* a
duplicate -- a rebase-dropped deletion always shows up as an added line.
`--all` audits the whole section (use it for the backfill that cleans `main`).

USAGE
  check_changelog_duplicates.py --base <sha> --head <sha> [--file CHANGELOG.md]
  check_changelog_duplicates.py --all [--file CHANGELOG.md]
  check_changelog_duplicates.py --self-test

Exit 0 = clean, 1 = duplicate found, 2 = usage/parse error.
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys

# --- tunables (justified in the module docstring; do not change blind) -------
LEAD_TOKENS = 24  # how much of the bolded lead sentence is compared
PREFIX_TOKENS = 12  # >= this many identical leading tokens => duplicate
DICE = 0.85  # >= this bigram Dice on the lead => duplicate
MIN_TOKENS = 6  # leads shorter than this carry no claim (e.g. "- (none)")

UNRELEASED_RE = re.compile(r"^##\s+\[Unreleased\]\s*$", re.IGNORECASE)
SECTION_RE = re.compile(r"^##\s+\[")
BOLD_LEAD_RE = re.compile(r"^\*\*(.+?)\*\*(.*)$", re.S)


def normalise(text: str) -> list[str]:
    """Lowercase alphanumeric token stream; markdown/code/link noise removed.

    Inline code spans collapse to the literal token `code` so that a path or a
    `file.rs:12-34` citation cannot dominate the comparison, and so that a
    citation whose LINE NUMBERS drifted still matches its clone.
    """
    text = re.sub(r"`[^`]*`", " code ", text)
    text = re.sub(r"\[([^\]]*)\]\([^)]*\)", r"\1", text)  # [label](url) -> label
    text = text.lower()
    text = re.sub(r"[^a-z0-9]+", " ", text)
    return text.split()


class Entry:
    __slots__ = ("line", "raw", "tokens")

    def __init__(self, line: int, raw: str, tokens: list[str]):
        self.line = line
        self.raw = raw
        self.tokens = tokens

    def lead_text(self, width: int = 150) -> str:
        one = " ".join(self.raw.split())
        return one if len(one) <= width else one[: width - 1] + "…"


def unreleased_bounds(lines: list[str]) -> tuple[int, int]:
    start = None
    for i, ln in enumerate(lines):
        if UNRELEASED_RE.match(ln.strip()):
            start = i
            continue
        if start is not None and SECTION_RE.match(ln):
            return start, i
    if start is None:
        raise ValueError("missing '## [Unreleased]' heading")
    return start, len(lines)


def parse_entries(lines: list[str]) -> list[Entry]:
    """Top-level `- ` bullets of the [Unreleased] section, with their leads.

    The lead is the bolded `**...**` span. When that span is shorter than
    LEAD_TOKENS it is extended with the entry body up to LEAD_TOKENS -- entries
    whose bold is a bare title ("**Wave-36 Trigger A**") are otherwise
    indistinguishable from each other while describing different work.
    """
    start, end = unreleased_bounds(lines)
    entries: list[Entry] = []
    i = start + 1
    while i < end:
        ln = lines[i]
        if not ln.startswith("- "):
            i += 1
            continue
        body = ln[2:]
        j = i + 1
        # fold hard-wrapped continuation lines into the same entry
        while j < end and lines[j].strip() and not lines[j].startswith(("- ", "#", "  - ")):
            body += " " + lines[j].strip()
            j += 1
        m = BOLD_LEAD_RE.match(body)
        lead, rest = (m.group(1), m.group(2)) if m else (body, "")
        toks = normalise(lead)
        if len(toks) < LEAD_TOKENS:
            toks = (toks + normalise(rest))[:LEAD_TOKENS]
        else:
            toks = toks[:LEAD_TOKENS]
        if len(toks) >= MIN_TOKENS:
            entries.append(Entry(i + 1, lead.strip(), toks))
        i = j
    return entries


def bigrams(tokens: list[str]) -> set[tuple[str, str]]:
    return set(zip(tokens, tokens[1:]))


def dice(a: list[str], b: list[str]) -> float:
    A, B = bigrams(a), bigrams(b)
    if not A or not B:
        return 1.0 if a == b else 0.0
    return 2 * len(A & B) / (len(A) + len(B))


def common_prefix(a: list[str], b: list[str]) -> int:
    n = 0
    for x, y in zip(a, b):
        if x != y:
            break
        n += 1
    return n


def find_duplicates(entries: list[Entry], focus: set[int] | None):
    """Yield (entry_a, entry_b, signal, score) for every duplicate pair.

    `focus` = post-image line numbers added by the diff under review; when set,
    a pair is reported only if it touches one of them.
    """
    hits = []
    for i in range(len(entries)):
        for j in range(i + 1, len(entries)):
            a, b = entries[i], entries[j]
            if focus is not None and a.line not in focus and b.line not in focus:
                continue
            p = common_prefix(a.tokens, b.tokens)
            if p >= PREFIX_TOKENS:
                hits.append((a, b, "lead-prefix", float(p)))
                continue
            d = dice(a.tokens, b.tokens)
            if d >= DICE:
                hits.append((a, b, "lead-dice", d))
    return hits


def added_lines(base: str, head: str, path: str) -> set[int]:
    """Post-image line numbers of `+` lines for `path` in base..head.

    The comparison point is `git merge-base base head`, not `base` itself, so a
    branch rebased onto a newer `main` is judged on what IT changed and not on
    what it merely inherited.
    """
    try:
        mb = subprocess.check_output(["git", "merge-base", base, head], text=True).strip()
    except subprocess.CalledProcessError:
        mb = base
    diff = subprocess.check_output(
        ["git", "diff", "--unified=0", f"{mb}..{head}", "--", path], text=True
    )
    out: set[int] = set()
    post = None
    for ln in diff.splitlines():
        m = re.match(r"^@@ -\d+(?:,\d+)? \+(\d+)(?:,\d+)? @@", ln)
        if m:
            post = int(m.group(1))
            continue
        if post is None or ln.startswith(("+++", "---")):
            continue
        if ln.startswith("+"):
            out.add(post)
            post += 1
        elif ln.startswith("-"):
            pass
        else:
            post += 1
    return out


def report(hits, path: str) -> None:
    print(
        f"::error file={path}::{len(hits)} duplicated [Unreleased] entr"
        f"{'y' if len(hits) == 1 else 'ies'} detected."
    )
    for a, b, signal, score in hits:
        shown = f"{int(score)} identical leading tokens" if signal == "lead-prefix" else f"Dice {score:.3f}"
        print("")
        print(f"  DUPLICATE [{signal}: {shown}]  {path}:{a.line}  <->  {path}:{b.line}")
        print(f"    {path}:{a.line}: {a.lead_text()}")
        print(f"    {path}:{b.line}: {b.lead_text()}")
    print("")
    print("  The usual cause is A REBASE. A commit that replaced an entry")
    print("  (1 insertion(+), 1 deletion(-)) becomes insertion-only when upstream")
    print("  moves the context the deletion was anchored to, so both the stale and")
    print("  the new entry survive -- and the stale one may assert exactly what the")
    print("  new one was written to retract.")
    print("  Fix: keep ONE entry per change under '## [Unreleased]' and delete the")
    print("  other. Verify with:")
    print("    git log -p --follow -- CHANGELOG.md   # find which rebase kept both")
    print(f"    python3 scripts/check_changelog_duplicates.py --all --file {path}")


# --------------------------------------------------------------------------
# self-test: the gate must be shown to catch the real thing AND to not cry wolf
# --------------------------------------------------------------------------
_HEADER = "# Changelog\n\n## [Unreleased]\n\n### Fixed\n"

# Reconstructed incident: same opening claim, divergent tail (a rebase-survivor
# pair). MUST fail.
_DUP = _HEADER + (
    "- **fix(ci): the `region_pinning` INV-REGION-NO-CROSS-LEAK forensic gate asserted"
    " two artifact paths that no longer existed -- RED on `main` since at least"
    " 2026-07-15.** Body A.\n"
    "- **fix(ci): the `region_pinning` INV-REGION-NO-CROSS-LEAK forensic gate asserted"
    " two artifact paths that have NEVER existed -- RED since the day it was"
    " authored.** Body B, which retracts A.\n"
)

# Honest siblings taken verbatim from main (L100/L102): same surface, two
# distinct changes. MUST pass.
_SIBLINGS = _HEADER + (
    "- **fix(container): the `/cargo/<tenant>/<key>` sccache/cargo WebDAV surface now"
    " accepts `MKCOL`, so the real `sccache` binary can write (prod-verified"
    " real-client gap).** Body A.\n"
    "- **fix(container): the `/cargo/<tenant>/<key>` sccache/cargo WebDAV surface now"
    " also serves `PROPFIND` (stat) and `DELETE` (write-check cleanup), completing the"
    " real-`sccache` round-trip.** Body B.\n"
)

# Released sections are history and may repeat wording. MUST pass.
_RELEASED_REPEAT = (
    "# Changelog\n\n## [Unreleased]\n\n### Fixed\n"
    "- **fix(ci): the `region_pinning` INV-REGION-NO-CROSS-LEAK forensic gate asserted"
    " two artifact paths that never existed.** Body.\n"
    "\n## [1.0.0]\n\n### Fixed\n"
    "- **fix(ci): the `region_pinning` INV-REGION-NO-CROSS-LEAK forensic gate asserted"
    " two artifact paths that never existed.** Body.\n"
)

# Boilerplate placeholders repeat by design. MUST pass.
_PLACEHOLDERS = "# Changelog\n\n## [Unreleased]\n\n### Deprecated\n- (none)\n\n### Removed\n- (none)\n"


def self_test() -> int:
    cases = [
        ("reconstructed incident (divergent tails)", _DUP, 1),
        ("honest siblings (main L100/L102)", _SIBLINGS, 0),
        ("repeat across a RELEASED section", _RELEASED_REPEAT, 0),
        ("`- (none)` placeholders", _PLACEHOLDERS, 0),
    ]
    failures = 0
    for name, text, want in cases:
        hits = find_duplicates(parse_entries(text.splitlines()), None)
        got = len(hits)
        ok = got == want
        failures += 0 if ok else 1
        print(f"  [{'PASS' if ok else 'FAIL'}] {name}: expected {want} hit(s), got {got}")
    print(f"self-test: {len(cases) - failures}/{len(cases)} passed")
    return 0 if failures == 0 else 1


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--file", default="CHANGELOG.md")
    ap.add_argument("--base")
    ap.add_argument("--head")
    ap.add_argument("--all", action="store_true", help="audit the whole [Unreleased] section")
    ap.add_argument("--self-test", action="store_true")
    args = ap.parse_args()

    if args.self_test:
        return self_test()

    if not args.all and not (args.base and args.head):
        ap.error("need --all, or both --base and --head")

    try:
        with open(args.file, encoding="utf-8") as fh:
            lines = fh.read().splitlines()
        entries = parse_entries(lines)
    except (OSError, ValueError) as exc:
        print(f"::error file={args.file}::{exc}")
        return 2

    focus = None if args.all else added_lines(args.base, args.head, args.file)
    if focus is not None and not focus:
        print(f"OK: no lines added to {args.file} in this range; duplicate check n/a.")
        return 0

    hits = find_duplicates(entries, focus)
    if hits:
        report(hits, args.file)
        return 1

    scope = "whole [Unreleased] section" if args.all else "entries added by this PR"
    print(f"OK: no duplicated [Unreleased] entries ({scope}; {len(entries)} entries compared).")
    return 0


if __name__ == "__main__":
    sys.exit(main())
