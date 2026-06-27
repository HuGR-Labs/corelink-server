---
type: "TestStrategy"
title: "End-to-end testing strategy"
description: "The consolidated gap map of CoreLink's user-simulation suites: broad in shape but thin in what actually runs+asserts, with two dominant structural problems (green-by-vacuum, the unproven moat)."
source_files:
  - "docs/testing/2026-06-23-gapmap-MASTER.md"
  - "tests/e2e-user-journeys/src/main.rs"
  - "tests/e2e-user-journeys/src/harness.rs"
  - "scripts/e2e-real-client/lib/clients.sh"
checkpoint_sha: "a367df9b6df02af27b91ef22a6d3a53824eca42d"
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
- Two structural problems dominated the audit: GREEN-by-vacuum (the runner exited 0 unless `fail>0`,
  and a normal run gates most journeys) and the moat having zero positive proof — the cross-team
  public-dedup HIT that the whole economic thesis rests on is tested nowhere
  (`docs/testing/2026-06-23-gapmap-MASTER.md:12-28`). **The green-by-vacuum half (M1) is now FIXED in
  the harness** — the runner requires a PASS floor and goes RED on an all-gated run
  (`tests/e2e-user-journeys/src/main.rs:117-163`); the moat-proof gap (M2) remains open.
- The P0 tier was false-confidence + the moat: the green-by-vacuum runner (M1 — **now FIXED**,
  `tests/e2e-user-journeys/src/main.rs:117-163`), the absent positive cross-team dedup HIT (M2, still
  open), `expect_denied` accepting 404 so a broken route scored as secure (M3 — **now FIXED**: the
  gate-probe helper `expect_gate_denied` accepts 401/403 only, `tests/e2e-user-journeys/src/harness.rs:436`),
  and all webhook→tier transitions gated (M4) (`docs/testing/2026-06-23-gapmap-MASTER.md:37-45`).
- The P1 tier is real-client fidelity + the money/runner value paths: curl-not-CLI probes (M5), a 502
  graded PASS (M6 — **now FIXED**: the real-client artifact probe classifies every status and a 5xx/502
  is a hard FAIL, `scripts/e2e-real-client/lib/clients.sh:208-237`), Bazel `findMissingBlobs`/AC never
  exercised (M7), runner admit/over-cap/reject absent (M8), the CAS batch plane untested (M9), and 8 of
  12 personas never run (M10) (`docs/testing/2026-06-23-gapmap-MASTER.md:47-56`).
- The recommended closure order led with harness honesty (M1+M3 — **both now landed**), then the moat
  journey (M2, still the highest-value open item), then provisioning+money+runners (M10+M4+M8), then
  real-client fidelity (M5+M6+M7 — M6 now landed, `scripts/e2e-real-client/lib/clients.sh:208-237`)
  (`docs/testing/2026-06-23-gapmap-MASTER.md:83-92`).

# Invariants

- A GREEN/SHIP verdict must require positive assertions to have actually run; an all-gated
  zero-assertion run must NOT pass — the owner's core gate principle, **now ENFORCED in the runner**
  via the PASS floor + gated ceiling (`docs/testing/2026-06-23-gapmap-MASTER.md:41-42`,
  `tests/e2e-user-journeys/src/main.rs:117-163`).
- The single load-bearing product claim (team A's public dep served to team B as a cache HIT) must be
  proven positively, not only negated — its absence is the highest-value missing test
  (`docs/testing/2026-06-23-gapmap-MASTER.md:42-43`).
- Route-level security probes must distinguish a real deny (401/403) from a route's absence (404);
  treating 404 as a PASS makes them structurally un-failable (`docs/testing/2026-06-23-gapmap-MASTER.md:43-43`)
  — **now ENFORCED** by `expect_gate_denied` (401/403-only) for active-gate probes
  (`tests/e2e-user-journeys/src/harness.rs:436`).

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
7. `tests/e2e-user-journeys/src/main.rs:117-163` — M1 FIX (live): the runner's PASS-floor + gated-ceiling refuse green-by-vacuum.
8. `tests/e2e-user-journeys/src/harness.rs:436` — M3 FIX (live): `expect_gate_denied` (401/403-only) for active-gate probes.
9. `scripts/e2e-real-client/lib/clients.sh:208-237` — M6 FIX (live): the real-client artifact probe grades a 5xx/502 (the `_public` fail-closed signature) as a hard FAIL, not the old catch-all PASS.
