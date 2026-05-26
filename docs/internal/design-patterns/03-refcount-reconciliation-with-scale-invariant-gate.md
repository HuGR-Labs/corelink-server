# Design Pattern 03 — Refcount Reconciliation with Scale-Invariant Auto-Fix Gate, Snapshot-Bounded Race Window, and Audit-Before-Mutation

**Location:** `crates/corelink-gc/src/reconcile.rs` + `crates/corelink-gc/src/reconcile/{plan,execute,verify}.rs`
**Work item:** WI-S06-005, WI-S06-007
**Invariants protected:** `INV-GC-RECONCILE-AUTO-FIX-BOUNDED`, audit append-only contract
**Sprint contract:** §5.5 R-S06-10

---

## 1. Problem

Distributed garbage collection on a content-addressed store with denormalised reference counters suffers from drift between **truth** (the canonical reference set: which `ac_meta` rows still cite each `blob_digest` in `blob_refs`) and **state** (the denormalised `blob_meta.refcount` counter used for the sweep decision).

Drift sources at scale:

| Source | Cause |
|---|---|
| Crashed in-flight writes | Counter incremented without `ac_meta` insert committing, or vice-versa |
| Replication delay | Cross-region replication races re-orders increment/decrement events |
| Manual recovery surgery | Operator UPDATEs during incident response |
| Soft-delete races | `ac_meta.deleted_at_ms` set without atomic refcount decrement |
| Cosmic rays | Single-bit flips on rare-but-real D1 page writes |

Three classes of catastrophe arise from **unchecked auto-fix** of detected drift:

1. **Mass false delete.** A buggy reconcile that treats `expected=0, stored=1` as drift and auto-fixes can mass-delete live blobs.
2. **Mass false retain.** Inverse: keeping blobs that should be GC'd inflates storage cost linearly.
3. **Silent mutation.** A reconcile that mutates without auditing leaves no forensic trail when something goes wrong.

A naive "drift > X → fix" gate is insufficient. The required design must be **scale-invariant** (a 100-tenant SaaS and a 1M-tenant SaaS need the same gate to behave correctly) and **fail-closed** under audit-emission failure.

## 2. Solution architecture

The reconcile phase sits at the tail of the GC phase chain:

```
Idle → Mark → Sweep → PhysicalDelete → Reconcile → Completed
                                       ^^^^^^^^^
                                       this pattern
```

For each candidate `(tenant_id, digest)` it executes:

```
1. snapshot_at_ms ← reconcile_started_at_ms (FIXED at phase entry)
2. expected ← RefcountSource::expected_refcount(tenant_id, digest, snapshot_at_ms)
   -- canonical SQL via json_each, NOT substring LIKE
3. stored   ← blob_meta.refcount(tenant_id, digest)
4. drift    ← stored - expected
5. IF drift == 0:
     emit AuditEvent::RefcountReconciled
6. ELSE IF auto_fix_gate_fires(tenant_drift):
     emit AuditEvent::RefcountAutoFixed   -- audit FIRST
     IF emit fails: abort row (no mutation)
     UPDATE blob_meta.refcount = expected -- mutation SECOND
7. ELSE:
     emit AuditEvent::RefcountManualReviewRequired (SEV-1)
8. IF stored > 0 AND r2.head(digest).is_not_found():
     emit AuditEvent::OrphanR2Detected
     -- DO NOT auto-fix; mutation would amplify the problem
```

Four innovations make this SOTA.

## 3. Innovation 1 — Scale-invariant dual-condition auto-fix gate

```rust
const AUTO_FIX_MAX_RECORDS: u64 = 5;
const AUTO_FIX_MAX_PERCENT: f64 = 0.0001;   // 0.01%

pub fn auto_fix_gate_fires(drift: TenantDrift, cfg: ReconcileConfig) -> bool {
    drift.count <= cfg.auto_fix_max_records
        AND
    drift.percent <= cfg.auto_fix_max_percent
}
```

The gate uses **both an absolute count AND a percentage**, joined by `AND`. Why this is non-obvious:

| Tenant size | Absolute alone (`≤ 5`) | Percentage alone (`≤ 0.01%`) | Both (`AND`) |
|---|---|---|---|
| 10 blobs | Allows fix of 50% drift — **dangerous** | Allows fix of 0 blobs (0.01% × 10 < 1) — locked out | Safe: count gate dominates |
| 1 000 blobs | Allows fix of 0.5% drift — moderate | Allows fix of 0 blobs (0.01% × 1k < 1) — locked out | Safe |
| 100 000 blobs | Allows fix of 0.005% — **operationally useless**: 6 drift records out of 100k blocks auto-fix unnecessarily | Allows fix of 10 blobs — too permissive given absolute number | Safe: both gates allow |
| 10 000 000 blobs | Useless at this scale | Allows fix of 1 000 blobs — **catastrophic if wrong** | Safe: count gate clamps to 5 |

