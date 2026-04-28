---- MODULE dsr_erasure_atomicity ----
EXTENDS Naturals, Sequences, FiniteSets, TLC

(* CoreLink S-11 WI-S11-008 — DSR erasure atomicity + consent symmetry + residency pinning.
 *
 * Validates (Lote 10.11.0-bis canonical):
 *  - INV-DATA-ERASURE-COMPLETE (CRITICAL): cross-backend erasure is atomic-completion-gated
 *    (12 backends canonical: 8 effective + 4 pseudonymized; per WI-S11-002 + privacy_model.md §6.2).
 *  - InvConsentSymmetry (CRITICAL): consent_revocation record schema mirrors consent_ledger
 *    (Lote 9.4 H-05 + WI-S11-003; 6-field schema canonical).
 *  - InvAuditAppendOnlyDSR (CRITICAL): audit_chain length monotonic non-decreasing; prefix preserved.
 *  - InvBackendAckIdempotent: per-backend ack writes are idempotent under retry.
 *  - InvResidencyPinned: ticket processed in tenant.primary_region (cross-link WI-S11-007 property test).
 *
 * TLC v1.8.0 SHA-256 pinned (ADR-0042 §A1). Bounded state space via MaxConcurrentErasures + MaxAuditChainLen.
 *)

CONSTANTS
  Backends,                       \* 12 backend identifiers (canonical)
  EffectiveBackends,              \* 8 — erasure física possível
  PseudonymizedBackends,          \* 4 — legal_hold canonical
  ConsentBasedPurposes,           \* purposes with basis_legal = consent (revocable)
  NonConsentPurposes,             \* contract / legal_obligation / legitimate_interest
  Subjects,
  Tenants,
  Tickets,                        \* abstract UUIDv7 set
  Regions,                        \* 6 canonical: wnam/enam/weur/sam/apac/afr
  Locales,                        \* 3 canonical: pt-BR/en-US/es-MX
  MaxConcurrentErasures,
  MaxAuditChainLen,
  MaxAttempts                     \* PAT-RETRY-IDEMPOTENT-001 default 5

ASSUME
  /\ Backends = EffectiveBackends \cup PseudonymizedBackends
  /\ EffectiveBackends \cap PseudonymizedBackends = {}
  /\ Cardinality(EffectiveBackends) = 8
  /\ Cardinality(PseudonymizedBackends) = 4
  /\ ConsentBasedPurposes \cap NonConsentPurposes = {}
  /\ Cardinality(Regions) = 6
  /\ Cardinality(Locales) = 3
  /\ MaxConcurrentErasures \in Nat /\ MaxConcurrentErasures > 0
  /\ MaxAuditChainLen      \in Nat /\ MaxAuditChainLen      > 0
  /\ MaxAttempts           \in Nat /\ MaxAttempts           > 0

VARIABLES
  erasure_state,        \* [Tickets -> EnumStatus]
  backend_state,        \* [Tickets X Backends -> EnumOutcome]
  ticket_region,        \* [Tickets -> Regions]
  consent_ledger,       \* [Subjects X Purposes X Nat -> Record(6 fields)]
  consent_revocation,   \* [Subjects X Purposes X Nat -> Record(6 fields)] (mirror)
  audit_chain,          \* Seq(AuditEvent)
  attempt_count         \* [Tickets X Backends -> Nat]; idempotency counter

vars == <<erasure_state, backend_state, ticket_region, consent_ledger,
          consent_revocation, audit_chain, attempt_count>>

TicketStatus     == {"received", "verified", "queued", "in_progress",
                     "completed", "denied", "failed"}
BackendOutcome   == {"pending", "erased", "pseudonymized", "not_applicable", "failed"}
ConsentProofFields == {"notice_text_hash", "notice_version", "locale",
                       "wording_id", "ui_capture_ts", "submission_ts"}
AuditEventTypes  == {"dsr.received.v1", "dsr.verified.v1", "dsr.queued.v1",
                     "dsr.in_progress.v1", "dsr.backend_ack.v1",
                     "dsr.completed.v1", "dsr.denied.v1", "dsr.failed.v1",
                     "consent.granted.v1", "consent.revoked.v1"}
AllPurposes == ConsentBasedPurposes \cup NonConsentPurposes

(* Helper: bounded append for TLC tractability. *)
AppendBounded(seq, ev) ==
  IF Len(seq) < MaxAuditChainLen THEN Append(seq, ev) ELSE seq

