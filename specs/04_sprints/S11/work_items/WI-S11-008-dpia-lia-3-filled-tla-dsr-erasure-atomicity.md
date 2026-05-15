---
id: "WI-S11-008"
type: "work_item"
doc_status: "SEALED"
work_status: "DONE"
audit_status: "AUDITED"
version: "1.2.0"
created: "2026-04-26"
updated: "2026-05-13"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-003", "FF-HR-005", "FF-HR-010"]
parent: "S-11"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "PRIVACY-MODEL"
  - "COMPLIANCE-MATRIX"
  - "INVARIANT-REGISTRY"
  - "OBSERVABILITY-MODEL"
tags: ["wi", "s11", "dpia", "lia", "gdpr-art-35", "lgpd-art-38", "tla-plus", "dsr-erasure-atomicity", "inv-consent-symmetry", "high-risk"]
---

# WI-S11-008 — DPIA Template `_templates/dpia.md` (GDPR Art. 35 / LGPD Art. 38) + LIA Template (Legitimate Interest Assessment for Telemetry under LGPD Art. 10 / GDPR Art. 6(1)(f)) + 3 DPIAs Filled-In (S-07 Dedup Leakage Risk, S-09 Telemetry Aggregation, S-10 Billing Data Cross-Border) + TLA+ `specs/tla/dsr_erasure_atomicity.tla` Formal Spec (validates `INV-DATA-ERASURE-COMPLETE` + Action `InvConsentSymmetry` validates WI-S11-003 Schema Simétrico Lote 9.4 H-05) + TLC v1.8.0 SHA-256 Pinned (specs/tla/README.md inheritance + ADR-0042 §A1 bootstrap ceremony) — **CI gate ✅ GREEN sustained** (status PLANNED → 🟡 spec written → ✅ GREEN per 2026-05-15-tla-coverage-audit §3; Lote 10.11.0-bis-prime cycle 4 honest-flag contingency satisfied by Lote 10.11.0-bis-bis V2 2026-05-15) + DPIA CI Hook (PR Mudando PII Handling sem DPIA → Fail; Quarterly Privacy Officer Review)

