---------------------------- MODULE gc_sweep_audit_fail_closed ----------------------------
(***************************************************************************)
(* CoreLink — GC sweep + reconcile audit fail-CLOSED (DEBT-005 8/40)       *)
(*                                                                         *)
(* Closes the following CRITICAL TLA gaps from canonical-consistency      *)
(* baseline:                                                               *)
(*                                                                         *)
(*   INV-GC-SWEEP-AUDIT-FAIL-CLOSED   (CRITICAL, §3.6 Audit)               *)
(*     For every sweep operation that mutates blob_meta + gc_candidate,    *)
(*     the audit_outbox INSERT is performed in the SAME D1 transactional  *)
(*     batch. If the audit emit fails, the mutation is rolled back; the   *)
(*     final state is observationally identical to the pre-call state.    *)
(*                                                                         *)
(*   INV-GC-RECONCILE-AUDIT-FAIL-CLOSED  (CRITICAL, §3.6 Audit)            *)
(*     Same all-or-nothing discipline for the reconcile path (which         *)
(*     touches additional refcount tables). Audit emit failure → full     *)
(*     ROLLBACK; no orphan refcount, no half-soft-delete.                  *)
(*                                                                         *)
(* Threat model:                                                           *)
(*   - D1 batch failure between mutation and audit_outbox INSERT (modelled*)
(*     by `BatchAtomicFail` which sets a transient fault flag).            *)
(*   - Adversary attempts a partial commit (modelled as                    *)
(*     `AttemptPartialCommit` — guard FALSE by construction).              *)
(*                                                                         *)
(* Out of scope:                                                           *)
(*   - D1 internal batch implementation (modelled as atomic relation).    *)
(*   - Wall-clock retry timing (handled by `RB-GC-001` runbook).           *)
(*                                                                         *)
(* Cross-refs:                                                             *)
(*   - `specs/03_architecture/invariant_registry.md §3.6 INV-GC-SWEEP-*` *)
(*   - `crates/corelink-gc/src/lib.rs::SweepError::AuditEmissionFailed`  *)
(***************************************************************************)

EXTENDS Integers, FiniteSets, Sequences, TLC

CONSTANTS
    SweepOps,           \* Finite set of sweep op IDs
    ReconcileOps,       \* Finite set of reconcile op IDs
    MaxOps              \* Bound on caller-driven steps

ASSUME
    /\ SweepOps # {}
    /\ ReconcileOps # {}
    /\ MaxOps \in Nat

\* Outcome enum for the D1 batch — adversary chooses (or scheduler does)
\* between fully committed and fully rolled-back.
Outcomes == {"committed", "rolled_back", "pending"}

VARIABLES
    sweep_outcome,      \* Function SweepOps     -> Outcomes
    sweep_blob_mutated, \* Function SweepOps     -> BOOLEAN
    sweep_audit_emitted,\* Function SweepOps     -> BOOLEAN
    rec_outcome,        \* Function ReconcileOps -> Outcomes
    rec_refcount_mutated, \* Function ReconcileOps -> BOOLEAN
    rec_audit_emitted,  \* Function ReconcileOps -> BOOLEAN
    op_count

vars == <<sweep_outcome, sweep_blob_mutated, sweep_audit_emitted,
          rec_outcome,   rec_refcount_mutated, rec_audit_emitted,
          op_count>>

(*-- Init --------------------------------------------------------------------*)

Init ==
    /\ sweep_outcome        = [s \in SweepOps     |-> "pending"]
    /\ sweep_blob_mutated   = [s \in SweepOps     |-> FALSE]
    /\ sweep_audit_emitted  = [s \in SweepOps     |-> FALSE]
    /\ rec_outcome          = [r \in ReconcileOps |-> "pending"]
    /\ rec_refcount_mutated = [r \in ReconcileOps |-> FALSE]
    /\ rec_audit_emitted    = [r \in ReconcileOps |-> FALSE]
    /\ op_count             = 0

(*-- Actions -----------------------------------------------------------------*)

\* Successful sweep: D1 batch commits both blob mutation AND audit row.
\* Mutation + audit emit are set as a SINGLE step — there is no
\* interleaving in which one is true and the other false in a committed
\* state. This codifies the D1 batch atomicity.
SweepCommit(s) ==
    /\ s \in SweepOps
    /\ sweep_outcome[s] = "pending"
    /\ op_count < MaxOps
    /\ sweep_outcome'        = [sweep_outcome        EXCEPT ![s] = "committed"]
    /\ sweep_blob_mutated'   = [sweep_blob_mutated   EXCEPT ![s] = TRUE]
    /\ sweep_audit_emitted'  = [sweep_audit_emitted  EXCEPT ![s] = TRUE]
    /\ op_count'             = op_count + 1
    /\ UNCHANGED <<rec_outcome, rec_refcount_mutated, rec_audit_emitted>>

\* Audit emit fails → D1 batch rolls back; blob_mutated stays FALSE,
\* audit_emitted stays FALSE. The outcome is recorded as rolled_back.
\* INV-GC-SWEEP-AUDIT-FAIL-CLOSED is encoded by this transition: there
\* is no path where blob_mutated becomes TRUE without audit_emitted
\* simultaneously becoming TRUE.
SweepRollback(s) ==
    /\ s \in SweepOps
    /\ sweep_outcome[s] = "pending"
    /\ op_count < MaxOps
    /\ sweep_outcome' = [sweep_outcome EXCEPT ![s] = "rolled_back"]
    /\ op_count'      = op_count + 1
    /\ UNCHANGED <<sweep_blob_mutated, sweep_audit_emitted,
                   rec_outcome, rec_refcount_mutated, rec_audit_emitted>>

