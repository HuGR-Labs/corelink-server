---------------------------- MODULE backup_restore_ephemeral ----------------------------
(***************************************************************************)
(* CoreLink — backup-verify ephemeral-restore invariant (DEBT-005 7/40)    *)
(*                                                                         *)
(* Closes CRITICAL TLA gap from canonical-consistency baseline:           *)
(*                                                                         *)
(*   INV-BACKUP-RESTORE-EPHEMERAL  (CRITICAL, §3.20 Backup)               *)
(*     Every successful `sample_restore` call MUST land in a namespace    *)
(*     tagged `ephemeral = true`. There is no reachable trace in which   *)
(*     the trait restores into a production-named namespace. The         *)
(*     ephemeral namespace MUST be torn down within the lifecycle        *)
(*     window — proved as a liveness property                            *)
(*     `EphemeralEventuallyTornDown`.                                    *)
(*                                                                         *)
(* Threat model:                                                           *)
(*   - Adversary supplies a NamespaceName equal to a production label    *)
(*     ("prod-iad", "prod-fra", "live-gru", ...). The model rejects this *)
(*     by construction at AttemptRestore.                                 *)
(*   - Adversary delays Teardown indefinitely → liveness fairness        *)
(*     forces Teardown to fire.                                           *)
(*                                                                         *)
(* Out of scope:                                                           *)
(*   - R2 bucket semantics (modelled as a set of namespace tags).         *)
(*   - Restored-data integrity (separate `cas_integrity.tla`).             *)
(*                                                                         *)
(* Cross-refs:                                                             *)
(*   - `specs/03_architecture/invariant_registry.md §3.20 INV-BACKUP-*`  *)
(*   - `crates/corelink-backup-verify/src/lib.rs`                        *)
(***************************************************************************)

EXTENDS Integers, FiniteSets, Sequences, TLC

CONSTANTS
    RestoreOps,         \* Finite set of restore-op IDs
    ProdNamespaces,     \* Finite set of forbidden production-label namespaces
    EphemeralNamespaces, \* Finite set of allowed ephemeral namespaces
    MaxOps              \* Bound on caller-driven steps

ASSUME
    /\ RestoreOps # {}
    /\ ProdNamespaces # {}
    /\ EphemeralNamespaces # {}
    /\ ProdNamespaces \intersect EphemeralNamespaces = {}   \* disjoint
    /\ MaxOps \in Nat

\* Total state for a restore op.
States == {"idle", "running", "torn_down", "rejected"}

VARIABLES
    op_namespace,    \* Function RestoreOps -> namespace ID \cup {"NONE"}
    op_state,        \* Function RestoreOps -> States
    op_ephemeral,    \* Function RestoreOps -> BOOLEAN (tag at restore time)
    op_count

vars == <<op_namespace, op_state, op_ephemeral, op_count>>

(*-- Init --------------------------------------------------------------------*)

Init ==
    /\ op_namespace = [o \in RestoreOps |-> "NONE"]
    /\ op_state     = [o \in RestoreOps |-> "idle"]
    /\ op_ephemeral = [o \in RestoreOps |-> FALSE]
    /\ op_count     = 0

(*-- Actions -----------------------------------------------------------------*)

\* Honest restore — caller asks the trait to restore into an ephemeral
\* namespace. The trait stamps ephemeral=TRUE. Allowed.
RestoreEphemeral(o, n) ==
    /\ o \in RestoreOps
    /\ n \in EphemeralNamespaces
    /\ op_state[o] = "idle"
    /\ op_count < MaxOps
    /\ op_namespace' = [op_namespace EXCEPT ![o] = n]
    /\ op_state'     = [op_state     EXCEPT ![o] = "running"]
    /\ op_ephemeral' = [op_ephemeral EXCEPT ![o] = TRUE]
    /\ op_count'     = op_count + 1

\* Adversarial restore — caller asks for restore into a production-label
\* namespace. The trait MUST reject. Modelled by the rejection branch:
\* state goes to "rejected" without writing op_namespace.
AttemptRestoreToProd(o, n) ==
    /\ o \in RestoreOps
    /\ n \in ProdNamespaces
    /\ op_state[o] = "idle"
    /\ op_count < MaxOps
    /\ op_state'     = [op_state EXCEPT ![o] = "rejected"]
    /\ op_count'     = op_count + 1
    /\ UNCHANGED <<op_namespace, op_ephemeral>>

\* Eventual teardown — the ephemeral namespace is wiped after the
\* verify window. Required by liveness fairness for EphemeralEventually-
\* TornDown.
Teardown(o) ==
    /\ o \in RestoreOps
    /\ op_state[o] = "running"
    /\ op_state' = [op_state EXCEPT ![o] = "torn_down"]
    /\ UNCHANGED <<op_namespace, op_ephemeral, op_count>>

(*-- Next --------------------------------------------------------------------*)

Next ==
    \/ \E o \in RestoreOps, n \in EphemeralNamespaces: RestoreEphemeral(o, n)
    \/ \E o \in RestoreOps, n \in ProdNamespaces:       AttemptRestoreToProd(o, n)
    \/ \E o \in RestoreOps:                              Teardown(o)

Spec ==
    /\ Init
    /\ [][Next]_vars
    /\ \A o \in RestoreOps: WF_vars(Teardown(o))

(*-- Safety invariants -------------------------------------------------------*)

\* INV-BACKUP-RESTORE-EPHEMERAL — every restore that reached the
\* "running" stage is tagged ephemeral = TRUE AND landed in an
\* EphemeralNamespace. By construction RestoreEphemeral is the only
\* writer of state="running" AND its guard restricts namespace to
\* EphemeralNamespaces; this codifies the structural property.
InvRunningIsEphemeral ==
    \A o \in RestoreOps:
        op_state[o] = "running"
        => /\ op_ephemeral[o] = TRUE
           /\ op_namespace[o] \in EphemeralNamespaces

\* No restore that succeeded ever landed in ProdNamespaces.
InvNoProdRestore ==
    \A o \in RestoreOps:
        op_namespace[o] \in ProdNamespaces => FALSE

\* Adversarial-rejection path — every "rejected" op retains namespace
\* "NONE" (NEVER wrote to a prod namespace).
InvRejectedHasNoNamespace ==
    \A o \in RestoreOps:
        op_state[o] = "rejected" => op_namespace[o] = "NONE"

\* Torn-down op preserves the original ephemeral stamp (audit trail).
InvTornDownPreservesTag ==
    \A o \in RestoreOps:
        op_state[o] = "torn_down" => op_ephemeral[o] = TRUE

SafetyInvariants ==
    /\ InvRunningIsEphemeral
    /\ InvNoProdRestore
    /\ InvRejectedHasNoNamespace
    /\ InvTornDownPreservesTag

(*-- Liveness ----------------------------------------------------------------*)

\* Every running restore eventually torn down (under WF_Teardown).
EphemeralEventuallyTornDown ==
    \A o \in RestoreOps:
        (op_state[o] = "running") ~> (op_state[o] = "torn_down")

================================================================================
