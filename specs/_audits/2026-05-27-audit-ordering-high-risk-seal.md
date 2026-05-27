---
id: "AUDIT-2026-05-27-AUDIT-ORDERING-HIGH-RISK"
type: "audit"
doc_status: "ACTIVE"
audit_status: "SEALED"
version: "1.0.0"
created: "2026-05-27"
updated: "2026-05-27"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["audit", "INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER", "ordering", "high-risk", "escalation", "seal"]
references:
  - "specs/_audits/2026-05-27-audit-ordering-sweep-seal.md"
  - "specs/_audits/2026-05-27-charter-strict-audit-post-w36.md"
  - "specs/_audits/2026-05-27-drata-fail-closed-fix.md"
  - "specs/03_architecture/invariant_registry.md"
  - "specs/03_architecture/adrs/ADR-0035-ac-handler-invariants.md"
  - "specs/04_sprints/_sealed/S04/work_items/WI-S04-001-reapi-actioncache-handlers.md"
  - "specs/04_sprints/S10/work_items/WI-S10-001-usage-event-emitter-r2-append-only-idempotency.md"
---

# Audit ordering HIGH-RISK escalations SEAL

Resolves the 5 HIGH-RISK escalations from
`specs/_audits/2026-05-27-audit-ordering-sweep-seal.md` (commit
`aa2430c0` in main).

## §1 User mandate

> "investiga e pode decidir voce" — Gustavo Schneiter, 2026-05-27.

Full autonomous decision authority delegated on the 5 escalations.

## §2 Per-site decision matrix

| # | Site | Orchestrator pre-classification | Final verdict | Action taken |
|---|---|---|---|---|
| 1 | `corelink-billing-emit/src/emitter.rs:238,259,272,286` | BUG (suspected) | **INTENTIONAL** | Inline-doc — decision-driven; fail-OPEN hot path per WI-S10-001 §5.1 |
| 2 | `corelink-gc/src/mark.rs:924` | BUG (suspected) | **INTENTIONAL** | Inline-doc — `PhaseStarted` (line 815) is the fail-CLOSED envelope; `PhaseCompleted` carries aggregates |
| 3 | `corelink-gc/src/physical_delete.rs:938` | INTENTIONAL (suspected) | **INTENTIONAL** | Inline-doc — clarified R2 external-API observability vs OWN-state INV scope |
| 4 | `corelink-worker/src/reapi/ac/handler/methods.rs:408,430` | INTENTIONAL (suspected) | **INTENTIONAL** | Inline-doc — WI-S04-001 §9.5 + ADR-0035 + upstream fail-CLOSED audits at 297/318/350 |
| 5 | `corelink-audit-chain/src/neon_shadow/real.rs:605,621,650,666,685` | INTENTIONAL (suspected) | **INTENTIONAL** | Inline-doc — txn-bracketed observability; shadow-of-audit layered defence separation |

**Net outcome**: 0 bug fixes (orchestrator suspected 2); 5
inline-doc clarifications. Investigation found that the
orchestrator's two BUG suspects were both legitimate decision-driven
or completion-event patterns that the WI specs and adjacent code
already established as canonical.

## §3 Per-site reasoning

### §3.1 Escalation 1 — `corelink-billing-emit/src/emitter.rs`

**Site**: `emit()` fn lines 238 / 259 / 272 / 286 — 4 audit emits
discriminated by `idempotency.insert(...)` return value
(`Accepted` / `DuplicateRejected` / `IdempotencyCollision` /
sink-failure).

**Investigation**:
- `IdempotencyTracker::insert` (`idempotency.rs:146`) IS an
  OWN-state mutation (inserts the canonical key into the accepted
  set; production = D1 `usage_event_staging`
  `(tenant_id, request_id)` UNIQUE).
- The 4 audit event TYPES are determined BY the insert's return.
  Pre-emitting them is structurally impossible.
- Canonical two-event pattern (intent + outcome, as used by
  `corelink-billing/src/quota/cas/cas.rs:319/359/379`) would require
  adding a new `UsageEmitAttempted` variant + breaking the
  cardinality budget inherited from WI-S09-001.

**Spec authority** — WI-S10-001 §5.1 R-S10-2 + §5.7 SLO-FRESH-BILLING
explicitly designates billing-emit as **fail-OPEN at the hot path**
(distinct from WI-S09-004 audit fail-CLOSED tier). The split-tier
model is documented:
- Hot path SLA p99 ≤ 3ms must be preserved (synchronous block on
  emit failure cascades to customer 5xx).
