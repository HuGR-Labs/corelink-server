### Fixed

- **The OKF gate pays for the wrong fix: a `source_blobs` anchor is vacuously
  green (B-123, B-124).** With an anchor present, C5 compares the tree **with
  itself** for that file — so it catches drift introduced after the anchor and
  hides the drift the anchor was written on top of. Advancing an anchor is one
  command and turns the gate green instantly; renumbering the citations is a
  script plus byte-for-byte content verification plus a manual sweep of the
  abbreviated `` `:N-M` `` citations `CITE_RE` cannot see. The cheap option is
  the wrong one and the gate rewards it, which makes this a defect of incentive
  rather than of discipline. Evidence: on 2026-08-30 **two independent sessions
  that both knew the caveat** took the shortcut on #1389 and #1393, and in both
  cases `validate_okf` reported **0 stale** while citations pointed at wrong
  lines — 17 of 18 `main.rs` citations in `planes/container.md`, and 16 more in
  #1393. The remedy is NOT removing the anchor (without it the gate red-flags a
  file the branch legitimately alters); it is that the anchor does not
  substitute for renumbering. B-124 records the companion hazard: a bulk shifter
  computes offsets from the ORIGINAL numbers, so running it over a
  partially hand-fixed file double-shifts what was already corrected
  (`48-55` → `49-56` by hand → `50-57` by script, pointing at a blank line).
  Documentation only — no gate behaviour changes here; both items carry
  `verify:` blocks that state explicitly what they do not prove.
