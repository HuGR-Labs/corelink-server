### Fixed

- **The `BACKLOG.md` gate failed OPEN on a malformed `id:` (B-143).**
  `scripts/backlog_verify.py` skipped any block whose id was not `B-<digits>`
  before checking that heading and block agree, and the density rule only ever
  collected ids that matched — so a malformed id escaped **both**. Measured
  against the real `BACKLOG.md` on the pre-fix script: `B-UNALLOCATED`, `B-TBD`,
  `B-131a` and `b-131` all came out `CONFIRMED … verify agrees with the declared
  status`, rc=0. The silent `continue` is now a named failure that reports the
  block by file and line.

- **A zero-padded id aliased silently onto a real one (B-143, adjacent).**
  `B-0142` satisfies `^B-\d+$`, `int("0142") == 142` keeps the density rule
  happy, and the duplicate check compared **strings** — so it coexisted with the
  real `B-142`: two headings, two blocks, one number. Duplicates are now keyed on
  the number.

  The item proposed refusing leading zeros in the canonical form instead. That
  would have been wrong, and the population is why: **99 of the 166 blocks are
  zero-padded** — `B-001`..`B-099` are this repo's own convention, `B-100`+ are
  three digits without padding. The proposed rule would have rejected 60% of the
  register. Padding stays legal; the alias is closed where duplicates are found.

- **`owner:` × `status` was never crossed (B-147).** `validate_schema()`
  validated `status` against `VALID_STATUS` and `owner` against `VALID_OWNER`
  independently, so `status: done` with `owner: owner` — a finished item that
  still claims to need the human — satisfied both. Closed work accumulated in the
  owner's queue and had to be drained by hand twice (#1510, #1522). The rule is
  scoped to `== "done"`, deliberately **not** `!= "open"`: a `parked` item waiting
  on an owner decision is exactly what `parked` is for. That scope is pinned by a
  test cell, so widening it later goes red instead of drifting.

### Changed

- **`scripts/test_backlog_verify.sh` grew 10 cells, 4 of them negative controls.**
  The seven red cells were measured against the actual pre-fix script rather than
  a hand-written mutant, and all six defect cells come back `exit 0` there — the
  fail-open reproduced, not deduced. The negative controls (a canonical id, a real
  zero-padded id, `parked`+`owner: owner`, `open`+`owner: owner`) are what stop
  "reject everything" and "reject every `owner: owner`" from passing the suite;
  each of those two over-broad mutants is killed by a named cell.
