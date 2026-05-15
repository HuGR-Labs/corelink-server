---------------------------- MODULE merkle_integrity ----------------------------
(***************************************************************************)
(* CoreLink — Merkle envelope + manifest integrity (DEBT-005 5/40)         *)
(*                                                                         *)
(* Closes 2 CRITICAL TLA gaps from canonical-consistency baseline:        *)
(*                                                                         *)
(*   INV-AC-MERKLE-VALID         (CRITICAL)                                *)
(*     `envelope.merkle_root` binds the entire tree of action results;    *)
(*     any byte flip in the serialized tree MUST cause                    *)
(*     verify_structure to reject. No tampered envelope is admitted.       *)
(*                                                                         *)
(*   INV-MULTIPART-MANIFEST-VALID  (CRITICAL)                              *)
(*     `manifest.merkle_root` binds the chunk tree; tampered manifests   *)
(*     are rejected by verify_structure. Same algebraic structure as the *)
(*     AC envelope: one Merkle tree, one root, one verifier.              *)
(*                                                                         *)
(* Abstraction:                                                            *)
(*   The Merkle tree is abstracted as a function leaves -> digest plus    *)
(*   a deterministic accumulator. Concrete hash math is out of scope;    *)
(*   we model COLLISION-FREE digest semantics: two distinct leaf vectors *)
(*   produce two distinct roots, modulo cryptographic assumptions.       *)
(*                                                                         *)
(* Threat model:                                                           *)
(*   - Adversary tampers an arbitrary leaf (TamperLeaf action).            *)
(*   - Adversary swaps the claimed root (TamperRoot action).              *)
(*   - Re-build by verifier MUST reject any inconsistent state.            *)
(*                                                                         *)
(* Cross-refs:                                                             *)
(*   - `specs/03_architecture/invariant_registry.md §3.4 INV-AC-MERKLE-*` *)
(*   - `specs/03_architecture/invariant_registry.md §3.5                   *)
(*       INV-MULTIPART-MANIFEST-VALID`                                     *)
(*   - `specs/03_architecture/security_model.md §6.9 CTRL-FORMAL-001`      *)
(***************************************************************************)

EXTENDS Integers, FiniteSets, Sequences, TLC

CONSTANTS
    Envelopes,       \* Finite set of envelope IDs (AC + Manifest unified)
    Leaves,          \* Finite set of leaf values the adversary may choose
    MaxOps           \* Bound on adversary moves

ASSUME
    /\ Envelopes # {}
    /\ Leaves # {}
    /\ Cardinality(Leaves) >= 2     \* needed to exercise tamper actions
    /\ MaxOps \in Nat

\* Abstract Merkle root: deterministic function of the leaf vector
\* projected as a SET (order independent — Lote 10.4bis canonical
\* `serde_jcs` JSON projection + lex sort produces the same root for
\* any permutation of the same input). Using the leaf set as the root
\* is collision-free for the abstraction: two distinct leaf sets =
\* two distinct roots. This is the model-checking analogue of the
\* SHA-256/BLAKE3 collision-resistance assumption.
MerkleRoot(leaf_set) == leaf_set

VARIABLES
    leaves_of,       \* Function Envelopes -> SUBSET Leaves: actual content
    claimed_root,    \* Function Envelopes -> the root the producer wrote
    accepted,        \* Set of Envelopes verify_structure admitted
    rejected,        \* Set of Envelopes verify_structure rejected
    op_count

vars == <<leaves_of, claimed_root, accepted, rejected, op_count>>

(*-- Init --------------------------------------------------------------------*)

Init ==
    /\ leaves_of    = [e \in Envelopes |-> {}]
    /\ claimed_root = [e \in Envelopes |-> {}]
    /\ accepted     = {}
    /\ rejected     = {}
    /\ op_count     = 0

(*-- Actions -----------------------------------------------------------------*)

\* Honest producer: writes a fresh envelope with leaves L and the correct
\* claimed_root derived from L.
ProduceHonest(e, L) ==
    /\ e \in Envelopes
    /\ L \subseteq Leaves
    /\ L # {}
    /\ e \notin accepted
    /\ e \notin rejected
    /\ leaves_of[e] = {}        \* fresh slot
    /\ op_count < MaxOps
    /\ leaves_of'    = [leaves_of    EXCEPT ![e] = L]
    /\ claimed_root' = [claimed_root EXCEPT ![e] = MerkleRoot(L)]
    /\ op_count'     = op_count + 1
    /\ UNCHANGED <<accepted, rejected>>

