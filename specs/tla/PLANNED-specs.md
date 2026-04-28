---
id: "TLA-PLANNED-SPECS"
type: "architecture"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "0.1.0"
created: "2026-04-24"
updated: "2026-04-24"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["architecture", "tla", "formal-verification", "planning", "obligation-matrix"]
---

# TLA+ Planned Specs — Obligation Matrix Pre-Implementation Detail

> **Propósito**: planning detail para 4 TLA+ specs novas identificadas em Lote 9.1+9.4 que cobrem invariants CRITICAL/HIGH adicionados em §3.12 + §3.13 do registry.
>
> **Status canonical pós Lote 10.11.0-bis-prime cycle 12**: S-11 dsr_erasure_atomicity.tla é **🟡 spec written** (committed em `specs/tla/`); demais (S-10 billing, S-13 key_lifecycle, S-14 byok_sovereignty / region_residency, S-19 onboarding_atomicity) permanecem **📋 PLANNED**. Este doc é o blueprint que garante (a) escopo capturado; (b) invariants alvo claras; (c) adversarial actions enumeradas; (d) CI integration plan documented.
>
> **Cross-reference**: `invariant_registry.md §4.2` (TLA+ obligation matrix) lista S-11 dsr_erasure_atomicity como `🟡 spec written` (Lote 10.11.0-bis-prime); demais 4 specs permanecem `📋 PLANNED` com sprint owners.

---

## Sumário

