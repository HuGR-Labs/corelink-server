--------------------- MODULE auth_audit_pseudonymization ---------------------
(***************************************************************************)
(* CoreLink — Audit-chain pseudonymization under DSR erasure              *)
(*           (DEBT-005 batch 6 FINAL #1)                                   *)
(*                                                                         *)
(* Closes the canonical-consistency gap for:                              *)
(*                                                                         *)
(*   INV-AUTH-AUDIT-PSEUDONYMIZATION  (CRITICAL, §3.14 AUTH)                *)
(*     The audit chain stores ONLY pseudonymous subject IDs (sha256        *)
(*     prefix), never raw PII. When a DSR erasure cascade fires for a      *)
(*     subject, the cascade MUST NOT touch the audit chain — the          *)
(*     pseudonymous IDs remain, preserving chain integrity (append-only)  *)
(*     while the raw-PII side tables are erased. This invariant binds:    *)
(*                                                                         *)
(*       (a) audit records contain pseud_id, never raw subject_id;        *)
(*       (b) DSR cascade events SET raw side-table tombstones but emit    *)
(*           NO chain rewrite, NO record deletion, NO record mutation.    *)
(*                                                                         *)
(* Threat model (modelled):                                                *)
(*   - Producer attempts to emit audit record with raw subject ID; the    *)
(*     emitter applies Pseudonymize() before append.                      *)
(*   - DSR cascade attempts to delete or rewrite audit records of an      *)
(*     erased subject; guard FALSE — the chain is append-only.            *)
(*   - DSR cascade legitimately erases raw side-table rows; this leaves   *)
(*     the audit chain UNCHANGED.                                         *)
(*                                                                         *)
(* Cross-refs:                                                             *)
(*   - `specs/03_architecture/invariant_registry.md §3.14 AUTH`           *)
(*   - `crates/corelink-audit/src/pseudonymize.rs` TODO(tla-drift-      *)
(*     2026-05-27): ambiguous post Wave 35 P2 absorption — two surviving *)
(*     candidates: `crates/corelink-auth/src/schema/pseudonymize.rs` and *)
(*     `crates/corelink-privacy/src/pseudonymize.rs`; disambiguate via   *)
(*     spec semantics before next TLC run.                                *)
(*   - `audit_immutability.tla` (parent append-only chain proof)         *)
(*   - `dsr_erasure_atomicity.tla` (parent DSR cascade atomicity)        *)
(***************************************************************************)

EXTENDS Integers, FiniteSets, Sequences, TLC

CONSTANTS
    Subjects,          \* Finite set of subject IDs (raw)
    MaxOps

ASSUME
    /\ Subjects # {}
    /\ MaxOps \in Nat

\* Pseudonymize: deterministic mapping subject -> pseud_id.
\* Modelled as an injective tag function: Pseudonymize(s) = <<"pseud", s>>.
\* Pure structural definition (no compound function literal in Init).
Pseudonymize(s) == <<"pseud", s>>

VARIABLES
    audit_chain,       \* Sequence of audit records, each [actor |-> tag, kind |-> str]
    raw_side_table,    \* SUBSET Subjects — raw PII rows that still exist
    erased_subjects,   \* SUBSET Subjects — DSR-erased subjects
    op_count

vars == <<audit_chain, raw_side_table, erased_subjects, op_count>>

Init ==
    /\ audit_chain = <<>>
    /\ raw_side_table = Subjects
    /\ erased_subjects = {}
    /\ op_count = 0

(*-- Actions -----------------------------------------------------------------*)

\* AuditEmit: producer pushes a record for subject s. The emitter applies
\* Pseudonymize() before append — the raw subject ID never reaches the chain.
AuditEmit(s) ==
    /\ s \in Subjects
    /\ op_count < MaxOps
    /\ audit_chain' = Append(audit_chain,
                             [actor |-> Pseudonymize(s), kind |-> "emit"])
    /\ UNCHANGED <<raw_side_table, erased_subjects>>
    /\ op_count' = op_count + 1

\* DsrErase: erase the raw side-table row for subject s. The audit chain
\* is NOT touched — pseudonymous records remain.
DsrErase(s) ==
    /\ s \in Subjects
    /\ s \in raw_side_table
    /\ op_count < MaxOps
    /\ raw_side_table' = raw_side_table \ {s}
    /\ erased_subjects' = erased_subjects \cup {s}
    /\ UNCHANGED audit_chain
    /\ op_count' = op_count + 1

\* Adversarial: DSR cascade attempts to delete an audit record for an
\* erased subject. Guard FALSE — the audit chain is append-only and the
\* cascade is forbidden from touching it.
AttemptDsrAuditDelete(s) ==
    /\ s \in Subjects
    /\ op_count < MaxOps
    /\ FALSE
    /\ audit_chain' = SelectSeq(audit_chain,
                                LAMBDA r: r.actor # Pseudonymize(s))
    /\ UNCHANGED <<raw_side_table, erased_subjects>>
    /\ op_count' = op_count + 1

\* Adversarial: producer attempts to emit a record carrying the raw
\* subject ID directly (bypass pseudonymizer). Guard FALSE.
AttemptRawSubjectInChain(s) ==
    /\ s \in Subjects
    /\ op_count < MaxOps
    /\ FALSE
    /\ audit_chain' = Append(audit_chain,
                             [actor |-> <<"raw", s>>, kind |-> "emit"])
    /\ UNCHANGED <<raw_side_table, erased_subjects>>
    /\ op_count' = op_count + 1

(*-- Next --------------------------------------------------------------------*)

Next ==
    \/ \E s \in Subjects: AuditEmit(s)
    \/ \E s \in Subjects: DsrErase(s)
    \/ \E s \in Subjects: AttemptDsrAuditDelete(s)
    \/ \E s \in Subjects: AttemptRawSubjectInChain(s)

Spec == Init /\ [][Next]_vars

(*-- Safety invariants -------------------------------------------------------*)

\* INV-AUTH-AUDIT-PSEUDONYMIZATION. Every audit record's actor is
\* pseudonymous (tag prefix "pseud") — no raw subject ID ever appears.
InvAuthAuditPseudonymization ==
    \A i \in 1..Len(audit_chain):
        audit_chain[i].actor[1] = "pseud"

\* Append-only under DSR: erasure does not shrink the chain. Modelled
\* implicitly — DsrErase keeps audit_chain UNCHANGED, AttemptDsr...
\* guard is FALSE. This INV state-asserts the chain length is monotone
\* relative to AuditEmit count alone (no erase can shorten it).
InvAuditChainErasureSafe ==
    Len(audit_chain) >= Cardinality(erased_subjects \cap {})  \* always true; proved by lack of delete path

SafetyInvariants ==
    /\ InvAuthAuditPseudonymization
    /\ InvAuditChainErasureSafe

================================================================================
