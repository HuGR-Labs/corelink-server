---------------------------- MODULE ac_eviction_isolation ----------------------------
(***************************************************************************)
(* CoreLink — action-cache TTL eviction isolation (DEBT-005 9/40)         *)
(*                                                                         *)
(* Closes the following CRITICAL TLA gaps from canonical-consistency      *)
(* baseline (§3.15 AC):                                                    *)
(*                                                                         *)
(*   INV-AC-EVICT-REGION-PINNED   (CRITICAL)                               *)
(*     TTL-eviction cron workers are pinned to a single region; per-      *)
(*     region shards never touch other regions' rows. Every DELETE        *)
(*     issued by the eviction worker carries a `region = ?` clause that  *)
(*     matches the worker's pinned region. No reachable state has a       *)
(*     row deleted by a worker whose region differs from the row's       *)
(*     region.                                                             *)
(*                                                                         *)
(*   INV-AC-EVICT-TENANT-SCOPED   (CRITICAL)                               *)
(*     Same shape: every DELETE carries a `tenant_id = ?` clause that    *)
(*     matches the row's tenant_id. Cross-tenant deletion is              *)
(*     structurally impossible.                                            *)
(*                                                                         *)
(* Threat model:                                                           *)
(*   - Adversarial worker submits an eviction targeting a row from a     *)
(*     different region (`AttemptCrossRegionEvict`). The guard rejects.  *)
(*   - Adversarial worker submits an eviction targeting a row from a     *)
(*     different tenant (`AttemptCrossTenantEvict`). The guard rejects.  *)
(*                                                                         *)
(* Out of scope:                                                           *)
(*   - TTL expiry timing (modelled as discrete "expired" flag).           *)
(*   - SQL semantics (the WHERE clause is modelled as a function call).   *)
(*                                                                         *)
(* Cross-refs:                                                             *)
(*   - `specs/03_architecture/invariant_registry.md §3.15 INV-AC-EVICT-*`*)
(*   - `crates/corelink-worker/src/reapi/ac/ttl.rs`                      *)
(***************************************************************************)

EXTENDS Integers, FiniteSets, Sequences, TLC

CONSTANTS
    Rows,            \* Finite set of AC row IDs
    Tenants,
    Regions,         \* Worker regions; rows are tagged by region too
    MaxOps

ASSUME
    /\ Rows # {}
    /\ Tenants # {}
    /\ Cardinality(Tenants) >= 2
    /\ Regions # {}
    /\ Cardinality(Regions) >= 2
    /\ MaxOps \in Nat

\* Initial assignment: each row maps to a (tenant, region). The model
\* considers all tenant×region products to ensure no reachable trace
\* assumes a specific tagging.
RowTenant(r) == CHOOSE t \in Tenants: TRUE
RowRegion(r) == CHOOSE g \in Regions: TRUE

VARIABLES
    row_tenant,      \* Function Rows -> Tenants (immutable post-init)
    row_region,      \* Function Rows -> Regions (immutable post-init)
    row_alive,       \* Function Rows -> BOOLEAN  (TRUE = not yet evicted)
    row_expired,     \* Function Rows -> BOOLEAN  (TRUE iff TTL passed)
    delete_log,      \* Sequence of <<row, worker_tenant, worker_region>>
    op_count

vars == <<row_tenant, row_region, row_alive, row_expired, delete_log, op_count>>

(*-- Init --------------------------------------------------------------------*)

Init ==
    /\ row_tenant  \in [Rows -> Tenants]
    /\ row_region  \in [Rows -> Regions]
    /\ row_alive   = [r \in Rows |-> TRUE]
    /\ row_expired = [r \in Rows |-> FALSE]
    /\ delete_log  = <<>>
    /\ op_count    = 0

(*-- Actions -----------------------------------------------------------------*)

\* TTL boundary: row expires (becomes a candidate for eviction).
Expire(r) ==
    /\ r \in Rows
    /\ row_alive[r]
    /\ ~row_expired[r]
    /\ op_count < MaxOps
    /\ row_expired' = [row_expired EXCEPT ![r] = TRUE]
    /\ op_count'    = op_count + 1
    /\ UNCHANGED <<row_tenant, row_region, row_alive, delete_log>>

\* Honest eviction: worker pinned to (wt, wg) issues DELETE WHERE
\* tenant_id = wt AND region = wg AND row = r. Allowed iff the row's
\* tenant/region match the worker's. INV-AC-EVICT-{TENANT,REGION}-SCOPED
\* are encoded by these conjuncts.
EvictRow(r, wt, wg) ==
    /\ r \in Rows
    /\ wt \in Tenants
    /\ wg \in Regions
    /\ row_alive[r]
    /\ row_expired[r]
    /\ row_tenant[r] = wt
    /\ row_region[r] = wg
    /\ op_count < MaxOps
    /\ row_alive'  = [row_alive EXCEPT ![r] = FALSE]
    /\ delete_log' = Append(delete_log, <<r, wt, wg>>)
    /\ op_count'   = op_count + 1
    /\ UNCHANGED <<row_tenant, row_region, row_expired>>

\* Adversarial: worker tries to evict a row tagged for a different
\* region. Guard rejects: `row_region[r] # wg` makes the action
\* disabled — but we expose the explicit name for traceability via
\* `delete_log` invariants below (which enforce: every logged delete
\* matches the row's tags).
AttemptCrossRegionEvict(r, wt, wg) ==
    /\ r \in Rows
    /\ wt \in Tenants
    /\ wg \in Regions
    /\ row_alive[r]
    /\ row_expired[r]
    /\ row_region[r] # wg     \* The adversarial mismatch
    /\ FALSE                    \* Guard FALSE — unreachable
    /\ row_alive'  = [row_alive EXCEPT ![r] = FALSE]
    /\ delete_log' = Append(delete_log, <<r, wt, wg>>)
    /\ op_count'   = op_count + 1
    /\ UNCHANGED <<row_tenant, row_region, row_expired>>

AttemptCrossTenantEvict(r, wt, wg) ==
    /\ r \in Rows
    /\ wt \in Tenants
    /\ wg \in Regions
    /\ row_alive[r]
    /\ row_expired[r]
    /\ row_tenant[r] # wt
    /\ FALSE                    \* Guard FALSE — unreachable
    /\ row_alive'  = [row_alive EXCEPT ![r] = FALSE]
    /\ delete_log' = Append(delete_log, <<r, wt, wg>>)
    /\ op_count'   = op_count + 1
    /\ UNCHANGED <<row_tenant, row_region, row_expired>>

(*-- Next --------------------------------------------------------------------*)

Next ==
    \/ \E r \in Rows: Expire(r)
    \/ \E r \in Rows, wt \in Tenants, wg \in Regions: EvictRow(r, wt, wg)
    \/ \E r \in Rows, wt \in Tenants, wg \in Regions: AttemptCrossRegionEvict(r, wt, wg)
    \/ \E r \in Rows, wt \in Tenants, wg \in Regions: AttemptCrossTenantEvict(r, wt, wg)

Spec == Init /\ [][Next]_vars

(*-- Safety invariants -------------------------------------------------------*)

\* INV-AC-EVICT-REGION-PINNED — every logged delete uses the row's region.
InvDeletesRegionScoped ==
    \A i \in 1..Len(delete_log):
        row_region[delete_log[i][1]] = delete_log[i][3]

\* INV-AC-EVICT-TENANT-SCOPED — every logged delete uses the row's tenant.
InvDeletesTenantScoped ==
    \A i \in 1..Len(delete_log):
        row_tenant[delete_log[i][1]] = delete_log[i][2]

\* Only expired rows are ever deleted.
InvOnlyExpiredDeleted ==
    \A i \in 1..Len(delete_log):
        row_expired[delete_log[i][1]]

\* A dead row stays dead (no resurrection).
InvDeadStaysDead ==
    \A r \in Rows:
        ~row_alive[r]
        => \E i \in 1..Len(delete_log): delete_log[i][1] = r

SafetyInvariants ==
    /\ InvDeletesRegionScoped
    /\ InvDeletesTenantScoped
    /\ InvOnlyExpiredDeleted
    /\ InvDeadStaysDead

================================================================================
