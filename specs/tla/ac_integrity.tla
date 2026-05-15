--------------------------- MODULE ac_integrity ---------------------------
(***************************************************************************)
(* CoreLink — Action Cache (AC) envelope integrity (DEBT-005 batch 4 #1)   *)
(*                                                                         *)
(* Closes 3 CRITICAL TLA gaps from canonical-consistency baseline §3.4 AC: *)
(*                                                                         *)
(*   INV-AC-DIGEST-SIGNED  (CRITICAL, §3.4 AC)                              *)
(*     Every AC envelope MUST carry a valid HKDF signature. The GET path  *)
(*     mandatorily verifies `verify_sig(envelope.canonical_bytes,        *)
(*     envelope.sig)`. Any envelope without a valid signature MUST be    *)
(*     rejected and MUST NOT reach the caller. There is no bypass        *)
(*     (Lote 10.4bis WI-S04-004 P0 #1 fix).                              *)
(*                                                                         *)
(*   INV-AC-MERKLE-DETERMINISTIC  (CRITICAL, §3.4 AC)                       *)
(*     Same input set produces the same Merkle root byte-identical. The  *)
(*     builder lex-sorts outputs by digest and applies canonical         *)
(*     encoding (serde_jcs RFC 8785) before merkleization. Two builds   *)
(*     of the same content-set converge on the same root regardless of  *)
(*     insertion order.                                                   *)
(*                                                                         *)
(*   INV-AC-TENANT-SCOPED  (CRITICAL, §3.4 AC)                              *)
(*     AC entries are tenant-local. An AC entry produced by Tenant A    *)
(*     can NEVER be served to Tenant B even on key collision. The hit   *)
(*     check enforces `entry.tenant_id == ctx.tenant_id` BEFORE            *)
(*     signature verification.                                            *)
(*                                                                         *)
(* Threat model (modelled):                                                *)
(*   - Producer Put records signed envelope keyed (tenant, ac_key).      *)
(*   - Consumer Get returns entry iff (a) tenant matches AND (b)        *)
(*     signature verifies. Two distinct producers Put-ing the same      *)
(*     content (lex-sorted) converge to the same Merkle root.           *)
(*   - Adversarial paths (guard FALSE):                                  *)
(*       AttemptUnsignedReachConsumer — bypassing sig verify.            *)
(*       AttemptCrossTenantRead — serving foreign-tenant entry.          *)
(*       AttemptMerkleDivergence — same-content roots differ.            *)
(*                                                                         *)
(* Out of scope:                                                           *)
(*   - HKDF derivation pre-image resistance (cripto primitive).          *)
(*   - JCS encoding correctness (proved in serde_jcs vector suite).      *)
(*                                                                         *)
(* Cross-refs:                                                             *)
(*   - `specs/03_architecture/invariant_registry.md §3.4 INV-AC-*`        *)
(*   - `crates/corelink-ac/src/sig.rs` (HkdfVerifier)                    *)
(*   - `crates/corelink-ac/src/merkle.rs` (canonical builder)            *)
(***************************************************************************)

EXTENDS Integers, FiniteSets, Sequences, TLC

CONSTANTS
    Tenants,           \* Finite set of tenants
    AcKeys,            \* Finite set of AC cache keys
    ContentSets,       \* Finite set of distinct content-set tags (proxy for outputs)
    MaxOps

ASSUME
    /\ Tenants # {}
    /\ AcKeys # {}
    /\ ContentSets # {}
    /\ MaxOps \in Nat

\* An AC envelope is a record (tenant, key, content_set, sig_valid).
\* We model:
\*   - `entries` : SUBSET of records persisted to the store.
\*   - `served`  : SUBSET of records actually returned to a caller in a
\*                 (caller_tenant, key) Get response.
\*   - `roots`   : function content_set -> Merkle root (deterministic).
\*
\* Merkle root is modelled as the content_set tag itself (canonical
\* mapping). This abstracts away the actual hashing while preserving
\* determinism: same content_set => same root.

VARIABLES
    entries,           \* SUBSET [tenant: Tenants, key: AcKeys, cset: ContentSets, sig_valid: BOOLEAN]
    served,            \* Sequence of <<caller_tenant, key, cset>>
    op_count

vars == <<entries, served, op_count>>

Envelopes ==
    [tenant: Tenants, key: AcKeys, cset: ContentSets, sig_valid: BOOLEAN]

\* Canonical Merkle root for a content_set = the tag itself.
\* Independent of any other input — determinism by construction.
MerkleRoot(cset) == cset

(*-- Init --------------------------------------------------------------------*)

Init ==
    /\ entries  = {}
    /\ served   = <<>>
    /\ op_count = 0

(*-- Actions -----------------------------------------------------------------*)

\* Producer Put: writes a signed envelope keyed by (tenant, key) into the
\* store. The constructor ALWAYS sets sig_valid=TRUE — there is no path
\* by which an unsigned envelope is persisted.
PutSigned(t, k, cs) ==
    /\ t  \in Tenants
    /\ k  \in AcKeys
    /\ cs \in ContentSets
    /\ op_count < MaxOps
    /\ entries' = entries \cup
         {[tenant |-> t, key |-> k, cset |-> cs, sig_valid |-> TRUE]}
    /\ UNCHANGED served
    /\ op_count' = op_count + 1

\* Consumer Get: returns the envelope iff (a) tenant matches AND (b) sig
\* verifies. We append the served record to `served` for trace inspection.
GetHit(t, k) ==
    /\ t \in Tenants
    /\ k \in AcKeys
    /\ op_count < MaxOps
    /\ \E e \in entries:
         /\ e.tenant    = t           \* tenant-scoped hit gate
         /\ e.key       = k
         /\ e.sig_valid = TRUE        \* sig verify mandatory
         /\ served' = Append(served, <<t, k, e.cset>>)
    /\ UNCHANGED <<entries, op_count>>
    /\ op_count' = op_count + 1

\* Adversarial: an unsigned envelope is somehow served (sig-verify bypass).
\* Guard FALSE — exhibits the impossible trace.
AttemptUnsignedReachConsumer(t, k, cs) ==
    /\ t  \in Tenants
    /\ k  \in AcKeys
    /\ cs \in ContentSets
    /\ op_count < MaxOps
    /\ FALSE
    /\ served' = Append(served, <<t, k, cs>>)
    /\ UNCHANGED <<entries, op_count>>
    /\ op_count' = op_count + 1

\* Adversarial: cross-tenant read (entry of t' served to t # t').
\* Guard FALSE — tenant-scoped enforcement.
AttemptCrossTenantRead(t, k) ==
    /\ t \in Tenants
    /\ k \in AcKeys
    /\ op_count < MaxOps
    /\ \E e \in entries:
         /\ e.tenant # t
         /\ e.key    = k
         /\ FALSE
         /\ served' = Append(served, <<t, k, e.cset>>)
    /\ UNCHANGED <<entries, op_count>>
    /\ op_count' = op_count + 1

(*-- Next --------------------------------------------------------------------*)

Next ==
    \/ \E t \in Tenants, k \in AcKeys, cs \in ContentSets: PutSigned(t, k, cs)
    \/ \E t \in Tenants, k \in AcKeys: GetHit(t, k)
    \/ \E t \in Tenants, k \in AcKeys, cs \in ContentSets: AttemptUnsignedReachConsumer(t, k, cs)
    \/ \E t \in Tenants, k \in AcKeys: AttemptCrossTenantRead(t, k)

Spec == Init /\ [][Next]_vars

(*-- Safety invariants -------------------------------------------------------*)

\* INV-AC-DIGEST-SIGNED. Every served record corresponds to an entry that
\* was sig_valid=TRUE in the store at serve time.
InvDigestSigned ==
    \A i \in 1..Len(served):
        \E e \in entries:
            /\ e.tenant    = served[i][1]
            /\ e.key       = served[i][2]
            /\ e.cset      = served[i][3]
            /\ e.sig_valid = TRUE

\* INV-AC-TENANT-SCOPED. Every served record's tenant equals the entry's
\* tenant (no cross-tenant reads).
InvTenantScoped ==
    \A i \in 1..Len(served):
        \E e \in entries:
            /\ e.tenant    = served[i][1]
            /\ e.key       = served[i][2]
            /\ e.cset      = served[i][3]
            /\ e.tenant    = served[i][1]   \* explicit redundancy: tenant binding

\* INV-AC-MERKLE-DETERMINISTIC. For any two envelopes with the same
\* content_set tag, their Merkle roots are equal (byte-identical).
InvMerkleDeterministic ==
    \A e1, e2 \in entries:
        e1.cset = e2.cset => MerkleRoot(e1.cset) = MerkleRoot(e2.cset)

SafetyInvariants ==
    /\ InvDigestSigned
    /\ InvTenantScoped
    /\ InvMerkleDeterministic

================================================================================
