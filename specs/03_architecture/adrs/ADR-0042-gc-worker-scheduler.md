---
id: "ADR-0042"
type: "adr"
doc_status: "FROZEN"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-25"
updated: "2026-04-25"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["adr", "gc", "worker", "scheduler", "degrade-mode", "s06"]
---

# ADR-0042 — GC Worker Scheduler Design + Degrade-Mode `gc-pause` Contract

## Status

FROZEN (S-06 WI-S06-001 ratificada em Lote 10.6 ship gate WI-S06-007).

## Context

S-06 GC worker é **single point of failure** para INV-GC-001 (reachable never deleted). Worker scheduler design impacta:

1. **Cron schedule** (per-region; jitter ±10min para evitar thundering herd 5 regions × 02:00 UTC).
2. **Sticky DO per region** (consistente WI-S04-005 sweeper pattern).
3. **Idempotent re-run via checkpoint** (gc_run table; PAT-RETRY-IDEMPOTENT-001).
4. **Single running per (tenant, region)** (partial UNIQUE WHERE status='running'; lesson Lote 10.5bis partial UNIQUE).
5. **Phase transitions monotonic** (idle→mark→sweep→physical_delete→reconcile→completed).
6. **Degrade-mode `gc-pause` global emergency stop** (PAT-DEGRADE-001 alignment).
7. **Manual trigger admin API** (forward S-13; staging stub OK).

## Decision

**Per-region sticky DO + cron 02:00 UTC + jitter ±10min + degrade-mode gc-pause via DO config-singleton + manual admin trigger forward S-13.**

**Architectural choices**:

| Aspect | Decision | Rationale |
|---|---|---|
| Worker topology | 5 regions × 1 sticky DO each | Pattern reuse WI-S04-005 sweeper; per-region isolation; aligned com R2 bucket region attribution |
| Cron schedule | 02:00 UTC daily + jitter ±10min | Off-peak hour; jitter spreads thundering herd; configurable env `CORELINK_GC_CRON_JITTER_MINUTES` |
| Concurrency control | Partial UNIQUE INDEX `WHERE status='running'` | Lote 10.5bis partial UNIQUE lesson; race-free; allows multiple completed/aborted records as audit trail |
| Idempotent resume | Checkpoint per batch boundary em gc_run.last_checkpoint_at_ms | PAT-RETRY-IDEMPOTENT-001; worker crashes resume from last checkpoint |
| Phase tracking | gc_run.phase enum monotonic | CHECK constraint inline; reverse transitions rejected |
| Degrade-mode | DO config-singleton + probe per batch boundary | Bounded propagation ≤100ms; PAT-DEGRADE-001 |
| Manual trigger | Admin API stub forward S-13 | Staging stub returns 501 in non-staging envs; per-tenant rate limit forward |
| Stale detection | Index `WHERE status='running' AND last_checkpoint > 1h` | Crashed workers detected within 1h; cron daily promotes to status='crashed' |
| gc_run cleanup | Daily cron truncate > 30d old | Bounded table size; configurable env |

**Rejected alternatives**:

- **Single global cron** (não per-region): single failure point; coordination overhead; rejected per WI-S04-005 pattern.
- **No jitter** (5 regions fire simultaneously): D1 throttle thundering herd; rejected.
- **Hash-based concurrency control** (não partial UNIQUE): D1 has no native row-locks for hashing; partial UNIQUE é cleanest SQLite-supported pattern.
- **Worker as monolithic** (não phase-tracked): impossible to checkpoint mid-phase; rejected for idempotent-resume requirement.
- **Synchronous degrade-mode** (não probe per batch): degrade-mode propagation latency unbounded; rejected.

## Consequences

**Positive**:
- Per-region scheduler scales horizontally (more regions = more workers; no coordination).
- Idempotent re-run handles crashes gracefully.
- Partial UNIQUE prevents race; race-free per-tenant per-region single-flight.
- Degrade-mode bounded propagation ≤ 100ms (probe per batch boundary).
- Pattern reuse from WI-S04-005 sweeper reduces cognitive load for ops team.

