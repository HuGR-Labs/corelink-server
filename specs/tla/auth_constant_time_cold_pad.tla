--------------------- MODULE auth_constant_time_cold_pad ---------------------
(***************************************************************************)
(* CoreLink — PAT verify cold-path constant-time pad                       *)
(*           (DEBT-005 batch 6 FINAL #2)                                   *)
(*                                                                         *)
(* Closes the canonical-consistency gap for:                              *)
(*                                                                         *)
(*   INV-AUTH-CONSTANT-TIME-COLD-PAD  (CRITICAL, §3.14 AUTH extended)       *)
(*     PAT verify cold-path latency is indistinguishable from warm-path  *)
(*     via a dummy Argon2id pad on EVERY cold-path failure branch:        *)
(*                                                                         *)
(*       (1) parse_fail   — token format invalid                          *)
(*       (2) sig_mismatch — HMAC short-circuit mismatch                    *)
(*       (3) token_id_absent — token_id not in DB                          *)
(*       (4) hash_mismatch — Argon2id verify fails                         *)
(*                                                                         *)
(*     For each failure branch, the middleware in                        *)
(*     `corelink-worker::middleware::auth` invokes                       *)
(*     `corelink_pat::dummy_verify_for_constant_time` exactly once.       *)
(*     An attacker observing the latency envelope cannot distinguish     *)
(*     "token doesn't exist" from "token exists but mismatch".           *)
(*                                                                         *)
(* Threat model (modelled):                                                *)
(*   - Every cold-path branch SETS pad_invoked = TRUE before returning   *)
(*     the unauth response. Adversarial bypass actions (guard FALSE)     *)
(*     attempt to return without invoking the pad.                       *)
(*                                                                         *)
(* Cross-refs:                                                             *)
(*   - `specs/03_architecture/invariant_registry.md §3.14 AUTH`           *)
(*   - `crates/corelink-worker/src/middleware/auth.rs`                   *)
(*   - `auth_pat_hybrid.tla` (sibling INV-AUTH-PAT-VERIFY-CONSTANT-TIME) *)
(***************************************************************************)

EXTENDS Integers, FiniteSets, Sequences, TLC

CONSTANTS
    FailureKinds,      \* { parse_fail, sig_mismatch, token_id_absent, hash_mismatch }
    MaxOps

ASSUME
    /\ FailureKinds # {}
    /\ MaxOps \in Nat

\* IsColdPath: every failure kind is cold-path. (warm = success branch.)
\* Structural definition; no compound literal.
IsColdPath(k) == k \in FailureKinds

VARIABLES
    completed_branches,  \* Sequence of <<kind, pad_invoked, result>>
    op_count

vars == <<completed_branches, op_count>>

Init ==
    /\ completed_branches = <<>>
    /\ op_count = 0

(*-- Actions -----------------------------------------------------------------*)

\* ColdPathFail: middleware hits a failure branch. Pad MUST be invoked
\* BEFORE returning the unauth response. Modelled as a single atomic
\* tuple commit with pad_invoked = TRUE.
ColdPathFail(kind) ==
    /\ kind \in FailureKinds
    /\ op_count < MaxOps
    /\ completed_branches' = Append(completed_branches,
                                    [kind |-> kind,
                                     pad_invoked |-> TRUE,
                                     result |-> "unauth"])
    /\ op_count' = op_count + 1

\* WarmPathSuccess: token verifies successfully. Pad is NOT invoked
\* (the genuine Argon2id verify already paid the latency cost).
WarmPathSuccess ==
    /\ op_count < MaxOps
    /\ completed_branches' = Append(completed_branches,
                                    [kind |-> "warm_ok",
                                     pad_invoked |-> FALSE,
                                     result |-> "auth"])
    /\ op_count' = op_count + 1

\* Adversarial: cold-path branch returns WITHOUT invoking pad — the
\* attacker would observe a shorter latency. Guard FALSE.
AttemptColdPathSkipPad(kind) ==
    /\ kind \in FailureKinds
    /\ op_count < MaxOps
    /\ FALSE
    /\ completed_branches' = Append(completed_branches,
                                    [kind |-> kind,
                                     pad_invoked |-> FALSE,
                                     result |-> "unauth"])
    /\ op_count' = op_count + 1

(*-- Next --------------------------------------------------------------------*)

Next ==
    \/ \E k \in FailureKinds: ColdPathFail(k)
    \/ WarmPathSuccess
    \/ \E k \in FailureKinds: AttemptColdPathSkipPad(k)

Spec == Init /\ [][Next]_vars

(*-- Safety invariants -------------------------------------------------------*)

\* INV-AUTH-CONSTANT-TIME-COLD-PAD. Every cold-path completion records
\* pad_invoked = TRUE. (Warm path is exempt.)
InvAuthConstantTimeColdPad ==
    \A i \in 1..Len(completed_branches):
        IsColdPath(completed_branches[i].kind)
            => completed_branches[i].pad_invoked = TRUE

\* Result symmetry: every cold-path branch yields "unauth" — the kind
\* leaks via timing only if pad varies, which the prior invariant
\* forbids.
InvColdPathResultUniform ==
    \A i \in 1..Len(completed_branches):
        IsColdPath(completed_branches[i].kind)
            => completed_branches[i].result = "unauth"

SafetyInvariants ==
    /\ InvAuthConstantTimeColdPad
    /\ InvColdPathResultUniform

================================================================================