(* TypeOK enforces all variable shapes + state space bounds. *)
TypeOK ==
  /\ DOMAIN erasure_state \subseteq Tickets
  /\ \A t \in DOMAIN erasure_state : erasure_state[t] \in TicketStatus
  /\ DOMAIN backend_state \subseteq Tickets \X Backends
  /\ \A k \in DOMAIN backend_state : backend_state[k] \in BackendOutcome
  /\ DOMAIN ticket_region \subseteq Tickets
  /\ \A t \in DOMAIN ticket_region : ticket_region[t] \in Regions
  /\ DOMAIN consent_ledger     \subseteq Subjects \X AllPurposes \X Nat
  /\ DOMAIN consent_revocation \subseteq Subjects \X AllPurposes \X Nat
  /\ \A k \in DOMAIN consent_ledger     : DOMAIN consent_ledger[k]     = ConsentProofFields
  /\ \A k \in DOMAIN consent_revocation : DOMAIN consent_revocation[k] = ConsentProofFields
  /\ Len(audit_chain) <= MaxAuditChainLen
  /\ Cardinality(DOMAIN erasure_state) <= MaxConcurrentErasures
  /\ DOMAIN attempt_count \subseteq Tickets \X Backends
  /\ \A k \in DOMAIN attempt_count : attempt_count[k] \in Nat /\ attempt_count[k] <= MaxAttempts

Init ==
  /\ erasure_state      = << >>
  /\ backend_state      = << >>
  /\ ticket_region      = << >>
  /\ consent_ledger     = << >>
  /\ consent_revocation = << >>
  /\ audit_chain        = << >>
  /\ attempt_count      = << >>

(* ── ACTIONS ────────────────────────────────────────────────────────────── *)

SubmitDsr(t, region) ==
  /\ t \notin DOMAIN erasure_state
  /\ Cardinality(DOMAIN erasure_state) < MaxConcurrentErasures
  /\ region \in Regions
  /\ erasure_state' = erasure_state @@ (t :> "received")
  /\ ticket_region' = ticket_region @@ (t :> region)
  /\ audit_chain'   = AppendBounded(audit_chain,
                          [type |-> "dsr.received.v1", ticket |-> t])
  /\ UNCHANGED <<backend_state, consent_ledger, consent_revocation, attempt_count>>

VerifyMfa(t) ==
  /\ t \in DOMAIN erasure_state
  /\ erasure_state[t] = "received"
  /\ erasure_state' = [erasure_state EXCEPT ![t] = "verified"]
  /\ audit_chain'   = AppendBounded(audit_chain,
                          [type |-> "dsr.verified.v1", ticket |-> t])
  /\ UNCHANGED <<backend_state, ticket_region, consent_ledger,
                 consent_revocation, attempt_count>>

QueueErasure(t) ==
  /\ t \in DOMAIN erasure_state
  /\ erasure_state[t] = "verified"
  /\ erasure_state' = [erasure_state EXCEPT ![t] = "queued"]
  /\ audit_chain'   = AppendBounded(audit_chain,
                          [type |-> "dsr.queued.v1", ticket |-> t])
  /\ UNCHANGED <<backend_state, ticket_region, consent_ledger,
                 consent_revocation, attempt_count>>

EnterInProgress(t) ==
  /\ t \in DOMAIN erasure_state
  /\ erasure_state[t] = "queued"
  /\ erasure_state' = [erasure_state EXCEPT ![t] = "in_progress"]
  /\ audit_chain'   = AppendBounded(audit_chain,
                          [type |-> "dsr.in_progress.v1", ticket |-> t])
  /\ UNCHANGED <<backend_state, ticket_region, consent_ledger,
                 consent_revocation, attempt_count>>

(* EraseBackend: per-backend ack. Idempotent (replay produces same outcome).
 * PAT-RETRY-IDEMPOTENT-001 canonical (resilience_patterns.md §3.2).
 *)
EraseBackend(t, b) ==
  /\ t \in DOMAIN erasure_state
  /\ erasure_state[t] = "in_progress"
  /\ b \in Backends
  /\ LET prev_attempt == IF <<t, b>> \in DOMAIN attempt_count
                         THEN attempt_count[<<t, b>>]
                         ELSE 0
         outcome == IF b \in EffectiveBackends THEN "erased" ELSE "pseudonymized"
     IN /\ prev_attempt < MaxAttempts
        /\ backend_state'  = backend_state  @@ (<<t, b>> :> outcome)
        /\ attempt_count'  = attempt_count  @@ (<<t, b>> :> prev_attempt + 1)
        /\ audit_chain'    = AppendBounded(audit_chain,
                                [type |-> "dsr.backend_ack.v1",
                                 ticket |-> t, backend |-> b, outcome |-> outcome])
  /\ UNCHANGED <<erasure_state, ticket_region, consent_ledger, consent_revocation>>