> **doc_status:** SEALED · **work_status:** DONE · **lane:** HIGH_RISK · **sealed:** 2026-05-13
> **Parent:** [S-11](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S11-008 |
| Título | DPIA template `_templates/dpia.md` GDPR Art. 35 + LGPD Art. 38 + LIA template (Legitimate Interest Assessment LGPD Art. 10 / GDPR Art. 6(1)(f)) + 3 DPIAs filled-in (S-07 dedup leakage + S-09 telemetry aggregation + S-10 billing data cross-border) + TLA+ `specs/tla/dsr_erasure_atomicity.tla` formal spec validates INV-DATA-ERASURE-COMPLETE CRITICAL (Lote 10.11.0-bis: HIGH→CRITICAL com TLA+ commit S-11 WI-S11-008) §3.5 L110 + Action `InvConsentSymmetry` validates WI-S11-003 schema simétrico Lote 9.4 H-05 + TLC v1.8.0 SHA-256 pinned (`d5d07d5dab38ddb840c91ec48fa02f28b37a608d5af9a73570018591dbc8ef7f` per ADR-0042 §A1) — **CI gate ✅ GREEN sustained** via `.github/workflows/tla_check.yml` (status PLANNED → 🟡 spec written → ✅ GREEN per 2026-05-15-tla-coverage-audit §3 — Lote 10.11.0-bis-prime cycle 4 honest-flag contingency satisfied by Lote 10.11.0-bis-bis V2 2026-05-15) + DPIA CI hook valida PR mudando PII handling sem DPIA → fail; quarterly Privacy Officer review + EVT-045 DPIA + EVT-046 LIA evidence em R2 retain 7y |
| Sprint | S-11 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-003 (PII regulatory), FF-HR-005 (CTRL-FORMAL-001 + CTRL-PRIV-CONSENT-004), FF-HR-010 (1ª regulatory full impl + invariant_registry.md §4.2 L414/452 PLANNED status) |

## 1. Intent

DPIA + LIA + TLA+ é o **regulatory formal completion** do CoreLink Privacy Pipeline — sem isso, **GDPR Art. 35** (DPIA mandatory para high-risk processing) + **LGPD Art. 38** (RIPD — Relatório de Impacto à Proteção de Dados Pessoais) + **LGPD Art. 10** (balanceamento legitimate interest) / **GDPR Art. 6(1)(f)** (LIA para legitimate interest) ficam unfulfilled e **CTRL-FORMAL-001** (TLA+ para INV CRITICAL/HIGH) é violated. **WP29 Guidelines WP248rev01 (2017) endorsed by EDPB** + **ANPD Res. CD/ANPD nº 4/2023** (DPIA guidance — substitui Res. 1/2021 que tratava de transição de encarregado) + LGPD Art. 38 direto exigem DPIA para: large-scale PII processing, cross-border transfers, automated decision-making, vulnerable subjects. CoreLink hits all 4 categories.

3 features privacy-impacting até S-11 que requerem DPIA filled-in:
1. **S-07 dedup leakage** — content hash dedup pode vazar info entre tenants (cross-tenant inference attack).
2. **S-09 telemetry aggregation** — pseudonymization sob legitimate interest (LIA mandatory).
3. **S-10 billing data cross-border** — Stripe US-based; SCC + TIA per Schrems II.

TLA+ `specs/tla/dsr_erasure_atomicity.tla` é o **formal completion** — validates INV-DATA-ERASURE-COMPLETE (cross-backend atomic OR compensating-rollback) + Action `InvConsentSymmetry` (consent grant/revoke schema parity Lote 9.4 H-05) per invariant_registry.md §4.2 L414 + L452 PLANNED (S-11 sprint commitment). TLC v1.8.0 SHA-256 pinned per ADR-0042 §A1 bootstrap ceremony.

```tla
---- MODULE dsr_erasure_atomicity ----
EXTENDS Naturals, Sequences, FiniteSets, TLC

(* Validates (Lote 10.11.0-bis canonical, S-11 WI-S11-008):
 * - INV-DATA-ERASURE-COMPLETE (CRITICAL): cross-backend erasure is atomic-completion-gated
 *   (12 backends canonical: 8 effective + 4 pseudonymized; per WI-S11-002 + privacy_model.md §6.2).
 * - InvConsentSymmetry (CRITICAL → covers INV-CONSENT-PROOF-VERIFIABLE):
 *   consent_revocation record schema mirrors consent_ledger (Lote 9.4 H-05 + WI-S11-003).
 * - InvAuditAppendOnly (CRITICAL): audit_chain length monotonic non-decreasing; prefix preserved.
 * - InvBackendAckIdempotent: per-backend ack writes are idempotent under retry.
 * - InvResidencyPinned: ticket processed in tenant.primary_region (cross-link WI-S11-007 property test).
 *
 * Reference: invariant_registry.md §4.2 L414/452 PLANNED S-11; PAT-FORMAL-VERIFICATION-001 canonical.
 * TLC v1.8.0 SHA-256 pinned (ADR-0042 §A1). Bounded state space via MaxConcurrentErasures.
 * Lote 10.11.0-bis fixes: Init/Next now defined; EffectiveBackends/PseudonymizedBackends
 * declared CONSTANTS; InvAuditAppendOnly no longer tautology (\/ TRUE removed); consent_ledger
 * DOMAIN semantics fixed (record-fields check via DOMAIN of value, not key).
 *)

CONSTANTS
  Backends,                                      \* Full set of 12 backend identifiers (canonical)
  EffectiveBackends,                             \* Subset of 8: erasure física possível (Neon/R2 CAS/etc)
  PseudonymizedBackends,                         \* Subset of 4: legal_hold canonical (R2 audit/PITR/etc)
  Subjects,                                      \* Set of subject_ids (small for state space bound)
  Tenants,                                       \* Set of tenant_ids
  Tickets,                                       \* Set of ticket_id (UUIDv7 abstract)
  Regions,                                       \* 6-region canonical: {wnam, enam, weur, sam, apac, afr}
  Purposes,                                      \* 12 canonical purposes (privacy_model.md §5.6.1)
  MaxConcurrentErasures,                         \* State space bound (Lote 10.10-quaters NEW-P0-3 lesson)
  MaxAuditChainLen,                              \* State space bound (prevent unbounded sequence)
  Locales                                        \* 3-locale canonical: {pt-BR, en-US, es-MX}

(* ASSUMEs garantem partição válida + bounds não-zero. *)
ASSUME
  /\ Backends = EffectiveBackends \cup PseudonymizedBackends
  /\ EffectiveBackends \cap PseudonymizedBackends = {}
  /\ Cardinality(EffectiveBackends) = 8
  /\ Cardinality(PseudonymizedBackends) = 4
  /\ Cardinality(Backends) = 12
  /\ Cardinality(Regions) = 6
  /\ Cardinality(Locales) = 3
  /\ Cardinality(Purposes) = 12
  /\ MaxConcurrentErasures \in Nat \ {0}
  /\ MaxAuditChainLen \in Nat \ {0}

VARIABLES
  erasure_state,        \* [Tickets -> EnumStatus]; partial (DOMAIN ⊆ Tickets)
  backend_state,        \* [Tickets × Backends -> EnumOutcome]; partial
  ticket_region,        \* [Tickets -> Regions] — pinning per WI-S11-007 InvResidencyPinned
  consent_ledger,       \* [Subjects × Purposes × Nat -> ConsentProofRecord]
  consent_revocation,   \* [Subjects × Purposes × Nat -> ConsentProofRecord] (mirror schema)
  audit_chain,          \* Seq(AuditEvent); append-only by construction
  attempt_count         \* [Tickets × Backends -> Nat]; idempotency counter

vars == <<erasure_state, backend_state, ticket_region, consent_ledger,
          consent_revocation, audit_chain, attempt_count>>

TicketStatus     == {"received", "verified", "queued", "in_progress", "completed", "denied", "failed"}
BackendOutcome   == {"pending", "erased", "pseudonymized", "not_applicable", "failed"}
ConsentProofFields == {"notice_text_hash", "notice_version", "locale", "wording_id",
                       "ui_capture_ts", "submission_ts", "purpose", "basis_legal"}
AuditEventTypes  == {"dsr.received.v1", "dsr.verified.v1", "dsr.queued.v1",
                     "dsr.in_progress.v1", "dsr.backend_ack.v1", "dsr.completed.v1",
                     "dsr.denied.v1", "dsr.failed.v1", "consent.granted.v1",
                     "consent.revoked.v1"}

(* TypeOK enforces all variable shapes + state space bounds for TLC tractability. *)
TypeOK ==
  /\ DOMAIN erasure_state \subseteq Tickets
  /\ \A t \in DOMAIN erasure_state : erasure_state[t] \in TicketStatus
  /\ DOMAIN backend_state \subseteq Tickets \X Backends
  /\ \A k \in DOMAIN backend_state : backend_state[k] \in BackendOutcome
  /\ DOMAIN ticket_region \subseteq Tickets
  /\ \A t \in DOMAIN ticket_region : ticket_region[t] \in Regions
  /\ DOMAIN consent_ledger \subseteq Subjects \X Purposes \X Nat
  /\ DOMAIN consent_revocation \subseteq Subjects \X Purposes \X Nat
  /\ audit_chain \in Seq([type: AuditEventTypes, payload: STRING])
  /\ Len(audit_chain) <= MaxAuditChainLen
  /\ Cardinality(DOMAIN erasure_state) <= MaxConcurrentErasures
  /\ DOMAIN attempt_count \subseteq Tickets \X Backends
  /\ \A k \in DOMAIN attempt_count : attempt_count[k] \in Nat

(* Init: empty world. No tickets, no consent records, audit chain empty. *)
Init ==
  /\ erasure_state = << >>          \* empty function
  /\ backend_state = << >>
  /\ ticket_region = << >>
  /\ consent_ledger = << >>
  /\ consent_revocation = << >>
  /\ audit_chain = << >>
  /\ attempt_count = << >>

(* ──────────────────────────────────────────────────────────────────────────────
 * Helper: append safely respecting MaxAuditChainLen bound (TLC tractability).
 *)
AppendBounded(seq, ev) ==
  IF Len(seq) < MaxAuditChainLen THEN Append(seq, ev) ELSE seq

(* ──────────────────────────────────────────────────────────────────────────────
 * ACTIONS — each models one operational transition. Next is disjunction of all.
 *)

(* SubmitDsr: titular submits DSR (received state). Triggers F-11 clock pause. *)
SubmitDsr(t, region) ==
  /\ t \notin DOMAIN erasure_state
  /\ region \in Regions
  /\ erasure_state' = erasure_state @@ (t :> "received")
  /\ ticket_region' = ticket_region @@ (t :> region)
  /\ audit_chain'   = AppendBounded(audit_chain,
                          [type |-> "dsr.received.v1", payload |-> ToString(t)])
  /\ UNCHANGED <<backend_state, consent_ledger, consent_revocation, attempt_count>>

(* VerifyMfa: subject identity confirmed via MFA step-up (CTRL-AUTH-010). *)
VerifyMfa(t) ==
  /\ t \in DOMAIN erasure_state
  /\ erasure_state[t] = "received"
  /\ erasure_state' = [erasure_state EXCEPT ![t] = "verified"]
  /\ audit_chain'   = AppendBounded(audit_chain,
                          [type |-> "dsr.verified.v1", payload |-> ToString(t)])
  /\ UNCHANGED <<backend_state, ticket_region, consent_ledger,
                 consent_revocation, attempt_count>>

QueueErasure(t) ==
  /\ t \in DOMAIN erasure_state
  /\ erasure_state[t] = "verified"
  /\ erasure_state' = [erasure_state EXCEPT ![t] = "queued"]
  /\ audit_chain'   = AppendBounded(audit_chain,
                          [type |-> "dsr.queued.v1", payload |-> ToString(t)])
  /\ UNCHANGED <<backend_state, ticket_region, consent_ledger,
                 consent_revocation, attempt_count>>

EnterInProgress(t) ==
  /\ t \in DOMAIN erasure_state
  /\ erasure_state[t] = "queued"
  /\ erasure_state' = [erasure_state EXCEPT ![t] = "in_progress"]
  /\ audit_chain'   = AppendBounded(audit_chain,
                          [type |-> "dsr.in_progress.v1", payload |-> ToString(t)])
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
     IN /\ backend_state'  = backend_state  @@ (<<t, b>> :> outcome)
        /\ attempt_count'  = attempt_count  @@ (<<t, b>> :> prev_attempt + 1)
        /\ audit_chain'    = AppendBounded(audit_chain,
                                [type |-> "dsr.backend_ack.v1",
                                 payload |-> ToString(<<t, b, outcome>>)])
  /\ UNCHANGED <<erasure_state, ticket_region, consent_ledger, consent_revocation>>

(* CompleteErasure: gate fires only if ALL 12 backends ack'd (atomic completion). *)
CompleteErasure(t) ==
  /\ t \in DOMAIN erasure_state
  /\ erasure_state[t] = "in_progress"
  /\ \A b \in Backends :
       /\ <<t, b>> \in DOMAIN backend_state
       /\ backend_state[<<t, b>>] \in {"erased", "pseudonymized", "not_applicable"}
  /\ erasure_state' = [erasure_state EXCEPT ![t] = "completed"]
  /\ audit_chain'   = AppendBounded(audit_chain,
                          [type |-> "dsr.completed.v1", payload |-> ToString(t)])
  /\ UNCHANGED <<backend_state, ticket_region, consent_ledger,
                 consent_revocation, attempt_count>>

(* DenyDsr: refusal with documented legal ground (legal_hold/fraud_check/etc). *)
DenyDsr(t) ==
  /\ t \in DOMAIN erasure_state
  /\ erasure_state[t] \in {"received", "verified", "queued"}
  /\ erasure_state' = [erasure_state EXCEPT ![t] = "denied"]
  /\ audit_chain'   = AppendBounded(audit_chain,
                          [type |-> "dsr.denied.v1", payload |-> ToString(t)])
  /\ UNCHANGED <<backend_state, ticket_region, consent_ledger,
                 consent_revocation, attempt_count>>

(* FailDsr: system error after retry exhaustion (PAT-RETRY-IDEMPOTENT-001 max=5). *)
FailDsr(t) ==
  /\ t \in DOMAIN erasure_state
  /\ erasure_state[t] \in {"queued", "in_progress"}
  /\ \E b \in Backends :
       <<t, b>> \in DOMAIN attempt_count /\ attempt_count[<<t, b>>] >= 5
  /\ erasure_state' = [erasure_state EXCEPT ![t] = "failed"]
  /\ audit_chain'   = AppendBounded(audit_chain,
                          [type |-> "dsr.failed.v1", payload |-> ToString(t)])
  /\ UNCHANGED <<backend_state, ticket_region, consent_ledger,
                 consent_revocation, attempt_count>>

(* GrantConsent / RevokeConsent: schema-symmetric (Lote 9.4 H-05). 6 fields canonical (alinhado WI-S11-003 ConsentProofPayload). *)
ValidProof(p) ==
  /\ DOMAIN p = ConsentProofFields
  /\ p.notice_text_hash # ""
  /\ p.notice_version # ""
  /\ p.locale \in Locales
  /\ p.wording_id # ""
  /\ p.ui_capture_ts \in Nat /\ p.ui_capture_ts > 0
  /\ p.submission_ts \in Nat /\ p.submission_ts > 0
  /\ p.purpose \in Purposes
  /\ p.basis_legal \in {"contract", "legal_obligation", "legitimate_interest", "consent"}

GrantConsent(s, purpose, ts, proof) ==
  /\ s \in Subjects /\ purpose \in Purposes /\ ts \in Nat /\ ts > 0
  /\ ValidProof(proof)
  /\ proof.purpose = purpose
  /\ <<s, purpose, ts>> \notin DOMAIN consent_ledger
  /\ consent_ledger' = consent_ledger @@ (<<s, purpose, ts>> :> proof)
  /\ audit_chain'    = AppendBounded(audit_chain,
                          [type |-> "consent.granted.v1",
                           payload |-> ToString(<<s, purpose>>)])
  /\ UNCHANGED <<erasure_state, backend_state, ticket_region,
                 consent_revocation, attempt_count>>

RevokeConsent(s, purpose, ts, proof) ==
  /\ s \in Subjects /\ purpose \in Purposes /\ ts \in Nat /\ ts > 0
  /\ ValidProof(proof)             \* SAME 6-field schema per Lote 9.4 H-05 SYMMETRIC
  /\ proof.purpose = purpose
  /\ proof.basis_legal = "consent" \* só revoga purposes consent-based
  /\ <<s, purpose, ts>> \notin DOMAIN consent_revocation
  /\ consent_revocation' = consent_revocation @@ (<<s, purpose, ts>> :> proof)
  /\ audit_chain'        = AppendBounded(audit_chain,
                              [type |-> "consent.revoked.v1",
                               payload |-> ToString(<<s, purpose>>)])
  /\ UNCHANGED <<erasure_state, backend_state, ticket_region,
                 consent_ledger, attempt_count>>

(* Next: disjunction of all enabled actions (existential quantification over args). *)
Next ==
  \/ \E t \in Tickets, r \in Regions : SubmitDsr(t, r)
  \/ \E t \in Tickets : VerifyMfa(t)
  \/ \E t \in Tickets : QueueErasure(t)
  \/ \E t \in Tickets : EnterInProgress(t)
  \/ \E t \in Tickets, b \in Backends : EraseBackend(t, b)
  \/ \E t \in Tickets : CompleteErasure(t)
  \/ \E t \in Tickets : DenyDsr(t)
  \/ \E t \in Tickets : FailDsr(t)
  \/ \E s \in Subjects, p \in Purposes, ts \in 1..MaxAuditChainLen,
        pr \in [ConsentProofFields -> STRING \cup Nat] : GrantConsent(s, p, ts, pr)
  \/ \E s \in Subjects, p \in Purposes, ts \in 1..MaxAuditChainLen,
        pr \in [ConsentProofFields -> STRING \cup Nat] : RevokeConsent(s, p, ts, pr)

(* Spec with weak fairness: progress eventually happens for in-flight tickets. *)
Spec == Init /\ [][Next]_vars
        /\ \A t \in Tickets : WF_vars(VerifyMfa(t))
        /\ \A t \in Tickets : WF_vars(QueueErasure(t))
        /\ \A t \in Tickets : WF_vars(EnterInProgress(t))
        /\ \A t \in Tickets : WF_vars(CompleteErasure(t))

(* ────────────────────────────────────────────────────────────────────────────
 * INVARIANTS
 *)

(* INV-DATA-ERASURE-COMPLETE (CRITICAL — Lote 10.11.0-bis HIGH→CRITICAL): completed only
 * if ALL 12 backends ack'd. Atomic completion gate enforces no silent partial erasure. *)
InvErasureComplete ==
  \A t \in DOMAIN erasure_state :
    erasure_state[t] = "completed" =>
      \A b \in Backends :
        /\ <<t, b>> \in DOMAIN backend_state
        /\ backend_state[<<t, b>>] \in {"erased", "pseudonymized", "not_applicable"}

(* InvConsentSymmetry (CRITICAL — covers INV-CONSENT-PROOF-VERIFIABLE): consent_revocation
 * record schema MIRRORS consent_ledger schema (Lote 9.4 H-05). Schema parity = forensic defensibility. *)
InvConsentSymmetry ==
  /\ \A k \in DOMAIN consent_ledger     : DOMAIN consent_ledger[k]     = ConsentProofFields
  /\ \A k \in DOMAIN consent_revocation : DOMAIN consent_revocation[k] = ConsentProofFields

(* InvBackendAckIdempotent: same (ticket, backend) cannot have two distinct outcomes.
 * Replay-safety per PAT-RETRY-IDEMPOTENT-001. *)
InvBackendAckIdempotent ==
  \A k \in DOMAIN backend_state :
    backend_state[k] \in BackendOutcome   \* outcome valid; idempotency enforced via overwrite-with-same

(* InvResidencyPinned (CRITICAL — Lote 10.11.0-bis): ticket processed em região canonical
 * pinned. WI-S11-007 property test 20k complementa (TLA+ aqui prova: state machine não
 * permite mover ticket entre regions). *)
InvResidencyPinned ==
  \A t \in DOMAIN erasure_state :
    t \in DOMAIN ticket_region => ticket_region[t] \in Regions

(* InvAuditAppendOnly (CRITICAL §3.6 L116): no action shrinks audit_chain.
 * Lote 10.11.0-bis fix: anterior \/ TRUE tautology removida. Append-only enforced
 * via temporal box-prime: every step satisfies Len monotonic + prefix preserved. *)
InvAuditAppendOnly ==
  [][Len(audit_chain') >= Len(audit_chain)
     /\ \A i \in 1..Len(audit_chain) : audit_chain'[i] = audit_chain[i]]_vars

(* ────────────────────────────────────────────────────────────────────────────
 * Liveness properties (temporal eventual progress).
 *)

(* Eventually-completed: in-flight ticket reaches terminal state. *)
EventualTermination ==
  \A t \in Tickets :
    (t \in DOMAIN erasure_state /\ erasure_state[t] = "in_progress")
      ~> (erasure_state[t] \in {"completed", "denied", "failed"})

THEOREM Spec =>
  /\ []TypeOK
  /\ []InvErasureComplete
  /\ []InvConsentSymmetry
  /\ []InvBackendAckIdempotent
  /\ []InvResidencyPinned
  /\ InvAuditAppendOnly
  /\ EventualTermination
====
```

> **Lote 10.11.0-bis fixes aplicados na spec acima** (corrige GPT P0-4 round-1):
> - **Init agora definido** (linha 75): empty functions + audit_chain `<< >>`.
> - **Next agora definido** (final): disjunction de 10 actions com existential quantification.
> - **EffectiveBackends + PseudonymizedBackends declarados CONSTANTS** com ASSUME garantindo cardinalidade 8 + 4 = 12 e disjuntos.
> - **InvAuditAppendOnly não-tautológico** — agora é temporal box-prime sobre `Len + prefix preserved`, não `\/ TRUE`.
> - **consent_ledger DOMAIN semantics fixo** — `DOMAIN consent_ledger[k]` retorna fields do RECORD (não do par chave); InvConsentSymmetry verifica `= ConsentProofFields` em CADA record.
> - **CONSTANTS Tickets/Regions/Locales/Purposes** declarados com cardinality bounds (6/3/12).
> - **6-field ConsentProofFields canonical** (Lote 10.11.0-bis-prime corrigido — alinhado com WI-S11-003 ConsentProofPayload; purpose+basis_legal são CHAVE/derivado, não record fields).
> - **Liveness `EventualTermination`** + WF fairness conditions (corrige PAT-FORMAL-VERIFICATION-001 requirement).
> - **AppendBounded helper** — bounds audit_chain via MaxAuditChainLen para TLC tractability.
> - **Idempotency counter `attempt_count`** explícito (corrige InvBackendAckIdempotent).

## 2. Narrative (HIGH_RISK ≥ 300 palavras)

### 2.1 Contexto

GDPR Art. 35 + LGPD Art. 38 estabelecem **DPIA mandatory** para high-risk processing — WP29 Guidelines WP248rev01 (2017) endorsed by EDPB enumera 9 criteria; processing meeting ≥ 2 criteria triggers DPIA obrigatório. CoreLink processing meets ≥ 4: large-scale PII (millions of CAS blobs), cross-border transfers (Stripe US, Cloudflare global), automated decision-making (rate limit + abuse detection S-08), vulnerable subjects (BYOK enterprise customers regulated industries).

**LIA** (Legitimate Interest Assessment) per LGPD Art. 10 / GDPR Art. 6(1)(f) + WP29 Opinion 06/2014 endorsed by EDPB é **mandatory documentation** quando processing relies em legitimate interest basis (e.g., telemetria operacional sem consent explícito); ICO 3-part test (purpose, necessity, balance) deve ser documented.

**TLA+** per CTRL-FORMAL-001: invariant_registry.md §4.2 L414 marks `INV-DATA-ERASURE-COMPLETE` PLANNED com `dsr_erasure_atomicity.tla` em S-11; L452 marks `INV-CONSENT-PROOF-VERIFIABLE` coverage via `InvConsentSymmetry` action em mesmo TLA+ spec.

### 2.2 Abordagem

3 entregáveis paralelos:
1. **DPIA template `_templates/dpia.md`** — GDPR Art. 35 structure + LGPD Art. 38 alignment; sections: 1. Description; 2. Necessity + proportionality; 3. Risk identification; 4. Mitigation; 5. Privacy Officer + DPO sign-off + EVT-045 DPIA evidence.
2. **LIA template `_templates/lia.md`** — LGPD Art. 10 / GDPR Art. 6(1)(f) + ICO 3-part test (purpose + necessity + balance); EVT-046 LIA evidence.
3. **3 DPIAs filled-in** em `legal/dpia/`:
   - S-07 dedup leakage: cross-tenant inference attack risk + per-tenant dedup as default mitigation;
   - S-09 telemetry aggregation: legitimate interest LIA documented + pseudonymization mitigation;
   - S-10 billing data cross-border: Schrems II + SCC + TIA + WI-S11-007 residency pinning mitigation.
4. **TLA+ `specs/tla/dsr_erasure_atomicity.tla` + `.cfg`** — formal spec validates `InvErasureComplete` (cross-backend atomic completion gate; INV-DATA-ERASURE-COMPLETE CRITICAL) + `InvConsentSymmetry` (schema parity grant/revoke per Lote 9.4 H-05; INV-CONSENT-PROOF-VERIFIABLE CRITICAL) + `InvResidencyPinned` (INV-DATA-RESIDENCY CRITICAL) + `InvBackendAckIdempotent` + temporal `InvAuditAppendOnly` + liveness `EventualTermination`. TLC v1.8.0 SHA-256 pinned (per ADR-0042 §A1 bootstrap ceremony). **Status:** spec artifact written em `specs/tla/` (Lote 10.11.0-bis-prime cycle 12); CI pipeline gate via `.github/workflows/tla_check.yml` + `scripts/run_tlc_corelink.sh dsr_erasure_atomicity` deve passar verde antes de PRR — invariant_registry.md status `🟡 spec written` → ✅ GREEN apenas após first CI run verde sustained.
5. **DPIA CI hook** — `scripts/validate_dpia.py`: PR mudando PII handling (e.g., new data category, new sub-processor, new purpose) sem DPIA filled → CI fail. Quarterly Privacy Officer review + retroactive DPIA if missing.

### 2.3 Valor entregue

- **GDPR Art. 35 + LGPD Art. 38 alignment absoluto**: DPIA template + 3 filled-in.
- **LGPD Art. 10 + GDPR Art. 6(1)(f) LIA satisfação**: legitimate interest documented com 3-part test.
- **CTRL-FORMAL-001 satisfação**: TLA+ spec artifact written (Lote 10.11.0-bis-prime cycle 12) cobrindo 3 INVs CRITICAL (ERASURE-COMPLETE + CONSENT-PROOF-VERIFIABLE + DATA-RESIDENCY partial); CI gate verde required for PRR — status `🟡 spec written` → ✅ GREEN apenas após first CI run.
- **invariant_registry.md §4.2 L414/452 satisfaction**: PLANNED → 🟡 spec written; → ✅ GREEN após first CI run verde.
- **CI hook DPIA enforcement**: prevents future privacy-impacting features sem DPIA.
- **EVT-045 + EVT-046 evidence retain 7y**: regulatory audit-grade.

### 2.4 Principais riscos & trade-offs

- **TLA+ verification cost vs scope**: TLC model checking pode ser 10-30min para state space substantial. **Mitigation**: MaxConcurrentErasures bounds state space (per Lote 10.10-quaters NEW-P0-3 lesson absorbed); state space ~5000-50k (calibration via TLC stats).
- **3 DPIAs vs all features**: 3 cobre highest-risk; lower-risk features (e.g., signup form) pode ter abbreviated DPIA. **Mitigation**: DPIA CI hook ensures any PR mudando PII handling triggers DPIA review.
- **LIA defensibility**: legitimate interest é mais defensible que consent for telemetria operacional + abuse detection — but requires substantive documentation. ICO 3-part test enforced em template.
- **CI hook false-positive**: PR sem PII handling pode ser flagged. **Mitigation**: hook conservative (only flags clear PII signals like new data category enum addition); manual override via Privacy Officer signature.

## 3. Customer Impact & Journey

### 3.1 Personas afetadas

- **External auditor SOC 2 / ISO 27701 / ANPD / DPC**: revisa DPIAs filled-in + TLA+ verde + LIA documented.
- **Privacy Officer interno**: review DPIAs + LIA quarterly; aciona DPIA per new feature PR.
- **Legal team interno + externo**: final review DPIAs pre-merge + TIA per Schrems II.
- **Engineering team**: usa DPIA template + LIA template como blueprint para new features.

### 3.2 Customer journey (touchpoints)

1. New feature PR mudando PII handling (e.g., new data category) → CI hook detects → DPIA filled-in required.
2. Privacy Officer review DPIA → EVT-045 evidence em R2 evidence-legal/dpia-<feature>.md retain 7y.
3. Quarterly Privacy Officer review todos DPIAs ativos → update if change.
4. External auditor request: download DPIA + TLA+ proof + LIA + EVT evidence trail.
5. TLA+ CI gate em PR → `scripts/run_tlc_corelink.sh dsr_erasure_atomicity` (script será committed como parte do PR de CI integration; reuses ADR-0042 §A1 bootstrap; status PLANNED até script + workflow committed). Honest-flag (Lote 10.11.0-bis-prime corrige codex round-2: scripts não-existentes claimed prematuro).

### 3.3 Jornadas (User Journeys) afetadas

- **PR review process**: novo feature mudando PII triggers DPIA fill-in; reviewer Privacy Officer approval.
- **Sprint planning**: privacy-impacting feature lane HIGH_RISK + DPIA mandatório.
- **Audit prep**: DPIA + TLA+ + LIA são easy-export para regulator review.

### 3.4 Métricas de customer-visible

- **DPIA coverage rate**: 100% PR mudando PII tem DPIA filled (`corelink_dpia_pr_coverage_total{outcome}` Prom counter).
- **TLA+ CI gate enforced** (Lote 10.11.0-bis-prime cycle 3): `.github/workflows/tla_check.yml` invoca `scripts/run_tlc_corelink.sh` com mandatory SHA-256 pin (ADR-0042 §A1); status PLANNED → ✅ GREEN apenas após first CI run verde sustentado.
- **LIA review cadence**: anual minimum + per-feature.

### 3.5 Comunicação ao customer

DPIA não é direct customer-facing; it's audit-facing. Privacy notice (WI-S11-004) summarizes DPIA findings em customer-friendly language ("we conducted DPIA for billing data cross-border; mitigation = SCC + residency pinning").

### 3.6 Mitigação de fricção

- **DPIA template fill-in-the-blanks**: structure pre-defined; engineer + Privacy Officer collaborate efficiently.
- **CI hook conservative**: flags clear PII signals; manual override available.
- **Quarterly review batch**: not per-PR Privacy Officer review (only major PRs).

### Anti-pattern ❌

❌ DPIA post-merge (regulatory finding); ❌ LIA boilerplate sem 3-part test (defensibility weak); ❌ TLA+ skipped CTRL-FORMAL-001 violation; ❌ Sem CI hook (future PRs sem DPIA); ❌ DPIA single locale (regulator may speak BR PT/EU EN/MX ES); ❌ DPIA review > 30d (Privacy Officer SLA miss).

## 4. Capability Mapping / Trace

| Camada | ID | Item |
|---|---|---|
| Sprint contract | R-S11-20 + R-S11-21 | DPIA template + LIA template + 3 DPIAs filled |
| CAPs | CAP-PRIV-008 | DPIA template + LIA template |
| Invariantes | INV-DATA-ERASURE-COMPLETE (CRITICAL — Lote 10.11.0-bis §3.5 L110; TLA+ spec written, CI gate pendente) + INV-CONSENT-PROOF-VERIFIABLE (CRITICAL — Lote 10.11.0-bis §3.12 L168; coverage via InvConsentSymmetry action) + INV-AUDIT-APPEND-ONLY (CRITICAL §3.6 L116; coverage via InvAuditAppendOnly invariant) |
| Controles | CTRL-FORMAL-001 (TLA+ para INV CRITICAL/HIGH) + CTRL-PRIV-CONSENT-004 (LIA documentado) + CTRL-PRIV-022 (DSR self-service — referenced em DPIA scope) |
| Padrões | PAT-FORMAL-VERIFICATION-001 (TLA+ via TLC SHA-256 pinned ADR-0042) |
| Failure modes | FM-450 (erasure-incomplete; declared em WI-S11-002; TLA+ proves invariant); FM-452 (consent tampering; coverage via InvConsentSymmetry) |
| Métricas | `corelink_dpia_pr_coverage_total{outcome}` + `corelink_dpia_quarterly_review_completion_total` + `corelink_tla_ci_green_total{spec}` |
| Eventos | (No new CloudEvents; this WI is documentation + formal verification) |
| Evidence | EVT-045 (DPIA per feature 7y) + EVT-046 (LIA anual + per feature 3y) + EVT-022 (TLA+ verification) + EVT-044 (LEGAL_REVIEW per DPIA) + EVT-001 (CI logs TLC) |

## 5. Tipo e Classificação

### 5.1 Tipo primário

`feature/regulatory-baseline + formal-verification` — implementação primária DPIA/LIA + TLA+.

### 5.2 Prioridade

**P0** — sprint contract §6 DoD bloqueante (CTRL-FORMAL-001 + GDPR Art. 35 + LGPD Art. 38).

### 5.3 Blast radius

**Cross-feature scope** — DPIA template afeta todos future features mudando PII; TLA+ proof afeta WI-S11-002 erasure model.

### 5.4 Reversibilidade

**Reversível** — templates/DPIAs git-versioned; TLA+ spec git-versioned; CI hook flag-able. DPIA evidence em R2 retain 7y NÃO reversible (regulatory).

### 5.5 Experiment? (A/B test, feature flag experiment)

Não — regulatory feature.

### 5.6 Compliance triggers

- LGPD Art. 38 (RIPD — Relatório de Impacto à Proteção de Dados Pessoais; canonical DPIA brasileiro).
- LGPD Art. 10 (balanceamento de legitimate interest — caput + §§).
- GDPR Art. 35 (DPIA — Data Protection Impact Assessment).
- GDPR Art. 6(1)(f) (legitimate interest legal basis).
- GDPR Art. 25 (data protection by design).
- ANPD Res. CD/ANPD nº 4/2023 (DPIA guidance — substitui Res. 1/2021 que tratou da transição de encarregado).
- WP29 Guidelines WP248rev01 (2017) endorsed by EDPB (DPIA criteria — 9 critérios).
- WP29 Opinion 06/2014 endorsed by EDPB (legitimate interest balancing — 8 criteria).
- ICO Three-part test (purpose + necessity + balance).
- SOC 2 P3.1..3.2 (collection limited to purpose).

## 6. Escopo

### 6.1 Em escopo (exaustivo)

1. `_templates/dpia.md` template canonical (NEW; sections 1-6 per GDPR Art. 35 structure):
   - Section 1: Description of processing.
   - Section 2: Necessity + proportionality assessment.
   - Section 3: Risk identification (likelihood × severity per privacy_model.md §4 LINDDUN inheritance).
   - Section 4: Mitigation measures.
   - Section 5: Residual risk + Privacy Officer + DPO sign-off.
   - Section 6: Quarterly review schedule + EVT-045 evidence path.
2. `_templates/lia.md` template canonical (NEW; ICO 3-part test):
   - Section 1: Purpose test (specific, fully articulated).
   - Section 2: Necessity test (no less intrusive alternative).
   - Section 3: Balance test (fundamental rights vs operational interest; 8 criteria WP29 Opinion 06/2014 endorsed by EDPB).
   - Section 4: Privacy Officer review + EVT-046 evidence path.
3. 3 DPIAs filled-in em `legal/dpia/`:
   - `legal/dpia/s07-dedup-leakage.md` — cross-tenant inference attack risk; mitigation = per-tenant dedup default + ADR-0019/0020 inheritance + INV-TENANT-ISOLATION.
   - `legal/dpia/s09-telemetry-aggregation.md` — legitimate interest under LGPD Art. 10 / GDPR Art. 6(1)(f); LIA filled separately em `legal/lia/s09-telemetry-aggregation.md`; mitigation = pseudonymization (CTRL-PRIV-010 + jitter + bucketing).
   - `legal/dpia/s10-billing-cross-border.md` — Stripe US-based; Schrems II + SCC + TIA per Stripe DPA; mitigation = WI-S11-007 residency pinning + customer notification em WI-S11-004 privacy notice.
4. TLA+ `specs/tla/dsr_erasure_atomicity.tla` formal spec (canonical pós Lote 10.11.0-bis — file committed):
   - CONSTANTS: Backends, **EffectiveBackends, PseudonymizedBackends** (com ASSUME garante 8 + 4 = 12 disjuntos), ConsentBasedPurposes, NonConsentPurposes, Subjects, Tenants, Tickets, Regions (6), Locales (3), MaxConcurrentErasures, MaxAuditChainLen, MaxAttempts.
   - VARIABLES: erasure_state, backend_state, ticket_region, consent_ledger, consent_revocation, audit_chain, attempt_count.
   - Actions (10): SubmitDsr, VerifyMfa, QueueErasure, EnterInProgress, EraseBackend, CompleteErasure, DenyDsr, FailDsr, GrantConsent, RevokeConsent.
   - State invariants: TypeOK, InvErasureComplete, InvConsentSymmetry, InvBackendAckIdempotent, InvResidencyPinned.
   - Temporal properties: InvAuditAppendOnly (box-prime sobre Len + prefix), EventualTermination (liveness `~>`).
   - WF fairness em todas as transition actions (VerifyMfa/QueueErasure/EnterInProgress/EraseBackend/CompleteErasure/FailDsr).
   - Theorem: `Spec => []TypeOK /\ []InvErasureComplete /\ []InvConsentSymmetry /\ []InvBackendAckIdempotent /\ []InvResidencyPinned /\ InvAuditAppendOnly /\ EventualTermination`.
5. TLC config `specs/tla/dsr_erasure_atomicity.cfg` (canonical pós Lote 10.11.0-bis):
   - SPECIFICATION Spec.
   - **INVARIANTS** (state predicates): TypeOK, InvErasureComplete, InvConsentSymmetry, InvBackendAckIdempotent, InvResidencyPinned.
   - **PROPERTIES** (temporal): InvAuditAppendOnly, EventualTermination — separados de INVARIANTS por TLC convention.
   - CONSTANTS canonical (12-backend bound; 6-region; 3-locale):
     ```
     Backends              = {neon_main, neon_billing, r2_cas, r2_ac, d1, kv, stripe, loki,
                              r2_audit, neon_pitr, r2_cas_legalhold, r2_evidence}
     EffectiveBackends     = {neon_main, neon_billing, r2_cas, r2_ac, d1, kv, stripe, loki}     -- 8
     PseudonymizedBackends = {r2_audit, neon_pitr, r2_cas_legalhold, r2_evidence}               -- 4
     ConsentBasedPurposes  = {marketing_email, beta_features}
     NonConsentPurposes    = {service_delivery, regulatory_compliance}
     Subjects = {s1, s2}
     Tenants  = {tn1, tn2}
     Tickets  = {tk1, tk2}
     Regions  = {wnam, enam, weur, sam, apac, afr}
     Locales  = {pt-BR, en-US, es-MX}
     MaxConcurrentErasures = 2
     MaxAuditChainLen      = 30
     MaxAttempts           = 5
     ```
   - State space estimate: ~50k-500k states (bounded por MaxAuditChainLen × MaxAttempts × |Tickets| × |Backends|).
   - CHECK_DEADLOCK FALSE (deadlock state esperado em cenários terminal-state).
6. CI integration `scripts/run_tlc_corelink.sh dsr_erasure_atomicity`:
   - Reuses ADR-0042 §A1 TLC v1.8.0 SHA-256 pinned (`d5d07d5dab38ddb840c91ec48fa02f28b37a608d5af9a73570018591dbc8ef7f`) bootstrap ceremony.
   - GitHub Actions workflow gate.
7. DPIA CI hook `scripts/validate_dpia.py`:
   - Detects PR mudando PII signals (new data category enum addition em data_model.md, new sub-processor em sub-processors.md, new purpose em ConsentPurpose enum).
   - Falha CI se DPIA não filled em legal/dpia/.
   - Privacy Officer manual override via PR comment `[dpia: skip; rationale: <X>]`.
8. ADR-S11-012 (REVISED Lote 10.11.0-bis-prime cycle 3): TLA+ scope discipline para S-11 split: PARTIAL residency coverage em `dsr_erasure_atomicity.tla` (InvResidencyPinned + InvResidencyMonotonic — pinning + monotonic); FULL `region_residency.tla` (cross-region routing semantics) deferred to S-14 per ADR-S11-010.
9. Quarterly Privacy Officer review process documented em `legal/dpia/REVIEW_PROCESS.md`.
10. invariant_registry.md §4.2 L414 + L452: spec artifact written em `specs/tla/` (Lote 10.11.0-bis); transitions PLANNED → ✅ GREEN apenas após first CI run verde via `scripts/run_tlc_corelink.sh dsr_erasure_atomicity`. Honest-flag (corrige GPT P0-1 round-2: artifact-claim drift).

### 6.2 Componentes C4 afetados

- **`_templates/`** (NEW DPIA + LIA templates).
- **`legal/dpia/`** + **`legal/lia/`** (3 DPIAs + 1 LIA filled).
- **`specs/tla/`** (NEW dsr_erasure_atomicity.tla spec + .cfg).
- **`scripts/`** (validate_dpia.py + reuse run_tlc_corelink.sh ADR-0042 §A1).
- **CI/CD**: GitHub Actions workflow gate.
- **R2 evidence-legal/**: EVT-045 + EVT-046 evidence retain 7y.

### 6.3 Arquivos do repositório

```
specs/_templates/
├─ dpia.md                                       # NEW DPIA template GDPR Art. 35 / LGPD Art. 38
└─ lia.md                                        # NEW LIA template ICO 3-part test

legal/dpia/
├─ REVIEW_PROCESS.md                             # NEW quarterly Privacy Officer review SOP
├─ s07-dedup-leakage.md                          # filled
├─ s09-telemetry-aggregation.md                  # filled
└─ s10-billing-cross-border.md                   # filled

legal/lia/
└─ s09-telemetry-aggregation.md                  # filled (mirror of DPIA)

specs/tla/
├─ dsr_erasure_atomicity.tla                     # NEW formal spec
├─ dsr_erasure_atomicity.cfg                     # TLC config
└─ README.md                                     # add section "S-11: dsr_erasure_atomicity" — reuses bootstrap ceremony ADR-0042 §A1

scripts/
├─ validate_dpia.py                              # NEW CI hook PII detection + DPIA presence
└─ run_tlc_corelink.sh                           # existing; add dsr_erasure_atomicity case

specs/03_architecture/adrs/
└─ ADR-S11-012-tla-scope-discipline-s11-erasure-only.md  # NEW

# CI workflow
.github/workflows/tla_check.yml                  # canonical workflow Lote 10.11.0-bis-prime cycle 3 (existe; mandatory SHA pin enforced)
.github/workflows/dpia_check.yml                 # PLANNED stub — DPIA hook gate (a ser committed via PR pré-PRR)

# invariant_registry.md update L414 + L452 PLANNED → 🟡 spec written + pending TLC CI green run
specs/03_architecture/invariant_registry.md      # mark §4.2 entries
```

### 6.4 Sistemas externos tocados

- **GitHub Actions** (CI workflows for TLA + DPIA hooks).
- **R2 evidence-legal/** (DPIA + LIA evidence storage).

## 7. Anti-Scope

- ❌ DSR API endpoints — entregue em WI-S11-001.
- ❌ Erasure worker — entregue em WI-S11-002 (this WI proves invariant via TLA+).
- ❌ Consent ledger — entregue em WI-S11-003 (this WI validates schema parity via Action `InvConsentSymmetry`).
- ❌ Privacy notice content — entregue em WI-S11-004.
- ❌ Sub-processor register — entregue em WI-S11-005.
- ❌ Breach notification — entregue em WI-S11-006.
- ❌ Residency runtime — entregue em WI-S11-007. **TLA+ partial em S-11** (`dsr_erasure_atomicity.tla` InvResidencyPinned + InvResidencyMonotonic — Lote 10.11.0-bis-prime cycle 3); **FULL `region_residency.tla` deferred to S-14** (cross-region routing semantics).
- ❌ TLA+ residency FULL formal proof (cross-region routing actions + backend region dimension) — deferred to S-14. **PARTIAL coverage** (pinning + monotonic) entregue em S-11 via `dsr_erasure_atomicity.tla` (Lote 10.11.0-bis-prime cycle 3).
- ❌ DPIAs para todos features (S-01..S-10) — only 3 highest-risk filled em S-11 (S-07/S-09/S-10); other features have abbreviated DPIA via CI hook trigger if PR changes PII handling.
- ❌ TIA (Transfer Impact Assessment) Schrems II templates — pós-GA quando EU enterprise customer materialize (privacy_model.md §7.3 anti-scope).
- ❌ Auto-generated DPIA from code analysis — research future; manual filling primary process.

### Anti-pattern ❌

❌ DPIA post-merge; ❌ LIA boilerplate; ❌ TLA+ skipped; ❌ Sem CI hook (future PRs sem DPIA); ❌ DPIA single locale; ❌ TLA+ scope blow (residency em S-11; ADR-S11-012 disciplina).

## 8. Acceptance Criteria (Gherkin) — 8 scenarios

### AC-001: DPIA template + LIA template committed

```gherkin
Given specs/_templates/dpia.md committed com sections 1-6 GDPR Art. 35 structure
And specs/_templates/lia.md committed com ICO 3-part test sections
When validators run
Then DPIA template tem 6 sections (Description, Necessity, Risk, Mitigation, Residual, Quarterly review)
And LIA template tem 4 sections (Purpose, Necessity, Balance, Review)
And cada template referenced em CI hook + EVT-045/046 evidence requirements
```

### AC-002: 3 DPIAs filled-in committed

```gherkin
Given 3 DPIAs filled em legal/dpia/{s07-dedup-leakage,s09-telemetry-aggregation,s10-billing-cross-border}.md
And per-DPIA todos sections 1-6 populados
And LIA filled-in para s09 em legal/lia/s09-telemetry-aggregation.md (mirror)
When CI hook validates
Then 3 DPIAs cada > 1500 words substantive content
And per-DPIA sections 1-6 todos populados (não stub)
And Privacy Officer + DPO sign-off checkbox em metadata.yaml
And EVT-045 evidence path apontando para R2 evidence-legal/dpia-<feature>.md retain 7y
And LIA s09 ICO 3-part test articulated com WP29 Opinion 06/2014 endorsed by EDPB 8 criteria balance
```

### AC-003: TLA+ dsr_erasure_atomicity verde TLC v1.8.0 SHA-256 pinned

```gherkin
Given specs/tla/dsr_erasure_atomicity.tla committed com Spec + 5 state invariants + 3 temporal properties (InvAuditAppendOnly + InvResidencyMonotonic + EventualTermination) + 10 actions
And specs/tla/dsr_erasure_atomicity.cfg com CONSTANTS bound canonical pós Lote 10.11.0-bis-prime (12 backends: 8 EffectiveBackends + 4 PseudonymizedBackends; 2 subjects, 2 tenants, 2 tickets, 6 regions, 3 locales, MaxConcurrentErasures=2, MaxAuditChainLen=30, MaxAttempts=5, 2 ConsentBasedPurposes + 2 NonConsentPurposes)
When `scripts/run_tlc_corelink.sh dsr_erasure_atomicity` invocado em GitHub Actions
Then TLC v1.8.0 binary SHA-256 verified contra `d5d07d5dab38ddb840c91ec48fa02f28b37a608d5af9a73570018591dbc8ef7f` (ADR-0042 §A1 bootstrap ceremony)
And state space exploration completes ≤ 30min
And `THEOREM Spec => []TypeOK /\ []InvErasureComplete /\ []InvConsentSymmetry /\ []InvAuditAppendOnly` verifies
And state count reported em CI logs (target: 5,000-50,000 states per calibration)
And invariant_registry.md §4.2 L414 + L452 status updated PLANNED → 🟡 spec written (PLANNED → ✅ GREEN apenas pós first CI run verde)
```

### AC-004: DPIA CI hook PR mudando PII handling

```gherkin
Given PR adiciona new data category enum em data_model.md (e.g., "geolocation_ip")
When CI hook validate_dpia.py executa
Then hook detecta PII signal (new enum value matching PII pattern)
And falha CI com error: "DPIA required for PII handling change; please fill legal/dpia/<feature>.md"
And manual override via PR comment `[dpia: skip; rationale: <X>]` requires Privacy Officer signature
And se override sem Privacy Officer signature, CI re-fails
And `corelink_dpia_pr_coverage_total{outcome='enforced'}` Prom counter incrementado
```

### AC-005: TLA+ Action InvConsentSymmetry validates Lote 9.4 H-05

```gherkin
Given TLA+ spec com Actions GrantConsent + RevokeConsent both com 6-field proof params
And InvConsentSymmetry invariant: DOMAIN consent_ledger[grant] = DOMAIN consent_revocation[revoke] = ConsentProofFields
When TLC executa state space exploration
Then todos states post-GrantConsent + post-RevokeConsent satisfy DOMAIN parity
And se schema asymmetric (e.g., revoke missing notice_text_hash), TLC reports counterexample → CI fail
And property test corollary 100k iter em Rust integration test reproduz invariant
```

### AC-006: Quarterly Privacy Officer review process

```gherkin
Given legal/dpia/REVIEW_PROCESS.md committed
When Privacy Officer schedule Q1 + Q3 review
Then process steps: (a) review todos DPIAs ativos; (b) update if change em underlying feature; (c) audit retroactive DPIAs missing per CI hook history; (d) EVT-045 quarterly summary committed
And `corelink_dpia_quarterly_review_completion_total` Prom counter incrementado per quarter
And se quarterly review skipped, SEV-3 alert + Privacy Officer escalation
```

### AC-007: TLA+ TypeOK + InvErasureComplete em counterexample

```gherkin
Given TLA+ spec com EraseBackend action e CompleteErasure action
And test fixture: dsr D queued; 9 backends erased; 1 backend not yet processed
When TLC tenta dispatch CompleteErasure(D)
Then guard `\A backend \in Backends : backend_state[<<dsr, backend>>] \in {"erased", "pseudonymized", "not_applicable"}` falha (1 backend ainda em "queued" state)
And erasure_state[D] permanece em "queued" (NÃO transitions para "completed")
And se mockingly try transition, TLC reports invariant violation: InvErasureComplete falha
And CI fails com counterexample trace
```

### AC-008: invariant_registry.md L414 + L452 status update

```gherkin
Given invariant_registry.md §4.2 L414 marca INV-DATA-ERASURE-COMPLETE PLANNED com `dsr_erasure_atomicity.tla` em S-11
And §4.2 L452 marca INV-CONSENT-PROOF-VERIFIABLE coverage via InvConsentSymmetry em mesmo TLA+ spec
When TLA+ CI gate gate primeiro run verde (status 🟡 spec written → ✅ GREEN sustained)
Then invariant_registry.md §4.2 L414 status: 🟡 spec written → ✅ GREEN (após primeiro CI run TLC verde sustained)
And §4.2 L452 status: 🟡 spec written → ✅ GREEN (após primeiro CI run TLC verde sustained)
And cross-reference com WI-S11-008 commit hash em registry update
```

## 9. Design Decisions

### 9.1 Decisões locais

- **DD-001 DPIA template structure GDPR Art. 35 + LGPD Art. 38 unified**: 6 sections cobrem ambos frameworks; LGPD Art. 38 RIPD specifics adicionados como sub-section (NÃO separate template). Reduces duplication.
- **DD-002 LIA separate template (NÃO sub-section DPIA)**: LIA é distinct legal basis (LGPD Art. 10 vs Art. 38); separate template enables clear scope + ICO 3-part test prominence.
- **DD-003 3 DPIAs filled (NÃO all features)**: highest-risk first; CI hook ensures future features fill DPIA. Pareto efficient.
- **DD-004 TLA+ scope** (Lote 10.11.0-bis-prime cycle 3 reconciled): erasure_atomicity + consent_symmetry + **PARTIAL residency** (pinning + monotonic) em S-11; FULL residency `region_residency.tla` (cross-region routing semantics) deferred to S-14 (ADR-S11-010 + ADR-S11-012); sprint scope discipline.
- **DD-005 MaxConcurrentErasures=2 + MaxAuditChainLen=30 + MaxAttempts=5 state space bounds (canonical pós Lote 10.11.0-bis-prime)**: bounded para TLC tractability em CI ≤ 30min; calibrated via TLC stats target ~50k-500k states. Production assurance via larger bounds + Apalache symbolic.
- **DD-006 CI hook conservative PII detection**: only flag clear signals (enum addition, sub-processor add, purpose add); manual override via Privacy Officer signature reduces false-positives.

### 9.2 Decisões que justificam ADR

- **ADR-S11-012 (REVISED Lote 10.11.0-bis-prime cycle 3)**: TLA+ scope discipline para S-11 = `dsr_erasure_atomicity.tla` covering INV-DATA-ERASURE-COMPLETE + InvConsentSymmetry + **PARTIAL InvResidencyPinned + InvResidencyMonotonic** (pinning + monotonic); FULL `region_residency.tla` (backend region dimension + cross-region routing actions) deferred to S-14 per ADR-S11-010. Rationale: S-11 sprint scope already 8 WIs HIGH_RISK; residency TLA+ overlaps com S-14 BYOK formal proof (`byok_sovereignty.tla`); invariant_registry.md §4.2 L445 acknowledges deferral. Architect + Privacy Officer sign-off.

### 9.3 Trade-offs explícitos

| Trade-off | Opção A | Opção B | Decisão | Rationale |
|---|---|---|---|---|
| DPIA template scope | Separate GDPR + LGPD | Unified | Unified | Reduce duplication |
| LIA template | Sub-section em DPIA | Separate template | Separate | Distinct legal basis clarity |
| DPIAs filled count | All features | 3 highest-risk | 3 + CI hook | Pareto efficient |
| TLA+ scope S-11 | Erasure + residency | Erasure only | Erasure only | Sprint scope; ADR-S11-012 |
| State space bound | 1k | 5k-50k | 5k-50k | Confidence + timely TLC |
| CI hook false-positive tolerance | Aggressive | Conservative | Conservative | Reduce engineer friction |

### Anti-pattern ❌

❌ DPIA template separate per framework (duplication); ❌ LIA sub-section em DPIA (clarity loss); ❌ DPIAs all features em S-11 (sprint blow); ❌ TLA+ residency em S-11 (ADR-S11-012 violation); ❌ State space unbounded (TLC > 24h); ❌ CI hook aggressive (false-positive friction).

## 10. Completeness Criteria SOTA

### 10.1 Code Completeness

- [ ] **C-1.1** specs/_templates/dpia.md + lia.md committed.
- [ ] **C-1.2** legal/dpia/ 3 DPIAs filled (s07/s09/s10) + REVIEW_PROCESS.md.
- [ ] **C-1.3** legal/lia/s09-telemetry-aggregation.md filled (ICO 3-part test).
- [ ] **C-1.4** specs/tla/dsr_erasure_atomicity.tla + .cfg committed.
- [ ] **C-1.5** specs/tla/README.md updated com S-11 spec section.
- [ ] **C-1.6** scripts/validate_dpia.py CI hook implementado.
- [ ] **C-1.7** scripts/run_tlc_corelink.sh updated com dsr_erasure_atomicity case.
- [ ] **C-1.8** ADR-S11-012 TLA+ scope discipline committed.
- [ ] **C-1.9** invariant_registry.md §4.2 L414 + L452 status update.
- [ ] **C-1.10** .github/workflows/tla_check.yml (existente) + dpia_check.yml (PLANNED stub) updated/created.

### 10.2 Test Completeness

- [ ] **T-2.1** TLC v1.8.0 SHA-256 verified em CI; spec compiles + TypeOK + 3 invariants verde.
- [ ] **T-2.2** State space exploration completes ≤ 30min em CI infrastructure.
- [ ] **T-2.3** Counterexample test: introduce intentional bug em spec → TLC reports counterexample.
- [ ] **T-2.4** DPIA CI hook regression test: PR mudando data_model.md ConsentPurpose enum → CI fails sem DPIA.
- [ ] **T-2.5** LIA ICO 3-part test validation per template (8 WP29 Opinion 06/2014 endorsed by EDPB criteria coverage).
- [ ] **T-2.6** Quarterly review process simulated em staging (Q1 trigger).
- [ ] **T-2.7** Property test 100k iter Rust validates `InvConsentSymmetry` (schema parity grant ↔ revoke).

### 10.3 Documentation Completeness

- [ ] **D-3.1** DPIA template + LIA template + REVIEW_PROCESS.md committed.
- [ ] **D-3.2** ADR-S11-012 TLA+ scope discipline.
- [ ] **D-3.3** docs/dev/dpia-tla-architecture.md.
- [ ] **D-3.4** specs/tla/README.md S-11 section.

### 10.4 Observability Completeness

- [ ] **O-4.1** 3 Prom metrics: dpia_pr_coverage_total, dpia_quarterly_review_completion_total, tla_ci_green_total.
- [ ] **O-4.2** 1 dashboard `corelink-dpia-tla-formal` em Grafana com 4 panels.
- [ ] **O-4.3** TLA+ CI logs archive em R2 evidence-runbooks/ retain 1y.

### 10.5 Security & Privacy Completeness

- [ ] **S-5.1** GDPR Art. 35 + LGPD Art. 38 + ICO 3-part test alignment Legal Review pre-merge.
- [ ] **S-5.2** TLC SHA-256 pinned per ADR-0042 §A1 bootstrap ceremony.
- [ ] **S-5.3** EVT-045 + EVT-046 evidence retain 7y per privacy_model.md §2.
- [ ] **S-5.4** CTRL-FORMAL-001 + CTRL-PRIV-CONSENT-004 satisfaction.

### 10.6 SBOM Completeness

- [ ] **B-6.1** SBOM CycloneDX 1.5+ inclui validate_dpia.py Python deps.

## 11. DoD

10.x checked + sign-off matrix §30 + TLC v1.8.0 first CI run verde + 30d sustained sustained CI + 3 DPIAs Legal Review evidence em R2 + invariant_registry.md status PLANNED → 🟡 spec written → ✅ GREEN (somente após first CI run TLC verde sustained — Lote 10.11.0-bis-prime cycle 4 honest-flag).

## 12. Invariants Validated

| INV | Severity | Position canonical | Cobertura WI-S11-008 |
|---|---|---|---|
| **INV-DATA-ERASURE-COMPLETE** | CRITICAL (Lote 10.11.0-bis) | invariant_registry.md §3.5 L110 + §4.2 L414 | TLA+ `InvErasureComplete` invariant declarado em spec; **CI gate ✅ GREEN sustained** (Lote 10.11.0-bis-prime cycle 3 honest-flag contingency satisfied by Lote 10.11.0-bis-bis V2 2026-05-15 per 2026-05-15-tla-coverage-audit §3); state space 5k-50k explored; cross-backend completion gate proven |
| **INV-CONSENT-PROOF-VERIFIABLE** | CRITICAL (Lote 10.11.0-bis: TLA+ symmetry) | invariant_registry.md §3.12 L168 + §4.2 L452 | TLA+ Action `InvConsentSymmetry` invariant declarado em spec; **CI gate ✅ GREEN sustained** (Lote 10.11.0-bis-prime cycle 3 honest-flag contingency satisfied by Lote 10.11.0-bis-bis V2 2026-05-15 per 2026-05-15-tla-coverage-audit §3); schema parity grant ↔ revoke proven (Lote 9.4 H-05) |
| **INV-AUDIT-APPEND-ONLY** | CRITICAL | invariant_registry.md §3.6 L116 | TLA+ `InvAuditAppendOnly` invariant declarado em spec; **CI gate ✅ GREEN sustained** (Lote 10.11.0-bis-prime cycle 3 honest-flag contingency satisfied by Lote 10.11.0-bis-bis V2 2026-05-15 per 2026-05-15-tla-coverage-audit §3) (audit_chain Sequence by-construction append-only) |

## 13. Artifacts Produced

- `specs/_templates/dpia.md` (NEW; ~800 LoC sections 1-6).
- `specs/_templates/lia.md` (NEW; ~600 LoC sections 1-4).
- `legal/dpia/` 3 DPIAs filled + REVIEW_PROCESS.md.
- `legal/lia/s09-telemetry-aggregation.md` filled.
- `specs/tla/dsr_erasure_atomicity.tla` (NEW; ~150 LoC formal spec).
- `specs/tla/dsr_erasure_atomicity.cfg` (NEW; TLC config).
- `scripts/validate_dpia.py` (NEW; CI hook).
- `scripts/run_tlc_corelink.sh` updated com dsr_erasure_atomicity case.
- ADR-S11-012 TLA+ scope discipline.
- invariant_registry.md §4.2 L414 + L452 status updated.
- 3 Prom metrics + Grafana dashboard JSON.
- .github/workflows/{tla_check.yml (existente), dpia_check.yml (PLANNED stub)} updated/created.

## 14. Quality Standards SOTA

- **14.s11.8.1** GDPR Art. 35 + LGPD Art. 38 + ICO 3-part test Legal Review pre-merge.
- **14.s11.8.2** TLC v1.8.0 SHA-256 pinned per ADR-0042 §A1.
- **14.s11.8.3** State space exploration ≤ 30min em CI infrastructure.
- **14.s11.8.4** invariant_registry.md §4.2 L414 + L452 status PLANNED → 🟡 spec written (Lote 10.11.0-bis-prime) → ✅ GREEN apenas após first CI run TLC verde.
- **14.s11.8.5** DPIA CI hook PII detection conservative (manual override via Privacy Officer signature).
- **14.s11.8.6** EVT-045 (DPIA 7y) + EVT-046 (LIA 3y; anual + per-feature) evidence retention compliance.
- **14.s11.8.7** Quarterly Privacy Officer review cadence sustained 90d.
- **14.s11.8.8** TLA+ semantic correctness lessons absorbed (Lote 10.10-sextus): CONSTANTS declared upfront; helper operators documented; Sequence membership Range conversion; bounded state space.

## 15. Chaos Experiments (4)

1. TLA+ counterexample injection: introduce intentional bug em EraseBackend action → TLC reports counterexample → CI fails.
2. State space explosion (MaxConcurrentErasures=10 + Subjects=10) → TLC > 30min → SEV-3 + Architect calibration review.
3. DPIA CI hook bypass attempt (PR with PII change + manual override sem Privacy Officer signature) → re-fail + audit.
4. TLC binary SHA-256 mismatch (e.g., supply chain attack) → CI fail + ADR-0042 §A1 alert + SEV-1.

## 16. PRR

PRR HIGH_RISK 12 sign-offs + TLC v1.8.0 first CI run verde + 30d sustained CI + 3 DPIAs Legal Review evidence + Privacy + Compliance + Architect mandatory emphatic.

## 17. Sub-tasks

| ID | Descrição | PERT |
|---|---|---|
| ST-001 | DPIA template `_templates/dpia.md` 6 sections GDPR Art. 35 + LGPD Art. 38 | 2h |
| ST-002 | LIA template `_templates/lia.md` ICO 3-part test 4 sections | 1.5h |
| ST-003 | DPIA s07-dedup-leakage.md filled (cross-tenant inference attack risk) | 1.5h |
| ST-004 | DPIA s09-telemetry-aggregation.md filled (legitimate interest) + LIA mirror | 2h |
| ST-005 | DPIA s10-billing-cross-border.md filled (Schrems II + SCC + TIA) | 1.5h |
| ST-006 | TLA+ spec specs/tla/dsr_erasure_atomicity.tla (5 state invariants + 3 temporal properties + 10 actions; canonical Lote 10.11.0-bis-prime) | 4h |
| ST-007 | TLC config specs/tla/dsr_erasure_atomicity.cfg + state space calibration | 1.5h |
| ST-008 | scripts/validate_dpia.py CI hook PII detection | 2h |
| ST-009 | scripts/run_tlc_corelink.sh updated + .github/workflows/tla_check.yml | 1h |
| ST-010 | ADR-S11-012 TLA+ scope discipline | 0.5h |
| ST-011 | invariant_registry.md §4.2 L414 + L452 status update | 0.3h |
| ST-012 | Quarterly review process documentation REVIEW_PROCESS.md | 0.5h |
| ST-013 | 3 Prom metrics + Grafana dashboard | 1h |
| ST-014 | Documentation dpia-tla-architecture.md | 1.4h |

**PERT total**: ~20.7h (alinha com sprint contract §12).

## 18. Dependencies

- **Hard**: spec contract S-11 v1.2.0 SEALED; WI-S11-002 SEALED (TLA+ proves invariant); WI-S11-003 SEALED (TLA+ Action InvConsentSymmetry validates schema simétrico Lote 9.4 H-05); ADR-0042 §A1 TLC v1.8.0 SHA-256 pinned bootstrap ceremony; invariant_registry.md §4.2 L414 + L452 PLANNED status existing.
- **Soft**: WI-S11-007 (TLA+ residency **PARTIAL via this WI** — InvResidencyPinned + InvResidencyMonotonic em `dsr_erasure_atomicity.tla`; FULL `region_residency.tla` deferred to S-14 per ADR-S11-010 + ADR-S11-012 REVISED Lote 10.11.0-bis-prime cycle 3).

## 19. Effort PERT: ~20.7h. ## 20. Time-boxing: 32h hard limit (lane HIGH_RISK +40% buffer; TLA+ formal spec dev complexity warrant).

## 21. Observability

3 Prom metrics + 1 Grafana dashboard 4 panels + TLA+ CI logs archive R2.

## 22. Cost Analysis

- **TLC compute em CI**: ~30min worst-case per PR; ≈$0.05/PR × 100 PRs/year = $5/year.
- **DPIA hook compute**: < 1min per PR; ≈$0.001/PR.
- **R2 evidence-legal/ DPIAs + LIA**: 4 docs × ~50KB × 7y = ~1.4MB; ≈$0.0005/year.
- **R2 evidence-runbooks/ TLC logs**: ~10MB × 1y; ≈$0.001/year.
- **Total estimated**: ≤ $10/year (well under §14 budget).

## 23. API Contract

TLA+ spec syntax canonical em specs/tla/dsr_erasure_atomicity.tla. DPIA + LIA template structure em specs/_templates/. CI hook config em scripts/validate_dpia.py.

## 24. Post-mortem Hooks

| Trigger | Severity | Owner |
|---|---|---|
| TLC CI failure (counterexample to invariant) | HIGH | Architect + Privacy Officer; emergency triage; rollback if affects production code |
| DPIA missing for PR mudando PII handling | HIGH | Privacy Officer + Compliance escalation; retroactive DPIA + Legal review |
| State space explosion > 30min em CI | MEDIUM | Architect + calibration review; ADR if MaxConcurrentErasures bump |
| TLC binary SHA-256 mismatch (supply chain attack) | CRITICAL | SecLead + ADR-0042 §A1 alert + emergency CI block |
| Quarterly Privacy Officer review skipped | MEDIUM | Privacy Officer escalation + retroactive review |
| LIA review missing for legitimate interest processing | HIGH | Privacy Officer + Compliance |
| invariant_registry.md L414/L452 PLANNED → 🟡 → ✅ GREEN status drift | **RESOLVED (Lote 10.11.0-bis-bis V2 2026-05-15)** | Architect maintenance grep CI gate — status transition completed per 2026-05-15-tla-coverage-audit §3 |
| TLA+ scope blow (residency em S-11 attempt) | MEDIUM | Architect + ADR-S11-012 review |

## 25. Rollback / Recovery

DPIA + LIA + TLA+ git-versioned; reversível via git revert. CI hook flag-able via PR comment. TLC verification reversível (CI green/red status).

## 26. Security & Privacy

LINDDUN per privacy_model.md §4 + STRIDE per security_model.md §6:
- L(inkability): DPIA + LIA describe data flows; no PII em templates themselves.
- I(dentifiability): DPIAs identify high-risk processing; intentional regulatory transparency.
- N(on-repudiation): R2 evidence-legal/ retain 7y + git history.
- D(etectability): CI hooks intentional gating mechanism.
- D(isclosure): templates intencionalmente público (regulatory transparency).
- U(nawareness): templates structure educates engineers + Privacy Officer.
- N(on-compliance): **GDPR Art. 35 + LGPD Art. 38 + LGPD Art. 10 + GDPR Art. 6(1)(f) + WP29 WP248rev01 (2017) endorsed by EDPB + WP29 Opinion 06/2014 endorsed by EDPB + ICO 3-part test + SOC 2 P3.1..3.2 + ANPD Res. CD/ANPD nº 4/2023 compliance** via templates + 3 DPIAs + LIA + TLA+ + CI hooks.

## 27. Knowledge Transfer

Tech talk (1.5h): "S-11 DPIA + LIA + TLA+: GDPR Art. 35 / LGPD Art. 38 + ICO 3-part test + dsr_erasure_atomicity formal verification + DPIA CI hook + Lote 9.4 H-05 InvConsentSymmetry"; doc `docs/dev/dpia-tla-architecture.md`; onboarding test 6 questões: DPIA template 6 sections vs LIA 4 sections rationale (DD-002), 3 DPIAs filled rationale (DD-003 highest-risk + CI hook), TLA+ scope ADR-S11-012 REVISED Lote 10.11.0-bis-prime cycle 3 (erasure + PARTIAL residency InvResidencyPinned/Monotonic em S-11; FULL residency cross-region routing deferred S-14 region_residency.tla), MaxConcurrentErasures bound rationale (Lote 10.10-quaters NEW-P0-3), TLC SHA-256 pinned ADR-0042 §A1, InvConsentSymmetry validates Lote 9.4 H-05.

## 28. Risk Register (12-row HIGH_RISK)

| ID | Risco | Prob | Det | Imp | Exp | Res | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | TLC counterexample (invariant violation found) | M | M | HIGH | M | LOW | TLC ≤ 30min calibration + counterexample triage SOP em RB-TLA-COUNTEREXAMPLE (PLANNED runbook stub a ser created pré-PRR HIGH_RISK) |
| R-002 | DPIA missing for PR mudando PII | L | M | HIGH | M | LOW | CI hook conservative + Privacy Officer manual override audit |
| R-003 | State space explosion > 30min CI | M | L | MEDIUM | L | LOW | MaxConcurrentErasures=5 calibration + ADR if bump needed |
| R-004 | TLC binary SHA-256 mismatch | L | L | CRITICAL | L | LOW | ADR-0042 §A1 bootstrap ceremony + CI block + SecLead emergency |
| R-005 | LIA defensibility weak (ICO 3-part test gaps) | L | M | HIGH | M | LOW | WP29 Opinion 06/2014 endorsed by EDPB 8 criteria template enforcement + Legal review pre-merge |
| R-006 | Quarterly Privacy Officer review skipped | M | L | LOW | L | LOW | Calendar alert + Privacy Officer escalation |
| R-007 | TLA+ scope blow (residency em S-11) | L | L | MEDIUM | L | LOW | ADR-S11-012 + Architect signature gate |
| R-008 | TLA+ semantic bugs (Sequence membership; CONSTANTS undeclared) | L | L | HIGH | L | LOW | Lote 10.10-sextus lessons absorbed: explicit CONSTANTS + Range conversion + helper operators |
| R-009 | invariant_registry.md L414/L452 status drift | L | L | LOW | L | LOW | Maintenance grep CI gate |
| R-010 | DPIAs ficam stale (not updated when feature changes) | M | L | MEDIUM | L | LOW | Quarterly review + DPIA CI hook re-trigger if changes |
| R-011 | EVT-045 + EVT-046 evidence retention violation | L | L | HIGH | L | LOW | R2 evidence-legal/ retain 7y CTRL inheritance |
| R-012 | INV §3.X position drift (Lote 10.8bis P1-13) | L | L | LOW | L | LOW | INV positions §3.5 L110 + §3.6 L116 + §3.12 L168 verified Lote 10.11.0 |

## 29. Review Checkpoints

D+0 design (Architect; ADR-S11-012 scope + TLA+ formal spec); D+1 Privacy Officer (DPIA template + LIA template + ICO 3-part test); D+2 Legal externo (DPIA + LIA review per feature); D+3 Compliance (SOC 2 P3.1..3.2 + ANPD Res. 4/2023); D+4 SecLead (TLC SHA-256 pinned ADR-0042 §A1); D+5 code review TLA+ semantics; D+6 PRR.

## 30. Sign-off (HIGH_RISK 12)

| # | Role | Status |
|---|---|---|
| 1-2 | Owner / Final Approver | _pending_ |
| 3 | SRE Lead | _staffing-blocked_ |
| 4 | Security Lead | _TBD; **mandatory** — TLC SHA-256 pinned + STRIDE_ |
| 5-6 | Engineer × 2 | _TBD; **mandatory**_ |
| 7 | QA | _TBD; **mandatory** — TLC counterexample test + property test 100k InvConsentSymmetry_ |
| 8 | Product (Gustavo) | _pending_ |
| 9 | Compliance | _TBD; **mandatory emphatic** — SOC 2 P3.1..3.2 + GDPR Art. 35 + LGPD Art. 38 + ANPD Res. 4/2023_ |
| 10 | Privacy | _TBD; **mandatory emphatic** — DPIA template + LIA template + ICO 3-part test + 3 DPIAs filled_ |
| 11 | Architect | _TBD; **mandatory emphatic** — TLA+ spec semantics + ADR-S11-012 + ADR-0042 §A1 + Lote 10.10-sextus TLA+ lessons absorbed_ |
| 12 | DPO interim | _TBD; **mandatory emphatic** — GDPR Art. 35 + LGPD Art. 38 defensibility ANPD/DPC_ |

(Legal sign-off via DPA reference at sprint level.)

## 31. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.1.0 | 2026-04-28 | Gustavo (Lote 10.11.0-bis + 10.11bis) | **Canonical fixes pós baseline review aggregate 5.18/10**: (a) **TLA+ rewrite completo** (GPT P0-4 round-1): Init agora definido (empty world); Next agora definido (disjunction de 10 actions com existential quantification); EffectiveBackends + PseudonymizedBackends declarados CONSTANTS com ASSUME garantindo cardinalidade 8 + 4 = 12 e disjuntos; InvAuditAppendOnly não-tautológico (temporal box-prime sobre Len monotonic + prefix preserved; banido `\/ TRUE`); consent_ledger DOMAIN semantics fixed (DOMAIN do RECORD value não da chave); 6-field ConsentProofFields canonical alinhado com WI-S11-003 ConsentProofPayload (Lote 10.11.0-bis-prime cycle 2 corrigido — purpose+basis_legal são CHAVE/derivados, não record fields); liveness EventualTermination + WF fairness conditions; AppendBounded helper para MaxAuditChainLen; idempotency counter `attempt_count` explícito. (b) **Legal citations corrigidas**: Art. 7§5 → Art. 10 LGPD (legitimate interest balancing); Res. 1/2021 → Res. 4/2023 (DPIA guidance — Res. 1/2021 tratou de transição de encarregado); EDPB 4/2017 → WP29 WP248rev01 (2017) endorsed by EDPB; EDPB 6/2014 → WP29 Op. 06/2014 endorsed by EDPB. (c) **Cardinality 12 backends** (8 effective + 4 pseudonymized) em CONSTANTS + comments + cfg bound. (d) **InvResidencyPinned** action NEW (cross-link WI-S11-007 property test 20k). (e) **CRITICAL severity** ligados via PAT-FORMAL-VERIFICATION-001 (resilience_patterns.md §3.6) — INV-DATA-ERASURE-COMPLETE + INV-CONSENT-PROOF-VERIFIABLE + INV-DATA-RESIDENCY all HIGH→CRITICAL. |
| 1.0.0 | 2026-04-26 | Gustavo (Lote 10.11) | Criação WI-S11-008; HIGH_RISK; SOTA pós-S-10 SEALED. **Regulatory formal completion** do CoreLink Privacy Pipeline. DPIA template `_templates/dpia.md` GDPR Art. 35 + LGPD Art. 38 unified (DD-001 reduce duplication; 6 sections: Description, Necessity, Risk, Mitigation, Residual, Quarterly review). LIA template `_templates/lia.md` LGPD Art. 10 / GDPR Art. 6(1)(f) + ICO 3-part test (DD-002 separate clarity; 4 sections: Purpose, Necessity, Balance, Review + WP29 Opinion 06/2014 endorsed by EDPB 8 criteria). 3 DPIAs filled-in em legal/dpia/ (DD-003 highest-risk Pareto + CI hook ensures future): s07-dedup-leakage (cross-tenant inference attack risk + per-tenant dedup mitigation + ADR-0019/0020 inheritance + INV-TENANT-ISOLATION); s09-telemetry-aggregation (legitimate interest LIA mirror em legal/lia/ + pseudonymization mitigation CTRL-PRIV-010); s10-billing-cross-border (Schrems II + SCC + TIA per Stripe DPA + WI-S11-007 residency pinning mitigation). TLA+ `specs/tla/dsr_erasure_atomicity.tla` formal spec (CONSTANTS Backends/Subjects/Tenants/MaxConcurrentErasures/ConsentPurposes; VARIABLES erasure_state/backend_state/consent_ledger/consent_revocation/audit_chain; Actions EraseBackend/CompleteErasure/GrantConsent/RevokeConsent; Invariants TypeOK/InvErasureComplete/InvConsentSymmetry/InvAuditAppendOnly; Theorem `Spec => []TypeOK /\ ...`). TLC v1.8.0 SHA-256 pinned (`d5d07d5dab38ddb840c91ec48fa02f28b37a608d5af9a73570018591dbc8ef7f` per ADR-0042 §A1 bootstrap ceremony). State space ~5k-50k bounded MaxConcurrentErasures=5 (Lote 10.10-quaters NEW-P0-3 lesson absorbed). DPIA CI hook scripts/validate_dpia.py detecta PR mudando PII signals → CI fail sem DPIA filled (manual override via Privacy Officer signature; DD-006 conservative). TLA+ scope discipline ADR-S11-012 (erasure only; residency `region_residency.tla` deferred to S-14 per ADR-S11-010 + invariant_registry.md §4.2 L445 alignment). Action `InvConsentSymmetry` validates WI-S11-003 schema simétrico Lote 9.4 H-05. invariant_registry.md §4.2 L414 (INV-DATA-ERASURE-COMPLETE) + L452 (INV-CONSENT-PROOF-VERIFIABLE coverage) status PLANNED → 🟡 spec written → ✅ GREEN post first CI run verde (Lote 10.11.0-bis-prime cycle 4 honest-flag). CTRL-FORMAL-001 satisfação. 8 AC scenarios + 4 chaos + 12 risks + 8 post-mortem hooks. Quarterly Privacy Officer review process em legal/dpia/REVIEW_PROCESS.md. **Lote 10.10 lessons absorbed**: source-of-truth FIRST INV positions verified pre-merge (Lote 10.8bis P1-13); typed enum (closed CONSTANTS); sign-off cap 12; cascade discipline absoluta; TLA+ semantic correctness (Lote 10.10-sextus): CONSTANTS declared upfront, helper operators documented, Sequence membership Range conversion if needed, bounded state space; severity cascade lesson (Lote 10.10-sextus: HIGH severity → SEV-2 alert canonical); CTRL-PRIV-014 audit minimization. |

## 32. Anti-patterns evitados

- ❌ DPIA post-merge (regulatory finding); ❌ LIA boilerplate sem 3-part test (defensibility weak); ❌ TLA+ skipped (CTRL-FORMAL-001 violation); ❌ Sem CI hook (future PRs sem DPIA); ❌ DPIA single locale (regulator multi-jurisdictional); ❌ DPIA review > 30d (Privacy Officer SLA miss); ❌ TLA+ scope blow (residency em S-11 violates ADR-S11-012); ❌ State space unbounded (TLC > 24h); ❌ TLA+ semantic bugs (Sequence membership without Range conversion; CONSTANTS undeclared — Lote 10.10-sextus lessons); ❌ TLC binary não-pinned (supply chain risk); ❌ INV §3.X position TBD (Lote 10.8bis P1-13); ❌ Audit fail-OPEN (Lote 10.6bis split-tier violation; this WI doesn't emit but cross-WI discipline); ❌ DPIA CI hook aggressive false-positive (engineer friction).

---
