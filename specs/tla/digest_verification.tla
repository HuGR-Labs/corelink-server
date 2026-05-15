------------------------ MODULE digest_verification ------------------------
(***************************************************************************)
(* CoreLink — BLAKE3 digest verification at Write path                    *)
(* (DEBT-005 batch 4 #3)                                                   *)
(*                                                                         *)
(* Closes 1 CRITICAL TLA gap from canonical-consistency baseline §3.3 CAS:*)
(*                                                                         *)
(*   INV-DIGEST-VERIFICATION  (CRITICAL, §3.3 CAS)                          *)
(*     (alias historic: `INV-DigestVerification`)                          *)
(*     Every write rejects body whose `BLAKE3(body) != claimed_digest`.  *)
(*     The CAS layer NEVER persists a body whose digest is unverified —  *)
(*     verification is a pre-condition of the persistence transition.    *)
(*     No fallback, no fail-open path; mismatched body rejected with     *)
(*     `400 DigestMismatch` and never written.                           *)
(*                                                                         *)
(* Threat model (modelled):                                                *)
(*   - Producer submits (body, claimed_digest). Two outcomes are valid:  *)
(*       WriteAccept (hash(body) == claimed_digest) → body persisted.   *)
(*       WriteRejectMismatch (hash(body) # claimed_digest) → no persist.*)
(*   - Adversarial path: a mismatched body somehow reaches the store —  *)
(*     guard FALSE.                                                      *)
(*                                                                         *)
(* Out of scope:                                                           *)
(*   - BLAKE3 collision resistance (cripto primitive; out of model).     *)
(*   - Idempotency on retry (covered by `cas_integrity.tla`).            *)
(*                                                                         *)
(* Cross-refs:                                                             *)
(*   - `specs/03_architecture/invariant_registry.md §3.3 INV-DIGEST-*`    *)
(*   - `crates/corelink-hash/src/lib.rs::verify_body`                    *)
(*   - `crates/corelink-worker/src/r2_put.rs` (Write path)               *)
(*   - sibling: `specs/tla/cas_integrity.tla` (InvPoisoningRejected)     *)
(***************************************************************************)

EXTENDS Integers, FiniteSets, Sequences, TLC

CONSTANTS
    Bodies,            \* Finite set of body tags
    Digests,           \* Finite set of digest tags
    Writes,            \* Finite set of submit IDs to model
    MaxOps

ASSUME
    /\ Bodies # {}
    /\ Digests # {}
    /\ Writes # {}
    /\ MaxOps \in Nat

\* TrueDigest is the actual BLAKE3 of a body. We enumerate it via TLC
\* at Init (function-literal lift) so the model covers every possible
\* body↔digest binding.

States == {"pending", "accepted", "rejected_mismatch"}

VARIABLES
    TrueDigest,        \* Bodies -> Digests (immutable; enumerated by TLC)
    w_body,            \* Writes -> Bodies — body submitted
    w_claimed,         \* Writes -> Digests — claimed digest
    w_state,           \* Writes -> States
    store,             \* SUBSET Writes — writes that landed in CAS store
    op_count

vars == <<TrueDigest, w_body, w_claimed, w_state, store, op_count>>

(*-- Init --------------------------------------------------------------------*)

Init ==
    /\ TrueDigest \in [Bodies -> Digests]
    /\ w_body     \in [Writes -> Bodies]
    /\ w_claimed  \in [Writes -> Digests]
    /\ w_state    = [w \in Writes |-> "pending"]
    /\ store      = {}
    /\ op_count   = 0

(*-- Actions -----------------------------------------------------------------*)

\* Write accepts: body's true digest matches claim. CAS persists.
WriteAccept(w) ==
    /\ w \in Writes
    /\ w_state[w] = "pending"
    /\ TrueDigest[w_body[w]] = w_claimed[w]
    /\ op_count < MaxOps
    /\ w_state' = [w_state EXCEPT ![w] = "accepted"]
    /\ store'   = store \cup {w}
    /\ UNCHANGED <<TrueDigest, w_body, w_claimed>>
    /\ op_count' = op_count + 1

\* Write rejected: digest mismatch. CAS refuses to persist.
WriteRejectMismatch(w) ==
    /\ w \in Writes
    /\ w_state[w] = "pending"
    /\ TrueDigest[w_body[w]] # w_claimed[w]
    /\ op_count < MaxOps
    /\ w_state' = [w_state EXCEPT ![w] = "rejected_mismatch"]
    /\ UNCHANGED <<TrueDigest, w_body, w_claimed, store>>
    /\ op_count' = op_count + 1

\* Adversarial: a digest-mismatched body somehow lands in the store.
\* Guard FALSE — fail-closed enforcement.
AttemptStoreMismatch(w) ==
    /\ w \in Writes
    /\ w_state[w] = "pending"
    /\ TrueDigest[w_body[w]] # w_claimed[w]
    /\ op_count < MaxOps
    /\ FALSE
    /\ w_state' = [w_state EXCEPT ![w] = "accepted"]
    /\ store'   = store \cup {w}
    /\ UNCHANGED <<TrueDigest, w_body, w_claimed>>
    /\ op_count' = op_count + 1

(*-- Next --------------------------------------------------------------------*)

Next ==
    \/ \E w \in Writes: WriteAccept(w)
    \/ \E w \in Writes: WriteRejectMismatch(w)
    \/ \E w \in Writes: AttemptStoreMismatch(w)

Spec == Init /\ [][Next]_vars

(*-- Safety invariants -------------------------------------------------------*)

\* INV-DIGEST-VERIFICATION. Every persisted write satisfies the digest
\* binding: BLAKE3(body) == claimed_digest.
InvDigestVerification ==
    \A w \in store:
        TrueDigest[w_body[w]] = w_claimed[w]

\* Coherence: a persisted write is in state "accepted".
InvStoreAccepted ==
    \A w \in store: w_state[w] = "accepted"

\* Rejected writes never land in the store.
InvRejectedNotInStore ==
    \A w \in Writes:
        w_state[w] = "rejected_mismatch" => w \notin store

SafetyInvariants ==
    /\ InvDigestVerification
    /\ InvStoreAccepted
    /\ InvRejectedNotInStore

================================================================================