The dual gate is the **only** design that is correct at all four scales. Absolute alone fails for small tenants (mass-fix); percentage alone fails for huge tenants (mass-fix). The conjunction makes the gate operate **as a safety floor** at every scale.

Boundary semantics: `count == 5` auto-fixes (operative bound `≤5`); `count == 6` rejects → manual review. Test fixtures pin both arms.

## 4. Innovation 2 — Snapshot-bounded race window

```rust
let snapshot_at_ms = phase_started_at_ms;   // captured ONCE at phase entry

// Every RefcountSource probe uses the SAME snapshot:
RefcountSource::expected_refcount(tenant_id, digest, snapshot_at_ms)
```

The canonical SQL:

```sql
SELECT COUNT(*)
  FROM ac_meta a, json_each(a.blob_refs) j
 WHERE a.tenant_id = ?
   AND j.value = ?
   AND a.deleted_at_ms IS NULL
   AND a.created_at_ms < ?    -- snapshot_at_ms
```

Why this matters: without the `created_at_ms < snapshot_at_ms` clause, **writes that land mid-reconcile race the probe**. A row written at `T+30s` (during reconcile) might:
- Be visible to row #N's probe (`expected=2`) but
- Not visible to row #N+1's probe of the same digest (`expected=1`)
- → false drift detected, false auto-fix, **counter corrupted**.

By pinning `snapshot_at_ms` at phase entry, every probe sees a **consistent snapshot** of `ac_meta`. Writes that land mid-reconcile are explicitly excluded — they will be reconciled in the next daily tick.

This is the same pattern as MVCC snapshot isolation, applied via a single `WHERE created_at_ms < $snapshot` clause rather than database-level transaction isolation (which D1 does not support for this query shape).

Reference: Lote 10.6bis P1-6 fix.

## 5. Innovation 3 — Audit-emit-BEFORE-mutation, fail-closed

```rust
// Wrong order:
//   blob_meta.refcount ← expected;
//   emit audit;
//   IF emit fails: ?? (state already mutated, no audit trail)

// Canonical order (WI §6.1.7):
emit_audit(AuditEvent::RefcountAutoFixed { … })?;   // ? = fail-closed
UPDATE blob_meta.refcount = expected ...;
```

The audit emit happens **before** the mutation. If audit emission fails (D1 unavailable, audit chain SDK transient error, etc.), the function returns `Err` and **the mutation never occurs**.

Why this matters: the alternative ("mutate first, audit second, retry audit on failure") creates a window where state diverges from the audit log. For a system whose entire compliance story rests on the audit chain being the source of truth, that window is unacceptable. Fail-closed means the worst case is "drift persists for one more day until next reconcile tick" — never "state changed without trace."

## 6. Innovation 4 — Orphan-R2 detection that explicitly refuses auto-fix

```rust
// blob_meta says refcount > 0 but R2 says blob does not exist:
if stored_refcount > 0 && r2.head(digest).await?.is_not_found() {
    emit_audit(AuditEvent::OrphanR2Detected { … })?;
    // DO NOT mutate. Surface for manual review.
    return Ok(ReconcileDecision::OrphanR2Detected);
}
```

The reconcile **does not auto-fix orphan-R2 cases**, even when the dual gate would permit. Reason: the corruption signal is in R2 (the source of truth for blob existence), not in `blob_meta`. Mutating `blob_meta.refcount = 0` to "match R2" could be exactly wrong — the R2 object may have been deleted by a buggy `PhysicalDelete` phase upstream. Auto-fixing here propagates the upstream bug into the refcount layer and erases evidence.

The right action is **surface and escalate**, not mutate. This is the "first do no harm" principle applied to distributed GC.

## 7. Tiered drift alerting

Two independent thresholds, two SEV levels:

| Threshold | Scope | SEV |
|---|---|---|
| `SEV1_PER_TENANT_DRIFT_PERCENT = 1.0%` | Any single tenant | SEV-1 page |
| `SEV2_GLOBAL_DRIFT_PERCENT = 0.1%` | Aggregate across all tenants | SEV-2 |

The per-tenant alarm catches **localized corruption** (one buggy tenant, one bad migration). The global alarm catches **systemic drift** (region-wide bug, broken counter increment path) that might not exceed 1% in any single tenant but adds up across the fleet.

Validation guard in `ReconcileConfig::new`:

