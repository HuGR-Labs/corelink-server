### Fixed

- **`id: B-0142` was a silent alias of the real `B-142` (B-167).** It satisfies
  `^B-\d+$` (the rule B-143 installed), density does `int("0142") == 142` so the
  sequence stays dense, the duplicate check compares **strings** so nothing
  collides, and a `### B-0142` heading satisfies the heading/block check. Two
  items every human reads as one number, both CONFIRMED, and every `[B-142]`
  citation from outside resolving to whichever one it happened to hit.

  **The decision B-167 required has been made and recorded: of its three
  candidate rules, the third.** An id's spelling must equal `f"B-{int(n):03d}"`.

  - **Not `^B-\d{3}$`** — it closes the hole exactly for today's ids and forbids
    the day the register reaches `B-1000`, which is the caveat the item itself
    raised. That wrong rule passes every other cell in the suite and dies only on
    the `B-1000` cell, which exists for it.
  - **Not normalising only the duplicate key** — that accepts the spelling and
    rejects only the collision, so a lone `B-0500` would still merge and still
    read as a different number than it is. It refuses the alias, not the form.
  - **`f"B-{int(n):03d}"`** canonicalises *without freezing the width*: `B-1000`
    round-trips, and all 167 ids today already satisfy it. One spelling per
    number, and future renumbering is not forbidden.

### Changed

- **`scripts/test_backlog_verify.sh`: 30 cells.** The cell B-167 had pinned in its
  "KNOWN GAP" polarity — carrying the note that it *"must be INVERTED when B-167
  is fixed, and its failure is the reminder"* — fired, and is now inverted, as
  agreed in the file. Three new cells: the alias refused, an under-padded `B-1`
  refused, and `B-1000` accepted (the control that kills the width-freezing rule).
  The suite's own 45 non-canonical fixture ids (`B-1`, `B-2`, `B-3`, `B-42`) were
  canonicalised too — the rule applies to the test or it is not a rule.