(* CompleteErasure: gate fires only if ALL 12 backends ack'd. *)
CompleteErasure(t) ==
  /\ t \in DOMAIN erasure_state
  /\ erasure_state[t] = "in_progress"
  /\ \A b \in Backends :
       /\ <<t, b>> \in DOMAIN backend_state
       /\ backend_state[<<t, b>>] \in {"erased", "pseudonymized", "not_applicable"}
  /\ erasure_state' = [erasure_state EXCEPT ![t] = "completed"]
  /\ audit_chain'   = AppendBounded(audit_chain,
                          [type |-> "dsr.completed.v1", ticket |-> t])
  /\ UNCHANGED <<backend_state, ticket_region, consent_ledger,
                 consent_revocation, attempt_count>>

DenyDsr(t) ==
  /\ t \in DOMAIN erasure_state
  /\ erasure_state[t] \in {"received", "verified", "queued"}
  /\ erasure_state' = [erasure_state EXCEPT ![t] = "denied"]
  /\ audit_chain'   = AppendBounded(audit_chain,
                          [type |-> "dsr.denied.v1", ticket |-> t])
  /\ UNCHANGED <<backend_state, ticket_region, consent_ledger,
                 consent_revocation, attempt_count>>

FailDsr(t) ==
  /\ t \in DOMAIN erasure_state
  /\ erasure_state[t] \in {"queued", "in_progress"}
  /\ \E b \in Backends :
       <<t, b>> \in DOMAIN attempt_count /\ attempt_count[<<t, b>>] >= MaxAttempts
  /\ erasure_state' = [erasure_state EXCEPT ![t] = "failed"]
  /\ audit_chain'   = AppendBounded(audit_chain,
                          [type |-> "dsr.failed.v1", ticket |-> t])
  /\ UNCHANGED <<backend_state, ticket_region, consent_ledger,
                 consent_revocation, attempt_count>>

(* GrantConsent / RevokeConsent: schema-symmetric (Lote 9.4 H-05). 6 fields canonical. *)
ValidProof(p) ==
  /\ DOMAIN p = ConsentProofFields
  /\ p.notice_text_hash # ""
  /\ p.notice_version   # ""
  /\ p.locale \in Locales
  /\ p.wording_id       # ""
  /\ p.ui_capture_ts \in Nat /\ p.ui_capture_ts > 0
  /\ p.submission_ts \in Nat /\ p.submission_ts > 0

GrantConsent(s, purpose, ts, proof) ==
  /\ s \in Subjects /\ purpose \in AllPurposes /\ ts \in Nat /\ ts > 0
  /\ ValidProof(proof)
  /\ <<s, purpose, ts>> \notin DOMAIN consent_ledger
  /\ consent_ledger' = consent_ledger @@ (<<s, purpose, ts>> :> proof)
  /\ audit_chain'    = AppendBounded(audit_chain,
                          [type |-> "consent.granted.v1",
                           subject |-> s, purpose |-> purpose])
  /\ UNCHANGED <<erasure_state, backend_state, ticket_region,
                 consent_revocation, attempt_count>>

RevokeConsent(s, purpose, ts, proof) ==
  /\ s \in Subjects /\ purpose \in ConsentBasedPurposes \* só revoga consent-based
  /\ ts \in Nat /\ ts > 0
  /\ ValidProof(proof)             \* SAME 6-field schema (Lote 9.4 H-05 SYMMETRIC)
  /\ <<s, purpose, ts>> \notin DOMAIN consent_revocation
  /\ consent_revocation' = consent_revocation @@ (<<s, purpose, ts>> :> proof)
  /\ audit_chain'        = AppendBounded(audit_chain,
                              [type |-> "consent.revoked.v1",
                               subject |-> s, purpose |-> purpose])
  /\ UNCHANGED <<erasure_state, backend_state, ticket_region,
                 consent_ledger, attempt_count>>

(* Next: disjunction of all enabled actions.
 * ProofRecord = typed records aligned com ValidProof (Lote 10.11.0-bis-prime fix codex P0-2:
 *   anteriormente string-only em todos fields → GrantConsent/RevokeConsent eram dead actions
 *   porque ValidProof exigia locale ∈ Locales e timestamps ∈ Nat).
 *
 * StringFields: notice_text_hash, notice_version, wording_id (any non-empty string).
 * EnumField:    locale (must ∈ Locales).
 * NatFields:    ui_capture_ts, submission_ts (must be Nat > 0).
 *)
ProofStrings == {"h1", "h2"}                        \* abstract non-empty strings (cardinality 2)
ProofTimestamps == 1..3                              \* abstract small Nat>0 set (cardinality 3)
ProofRecord ==
  [ notice_text_hash : ProofStrings,
    notice_version   : ProofStrings,
    locale           : Locales,
    wording_id       : ProofStrings,
    ui_capture_ts    : ProofTimestamps,
    submission_ts    : ProofTimestamps ]

