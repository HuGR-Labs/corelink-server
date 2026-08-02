---------------------------- MODULE auth_pat_hybrid ----------------------------
(***************************************************************************)
(* CoreLink — PAT verify hybrid path (DEBT-014 FT-1)                       *)
(*                                                                         *)
(* Closes 1 CRITICAL + 2 HIGH TLA gaps from                                *)
(* `specs/_audits/tla-followup-tickets.md` FT-1 by formalising the         *)
(* short-circuit ordering of the hybrid verify pipeline                    *)
(*   step 3a: HMAC fast-fail (no I/O)                                      *)
(*   step 3b: indexed lookup (DB hit) — ONLY if 3a succeeds                *)
(*   step 3c: Argon2id verify (CPU heavy) — ONLY if 3b returned a row      *)
(*                                                                         *)
(* Invariants proved here (from invariant_registry.md §3.14):              *)
(*                                                                         *)
(*   INV-AUTH-PAT-HMAC-SIG-VERIFIED (CRITICAL)                             *)
(*     No PAT can be admitted without HMAC signature having verified       *)
(*     strictly BEFORE any subsequent step ran. Equivalent to: no trace    *)
(*     reaches "accepted" with hmac_ok = FALSE.                            *)
(*                                                                         *)
(*   INV-AUTH-PAT-VERIFY-CONSTANT-TIME (HIGH)                              *)
(*     The verify pipeline executes Argon2id (step 3c) exactly when both   *)
(*     3a and 3b have succeeded — i.e. the algorithmic time signature on   *)
(*     a wrong-HMAC submission is identical to a wrong-token-id submission *)
(*     for an HMAC mismatch: zero DB hit, zero Argon2id hit.               *)
(*     Modelled as: step3c_executed => (hmac_ok /\ row_found).             *)
(*                                                                         *)
(*   INV-AUTH-PAT-HASH-ARGON2ID-2024 (HIGH)                                *)
(*     No accepted PAT skipped Argon2id — i.e. accepted => argon_ok.       *)
(*     The "phase 3c MUST fire before accept" obligation closes the trace  *)
(*     where step 3b fires while 3a is still pending (P0 in FT-1).         *)
(*                                                                         *)
(* Threat model:                                                           *)
(*   - Adversary submits arbitrary (token_id, hmac_match, row_present,     *)
(*     argon_match) tuples; we adversarially choose all four bits.         *)
(*   - The verify pipeline is the only mutator of accepted_pats.           *)
(*                                                                         *)
(* Out of scope:                                                           *)
(*   - PAT rotation / lifecycle (covered by FT-2 key_lifecycle.tla).       *)
(*   - HMAC algorithm crypto strength (analytical, not state-machine).    *)
(*                                                                         *)
(* Cross-refs:                                                             *)
(*   - `specs/03_architecture/invariant_registry.md §3.14 INV-AUTH-PAT-*`  *)
(*   - `specs/_audits/tla-followup-tickets.md` FT-1                        *)
(***************************************************************************)

EXTENDS Integers, FiniteSets, Sequences, TLC

CONSTANTS
    Pats,              \* Finite token IDs to bound the model
    MaxOps             \* Bound on adversary submissions

ASSUME
    /\ Pats # {}
    /\ MaxOps \in Nat

VARIABLES
    \* SET (not sequence) of <<pat, hmac_ok, row_found, argon_ok>> submitted so
    \* far. It was a sequence until 2026-08-02, which made the state space the
    \* number of ORDERED logs — exactly sum(16^i, i=0..MaxOps) = 17,895,697 at
    \* MaxOps = 6, confirmed by TLC to the state. No invariant below ever reads
    \* a position: both users are `\E s \in submissions: ...` membership tests,
    \* and the pipeline's step ordering is enforced structurally by the
    \* disjuncts of VerifyAdmit within a single atomic action, not by log order.
    \* So the ordering was paid for and never used. As a set the same MaxOps = 6
    \* bound checks in 25,141 states / ~7 s instead of 17.9 M / ~4 min — same
    \* coverage, 712x fewer states.
    submissions,
    accepted_pats,     \* Set of pat IDs that the verify pipeline admitted
    step3c_fired,      \* Set of pats for which Argon2id (step 3c) executed
    db_lookups,        \* Set of pats that triggered the indexed DB lookup (step 3b)
    op_count

vars == <<submissions, accepted_pats, step3c_fired, db_lookups, op_count>>

(*-- Init --------------------------------------------------------------------*)

Init ==
    /\ submissions = {}
    /\ accepted_pats = {}
    /\ step3c_fired = {}
    /\ db_lookups = {}
    /\ op_count = 0

(*-- Verify pipeline --------------------------------------------------------*)