**Negative**:
- Per-region sticky DO means single tenant's GC runs sequentially per region (não parallelizable per-tenant); for fat tenants > 5M blobs phase budget exceeded → SEV-2 alert.
- Manual admin trigger forward S-13 (staging stub may surprise prod ops; 501 fallback documented).

**Neutral**:
- gc_run table size bounded to 30d retention (~15M rows max em high-volume tenant; D1 single-shard fits).

## References

- WI-S06-001 §1 worker skeleton design.
- WI-S06-002..005 consume scheduler.
- ADR-0019 (TTL ownership; sweeper pattern reuse).
- ADR-0034 (PRR staffing waiver path).
- ADR-0036 (schema migration governance) — gc_run schema reuse pattern.
- `failure_modes.md FM-300/305/404` — runbook RB-FM-* dry-runs em WI-S06-007.
- `resilience_patterns.md PAT-DEGRADE-001 + PAT-RETRY-IDEMPOTENT-001`.

## §A1 Addendum — TLC v1.8.0 Version + SHA-256 Pinning Policy (Lote 10.6-tris NEW-P0-1 + Lote 10.6bis P0-W6-2)

**Policy**: TLA+ Tools (`tla2tools.jar`) version + SHA-256 are pinned in CI workflow `tla_check.yml`. Bumps to either require ADR + Architect + Crypto SME signoff in this addendum.

**Current pinned values** (2026-04-25):
- Version: `v1.8.0`
- SHA-256: `d5d07d5dab38ddb840c91ec48fa02f28b37a608d5af9a73570018591dbc8ef7f`
- Source: `https://github.com/tlaplus/tlaplus/releases/download/v1.8.0/tla2tools.jar`
- Artifact size: 4356704 bytes

**Bootstrap trust ceremony** (REQUIRED pre-merge of `tla_check.yml` to main):
1. Owner (Gustavo Schneiter) computed SHA via fresh download from official GitHub release on 2026-04-25.
2. **Architect**: independent re-download + `shasum -a 256 tla2tools.jar` verification → commit signed verification comment to this addendum sign-off block confirming SHA matches.
3. **Crypto SME**: independent re-download (different machine, different network) + verification → commit signed verification comment.
4. Both signatures REQUIRED before `tla_check.yml` merges to main. Branch protection enforced via CODEOWNERS + `tla_override_validate.yml` (PLANNED WI-S06-006 deliverable — not yet in tree; WI-006 §1 invariant 2).

**Sign-off block** (populated as ceremony completes):
- [ ] Owner: Gustavo Schneiter (computed 2026-04-25)
- [ ] Architect: TBD — independent re-verification
- [ ] Crypto SME: TBD — independent re-verification

**Bump procedure** (TLC version OR SHA change):
1. Open ADR (this file or supersession ADR).
2. Architect + Crypto SME independent verification of new SHA.
3. Update `TLC_SHA256_PINNED` literal in `tla_check.yml`.
4. CI must pass green on a test PR before merge.
5. Update this addendum with new pinned values + sign-off block.

**Why pinning matters**: TLC is the formal verification baseline. A compromised binary (maintainer key compromise; GitHub account takeover; CDN MITM) running in CI would silently report `0 errors found` for any input — voiding all formal verification claims. Pinning + bootstrap ceremony establishes a chain of trust.

---

## §A2 Addendum — TLC cfg Bounds Documentation (Lote 10.6-tris OPUS-MISS-1)

**Policy**: `gc_correctness.cfg` SETS bounds are documented here; changes require ADR.

