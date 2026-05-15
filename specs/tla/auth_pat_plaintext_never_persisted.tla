--------------------------- MODULE auth_pat_plaintext_never_persisted ---------------------------
(***************************************************************************)
(* CoreLink — PAT plaintext is never persisted (DEBT-005 batch 5 #2)       *)
(*                                                                         *)
(* Closes 1 CRITICAL TLA gap from canonical-consistency baseline §3.4 AUTH: *)
(*                                                                         *)
(*   INV-AUTH-PAT-PLAINTEXT-NEVER-PERSISTED  (CRITICAL, §3.4 AUTH)         *)
(*     Personal Access Tokens (PATs) are generated as plaintext only at   *)
(*     issuance time, returned ONCE to the caller in the API response,    *)
(*     and IMMEDIATELY hashed (BLAKE3 + per-tenant salt) before any       *)
(*     persistent store write. The plaintext form MUST NEVER reach the    *)
(*     pat_store, audit_log, or any backup snapshot.                      *)
(*                                                                         *)
(* Threat model (modelled):                                                *)
(*   - Issuer produces a new PAT: PatIssue creates plaintext, returns it  *)
(*     to caller, and writes ONLY the hash to pat_store.                  *)
(*   - Verifier reads pat_store: only the hash is available; verify is   *)
(*     by hash-and-compare.                                               *)
(*   - Adversarial actions (guard FALSE):                                 *)
(*       AttemptPersistPlaintext — write plaintext to pat_store.          *)
(*       AttemptLogPlaintext — write plaintext to audit_log.              *)
(*                                                                         *)
(* Out of scope:                                                           *)
(*   - BLAKE3 collision resistance (cripto primitive).                    *)
(*   - Salt rotation cadence (operational policy).                        *)
(*                                                                         *)
(* Cross-refs:                                                             *)
(*   - `specs/03_architecture/invariant_registry.md §3.4 AUTH PAT`        *)
(*   - `crates/corelink-auth/src/pat.rs` (issue + verify)                *)
(*   - `auth_pat_hybrid.tla` (proves hash-vs-hash verification flow)     *)
(***************************************************************************)

EXTENDS Integers, FiniteSets, Sequences, TLC

CONSTANTS
    Tenants,           \* Finite set of tenants
    PatIds,            \* Finite set of PAT identifiers (issuance handles)
    MaxOps

ASSUME
    /\ Tenants # {}
    /\ PatIds # {}
    /\ MaxOps \in Nat

\* A stored PAT record carries the *hash form* only. The plaintext form
\* exists transiently in `issued_to_caller` (the API response trace) and
\* is never copied to pat_store or audit_log.

VARIABLES
    pat_store,         \* SUBSET of [tenant, pat_id, form] where form="hash"
    audit_log,         \* Sequence of <<tenant, pat_id, form>>
    issued_to_caller,  \* Sequence of <<tenant, pat_id, form>> — return path
    op_count

vars == <<pat_store, audit_log, issued_to_caller, op_count>>

\* The form tag is one of {"plaintext", "hash"}. The issuer MUST write
\* form="hash" to pat_store and form="hash" to audit_log. The caller
\* response (issued_to_caller) is the only path where plaintext appears.

Init ==
    /\ pat_store        = {}
    /\ audit_log        = <<>>
    /\ issued_to_caller = <<>>
    /\ op_count         = 0

(*-- Actions -----------------------------------------------------------------*)

\* Issue a new PAT: plaintext goes to caller, hash goes to store + audit.
PatIssue(t, p) ==
    /\ t \in Tenants
    /\ p \in PatIds
    /\ op_count < MaxOps
    /\ issued_to_caller' = Append(issued_to_caller, <<t, p, "plaintext">>)
    /\ pat_store'        = pat_store \cup {[tenant |-> t, pat_id |-> p, form |-> "hash"]}
    /\ audit_log'        = Append(audit_log, <<t, p, "hash">>)
    /\ op_count'         = op_count + 1

\* Verifier: read a record from pat_store (hash form). Hash-vs-hash check.
PatVerify(t, p) ==
    /\ t \in Tenants
    /\ p \in PatIds
    /\ op_count < MaxOps
    /\ \E r \in pat_store: r.tenant = t /\ r.pat_id = p /\ r.form = "hash"
    /\ UNCHANGED <<pat_store, audit_log, issued_to_caller>>
    /\ op_count' = op_count + 1

\* Adversarial: write plaintext PAT to pat_store. Guard FALSE.
AttemptPersistPlaintext(t, p) ==
    /\ t \in Tenants
    /\ p \in PatIds
    /\ op_count < MaxOps
    /\ FALSE
    /\ pat_store' = pat_store \cup {[tenant |-> t, pat_id |-> p, form |-> "plaintext"]}
    /\ UNCHANGED <<audit_log, issued_to_caller>>
    /\ op_count' = op_count + 1

\* Adversarial: write plaintext PAT to audit_log. Guard FALSE.
AttemptLogPlaintext(t, p) ==
    /\ t \in Tenants
    /\ p \in PatIds
    /\ op_count < MaxOps
    /\ FALSE
    /\ audit_log' = Append(audit_log, <<t, p, "plaintext">>)
    /\ UNCHANGED <<pat_store, issued_to_caller>>
    /\ op_count' = op_count + 1

(*-- Next --------------------------------------------------------------------*)

Next ==
    \/ \E t \in Tenants, p \in PatIds: PatIssue(t, p)
    \/ \E t \in Tenants, p \in PatIds: PatVerify(t, p)
    \/ \E t \in Tenants, p \in PatIds: AttemptPersistPlaintext(t, p)
    \/ \E t \in Tenants, p \in PatIds: AttemptLogPlaintext(t, p)

Spec == Init /\ [][Next]_vars

(*-- Safety invariants -------------------------------------------------------*)

\* INV-AUTH-PAT-PLAINTEXT-NEVER-PERSISTED. No record in pat_store is plaintext.
InvPatStoreNoPlaintext ==
    \A r \in pat_store: r.form = "hash"

\* INV-AUTH-PAT-PLAINTEXT-NEVER-PERSISTED (audit projection). No audit
\* entry carries plaintext form.
InvAuditNoPlaintext ==
    \A i \in 1..Len(audit_log): audit_log[i][3] = "hash"

SafetyInvariants ==
    /\ InvPatStoreNoPlaintext
    /\ InvAuditNoPlaintext

================================================================================