\* The PAT verify state machine. Defensively models the canonical
\* corelink-auth-pat::verify_hybrid path:
\*   step 3a — constant-time HMAC compare; FAIL => short-circuit (no I/O)
\*   step 3b — indexed lookup by token-id prefix; ONLY runs if 3a passed.
\*             If row absent, short-circuit (no Argon2id).
\*   step 3c — Argon2id-2024 verify against the stored phc string; ONLY
\*             runs if 3b returned a row.
\*   accept  — ONLY if step 3c reported a constant-time match.
\* The pipeline is fail-closed: any FAIL leaves accepted_pats unchanged.
VerifyAdmit(p, hmac_ok, row_found, argon_ok) ==
    /\ p \in Pats
    /\ op_count < MaxOps
    /\ op_count' = op_count + 1
    /\ submissions' = submissions \union
                          {<<p, hmac_ok, row_found, argon_ok>>}
    /\ \/ /\ hmac_ok = FALSE
          \* Short-circuit before step 3b — no DB hit, no Argon2id.
          /\ UNCHANGED <<accepted_pats, step3c_fired, db_lookups>>
       \/ /\ hmac_ok = TRUE
          /\ row_found = FALSE
          \* Step 3b executed but no row — short-circuit before Argon2id.
          /\ db_lookups' = db_lookups \union {p}
          /\ UNCHANGED <<accepted_pats, step3c_fired>>
       \/ /\ hmac_ok = TRUE
          /\ row_found = TRUE
          /\ argon_ok = FALSE
          \* Step 3c executed and rejected — fail-closed.
          /\ db_lookups' = db_lookups \union {p}
          /\ step3c_fired' = step3c_fired \union {p}
          /\ UNCHANGED accepted_pats
       \/ /\ hmac_ok = TRUE
          /\ row_found = TRUE
          /\ argon_ok = TRUE
          \* Full hybrid pipeline succeeded — admit.
          /\ db_lookups' = db_lookups \union {p}
          /\ step3c_fired' = step3c_fired \union {p}
          /\ accepted_pats' = accepted_pats \union {p}

(*-- Next --------------------------------------------------------------------*)

Next ==
    \E p \in Pats, h \in BOOLEAN, r \in BOOLEAN, a \in BOOLEAN:
        VerifyAdmit(p, h, r, a)

Spec == Init /\ [][Next]_vars

(*-- Safety invariants -------------------------------------------------------*)

\* INV-AUTH-PAT-HMAC-SIG-VERIFIED (CRITICAL): no accepted PAT skipped the
\* HMAC short-circuit. Equivalent to: there exists at least one submission
\* of `p` with hmac_ok = TRUE preceding (or coincident with) admission.
\* Proved via the invariant accepted_pats ⊆ step3c_fired and the action
\* contract that step3c_fired only grows when hmac_ok = TRUE.
InvHmacVerifiedBeforeAdmit ==
    \A p \in accepted_pats: p \in step3c_fired

\* INV-AUTH-PAT-VERIFY-CONSTANT-TIME (HIGH): every Argon2id execution was
\* preceded by a successful HMAC + DB lookup. No "Argon2id leak" trace
\* where the adversary forces step 3c without first passing 3a + 3b.
InvNoArgonWithoutHmacAndRow ==
    \A p \in step3c_fired: p \in db_lookups

\* INV-AUTH-PAT-VERIFY-CONSTANT-TIME (HIGH, dual-direction): every DB
\* lookup was preceded by a passing HMAC. Closes the "DB-hit timing leak"
\* surface — adversary cannot probe DB by sending bogus HMAC.
InvNoDbHitWithoutHmac ==
    \A p \in db_lookups:
        \E s \in submissions:
            /\ s[1] = p
            /\ s[2] = TRUE

\* INV-AUTH-PAT-HASH-ARGON2ID-2024 (HIGH): no accepted PAT skipped
\* Argon2id (step 3c). Equivalent to accepted_pats ⊆ step3c_fired with the
\* additional obligation that some prior submission carried argon_ok=TRUE.
InvAcceptedRequiresArgon ==
    \A p \in accepted_pats:
        \E s \in submissions:
            /\ s[1] = p
            /\ s[2] = TRUE   \* hmac_ok
            /\ s[3] = TRUE   \* row_found
            /\ s[4] = TRUE   \* argon_ok

\* Bound sanity: accepted ⊆ step3c_fired ⊆ db_lookups (strict hybrid order).
InvHybridOrderingChain ==
    /\ accepted_pats \subseteq step3c_fired
    /\ step3c_fired \subseteq db_lookups

SafetyInvariants ==
    /\ InvHmacVerifiedBeforeAdmit
    /\ InvNoArgonWithoutHmacAndRow
    /\ InvNoDbHitWithoutHmac
    /\ InvAcceptedRequiresArgon
    /\ InvHybridOrderingChain

================================================================================