**Current bounds** (verified em `specs/tla/gc_correctness.cfg` Lote 10.6 cycle 2):
- `Blobs = {b1, b2}` — 2 blobs (sufficient for race scenarios with ≥1 reachable + ≥1 orphan)
- `AC_Entries = {e1}` — 1 AC entry (sufficient for INV-GC-004 boundary cases at minimal cardinality)
- `MaxTime = 10` — time horizon for interleaving exhaustion
- `GracePeriod = 2` — abstract time units modeling 72h CAS grace
- **Note**: Bounds intentionally minimal for CI tractability (~5k-50k states; ≤30s TLC). Production assurance via larger bounds em distributed TLC + Apalache symbolic + property test 100k random sampling (defense-in-depth layer 2).

**Coverage at these bounds**: TLC exhaustively explores all interleavings of `Mark + UpdateActionResult + Sweep` actions over the bounded state space. Property test 100k extends coverage via random sampling against real Rust impl at larger scale (defense-in-depth layer 2).

**Scope limitation**: TLC at these bounds does NOT prove the algorithm correct for ≥4 blobs concurrently referenced by the same AC entry, nor for ≥5 concurrent AC entries. Property test 100k provides probabilistic coverage at larger scale.

**Bump procedure**: increasing bounds requires ADR (state space grows exponentially; cost gate ≤5min p99 per PR may be exceeded).

---

## §A3 Addendum — TLA+ Formal Verification SCOPE (Lote 10.6-tris NEW-P0-2)

**Policy**: `gc_correctness.tla` covers the mark-sweep algorithm's correctness. Soft-delete grace window, DSR bypass auth, and physical-delete cron orchestration are NOT in TLA+ scope.

**What `gc_correctness.tla` proves**:
- INV-GC-001 (reachable never deleted) at the active → physically_deleted transition.
- INV-GC-004 (mark-phase-aware re-ref) via `ac.created_at >= mark_started_at` protect-if-equal-or-newer canonical TLA semantics (`gc_correctness.tla` L152-154; equivalent: delete only if all `ac.created_at < mark_started_at`).
- All interleavings of `Mark + UpdateActionResult` actions at the bounded state space (§A2 bounds).

**What `gc_correctness.tla` does NOT prove** (covered architecturally instead):
- **Soft-delete grace window** (72h CAS / 24h AC): protected by (a) `WHERE refcount = 0` conditional D1 predicate in WI-S06-004 physical-delete; (b) `undelete` path via re-upload (CAP-GC-002).
- **DSR bypass auth**: protected by Ed25519 verify + `dsr_signals_processed.signal_id` UNIQUE + scope rejection (WI-S06-004 §1.4).
- **Physical-delete cron orchestration**: operational; crash-recovery via WI-S06-005 reconcile orphan detection.

**Implication**: "INV-GC-001 formally proven" claim is correct AT THE ALGORITHM LEVEL but does NOT extend to grace-window reversibility. Crypto SME PRR review must acknowledge this scope.

**Future TLA+ extension** (S-07+ deferred): extend `gc_correctness.tla` with `soft_deleted` state + `grace_period` time variable + `Undelete` action; verify INV-GC-001 across two-phase soft+physical delete. Effort ~8h TLA+ + ~4h Crypto SME re-review.

---

## Change log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-25 | Gustavo (via Claude Opus 4.7) | Criação ADR-0042 (Lote 10.6 worker scheduler design). Per-region sticky DO; cron 02:00 + jitter; partial UNIQUE concurrency control; idempotent resume; degrade-mode gc-pause probe per batch. |
| 1.1.0 | 2026-04-25 | Gustavo (via Claude Opus 4.7) | Lote 10.6-tris: §A1 addendum TLC v1.8.0 SHA-256 pinning policy (`d5d07d5dab38ddb840c91ec48fa02f28b37a608d5af9a73570018591dbc8ef7f`) + bootstrap ceremony; §A2 addendum TLC cfg bounds documentation; §A3 addendum TLA+ formal verification SCOPE limitations (soft-delete grace window NOT in TLA+ coverage). |