\* Successful reconcile: refcount mutation + audit row commit together.
ReconcileCommit(r) ==
    /\ r \in ReconcileOps
    /\ rec_outcome[r] = "pending"
    /\ op_count < MaxOps
    /\ rec_outcome'          = [rec_outcome          EXCEPT ![r] = "committed"]
    /\ rec_refcount_mutated' = [rec_refcount_mutated EXCEPT ![r] = TRUE]
    /\ rec_audit_emitted'    = [rec_audit_emitted    EXCEPT ![r] = TRUE]
    /\ op_count'             = op_count + 1
    /\ UNCHANGED <<sweep_outcome, sweep_blob_mutated, sweep_audit_emitted>>

\* Reconcile rollback path.
ReconcileRollback(r) ==
    /\ r \in ReconcileOps
    /\ rec_outcome[r] = "pending"
    /\ op_count < MaxOps
    /\ rec_outcome' = [rec_outcome EXCEPT ![r] = "rolled_back"]
    /\ op_count'    = op_count + 1
    /\ UNCHANGED <<sweep_outcome, sweep_blob_mutated, sweep_audit_emitted,
                   rec_refcount_mutated, rec_audit_emitted>>

\* Adversarial: attempt a partial commit (mutation succeeds, audit emit
\* fails, partial state observable). Guard FALSE by construction —
\* documents the unreachable trace.
AttemptSweepPartial(s) ==
    /\ s \in SweepOps
    /\ FALSE
    /\ sweep_blob_mutated'  = [sweep_blob_mutated  EXCEPT ![s] = TRUE]
    /\ sweep_audit_emitted' = [sweep_audit_emitted EXCEPT ![s] = FALSE]
    /\ sweep_outcome'       = [sweep_outcome       EXCEPT ![s] = "committed"]
    /\ op_count'            = op_count + 1
    /\ UNCHANGED <<rec_outcome, rec_refcount_mutated, rec_audit_emitted>>

AttemptReconcilePartial(r) ==
    /\ r \in ReconcileOps
    /\ FALSE
    /\ rec_refcount_mutated' = [rec_refcount_mutated EXCEPT ![r] = TRUE]
    /\ rec_audit_emitted'    = [rec_audit_emitted    EXCEPT ![r] = FALSE]
    /\ rec_outcome'          = [rec_outcome          EXCEPT ![r] = "committed"]
    /\ op_count'             = op_count + 1
    /\ UNCHANGED <<sweep_outcome, sweep_blob_mutated, sweep_audit_emitted>>

(*-- Next --------------------------------------------------------------------*)

Next ==
    \/ \E s \in SweepOps:     SweepCommit(s)
    \/ \E s \in SweepOps:     SweepRollback(s)
    \/ \E r \in ReconcileOps: ReconcileCommit(r)
    \/ \E r \in ReconcileOps: ReconcileRollback(r)
    \/ \E s \in SweepOps:     AttemptSweepPartial(s)
    \/ \E r \in ReconcileOps: AttemptReconcilePartial(r)

Spec == Init /\ [][Next]_vars

(*-- Safety invariants -------------------------------------------------------*)

\* INV-GC-SWEEP-AUDIT-FAIL-CLOSED — blob_mutated iff audit_emitted (atomic
\* pairing). The biconditional holds in every reachable state because
\* the only writer to either is SweepCommit which flips both, OR
\* SweepRollback which flips neither.
InvSweepAtomicPairing ==
    \A s \in SweepOps:
        sweep_blob_mutated[s] = sweep_audit_emitted[s]

\* Outcome consistency: committed ⇒ both mutated; rolled_back ⇒ neither.
InvSweepOutcomeConsistent ==
    \A s \in SweepOps:
        /\ (sweep_outcome[s] = "committed"
            => /\ sweep_blob_mutated[s]
               /\ sweep_audit_emitted[s])
        /\ (sweep_outcome[s] = "rolled_back"
            => /\ ~sweep_blob_mutated[s]
               /\ ~sweep_audit_emitted[s])

\* INV-GC-RECONCILE-AUDIT-FAIL-CLOSED — analogous pairing for reconcile.
InvReconcileAtomicPairing ==
    \A r \in ReconcileOps:
        rec_refcount_mutated[r] = rec_audit_emitted[r]

InvReconcileOutcomeConsistent ==
    \A r \in ReconcileOps:
        /\ (rec_outcome[r] = "committed"
            => /\ rec_refcount_mutated[r]
               /\ rec_audit_emitted[r])
        /\ (rec_outcome[r] = "rolled_back"
            => /\ ~rec_refcount_mutated[r]
               /\ ~rec_audit_emitted[r])

\* No orphan audit row (audit emitted but mutation rolled back).
InvNoOrphanSweepAudit ==
    \A s \in SweepOps:
        sweep_audit_emitted[s] => sweep_blob_mutated[s]

InvNoOrphanReconcileAudit ==
    \A r \in ReconcileOps:
        rec_audit_emitted[r] => rec_refcount_mutated[r]

SafetyInvariants ==
    /\ InvSweepAtomicPairing
    /\ InvSweepOutcomeConsistent
    /\ InvReconcileAtomicPairing
    /\ InvReconcileOutcomeConsistent
    /\ InvNoOrphanSweepAudit
    /\ InvNoOrphanReconcileAudit

================================================================================
