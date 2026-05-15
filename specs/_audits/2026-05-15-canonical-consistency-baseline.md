---
type: audit
title: Canonical consistency baseline (INV registry × TLA × code × tests)
date: 2026-05-15
reviewer: Agent (R-PREP swarm — canonical-lint worktree)
sprint: R-PREP (GA hardening)
target: invariant_registry.md §3 × specs/tla/*.tla × crates/*/src/ × crates/*/tests/
status: BASELINE_LOCKED
related_validators:
  - scripts/validate_canonical_consistency.py
  - scripts/validate_references.py
  - scripts/check_tla_obligations.py
related_runbooks:
  - specs/_runbooks/RB-CANONICAL-DRIFT.md
---

# Canonical consistency baseline — 2026-05-15

First execution of `scripts/validate_canonical_consistency.py` against
`origin/main` at the start of R-prep canonical-lint wave. The numbers below
become the **CI ratchet floor**: code-referenced / test-referenced /
critical-referenced counts may go UP, never DOWN. Orphan refs may go DOWN,
never UP.

The validator addresses a gap that `validate_references.py` (cross-doc IDs)
and `check_tla_obligations.py` (registry §4 obligation matrix) miss:
**does the IMPLEMENTATION corpus actually reference the invariants the
spec corpus declares?** That cross-check between declared INVs and
production-code mentions is the canonical drift surface we now ratchet.

## 1. Headline counts

| Metric | Count | Interpretation |
|---|---|---|
| **INVs declared** (registry §3 rows) | 154 | Canonical catalogue of invariants |
| └ CRITICAL | 52 | TLA+ + property test mandatory |
| └ HIGH | 98 | Property test mandatory; TLA+ if non-trivial |
| └ MEDIUM | 4 | Unit test mandatory |
| └ UNKNOWN severity | 0 | (parser would flag UNKNOWN if any) |
| **Aliases declared** (registry §5) | 12 | Legacy CamelCase + cross-domain redirects |
| **TLA+ verified** (declared INVs proved in `specs/tla/*.tla`) | 18 | 7 canonical + 4 runbook .tla files |
| **Code-referenced** (declared INVs cited in `crates/*/src/`) | 77 | 50% of declared corpus has src/ pointer |
| **Test-referenced** (declared INVs cited in `crates/*/tests/`) | 65 | 42% of declared corpus has test pointer |

## 2. Drift counts at baseline

| Drift class | Count | Severity at GA |
|---|---|---|
| **orphan-ref** (INV in code, NOT in registry+aliases) | 15 | **HARD FAIL** at GA — every entry is either a typo or a forward-looking INV that must be promoted to registry §3 |
| **declared-no-code-or-test** (in registry; no src/ or test reference) | 68 | Warns; ratcheted in §4 (per-sprint impl WIs absorb these as they ship) |
| **declared-test-only** (test references it but no src/ reference) | 9 | Warns; expected for assertions about absent-behaviour |
| **CRITICAL-no-TLA** (CRITICAL severity but no .tla proof yet) | 40 | Tracked by `check_tla_obligations.py` against registry §4.2 PLANNED matrix; this validator surfaces the same count as a sanity backstop |

## 3. Orphan references inventory (15)

Each orphan = INV-* mentioned in code but neither in registry §3 nor in
§5 aliases. Triage: see `specs/_runbooks/RB-CANONICAL-DRIFT.md` §2.

| Orphan ID | First file | Likely category |
|---|---|---|
| `INV-AC-EVICT-AUDIT-EMITTED` | `crates/corelink-worker/src/reapi/ac/ttl.rs` | Forward-looking — promote to registry §3.15 or §3.18 |
| `INV-AC-EVICT-REGION-PINNED` | `crates/corelink-worker/src/reapi/ac/ttl.rs` | Forward-looking — promote |
| `INV-AC-REGION-PINNED` | `crates/corelink-worker/tests/reapi_v2_ac_conformance.rs` | Forward-looking — promote |
| `INV-AC-TTL-REFRESH-MONOTONIC` | `crates/corelink-ac-schema/src/sim.rs` (+2) | Forward-looking — promote to §3.15 |
| `INV-AUTH-CONSTANT-TIME-COLD-PAD` | `crates/corelink-worker/src/middleware/auth.rs` | Forward-looking — promote to §3.14 |
| `INV-AVAIL-DOS` | `crates/corelink-logpush/src/lib.rs` (+1) | Forward-looking — promote to §3.8 |
| `INV-BACKUP-FRESH` | `crates/corelink-backup-verify/src/lib.rs` | New domain BACKUP — open §3 sub-section |
| `INV-BACKUP-INTEGRITY-SAMPLE-CAP` | `crates/corelink-backup-verify/src/error.rs` (+1) | New domain BACKUP |
| `INV-BACKUP-RESTORE-EPHEMERAL` | `crates/corelink-backup-verify/src/error.rs` (+3) | New domain BACKUP |
| `INV-BILLING-CHAIN-INTEGRITY` | `crates/corelink-billing-aggregator/src/lib.rs` (+1) | Likely alias of `INV-BILLING-LEDGER-CHAIN-INTEGRITY` — add §5 alias OR rename code |
| `INV-ROLLOUT-AUTO-ROLLBACK-TRIGGERS` | `crates/corelink-rollout-controller/src/auto_rollback.rs` (+1) | New domain ROLLOUT — open §3 sub-section |
| `INV-ROLLOUT-BUDGET-CAP` | `crates/corelink-rollout-controller/src/controller.rs` (+2) | New domain ROLLOUT |
| `INV-ROLLOUT-COSIGN-GATE` | `crates/corelink-rollout-controller/src/lib.rs` | New domain ROLLOUT |
| `INV-ROLLOUT-NO-STAGE-SKIP` | `crates/corelink-rollout-controller/src/controller.rs` (+5) | New domain ROLLOUT |
| `INV-ROLLOUT-SINGLE-ACTIVE` | `crates/corelink-rollout-controller/src/controller.rs` (+3) | New domain ROLLOUT |

**Recommendation for R-prep follow-up:** open `INV-BACKUP-*` and
`INV-ROLLOUT-*` sub-sections in registry §3 (currently both fields
implemented in code without a canonical home). Until then these 15 are
**accepted at baseline** — see §4.

## 4. CI ratchet floor (baseline pinned)

The validator reads the BASELINE comments below; reducing any of
`code_referenced` / `test_referenced` / `critical_referenced` beneath
these floors fails CI. The `orphan_refs` floor is the count we accept at
baseline; the next PR that adds another orphan fails CI.

<!-- BASELINE declared=154 -->
<!-- BASELINE tla_verified=18 -->
<!-- BASELINE code_referenced=77 -->
<!-- BASELINE test_referenced=65 -->
<!-- BASELINE critical_referenced=29 -->
<!-- BASELINE orphan_refs=15 -->
<!-- BASELINE declared_no_code=68 -->
<!-- BASELINE critical_no_tla=40 -->

## 5. Forward burn-down expectation

- **R-prep sprints (S-12 .. S-21):** reduce `declared_no_code` from 68 →
  ≤ 40 by promoting forward-looking INVs to canonical or by adding
  property tests as their parent WIs ship.
- **R-7 GA gate:** orphan_refs must be 0 (every code reference must be
  declared or aliased). `critical_no_tla` must be reconciled with
  `check_tla_obligations.py` §4.2 PLANNED matrix.
- **Post-GA:** quarterly review of baseline; tighten floors when a sprint
  closes its INV cluster.

## 6. Cross-linked controls

- **CTRL-FORMAL-001** (`security_model.md §6.9`) — every CRITICAL invariant
  has a TLA+ spec. This validator surfaces the gap as a sanity backstop.
- **SOC 2 CC4.1** (`specs/_compliance/SOC2-EVIDENCE-ROLLUP-2026-05-15.md`)
  — continuous compliance includes spec-corpus consistency monitoring.
- **ROADMAP §7 R-7 gate** (`ROADMAP-TO-GA.md`) — pre-GA evidence pack
  includes the latest canonical-consistency JSON report.