- Staging table `(tenant_id, request_id)` UNIQUE = canonical
  commit-of-intent.
- Retry queue + SLO-FRESH-BILLING ≤ 15min eventual reconciliation
  is the backstop for emit failures.

**Verdict**: INTENTIONAL. The idempotency.insert IS the
commit-of-intent; the audit observes the decision outcome. Doubling
audit cardinality with a "Started" variant would violate the WI
cardinality budget AND the hot-path SLO.

**Action**: Inline doc-comment block (lines 227–251) cites
INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER scope clarification + WI-S10-001
§5.1 R-S10-2 + §5.7 fail-OPEN tier + INV-BILLING-NO-LOSS /
INV-BILLING-NO-DUP foundation.

### §3.2 Escalation 2 — `corelink-gc/src/mark.rs:924`

**Site**: `MarkPhase::execute()` — emit `PhaseTransitioned`
(`mark_phase_completed`) AFTER candidates.insert_candidate (loop
line 887), runs.checkpoint (line 909), metrics.record_phase_duration
(line 921).

**Investigation**:
- Audit payload (`reachable_count`, `candidates_count`,
  `duration_ms`, `phase_end_now`) is computed FROM the mutations.
  Pre-emit impossible without the aggregates.
- **`PhaseStarted` emit ALREADY exists at line 815** (BEFORE any
  mutation), with `from_phase=Idle → to_phase=Mark` +
  `reason="mark_phase_started"`. This IS the canonical
  fail-CLOSED envelope.
- Two-event pattern (intent + outcome) is fully satisfied. Mirrors:
  - `corelink-billing/src/quota/cas/cas.rs:359` (CasCheckPassed →
    try_commit_delta → CasCommitSucceeded);
  - drata `Sent` post-fix (`specs/_audits/2026-05-27-drata-fail-closed-fix.md`);
  - `corelink-replica-worker::ReplicationStarted` →
    `ReplicationCompleted`.

**Verdict**: INTENTIONAL. The orchestrator suspicion missed the
upstream `PhaseStarted` audit. The flagged emit at line 924 is the
outcome-event of the started-pair, not an isolated post-mutation
audit.

**Action**: Inline doc-comment block (lines 923–937) cites the
two-event pattern + line 815 `PhaseStarted` envelope + the
drata/replica adjacencies.

### §3.3 Escalation 3 — `corelink-gc/src/physical_delete.rs:938`

**Site**: `physical_delete()` fn — emit `PhysicalDeleted` AFTER
`r2.delete(...)`.

**Investigation**:
- R2 is EXTERNAL state, idempotent (PAT-RETRY-IDEMPOTENT-001;
  `Deleted` and `NotFound` both = success).
- Audit precedes the OWN-state D1 mutations
  (`conditional_purge` step 6, `transition_status` step 7) — the
  canonical fail-CLOSED envelope IS in place for OWN state.
- INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER (registry §INV-AUDIT-...)
  specifies `db.batch([INSERT blob_meta OR TenantCtx, INSERT
  audit_outbox])` atomic — OWN-state pairing. External APIs (R2,
  KMS, Slack, Statuspage) follow the external-API observability
  pattern (audit AFTER the external call, reporting outcome).
- Lote 10.6bis P0-2 explicitly designed R2-first ordering with D1
  batch rollback on emit failure.

**Verdict**: INTENTIONAL. R2-first is documented architecture; OWN
state IS preceded by emit. The INV scope is OWN state, not external
APIs.

**Action**: Inline doc-comment expanded (lines 928–945) explicitly
classifies R2 as external-API observability (citing parallel
patterns: byok KMS, slack, statuspage) and reaffirms OWN-state
fail-CLOSED envelope on the D1 batch.

### §3.4 Escalation 4 — `corelink-worker/src/reapi/ac/handler/methods.rs:408,430`

**Site**: `update_action_result_inner()` — emit
`UpdateResultMismatch` (line 408) and `UpdateOk` (line 430) AFTER
envelope_store.put (line 374), persist_action_result (line 384),
meta.upsert (line 391).

**Investigation**:
- WI-S04-001 §9.5 ("Why R2-first then D1 INSERT (não D1-first)")
  explicitly designs this ordering. R2 content-addressable +
  idempotent (overwrite OK); D1 INSERT failure leaves orphan R2
  (recoverable via S-06 GC reconcile). Reverse would create ghost
  rows (404 forever).
- ADR-0035 ("AC handler invariants") ratifies the design: "R2-first
  then D1 INSERT-or-idempotent".
