#!/usr/bin/env python3
"""Fail when the `## [Unreleased]` CHANGELOG section carries the same entry twice.

WHY THIS EXISTS (observed defect, 2026-08-01, PR #955)
-----------------------------------------------------
A rebase turned a `1 insertion(+), 1 deletion(-)` CHANGELOG commit into an
insertion-only one: upstream had moved the context the deletion was anchored
to, so `git` dropped the deletion and kept the insertion. Two entries for the
same change survived, and the STALE one carried a factual claim that the newer
one existed specifically to retract. `changelog-validate` stayed green, because
it only asserted that a `feat:`/`fix:` PR HAS an `[Unreleased]` entry -- never
that it has exactly one.

This is not hypothetical and it is not rare. On `origin/main` @656aac35 (583
comparable `[Unreleased]` entries, 169 653 pairs) this detector finds FIVE
surviving duplicate groups / six redundant entries:

  * L47/L48    byte-identical `fix(admin-ui): four customer screens read fields
               the server never sends ...`
  * L50/L51    byte-identical `fix(docs+marketing): 51 more ... invocations`
  * L66/L67    `fix(ci): 8 crons re-proved an UNCHANGED tree ...` -- the second
               RETRACTS the first ("...but gated on the PR that could actually
               break them" vs. "Six of them have also never been able to pass,
               which is why they do NOT get a PR trigger")
  * L80/L81/L82 the incident group: "paths that no longer existed -- RED since
               at least 2026-07-15" vs. its retraction "paths that have NEVER
               existed -- RED since the day it was authored", plus a byte-clone
               of the retraction
  * L117/L119  `fix(container+worker)` / `fix(container)` for finding H17 --
               BOTH added by the SAME commit 9be1ad23, which shipped ONE fix.
               Not a rebase: an author writing the entry twice. Same defect.

THE SIGNALS, AND WHY THERE ARE THREE
------------------------------------
The naive check is whole-body equality; it would have missed the incident,
whose two entries diverged in their tails. The first-generation check compared
only the bolded lead sentence; that has a blind spot in exactly the dangerous
direction, because A RETRACTION REWRITES THE LEAD -- that is what retracting
means. So the lead signal is strongest where the duplicate is most benign (a
byte-identical clone) and weakest where it is most dangerous (a corrected entry
whose predecessor asserts something false). L80/L81 survived lead detection
only because that author happened to leave the first 16 tokens intact.

Three independent signals therefore run, and an entry pair is linked if ANY
fires. Groups are then the TRANSITIVE CLOSURE of those links (see REPORTING).

  S1 LEAD-PREFIX  length of the common token prefix of the first LEAD_TOKENS
                  (24) normalised tokens of the bolded lead sentence.
  S2 BODY-SHINGLE containment (|A n B| / min(|A|,|B|)) of the 5-word shingle
                  sets of the entry BODY, i.e. everything AFTER the bold lead.
                  Survives a total rewrite of the lead.
  S3 CITED-EVIDENCE  containment of the RARE backticked citations (file paths,
                  symbols, identifiers appearing in <= CITE_DF entries corpus-
                  wide), gated on the two entries sharing a primary
                  Conventional-Commit scope. Survives a rewrite of BOTH the
                  lead and the body: two entries for one change cite the same
                  artifacts even when every sentence around them changed.

THRESHOLDS -- MEASURED FROM THE CORPUS, NOT GUESSED
---------------------------------------------------
Every pair on `origin/main` was scored and hand-classified, with git provenance
(`git log -S` on the lead) used to settle the ambiguous ones -- that is how
L117/L119 was identified as a duplicate (one commit, one fix, two entries) and
how L27/L33, L200/L250, L1677/L1830, L95/L98 were confirmed honest (different
commits, different changes).

  signal          honest siblings (max)              duplicates it must catch
  ---------------------------------------------------------------------------
  S1 lead-prefix  9   L100/L102-class: the MKCOL     14  (L66/L67)
                      WebDAV fix vs the PROPFIND/
                      DELETE one -- two changes
  S2 body-shingle 0.297  (L27/L33, two distinct      0.551 (L66/L67)
                      KV-L2 latency slices)
  S3 cited-evid.  0.571  (L200/L250, over 702        0.727 (L80/L81, L80/L82)
                      eligible same-scope pairs)     0.833 (L117/L119)

  => S1 = 12: midpoint of the empty band [9, 14], >= 3 tokens of margin.
  => S2 = 0.42: midpoint of the empty band [0.297, 0.551], ~0.13 of margin.
  => S3 = 0.65: midpoint of the empty band [0.571, 0.727], ~0.08 of margin.

  Also kept: S1b lead-DICE >= 0.85, bigram Dice on the same 24-token lead. It
  is NOT a discriminator on its own -- the L80/L81 duplicate scores 0.652,
  BELOW the 0.800 of the honest DSR-adapter pair -- and is retained only as a
  high-confidence catch for the clone whose opening tokens were edited.
  Recorded here so nobody "simplifies" this file down to one ratio.

NO SINGLE SIGNAL SUFFICES, and the failures were measured, not assumed:
  * S2 alone cannot catch L80/L81 (0.118 with 5-shingles, 0.387 with 3-word
    shingles -- both under the honest maximum) or L117/L119 (0.196): those
    retractions rewrote the evidence, not just the lead.
  * S3 alone cannot catch L66/L67 (0.429, under the honest 0.571).
  * TF-IDF cosine over the whole entry was tried and REJECTED: min(duplicate)
    = 0.496 vs. max(honest) = 0.626 -- fully overlapping, no band.
  * Rare-word (IDF) containment was tried and REJECTED: the hardest duplicates
    share too little rare vocabulary once their bodies are rewritten.
  * Rare-citation containment WITHOUT the same-scope gate was tried and
    REJECTED: max(honest) = 0.833 (L117/L119-class scores collide with
    cross-subsystem pairs), no band.
S1 u S2 u S3 covers all five groups even if every lead is rewritten: G1 0.923,
G2 0.959, G3 0.603 by body; G4 joined 81<->82 by body (0.980) and 80<->81 by
evidence (0.727); G5 by evidence (0.833).

REPORTING
---------
Findings are reported as GROUPS (transitive closure of the linked pairs), with
the number of REDUNDANT entries (group size - 1), never as raw pairs. A 3-way
group emits 3 pairs; reporting "6 duplicated entries" for what is really 5
groups / 6 redundant entries sends the operator hunting for groups that do not
exist. The 3-way group on `main` is exactly that case.

SCOPE
-----
Only `## [Unreleased]`. Released sections are frozen history and may legitimately
repeat wording across versions.

By default only groups where AT LEAST ONE member was ADDED by the diff under
review are reported, because (a) `main` already carries the pre-existing groups
listed above and a whole-file gate would wedge every unrelated PR on inherited
debt, and (b) the failure mode being closed is a PR *introducing* a duplicate --
a rebase-dropped deletion always shows up as an added line. `--all` audits the
whole section (use it for the backfill that cleans `main`).

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

# --- tunables (bands measured in the module docstring; do not change blind) ---
LEAD_TOKENS = 24  # how much of the bolded lead sentence S1/S1b compare
PREFIX_TOKENS = 12  # S1: >= this many identical leading tokens => linked
DICE = 0.85  # S1b: >= this bigram Dice on the lead => linked
SHINGLE_K = 5  # S2: word-shingle width over the body
BODY_CONTAIN = 0.42  # S2: >= this body-shingle containment => linked
BODY_MIN_SHINGLES = 25  # S2: guard — a 2-line entry has no body to compare
CITE_CONTAIN = 0.65  # S3: >= this rare-citation containment => linked
CITE_DF = 3  # S3: a citation is "rare" at <= this many entries corpus-wide
CITE_MIN = 5  # S3: guard — both entries must cite >= this many rare artifacts
MIN_TOKENS = 6  # leads shorter than this carry no claim (e.g. "- (none)")

UNRELEASED_RE = re.compile(r"^##\s+\[Unreleased\]\s*$", re.IGNORECASE)
SECTION_RE = re.compile(r"^##\s+\[")
BOLD_LEAD_RE = re.compile(r"^\*\*(.+?)\*\*(.*)$", re.S)
CODE_SPAN_RE = re.compile(r"`([^`]+)`")
SCOPE_RE = re.compile(
    r"^(feat|fix|perf|refactor|chore|docs|test|ci|build|style|revert)\(([^)]+)\)!?:",
    re.IGNORECASE,
)


def normalise(text: str) -> list[str]:
    """Lowercase alphanumeric token stream; markdown/code/link noise removed.

    Inline code spans collapse to the literal token `code` so that a path or a
    `file.rs:12-34` citation cannot dominate the comparison, and so that a
    citation whose LINE NUMBERS drifted still matches its clone. (S3 looks at
    those citations directly, so nothing is lost.)
    """
    text = re.sub(r"`[^`]*`", " code ", text)
    text = re.sub(r"\[([^\]]*)\]\([^)]*\)", r"\1", text)  # [label](url) -> label
    text = text.lower()
    text = re.sub(r"[^a-z0-9]+", " ", text)
    return text.split()


def shingles(tokens: list[str], k: int = SHINGLE_K) -> frozenset:
    return frozenset(tuple(tokens[i : i + k]) for i in range(len(tokens) - k + 1))


def citations(text: str) -> set[str]:
    """Backticked artifacts an entry cites, with `:line` suffixes stripped.

    Spans of <= 3 chars are dropped: `id`, `sha`, `--` and friends are noise,
    not evidence.
    """
    out = set()
    for span in CODE_SPAN_RE.findall(text):
        span = re.sub(r":[\d\-,\s]+$", "", span.strip())
        if len(span) > 3:
            out.add(span)
    return out


class Entry:
    __slots__ = ("line", "raw", "tokens", "body", "cites", "rare", "scope")

    def __init__(self, line: int, raw: str, tokens: list[str], body: frozenset, cites: set, scope):
        self.line = line
        self.raw = raw
        self.tokens = tokens
        self.body = body
        self.cites = cites
        self.rare: frozenset = frozenset()
        self.scope = scope

    def lead_text(self, width: int = 140) -> str:
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
    """Top-level `- ` bullets of the [Unreleased] section, split lead / body.

    The lead is the bolded `**...**` span. When that span is shorter than
    LEAD_TOKENS it is extended with the body up to LEAD_TOKENS -- entries whose
    bold is a bare title ("**Wave-36 Trigger A**") are otherwise
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
        body_raw = ln[2:]
        j = i + 1
        # fold hard-wrapped continuation lines into the same entry
        while j < end and lines[j].strip() and not lines[j].startswith(("- ", "#", "  - ")):
            body_raw += " " + lines[j].strip()
            j += 1
        m = BOLD_LEAD_RE.match(body_raw)
        lead, rest = (m.group(1), m.group(2)) if m else (body_raw, "")
        toks = normalise(lead)
        toks = toks[:LEAD_TOKENS] if len(toks) >= LEAD_TOKENS else (toks + normalise(rest))[:LEAD_TOKENS]
        if len(toks) >= MIN_TOKENS:
            sm = SCOPE_RE.match(lead.strip())
            scope = sm.group(2).split("+")[0].strip().lower() if sm else None
            entries.append(
                Entry(
                    line=i + 1,
                    raw=lead.strip(),
                    tokens=toks,
                    body=shingles(normalise(rest)),
                    cites=citations(lead) | citations(rest),
                    scope=scope,
                )
            )
        i = j
    # corpus-wide document frequency decides which citations count as evidence
    df: dict[str, int] = {}
    for e in entries:
        for c in e.cites:
            df[c] = df.get(c, 0) + 1
    for e in entries:
        e.rare = frozenset(c for c in e.cites if df[c] <= CITE_DF)
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