```rust
if sev1_per_tenant <= sev2_global { return Err("SEV-1 floor must exceed SEV-2"); }
if auto_fix_max_percent >= sev2_global { return Err("auto-fix gate must be below SEV-2"); }
```

The configuration **refuses to construct** if the thresholds are not strictly ordered. This prevents an ops misconfiguration from silently disabling either tier of alerting.

## 8. JSON-aware membership (`json_each`, NOT `LIKE`)

```sql
-- Wrong:
WHERE a.blob_refs LIKE '%' || ? || '%'
-- substring collisions: digest "abc" matches blob_refs '["xabcd"]'

-- Right:
FROM ac_meta a, json_each(a.blob_refs) j
WHERE j.value = ?
-- exact element membership
```

The `LIKE` form has a real collision risk: digests that are substrings of other digests appear to be members of arrays that don't actually cite them. At billion-digest scale this hits with non-negligible frequency. The `json_each` form is exact-element membership.

Reference: Lote 10.6bis Part 2a P0-1.

## 9. Phase budget

```rust
pub const CANONICAL_RECONCILE_PHASE_BUDGET_MS: u64 = 60 * 60 * 1000;   // 1 hour
```

Reconcile is a daily cron tick with a 1-hour p99 budget at 1M blobs per (tenant, region). Beyond this budget the phase is `ReconcileError::Backend` and SEV-2 fires. The 1M-blob budget is derived from `O(json_array_size)` join cost × bounded concurrency 4-8 × 1k blobs/chunk batches.

## 10. Anti-patterns explicitly rejected

| Anti-pattern | Why rejected |
|---|---|
| Single-threshold gate (count OR percent alone) | Fails at one end of the scale spectrum |
| Auto-fix for orphan-R2 detection | Propagates upstream bugs; erases evidence |
| Audit-after-mutation, retry on failure | Creates state-without-trace window |
| `LIKE '%digest%'` substring match | Cross-digest collisions at scale |
| Floating `snapshot_at_ms` (re-captured per row) | Mid-phase write race producing false drift |
| Operator override on the auto-fix gate | Same incident surface as TTL-cap override |
| Treating SEV-1 and SEV-2 as equal | Loses systemic-vs-localized signal |
| Cross-tenant query surface | Tenant isolation violation; fail-closed `Backend` error |

## 11. Verification

| Property | Test |
|---|---|
| Gate fires at `count=5, percent=0.0001`, rejects at `count=6` | `tests_scenarios::auto_fix_gate_boundary` |
| Gate rejects at `count=5, percent=0.0002` (only one arm passes) | `tests_scenarios::auto_fix_gate_dual_condition` |
| Audit-emit failure aborts mutation | `tests_scenarios::audit_fail_closed` |
| Snapshot consistency under concurrent writes | `tests_scenarios::snapshot_race_window` (property test) |
| Orphan-R2 never auto-fixes | `tests_scenarios::orphan_r2_no_mutation` |
| `json_each` rejects substring matches | `tests::json_aware_membership` |
| Config refuses inverted thresholds | `tests::config_rejects_inverted_severities` |
| Phase budget exceeded → SEV-2 | `tests::phase_budget_overrun` |

## 12. The composite that makes this SOTA

Any one of the four innovations would be a credible improvement over a naive reconcile. The **composite** — scale-invariant dual gate × snapshot-bounded race window × audit-before-mutation fail-closed × orphan-refuse-to-fix × tiered alerting × JSON-aware membership — is the engineering shape produced by:

1. Operating distributed GC at scale and observing failure modes
2. Encoding each lesson as a structural invariant rather than a runbook entry
3. Refusing to compromise any single invariant for operational expedience

Most CAS systems either (a) don't reconcile (drift accumulates silently), (b) reconcile with single-threshold gates (corrupts at scale), or (c) reconcile with audit-after-mutation (loses forensic trail). The combination shipped here is the right answer to a problem that takes years to fully understand.

## 13. References

- ADR-0028 — Audit envelope contract
- WI-S06-005 — Reconcile phase specification
- WI-S06-007 — Production wiring (D1 + Cron DO + atomic batch)
- Sprint contract §5.5 R-S06-10 — Drift thresholds
- Lote 10.6bis Part 2a P0-1 — `json_each` vs `LIKE` substring fix
- Lote 10.6bis Part 2a P0-6 — Auto-fix dual-condition gate
- Lote 10.6bis P1-6 — Snapshot-bounded race window
- `specs/03_architecture/gc_model.md` — Phase machine
- Bernstein, P.A.; Hadzilacos, V.; Goodman, N. (1987). *Concurrency Control and Recovery in Database Systems*. Ch. 5 (snapshot isolation theory)
