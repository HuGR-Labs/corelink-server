---
type: "TestStrategy"
title: "CI gate machinery & quality gaps"
description: "The test-quality / methodology weakness map: how the user-simulation suites are built, where they create false confidence (green-by-vacuum, 404-as-PASS, curl-not-CLI, 502-as-PASS), and the fix themes."
source_files:
  - "docs/testing/2026-06-23-gapmap-quality.md"
checkpoint_sha: "c100df62c1ce7d50185f5102ce1185da0a9fe9f9"
provenance: "AUTHORED"
tags: ["testing", "ci", "gates", "false-confidence", "methodology"]
timestamp: "2026-06-26T00:00:00Z"
---

# CI gate machinery & quality gaps

A test that passes by 503/skip/gate/wrong-shape is worse than no test, because it manufactures
confidence the system doesn't earn. This concept captures the test-QUALITY audit of CoreLink's
user-simulation suites — the lens of HOW the tests are built (assertion strength, false confidence,
fidelity) rather than which surface is missing. It catalogs the concentrated false-confidence: the
green-by-vacuum runner, the deny-probe family that scores a 404 as a security PASS, the real-client
shell harness that is mostly curl and graded a known-bad 502 as PASS, and journeys whose names claim
contracts the code doesn't assert. It is the methodology companion to the
[e2e strategy](/testing/e2e-strategy.md) gap map and the
[real-user suites](/testing/real-user-suites.md).

# Role

It is the gate-honesty audit: every entry is `file:line` · what it FALSELY implies · what real failure
it would MISS · severity — so the suites can be fixed to fail-closed on "asserted nothing" and to
distinguish a real gate from an absent route.

# How it works

- The audit's lens is HOW the tests are built (assertion strength/false-confidence/fidelity), and its
  net read is that the per-surface Rust happy-path journeys are mostly STRONG while the
  false-confidence is concentrated in a few places (`docs/testing/2026-06-23-gapmap-quality.md:1-20`).
- Q1 (CRITICAL) GREEN-by-vacuum: the runner exits 0 unless `fail>0`, so an all-gated zero-assertion
  run prints a banner containing "GREEN" and exits 0 — a machine/cron consumes a pass that asserted
  nothing (`docs/testing/2026-06-23-gapmap-quality.md:23-42`).
- Q2 (HIGH) deny-probes PASS on 404: the single `expect_denied` helper accepts 401|403|404 across ~50
  call-sites, so route-level security probes (internal-introspect, webhook-unsigned, no-auth) pass on
  exactly the response that means the gate never ran (`docs/testing/2026-06-23-gapmap-quality.md:44-76`).
- Q3 (CRITICAL) the real-client harness is curl-shaped, not the real CLI (only docker + cargo/sccache
  drive a real binary), and it graded a known-bad 502 — the `_public` fail-closed signature — as PASS
  in the committed run (`docs/testing/2026-06-23-gapmap-quality.md:78-116`).
- Q4 (HIGH) `run.sh` declares SHIP when it could bootstrap nothing (no Clerk secret → entire run gates
  → exits 0/SHIP), and grades the docker digest on an empty value as PASS
  (`docs/testing/2026-06-23-gapmap-quality.md:118-133`).
- Q5 (HIGH) the `ac.rs::divergent_body_reput` journey is named "→ 409 integrity guard" but accepts
  BOTH 409 and last-write-wins — a tautology that advertises AC-poisoning coverage it doesn't have
  (`docs/testing/2026-06-23-gapmap-quality.md:135-154`).
- The cross-cutting fix themes: fail-closed on "asserted nothing," split the deny helper (401/403 for
  gate probes), grade real-client probes on bytes + the 502 signature, make names match assertions,
  and provision the full persona set (`docs/testing/2026-06-23-gapmap-quality.md:324-339`).

# Invariants

- A GREEN/SHIP verdict must require `pass>0` on the journeys that were supposed to run; an all-gated
  run must be a distinct non-green verdict / non-zero exit (`docs/testing/2026-06-23-gapmap-quality.md:326-328`).
- A 404 is a valid PASS only where a hidden-object 404 is the contract — never on
  route-existence/internal-gate probes, which need a positive reachability assertion
  (`docs/testing/2026-06-23-gapmap-quality.md:329-332`).
- Real-client probes must FAIL (not PASS) on a 502 and must GET-and-compare bytes, not assert status
  codes only (`docs/testing/2026-06-23-gapmap-quality.md:333-335`).

# Gotchas

- The map is explicitly NOT "all broken": `cas.rs`/`ac.rs`/`concurrency.rs` real round-trips, the
  `oci.rs` protocol suite, `pat_lifecycle.rs` (which correctly refuses a 404 on revoke-deny), and the
  RFC-4231 Stripe-signature vector are genuinely strong (`docs/testing/2026-06-23-gapmap-quality.md:306-322`).
- The headline counts mix cred-free deny-probes and GATED journeys into "PASS/GREEN," so the honest
  count of real positive byte-verified assertions is far below the advertised number
  (`docs/testing/2026-06-23-gapmap-quality.md:293-302`).

# Citations

1. `docs/testing/2026-06-23-gapmap-quality.md:1-20` — the test-quality lens + net read.
2. `docs/testing/2026-06-23-gapmap-quality.md:23-42` — Q1: GREEN-by-vacuum (exits 0 with zero assertions).
3. `docs/testing/2026-06-23-gapmap-quality.md:44-76` — Q2: deny-probes PASS on 404.
4. `docs/testing/2026-06-23-gapmap-quality.md:78-116` — Q3: curl-not-CLI real-client harness + 502-as-PASS.
5. `docs/testing/2026-06-23-gapmap-quality.md:118-133` — Q4: SHIP on bootstrap-nothing + empty-digest PASS.
6. `docs/testing/2026-06-23-gapmap-quality.md:135-154` — Q5: the AC `divergent_body` tautology.
7. `docs/testing/2026-06-23-gapmap-quality.md:293-302` — coverage-claim honesty: GATED/deny folded into PASS.
8. `docs/testing/2026-06-23-gapmap-quality.md:306-322` — what is genuinely strong.
9. `docs/testing/2026-06-23-gapmap-quality.md:324-339` — the cross-cutting fix themes.