\* Adversary tampers leaves AFTER production: replaces leaves_of[e] with
\* a different non-empty subset. The claimed_root from the producer stays
\* the same — modelling on-wire byte flip of payload but not of header.
TamperLeaves(e, Lnew) ==
    /\ e \in Envelopes
    /\ Lnew \subseteq Leaves
    /\ Lnew # {}
    /\ leaves_of[e] # {}
    /\ Lnew # leaves_of[e]
    /\ e \notin accepted
    /\ e \notin rejected
    /\ op_count < MaxOps
    /\ leaves_of' = [leaves_of EXCEPT ![e] = Lnew]
    /\ op_count'  = op_count + 1
    /\ UNCHANGED <<claimed_root, accepted, rejected>>

\* Adversary tampers claimed_root: swaps it for a different value while
\* leaves remain genuine — modelling header tampering with intact body.
TamperRoot(e, Rnew) ==
    /\ e \in Envelopes
    /\ Rnew \subseteq Leaves
    /\ leaves_of[e] # {}
    /\ Rnew # claimed_root[e]
    /\ e \notin accepted
    /\ e \notin rejected
    /\ op_count < MaxOps
    /\ claimed_root' = [claimed_root EXCEPT ![e] = Rnew]
    /\ op_count'     = op_count + 1
    /\ UNCHANGED <<leaves_of, accepted, rejected>>

\* Verifier: re-builds the tree from leaves and compares to claimed_root.
\* Mismatch -> rejected; match -> accepted. Idempotent post-decision.
Verify(e) ==
    /\ e \in Envelopes
    /\ leaves_of[e] # {}
    /\ e \notin accepted
    /\ e \notin rejected
    /\ op_count < MaxOps
    /\ \/ /\ MerkleRoot(leaves_of[e]) = claimed_root[e]
          /\ accepted' = accepted \union {e}
          /\ UNCHANGED rejected
       \/ /\ MerkleRoot(leaves_of[e]) # claimed_root[e]
          /\ rejected' = rejected \union {e}
          /\ UNCHANGED accepted
    /\ op_count' = op_count + 1
    /\ UNCHANGED <<leaves_of, claimed_root>>

(*-- Next --------------------------------------------------------------------*)

Next ==
    \/ \E e \in Envelopes, L \in SUBSET Leaves: ProduceHonest(e, L)
    \/ \E e \in Envelopes, L \in SUBSET Leaves: TamperLeaves(e, L)
    \/ \E e \in Envelopes, R \in SUBSET Leaves: TamperRoot(e, R)
    \/ \E e \in Envelopes: Verify(e)

Spec == Init /\ [][Next]_vars

(*-- Safety invariants -------------------------------------------------------*)

\* INV-AC-MERKLE-VALID (envelope flavour) /
\* INV-MULTIPART-MANIFEST-VALID (manifest flavour):
\* every accepted envelope has a genuine root binding to its leaves.
\* Any state where `e \in accepted` AND `MerkleRoot(leaves_of[e]) #
\* claimed_root[e]` is a forgery admission — by construction
\* unreachable from Verify, but the safety invariant codifies it.
InvAcceptedBindsRoot ==
    \A e \in accepted: MerkleRoot(leaves_of[e]) = claimed_root[e]

\* No envelope can be both accepted and rejected.
InvDecisionsDisjoint ==
    accepted \cap rejected = {}

\* If a tampered envelope was verified, it landed in `rejected`. Every
\* state in which `claimed_root[e] # MerkleRoot(leaves_of[e])` MUST
\* have e NOT in accepted (combined with InvAcceptedBindsRoot this is
\* tautological — but explicitly named for traceability).
InvTamperedNotAccepted ==
    \A e \in Envelopes:
        (leaves_of[e] # {} /\ MerkleRoot(leaves_of[e]) # claimed_root[e])
            => e \notin accepted

SafetyInvariants ==
    /\ InvAcceptedBindsRoot
    /\ InvDecisionsDisjoint
    /\ InvTamperedNotAccepted

================================================================================