def containment(A, B, guard: int) -> float:
    m = min(len(A), len(B))
    if m < guard:
        return 0.0
    return len(A & B) / m


def link_reasons(a: Entry, b: Entry) -> list[str]:
    """Every signal that fires for this pair; empty list = not a duplicate."""
    reasons = []
    p = common_prefix(a.tokens, b.tokens)
    if p >= PREFIX_TOKENS:
        reasons.append(f"lead-prefix({p})")
    else:
        d = dice(a.tokens, b.tokens)
        if d >= DICE:
            reasons.append(f"lead-dice({d:.3f})")
    c = containment(a.body, b.body, BODY_MIN_SHINGLES)
    if c >= BODY_CONTAIN:
        reasons.append(f"body-shingle({c:.3f})")
    if a.scope is not None and a.scope == b.scope:
        e = containment(a.rare, b.rare, CITE_MIN)
        if e >= CITE_CONTAIN:
            reasons.append(f"cited-evidence({e:.3f}, scope={a.scope})")
    return reasons


class Groups:
    """Union-find over entries, so a 3-way duplicate is ONE finding, not three."""

    def __init__(self, n: int):
        self.parent = list(range(n))
        self.reasons: dict[int, list[str]] = {}

    def find(self, x: int) -> int:
        while self.parent[x] != x:
            self.parent[x] = self.parent[self.parent[x]]
            x = self.parent[x]
        return x

    def union(self, x: int, y: int) -> None:
        rx, ry = self.find(x), self.find(y)
        if rx != ry:
            self.parent[ry] = rx


