---
id: AUDIT-AGENT-R4-S07-WI-REVIEW
type: audit
doc_status: REVIEW
audit_status: ACTIVE
version: 1.0.0
created: 2026-04-25
reviewer: Agent R4 (Claude Opus 4.7 1M context)
scope: S-07 all 5 WIs + sprint contract v1.1.0 + 5 NEW INVs §3.18 + ADR-0019 + ADR-0020
sprint_contract: specs/04_sprints/S07/_spec_contract.md v1.1.0
tags: [audit, sota, lote-10.7, s-07, work-items, agent-r4, adversarial]
---

# Agent R4 — Lote 10.7 S-07 Adversarial WI Review

## 0. Executive Summary

**SOTA score this lote: 7.6 / 10.** Below the 9-10 SOTA bar the user demanded, and notably below S-06's post-bis aggregate (8.0+). The dev's claim of "Sonnet R5 lessons preemptively absorbed" is *partially* true at the surface level (TenantCtx-only is consistent across all 5 WIs; `json_each` idiom is correctly cited in WI-S07-002; D1 batch ≤250 is mentioned everywhere; alarm re-arm AT START is present), BUT the spec contains **two load-bearing schema-level defects that no amount of pattern absorption could mask** — one of which (P0-1) silently invalidates the entire dedup design built on top of it, and one of which (P0-2) makes WI-S07-003's quota state machine reference a column that does not exist in the canonical data model. These are *not* "lessons-from-prior-sprint" defects; they are first-principles schema-misreading defects that should have been caught by a careful 30-min `data_model.md` re-read before WI authoring.

Headline P0s:
1. **WI-S07-001 conflates `manifest_chunks` with `chunks`** and proposes `CREATE UNIQUE INDEX idx_manifest_chunks_tenant_digest ON manifest_chunks(tenant_id, chunk_digest)` — this would **break the existing schema**: WI-S05-004 §6.1 establishes `manifest_chunks` PK as `(tenant_id, blob_digest, chunk_index)` precisely to allow the same `chunk_digest` to appear in MULTIPLE manifests within the same tenant (which is the entire point of dedup — N manifests pointing to 1 chunk via refcount). A UNIQUE constraint on `(tenant_id, chunk_digest)` against `manifest_chunks` would reject the second manifest's INSERT and **completely break content-addressable reuse**. The correct dedup index already exists in WI-S05-004: the `chunks` table with PK `(tenant_id, chunk_digest)` and `refcount` column. WI-S07-001 should query `chunks` (not `manifest_chunks`).
2. **WI-S07-003 references `tenant_quota.bytes_used` repeatedly** (§5, §6.1.5, §6.1.4, Gherkin scenarios) but `tenant_quota` schema in `data_model.md §4.2` has no such column — only `max_storage_bytes`, `max_rps`, `max_concurrent_exec`, `updated_at`. The "D1 source-of-truth periodic sync" pattern in §6.1.5 is therefore writing to a non-existent column. ADR-0020 also dangles this hook (`tenant.bytes_used`) without a migration. No WI in S-07 introduces this column. Either S-07 must add a migration `ALTER TABLE tenant_quota ADD COLUMN bytes_used INTEGER NOT NULL DEFAULT 0`, OR adopt `usage_counter` (period-keyed, exists today) — either decision needs to be explicit.
3. **WI-S07-002 / WI-S07-005 reference `blob_meta.deleted_at_ms`** but `data_model.md §4.2` canonical column is `blob_meta.deleted_at` (without `_ms`). This is *the same column-name drift defect class* that R4 flagged as P0-2 in S-06 (which generated Lote 10.6bis fixes to align WI-S06-002/003 onto `_ms`-suffixed names) — but the canonical D1 DDL was *never updated to match*. So either S-06 is wrong, S-07 inherits the wrong canonical, or `data_model.md` needs a migration to add `_ms` suffix. Either way, the spec ships incoherent on column names: `sqlx::query!` will fail at compile time on whichever side mismatches the actual D1 schema after migration runs.

Plus three more P0-class defects:

4. **ADR-0020 quota threshold contradiction** — sprint contract §4 CAP-EVICT-003 says "80% soft-warn (email), 95% trigger eviction worker" but ADR-0020 says "soft-pressure quando `tenant.bytes_used > 0.8 × tenant.storage_limit`" (single threshold at 80%, not the gradient 80→95→100). WI-S07-002 / WI-S07-003 only implement the 95% trigger, ignoring the 80% soft-warn email entirely. The 80% email path is anti-scoped without justification.
5. **ADR-0019 / ADR-0020 status drift** — both ADRs are `doc_status: DRAFT` (not `FROZEN` as repeatedly claimed in WI-S07-002 §9.9, WI-S07-005 §1, WI-S07-005 §6.1.6, WI-S07-005 Gherkin scenarios). The "ADR ratificação cite-and-acknowledge" anti-rubber-stamp control in WI-S07-005 §9.5 is asking the Architect to acknowledge a DRAFT as if it were FROZEN — this is the *exact rubber-stamp pattern* the control claims to prevent.
6. **Eviction reachable check is NOT race-aware** — WI-S07-002 §6.1.6 reachable-check SQL is missing the `created_at_ms < gc_mark_started_at` strict-`<` predicate that S-06 WI-S06-003 INV-GC-004 enforces. Without it, the cascade-prevention claim in §1.1 ("eviction respects mark-phase-aware re-ref protection") is *not implemented*. The chaos test #1 ("GC race") is asserting an invariant the SQL does not enforce. This is the same defect class as S-06 P0-1, *not absorbed*.

Cross-WI consistency claims verified:
- TenantCtx-only enforcement: **PASS** — consistent across all 5 WIs.
- `json_each` canonical idiom: **PASS** in WI-S07-002 reachable check (correctly cited; correctly used).
- D1 batch ≤250: **PASS** — referenced consistently.
- `async_lock::Semaphore` (NOT `tokio::sync`) for CF Workers WASM: **NOT MENTIONED** in any S-07 WI. This is a S-06 lesson that MAY apply to WI-S07-003's DO singleton (if it uses any cross-await synchronization). Spec is silent — could be aspirational, could be missed entirely.
- Audit fail-closed: **PASS** in WI-S07-002 / WI-S07-003.
- Alarm re-arm AT START: **PASS** in WI-S07-002 / WI-S07-003 / WI-S07-004.
- ChunkPutReceipt-style enforcement for quota reservation: **PARTIAL** — reservation pattern is described but no signed receipt; race-free claim depends on DO actor model alone (which IS sufficient for FM-059 within a single region; but cross-region tenant scenarios are dismissed).
- ADR cite-and-acknowledge: **WEAKENED** because both ADRs are still DRAFT (see P0-5).
- SLO-DEDUP-RATIO definition: **MISSING** — `slo_catalog.md` has no entry for SLO-DEDUP-RATIO; WI-S07-005 §4 references it but does not define it; the sprint review ship-gate cannot validate against it. Forward-whitelisting only papers over the gap; the actual SLO definition (objective ≥ 2.5×, error budget, multi-burn-rate alert windows) is missing entirely.
- Custom clippy lint (manifest_chunks SELECT sem tenant_id): **ASPIRATIONAL** — the WI claims "CI lint: `clippy::disallowed_method` rule prohibiting `manifest_chunks` SELECT without `tenant_id` filter" but `clippy::disallowed_method` is a list-of-paths lint, not a SQL-content lint; it cannot inspect query strings. This would need a `cargo-spellcheck`-style or AST-grep based custom lint, OR a CI grep gate over `*.rs` files. The framing is wrong; either drop the claim or substitute a feasible mechanism.