1. [billing_atomicity.tla](#1-billing_atomicityla-s-10) (S-10)
2. [dsr_erasure_atomicity.tla](#2-dsr_erasure_atomicityla-s-11) (S-11)
3. [byok_sovereignty.tla](#3-byok_sovereigntyla-s-14) (S-14)
4. [onboarding_atomicity.tla](#4-onboarding_atomicityla-s-19) (S-19)
5. [Bonus: key_lifecycle.tla](#5-bonus-key_lifecycleltla-s-13) (S-13)
6. [CI integration plan](#6-ci-integration-plan)

---

## 1. `billing_atomicity.tla` (S-10)

### Invariants alvo

- **INV-BILLING-NO-LOSS** (HIGH — registry §3.9; severity drift fix Lote 9.5b Codex R3-05): Σ(events emitidos) = Σ(invoiced + tombstoned + late_pending). Drift > 0.1% = SEV-1.
- **INV-BILLING-NO-DUP** (HIGH — registry §3.9): nenhum charge duplicado por mesma fonte.
- **INV-BILLING-RECONCILE-3-LAYER** (HIGH — §3.12): drift > 0.1% em qualquer layer = SEV-2; bloqueia close-of-month.
- **INV-BILLING-REPLAYABLE-FROM-EVENTS** (HIGH — §3.12): InvReplayDeterministic.

### Modelo

State machine: `Event → Counter → Stripe Invoice` com 3-layer reconciliation.

```
Variables:
  events: Sequence<Event>      \* R2 events bucket (append-only)
  counters: Map<Tenant, Counter>  \* D1 usage_counter
  invoices: Map<Tenant, [LineItem]>  \* D1 invoice_line_item
  stripe_invoices: Map<StripeID, Amount>  \* Stripe customer.invoice
  reconciliation_state: {clean, layer1_drift, layer2_drift, layer3_drift, frozen}

Actions:
  EmitEvent(tenant, sku, qty)
  AggregateCounter(tenant, hour)  \* hourly DO cron rollup
  GenerateInvoice(tenant, period)  \* monthly close-of-month
  StripeChargeWebhook(stripe_id, status)  \* idempotent on (stripe_event_id)
  Reconcile(layer)  \* daily 02:00 UTC

Adversarial actions:
  EventReplay(event)  \* idempotency test
  StripeOutage(duration)  \* PAT-QUEUE-EVENTS-001 invariant test
  ClockSkew(worker, delta)  \* late event detection
  ConcurrentCounterUpdate(tenant)  \* race condition
```

### Invariants formais

```
InvNoLoss == Sum(events.qty) = Sum(counters.qty) + Sum(late_pending)
InvNoDup == \A e1, e2 \in events: (e1.idempotency_key = e2.idempotency_key) => (e1 = e2)
InvReconcile3Layer == 
  /\ Abs(Sum(events.qty) - Sum(counters.qty)) <= 0.001 * Sum(events.qty)
  /\ Abs(Sum(counters.qty) - Sum(invoices.amount)) <= 0.001 * Sum(counters.qty)
  /\ Abs(Sum(invoices.amount) - Sum(stripe_invoices.amount)) <= 0.001 * Sum(invoices.amount)
InvReplayDeterministic == \E replay_function: 
  replay_function(events, period) = invoices[tenant, period]
```

### CI integration

- `make tla-billing-atomicity` runs TLC com bound (Tenant=2, MaxEvents=10, MaxStripeEvents=5).
- PR triggers: changes em `crates/corelink-billing/**` OR `data_model §4.X (billing)`.
- Pass criteria: 4 invariants verde + adversarial scenarios PASS.

### Owner: S-10 implementação (D+18 milestone)

---

## 2. `dsr_erasure_atomicity.tla` (S-11)

### Invariants alvo

- **INV-DATA-ERASURE-COMPLETE** (CRITICAL — §3.5; Lote 10.11.0-bis HIGH→CRITICAL com TLA+ commit): erasure efetiva em 12/12 backends canonical (8 effective + 4 pseudonymized; privacy_model.md §6.2 source-of-truth pós Lote 10.11.0-bis).
- **INV-CONSENT-PROOF-VERIFIABLE** (CRITICAL — §3.12; Lote 10.11.0-bis HIGH→CRITICAL com TLA+ symmetry): consent records verifiable post-facto via 6-field proof + HMAC.

### Modelo

State machine: DSR request → erasure cross-backend OR compensating-rollback.

```
Variables:
  dsr_requests: Map<RequestID, DSRRequest>
  effective_backends: Set<{NeonMain, NeonBilling, R2Cas, R2Ac, D1, Kv, Stripe, Loki}>  \* 8 effective canonical Lote 10.11.0-bis
  pseudo_backends: Set<{R2AuditPseudo, NeonPitrPseudo, R2CasLegalHoldPseudo, R2EvidencePseudo}>  \* 4 pseudonymized canonical
  records: Map<{Backend, SubjectID}, Record>
  audit_chain: Sequence<AuditEvent>

Actions:
  SubmitDSR(subject_id, type=erasure)
  EraseFromBackend(backend, subject_id)  \* delete em mutable
  PseudonymizeBackend(backend, subject_id)  \* SHA-256 replace em immutable
  VerificationJob24h(request_id)  \* sweep all 12 backends canonical (8 effective + 4 pseudonymized; Lote 10.11.0-bis)
  CompensatingRollback(request_id)  \* if partial failure
  ConsentRevoke(subject_id, purpose)  \* symmetric proof

Adversarial actions:
  BackendUnavailable(backend, duration)
  ConcurrentNewWrite(subject_id, backend)  \* post-erasure new write race
  CryptoEraseBYOK(tenant_id)  \* S-14 key destruction interaction
  ObjectLockConflict(R2_audit, subject_id)  \* WORM vs erasure
```

### Invariants formais

```
InvErasureComplete == 
  \A request \in dsr_requests, backend \in backends:
    (request.type = erasure /\ request.status = completed /\ TimeNow > request.completed_ts + 24h) =>
      ~ \E record \in records: (record.subject_id = request.subject_id /\ record.backend = backend)

InvPseudonymizationApplied ==
  \A request \in dsr_requests, backend \in pseudo_backends:
    (request.status = completed /\ TimeNow > request.completed_ts + 24h) =>
      \A record \in records[backend, request.subject_id]:
        record.subject_id = sha256(request.original_subject_id || erasure_salt)

InvConsentSymmetry ==
  \A subject, purpose: 
    granted(subject, purpose) <=> revoked(subject, purpose) \/ active(subject, purpose)
  /\ \A record: revoked(record.subject, record.purpose) => proof_exists(record.revocation)
```

### CI integration

- `make tla-dsr-erasure-atomicity` TLC bound canonical pós Lote 10.11.0-bis (Subjects=2, Tickets=2, Backends=12 (8 effective + 4 pseudonymized), Regions=6, Locales=3, MaxConcurrentErasures=2, MaxAuditChainLen=30, MaxAttempts=5).
- PR triggers: `crates/corelink-privacy/**` OR `privacy_model.md §X (DSR)`.

### Owner: S-11 implementação (D+12 milestone)

---

## 3. `byok_sovereignty.tla` (S-14)

### Invariants alvo

- **INV-BYOK-CRYPTO-SOVEREIGNTY** (CRITICAL — §3.12): customer revoga CMK → cache inacessível ≤ 5 min global.
- **INV-REGION-NO-CROSS-LEAK** (CRITICAL — §3.12): tenant region pinned não vaza.
- **INV-KEY-NO-SKIP** (HIGH — §3.13): writes nunca em key state inválido.

### Modelo

State machine: BYOK customer CMK lifecycle + DEK cache + region routing.

```
Variables:
  customer_cmk_state: Map<TenantID, {active, access_revoked, destroyed}>
  dek_cache: Map<{TenantID, BlobDigest}, {dek_bytes, expires_at}>  \* 5 min TTL
  blobs: Map<{TenantID, BlobDigest}, EncryptedBody>
  region_pins: Map<TenantID, Region>
  audit_chain: Sequence<AuditEvent>

Actions:
  WrapDEK(tenant_id, dek)  \* via customer CMK KMS
  UnwrapDEK(tenant_id, wrapped_dek)
  CMKAccessCheck(tenant_id)  \* every 60s background
  RevokeCMKAccess(tenant_id)  \* customer trigger
  DestroyCMK(tenant_id)  \* customer kill switch
  CacheEvict(tenant_id, digest)  \* TTL expiry
  RegionRoute(tenant_id, request_region)  \* enforce pinning

Adversarial actions:
  KMSProviderOutage(provider, duration)
  RaceCMKRevokeVsRead
  CrossRegionRequest(wrong_region)
  DEKCacheTTLBypass  \* attempt to keep DEK > 5 min
  ObservedCacheLeakWindow  \* timing attack
```

### Invariants formais

```
InvCryptoSovereignty ==
  \A tenant_id: 
    customer_cmk_state[tenant_id] = access_revoked =>
      [](TimeNow - revoke_ts > 5*60 => 
         \A digest: dek_cache[tenant_id, digest] = NULL)

InvRegionNoCrossLeak ==
  \A tenant_id, request: 
    request.tenant_id = tenant_id =>
      LocationOf(blobs[tenant_id, request.digest]) = region_pins[tenant_id]

InvKeyNoSkip ==
  \A tenant_id, write: 
    write.uses_key.state \notin {pending, rotated, retired, destroyed}
```

### CI integration

- `make tla-byok-sovereignty` TLC bound (Tenants=2, Regions=3, Times=15 ticks).
- PR triggers: `crates/corelink-byok/**` OR `key_management.md §4 (BYOK)`.

### Owner: S-14 implementação (D+18 milestone)

---

## 4. `onboarding_atomicity.tla` (S-19)

### Invariants alvo

- **INV-ONBOARD-DPA-FIRST** (HIGH — §3.12): subscription activation requires DPA signed primeiro.
- **INV-ONBOARD-ATOMIC-PROVISIONING** (HIGH — §3.12): tenant + DPA + Stripe atomic.

### Modelo

State machine: signup orchestration com Clerk + Stripe + DPA + first PAT.

```
Variables:
  pending_signups: Map<Email, {state, started_at, steps_completed}>
  tenants: Map<TenantID, {created_at, dpa_signed_at, stripe_customer_id, first_pat_id}>
  consent_ledger: Map<{SubjectID, Purpose}, Consent>
  stripe_customers: Map<StripeID, CustomerProfile>
  pats: Map<PATID, {tenant_id, scopes, created_at}>

Actions:
  SubmitSignup(email)
  EmailVerify(email)
  ProvisionTenant(email)  \* D1 transaction begin
  AcceptDPA(tenant_id, payload_6_field)  \* CTRL-PRIV-CONSENT
  CreateStripeCustomer(tenant_id)
  ActivateSubscription(tenant_id, plan)
  GenerateFirstPAT(tenant_id)
  CommitTransaction(tenant_id)
  RollbackTransaction(tenant_id)

Adversarial actions:
  StripeOutage(duration)  \* PAT-QUEUE-EVENTS-001 alignment
  BrowserDisconnect(step)
  RaceParallelSignups(email)  \* same email two tabs
  DPAVersionBumpMidFlight
  ClerkAPIFailure(step)
```

### Invariants formais

```
InvDPAFirst ==
  \A tenant_id: 
    Exists(stripe_customers[tenants[tenant_id].stripe_customer_id]) /\ subscription_active(tenant_id) =>
      DPASignedBeforeSubscription(tenant_id)

InvAtomicTx ==
  \A email: 
    SignupCompleted(email) =>
      ( Exists(tenants[*]) /\ Exists(consent_ledger[*, dpa]) /\ 
        Exists(stripe_customers[*]) /\ Exists(pats[*]) )
  /\ \A email:
    SignupRolledBack(email) =>
      ( ~Exists(tenants[*]) /\ ~Exists(consent_ledger[*, dpa]) /\ 
        ~Exists(stripe_customers[*]) /\ ~Exists(pats[*]) )
```

### CI integration

- `make tla-onboarding-atomicity` TLC bound (Emails=3, Steps=8, AdversarialActions=5).
- PR triggers: `crates/corelink-onboarding/**` OR S-19 changes.

### Owner: S-19 implementação (D+15 milestone)

---

## 5. Bonus: `key_lifecycle.tla` (S-13)

### Invariants alvo

- **INV-KEY-NO-SKIP** (HIGH — §3.13).
- **INV-KEY-OVERLAP** (HIGH — §3.13 + ADR-0018): per asset class table.
- **INV-ADMIN-DUAL-APPROVAL** (HIGH — §3.12): caller ≠ approver + collusion-rotation 3-distinct em 24h.

### Modelo

State machine: key rotation per asset class + dual-approval admin ops.

```
Variables:
  keys: Map<{AssetClass, KeyID}, {state, created_at}>
  rotations: Map<RotationID, {asset_class, old_key, new_key, started_at}>
  destructive_ops: Sequence<AdminOp>  \* with caller + approver
  active_keys: Map<AssetClass, KeyID>

Actions:
  CreateKey(asset_class)  \* state: pending
  PromoteToActive(key_id)
  RetireKey(key_id)  \* old vira retired
  DestroyKey(key_id)  \* hard destroy
  RewrapBlobs(asset_class)  \* background re-wrap
  AdminDestructiveOp(caller, approver, op_payload)
  CheckDualApproval(op)  \* hard-check D1
  CheckRotationCollusion(op, last_3_in_24h)  \* NIST AC-2(7)

Adversarial actions:
  ConcurrentRotation(asset_class)  \* 2 rotations same time
  InflightReadDuringRotation
  CollusionAttempt(caller_A, approver_B_then_caller_B_approver_A)
  TimestampBypass(mfa_ts)
```

### Invariants formais

```
InvKeyNoSkip ==
  \A write: write.uses_key.state \in {active, rotated_during_overlap}
  
InvKeyOverlap(asset_class) ==
  LET overlap_max == AssetOverlapTable[asset_class]
  IN \A rotation: TimeNow - rotation.started_at <= overlap_max

InvDualApprovalRotation ==
  \A op_seq \in SubSequencesOfLength3(destructive_ops, window=24h):
    Cardinality({op.approver | op \in op_seq}) >= 3
```

### CI integration

- `make tla-key-lifecycle` TLC bound (AssetClasses=5, MaxRotations=10, MaxAdminOps=15).
- PR triggers: `crates/corelink-key/**` OR `key_management.md`.

### Owner: S-13 implementação (D+13 milestone)

---

## 6. CI integration plan

### Per-spec gate

CI script `scripts/check_tla_obligations.py` (planned Lote 9.5):

```python
# Pseudocode
for inv in registry.critical_high_invariants:
    if inv.tla_status == "PLANNED" and inv.sprint_owner in current_sprint_path:
        if not tla_file_exists(inv.planned_path):
            fail(f"Sprint {inv.sprint_owner} sealing requires {inv.planned_path}")
        if not tla_green_in_ci(inv.planned_path):
            fail(f"TLA+ {inv.planned_path} red blocks merge")
```

### Per-PR gate

- PR touching `crates/corelink-billing/**` triggers `billing_atomicity` TLC.
- PR touching `crates/corelink-privacy/**` triggers `dsr_erasure_atomicity` TLC.
- PR touching `crates/corelink-byok/**` triggers `byok_sovereignty` TLC.
- PR touching `crates/corelink-onboarding/**` triggers `onboarding_atomicity` TLC.
- PR touching `crates/corelink-key/**` OR `key_management.md` triggers `key_lifecycle` TLC.

### Sprint S-20 GA gate

- All 4 (+1 bonus) TLA+ specs em status `GREEN` em CI sustained 30d staging.
- Adversarial actions documented + executados.
- Apalache symbolic check pós-GA Q1 (futuro upgrade).

---

## 7. Resource estimation

Effort por sprint:

| Spec | Sprint | Estimativa effort | Buffer |
|---|---|---|---|
| `billing_atomicity.tla` | S-10 | 12-18h | 4h |
| `dsr_erasure_atomicity.tla` | S-11 | 14-22h | 4h |
| `byok_sovereignty.tla` | S-14 | 16-24h | 6h |
| `onboarding_atomicity.tla` | S-19 | 10-14h | 3h |
| `key_lifecycle.tla` | S-13 | 12-18h | 4h |

Total ~70-100h cumulativo, distributed per sprint owner.

### Apalache upgrade path

Pós-GA Q1 (S-21+ Fase 2), avaliar `Apalache` (Bounded Model Checker symbolic) para covers state space explosions onde TLC vanilla não escala. Pre-GA: TLC explicit-state suficiente given small bounds.

---

**Fim TLA-PLANNED-SPECS.md.** Cross-reference em `invariant_registry.md §4.2` + sprint contracts S-10/11/13/14/19.
