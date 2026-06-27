---
type: "Runbook"
title: "Performance playbook"
description: "The hot-path performance discipline: per-request allocation/lock/JSON traps and their fixes, the p99 regression gate, and the measured CAS hot-path root cause (two synchronous D1-over-HTTP hops, not Argon2id)."
source_files:
  - "docs/internal/PERFORMANCE-PLAYBOOK.md"
  - "docs/perf/2026-06-19-cas-hot-path-latency.md"
  - ".github/workflows/perf-regression.yml"
checkpoint_sha: "c100df62c1ce7d50185f5102ce1185da0a9fe9f9"
provenance: "AUTHORED"
tags: ["ops", "performance", "latency", "hot-path", "runbook"]
timestamp: "2026-06-26T00:00:00Z"
---

# Performance playbook

Any code on a per-request path (CAS/AC read+write, audit emit, BYOK, tenant-prefix derivation, auth
middleware) pays its cost millions of times, so CoreLink keeps an explicit playbook of the allocation,
locking and parsing traps that quietly inflate p99 — plus a p99 regression gate that blocks merges on a
**tiered** burn (>5% on a perf-critical bench, >15% on a non-critical one; wave-22 split). The companion measured case study is the CAS hot-path latency investigation, which is the
canonical worked example of the playbook's discipline: it *measured* the slow `/v1/cas` path against prod
and disproved the obvious-but-wrong "Argon2id is the cost" hypothesis, locating the real dominator in two
synchronous D1-over-HTTP control-plane round-trips. This runbook is the engineering counterpart to the
[CAS hot-path latency](/storage/cas-hot-path-latency.md) storage concept and governs every change to the
[CAS write flow](/flows/cas-write.md).

# Role
- The hot-path style guide: how to add code to a per-request path without regressing latency or per-request
  allocations (`docs/internal/PERFORMANCE-PLAYBOOK.md:1-17`).
- The regression gate of record: a measured p99 budget that fails CI when a hot kernel slows.
- The measurement discipline: prove the dominant cost before optimizing — the CAS case is the exemplar.

# How it works
1. The playbook is structured per-pattern as trap → fix → bench evidence → when-acceptable, so a reviewer
   can place any change against a known pattern (`docs/internal/PERFORMANCE-PLAYBOOK.md:21-36`).
2. Pattern A bans per-request heap allocation: pass `&[u8]`/`Bytes` (clone = ~8ns refcount bump) instead of
   `Vec<u8>::clone` (~200ns + heap pressure), avoiding ~10-15% of CPU under load
   (`docs/internal/PERFORMANCE-PLAYBOOK.md:38-91`).
3. Pattern E batches D1 writes inside a Durable Object transaction window rather than one round-trip per
   event (`docs/internal/PERFORMANCE-PLAYBOOK.md:286-340`).
4. A new hot-path change runs the cross-cutting checklist before merge (`docs/internal/PERFORMANCE-PLAYBOOK.md:342-371`).
5. The regression gate compares p99 (not median) against a baseline manifest and fails on a **tiered** burn —
   `PERF_REGRESS_CRITICAL_PCT_DEFAULT=5` for perf-critical benches, `PERF_REGRESS_DEFAULT_PCT_DEFAULT=15`
   for the rest (a per-bench `tolerance_pct` override wins), running on Linux CI so it is immune to the
   shared-Mac noise (`.github/workflows/perf-regression.yml:80-81`, `.github/workflows/perf-regression.yml:79-85`;
   `docs/internal/PERFORMANCE-PLAYBOOK.md:372-455`).
6. The CAS case study measured 5 sequential prod requests: ~4.0s cold then a steady ~1.5s warm that does
   NOT decay (`docs/perf/2026-06-19-cas-hot-path-latency.md:18-33`).
7. Root cause: `/v1/cas` is served by the native container which makes two synchronous D1-over-HTTP hops
   ($-ceiling quota charge + 410-tombstone check) before the R2 GET; Argon2id only runs on a PAT-gate
   miss and is skipped warm (`docs/perf/2026-06-19-cas-hot-path-latency.md:9-16`).
8. The remediation is four risk-separated WPs — bulk PUT, drop the two D1 hops, keep-warm, and fast-hash
   PAT verify — each naming the invariant it must not break (`docs/perf/2026-06-19-cas-hot-path-latency.md:55-94`).

