---
type: "TestStrategy"
title: "CI gate machinery & quality gaps"
description: "The test-quality / methodology weakness map: how the user-simulation suites are built, where they USED to create false confidence (green-by-vacuum, 404-as-PASS, curl-not-CLI, 502-as-PASS), the fix themes, and the now-LANDED harness fixes (Q1-Q5 all closed: the green-by-vacuum runner, the 404-deny helper, the real-client 502-FAIL + run.sh PASS-floor, and the strict-409 AC integrity journey)."
source_files:
  - "docs/testing/2026-06-23-gapmap-quality.md"
  - "tests/e2e-user-journeys/src/harness.rs"
  - "tests/e2e-user-journeys/src/main.rs"
  - "tests/e2e-user-journeys/src/journeys/ac.rs"
  - "scripts/e2e-real-client/run.sh"
  - "scripts/e2e-real-client/lib/clients.sh"
checkpoint_sha: "a367df9b6df02af27b91ef22a6d3a53824eca42d"
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
contracts the code doesn't assert. **The two harness-level findings (Q1 green-by-vacuum, Q2 the
404-as-deny helper) are now FIXED in the live harness** (#484/#489/#495/#501): the runner refuses
green-by-vacuum and goes RED on an all-gated run (`tests/e2e-user-journeys/src/main.rs:117-163`), and
a separate gate-probe helper `expect_gate_denied` accepts 401/403 ONLY — a 404 is a failure
(`tests/e2e-user-journeys/src/harness.rs:436`). **The remaining three (Q3 502-as-PASS, Q4
SHIP-by-vacuum, Q5 the AC-tautology) are now ALSO FIXED**: the real-client harness classifies every
status so a 5xx/502 is a hard FAIL (`scripts/e2e-real-client/lib/clients.sh:208-237`), `run.sh` is
NO-SHIP unless `PASS ≥ CORELINK_E2E_MIN_PASS` so an all-gated credential-less run can no longer
false-certify (`scripts/e2e-real-client/run.sh:33-37`, `scripts/e2e-real-client/run.sh:120-130`), and the AC divergent-body journey now
requires a strict 409 + original-body-preserved instead of the 409-OR-LWW tautology (the "M11 fix",
`tests/e2e-user-journeys/src/journeys/ac.rs:158,198-218`). It is the methodology companion to the
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
- Q1 (CRITICAL — **NOW FIXED**) GREEN-by-vacuum: the runner USED to exit 0 unless `fail>0`, so an
  all-gated zero-assertion run printed a banner containing "GREEN" and exited 0
  (`docs/testing/2026-06-23-gapmap-quality.md:23-42`). The live runner now refuses this: a PASS floor
  (`CORELINK_E2E_MIN_PASS`, default 1) makes a below-floor run RED ("Nothing substantive ran; this is
  NOT a green"), and an optional gated CEILING (`CORELINK_E2E_MAX_GATED`) catches a single load-bearing
  journey silently flipping Pass→Gated — so an all-gated or re-gated run exits non-zero
  (`tests/e2e-user-journeys/src/main.rs:117-163`).
- Q2 (HIGH — **NOW FIXED**) deny-probes PASS on 404: the deny helper USED to be a single `expect_denied`
  accepting 401|403|404 at every call-site (`docs/testing/2026-06-23-gapmap-quality.md:44-76`). The
  helper is now SPLIT: `expect_gate_denied` accepts 401/403 ONLY — a 404 is an explicit FAILURE
  ("the gate is absent/renamed, not that it rejected — security property UNPROVEN") and is used for
  every active-gate probe (edge-mint, priv-esc, internal-introspect, header-strip)
  (`tests/e2e-user-journeys/src/harness.rs:436`); the legacy `expect_denied` (401|403|404) survives
  ONLY for paths where a hidden-object 404 is the privacy-preserving contract (e.g. tenant-isolation
  reads), with a doc-comment forbidding its use to prove a gate rejected
  (`tests/e2e-user-journeys/src/harness.rs:420`).
- Q3 (CRITICAL — **NOW FIXED**) the real-client harness is curl-shaped, not the real CLI (only docker +
  cargo/sccache drive a real binary), and it graded a known-bad 502 — the `_public` fail-closed
  signature — as PASS in the committed run (`docs/testing/2026-06-23-gapmap-quality.md:78-116`). The
  502-as-PASS half is now closed: the brew/artifact probe classifies every status code and a `server5xx`
  (including the `_public` fail-closed 502) is an explicit FAIL, never the old `*)` catch-all PASS
  (`scripts/e2e-real-client/lib/clients.sh:208-237`).
- Q4 (HIGH — **NOW FIXED**) `run.sh` USED to declare SHIP when it could bootstrap nothing (no Clerk
  secret → entire run gates → exits 0/SHIP) (`docs/testing/2026-06-23-gapmap-quality.md:118-133`). It now
  carries an anti-vacuum PASS floor: a run with `PASS < CORELINK_E2E_MIN_PASS` (default 1) is
  `NO-SHIP (vacuum)` / exit 1 — so a credential-less all-gated run can no longer false-certify a launch
  (`scripts/e2e-real-client/run.sh:33-37`, `scripts/e2e-real-client/run.sh:120-130`; the floor is
  computed once in `emit_cert`/`E2E_VERDICT_RC`).
- Q5 (HIGH — **NOW FIXED**) the AC divergent-body journey USED to be named "→ 409 integrity guard" but
  accept BOTH 409 and last-write-wins — a tautology that advertised AC-poisoning coverage it didn't have
  (`docs/testing/2026-06-23-gapmap-quality.md:135-154`). The "M11 fix" rewrote it
  (`divergent_body_integrity_guard`): the second divergent re-PUT MUST be 409 (a 200/201 LWW is a FAILED
  cache-poisoning regression) and the follow-up GET MUST return the ORIGINAL body
  (`tests/e2e-user-journeys/src/journeys/ac.rs:158,198-218`).
- The cross-cutting fix themes: fail-closed on "asserted nothing," split the deny helper (401/403 for
  gate probes), grade real-client probes on bytes + the 502 signature, make names match assertions,
  and provision the full persona set (`docs/testing/2026-06-23-gapmap-quality.md:324-339`).

# Invariants

- A GREEN/SHIP verdict must require `pass>0` on the journeys that were supposed to run; an all-gated
  run must be a distinct non-green verdict / non-zero exit (`docs/testing/2026-06-23-gapmap-quality.md:326-328`)
  — **now ENFORCED** by the runner's PASS-floor + gated-ceiling (`tests/e2e-user-journeys/src/main.rs:117-163`).
- A 404 is a valid PASS only where a hidden-object 404 is the contract — never on
  route-existence/internal-gate probes, which need a positive reachability assertion
  (`docs/testing/2026-06-23-gapmap-quality.md:329-332`) — **now ENFORCED** by the
  `expect_gate_denied` (401/403-only) helper for gate probes (`tests/e2e-user-journeys/src/harness.rs:436`).
- Real-client probes must FAIL (not PASS) on a 502 and must GET-and-compare bytes, not assert status
  codes only (`docs/testing/2026-06-23-gapmap-quality.md:333-335`) — **now ENFORCED**: a 5xx is a hard
  FAIL in the artifact probe (`scripts/e2e-real-client/lib/clients.sh:208-237`).
- A real-client SHIP verdict must clear the anti-vacuum PASS floor — an all-gated/credential-less run is
  `NO-SHIP (vacuum)`, never a silent green — **now ENFORCED** by `CORELINK_E2E_MIN_PASS`
  (`scripts/e2e-real-client/run.sh:33-37`, gating block `scripts/e2e-real-client/run.sh:120-130`).
- The AC divergent-body re-PUT must be graded strictly (409 + original-body-preserved, never 409-OR-LWW)
  so it actually proves AC cache-poisoning resistance — **now ENFORCED**
  (`tests/e2e-user-journeys/src/journeys/ac.rs:198-218`).

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
10. `tests/e2e-user-journeys/src/main.rs:117-163` — Q1 FIX: the runner refuses green-by-vacuum (PASS floor `CORELINK_E2E_MIN_PASS` + gated ceiling `CORELINK_E2E_MAX_GATED` → RED on all-gated / Pass→Gated).
11. `tests/e2e-user-journeys/src/harness.rs:436` — Q2 FIX: `expect_gate_denied` accepts 401/403 ONLY (a 404 is a FAILURE) for active-gate probes.
12. `tests/e2e-user-journeys/src/harness.rs:420` — the legacy `expect_denied` (401|403|404) retained ONLY for hidden-object/tenant-isolation 404-deny paths.
13. `scripts/e2e-real-client/lib/clients.sh:208-237` — Q3 FIX: the artifact probe classifies every status; a `server5xx` (incl. the `_public` 502) is an explicit FAIL, not the old `*)` catch-all PASS.
14. `scripts/e2e-real-client/run.sh:33-37` (exit-code contract) + `scripts/e2e-real-client/run.sh:120-130` (the gating block) — Q4 FIX: the anti-vacuum PASS floor (`CORELINK_E2E_MIN_PASS`, default 1) makes a credential-less all-gated run `NO-SHIP (vacuum)`/exit 1.
15. `tests/e2e-user-journeys/src/journeys/ac.rs:158,198-218` — Q5 FIX ("M11"): `divergent_body_integrity_guard` requires a strict 409 on the divergent re-PUT (200/201 LWW = FAIL) + the original body on the follow-up GET.
