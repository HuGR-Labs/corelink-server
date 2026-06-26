---
type: "TestStrategy"
title: "End-to-end testing strategy"
description: "The consolidated gap map of CoreLink's user-simulation suites: broad in shape but thin in what actually runs+asserts, with two dominant structural problems (green-by-vacuum, the unproven moat)."
source_files:
  - "docs/testing/2026-06-23-gapmap-MASTER.md"
checkpoint_sha: "c100df62c1ce7d50185f5102ce1185da0a9fe9f9"
provenance: "AUTHORED"
tags: ["testing", "e2e", "user-simulation", "coverage", "gap-map"]
timestamp: "2026-06-26T00:00:00Z"
---

# End-to-end testing strategy

CoreLink's go-live confidence rests on user-simulation suites that pretend to be a real customer
against prod — but the consolidated audit found them broad in shape and thin in what actually runs
and asserts. This concept captures the MASTER gap map: the two structural problems that dominate
everything else (a runner that reports GREEN even when every journey is gated, and the single
load-bearing product claim — the network-effect cache moat — having zero positive proof), the ranked
findings, and what is genuinely strong. The owner's standing distrust is the frame: *green CI ≠
validated*. It is the strategy-level view above the
[CI gate machinery](/testing/gate-machinery.md) and the
[real-user black-box suites](/testing/real-user-suites.md).

# Role

It is the honest coverage map: where the suites prove what they claim, where a GREEN can mean "ran
almost nothing," and the cheapest-trust-per-unit order to close the distance between "ships
correctly" and "we have a test that proves it stays correct."

# How it works

- The map consolidates three independent read-only audits (surfaces / journeys / quality) into one
  deduplicated, blast-radius-ranked view, under a brutal-honesty mandate that refuses to claim
  coverage we don't have (`docs/testing/2026-06-23-gapmap-MASTER.md:1-10`).
- Two structural problems dominate: GREEN-by-vacuum (the runner exits 0 unless `fail>0`, and a normal
  run gates most journeys) and the moat having zero positive proof — the cross-team public-dedup HIT
  that the whole economic thesis rests on is tested nowhere (`docs/testing/2026-06-23-gapmap-MASTER.md:12-28`).
- The P0 tier is false-confidence + the moat: the green-by-vacuum runner (M1), the absent positive
  cross-team dedup HIT (M2), `expect_denied` accepting 404 so a broken route scores as secure (M3),
  and all webhook→tier transitions gated (M4) (`docs/testing/2026-06-23-gapmap-MASTER.md:37-45`).
- The P1 tier is real-client fidelity + the money/runner value paths: curl-not-CLI probes (M5), a 502
  graded PASS (M6), Bazel `findMissingBlobs`/AC never exercised (M7), runner admit/over-cap/reject
  absent (M8), the CAS batch plane untested (M9), and 8 of 12 personas never run (M10)
  (`docs/testing/2026-06-23-gapmap-MASTER.md:47-56`).
- The recommended closure order leads with harness honesty (M1+M3), then the moat journey (M2), then
  provisioning+money+runners (M10+M4+M8), then real-client fidelity (M5+M6+M7)
  (`docs/testing/2026-06-23-gapmap-MASTER.md:83-92`).

# Invariants

- A GREEN/SHIP verdict must require positive assertions to have actually run; an all-gated
  zero-assertion run must NOT pass — this is the M1 fix and the owner's core gate principle
  (`docs/testing/2026-06-23-gapmap-MASTER.md:41-42`).
- The single load-bearing product claim (team A's public dep served to team B as a cache HIT) must be
  proven positively, not only negated — its absence is the highest-value missing test
  (`docs/testing/2026-06-23-gapmap-MASTER.md:42-43`).
- Route-level security probes must distinguish a real deny (401/403) from a route's absence (404);
  treating 404 as a PASS makes them structurally un-failable (`docs/testing/2026-06-23-gapmap-MASTER.md:43-43`).

# Gotchas

- Nothing in the map is a prod bug — the product passed 6 Opus security/code reviews (0 Critical/0
  High); these are gaps in what can be *proven*, not defects (`docs/testing/2026-06-23-gapmap-MASTER.md:91-93`).
- The suites ARE genuinely strong where they do real byte-for-byte round-trips (`cas.rs`/`ac.rs`/
  `concurrency.rs`, the OCI J1-J9 protocol suite, cargo/sccache) — the map must not be misread as
  "all broken" (`docs/testing/2026-06-23-gapmap-MASTER.md:73-79`).

# Citations

1. `docs/testing/2026-06-23-gapmap-MASTER.md:1-10` — the map consolidates the three audits under a brutal-honesty mandate.
2. `docs/testing/2026-06-23-gapmap-MASTER.md:12-28` — the one-paragraph truth: green-by-vacuum + the moat's zero positive proof.
3. `docs/testing/2026-06-23-gapmap-MASTER.md:37-45` — P0 findings M1-M4 (runner verdict, moat HIT, `expect_denied` 404, webhook→tier).
4. `docs/testing/2026-06-23-gapmap-MASTER.md:47-56` — P1 findings M5-M10 (real-client fidelity, money/runner value paths).
5. `docs/testing/2026-06-23-gapmap-MASTER.md:73-79` — what is genuinely strong (real round-trips).
6. `docs/testing/2026-06-23-gapmap-MASTER.md:83-93` — recommended closure order + the "not a prod bug" caveat.