- Upstream fail-CLOSED audits ARE in place — `UpdateMerkleInvalid`
  (line 297), `UpdateOutputsMissing` (line 318), `UpdateSigInvalid`
  (line 350) — all fire BEFORE any state mutation, covering the
  pre-decision envelope.
- `UpdateResultMismatch` at line 408 is decision-driven on
  `meta.upsert` return; structurally same shape as
  Escalation 1.
- `UpdateOk` at line 430 is the completion-event mirroring the
  PhaseCompleted / ReplicationCompleted pattern.

**Verdict**: INTENTIONAL. WI-S04-001 §9.5 + ADR-0035 are explicit
architecture; upstream fail-CLOSED audits cover the pre-decision
envelope; downstream emits are completion + decision-driven.

**Action**: Inline doc-comment block (lines 369–390) cites
INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER scope clarification + WI-S04-001
§9.5 + ADR-0035 + R-009 mitigation reference + upstream fail-CLOSED
audit chain.

### §3.5 Escalation 5 — `corelink-audit-chain/src/neon_shadow/real.rs:605,621,650,666,685`

**Site**: `sync_chunk()` — shadow-sync to Neon Postgres
(`BEGIN → SET RLS GUC → INSERT (per row) → COMMIT`). Failure-arm
audits fire after each `executor.execute(...)` that failed; success
audit fires after COMMIT.

**Investigation**:
- Failure-arm audits CANNOT logically precede the SQL call they
  observe a failure of (cannot audit "BEGIN failed" before BEGIN was
  attempted).
- Success-arm audit at line 685 fires after COMMIT — sync was the
  observed outcome.
- The shadow-sync layer is OBSERVATIONAL of the primary audit emit;
  primary audit emit at the source site IS
  INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER governed (per the D1 batch
  pattern at each primary writer).
- Moving `emit_audit` (itself a SQL INSERT into `shadow_audit`)
  INTO the per-row INSERT txn would change commit semantics:
  audit-of-sync would become part of sync atomicity, violating the
  layered defence separation per RB-REPLICA-FAILOVER §audit-chain
  layered defences.

**Verdict**: INTENTIONAL. Txn-bracketed observability is the
canonical pattern for this layer; INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER
governs the primary write site, not the audit-chain shadow.

**Action**: Inline doc-comment block (top of `sync_chunk` body)
cites INV scope clarification + RB-REPLICA-FAILOVER layered defence
separation + commit-semantics rationale.

## §4 Verification

Per-crate acceptance gates run on the 4 affected crates
(corelink-billing-emit + corelink-gc + corelink-worker +
corelink-audit-chain).

- `cargo build -p corelink-billing-emit -p corelink-gc -p corelink-worker -p corelink-audit-chain` → **GREEN**
- `cargo clippy -p corelink-billing-emit -p corelink-gc -p corelink-worker -p corelink-audit-chain --tests -- -D warnings` → **GREEN**
- `cargo test -p corelink-billing-emit -p corelink-gc -p corelink-worker -p corelink-audit-chain --no-run` → **GREEN**

(All edits are inline doc comments — `//`-prefixed; pure
documentation; no AST-level change; no test assertion should change.)

## §5 Charter consequence

INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER scope is hereby clarified
in-tree (inline doc comments at 5 sites). The invariant applies to
**OWN-state mutations** (D1 batch pairing canonical). It does NOT
apply to:

1. **Decision-driven audit variants** where the audit event TYPE is
   discriminated by a commit-of-intent gate (idempotency tracker,
   meta upsert) — provided the upstream pre-decision envelope is
   present and downstream emits cover all outcome arms.
2. **External-API observability** (R2, KMS, Slack, Statuspage,
   external HTTP) — audit reports outcome of external call;
   external APIs are not OWN state; OWN-state mutations downstream
   MUST still be preceded by emit per INV.
3. **Completion / outcome events** in a two-event (intent +
   outcome) pair — provided the `*Started` / `*CheckPassed` /
   `*Attempted` emit at the start of the block satisfies the
   fail-CLOSED envelope.
4. **Audit-chain shadow / replica sync layers** — txn-bracketed
   observability of the SQL outcome cannot precede the SQL call;
   primary audit emit at the source site retains
   INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER governance.

A future charter amendment (out of scope for this SEAL; recommend
adding a §INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER-SCOPE clarification to
`specs/03_architecture/invariant_registry.md`) should formalize
these 4 exemptions. Until that amendment lands, the inline doc
comments at the 5 sites are the canonical reference.

## §6 DCO sign-off

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>.
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>.

**End of audit ordering HIGH-RISK escalations SEAL.**