If the dev applies all 9 P0s plus 4 P1s (Lote 10.7bis), projected score: **8.4 / 10** — back into the post-bis envelope but still short of the SOTA bar. Reaching ≥9.0 requires (a) closing the schema gaps coherently with proper ADR migrations and (b) defining SLO-DEDUP-RATIO formally with multi-burn-rate alert windows.

## 1. Per-WI Scores

| WI | Score | Strongest aspect | Headline weakness |
|---|---|---|---|
| **WI-S07-001** Dedup Index + FindMissingBlobs | **7.0 / 10** | REAPI v2 conformance discipline; CTRL-ISO-005 cross-tenant gate explicit; bench discipline | **P0-1 schema confusion `manifest_chunks` vs `chunks`** invalidates entire dedup design |
| **WI-S07-002** Eviction Worker LRU+TTL+Quota Trigger | **7.5 / 10** | `json_each` correctly cited; soft-delete-first inheritance well-framed; cascade prevention concept is right | **P0-3 column drift `deleted_at_ms`**, **P0-6 reachable check not race-aware**, **P0-7 tier resolution undefined** |
| **WI-S07-003** Quota Enforcement Middleware | **7.2 / 10** | DO actor model + reservation TTL pattern is sound; FM-059 elimination claim is defensible *modulo schema* | **P0-2 `tenant_quota.bytes_used` column does not exist**; cross-region tenant problem dismissed |
| **WI-S07-004** `last_accessed_at` Hot Path | **8.2 / 10** | DO batch coalescing is the correct write-amp mitigation; authoritative UNION lookup is the right pattern | INV name drift (INV-AC-TTL-MONOTONIC vs INV-DATA-MONOTONIC-TS); race-correctness is good but only 10k iter property test |
| **WI-S07-005** DASH-DEDUP + Alerts + Ship Gate | **7.8 / 10** | 10-panel dashboard scope is right; cumulative INV promotion mechanics in place | **P0-5 ADR FROZEN claim is false**; SLO-DEDUP-RATIO undefined; custom clippy lint aspirational |

**Aggregate (mean): 7.54 / 10.** Rounded headline: **7.6 / 10**.

## 2. P0 Findings (must-fix Lote 10.7bis)

### P0-1 — WI-S07-001 conflates `manifest_chunks` with `chunks`; UNIQUE INDEX would break existing schema [LOAD-BEARING SCHEMA]

**Severity**: P0 catastrophic-if-deployed. The migration as written is *destructively wrong*: it would either (a) fail at `CREATE UNIQUE INDEX` time because `manifest_chunks` already contains multiple rows with the same `(tenant_id, chunk_digest)` after the first multi-manifest tenant, OR (b) succeed only on a freshly-deployed staging with 1 manifest per chunk and *silently break dedup* once a second manifest tries to reference an existing chunk.

**Defect**: WI-S07-001 §6.1.2 specifies:

```sql
CREATE UNIQUE INDEX idx_manifest_chunks_tenant_digest
    ON manifest_chunks (tenant_id, chunk_digest);
```

This contradicts WI-S05-004 §6.1 (lines 104-129):

```sql
CREATE TABLE IF NOT EXISTS manifest_chunks (
  tenant_id           TEXT        NOT NULL,
  blob_digest         TEXT        NOT NULL,
  chunk_index         INTEGER     NOT NULL,
  chunk_digest        TEXT        NOT NULL,
  PRIMARY KEY (tenant_id, blob_digest, chunk_index),
  ...
);

-- Already exists (NON-UNIQUE):
CREATE INDEX IF NOT EXISTS idx_manifest_chunks_tenant_chunk
  ON manifest_chunks(tenant_id, chunk_digest);
```

`manifest_chunks` is a per-blob ordered list of chunk references. Tenant T's blob B1 has chunks [C1, C2, C3]; tenant T's blob B2 has chunks [C2, C4, C5]. In `manifest_chunks`, both `(T, B1, 1, C2)` and `(T, B2, 0, C2)` MUST coexist — that's literally how content-addressable dedup works. UNIQUE on `(tenant_id, chunk_digest)` rejects the second row. Dedup is broken.

The actual dedup table is `chunks`, also defined in WI-S05-004 §6.1 (lines 73-101):

```sql
CREATE TABLE IF NOT EXISTS chunks (
  tenant_id           TEXT        NOT NULL,
  chunk_digest        TEXT        NOT NULL,
  ...
  refcount            INTEGER     NOT NULL DEFAULT 1,
  ...
  PRIMARY KEY (tenant_id, chunk_digest),
  ...
);
```

`chunks` already has the correct `(tenant_id, chunk_digest)` PRIMARY KEY (= UNIQUE), AND a `refcount` column. WI-S07-001's `chunk_exists` and `find_missing` should query `chunks` not `manifest_chunks`.

**Why it matters**: This is the load-bearing claim of WI-S07-001 — "FindMissingBlobs returns absent-only chunks (skip-upload optimization); ≥ 50% bytes saved". If the underlying dedup table is wrong, every benchmark, every customer claim, every SOTA comparison vs NativeLink/BuildBuddy is invalid. AppSec review will catch this on first read; the spec ships incoherent.

**Fix recommendation**: Rewrite WI-S07-001 to use `chunks` table. The migration is a no-op (the index already exists at `chunks.PRIMARY KEY (tenant_id, chunk_digest)` since S-05). What WI-S07-001 actually needs to add is **only** `idx_manifest_chunks_tenant_chunk` if it's not already created in S-05 (it IS — line 128). So the migration is *zero new schema changes*; the WI is purely about (a) implementing `chunk_exists` against `chunks` table, (b) wiring the REAPI `FindMissingBlobs` handler, (c) custom CI lint and metrics. The spec needs deletion of the migration §6.1.2 entirely + rewrite of all `manifest_chunks` SELECT references in §6.1, §8 Gherkin, §17 sub-tasks, §28 risk register, etc. to query `chunks`. Estimated effort: ~6h rewrite.

**Crypto SME mandatory non-waivable**: schema review for whether `chunks.refcount > 0` is the right "blob exists for this tenant" predicate, OR whether dedup should also accept `chunks.refcount = 0 AND deleted_at_ms IS NULL` (chunk soft-deleted but still recoverable within grace; treating it as "missing" forces re-upload but customer expects skip-upload — Lote 10.6 grace period semantics).

### P0-2 — WI-S07-003 references `tenant_quota.bytes_used` column that does not exist [LOAD-BEARING SCHEMA]

**Severity**: P0 schema-incoherent. The DO ↔ D1 sync pattern in §6.1.5 attempts to write to a non-existent column.

**Defect**: WI-S07-003 §5, §6.1.3, §6.1.5 all reference `tenant_quota.bytes_used`:
- §5 "D1 source-of-truth periodic sync: DO state synced to D1 `tenant_quota.bytes_used` every 5min".
- §6.1.5 "Every 5min, DO writes `tenant_quota.bytes_used` to D1 source-of-truth".
- §6.1.5 "On DO cold start (worker restart): read `bytes_used` from D1".

But `data_model.md §4.2` (lines 266-272):

```sql
CREATE TABLE tenant_quota (
  tenant_id         TEXT        PRIMARY KEY,
  max_storage_bytes INTEGER     NOT NULL,
  max_rps           INTEGER     NOT NULL,
  max_concurrent_exec INTEGER   NOT NULL,
  updated_at        INTEGER     NOT NULL
);
```