def find_groups(entries: list[Entry], focus: set[int] | None):
    """Return [(members, reasons)] -- transitive closure of all linked pairs.

    `focus` = post-image line numbers added by the diff under review; when set,
    a group is reported only if it contains one of them.
    """
    uf = Groups(len(entries))
    reasons: dict[frozenset, list[str]] = {}
    for i in range(len(entries)):
        a = entries[i]
        for j in range(i + 1, len(entries)):
            b = entries[j]
            r = link_reasons(a, b)
            if r:
                uf.union(i, j)
                reasons[frozenset((a.line, b.line))] = r
    buckets: dict[int, list[int]] = {}
    for i in range(len(entries)):
        root = uf.find(i)
        buckets.setdefault(root, []).append(i)
    out = []
    for members in buckets.values():
        if len(members) < 2:
            continue
        lines = [entries[i].line for i in members]
        if focus is not None and not any(ln in focus for ln in lines):
            continue
        why = sorted({r for k, rs in reasons.items() if k <= set(lines) for r in rs})
        out.append(([entries[i] for i in sorted(members, key=lambda i: entries[i].line)], why))
    out.sort(key=lambda g: g[0][0].line)
    return out


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


def report(groups, path: str) -> None:
    redundant = sum(len(m) - 1 for m, _ in groups)
    print(
        f"::error file={path}::{len(groups)} duplicate group"
        f"{'' if len(groups) == 1 else 's'} under '## [Unreleased]' — "
        f"{redundant} redundant entr{'y' if redundant == 1 else 'ies'} to remove."
    )
    for n, (members, why) in enumerate(groups, 1):
        print("")
        print(
            f"  GROUP {n}/{len(groups)} — {len(members)} entries describing one change,"
            f" {len(members) - 1} redundant:"
        )
        for e in members:
            print(f"    {path}:{e.line}: {e.lead_text()}")
        print(f"    linked by: {', '.join(why)}")
    print("")
    print("  The usual cause is A REBASE. A commit that replaced an entry")
    print("  (1 insertion(+), 1 deletion(-)) becomes insertion-only when upstream")
    print("  moves the context the deletion was anchored to, so both the stale and")
    print("  the new entry survive -- and the stale one may assert exactly what the")
    print("  new one was written to retract. The other cause is an entry written")
    print("  twice in one commit.")
    print("  Fix: keep ONE entry per change under '## [Unreleased]'; delete the rest.")
    print("  Investigate with:")
    print("    git log -p --follow -- CHANGELOG.md   # find which rebase kept both")
    print(f"    python3 scripts/check_changelog_duplicates.py --all --file {path}")


