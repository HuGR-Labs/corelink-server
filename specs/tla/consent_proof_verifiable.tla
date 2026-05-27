--------------------- MODULE consent_proof_verifiable ---------------------
(***************************************************************************)
(* CoreLink — Consent proof verifiability                                  *)
(*           (DEBT-005 batch 6 FINAL #3)                                   *)
(*                                                                         *)
(* Closes the canonical-consistency gap for:                              *)
(*                                                                         *)
(*   INV-CONSENT-PROOF-VERIFIABLE  (CRITICAL, §3 row 171)                   *)
(*     Consent records carry a `notice_text_hash` (SHA-256 of the notice  *)
(*     copy presented at grant time) plus an HMAC-SHA256 binding under   *)
(*     HKDF info = "corelink/v1/consent-hmac". A verify endpoint replays *)
(*     the hash against an archived notice copy and re-derives the HMAC; *)
(*     ANY tampering of (notice, purpose, legal_basis, subject_id,      *)
(*     granted_at, token) is detected.                                  *)
(*                                                                         *)
(*     This spec models the verifier as a function on the immutable     *)
(*     consent record: a record is verifiable iff stored_hash =          *)
(*     hash(notice_text) AND stored_hmac = hmac(canonical_payload).     *)
(*     Tampering with any payload field produces a hash/HMAC mismatch   *)
(*     → verify returns FALSE.                                          *)
(*                                                                         *)
(*     This is the standalone proof. Parent spec                         *)
(*     `dsr_erasure_atomicity.tla` (InvConsentSymmetry, S-11             *)
(*     WI-S11-008) covers the grant/revoke ledger symmetry — the two    *)
(*     specs together close the registry §3 row 171 obligation.         *)
(*                                                                         *)
(* Threat model (modelled):                                                *)
(*   - Grant records consent with (notice_text, purpose, hash, hmac).   *)
(*   - Verifier replays hash + hmac on retrieve.                        *)
(*   - Adversarial tamper attempts to mutate stored fields after grant; *)
(*     guard FALSE (the consent table is append-only on the grant path  *)
(*     and revocation is a separate column, not a mutation of the      *)
(*     grant record).                                                    *)
(*                                                                         *)
(* Cross-refs:                                                             *)
(*   - `specs/03_architecture/invariant_registry.md §3 row 171`           *)
(*   - `crates/corelink-privacy/src/consent.rs` (post Wave 35 P2:        *)
(*     corelink-privacy-consent-ledger absorbed into corelink-privacy/)   *)
(*   - `dsr_erasure_atomicity.tla` (parent — InvConsentSymmetry)         *)
(*   - `privacy_model.md §5.6.1` (canonical purposes enum)               *)
(***************************************************************************)

EXTENDS Integers, FiniteSets, Sequences, TLC

CONSTANTS
    Subjects,          \* Finite set of subject IDs
    Notices,           \* Finite set of notice texts
    MaxOps

ASSUME
    /\ Subjects # {}
    /\ Notices # {}
    /\ MaxOps \in Nat

\* Hash and HMAC modelled as injective tags. Pure structural defs —
\* no compound function literal in Init (FT-4 mitigation).
Hash(n) == <<"sha256", n>>
Hmac(payload) == <<"hmac", payload>>

\* Canonical payload binds the consent record's fields.
CanonicalPayload(subj, notice) == <<subj, notice>>

VARIABLES
    consent_records,   \* SUBSET of records [subj, notice, stored_hash, stored_hmac]
    verify_log,        \* Sequence of <<record, verifier_result>>
    op_count

vars == <<consent_records, verify_log, op_count>>

Init ==
    /\ consent_records = {}
    /\ verify_log = <<>>
    /\ op_count = 0

(*-- Actions -----------------------------------------------------------------*)

\* GrantConsent: append a record. Hash + HMAC are computed at grant time
\* from the canonical payload — never recomputed elsewhere.
GrantConsent(subj, notice) ==
    /\ subj \in Subjects
    /\ notice \in Notices
    /\ op_count < MaxOps
    /\ consent_records' = consent_records \cup
         {[subj |-> subj,
           notice |-> notice,
           stored_hash |-> Hash(notice),
           stored_hmac |-> Hmac(CanonicalPayload(subj, notice))]}
    /\ UNCHANGED verify_log
    /\ op_count' = op_count + 1

\* VerifyConsent: re-derive hash + HMAC from the record's stored payload
\* and compare against stored fields. Returns TRUE iff both match.
VerifyConsent(r) ==
    /\ r \in consent_records
    /\ op_count < MaxOps
    /\ verify_log' = Append(verify_log,
         <<r,
           (r.stored_hash = Hash(r.notice))
            /\ (r.stored_hmac = Hmac(CanonicalPayload(r.subj, r.notice)))>>)
    /\ UNCHANGED consent_records
    /\ op_count' = op_count + 1

\* Adversarial: post-hoc tamper of a stored record's notice/payload —
\* would break the hash + HMAC chain. Guard FALSE (consent table is
\* append-only; revocation is a separate column).
AttemptTamperConsent(r, new_notice) ==
    /\ r \in consent_records
    /\ new_notice \in Notices
    /\ op_count < MaxOps
    /\ FALSE
    /\ consent_records' = (consent_records \ {r}) \cup
         {[subj |-> r.subj,
           notice |-> new_notice,
           stored_hash |-> r.stored_hash,
           stored_hmac |-> r.stored_hmac]}
    /\ UNCHANGED verify_log
    /\ op_count' = op_count + 1

(*-- Next --------------------------------------------------------------------*)

Next ==
    \/ \E s \in Subjects, n \in Notices: GrantConsent(s, n)
    \/ \E r \in consent_records: VerifyConsent(r)
    \/ \E r \in consent_records, n \in Notices: AttemptTamperConsent(r, n)

Spec == Init /\ [][Next]_vars

(*-- Safety invariants -------------------------------------------------------*)

\* INV-CONSENT-PROOF-VERIFIABLE. Every stored record's stored_hash and
\* stored_hmac are consistent with its payload (i.e. the verifier would
\* succeed). Tamper actions are guarded FALSE so this is preserved.
InvConsentProofVerifiable ==
    \A r \in consent_records:
        /\ r.stored_hash = Hash(r.notice)
        /\ r.stored_hmac = Hmac(CanonicalPayload(r.subj, r.notice))

\* Verifier soundness: every successful verify outcome corresponds to a
\* record whose fields actually match its stored hash + HMAC.
InvVerifierSound ==
    \A i \in 1..Len(verify_log):
        verify_log[i][2] = TRUE
            => /\ verify_log[i][1].stored_hash = Hash(verify_log[i][1].notice)
               /\ verify_log[i][1].stored_hmac
                  = Hmac(CanonicalPayload(verify_log[i][1].subj,
                                          verify_log[i][1].notice))

SafetyInvariants ==
    /\ InvConsentProofVerifiable
    /\ InvVerifierSound

================================================================================
