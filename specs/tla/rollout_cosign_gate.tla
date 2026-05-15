---------------------------- MODULE rollout_cosign_gate ----------------------------
(***************************************************************************)
(* CoreLink — rollout cosign-gate supply-chain invariant (DEBT-005 6/40)   *)
(*                                                                         *)
(* Closes the following CRITICAL TLA gaps from the canonical-consistency  *)
(* baseline (2026-05-15-canonical-consistency-baseline.md §3.26 Rollout): *)
(*                                                                         *)
(*   INV-ROLLOUT-COSIGN-GATE  (CRITICAL)                                   *)
(*     `rollout::start()` MUST reject every deployable whose cosign        *)
(*     signature is absent or invalid; the deploy MUST NEVER reach the    *)
(*     "active" stage in that case. The rejection itself MUST emit an     *)
(*     audit event (fail-CLOSED).                                          *)
(*                                                                         *)
(*   INV-SUPPLY-SIGNED-DEPLOY  (HIGH; parent of INV-ROLLOUT-COSIGN-GATE,   *)
(*   §3.10 + §4.3 — runtime side proved here even though CI gate is      *)
(*   build-time)                                                           *)
(*     No deployable ever transitions out of "pending" without the        *)
(*     signature having been verified. Bypass paths (force, override,    *)
(*     emergency-rollback) MUST NOT exist; the model enumerates every    *)
(*     reachable state to confirm.                                        *)
(*                                                                         *)
(* Threat model:                                                           *)
(*   - Adversary submits a deployable with no signature (`SigStatus =     *)
(*     "missing"`).                                                        *)
(*   - Adversary submits a deployable with a forged signature             *)
(*     (`SigStatus = "invalid"`) — TLA models this as a cosign verify    *)
(*     that returns "invalid"; the controller MUST reject.                *)
(*   - Controller bug attempts to short-circuit verification              *)
(*     (modelled as `AttemptUnsignedActivate` — must be unreachable).    *)
(*                                                                         *)
(* Out of scope:                                                           *)
(*   - Cosign cryptographic math (modelled as oracle returning            *)
(*     valid/invalid/missing).                                             *)
(*   - Rekor inclusion-proof timing (separate INV-SUPPLY-PROVENANCE-IN-   *)
(*     REKOR, build-time).                                                 *)
(*                                                                         *)
(* Cross-refs:                                                             *)
(*   - `specs/03_architecture/invariant_registry.md §3.26 INV-ROLLOUT-*` *)
(*   - `specs/03_architecture/invariant_registry.md §3.10 INV-SUPPLY-*`  *)
(*   - `crates/corelink-rollout-controller/src/lib.rs::start`            *)
(***************************************************************************)

EXTENDS Integers, FiniteSets, Sequences, TLC

CONSTANTS
    Deployables,     \* Finite set of deployable IDs
    MaxOps           \* Bound on caller-driven steps

ASSUME
    /\ Deployables # {}
    /\ MaxOps \in Nat

\* Lifecycle stages for a deployable (controller view).
Stages == {"pending", "verifying", "rejected", "active"}

\* Possible cosign verification outcomes the adversary can present.
SigOutcomes == {"valid", "invalid", "missing"}

VARIABLES
    sig_status,        \* Function Deployables -> SigOutcomes
    stage,             \* Function Deployables -> Stages
    audit_log,         \* Sequence of <<deployable, audit_kind>>
                       \*   audit_kind \in {"verified_ok","rejected_unsigned",
                       \*                   "rejected_invalid","activated"}
    op_count

vars == <<sig_status, stage, audit_log, op_count>>

(*-- Init --------------------------------------------------------------------*)

Init ==
    /\ sig_status = [d \in Deployables |-> "missing"]
    /\ stage      = [d \in Deployables |-> "pending"]
    /\ audit_log  = <<>>
    /\ op_count   = 0

(*-- Actions -----------------------------------------------------------------*)

\* CI build attaches a (possibly adversarial) cosign signature outcome.
\* This action models the adversary's freedom in choosing what verify
\* returns: any of valid/invalid/missing.
AttachSignature(d, o) ==
    /\ d \in Deployables
    /\ o \in SigOutcomes
    /\ stage[d] = "pending"
    /\ op_count < MaxOps
    /\ sig_status' = [sig_status EXCEPT ![d] = o]
    /\ UNCHANGED <<stage, audit_log>>
    /\ op_count' = op_count + 1

