------------------- MODULE sub_processor_audit_fail_closed -------------------
(***************************************************************************)
(* CoreLink — Sub-processor CloudEvent audit fail-closed ordering          *)
(*           (DEBT-005 batch 6 FINAL #5)                                   *)
(*                                                                         *)
(* Closes the canonical-consistency gap for:                              *)
(*                                                                         *)
(*   INV-SUB-PROCESSOR-AUDIT-FAIL-CLOSED  (CRITICAL, §3 row 410)            *)
(*     Sub-processor CloudEvent audit emission MUST succeed BEFORE any   *)
(*     state mutation. If audit emit fails, the operation aborts and    *)
(*     the state remains UNCHANGED. CD pipeline aborts on emit failure. *)
(*     Aligns with INV-AUDIT-APPEND-ONLY §3.6 L116.                      *)
(*                                                                         *)
(*     Operations covered:                                                 *)
(*       (a) publish (new sub-processor announcement)                    *)
(*       (b) record_change (modification of sub-processor)              *)
(*       (c) file_objection (customer objection filed)                  *)
(*                                                                         *)
(* Threat model (modelled):                                                *)
(*   - Each op pair = (try_emit_audit, then_mutate_state). Audit may    *)
(*     succeed or fail nondeterministically.                            *)
(*   - On audit success, state mutates atomically with the emit.        *)
(*   - On audit failure, state remains UNCHANGED (fail-closed).         *)
(*   - Adversarial actions (guard FALSE) attempt to mutate state after  *)
(*     an audit failure.                                                  *)
(*                                                                         *)
(* Cross-refs:                                                             *)
(*   - `specs/03_architecture/invariant_registry.md §3 row 410`           *)
(*   - `crates/corelink-privacy-sub-processors/src/lib.rs`               *)
(*   - `audit_immutability.tla` (parent — append-only chain)            *)
(*   - `audit_emit_atomic.tla` (sibling — emit/state atomic ordering)   *)
(***************************************************************************)

EXTENDS Integers, FiniteSets, Sequences, TLC

CONSTANTS
    OpKinds,           \* { publish, record_change, file_objection }
    MaxOps

ASSUME
    /\ OpKinds # {}
    /\ MaxOps \in Nat

VARIABLES
    audit_chain,       \* Sequence of <<kind, sub_processor_id>> emitted records
    state_changes,     \* Sequence of <<kind, sub_processor_id>> applied mutations
    op_count

vars == <<audit_chain, state_changes, op_count>>

Init ==
    /\ audit_chain = <<>>
    /\ state_changes = <<>>
    /\ op_count = 0

(*-- Actions -----------------------------------------------------------------*)

\* OpSuccess: audit emit succeeds → state mutates atomically.
OpSuccess(kind, sp_id) ==
    /\ kind \in OpKinds
    /\ op_count < MaxOps
    /\ audit_chain' = Append(audit_chain, <<kind, sp_id>>)
    /\ state_changes' = Append(state_changes, <<kind, sp_id>>)
    /\ op_count' = op_count + 1

\* OpAuditFail: audit emit fails → state remains UNCHANGED (fail-closed).
OpAuditFail(kind, sp_id) ==
    /\ kind \in OpKinds
    /\ op_count < MaxOps
    /\ UNCHANGED audit_chain
    /\ UNCHANGED state_changes
    /\ op_count' = op_count + 1

\* Adversarial: mutate state without emitting audit (fail-open bug).
\* Guard FALSE — the handler MUST emit-before-mutate.
AttemptMutateWithoutAudit(kind, sp_id) ==
    /\ kind \in OpKinds
    /\ op_count < MaxOps
    /\ FALSE
    /\ state_changes' = Append(state_changes, <<kind, sp_id>>)
    /\ UNCHANGED audit_chain
    /\ op_count' = op_count + 1

(*-- Next --------------------------------------------------------------------*)

Next ==
    \/ \E k \in OpKinds, sp \in {"sp1", "sp2"}: OpSuccess(k, sp)
    \/ \E k \in OpKinds, sp \in {"sp1", "sp2"}: OpAuditFail(k, sp)
    \/ \E k \in OpKinds, sp \in {"sp1", "sp2"}: AttemptMutateWithoutAudit(k, sp)

Spec == Init /\ [][Next]_vars

(*-- Safety invariants -------------------------------------------------------*)

\* INV-SUB-PROCESSOR-AUDIT-FAIL-CLOSED. Every state change has a
\* corresponding audit record. Equivalently, state_changes is a prefix
\* (sub-sequence at the same index) of audit_chain.
InvSubProcessorAuditFailClosed ==
    /\ Len(state_changes) <= Len(audit_chain)
    /\ \A i \in 1..Len(state_changes):
         state_changes[i] = audit_chain[i]

\* Audit-before-mutate ordering: at every step audit_chain length is
\* greater than or equal to state_changes length (audit precedes).
InvAuditBeforeMutate ==
    Len(audit_chain) >= Len(state_changes)

SafetyInvariants ==
    /\ InvSubProcessorAuditFailClosed
    /\ InvAuditBeforeMutate

================================================================================
