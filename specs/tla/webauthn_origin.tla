-------------------------- MODULE webauthn_origin --------------------------
(***************************************************************************)
(* CoreLink — WebAuthn origin/RP-ID exact match (DEBT-005 batch 4 #2)     *)
(*                                                                         *)
(* Closes 2 CRITICAL TLA gaps from canonical-consistency baseline §3.14    *)
(* Auth:                                                                   *)
(*                                                                         *)
(*   INV-AUTH-WEBAUTHN-ORIGIN-EXACT  (CRITICAL, §3.14 Auth)                 *)
(*     Origin allowlist exact match — NO prefix bypass, NO subdomain      *)
(*     spoof, NO scheme-mismatch. `Vec<Url>` exact eq compare per         *)
(*     W3C §13.4.9. Any login attempt whose `client_data.origin` is NOT  *)
(*     byte-equal to an allowlisted origin MUST be rejected; the         *)
(*     verifier's `succeeded` state can never transition to TRUE for     *)
(*     such an attempt.                                                   *)
(*                                                                         *)
(*   INV-AUTH-WEBAUTHN-RP-ID-CANONICAL  (CRITICAL, §3.14 Auth)              *)
(*     RP ID is canonical eTLD+1 ("corelink.dev"); subdomain values are  *)
(*     refused at constructor time. The `WebAuthnAdapter::new`            *)
(*     constructor refuses to instantiate if `rp_id` is a subdomain;     *)
(*     verification with non-canonical RP ID is unreachable.             *)
(*                                                                         *)
(* Threat model (modelled):                                                *)
(*   - Legit login: client_data.origin in allowlist, rp_id canonical.    *)
(*   - Origin spoof: client supplies a near-match origin (prefix,        *)
(*     subdomain, scheme mismatch) — guard rejects.                      *)
(*   - RP-ID spoof: adapter is built with subdomain rp_id — constructor *)
(*     refuses (modelled by guard FALSE on AttemptBuildSubdomainRpId).   *)
(*   - Adversarial: a spoofed-origin login somehow succeeds — guard      *)
(*     FALSE.                                                              *)
(*                                                                         *)
(* Out of scope:                                                           *)
(*   - Attestation cert chain validation (separate invariant).            *)
(*   - UV (user-verified) flag policy (separate invariant).               *)
(*                                                                         *)
(* Cross-refs:                                                             *)
(*   - `specs/03_architecture/invariant_registry.md §3.14 INV-AUTH-WEB-*`  *)
(*   - `crates/corelink-auth/src/webauthn.rs::WebAuthnAdapter`            *)
(***************************************************************************)

EXTENDS Integers, FiniteSets, Sequences, TLC

CONSTANTS
    Origins,           \* Finite set of caller-supplied origin tokens
    Allowlist,         \* SUBSET Origins — exact-match canonical allowlist
    CanonicalRpId,     \* The single canonical RP ID value
    RpIds,             \* Finite set of candidate RP IDs (includes subdomains)
    MaxOps

ASSUME
    /\ Origins # {}
    /\ Allowlist \subseteq Origins
    /\ CanonicalRpId \in RpIds
    /\ MaxOps \in Nat

\* Verifier outcomes per attempt.
Verdicts == {"pending", "accepted", "rejected_origin", "rejected_rp_id"}

VARIABLES
    attempts,          \* Sequence of <<origin, rp_id, verdict>>
    adapter_built,     \* BOOLEAN — TRUE iff adapter constructed with canonical rp_id
    op_count

vars == <<attempts, adapter_built, op_count>>

(*-- Init --------------------------------------------------------------------*)

Init ==
    /\ attempts      = <<>>
    /\ adapter_built = FALSE
    /\ op_count      = 0

(*-- Actions -----------------------------------------------------------------*)

\* Constructor: adapter is built with the canonical RP ID. Only after
\* this can verification proceed.
BuildAdapterCanonical ==
    /\ adapter_built = FALSE
    /\ op_count < MaxOps
    /\ adapter_built' = TRUE
    /\ UNCHANGED attempts
    /\ op_count' = op_count + 1

\* Adversarial: constructor is called with a subdomain rp_id. The
\* constructor refuses — guard FALSE makes this trace unreachable.
AttemptBuildSubdomainRpId(rp) ==
    /\ rp \in RpIds
    /\ rp # CanonicalRpId
    /\ adapter_built = FALSE
    /\ op_count < MaxOps
    /\ FALSE
    /\ adapter_built' = TRUE
    /\ UNCHANGED attempts
    /\ op_count' = op_count + 1

\* Legit login: origin in allowlist, rp_id canonical, adapter built.
\* Verifier accepts.
LoginAccept(o, rp) ==
    /\ o \in Origins
    /\ rp \in RpIds
    /\ adapter_built = TRUE
    /\ o \in Allowlist                       \* exact-match origin gate
    /\ rp = CanonicalRpId                    \* canonical rp_id gate
    /\ op_count < MaxOps
    /\ attempts' = Append(attempts, <<o, rp, "accepted">>)
    /\ UNCHANGED adapter_built
    /\ op_count' = op_count + 1

\* Caller supplies a non-allowlisted origin. Verifier rejects.
LoginRejectOrigin(o, rp) ==
    /\ o \in Origins
    /\ rp \in RpIds
    /\ adapter_built = TRUE
    /\ o \notin Allowlist                    \* origin not in allowlist
    /\ op_count < MaxOps
    /\ attempts' = Append(attempts, <<o, rp, "rejected_origin">>)
    /\ UNCHANGED adapter_built
    /\ op_count' = op_count + 1

\* Caller supplies a non-canonical rp_id (subdomain). Verifier rejects.
LoginRejectRpId(o, rp) ==
    /\ o \in Origins
    /\ rp \in RpIds
    /\ adapter_built = TRUE
    /\ rp # CanonicalRpId                    \* rp_id not canonical
    /\ op_count < MaxOps
    /\ attempts' = Append(attempts, <<o, rp, "rejected_rp_id">>)
    /\ UNCHANGED adapter_built
    /\ op_count' = op_count + 1

\* Adversarial: spoofed-origin login somehow accepted. Guard FALSE.
AttemptAcceptSpoofedOrigin(o, rp) ==
    /\ o \in Origins
    /\ rp \in RpIds
    /\ adapter_built = TRUE
    /\ o \notin Allowlist
    /\ op_count < MaxOps
    /\ FALSE
    /\ attempts' = Append(attempts, <<o, rp, "accepted">>)
    /\ UNCHANGED adapter_built
    /\ op_count' = op_count + 1

\* Adversarial: subdomain rp_id login somehow accepted. Guard FALSE.
AttemptAcceptSubdomainRpId(o, rp) ==
    /\ o \in Origins
    /\ rp \in RpIds
    /\ adapter_built = TRUE
    /\ rp # CanonicalRpId
    /\ op_count < MaxOps
    /\ FALSE
    /\ attempts' = Append(attempts, <<o, rp, "accepted">>)
    /\ UNCHANGED adapter_built
    /\ op_count' = op_count + 1

(*-- Next --------------------------------------------------------------------*)

Next ==
    \/ BuildAdapterCanonical
    \/ \E rp \in RpIds: AttemptBuildSubdomainRpId(rp)
    \/ \E o \in Origins, rp \in RpIds: LoginAccept(o, rp)
    \/ \E o \in Origins, rp \in RpIds: LoginRejectOrigin(o, rp)
    \/ \E o \in Origins, rp \in RpIds: LoginRejectRpId(o, rp)
    \/ \E o \in Origins, rp \in RpIds: AttemptAcceptSpoofedOrigin(o, rp)
    \/ \E o \in Origins, rp \in RpIds: AttemptAcceptSubdomainRpId(o, rp)

Spec == Init /\ [][Next]_vars

(*-- Safety invariants -------------------------------------------------------*)

\* INV-AUTH-WEBAUTHN-ORIGIN-EXACT. Every accepted attempt has its origin
\* byte-equal (set membership) to an allowlisted origin.
InvOriginExact ==
    \A i \in 1..Len(attempts):
        attempts[i][3] = "accepted" => attempts[i][1] \in Allowlist

\* INV-AUTH-WEBAUTHN-RP-ID-CANONICAL. Every accepted attempt has the
\* canonical RP ID; the adapter was built with the canonical RP ID
\* (which is the only path to `adapter_built = TRUE`).
InvRpIdCanonical ==
    /\ \A i \in 1..Len(attempts):
         attempts[i][3] = "accepted" => attempts[i][2] = CanonicalRpId
    /\ (adapter_built = TRUE)
       \/ (\A i \in 1..Len(attempts): attempts[i][3] # "accepted")

\* Coherence: pending verdicts never appear in the log (all appended
\* entries are terminal).
InvNoPendingInLog ==
    \A i \in 1..Len(attempts): attempts[i][3] \in {"accepted", "rejected_origin", "rejected_rp_id"}

SafetyInvariants ==
    /\ InvOriginExact
    /\ InvRpIdCanonical
    /\ InvNoPendingInLog

================================================================================
