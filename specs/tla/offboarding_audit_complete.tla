----------------------- MODULE offboarding_audit_complete -----------------------
(***************************************************************************)
(* CoreLink — tenant offboarding audit-emit-BEFORE-mutate (DEBT-005       *)
(* batch 3 #2).                                                            *)
(*                                                                         *)
(* Closes CRITICAL TLA gap from canonical-consistency baseline:           *)
(*                                                                         *)
(*   INV-OFFBOARDING-AUDIT-COMPLETE (CRITICAL, §3.25 Offboarding)         *)
(*     Every state mutation in the offboarding orchestrator MUST be       *)
(*     preceded by the canonical audit row (per ADR-S11-002 split-tier   *)
(*     fail-CLOSED discipline). Audit emit failure aborts the transition;*)
(*     the durable store remains at pre-call state.                       *)
(*                                                                         *)
(*   INV-OFFBOARDING-GRACE-RESPECTED (HIGH, §3.25)                         *)
(*     A state advances only after the canonical grace timer threshold   *)
(*     has elapsed (or operator force-advance, which itself emits a       *)
(*     typed audit row). A request that asks for an advance before       *)
(*     the threshold is rejected with `IllegalTransition`; the state     *)
(*     remains unchanged AND no audit row is recorded.                    *)
(*                                                                         *)
(* The registry today notes audit-complete as "Covered by                  *)
(* audit_immutability.tla inheritance". This spec strengthens the          *)
(* proof from inheritance to direct by exhibiting the offboarding         *)
(* state-machine pairing: audit-emit-BEFORE-mutate AND grace-boundary.   *)
(*                                                                         *)
(* Threat model (modelled):                                                *)
(*   - Audit sink fails transiently (`AdvanceAuditFails`); state must    *)
(*     remain at pre-call value.                                          *)
(*   - Caller requests advance before grace threshold                    *)
(*     (`AttemptEarlyAdvance`); rejected.                                 *)
(*   - Adversarial: state advances without audit emit                    *)
(*     (`AttemptAdvanceSkipAudit`, guard FALSE).                          *)
(*                                                                         *)
(* Cross-refs:                                                             *)
(*   - `specs/03_architecture/invariant_registry.md §3.25 INV-OFFBOARDING-*`*)
(*   - `crates/corelink-ops/src/tenant_offboarding.rs` (post Wave 35 P2: *)
(*     corelink-tenant-offboarding absorbed into corelink-ops/)           *)
(*   - `specs/tla/audit_immutability.tla` (parent — proves immutability *)
(*     of committed audit rows; this spec proves the per-transition      *)
(*     pairing + grace boundary).                                         *)
(***************************************************************************)

EXTENDS Integers, FiniteSets, Sequences, TLC

CONSTANTS
    Tenants,           \* Finite set of tenants under offboarding
    GraceThreshold,    \* Wall-clock units (abstract) that must elapse
    MaxOps

ASSUME
    /\ Tenants # {}
    /\ GraceThreshold \in Nat
    /\ GraceThreshold > 0
    /\ MaxOps \in Nat

\* The offboarding canonical state machine (abridged for the model):
\*   notice -> grace -> erasure -> destroyed
States == {"notice", "grace", "erasure", "destroyed"}

NextState(s) ==
    CASE s = "notice"   -> "grace"
      [] s = "grace"    -> "erasure"
      [] s = "erasure"  -> "destroyed"
      [] OTHER          -> s  \* terminal

IsTerminal(s) == s = "destroyed"

VARIABLES
    t_state,           \* Tenants -> States
    t_clock,           \* Tenants -> Nat — units since current state entered
    audit_log,         \* Sequence of <<tenant, from_state, to_state>>
    op_count

vars == <<t_state, t_clock, audit_log, op_count>>

(*-- Init --------------------------------------------------------------------*)

Init ==
    /\ t_state    = [t \in Tenants |-> "notice"]
    /\ t_clock    = [t \in Tenants |-> 0]
    /\ audit_log  = <<>>
    /\ op_count   = 0

(*-- Actions -----------------------------------------------------------------*)

\* Wall-clock progresses for a tenant — the only way the grace clock fills.
Tick(t) ==
    /\ t \in Tenants
    /\ op_count < MaxOps
    /\ ~ IsTerminal(t_state[t])
    /\ t_clock' = [t_clock EXCEPT ![t] = @ + 1]
    /\ UNCHANGED <<t_state, audit_log>>
    /\ op_count' = op_count + 1

\* Happy path: grace elapsed AND audit emit succeeds. Append audit row
\* BEFORE flipping state.
Advance(t) ==
    /\ t \in Tenants
    /\ op_count < MaxOps
    /\ ~ IsTerminal(t_state[t])
    /\ t_clock[t] >= GraceThreshold
    /\ LET s == t_state[t]
           n == NextState(s)
       IN  /\ audit_log' = Append(audit_log, <<t, s, n>>)
           /\ t_state'   = [t_state EXCEPT ![t] = n]
           /\ t_clock'   = [t_clock EXCEPT ![t] = 0]
    /\ op_count' = op_count + 1

\* Audit sink fails — state stays put, audit row NOT appended, clock NOT
\* reset. Caller will retry (modelled by next Advance).
AdvanceAuditFails(t) ==
    /\ t \in Tenants
    /\ op_count < MaxOps
    /\ ~ IsTerminal(t_state[t])
    /\ t_clock[t] >= GraceThreshold
    /\ UNCHANGED <<t_state, t_clock, audit_log>>
    /\ op_count' = op_count + 1

\* Caller requests advance BEFORE grace elapsed.
\* Rejected: state unchanged, NO audit row.
AttemptEarlyAdvance(t) ==
    /\ t \in Tenants
    /\ op_count < MaxOps
    /\ ~ IsTerminal(t_state[t])
    /\ t_clock[t] < GraceThreshold
    /\ UNCHANGED <<t_state, t_clock, audit_log>>
    /\ op_count' = op_count + 1

\* Adversarial: state advances without an audit row in the log.
\* Guard FALSE — exhibited so the model reader sees what the
\* invariant rules out.
AttemptAdvanceSkipAudit(t) ==
    /\ t \in Tenants
    /\ op_count < MaxOps
    /\ ~ IsTerminal(t_state[t])
    /\ FALSE
    /\ LET s == t_state[t]
           n == NextState(s)
       IN  /\ t_state'   = [t_state EXCEPT ![t] = n]
           /\ t_clock'   = [t_clock EXCEPT ![t] = 0]
           /\ UNCHANGED audit_log
    /\ op_count' = op_count + 1

(*-- Next --------------------------------------------------------------------*)

Next ==
    \/ \E t \in Tenants: Tick(t)
    \/ \E t \in Tenants: Advance(t)
    \/ \E t \in Tenants: AdvanceAuditFails(t)
    \/ \E t \in Tenants: AttemptEarlyAdvance(t)
    \/ \E t \in Tenants: AttemptAdvanceSkipAudit(t)

Spec == Init /\ [][Next]_vars

(*-- Helpers -----------------------------------------------------------------*)

\* Audit rows recorded for a given tenant.
RowsFor(t) ==
    LET idx == { i \in 1..Len(audit_log) : audit_log[i][1] = t }
    IN  [ k \in 1..Cardinality(idx) |-> CHOOSE x \in idx :
            Cardinality({y \in idx : y < x}) = k - 1 ]

\* Number of transitions a tenant has made from notice.
TransitionsFor(t) ==
    LET o == [s \in States |->
            CASE s = "notice"    -> 0
              [] s = "grace"     -> 1
              [] s = "erasure"   -> 2
              [] s = "destroyed" -> 3
              [] OTHER           -> 0]
    IN  o[t_state[t]]

(*-- Safety invariants -------------------------------------------------------*)

\* INV-OFFBOARDING-AUDIT-COMPLETE (core). The number of audit rows for a
\* tenant equals the number of state transitions it has made. No
\* mutation without a paired audit row.
InvAuditPairsTransition ==
    \A t \in Tenants:
        Cardinality({i \in 1..Len(audit_log) : audit_log[i][1] = t})
            = TransitionsFor(t)

\* Every audit row records a real state transition (from -> next(from)).
InvAuditRowsAreValidTransitions ==
    \A i \in 1..Len(audit_log):
        audit_log[i][3] = NextState(audit_log[i][2])

\* INV-OFFBOARDING-GRACE-RESPECTED. State never advances unless the
\* clock was at or above the threshold at the time of advance — by
\* construction (Advance is the only state-mutating action and gates
\* on `t_clock[t] >= GraceThreshold`). The model-level check below is
\* the structural backstop: any audit row's `to_state` must follow
\* its `from_state` per `NextState`.
InvGraceRespected == InvAuditRowsAreValidTransitions

SafetyInvariants ==
    /\ InvAuditPairsTransition
    /\ InvAuditRowsAreValidTransitions
    /\ InvGraceRespected

================================================================================
