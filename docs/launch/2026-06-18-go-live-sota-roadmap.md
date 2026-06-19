# Go-live SOTA roadmap — all-tier remediation (techlead, 2026-06-18)

Validation model (owner-ratified): **CI = local-verify on a quiet Mac → `--admin` merge with documented reason.** Not a bypass; the sanctioned gate. Rust WPs are SEQUENCED (the shared Mac is disk-starved — no parallel from-scratch compiles); non-Rust + read-only WPs parallelize. The lead reviews every diff before merge.

## Owner-only (Tier 0 + infra) — NOT decomposable to agents
| # | Item | Why it gates SOTA |
|---|---|---|
| O1 | Delete the duplicate Stripe webhook endpoint in the dashboard | code gate is defense-in-depth; canonical dedup is the dashboard (task #46) |
| O2 | Verify a PagerDuty/Better Stack page fires end-to-end (+ email rule) | never tested; can't launch blind to prod alerts |
| O3 | Confirm the leaked `whsec` is rotated/dead in Stripe | — |

## WAVE A — parallel-safe (non-Rust / light) — DISPATCHED
| WP | Tier item | Files (disjoint) | DoD |
|----|-----------|------------------|-----|
| A1 | safe-3 deps drain (#8 partial) | package.json/lockfile + CHANGELOG | rebased on main, frozen-lockfile + admin-ui/docs build+lint green |
| A2 | multi-region over-count #11 | worker/src/index.ts | fanout no longer double-meters; tier-resolve + storageQuotaHeader stay unconditional |
| A3 | worker test-debt #9 | worker/tests/** | green the fixable (health-env stale); document the genuinely-blocked (miniflare pool); 0 unexplained reds |
| A4 | secrets/config hygiene #6/#17/#18/#24 | durable_object.ts + secrets-matrix + scripts + workflow | `check-env-contract.py` green + wired as a gate; HUGR_ legacy alias documented; keep the routes.rs fallback (prod uses HUGR_) |

## WAVE B — Rust, SEQUENCED (one at a time, scoped `cargo check -p`)
| WP | Tier item | Crate | Note |
|----|-----------|-------|------|
| B1 | storage OCI residual #10 | container OCI write path | thread the resolved cap into the OCI write |
| B2 | events fairness #8/#9 | turbo_v8 | body-read timeout + per-tenant `/events` cap |
| B3 | Argon2id per-tenant fairness #1/#12 | adapter_pat | auth HOT PATH — verify with **shuttle** (async, not loom). Highest blast radius → last + most careful |
| B4 | migration ledger reconcile #7 | scripts + prod D1 ledger | DR landmine; DESIGN first, owner-paired before any prod-ledger mutation |
| — | OCI in-flight tuning #5 | — | DEFERRED (needs real push-size workload data) — documented, owner-aware |

## WAVE C — verification (read-only, parallel) — Tier 2/3
| WP | Item |
|----|------|
| C1 | Tier-3 sweep: admin-ui checkout e2e (the live money UI), docs deploy, DPA/subprocessors, rollback/DR runbook, observability |
| C2 | Tier-2 #11 PAT key parity e2e (signup→PAT→native 200) + #12 over-cap 402/429 e2e |

## Merge order (DAG)
A1–A4 land independently (sequential `--admin`, resolve CHANGELOG per merge) → B1 → B2 → B3 (each local-verified, gated on the prior to keep the Mac sane) → B4 (owner-paired). C runs anytime (read-only). Deps majors #290/#315 = QA'd fast-follow.