# --------------------------------------------------------------------------
# self-test: the gate must be shown to catch the real thing AND to not cry wolf
# --------------------------------------------------------------------------
_HEADER = "# Changelog\n\n## [Unreleased]\n\n### Fixed\n"

# G4 as it exists on main, with the LEAD OF THE STALE ENTRY REWRITTEN so the
# lead signals cannot see it (common prefix 1, Dice far under 0.85). It must
# still be caught -- by cited evidence. MUST fail.
_RETRACTION_LEAD_REWRITTEN = _HEADER + (
    "- **fix(ci): a false red on the tenant-isolation forensic gate — the two artifact"
    " paths it asserts were renumbered out from under it, so it has scored `PASS=5"
    " FAIL=2` on stale assertions while every substantive check passed.**"
    " `scripts/rb_region_leak_dry_run.sh` Step 2/Step 3 (and the mirrored `paths:`"
    " filter + job name in `.github/workflows/region_pinning.yml`) hard-coded"
    " `migrations/d1/0027_tenant_primary_region.sql` and"
    " `specs/03_architecture/adrs/ADR-S14-001-region-pinning-enforcement.md`. Both"
    " artifacts EXIST but were renumbered — `0028_tenant_primary_region.sql` and"
    " `ADR-S14-002-region-pinning-enforcement.md`. The substantive steps were green"
    " throughout: `residency_property_region_pinning_30k` compiles and all 10"
    " adversarial cross-region scenarios pass. **No isolation defect — a false red.**\n"
    "- **fix(ci): the `region_pinning` INV-REGION-NO-CROSS-LEAK forensic gate asserted"
    " two artifact paths that have NEVER existed — it has been RED since the day it was"
    " authored, 2026-05-14, and the work item that shipped it was SEALED on a fabricated"
    " verification result.** `scripts/rb_region_leak_dry_run.sh` Step 2/Step 3 (and the"
    " mirrored `paths:` filter + job name in `.github/workflows/region_pinning.yml`)"
    " asserted `migrations/d1/0027_tenant_primary_region.sql` and"
    " `specs/03_architecture/adrs/ADR-S14-001-region-pinning-enforcement.md`. **Those"
    " paths were not renumbered — they never existed.** `git show cc65c4ee --stat` shows"
    " the artifacts were BORN as `0028_tenant_primary_region.sql` and"
    " `ADR-S14-002-region-pinning-enforcement.md` in the SAME commit that authored the"
    " script pointing at 0027. The score was `PASS=5 FAIL=2` from birth, and"
    " `residency_property_region_pinning_30k` was never the failing part.\n"
)

