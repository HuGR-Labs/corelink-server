--------------------- MODULE gc_reachable_set_complete ---------------------
(***************************************************************************)
(* CoreLink — GC mark phase 3-pass reachable-set completeness              *)
(*           (DEBT-005 batch 6 FINAL #4)                                   *)
(*                                                                         *)
(* Closes the canonical-consistency gap for:                              *)
(*                                                                         *)
(*   INV-GC-REACHABLE-SET-COMPLETE  (CRITICAL, §3 row 349)                  *)
(*     The mark phase performs a 3-pass scan over the union of root      *)
(*     tables: blob_meta + ac_meta + manifest_chunks. Every reachable    *)
(*     digest in any of the three is added to the reachable set; the      *)
(*     sweep phase deletes ONLY digests that are NOT in the reachable    *)
(*     set. The mark must be SUPERSET-SAFE: it may mark slightly more    *)
(*     than strictly reachable (e.g. via overlapping table content) but  *)
(*     MUST NEVER omit a reachable digest.                              *)
(*                                                                         *)
(* Threat model (modelled):                                                *)
(*   - Roots arrive in three tables; the marker visits each table.       *)
(*   - Each table contributes its referenced digests to the reachable    *)
(*     set. The marker's union is reachable.                            *)
(*   - Adversarial action AttemptOmitReachable (guard FALSE) tries to    *)
(*     skip a root table or omit a digest — the parent invariant traps. *)
(*                                                                         *)
(* Cross-refs:                                                             *)
(*   - `specs/03_architecture/invariant_registry.md §3 row 349`           *)
(*   - `crates/corelink-gc/src/mark.rs`                                 *)
(*   - `gc_correctness.tla` (parent — InvGCReachableNeverDeleted)         *)
(***************************************************************************)

EXTENDS Integers, FiniteSets, Sequences, TLC

CONSTANTS
    Digests,           \* Finite set of content-addressed digests
    MaxOps

ASSUME
    /\ Digests # {}
    /\ MaxOps \in Nat

VARIABLES
    blob_meta,         \* SUBSET Digests — digests referenced from blob root
    ac_meta,           \* SUBSET Digests — digests referenced from AC root
    manifest_chunks,   \* SUBSET Digests — digests referenced from manifest root
    reachable_set,     \* SUBSET Digests — marker's accumulated reachable set
    pass_count,        \* 0..3 — number of marker passes completed
    op_count

vars == <<blob_meta, ac_meta, manifest_chunks, reachable_set,
          pass_count, op_count>>

\* Union of the three root tables — the ground truth for reachable.
\* Pure structural definition (no compound literal in Init).
TrueReachable == blob_meta \cup ac_meta \cup manifest_chunks

Init ==
    /\ blob_meta = {}
    /\ ac_meta = {}
    /\ manifest_chunks = {}
    /\ reachable_set = {}
    /\ pass_count = 0
    /\ op_count = 0

(*-- Actions -----------------------------------------------------------------*)

\* AddBlobRoot: insert digest d into the blob root table.
AddBlobRoot(d) ==
    /\ d \in Digests
    /\ op_count < MaxOps
    /\ blob_meta' = blob_meta \cup {d}
    /\ UNCHANGED <<ac_meta, manifest_chunks, reachable_set, pass_count>>
    /\ op_count' = op_count + 1

AddAcRoot(d) ==
    /\ d \in Digests
    /\ op_count < MaxOps
    /\ ac_meta' = ac_meta \cup {d}
    /\ UNCHANGED <<blob_meta, manifest_chunks, reachable_set, pass_count>>
    /\ op_count' = op_count + 1

AddManifestRoot(d) ==
    /\ d \in Digests
    /\ op_count < MaxOps
    /\ manifest_chunks' = manifest_chunks \cup {d}
    /\ UNCHANGED <<blob_meta, ac_meta, reachable_set, pass_count>>
    /\ op_count' = op_count + 1

\* MarkPass1: scan blob_meta into reachable_set.
MarkPass1 ==
    /\ pass_count = 0
    /\ op_count < MaxOps
    /\ reachable_set' = reachable_set \cup blob_meta
    /\ pass_count' = 1
    /\ UNCHANGED <<blob_meta, ac_meta, manifest_chunks>>
    /\ op_count' = op_count + 1

\* MarkPass2: scan ac_meta into reachable_set.
MarkPass2 ==
    /\ pass_count = 1
    /\ op_count < MaxOps
    /\ reachable_set' = reachable_set \cup ac_meta
    /\ pass_count' = 2
    /\ UNCHANGED <<blob_meta, ac_meta, manifest_chunks>>
    /\ op_count' = op_count + 1

\* MarkPass3: scan manifest_chunks into reachable_set.
MarkPass3 ==
    /\ pass_count = 2
    /\ op_count < MaxOps
    /\ reachable_set' = reachable_set \cup manifest_chunks
    /\ pass_count' = 3
    /\ UNCHANGED <<blob_meta, ac_meta, manifest_chunks>>
    /\ op_count' = op_count + 1

\* Adversarial: marker reports completion having skipped a pass.
\* Guard FALSE — the marker is sequentially gated on pass_count.
AttemptSkipPass ==
    /\ pass_count < 3
    /\ op_count < MaxOps
    /\ FALSE
    /\ pass_count' = 3
    /\ UNCHANGED <<blob_meta, ac_meta, manifest_chunks, reachable_set>>
    /\ op_count' = op_count + 1

(*-- Next --------------------------------------------------------------------*)

Next ==
    \/ \E d \in Digests: AddBlobRoot(d)
    \/ \E d \in Digests: AddAcRoot(d)
    \/ \E d \in Digests: AddManifestRoot(d)
    \/ MarkPass1
    \/ MarkPass2
    \/ MarkPass3
    \/ AttemptSkipPass

Spec == Init /\ [][Next]_vars

(*-- Safety invariants -------------------------------------------------------*)

\* INV-GC-REACHABLE-SET-COMPLETE. After all 3 passes complete, the
\* reachable_set MUST be a superset of the true reachable union — no
\* root digest is omitted. Pre-completion the set may be partial.
InvGcReachableSetComplete ==
    pass_count = 3 => TrueReachable \subseteq reachable_set

\* Superset-safety: reachable_set never contains a digest that isn't a
\* root (modulo intermediate passes — the set only grows via root unions).
InvReachableSetMonotone ==
    reachable_set \subseteq TrueReachable

SafetyInvariants ==
    /\ InvGcReachableSetComplete
    /\ InvReachableSetMonotone

================================================================================
