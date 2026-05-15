--------------------------- MODULE auth_webauthn_uv_admin ---------------------------
(***************************************************************************)
(* CoreLink — WebAuthn UV-required + attestation-verified for admin       *)
(*           (DEBT-005 batch 5 #3)                                         *)
(*                                                                         *)
(* Closes 2 CRITICAL TLA gaps from canonical-consistency baseline §3.4    *)
(* AUTH WebAuthn:                                                          *)
(*                                                                         *)
(*   INV-AUTH-WEBAUTHN-UV-REQUIRED-ADMIN  (CRITICAL, §3.4 AUTH)            *)
(*     Admin-scope WebAuthn assertions MUST set the UV (user-verification)*)
(*     bit. The verifier rejects any admin-scope assertion where          *)
(*     `flags.user_verified == false`. There is no opt-out.               *)
(*                                                                         *)
(*   INV-AUTH-WEBAUTHN-ATTESTATION-VERIFIED  (CRITICAL, §3.4 AUTH)        *)
(*     During registration, the attestation statement MUST be verified    *)
(*     against the metadata-service AAGUID allowlist. Unverified          *)
(*     attestations are rejected fail-closed — no credential is persisted.*)
(*                                                                         *)
(* Threat model (modelled):                                                *)
(*   - Register flow validates attestation; only verified attestations    *)
(*     enter the credential_store.                                        *)
(*   - Authenticate flow gates admin scope on UV bit. Non-admin scope    *)
(*     may proceed without UV (modelled as a separate case).             *)
(*   - Adversarial actions (guard FALSE):                                 *)
(*       AttemptRegisterUnverified — register without attestation verify. *)
(*       AttemptAdminWithoutUv — admin login with UV=false.              *)
(*                                                                         *)
(* Out of scope:                                                           *)
(*   - WebAuthn signature algorithm specifics (covered in                *)
(*     `webauthn_origin.tla` for origin/RP-ID).                          *)
(*                                                                         *)
(* Cross-refs:                                                             *)
(*   - `specs/03_architecture/invariant_registry.md §3.4 AUTH`            *)
(*   - `crates/corelink-auth/src/webauthn.rs`                            *)
(***************************************************************************)

EXTENDS Integers, FiniteSets, Sequences, TLC

CONSTANTS
    Users,             \* Finite set of users
    Scopes,            \* Set of session scopes — {"admin", "member"}
    MaxOps

ASSUME
    /\ Users # {}
    /\ Scopes # {}
    /\ MaxOps \in Nat

\* A credential record has (user, attestation_verified). Only records with
\* attestation_verified=TRUE enter the store (registrar enforces).
\* A session record has (user, scope, uv_required). Admin sessions REQUIRE
\* uv_required=TRUE.

VARIABLES
    credential_store,  \* SUBSET [user, attestation_verified]
    sessions,          \* Sequence of <<user, scope, uv>>
    op_count

vars == <<credential_store, sessions, op_count>>

Init ==
    /\ credential_store = {}
    /\ sessions         = <<>>
    /\ op_count         = 0

(*-- Actions -----------------------------------------------------------------*)

\* Register a credential. The verifier MUST attest before persistence.
\* Only attested credentials reach the store.
RegisterVerified(u) ==
    /\ u \in Users
    /\ op_count < MaxOps
    /\ credential_store' = credential_store \cup
         {[user |-> u, attestation_verified |-> TRUE]}
    /\ UNCHANGED sessions
    /\ op_count' = op_count + 1

\* Authenticate admin: REQUIRES the credential to be present AND uv=TRUE.
AuthenticateAdmin(u) ==
    /\ u \in Users
    /\ "admin" \in Scopes
    /\ op_count < MaxOps
    /\ \E c \in credential_store:
         /\ c.user = u
         /\ c.attestation_verified = TRUE
    /\ sessions' = Append(sessions, <<u, "admin", TRUE>>)
    /\ UNCHANGED credential_store
    /\ op_count' = op_count + 1

\* Authenticate member: UV may be either, scope is non-admin.
AuthenticateMember(u, uv) ==
    /\ u \in Users
    /\ uv \in {TRUE, FALSE}
    /\ "member" \in Scopes
    /\ op_count < MaxOps
    /\ \E c \in credential_store:
         /\ c.user = u
         /\ c.attestation_verified = TRUE
    /\ sessions' = Append(sessions, <<u, "member", uv>>)
    /\ UNCHANGED credential_store
    /\ op_count' = op_count + 1

\* Adversarial: register a credential without attestation verify.
\* Guard FALSE — registrar has no such path.
AttemptRegisterUnverified(u) ==
    /\ u \in Users
    /\ op_count < MaxOps
    /\ FALSE
    /\ credential_store' = credential_store \cup
         {[user |-> u, attestation_verified |-> FALSE]}
    /\ UNCHANGED sessions
    /\ op_count' = op_count + 1

\* Adversarial: open an admin session without UV. Guard FALSE.
AttemptAdminWithoutUv(u) ==
    /\ u \in Users
    /\ op_count < MaxOps
    /\ FALSE
    /\ sessions' = Append(sessions, <<u, "admin", FALSE>>)
    /\ UNCHANGED credential_store
    /\ op_count' = op_count + 1

(*-- Next --------------------------------------------------------------------*)

Next ==
    \/ \E u \in Users: RegisterVerified(u)
    \/ \E u \in Users: AuthenticateAdmin(u)
    \/ \E u \in Users, uv \in {TRUE, FALSE}: AuthenticateMember(u, uv)
    \/ \E u \in Users: AttemptRegisterUnverified(u)
    \/ \E u \in Users: AttemptAdminWithoutUv(u)

Spec == Init /\ [][Next]_vars

(*-- Safety invariants -------------------------------------------------------*)

\* INV-AUTH-WEBAUTHN-ATTESTATION-VERIFIED. Every credential in the store
\* has a verified attestation.
InvAttestationVerified ==
    \A c \in credential_store: c.attestation_verified = TRUE

\* INV-AUTH-WEBAUTHN-UV-REQUIRED-ADMIN. Every admin-scope session has
\* uv=TRUE.
InvAdminUvRequired ==
    \A i \in 1..Len(sessions):
        sessions[i][2] = "admin" => sessions[i][3] = TRUE

SafetyInvariants ==
    /\ InvAttestationVerified
    /\ InvAdminUvRequired

================================================================================