# G3 as it exists on main, with the second lead rewritten past the prefix window.
# Caught by body shingles. MUST fail.
_RETRACTION_BODY_ONLY = _HEADER + (
    "- **fix(ci): 8 crons re-proved an UNCHANGED tree every single day — now weekly, but"
    " gated on the PR that could actually break them (so coverage goes UP, not down).**"
    " Measured, not guessed: 1000 runs / 3851 wall-min in 3 days across 69 workflows,"
    " and **40 daily crons**. The waste concentrated in checks whose only input is the"
    " tree itself — 5 TLA+ model-checkers firing daily (07:00/07:20/07:40/08:00/08:20)"
    " against `specs/tla/*.tla` files that change on the order of months, plus"
    " `region_pinning` (**~33 min per run**), `reproducible-build`, and"
    " `proptest-density-gate`. **The naive cut would have been a regression:** 6 of the 8"
    " had NO `pull_request` trigger at all, so the cron was their only gate.\n"
    "- **fix(ci): the daily cron budget was 40 jobs re-proving a tree nobody touched;"
    " six of the eight gates involved have never passed at all, so they get no PR"
    " trigger.** Measured, not guessed: 1000 runs / 3851 wall-min in 3 days across 69"
    " workflows, and **40 daily crons**. The waste concentrated in checks whose only"
    " input is the tree itself — 5 TLA+ model-checkers firing daily"
    " (07:00/07:20/07:40/08:00/08:20) against `specs/tla/*.tla` files that change on the"
    " order of months, plus `region_pinning` (**~33 min per run**),"
    " `reproducible-build`, and `proptest-density-gate`. Measuring their actual pass"
    " rate killed the PR-trigger plan and is the more important finding.\n"
)

# The original incident: same opening claim, divergent tails. MUST fail.
_DUP_LEAD = _HEADER + (
    "- **fix(ci): the `region_pinning` INV-REGION-NO-CROSS-LEAK forensic gate asserted"
    " two artifact paths that no longer existed -- RED on `main` since at least"
    " 2026-07-15.** Body A.\n"
    "- **fix(ci): the `region_pinning` INV-REGION-NO-CROSS-LEAK forensic gate asserted"
    " two artifact paths that have NEVER existed -- RED since the day it was"
    " authored.** Body B, which retracts A.\n"
)