No `bytes_used` column. The closest is `usage_counter (tenant_id, period, metric)` which is period-keyed (`'2026-04'`) — fundamentally a different structure (resets monthly), unsuitable as DO running counter source-of-truth.

**Why it matters**: cold-start recovery, D1 reconcile, S-08 hand-off (ADR-0020 says "S-08 quota check lê `tenant.bytes_used` que S-07 mantém — coupling, mas explicit" — but the column doesn't exist). The entire reservation pattern's durability story is built on a phantom column. `sqlx::query!` will fail at compile time on every reference.

**Fix recommendation**: Pick one of the two paths and commit:
- (A) Add migration in WI-S07-003 §6.1: `ALTER TABLE tenant_quota ADD COLUMN bytes_used INTEGER NOT NULL DEFAULT 0` + `bytes_used_updated_at INTEGER NOT NULL DEFAULT 0`. This requires `data_model.md §4.2` patch + WI-S07-003 schema migration + reconcile job to backfill from `blob_meta` aggregate `SUM(size_bytes)` per tenant.
- (B) Add a NEW table `tenant_storage_state (tenant_id PRIMARY KEY, bytes_used, last_synced_at_ms)` — semantically clearer (running counter, not quota config); tenant_quota stays config-only.

Recommend (B) — separates "policy" (max_storage_bytes is a config) from "state" (bytes_used is mutable telemetry). Add migration `00X_tenant_storage_state.sql`. Estimated effort: ~3h (migration + WI patches + data_model.md patch + reconcile backfill design).

### P0-3 — `blob_meta.deleted_at_ms` column does not exist (canonical is `deleted_at`) [LOAD-BEARING SCHEMA — CARRIED FORWARD FROM S-06 UNRESOLVED]

**Severity**: P0 column-name drift; will fail `sqlx::query!` compile time.

**Defect**: WI-S07-002 references `blob_meta.deleted_at_ms` in §1.1 (multiple times), §6.1.5 (LRU scan SQL `AND deleted_at_ms IS NULL`), §6.1.6 (reachable check `AND a.deleted_at_ms IS NULL`), §6.1.7 (soft-delete batch `UPDATE blob_meta SET deleted_at_ms = ?`), §8 Gherkin (3 scenarios), §12 INV-EVICT-SOFT-DELETE-FIRST registry entry. Same column name appears throughout invariant_registry.md §3.18.

But `data_model.md §4.2` line 237 canonical is:

```sql
deleted_at        INTEGER     NULL,       -- soft-delete grace
```

Note: same drift exists in S-06 WI-S06-003 §6.1 (line 257-260) and WI-S06-005. This was *acknowledged but not fixed* in Lote 10.6bis P0-2 — the lesson was applied to align WI-S06-002/003 onto `_ms`-suffixed names internally, but the canonical D1 DDL in `data_model.md` was *not* updated. So S-07 inherits the inconsistency: the column is `deleted_at` in the schema, `deleted_at_ms` in every WI's SQL.

**Why it matters**: at compile time, `sqlx::query!("UPDATE blob_meta SET deleted_at_ms = ? ...")` will fail (no such column). At runtime, even if `deleted_at` is renamed via migration, every prior INSERT/SELECT path (S-01 CAS PUT handler, S-04 AC handlers, S-06 GC) references the old name and breaks.

**Fix recommendation**: This needs to be resolved at the **data_model.md level** before S-07 ships. Either:
- (A) Add migration to RENAME `blob_meta.deleted_at` → `blob_meta.deleted_at_ms` (D1 SQLite supports `ALTER TABLE ... RENAME COLUMN ...` since SQLite 3.25); patch `data_model.md` canonical DDL; verify S-01/S-04 SEALED handlers are also patched (probably broken since they read/write `deleted_at`).
- (B) Patch all S-06 + S-07 WIs back to `deleted_at` (no `_ms` suffix); preserve ms semantics via column comment.

This is **NOT a S-07 problem alone** — it's a cross-cutting governance failure. Recommend option (A) executed via Lote 10.7bis with explicit ADR-0043 "datetime column naming convention: `_ms` suffix mandatory for unix milliseconds". Estimated effort: ~4h migration + 6h WI patches across S-04/S-06/S-07.

### P0-4 — ADR-0020 quota threshold contradicts sprint contract (80% soft-warn vs 95% trigger)

**Severity**: P0 design coherence — the entire quota gradient is unclear.

**Defect**: ADR-0020 §Decision says:

> **Trigger:** soft-pressure quando `tenant.bytes_used > 0.8 × tenant.storage_limit`.

Sprint contract S-07 §4 CAP-EVICT-003 says:

> **Storage soft-pressure eviction** — 80% soft-warn (email), 95% trigger eviction worker.

But WI-S07-002 §6.1.8 + WI-S07-003 §6.1.4 only implement the **95% trigger eviction**:

> When middleware detects `bytes_used / max_storage_bytes ≥ 0.95`, invokes `execute_quota_trigger(tenant_ctx, target_bytes_to_reclaim)`.

The 80% soft-warn email path is **not implemented**, **not stubbed**, **not anti-scoped with justification**. WI-S07-002 / WI-S07-003 / WI-S07-005 dashboards do not mention 80% soft-warn anywhere.

**Why it matters**: Customer-visible UX claim (sprint contract: "80% soft-warn (email)") shipped without the email infra. Compliance/Sales will reasonably claim "we email you at 80%" — false claim. Either implement the email path, or admit that ADR-0020 + sprint contract contradict + remove the 80% claim everywhere.

**Fix recommendation**: Pick one decision and propagate:
- (A) Implement 80% soft-warn email — adds work to WI-S07-003 (~3h) plus email template + S-13 admin notification surface (forward dependency); arguably belongs in S-13 (admin/notifications) not S-07.
- (B) Defer 80% email to S-13 explicitly + patch sprint contract §4 CAP-EVICT-003 to drop "(email)" claim + patch ADR-0020 to clarify "soft-pressure 80% = telemetry only (alert SEV-3 oncall); 95% = ad-hoc eviction trigger; 100% = S-08 hard-block". This is the cleaner option.

Estimated effort if (B): ~1h spec edits.

### P0-5 — ADR-0019 / ADR-0020 are doc_status DRAFT but repeatedly cited as FROZEN

**Severity**: P0 governance — the cite-and-acknowledge anti-rubber-stamp control rests on these being FROZEN.

**Defect**: WI-S07-002 §1.0 (sprint contract metadata: "ADR-0019 (TTL ownership; FROZEN)") + WI-S07-002 §9.9 + WI-S07-005 §1 + WI-S07-005 §6.1.6 + multiple Gherkin scenarios all assume ADR-0019 + ADR-0020 are at FROZEN status. But:

- `specs/03_architecture/adrs/ADR-0019-ttl-ownership-s04-s07.md` line 4: `doc_status: "DRAFT"`.
- `specs/03_architecture/adrs/ADR-0020-quota-ownership-s07-s08.md` line 4: `doc_status: "DRAFT"`.

Both ADRs were created 2026-04-24 (Lote 9.1) and have not been promoted past DRAFT. WI-S07-005 §6.1.6 Gherkin "ADR-0019 ratificação confirmation" is asking the Architect to acknowledge a DRAFT — which is precisely the rubber-stamp pattern the control claims to prevent.

**Why it matters**: The single ship-gate signal that distinguishes S-07 from a "trust me, the boundary is clear" hand-wave is the cite-and-acknowledge ceremony. If the artifact being cited is itself DRAFT, the ceremony is theatre.

**Fix recommendation**: Promote both ADRs to FROZEN as a Lote 10.7bis pre-step, OR rewrite WI-S07-005 §6.1.6 to require *promotion-then-cite* (Architect must FIRST raise PR to promote ADR-0019/0020 from DRAFT to FROZEN with their content audit + signature, THEN cite the FROZEN version in sprint review). The latter is more rigorous (forces Architect to actually review ADR content as part of promotion, not just retroactively acknowledge). Estimated effort: ~3h (Architect content audit per ADR + status flip + downstream WI patches removing the "FROZEN" claim where it precedes promotion).

### P0-6 — Eviction reachable check is NOT race-aware (missing `created_at_ms < gc_mark_started_at` predicate); INV-GC-001 inheritance claim is partially false

**Severity**: P0 cripto-load-bearing for the cascade-prevention claim.

**Defect**: WI-S07-002 §6.1.6 publishes the reachable-check SQL:

```sql
SELECT
    (SELECT COUNT(*) FROM manifest_chunks
     WHERE tenant_id = ? AND chunk_digest = ?) +
    (SELECT COUNT(*) FROM ac_meta a, json_each(a.blob_refs) j
     WHERE a.tenant_id = ? AND j.value = ? AND a.deleted_at_ms IS NULL)
    AS active_refcount;
```

This is an **instantaneous reachable check** — it counts current references. But WI-S07-002 §1 claim is "INV-GC-001 inheritance via S-06 GC mark/sweep + grace + reconcile" and §2 "eviction must respect mark-phase-aware re-ref protection (INV-GC-004 via S-06)". INV-GC-004 in WI-S06-003 §6.1 enforces strict-`<` semantics: a sweep operation can soft-delete blob B only if NO `ac_meta` entry references B with `created_at_ms >= mark_started_at_ms` (the legitimate-re-ref protection — if an UpdateActionResult fires *during* mark phase, it gets protected).

WI-S07-002's reachable check has **no analogous predicate**. It does not capture an `evict_started_at_ms`; it does not filter `ac_meta.created_at_ms >= evict_started_at_ms`. So the race-correctness story is:

- T0: customer GET fires on chunk C of blob B → DO buffer add (T0).
- T1 = T0 + 50ms: eviction worker fires; reachable check runs.
- T2 = T0 + 80ms: customer UpdateActionResult fires → INSERT into ac_meta with created_at_ms = T2; blob_refs now contains C.
- T3 = T0 + 90ms: reachable check completes; sees `active_refcount = 1` (T2 INSERT visible) → cascade prevented. **OK in this race direction.**

But the inverse race:

- T0: eviction starts; reads ac_meta — NO entry references C; active_refcount = 0.
- T1 = T0 + 50ms: customer UpdateActionResult fires; INSERT ac_meta references C; created_at_ms = T1.
- T2 = T0 + 100ms: eviction sees active_refcount = 0 (snapshot from T0); proceeds to soft-delete C.
- **INV-GC-001 violated**: chunk C is now soft-deleted, but ac_meta references it.

S-06 mitigates this via INV-GC-004 strict-`<`: sweep refuses to delete if `ac_meta.created_at_ms >= mark_started_at_ms` (legitimate-re-ref). WI-S07-002 inherits NONE of that.

**Why it matters**: The cascade-prevention claim is *the cripto-load-bearing claim* of WI-S07-002 — chaos test #1 ("GC race: 0 INV-GC-001 violations 30d sustained") is asserting it. The property test `prop_evict_gc_race` will eventually fire false-negative under sufficient load. This is the same defect class as S-06 P0-1 (LIKE vs json_each), arguably worse — S-06 had a wrong SQL idiom; S-07 has a *missing* SQL predicate.

**Fix recommendation**: Adopt the strict-`<` semantics. Eviction worker captures `evict_started_at_ms` at phase start; reachable check becomes:

```sql
SELECT
    (SELECT COUNT(*) FROM manifest_chunks
     WHERE tenant_id = ? AND chunk_digest = ?
       AND created_at_ms < ?) +              -- evict_started_at_ms; legitimate-re-ref via Split
    (SELECT COUNT(*) FROM ac_meta a, json_each(a.blob_refs) j
     WHERE a.tenant_id = ? AND j.value = ? AND a.deleted_at_ms IS NULL
       AND a.created_at_ms < ?)              -- evict_started_at_ms; legitimate-re-ref via UpdateAR
    AS active_refcount_at_phase_start;
```

This requires `manifest_chunks.created_at_ms` column (does it exist? Per WI-S05-004 §6.1, NO — `manifest_chunks` has only PK + `chunk_digest`; need to add). Estimated effort: ~10h (schema migration + WI rewrite + chaos test re-design + Crypto SME pair-review of strict-`<` semantics analogous to S-06).

**Crypto SME mandatory non-waivable**: same as S-06 INV-GC-004 review — derive strict-`<` direction from TLA+ obligation; verify boundary scenario (`a.created_at_ms == evict_started_at_ms`) goes the safe direction (eviction refused, NOT proceeded).

### P0-7 — Tier resolution undefined in eviction loop; tier vocabulary drift (3 vs 5 tiers)

**Severity**: P0 incoherent on the mechanism that drives `ttl_for_tier`.

**Defect**: WI-S07-002 §6.1.3 publishes:

```rust
pub fn ttl_for_tier(tier: &Tier) -> Duration {
    match tier {
        Tier::Free       => 7d,
        Tier::Solo       => 30d,
        Tier::Team       => 90d,
        Tier::Business   => 365d,
        Tier::Enterprise => 730d,
    }
}
```

But:
1. **Tier vocabulary drift**: `data_model.md §3.4` Plan table lists `free / team / enterprise` (3 tiers). ADR-0034 (PRR staffing waiver solo tier) also references `solo`. Spec contract S-07 §4 CAP-EVICT-002 lists 5 tiers. There's no canonical taxonomy registry; `Solo` and `Business` may or may not exist depending on which doc you read.
2. **Tier source-of-truth path**: `Plan` is in Neon (global/Postgres); eviction worker is in CF Worker reading D1 (regional/SQLite). How does the worker know `tenant T's tier` at scan time? WI-S07-002 §6.1.3 says "TTL per-tier resolution at runtime via tenant_quota row (forward S-13 admin override; default tier-mapped)" — but `tenant_quota` has no tier column either (see P0-2). The mechanism is undefined.
3. **Enterprise admin override (730d max)**: WI-S07-002 §6.1.4 says "enterprise customer override stored em config-singleton (S-13)" but S-13 is forward-deferred; meanwhile §10.s07.002.12 chaos test #7 asserts "Enterprise TTL > 730d override attempt rejected by config validator (730d hard cap)" — but no validator code path is shown (the validator is undefined).

**Why it matters**: TTL per-tier is the LOAD-BEARING customer claim of CAP-EVICT-002 ("free=7d → enterprise=730d max — pricing semantics"). If the eviction worker can't reliably resolve tier per tenant, every TTL decision is undefined. Property test `prop_evict_idempotent` won't catch this (it runs in a fixed-tier fixture); property test `prop_evict_tier_isolation` doesn't exist.

**Fix recommendation**: 
1. **Canonicalize tier taxonomy**: ADR (or amend ADR-0034) to enumerate `{free, solo, team, business, enterprise}` (5 tiers) OR `{free, team, enterprise}` (3) — pick one. Update `data_model.md §3.4 Plan` table values.
2. **Path for tier resolution in regional D1**: Either (a) replicate `tenant.tier` column into `tenant_quota` table per region (S-13 sync responsibility), OR (b) cache tier in DO singleton on first access (5-min TTL). Choose (a) — simpler, eliminates DO cold-start race; forward-couples to S-13 via a `tenant_quota.plan_id` column (extending P0-2 fix).
3. **Validator stub in S-07**: WI-S07-002 §6.1.X must publish the validator pseudo-code (e.g., `fn validate_ttl_override(tier: Tier, ttl_days: u32) -> Result<(), ConfigError> { if tier != Enterprise && ttl_days > tier_max(tier) { Err } else if tier == Enterprise && ttl_days > 730 { Err(TtlExceedsMax) } ... }`). Absent that, chaos test #7 cannot run.

Estimated effort: ~6h (ADR amendment + schema migration tying P0-2 + validator stub + WI patches).

### P0-8 — `blob_meta.refcount` based cascade prevention; no path for `chunks.refcount` (the actual dedup table)

**Severity**: P0 follow-on from P0-1 + P0-6.

**Defect**: WI-S07-002 §1.1.1 says "Cascade prevention: pre-evict, verify `blob_meta.refcount > 0` query: if reachable via active manifest_chunks OR active ac_meta.blob_refs". But §6.1.6 reachable-check SQL counts via `manifest_chunks` and `ac_meta` *without* consulting `blob_meta.refcount` directly. And `chunks.refcount` (the actual dedup table from S-05) is never mentioned.

The eviction loop iterates `blob_meta` (LRU scan §6.1.5), but the reachable check counts via `manifest_chunks` (which references `chunk_digest`, not `blob_digest`) and `ac_meta.blob_refs` (which contains blob digests). So the reachable check uses the wrong identifier:

```sql
-- WI-S07-002 §6.1.6 (paraphrased):
SELECT (SELECT COUNT(*) FROM manifest_chunks WHERE tenant_id = $1 AND chunk_digest = $2) ...
```

But the eviction loop §6.1.5 is iterating *blobs* via `blob_meta.digest`. The `$2` parameter is a blob digest, not a chunk digest. So `manifest_chunks WHERE chunk_digest = blob_digest` will return 0 essentially always (BLAKE3 hash collisions across different content are vanishingly rare; a blob's digest is the digest of its full content, a chunk's digest is the digest of its chunk content; they coincide only for blobs that fit in 1 chunk and are non-multipart).

**Why it matters**: This is *another* layer of the same schema confusion as P0-1. The eviction can target blobs OR chunks, but the reachable check needs to use the right reachability graph for each:
- Evicting a **blob** (`blob_meta` row): reachable iff `blob_digest` appears in any active `ac_meta.blob_refs` for the tenant.
- Evicting a **chunk** (`chunks` row): reachable iff `chunk_digest` appears in any active `manifest_chunks` for the tenant (and transitively any `cas_blobs` referencing it). 

WI-S07-002 conflates both into a single reachable check that does neither correctly.

**Fix recommendation**: Split the eviction worker into two scan paths:
- `evict_blob_candidate`: LRU scan `blob_meta` + reachable check against `ac_meta.blob_refs` only (chunks are handled by S-06 GC via `chunks.refcount`).
- `evict_chunk_candidate`: defer to S-06 `chunks.refcount = 0 + grace`. **Eviction does not touch chunks directly.**

Sprint contract §4 CAP-EVICT-002 only mentions "AC entry expiry" + "soft-pressure storage eviction"; it never says "eviction of orphan chunks". That's S-06 GC's domain. So the simplest fix is to remove the chunk-eviction path entirely from WI-S07-002 (only blobs are eviction candidates; chunk lifecycle stays with S-06 refcount + sweep). This is a significant scope-reduction; estimated effort: ~4h spec rewrite + simpler reachable check (only ac_meta side; manifest_chunks side becomes irrelevant since blobs aren't chunks at this granularity).

### P0-9 — DO singleton `quota-<tenant_id>` cross-region inconsistency dismissed without analysis

**Severity**: P0 if multi-region tenants exist GA-day; P1 if not.

**Defect**: WI-S07-003 §6.1.3 specifies "DO singleton per tenant `quota-<tenant_id>`" with state `bytes_used`. CF Durable Objects are single-region by design — a DO is pinned to a Cloudflare data center. If tenant T has data in 2 regions (sam + iad), and writes hit both regions simultaneously, the DO ID `quota-<tenant_id>` resolves to ONE physical DO instance — fine for serialization, BUT the request paths from sam and iad to that DO add cross-region latency (if DO is in iad and write is in sam, ~50-100ms RTT just for the quota check, blowing the §10.s07.3 ≤ 3ms p99 budget by 50×).

WI-S07-003 §6.2 anti-scopes this:

> Quota across multi-region tenant (M tenants > 1 region) — S-14 forward.

But sprint contract S-07 §15 R-S07-005 lists **5 regions** and `corelink.evict.cron_fired_total{region}` metric is per-region. If the eviction trigger fires in sam (regional D1) and quota DO is pinned to iad, the trigger->DO->reservation->commit round-trip is cross-region — and the 3ms p99 SLA cannot be met.

**Why it matters**: Anti-scoping multi-region tenants is fine if **GA day is single-region** (which it should be — bench shows S-14 region expansion is post-GA). But the sprint contract metrics already enumerate 5 regions, and the §6.1 region jitter ±10min implies multi-region. So the spec is either:
- (A) Truly single-region GA — patch sprint contract to remove region-suffixed metrics; clarify "regions are deployment topology, not tenant-residency".
- (B) Multi-region tenants exist GA-day — must address DO ID per-region OR per-(tenant,region) sharding; raise SLA p99 ceiling above 3ms; redesign reservation pattern for cross-region.

**Fix recommendation**: Pick (A) for simplicity at GA — all 5 regions deploy code, but each tenant is pinned to ONE primary region (per `Tenant.primary_region` already in `data_model.md` line 152). Quota DO ID is `quota-<tenant_id>` resolved in tenant's primary region. WI-S07-003 §6.1.3 needs explicit "DO ID location: derived from tenant.primary_region; routed via Worker→DO binding per region; cross-region requests routed via primary_region edge". Estimated effort: ~3h spec patch + integration test for primary_region routing.

## 3. P1 / P2 / P3 Findings

### P1 Findings (must-fix Lote 10.7bis OR document deferral)

**P1-1 — SLO-DEDUP-RATIO not defined in slo_catalog.md**

WI-S07-005 §4 references `SLO-DEDUP-RATIO` but `slo_catalog.md` has no such SLO entry (verified via grep). The sprint contract §6 DoD says "≥ 2.5× sustained 7d" but this is not formalized as an SLO with multi-burn-rate alert windows (per `Google SRE Workbook Ch. 5` cited in §17). Forward-whitelist via `validate_inv_promotion.py` is for INVs, not SLOs — there's no equivalent gate for SLO definitions. Fix: add `SLO-DEDUP-RATIO` definition to `slo_catalog.md` per existing pattern (objective ≥ 2.5×, error budget = 5% of 30d window, multi-burn 1h/6h windows, owner WI-S07-001+005). Effort: ~2h.

**P1-2 — Custom clippy lint claim is technically infeasible**

WI-S07-001 §6.1.6 + §10.s07.001.9 + §28 R-008 claim "CI lint: `clippy::disallowed_method` rule prohibiting `manifest_chunks` SELECT without `tenant_id` filter". `clippy::disallowed_method` is path-based (e.g., disallow `std::process::exit`), not content-based; it cannot inspect SQL string contents. Feasible alternatives: (a) `cargo-spellcheck` regex rule, (b) AST-grep custom rule on `sqlx::query!` macro arguments, (c) CI grep gate `! grep -E 'manifest_chunks(?!.*tenant_id)' src/**/*.rs`. Pick one and rewrite. Estimated effort: ~3h.

**P1-3 — Property test 10k iter is below SOTA bar for race-correctness claims**

WI-S07-002 chaos #1 (GC race; 30d sustained 0 INV-GC-001 violations) + WI-S07-003 chaos #1 (FM-059 race; 1000 concurrent at 99.9%) + WI-S07-004 chaos #1 (eviction race; 0 INV-LRU-CONSISTENCY violations) all rely on property tests at 10k iter PR + 100k nightly. S-06 P0-3 review (Crypto SME EMPHATIC) elevated INV-GC-004 to 100k race iterations (Lote 10.6bis). S-07 inherits the cripto-adjacency via INV-GC-001 inheritance, so the same standard should apply. 10k iter is ~1 / 10^5 sensitivity; race conditions at quota boundary may have 10^-6 frequency at 1k QPS, requiring 10^7 iter for credible 95% CI confidence. Recommend elevating WI-S07-002 chaos #1 + WI-S07-003 chaos #1 to 100k PR + 1M nightly. Effort: ~2h compute + property test runner config.

**P1-4 — Reservation TTL 60s vs write timeout coordination undefined**

WI-S07-003 §1.1.2 + §6.1.4 specify reservation TTL 60s. CF Workers max execution time is 30s for HTTP requests, but R2 multipart uploads can exceed that (S-05 introduces multipart for blobs > 50 MiB). If a multipart write of 1 GiB at 100 Mbps takes ~80s, the reservation auto-releases at 60s mid-flight; concurrent reservation can fit at boundary; under-counting risk (the WI itself notes this in §1.1.2 narrative but does not propose mitigation). 

Mitigation options: (a) reservation TTL coupled to multipart session TTL (S-05 multipart_sessions.last_activity_at + sweeper window); (b) re-extend reservation on each UploadPart RPC via `extend_reservation(id, +60s)` heartbeat; (c) raise reservation TTL to multipart session max (24h? 7d?). Recommend (b) — clean semantic, bounded operationally, low cost. Effort: ~4h spec + integration test for multipart write race vs reservation expiry.

**P1-5 — DO storage limit (32 MiB per DO) vs reservation pattern + LRU buffer co-location**

WI-S07-003 DO durable storage holds `pending_reservations: HashMap<ReservationId, ReservationEntry>` (state per pending write); WI-S07-004 DO durable storage holds `buffer: BTreeMap<(TenantId, Digest), u64>` (LRU buffer ≤1000 entries). FM-059 explicitly says "DO storage quota exceeded (32 MiB por DO)". Quantify: each pending reservation is ~80 bytes (UUID + 2 u64); 32 MiB / 80 bytes = ~400k pending reservations max — fine for typical tenant. Each LRU buffer entry is ~70 bytes (TenantId 16 + Digest 32 + u64 8 + overhead); 1000 cap = 70 KiB, trivial. **HOWEVER**: WI-S07-003 + WI-S07-004 are DIFFERENT DOs (`quota-<tenant_id>` vs `lru-tracker-<region>`), so storage doesn't co-locate. Fine. But the spec doesn't quantify this — fix: add explicit "DO storage budget < 1 MiB sustained, < 5 MiB worst-case under D1 outage" check in §22 cost analysis of each WI. Effort: ~1h.

**P1-6 — Sprint review STANDARD lane lacks explicit lessons-from-S-06-PRR retrospective**

S-06 ran a 13-sign-off HIGH_RISK PRR with substantial procedural learning. WI-S07-005 §15 chaos lists "ADR ratificação rubber-stamp → CI gate fails" as #5 chaos but no other procedural lessons from S-06 PRR feed forward. STANDARD lane sprint review should explicitly inherit S-06 lessons (e.g., "sign-off booking calendar (Lote 10.6bis lesson; STANDARD lighter)" is mentioned in §28 R-007 but not actually scheduled/executed). Effort: ~1h sprint review checklist patch.

**P1-7 — Eviction race property test `prop_evict_gc_race` does not encode INV-GC-004 strict-`<`**

Tied to P0-6 above. The property test as described in WI-S07-002 §6.1.11 just "simulates S-06 mark phase concurrent with eviction; 0 INV-GC-001 violations" — but doesn't specify the race interleaving (when does mark_started_at_ms snapshot? when does UpdateAR fire? what's the boundary case?). Crypto SME R5 review S-06 P0-3 specified `SHRINK_BOUND_NS = 1_000_000` (1ms tight interleaving) for INV-GC-004 — same rigor needed here. Recommend literal copy-paste of S-06 race property test scaffolding into S-07 with substitution `mark_started_at_ms → evict_started_at_ms`. Effort: ~3h spec rewrite + property test code.

### P2 Findings (defer to Lote 10.7bis OR Lote 10.8 polish)

**P2-1 — Eviction "if eviction CAN'T reclaim enough" failure mode undefined**

User probe: "95% threshold ad-hoc trigger — what if eviction CAN'T reclaim enough (cascade prevention refuses ALL candidates)? Does write proceed at 96%? Does quota check return Exceeded?" WI-S07-002 §6.1.8 budget says "Synchronous trigger blocks middleware up to phase budget 500ms p99; longer offloaded to next daily cron". WI-S07-003 §6.1.4 says "Target: reclaim until 90% (= reduce by ~5% of max). Trigger latency budget 500ms p99; if exceeds, write proceeds (eviction continues async)." So the answer is: write proceeds at 96% if eviction misses budget. **But** what if reclaim returns 0 bytes (all candidates cascade-prevented)? The reservation pattern's `would_use ≤ max` check still fires (write proceeds because 96% < 100%). The customer never knows 96% headroom is illusory. Fix: emit `corelink.evict.reclaim_target_missed{tenant_id}` SEV-2 alert when reclaim < target × 0.5. Effort: ~1h metric + alert.

**P2-2 — Eviction race vs S-06 reconcile (S-06 daily 03:00 UTC + jitter)**

Sprint contract S-06 §13 timeline says reconcile fires daily 03:00 UTC; WI-S07-002 says eviction fires daily 02:00 UTC. With ±10min jitter on both, the windows overlap (02:00-02:10 + 02:50-03:10). Eviction soft-deletes at 02:05; reconcile at 03:00 detects refcount drift (eviction soft-delete decremented refcount; ac_meta.blob_refs not updated until customer DeleteActionResult). Reconcile auto-fix triggers; could mask eviction's intent. Property test scenario: `prop_evict_reconcile_race` not in WI-S07-002 §6.1.11. Recommend add. Effort: ~2h.

**P2-3 — DASH-DEDUP cardinality bomb (per-tenant heatmap)**

WI-S07-005 §6.1.4 panel "Quota Utilization Heatmap (per Tenant)" — at 1k tenants × 10 regions × 1 tier dim = 10k series; Grafana datasource limits typically 5k series per panel. WI mentions "top-50 tenants only em heatmap" in §27 anti-pattern but the panel YAML §1 doesn't filter. Fix: add `topk(50, ...)` Prometheus query wrapper. Effort: ~30min.

**P2-4 — SOTA bench representativeness (Docker pulls, ML training, generic Bazel)**

User probe: "the 3 workloads listed (Docker pulls, ML training, generic Bazel) — workload representativeness for 'Docker target ≥ 3×' claim?" Sprint contract §16 cites NativeLink baseline ~2.1× from "Bazel builds typical" — different workload than CoreLink's claimed 3× Docker. The bench report scope (3 workloads) is reasonable but the comparison against NativeLink should be apples-to-apples (Bazel-Bazel) — citing "CoreLink Docker 3× vs NativeLink Bazel 2.1×" is misleading. Fix: bench report must include (a) Docker workload run on BOTH CoreLink and NativeLink (apples-to-apples); (b) Bazel workload run on both. Effort: ~3h additional bench infrastructure (NativeLink staging deploy is non-trivial).

**P2-5 — `enterprise=730d max` hard cap; prior data_model.md ttl_days field?**

ADR-0019 promises customer-configurable for enterprise (default 365d, max 730d) but `tenant_quota` schema has no `ttl_override_days` column. Either add via S-13 (admin override → DO config-singleton) OR at S-07 schema level. WI-S07-002 §6.1.4 attempts both (says config in S-13 but validator runs in S-07). Fix: explicit deferred-to-S-13 for the override path; default tier-mapped at S-07 hard-coded; clarify in spec. Effort: ~1h.

**P2-6 — `bytes_reclaimed_last_30d` rollup math**

WI-S07-005 §6.1.3 says `corelink.cache.bytes_reclaimed_last_30d{tenant_id, tier}` is "aggregate of corelink_evict_bytes_reclaimed_total over 30d window" — but Prometheus counters can reset on Worker redeploy; `rate()`-then-`sum_over_time` is the correct idiom, not direct `sum()`. Fix: PromQL idiom in panel YAML. Effort: ~30min.

### P3 Findings (style / polish)

**P3-1 — WI titles are excessively long (WI-S07-002 title is 8 lines)**: makes git log + GitHub PR navigation painful. Convention says title ≤ 1 sentence; the qualifying details belong in §0 metadata table. Effort: ~30min trim across 5 WIs.

**P3-2 — `Lote 10.7bis fix budget` not estimated upfront**: prior bis cycles published a P0/P1/P2 fix budget table at WI authoring time so triage was cheap; this WI batch lacks it. Effort: ~1h add a "Lote 10.7bis projection" section to WI-S07-005 §1.

**P3-3 — Audit emit kind enumeration**: `corelink.evict.{executed, cascade_prevented, ...}` enumerated but not added to a central event catalog (`observability_model.md` §X). Effort: ~1h.

**P3-4 — Tier vocabulary in CHECK constraint**: `CHECK (tier IN ('free', 'solo', 'team', 'business', 'enterprise'))` not added to `tenant_quota` schema (would be P0-7 fix's correct landing).

**P3-5 — `last_accessed_at_ms` vs `last_accessed_at` drift**: same as P0-3 but for the LRU column; canonical `data_model.md §4.2` is `last_accessed_at INTEGER NOT NULL` (no `_ms` suffix), unit unspecified. WI-S07-004 SQL uses `last_accessed_at` (correct), but WI-S07-002 §6.1.5 uses `last_accessed_at` (correct) — actually consistent. False alarm; PASS. (Removing this as a P3.)

## 4. Cross-WI Consistency Check

| Lesson / pattern | WI-S07-001 | WI-S07-002 | WI-S07-003 | WI-S07-004 | WI-S07-005 | Verdict |
|---|---|---|---|---|---|---|
| TenantCtx-only enforcement (Lote 10.4bis) | ✅ §1.1.4, §7 | ✅ §1.1.2, §7 | ✅ §1.1.3, §7 | ✅ §6.1.8, §7 | n/a (dashboard) | **PASS** |
| `json_each` canonical idiom NOT `LIKE` (Lote 10.6bis P0-1) | n/a (queries `chunks` PK; should query single-row, no JSON column) | ✅ §6.1.6, §1.1.1.4 (cited correctly) | n/a (no JSON column) | n/a | n/a | **PASS** but P0-6 missing strict-`<` |
| D1 batch ≤250 (Lote 10.5bis) | ✅ §1.1.3, §6.1.4 | ✅ §6.1.5 (LIMIT 250); §6.1.7 batch | n/a (DO state, no D1 batch) | ✅ §6.1.6 chunked | n/a | **PASS** |
| `async_lock::Semaphore` (NOT `tokio::sync`) WASM-compat | n/a | n/a | NOT MENTIONED — DO has internal serialization but if any code uses Semaphore needs attention | n/a (DO serial) | n/a | **GAP** — silent on WASM concurrency |
| Audit fail-closed (Lote 10.6bis) | ✅ §1.1.3 (cross-tenant audit); not deep | ✅ §1.1.4, §6.1.7 BEGIN/COMMIT | ✅ §1.1.4, §6.1.9 | n/a (best-effort LRU) | n/a | **PASS** |
| Alarm re-arm AT START (Lote 10.4bis) | n/a (no DO) | ✅ §6.1.2 | ✅ §6.1.3 | ✅ §6.1.3 | n/a | **PASS** |
| ChunkPutReceipt-style enforcement / signed receipts | n/a | n/a | PARTIAL — reservation pattern uses ReservationId UUID not signed receipt | n/a | n/a | **PARTIAL** — UUID is sufficient if DO state is durable; no cripto-grade requirement here |
| ADR cite-and-acknowledge (anti-rubber-stamp) | n/a | references ADR-0019/0020 | references ADR-0020 | n/a | ✅ §9.5 control + §6.1.6 Gherkin | **WEAKENED by P0-5** (ADRs DRAFT) |
| Crypto SME EMPHATIC mandatory (race-correctness invariants) | n/a (no race) | NOT mentioned despite INV-GC-001 inheritance + INV-GC-004 strict-< claim — should be EMPHATIC for cascade-prevention | NOT mentioned despite FM-059 race claim — should be ADVISORY at minimum | NOT mentioned despite eviction race correctness | n/a | **GAP** — S-06 standard not propagated |
| INV promotion via validate_inv_promotion.py | n/a (INV-DEDUP-CONSISTENCY pre-existing) | declares 3 NEW (SOFT-DELETE-FIRST, CASCADE-PREVENTED, TTL-CAP-RESPECTED); registry §3.18 has them | declares INV-QUOTA-RESERVATION-TTL; registry has it | declares INV-LRU-CONSISTENCY; registry has it | sprint review confirms cumulative | **PASS** — INVs registered preemptively |

## 5. Verdict per WI

- **WI-S07-001** (7.0/10): **GO-WITH-FIXES** mandatory. P0-1 (schema confusion) is a 1-line spec change that unblocks the entire WI; without it, the WI is incoherent. After P0-1 + P1-2 (clippy lint): projected 8.3.
- **WI-S07-002** (7.5/10): **GO-WITH-FIXES** mandatory. P0-3 (column drift), P0-6 (race-aware reachable check), P0-7 (tier resolution), P0-8 (chunk vs blob conflation). Most-load-bearing WI in S-07 — Crypto SME EMPHATIC needed for P0-6 race-correctness. After all P0s applied: projected 8.6.
- **WI-S07-003** (7.2/10): **GO-WITH-FIXES** mandatory. P0-2 (`bytes_used` column) blocks compile; P0-9 (cross-region) is design coherence. Reservation pattern soundness post-fix is solid. After P0-2 + P0-9 + P1-4 (reservation TTL vs multipart): projected 8.4.
- **WI-S07-004** (8.2/10): **GO-WITH-MINOR-FIXES**. Strongest WI in S-07 — DO batch coalescing is the right answer to write-amp; authoritative UNION lookup mechanics are race-correct. Minor: INV name drift (INV-AC-TTL-MONOTONIC → INV-DATA-MONOTONIC-TS), 10k iter property test below SOTA (P1-3), Crypto SME advisory missing. After fixes: projected 8.7.
- **WI-S07-005** (7.8/10): **GO-WITH-FIXES** mandatory. P0-5 (ADR DRAFT) breaks the cite-and-acknowledge ceremony's premise; P1-1 (SLO-DEDUP-RATIO undefined). After fixes: projected 8.5.

**Aggregate post-Lote 10.7bis projection: 8.5 / 10.** Still below the 9-10 SOTA bar; requires (a) P0-7 tier taxonomy ADR + schema migration (cross-cutting governance), (b) P0-3 datetime column convention ADR, (c) P0-2 + P0-7 schema migrations executed coherently, (d) Crypto SME EMPHATIC ratification of WI-S07-002 P0-6 fix, (e) S-06 strict-`<` race property test scaffolding ported. Estimated Lote 10.7bis effort: **~58h** (within 1-week budget for one engineer + Crypto SME pair-review of P0-6, comparable to Lote 10.6bis ~68h budget).

## 6. Lote 10.7bis fix plan

Priority queue (in execution order; P0s first; P0-3 + P0-7 are cross-cutting and can land in parallel):

| # | Fix ID | WI(s) affected | Severity | Est. effort | Reviewer |
|---|---|---|---|---|---|
| 1 | **P0-1** Rewrite WI-S07-001 to use `chunks` table not `manifest_chunks` UNIQUE | WI-S07-001 | P0 cripto-load-bearing | ~6h | Engineer + Crypto SME |
| 2 | **P0-2** Add `bytes_used` column via new `tenant_storage_state` table + migration | WI-S07-003, data_model.md | P0 schema | ~3h | Engineer + Architect |
| 3 | **P0-3** ADR-0043 datetime column convention `_ms` suffix + RENAME COLUMN migration | data_model.md, S-04/S-06/S-07 WIs | P0 cross-cutting | ~10h (+ ADR ~2h) | Architect + Engineer |
| 4 | **P0-4** Pick (B): defer 80% soft-warn email to S-13; patch sprint contract + ADR-0020 | sprint contract S-07, ADR-0020 | P0 design | ~1h | Architect |
| 5 | **P0-5** Promote ADR-0019 + ADR-0020 from DRAFT → FROZEN with Architect content audit + signature | ADR-0019, ADR-0020 | P0 governance | ~3h | Architect |
| 6 | **P0-6** Add strict-`<` race-aware predicate to reachable check + `evict_started_at_ms` capture + add `manifest_chunks.created_at_ms` if S-05 schema lacks it | WI-S07-002, WI-S05-004 patch | P0 cripto-load-bearing | ~10h (+ Crypto SME pair) | Engineer + Crypto SME EMPHATIC |
| 7 | **P0-7** Canonicalize tier taxonomy (5 tiers); add `tenant_quota.plan_id` column or replicate from Neon; publish validator stub | data_model.md, WI-S07-002, ADR amendment | P0 design | ~6h | Architect + Engineer |
| 8 | **P0-8** Scope-reduce WI-S07-002 to blob-only eviction; chunk lifecycle stays in S-06 | WI-S07-002 | P0 design | ~4h | Engineer |
| 9 | **P0-9** DO ID per tenant resolved via `tenant.primary_region`; document cross-region routing | WI-S07-003 | P0 design | ~3h | Architect |
| 10 | **P1-1** Define SLO-DEDUP-RATIO in slo_catalog.md (objective + error budget + multi-burn windows) | slo_catalog.md | P1 | ~2h | SRE |
| 11 | **P1-2** Replace clippy lint claim with feasible mechanism (cargo-spellcheck OR CI grep) | WI-S07-001 | P1 | ~3h | Engineer |
| 12 | **P1-3** Elevate race property tests to 100k PR + 1M nightly | WI-S07-002, WI-S07-003, WI-S07-004 | P1 | ~2h spec + ~2h compute | QA + SRE |
| 13 | **P1-4** Reservation TTL extend-on-heartbeat for multipart writes | WI-S07-003 | P1 | ~4h | Engineer |
| 14 | **P1-7** Port S-06 race property test scaffolding to S-07 (`prop_evict_gc_race` analogue to `prop_sweep_inv_gc_004_race`) | WI-S07-002 | P1 | ~3h | Engineer + Crypto SME advisory |

**Total Lote 10.7bis P0+P1 fix budget: ~58h** (within 1-week budget for one engineer + 6h Crypto SME pair-review of P0-6 + 6h Architect content audit of ADR-0019/0020 promotion + ~4h SRE for SLO definition).

**Highest-leverage single fix: P0-1** (1-line WI rewrite from `manifest_chunks` to `chunks`). Without P0-1, WI-S07-001 is dead-on-arrival; with P0-1, the rest of the WI is coherent and the SOTA bench claim has a defensible foundation. Score impact: WI-S07-001 7.0 → 8.3 (+1.3).

**Crypto SME mandatory non-waivable**:
- **P0-6**: strict-`<` derivation analogous to S-06 INV-GC-004 review; race interleaving boundary case (`a.created_at_ms == evict_started_at_ms` goes the safe direction); 100k race property test traceability to TLA+ obligation.
- **P0-1** (advisory): chunk dedup semantic when `chunks.refcount = 0 AND deleted_at IS NULL` (grace window); should `chunk_exists` return true (skip-upload) or false (force re-upload)?

**Architect mandatory non-waivable**:
- **P0-3**: datetime column convention ADR-0043 + RENAME COLUMN migration impact analysis across S-01/S-04/S-06/S-07.
- **P0-5**: ADR-0019 + ADR-0020 content audit + DRAFT → FROZEN promotion with cite-traceable PR.
- **P0-7**: tier taxonomy canonicalization (5 tiers) + ADR amendment of ADR-0034 (or new ADR-0044).

**Path to 9.0+ for Lote 10.7tris**: close P0-1 + P0-2 + P0-3 + P0-6 (the cripto-load-bearing + schema-coherence quartet); promote ADR-0019/0020 to FROZEN; canonicalize tier taxonomy; elevate race property tests to 100k+; add SLO-DEDUP-RATIO definition. Estimated post-fix score: WI-S07-002 → 9.0, WI-S07-001 → 8.5, WI-S07-003 → 8.7, WI-S07-004 → 8.9, WI-S07-005 → 8.7. **Aggregate projected: 8.76**, with stretch to 9.0+ if Lote 10.7tris executes additional polish (cardinality-bounded heatmap, SOTA bench apples-to-apples vs NativeLink, eviction reclaim_target_missed alert).

---

**Closing note (adversarial calibration)**: the dev's claim of "Sonnet R5 lessons preemptively absorbed" is a *necessary but not sufficient* condition. Pattern absorption (TenantCtx-only, json_each, D1 batch ≤250, alarm re-arm, audit fail-closed) is verified — and this *is* a meaningful improvement over S-04/S-05's pre-bis output. But pattern absorption does not substitute for **first-principles schema reading** (P0-1, P0-2, P0-3) or **first-principles race analysis** (P0-6). The SOTA bar is not "absorbed prior lessons"; it is "no first-principles errors". S-07 fails the latter test on multiple fronts. The user's directive "Average não serve. SOTA puro 9-10" is not yet satisfied; Lote 10.7bis is mandatory.

**Verdict aggregate**: 7.6 / 10. **GO-WITH-FIXES** for all 5 WIs; 9 P0s + 7 P1s; estimated post-bis score 8.5 / 10 (with stretch to 8.76 if all P0s + key P1s land cleanly). Crypto SME EMPHATIC required for P0-6 race-correctness. Architect mandatory for P0-3 + P0-5 + P0-7 cross-cutting governance.
