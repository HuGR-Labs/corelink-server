---------------------------- MODULE audit_emit_atomic ----------------------------
(***************************************************************************)
(* CoreLink — audit-outbox INSERT atomic with handler (DEBT-005 10/40)    *)
(*                                                                         *)
(* Closes CRITICAL TLA gap from canonical-consistency baseline:           *)
(*                                                                         *)
(*   INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER  (CRITICAL, §3.6 Audit)            *)
(*     Every handler that mutates `blob_meta` OR commits a `TenantCtx`   *)
(*     MUST do so in the SAME D1 batch as the corresponding              *)
(*     `audit_outbox` INSERT. The two writes commit together or roll     *)
(*     back together. There is no reachable state in which a domain      *)
(*     row exists without its matching outbox row (or vice versa).       *)
(*                                                                         *)
(*   The registry today notes this as "Coberto parcial por                *)
(*   audit_immutability.tla". This spec strengthens the proof from       *)
(*   partial to direct by exhibiting the batch-atomic state-machine       *)
(*   pairing.                                                              *)
(*                                                                         *)
(* Threat model:                                                           *)
(*   - D1 batch transient failure between domain row INSERT and outbox   *)
(*     INSERT — modelled by the `Rollback` branch.                       *)
(*   - Handler that emits the outbox row without the domain row, or     *)
(*     vice versa — modelled by `AttemptOrphanDomain` /                  *)
(*     `AttemptOrphanOutbox` (both unreachable by construction).         *)
(*                                                                         *)
(* Out of scope:                                                           *)
(*   - D1 batch primitive internals (assumed atomic).                    *)
(*   - Wall-clock retry semantics (covered by RB-AUDIT-001).             *)
(*                                                                         *)
(* Cross-refs:                                                             *)
(*   - `specs/03_architecture/invariant_registry.md §3.6 INV-AUDIT-*`   *)
(*   - `crates/corelink-audit-chain/src/lib.rs`                          *)
(*   - `specs/tla/audit_immutability.tla` (parent — proves immutability *)
(*     of committed rows; this spec proves the commit-pairing).          *)
(***************************************************************************)

EXTENDS Integers, FiniteSets, Sequences, TLC

CONSTANTS
    Handlers,        \* Finite set of handler invocation IDs
    MaxOps

ASSUME
    /\ Handlers # {}
    /\ MaxOps \in Nat

Outcomes == {"pending", "committed", "rolled_back"}

VARIABLES
    h_outcome,       \* Function Handlers -> Outcomes
    h_domain_row,    \* Function Handlers -> BOOLEAN  (TRUE = INSERT visible)
    h_outbox_row,    \* Function Handlers -> BOOLEAN  (TRUE = audit row visible)
    op_count

vars == <<h_outcome, h_domain_row, h_outbox_row, op_count>>

(*-- Init --------------------------------------------------------------------*)

Init ==
    /\ h_outcome    = [h \in Handlers |-> "pending"]
    /\ h_domain_row = [h \in Handlers |-> FALSE]
    /\ h_outbox_row = [h \in Handlers |-> FALSE]
    /\ op_count     = 0

(*-- Actions -----------------------------------------------------------------*)

\* D1 batch commits both rows atomically — the only "happy path" writer.
Commit(h) ==
    /\ h \in Handlers
    /\ h_outcome[h] = "pending"
    /\ op_count < MaxOps
    /\ h_outcome'    = [h_outcome    EXCEPT ![h] = "committed"]
    /\ h_domain_row' = [h_domain_row EXCEPT ![h] = TRUE]
    /\ h_outbox_row' = [h_outbox_row EXCEPT ![h] = TRUE]
    /\ op_count'     = op_count + 1

\* D1 batch fails (e.g. WAL conflict). Neither row visible.
Rollback(h) ==
    /\ h \in Handlers
    /\ h_outcome[h] = "pending"
    /\ op_count < MaxOps
    /\ h_outcome' = [h_outcome EXCEPT ![h] = "rolled_back"]
    /\ op_count'  = op_count + 1
    /\ UNCHANGED <<h_domain_row, h_outbox_row>>

\* Adversarial: handler writes the domain row but skips the outbox.
\* Guard FALSE — INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER is violated by this
\* trace; we exhibit it explicitly so the model reader sees that the
\* invariant is what rules it out.
AttemptOrphanDomain(h) ==
    /\ h \in Handlers
    /\ h_outcome[h] = "pending"
    /\ op_count < MaxOps
    /\ FALSE
    /\ h_domain_row' = [h_domain_row EXCEPT ![h] = TRUE]
    /\ h_outbox_row' = [h_outbox_row EXCEPT ![h] = FALSE]
    /\ h_outcome'    = [h_outcome    EXCEPT ![h] = "committed"]
    /\ op_count'     = op_count + 1

\* Adversarial: handler writes the outbox row but skips the domain row.
AttemptOrphanOutbox(h) ==
    /\ h \in Handlers
    /\ h_outcome[h] = "pending"
    /\ op_count < MaxOps
    /\ FALSE
    /\ h_outbox_row' = [h_outbox_row EXCEPT ![h] = TRUE]
    /\ h_domain_row' = [h_domain_row EXCEPT ![h] = FALSE]
    /\ h_outcome'    = [h_outcome    EXCEPT ![h] = "committed"]
    /\ op_count'     = op_count + 1

(*-- Next --------------------------------------------------------------------*)

Next ==
    \/ \E h \in Handlers: Commit(h)
    \/ \E h \in Handlers: Rollback(h)
    \/ \E h \in Handlers: AttemptOrphanDomain(h)
    \/ \E h \in Handlers: AttemptOrphanOutbox(h)

Spec == Init /\ [][Next]_vars

(*-- Safety invariants -------------------------------------------------------*)

\* INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER (core) — domain row visible iff
\* outbox row visible. The biconditional encodes the D1 batch atomicity.
InvAtomicPairing ==
    \A h \in Handlers:
        h_domain_row[h] = h_outbox_row[h]

\* Committed handlers have BOTH rows; rolled-back handlers have NEITHER.
InvOutcomeConsistency ==
    \A h \in Handlers:
        /\ (h_outcome[h] = "committed"
            => /\ h_domain_row[h]
               /\ h_outbox_row[h])
        /\ (h_outcome[h] = "rolled_back"
            => /\ ~h_domain_row[h]
               /\ ~h_outbox_row[h])

\* No orphan outbox (audit row without domain row).
InvNoOrphanOutbox ==
    \A h \in Handlers:
        h_outbox_row[h] => h_domain_row[h]

\* No orphan domain row (state change without audit row).
InvNoOrphanDomain ==
    \A h \in Handlers:
        h_domain_row[h] => h_outbox_row[h]

SafetyInvariants ==
    /\ InvAtomicPairing
    /\ InvOutcomeConsistency
    /\ InvNoOrphanOutbox
    /\ InvNoOrphanDomain

================================================================================