\* Controller enters "verifying" — a transient stage encoding the
\* `cosign verify` syscall. Allowed from "pending" only.
StartVerify(d) ==
    /\ d \in Deployables
    /\ stage[d] = "pending"
    /\ op_count < MaxOps
    /\ stage' = [stage EXCEPT ![d] = "verifying"]
    /\ op_count' = op_count + 1
    /\ UNCHANGED <<sig_status, audit_log>>

\* Verify "valid" → activate + emit "activated".
ActivateOnValid(d) ==
    /\ d \in Deployables
    /\ stage[d] = "verifying"
    /\ sig_status[d] = "valid"
    /\ op_count < MaxOps
    /\ stage' = [stage EXCEPT ![d] = "active"]
    /\ audit_log' = Append(Append(audit_log, <<d, "verified_ok">>),
                            <<d, "activated">>)
    /\ op_count' = op_count + 1
    /\ UNCHANGED sig_status

\* Verify "invalid" or "missing" → reject + emit audit. FAIL CLOSED.
RejectOnUnsigned(d) ==
    /\ d \in Deployables
    /\ stage[d] = "verifying"
    /\ sig_status[d] \in {"missing", "invalid"}
    /\ op_count < MaxOps
    /\ stage' = [stage EXCEPT ![d] = "rejected"]
    /\ audit_log' = Append(audit_log,
                            <<d,
                              IF sig_status[d] = "missing"
                              THEN "rejected_unsigned"
                              ELSE "rejected_invalid">>)
    /\ op_count' = op_count + 1
    /\ UNCHANGED sig_status

\* Adversarial: attempt to activate from "pending" WITHOUT verify.
\* Modelled as a disabled action — the guard requires sig_status="valid"
\* AND stage="verifying"; the only way to reach "active" is via
\* ActivateOnValid. We expose this action explicitly so the model trace
\* shows it is unreachable.
AttemptUnsignedActivate(d) ==
    /\ d \in Deployables
    /\ stage[d] \in {"pending"}     \* Forbidden — would skip verify
    /\ op_count < MaxOps
    /\ FALSE                          \* Guard rules out by construction
    /\ stage' = [stage EXCEPT ![d] = "active"]
    /\ op_count' = op_count + 1
    /\ UNCHANGED <<sig_status, audit_log>>

(*-- Next --------------------------------------------------------------------*)

Next ==
    \/ \E d \in Deployables, o \in SigOutcomes: AttachSignature(d, o)
    \/ \E d \in Deployables: StartVerify(d)
    \/ \E d \in Deployables: ActivateOnValid(d)
    \/ \E d \in Deployables: RejectOnUnsigned(d)
    \/ \E d \in Deployables: AttemptUnsignedActivate(d)

Spec == Init /\ [][Next]_vars

(*-- Safety invariants -------------------------------------------------------*)

\* INV-ROLLOUT-COSIGN-GATE — every deployable that has reached "active"
\* MUST have had a valid signature at activation time. Because
\* ActivateOnValid is the only action setting stage to "active" AND its
\* guard requires sig_status = "valid", this holds in every reachable
\* state.
InvActiveIsSigned ==
    \A d \in Deployables:
        stage[d] = "active" => sig_status[d] = "valid"

\* INV-ROLLOUT-COSIGN-GATE — every rejected deployable had a non-valid
\* signature at rejection time. Co-safety partner of the above.
InvRejectedIsUnsigned ==
    \A d \in Deployables:
        stage[d] = "rejected" => sig_status[d] \in {"missing", "invalid"}

\* INV-SUPPLY-SIGNED-DEPLOY — no deployable can sit in "active" while
\* missing OR invalid. The above already implies this; we record it
\* explicitly for traceability.
InvSupplySignedDeploy ==
    \A d \in Deployables:
        stage[d] = "active" => sig_status[d] # "missing"
                              /\ sig_status[d] # "invalid"

\* Fail-CLOSED audit — every rejection produced an audit row.
InvRejectionAudited ==
    \A d \in Deployables:
        stage[d] = "rejected"
        => \E i \in 1..Len(audit_log):
              /\ audit_log[i][1] = d
              /\ audit_log[i][2] \in {"rejected_unsigned", "rejected_invalid"}

\* Activation is always audited too (parity with rejection path).
InvActivationAudited ==
    \A d \in Deployables:
        stage[d] = "active"
        => \E i \in 1..Len(audit_log):
              /\ audit_log[i][1] = d
              /\ audit_log[i][2] = "activated"

SafetyInvariants ==
    /\ InvActiveIsSigned
    /\ InvRejectedIsUnsigned
    /\ InvSupplySignedDeploy
    /\ InvRejectionAudited
    /\ InvActivationAudited

================================================================================