# Invariants
- p99, not median, is the gated statistic, and the budget burn threshold is **tiered** — 5% on a
  perf-critical bench, 15% on a non-critical one (wave-22 split; per-bench `tolerance_pct` overrides) — a
  change that burns more fails CI regardless of median (`.github/workflows/perf-regression.yml:51-60`,
  `.github/workflows/perf-regression.yml:153-163`; `docs/internal/PERFORMANCE-PLAYBOOK.md:399-455`).
- The keep-warm WP must be scoped to recently-active tenants, never global: warming every tenant 24/7
  breaks the ~80% SMB margin and is a stakeholder-visible COGS decision
  (`docs/perf/2026-06-19-cas-hot-path-latency.md:82-87`).
- The drop-the-D1-hops WP must keep the $-ceiling fail-CLOSED with a bounded, reconciled overshoot and the
  tombstone structure may have false positives but NEVER false negatives (a served-erased object is a GDPR
  Art.17 breach) (`docs/perf/2026-06-19-cas-hot-path-latency.md:68-80`).
- An optimization claim needs measured (or explicitly projected) bench evidence — the CAS case is the
  standing proof that a plausible code-trace ("blame Argon2id") can be wrong until measured
  (`docs/perf/2026-06-19-cas-hot-path-latency.md:1-16`).

# Gotchas
- The traps are explicitly hot-path-only: on a once-per-hour path none of it matters and readability wins
  — applying the playbook to a cold path is premature optimization (`docs/internal/PERFORMANCE-PLAYBOOK.md:15-17`).
- The warm ~1.5s is the cost WITH Argon2id already skipped (the 5s PAT verify-cache makes req2-5 cache
  hits), which is exactly why "it's Argon2id" is the seductive wrong answer
  (`docs/perf/2026-06-19-cas-hot-path-latency.md:28-33`).
- Native containers do not get the Worker's fast D1 binding — they hit the remote
  `api.cloudflare.com/.../d1/query` HTTP API, so each "quick D1 check" is a ~0.3-0.7s network round-trip
  (`docs/perf/2026-06-19-cas-hot-path-latency.md:50-53`).

# Citations
1. `docs/internal/PERFORMANCE-PLAYBOOK.md:1-17` — audience + rule-of-thumb (hot path only).
2. `docs/internal/PERFORMANCE-PLAYBOOK.md:15-17` — the cold-path exemption.
3. `docs/internal/PERFORMANCE-PLAYBOOK.md:21-36` — the per-pattern trap/fix/bench/when-acceptable structure.
4. `docs/internal/PERFORMANCE-PLAYBOOK.md:38-91` — Pattern A: avoid per-request heap allocation.
5. `docs/internal/PERFORMANCE-PLAYBOOK.md:286-340` — Pattern E: batch D1 writes in a DO transaction window.
6. `docs/internal/PERFORMANCE-PLAYBOOK.md:342-371` — cross-cutting checklist for a new hot-path change.
7. `docs/internal/PERFORMANCE-PLAYBOOK.md:372-455` — how the regression gate works (Linux CI, baseline).
8. `docs/internal/PERFORMANCE-PLAYBOOK.md:399-455` — why p99 / why 10% / baseline manifest / fail handling.
9. `docs/perf/2026-06-19-cas-hot-path-latency.md:1-16` — measurement disproves the Argon2id hypothesis (TL;DR).
10. `docs/perf/2026-06-19-cas-hot-path-latency.md:18-33` — measured cold/warm evidence against prod.
11. `docs/perf/2026-06-19-cas-hot-path-latency.md:50-53` — native container hits the remote D1 HTTP API.
12. `docs/perf/2026-06-19-cas-hot-path-latency.md:55-94` — the 4 remediation work-packages + invariants.
13. `docs/perf/2026-06-19-cas-hot-path-latency.md:68-80` — WP-2 fail-closed quota + no-false-negative tombstone.
14. `docs/perf/2026-06-19-cas-hot-path-latency.md:82-87` — WP-3 keep-warm scoped to active tenants (margin).
15. `.github/workflows/perf-regression.yml:80-81` — the executed env block `PERF_REGRESS_CRITICAL_PCT_DEFAULT: "5"` / `PERF_REGRESS_DEFAULT_PCT_DEFAULT: "15"`: the deployed split (critical benches gate at >5% p99, non-critical at >15%, NOT a flat 10%). (`:11-12` is the descriptive header comment, not the enforcer.)
16. `.github/workflows/perf-regression.yml:51-60` / `:79-85` / `:153-163` — the `critical`/`default` threshold inputs + `PERF_REGRESS_*_PCT_DEFAULT` env defaults (5 / 15) + the split-threshold check step (`tolerance_pct` per-bench override wins).