# Honest siblings taken verbatim from main (the MKCOL vs PROPFIND/DELETE WebDAV
# pair): same surface, same scope, two distinct changes. MUST pass.
_SIBLINGS = _HEADER + (
    "- **fix(container): the `/cargo/<tenant>/<key>` sccache/cargo WebDAV surface now"
    " accepts `MKCOL`, so the real `sccache` binary can write (prod-verified real-client"
    " gap).** `sccache`'s WebDAV backend (opendal) shards a cache key as `X/Y/Z/<hash>`"
    " and issues a `MKCOL` for each path segment before the `PUT`. The container router"
    " answered `405` for `MKCOL`, so every write died before the first byte moved."
    " `MKCOL` now returns `201` for a path inside the tenant prefix and `403` outside"
    " it; the object store itself is flat, so the call is an assertion, not a mkdir.\n"
    "- **fix(container): the `/cargo/<tenant>/<key>` sccache/cargo WebDAV surface now"
    " also serves `PROPFIND` (stat) and `DELETE` (write-check cleanup), completing the"
    " real-`sccache` round-trip.** With `MKCOL` fixed, the real `sccache` binary got"
    " further and then stalled on its start-up write-check, which `PROPFIND`s the"
    " probe object and `DELETE`s it. `PROPFIND` returns a minimal `207 Multi-Status`"
    " for a single resource; `DELETE` is idempotent and returns `204` whether or not"
    " the object existed. Proven by running the real binary, not by curl.\n"
)

# Two distinct changes to one subsystem that legitimately cite the same
# artifacts (the shape S3 must not fire on). MUST pass.
_SHARED_CITATIONS = _HEADER + (
    "- **fix(signup-worker): tie \"App public\" to \"OAuth ownership proof enforced\" so"
    " they cannot diverge.** `apps/signup-worker/src/install.ts` read"
    " `GITHUB_APP_PUBLIC` and `REQUIRE_OWNERSHIP_PROOF` as independent flags, so a"
    " deploy could flip the app public while ownership proof stayed off. The two now"
    " resolve from one derived value in `apps/signup-worker/src/config.ts`, and"
    " `installation_id` binding refuses to run when the pair disagrees.\n"
    "- **feat(signup-worker): prove GitHub App installation ownership before binding —"
    " closes the runner install cross-tenant hijack (the public-flip HARD GATE).** A"
    " caller who knew any `installation_id` could bind it to their own tenant:"
    " `apps/signup-worker/src/install.ts` trusted the callback parameter. The new"
    " exchange calls the GitHub installation API with an app JWT, compares the"
    " installation account against the OAuth identity, and rejects on mismatch;"
    " `REQUIRE_OWNERSHIP_PROOF` gates the rollout and `GITHUB_APP_PUBLIC` stays off"
    " until it is on.\n"
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
        ("incident, lead intact (S1)", _DUP_LEAD, 1, 1),
        ("retraction, lead rewritten — body evidence (S2)", _RETRACTION_BODY_ONLY, 1, 1),
        ("retraction, lead AND body rewritten — citations (S3)", _RETRACTION_LEAD_REWRITTEN, 1, 1),
        ("honest siblings, same scope + surface", _SIBLINGS, 0, 0),
        ("honest pair sharing cited artifacts", _SHARED_CITATIONS, 0, 0),
        ("repeat across a RELEASED section", _RELEASED_REPEAT, 0, 0),
        ("`- (none)` placeholders", _PLACEHOLDERS, 0, 0),
    ]
    failures = 0
    for name, text, want_groups, want_redundant in cases:
        groups = find_groups(parse_entries(text.splitlines()), None)
        got_g = len(groups)
        got_r = sum(len(m) - 1 for m, _ in groups)
        ok = got_g == want_groups and got_r == want_redundant
        failures += 0 if ok else 1
        detail = "; ".join(", ".join(w) for _, w in groups) or "-"
        print(
            f"  [{'PASS' if ok else 'FAIL'}] {name}: expected {want_groups} group(s)/"
            f"{want_redundant} redundant, got {got_g}/{got_r}  [{detail}]"
        )
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

    groups = find_groups(entries, focus)
    if groups:
        report(groups, args.file)
        return 1

    scope = "whole [Unreleased] section" if args.all else "groups touching a line this PR added"
    print(f"OK: no duplicated [Unreleased] entries ({scope}; {len(entries)} entries compared).")
    return 0


if __name__ == "__main__":
    sys.exit(main())