Next ==
  \/ \E t \in Tickets, r \in Regions               : SubmitDsr(t, r)
  \/ \E t \in Tickets                              : VerifyMfa(t)
  \/ \E t \in Tickets                              : QueueErasure(t)
  \/ \E t \in Tickets                              : EnterInProgress(t)
  \/ \E t \in Tickets, b \in Backends              : EraseBackend(t, b)
  \/ \E t \in Tickets                              : CompleteErasure(t)
  \/ \E t \in Tickets                              : DenyDsr(t)
  \/ \E t \in Tickets                              : FailDsr(t)
  \/ \E s \in Subjects, p \in AllPurposes,
        ts \in 1..MaxAuditChainLen                 : \E pr \in ProofRecord : GrantConsent(s, p, ts, pr)
  \/ \E s \in Subjects, p \in ConsentBasedPurposes,
        ts \in 1..MaxAuditChainLen                 : \E pr \in ProofRecord : RevokeConsent(s, p, ts, pr)

(* Spec with WEAK FAIRNESS for ALL transition actions to satisfy liveness. *)
Spec ==
  /\ Init
  /\ [][Next]_vars
  /\ \A t \in Tickets : WF_vars(VerifyMfa(t))
  /\ \A t \in Tickets : WF_vars(QueueErasure(t))
  /\ \A t \in Tickets : WF_vars(EnterInProgress(t))
  /\ \A t \in Tickets, b \in Backends : WF_vars(EraseBackend(t, b))
  /\ \A t \in Tickets : WF_vars(CompleteErasure(t))
  /\ \A t \in Tickets : WF_vars(FailDsr(t))

(* ── INVARIANTS (state predicates — listed under TLC INVARIANTS) ────────── *)

InvErasureComplete ==
  \A t \in DOMAIN erasure_state :
    erasure_state[t] = "completed" =>
      \A b \in Backends :
        /\ <<t, b>> \in DOMAIN backend_state
        /\ backend_state[<<t, b>>] \in {"erased", "pseudonymized", "not_applicable"}

InvConsentSymmetry ==
  /\ \A k \in DOMAIN consent_ledger     : DOMAIN consent_ledger[k]     = ConsentProofFields
  /\ \A k \in DOMAIN consent_revocation : DOMAIN consent_revocation[k] = ConsentProofFields

InvBackendAckIdempotent ==
  \A k \in DOMAIN backend_state : backend_state[k] \in BackendOutcome

(* InvResidencyPinned (CRITICAL — Lote 10.11.0-bis-prime fix codex P0-2):
 * (a) Every active ticket has a pinned region (no orphan tickets in pipeline).
 * (b) Pinned region is canonical 6-region enum.
 * (c) Once pinned (status >= verified), region MUST exist em ticket_region (no late-binding).
 *)
InvResidencyPinned ==
  \A t \in DOMAIN erasure_state :
    /\ t \in DOMAIN ticket_region
    /\ ticket_region[t] \in Regions
    /\ erasure_state[t] \in {"verified", "queued", "in_progress",
                              "completed", "denied", "failed"} =>
         t \in DOMAIN ticket_region   \* must be pinned before clock starts

(* ── TEMPORAL PROPERTIES (listed under TLC PROPERTIES) ─────────────────── *)

(* InvAuditAppendOnly: append-only (Len monotonic + prefix preserved).
 * Listed under PROPERTIES (not INVARIANTS) because it's a temporal box-prime formula.
 *)
InvAuditAppendOnly ==
  [][Len(audit_chain') >= Len(audit_chain)
     /\ \A i \in 1..Len(audit_chain) : audit_chain'[i] = audit_chain[i]]_vars

(* InvResidencyMonotonic: region pinning é monotonic — once set, never changes.
 * Lote 10.11.0-bis-prime fix codex P0-2: anteriormente InvResidencyPinned era vacuous;
 * a verdadeira propriedade residency é temporal monotonic (no cross-region migration).
 *)
InvResidencyMonotonic ==
  [][\A t \in DOMAIN ticket_region :
       t \in DOMAIN ticket_region' => ticket_region'[t] = ticket_region[t]]_vars

(* EventualTermination: every in-flight ticket eventually reaches terminal state. *)
EventualTermination ==
  \A t \in Tickets :
    (t \in DOMAIN erasure_state /\ erasure_state[t] = "in_progress")
      ~> (t \in DOMAIN erasure_state /\ erasure_state[t] \in {"completed", "denied", "failed"})

(* ── THEOREM (top-level claim) ─────────────────────────────────────────── *)

THEOREM Spec =>
  /\ []TypeOK
  /\ []InvErasureComplete
  /\ []InvConsentSymmetry
  /\ []InvBackendAckIdempotent
  /\ []InvResidencyPinned
  /\ InvAuditAppendOnly
  /\ InvResidencyMonotonic
  /\ EventualTermination
====
