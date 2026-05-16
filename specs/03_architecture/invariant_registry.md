---
id: "INVARIANT-REGISTRY"
type: "invariant"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "0.2.2"
created: "2026-04-24"
updated: "2026-05-16"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["architecture", "invariants", "registry", "tla"]
---

# Invariant Registry — Catálogo Canônico de INV-XXX

> **doc_status:** DRAFT
> **Versão:** 0.2.2
> **Última atualização:** 2026-05-16 (Wave-24 R-PREP `auth_pat_revoke.tla` dispatch: INV-PAT-REVOKE-PROPAGATION §3.28 PLANNED → TLA-VERIFIED via new `specs/tla/auth_pat_revoke.tla` + PR/nightly cfgs + CI matrix wiring; safety 5 invariants (`InvRevokedTokenNeverValidates` + `InvRevokeAuditAtomic` + `InvRevokeIsIdempotent` + `InvRegionImpliesSoTRevoked` + `InvRevokeIsMonotonic`) + liveness `InvRevokeAtLeastOncePropagation` proven; PR-lane 1 263 distinct states / depth 10 / 3 s wall clock; nightly 61 293 distinct states / depth 14 / 1 min 06 s wall clock; all temporal branches green; see `specs/_audits/2026-05-16-auth-pat-revoke-tla.md`. Wave-23 invariant-draft sweep: §3.27 OPS domain (4 INVs INV-S17-OPS-EXCLUSIVITY / INV-S17-SEV1-DRILL-PAUSE / INV-S17-CHAOS-STAGING-ONLY / INV-S17-ONCALL-FATIGUE-AUTOROTATE) promoted from S-17 `_spec_contract.md §8`; §3.28 PAT revocation domain (INV-PAT-REVOKE-PROPAGATION) promoted from apps/docs OpenAPI + 4 i18n MDX endpoint contracts; 5 aliases added (INV-AUDIT-CHAIN, INV-AUDIT-EMIT-ATOMIC, INV-AUTH-WEBAUTHN family-shorthand, INV-BLAKE3-256-LOWER-HEX-64 subsumed) — see `specs/_audits/2026-05-16-inv-draft-sweep.md`. 2026-05-07 Hardening: INV-LRU-CONSISTENCY race-window claim corrected (S-07 R5 P2-1); INV-OBS-CARDINALITY-BUDGET suspended-tier policy documented (S-09 R5 P2-3))
> **Owner:** Gustavo Schneiter
> **Aprovador Final:** Gustavo Schneiter
> **Revisores:** ⚠️ **staffing-blocked** — promoção a `doc_status: FROZEN` bloqueada até ≥ 2 reviewers nomeados conforme roles indicados (endereça F-09 audit Lote 3+4)
> **Supersedes:** —
> **Superseded By:** —

> **Propósito:** fonte canônica (Nível 3) de **todos** os IDs `INV-XXX` do CoreLink. Endereça audit finding S-07 (naming inconsistente entre canonical sources: `INV-AUDIT-APPEND-ONLY` vs `INV-AuditLogImmutability`, `INV-TenantIsolation` vs `INV-DATA-TENANT-ISOLATION`).
>
> Consumido por: todos canonical sources que citam invariantes, templates WI/Sprint/PRR §12, TLA+ spec files em `specs/tla/`.
>
> **Regra:** nomes de invariante **NÃO PODEM** ser criados fora deste registro. Novo INV = PR com entry aqui + reference em canonical source + (se CRITICAL) TLA+ spec.

---

## Sumário

1. [Regras de naming](#1-regras-de-naming)
2. [Severidade e enforcement](#2-severidade-e-enforcement)
3. [Registry](#3-registry)
4. [TLA+ coverage matrix](#4-tla-coverage-matrix)
5. [Aliases históricos (deprecated names)](#5-aliases-históricos-deprecated-names)

---

## 1. Regras de naming

Formato canônico: `INV-<DOMAIN>-<NAME>`.

- `<DOMAIN>` ∈ `{TENANT, CAS, AC, GC, DATA, AUDIT, CONF, AVAIL, BILLING, SUPPLY, PRODUCT, KEY, ADMIN, OBS, DEDUP, RATE-LIMIT, BYOK, REGION, CONSENT, ONBOARD, ERASURE}` (ver §3). Domains adicionados em §3.11+ via Lote 6+9 expansion.
- `<NAME>` em `SCREAMING_KEBAB_CASE` (hifens, não camelCase).
- Total ≤ 40 chars.

**Formato proibido:** `INV-<CamelCase>` (ex: `INV-TenantIsolation`). Aliases históricos tolerados por 6 meses com redirecionamento; novos docs usam só o canonical.

Exceção histórica: **INV-TenantIsolation** e **INV-AuditLogImmutability** e **INV-CASIdempotency** são mantidos como canonical (não quebrar código TLA+ existente) — §5 lista todos.

---

## 2. Severidade e enforcement

| Severidade | Definição | Enforcement | Evidence |
|---|---|---|---|
| **CRITICAL** | Violação = data loss, cross-tenant breach, ou fraude financeira | TLA+ **obrigatório** (CTRL-FORMAL-001); property test; CI falha se model check não verde | EVT-022 + EVT-002 |
| **HIGH** | Violação = degradação funcional severa | Property test obrigatório; TLA+ se algoritmo não-trivial | EVT-002 |
| **MEDIUM** | Violação = bug observável | Unit test obrigatório; property test opcional | EVT-002 |

Upgrade de severidade requer ADR.

---

## 3. Registry

### 3.1 Tenant Isolation (domain TENANT)

| ID | Nome | Severidade | Descrição | Enforcement | TLA+ file |
|---|---|---|---|---|---|
| **INV-TENANT-ISOLATION** (alias histórico: `INV-TenantIsolation`) | Tenant isolation across storage, auth, metrics | CRITICAL | Nenhum principal de Tenant A pode ler/escrever/enumerar dados de Tenant B, sob nenhuma circunstância, inclusive com credencial legítima de A | Property test + TLA+ obrigatório + 5 camadas de defesa (ver `auth_model.md §8.1`) | `specs/tla/tenant_isolation.tla` |

### 3.2 CAS Integrity (domain CAS)

| ID | Nome | Severidade | Descrição | Enforcement |
|---|---|---|---|---|
| **INV-CAS-INTEGRITY** | Blob body hash matches path digest | CRITICAL | `hash_fn(body) == path.digest` para todo R2 object em `cas-*` buckets | Write-time check (reject if mismatch) + scrub periódico + client-side verify |
| **INV-CAS-IDEMPOTENCY** (alias histórico: `INV-CASIdempotency`) | Same content → same digest | CRITICAL | Upload do mesmo byte-sequence **DEVE** resultar no mesmo `digest` determinístico | Algorithm choice (BLAKE3/SHA-256) + test de idempotência |
| **INV-CAS-IMMUTABILITY** | Blob bytes never change post-creation | CRITICAL | Após primeira write, body é read-only (substituição de body requer path novo por causa de INV-CAS-INTEGRITY) | R2 versioning + write-once contract |

### 3.3 Action Cache (domain AC)

| ID | Nome | Severidade | Descrição | Enforcement |
|---|---|---|---|---|
| **INV-AC-OUTPUTS-VALID** | AC outputs apontam para blobs existentes | HIGH | Toda entry AC (`action_digest → result_proto`) tem todos os output digests com entry em `blob_meta` alive | Reconcile diário + CI integration test |
| **INV-AC-TENANT-SCOPED** | AC entries são tenant-local | CRITICAL | AC entry de Tenant A não pode ser servida a Tenant B | Deriva de INV-TENANT-ISOLATION |

### 3.4 Garbage Collection (domain GC)

| ID | Nome | Severidade | Descrição | Enforcement | TLA+ file |
|---|---|---|---|---|---|
| **INV-GC-001** (reachable never deleted) | Blob reachable → blob preservado | CRITICAL | Se blob B está no set reachable (cas + ac + manifests + chunks) no momento do sweep, B **não é deletado** | TLA+ model obrigatório (FF-HR-011) + mark-phase-aware grace 72h + soft-delete | `specs/tla/gc_correctness.tla` |
| **INV-GC-002** | Orphan eventualmente deletado | MEDIUM | Blob não-reachable por > grace period eventualmente é deletado (eventually-consistent) | GC scheduler + monitor |
| **INV-GC-003** | Refcount consistency | HIGH | `blob_meta.refcount = count of AC entries referencing this digest within tenant` (±stale grace 5min) | Reconcile + property test |
| **INV-GC-004** | Mark-phase-aware re-ref safe | CRITICAL | Blob re-referenciado via AC update após `mark_started_at` não é deletado mesmo que Mark não o tenha visto | Implementação: sweep só deleta se TODAS `ac.created_at < mark_started_at` (protect-if-`>=`; canonical TLA semantics em `gc_correctness.tla` L152-154 — Lote 10.6 cycle 4 fix; was `>` strict, now `>=` matches formal model boundary) | Coberto por `gc_correctness.tla` |

### 3.5 Data Integrity (domain DATA)

| ID | Nome | Severidade | Descrição | Enforcement |
|---|---|---|---|---|
| **INV-DATA-MONOTONIC-TS** | Timestamps monotonic | MEDIUM | `last_accessed_at`, `updated_at` nunca regridem | Writer enforces via `MAX(now, prev_value)` |
| **INV-DATA-BILLING-RECONCILE** | Usage counter ≈ Σ(usage events) | HIGH | Drift ≤ 0.1% entre `usage_counter` agregado e eventos emitidos | Reconcile diário (PAT-RECONCILE-001) |
| **INV-DATA-ERASURE-COMPLETE** | DSR erasure é efetiva | CRITICAL (Lote 10.11.0-bis: HIGH→CRITICAL com TLA+ commit S-11 WI-S11-008) | Após DSR-erasure resolved, nenhum backend retorna dado do subject (exceto legal hold). Aplica-se aos 12 backends canonical (8 effective: Neon multi-tabela / Neon billing fiscal exception / R2 CAS refcount-aware / R2 AC / D1 / KV / Stripe `Customer.update` / Loki; 4 pseudonymized: R2 audit Object Lock 7y / Neon PITR backup 30d / R2 CAS legal_hold partition / R2 evidence-* buckets 7y) | E2E test (EVT-042) + **TLA+ em `specs/tla/dsr_erasure_atomicity.tla`** (S-11 WI-S11-008) + PAT-RETRY-IDEMPOTENT-001 + PAT-FORMAL-VERIFICATION-001 |

### 3.6 Audit (domain AUDIT)

| ID | Nome | Severidade | Descrição | Enforcement |
|---|---|---|---|---|
| **INV-AUDIT-APPEND-ONLY** (alias histórico: `INV-AuditLogImmutability`) | Audit log é append-only, cadeia de hash íntegra | CRITICAL | UPDATE/DELETE em audit_log rejeitado; hash chain verifica continuamente | D1 CHECK constraint + R2 Object Lock + daily chain verify (PAT-AUDIT-VERIFY-001) + **TLA+ em `specs/tla/audit_immutability.tla`** (criado Lote 6.2) |
| **INV-AUDIT-RETENTION** | Audit retention ≥ 7 anos | HIGH | Archive em R2 Object Lock ≥ 7y (SOC 2 + LGPD Art. 16) | Object Lock + quarterly audit |

### 3.7 Confidentiality (domain CONF)

| ID | Nome | Severidade | Descrição | Enforcement |
|---|---|---|---|---|
| **INV-CONF-AT-REST** | Dados em repouso cifrados | HIGH | R2 SSE-S3 em todos os buckets; D1/Neon SSE enabled; BYOK opcional enterprise | CTRL-CRYPTO-002; quarterly config audit (EVT-028) |
| **INV-CONF-IN-FLIGHT** | TLS 1.3 obrigatório | HIGH | Nenhum endpoint CoreLink aceita < TLS 1.3; mTLS entre Worker/Container bindings | CTRL-CRYPTO-001; SSL Labs A+ check (EVT-037) |

### 3.8 Availability (domain AVAIL)

| ID | Nome | Severidade | Descrição | Enforcement |
|---|---|---|---|---|
| **INV-AVAIL-ISOLATION** | Tenant DoS não cascateia | HIGH | Abuse ou outage de Tenant A não degrada SLO de Tenant B além do ruído expected | Per-tenant bulkhead (PAT-BULKHEAD-001) + rate limits (CTRL-RATE-001); chaos test (EVT-023) |
| **INV-AVAIL-DOS** | Log redaction regex hot-paths são DoS-resistant (no catastrophic backtracking) | HIGH | Falsifiability canary em `corelink-logpush::redaction`: every regex used in log shipping is constrained to linear-time semantics. Catastrophic backtracking on attacker-controlled log lines could brown out the worker and degrade availability. | Property test corpus de adversarial inputs + ad-hoc `regex` config audit + bounded-time CI fuzz (DEBT-004 promotion) |

### 3.9 Billing (domain BILLING)

| ID | Nome | Severidade | Descrição | Enforcement |
|---|---|---|---|---|
| **INV-BILLING-NO-LOSS** | Todo billable event contabilizado | HIGH | Nenhum evento billable é perdido; at-least-once delivery + dedup | Queue com durável + reconciliation (PAT-RECONCILE-001) |
| **INV-BILLING-NO-DUP** | Nenhum evento billable contabilizado 2x | HIGH | Idempotency key + dedup window 24h | PAT-IDEMPOTENCY-001 |
| **INV-BILLING-CHAIN-INTEGRITY** | Per-`(tenant, billing_period)` BLAKE3 chain de aggregates é unbroken (analogous to `INV-OBS-AUDIT-CHAIN-INTEGRITY` S-09; Bitcoin block-header pattern inheritance from `corelink-audit-chain`) | HIGH | Verifier recomputes link from `(prev_hash, JCS(aggregate))` and rejects at first tampered sequence; tamper detection at any aggregate fails verify | Property test `prop_jcs_canonicalization_byte_stable` + chain reconstruction tests em `corelink-billing-aggregator`; daily verifier job (mirrors S-09 audit chain) |

### 3.10 Supply Chain (domain SUPPLY)

| ID | Nome | Severidade | Descrição | Enforcement |
|---|---|---|---|---|
| **INV-SUPPLY-SIGNED-DEPLOY** | Apenas binários assinados deployados | HIGH | CF Worker deploy valida cosign signature antes de ativar | CTRL-SUPPLY-002; CI gate |
| **INV-SUPPLY-SBOM-PRESENT** | Todo release tem SBOM | HIGH | PR de release é BLOCKED sem EVT-010 (CycloneDX 1.5+ via ADR-0014) | CI gate; PRR gate |

### 3.11 Product / Quota (domain PRODUCT)

Adicionado em Lote 6.3 endereçando G-04 do re-audit (INVs legados CamelCase).

| ID | Nome | Severidade | Descrição | Enforcement |
|---|---|---|---|---|
| **INV-QUOTA-ENFORCEMENT** (alias histórico: `INV-QuotaEnforcement`) | Quota de tenant nunca ultrapassada | HIGH | Em nenhum estado, tenant consome mais que quota configurada | DO atomic counter per-tenant + write path check (CTRL-QUOTA-001) |
| **INV-DIGEST-VERIFICATION** (alias histórico: `INV-DigestVerification`) | Write rejeita hash mismatch | CRITICAL | Toda write valida `hash(body) == claimed_digest` antes de persistir | CTRL-CAS-001 + TLA+ cas_integrity.tla (InvPoisoningRejected) |
| **INV-DATA-RESIDENCY** (alias histórico: `INV-DataResidency`) | Dado de tenant fica em região pinned | CRITICAL (Lote 10.11.0-bis: HIGH→CRITICAL — Schrems II + LGPD Art. 33 §1º; 20k property test + custom domain routing TLA+ S-11 WI-S11-007) | R2 bucket com locationHint; D1 primary na região escolhida; DO stickiness; edge router PAT-ROUTING-PINNED-001 fail-CLOSED 451 em mismatch (NUNCA passthrough silencioso) | CTRL-PRIV-031; quarterly config audit + 20k property test cases ≥ 0 leaks + PAT-FORMAL-VERIFICATION-001 |

### 3.12 Sprint-driven invariants (Lote 9.1 SOTA elevation, expandido em Lote 9.4)

Invariantes introduzidas via SOTA elevation dos sprint contracts S-07..S-19 (Lotes 9.1 + 9.4). Todas adicionadas com sprint_origin column para traceability.

| ID | Nome | Severidade | Descrição | Enforcement | Sprint origin |
|---|---|---|---|---|---|
| **INV-DEDUP-CONSISTENCY** | Dedup chunk_body 1:1 dentro do tenant | HIGH | `(tenant_id, chunk_digest) → chunk_body` é 1:1; nenhum dup chunk_body para mesmo digest dentro de tenant | UNIQUE index D1 + property test (PAT-DEDUP-CHECK-001) | S-07 |
| **INV-RATE-LIMIT-PROPORTIONALITY** | Rate limit refill_rate × window consistente com tenant plan | HIGH | `refill_rate × window` sempre consistente com tenant plan; mudança de plan reflete em ≤ 5 min via DO config sync | DO atomic update + sync test (CTRL-RATE-001) | S-08 |
| **INV-OBS-CARDINALITY-BUDGET** | Cardinality de métrica respeita budget | HIGH | Nenhuma métrica excede 20k séries únicas; total ≤ 100k. Reference: `observability_model.md §11.2`. Tier label canonical enum = `{Free, Solo, Team, Business, Enterprise}` 5-element (WI-S09-001 §1.7); **suspended/canceled tenant policy**: suspended tenants emit as `Free` tier label (deliberate choice — avoids adding a 6th Suspended variant that would increase cardinality 5→6 × 30 × 3 = 540 series baseline; documented per S-09 R5 P2-3; Architect sign-off recorded in sprint contract v1.3.0 SEAL). | Cardinality validator CI + Grafana Mimir tenant limit | S-09 |
| **INV-OBS-AUDIT-CHAIN-INTEGRITY** | Audit events R2 hash chain unbroken | HIGH | Hash chain de audit events em R2 é unbroken; daily verifier alerta em break | Background daily job + INV-AUDIT-APPEND-ONLY | S-09 |
| **INV-BILLING-APPEND-ONLY** | R2 usage event log append-only (mirror INV-AUDIT-APPEND-ONLY scoped to billing) | HIGH | UPDATE/DELETE em `usage/{tenant}/{period}/{seq}.usage.ndjson` rejeitado; idempotency_key BLAKE3-of-JCS estabelece dedup; R2 PutObject com If-None-Match: *; equivalente structurally a INV-AUDIT-APPEND-ONLY (S-06 lift) escopado ao billing event log (NÃO ao audit log geral) | R2 If-None-Match guard + IdempotencyTracker dedup at sink layer + ChainHash verify daily | S-10 |
| **INV-BILLING-RECONCILE-3-LAYER** | 3-layer reconciliation diária events ↔ counters ↔ Stripe | HIGH | Drift > 0.1% em qualquer layer = SEV-2; drift > 1% = SEV-1 + auto-pause Stripe (uniform threshold ladder per spec_contract S-10 §14.s10.1; supersedes draft R-S10-8 Layer-3-specific 0.1% SEV-1 — see S-10 sprint-close P1-2 fix). Bloqueia close-of-month até resolution. Strengthens INV-DATA-BILLING-RECONCILE | Cron daily + 3-layer compare + alert escalation | S-10 |
| **INV-BILLING-REPLAYABLE-FROM-EVENTS** | Invoice reconstrutível byte-a-byte de events | HIGH | Qualquer invoice deve poder ser reconstruída byte-a-byte a partir de events R2; replay endpoint role-protected | POST /v1/billing/replay endpoint + monthly CI test + audit-grade | S-10 |
| **INV-CONSENT-PROOF-VERIFIABLE** | Consent records têm notice_text_hash verifiable post-facto | CRITICAL (Lote 10.11.0-bis: HIGH→CRITICAL com TLA+ symmetry InvConsentSymmetry) | SHA-256 do notice + HMAC-SHA256 com HKDF info=`corelink/v1/consent-hmac`; tampering detected via signature; grant/revoke ledger symmetric (Lote 9.4 H-05); 12 canonical purposes enum (privacy_model.md §5.6.1); legal_basis fixo per purpose (no fail-open swap) | Verify endpoint JWS-signed + GDPR Art. 7 + LGPD Art. 8 alignment + **TLA+ via `dsr_erasure_atomicity.tla`** (InvConsentSymmetry, S-11 WI-S11-008) + PAT-FORMAL-VERIFICATION-001 | S-11 |
| **INV-SUPPLY-PROVENANCE-IN-REKOR** | Provenance attestation publicada em Rekor | HIGH | Toda provenance attestation deve estar em Rekor transparency log; release sem inclusion proof = blocked | Deploy webhook checks Rekor inclusion antes rollout | S-12 |
| **INV-SUPPLY-NO-YANKED** | Zero yanked deps em Cargo.lock main | HIGH | Author retired pode ser segurança ou bug; usar = risco | cargo-deny policy + CI gate | S-12 |
| **INV-SUPPLY-LICENSE-ALLOWLIST** | Zero deps fora da license allowlist | HIGH | Allowlist: MIT/Apache-2.0/BSD/ISC/MPL-2.0/Unicode-DFS-2016. GPL/AGPL/SSPL banned | cargo-deny enforces; quarterly Legal review | S-12 |
| **INV-ADMIN-DUAL-APPROVAL** | Destructive admin op tem 2 distinct signatures | HIGH | Caller + approver enforced D1 hard-check; 0 bypasses em property test 10k attempts | D1 hard-check + property test + audit emission | S-13 |
| **INV-ADMIN-MFA-FRESHNESS** | Admin op exige MFA timestamp ≤ 30 min | HIGH | Stale MFA = 401 + force re-MFA (CTRL-AUTH-010) | Middleware check + signed timestamp + clock-skew tolerance ≤ 60s | S-13 |
| **INV-BYOK-CRYPTO-SOVEREIGNTY** | Customer revoga CMK → cache inacessível ≤ 5 min | CRITICAL | Nenhum bypass via cached unwrapped DEK > 5 min; customer real control; per-region DEK caches evict independently with no cross-region copy | DEK cache TTL 5 min hard + KMS access check 60s + INV propagation + **TLA+ em `specs/tla/byok_envelope_aad.tla`** (DEBT-005 closure 2026-05-15; AAD binding + cache-TTL liveness) + **TLA+ em `specs/tla/byok_dek_race.tla`** (DEBT-014 FT-7 CRITICAL upgrade closed 2026-05-16; per-region cache eviction race; TLC 3906 distinct states + 2 temporal branches; InvNoCrossRegionDekCopy proves no inter-region DEK leakage) | S-14 |
| **INV-REGION-NO-CROSS-LEAK** | Blob/AC/billing tagged com region; cross-region read = 403 | CRITICAL | Schrems II + LGPD Art. 33 baseline; 30k property test 0 leaks | Insert checks + property test + custom domain routing + **TLA+ em `specs/tla/failover_no_split_brain.tla`** (DEBT-005 closure 2026-05-15; write-lease handoff side) | S-14 |
| **INV-FAILOVER-NO-SPLIT-BRAIN** | Coordinator NEVER allows two regions to claim `Primary` simultaneously; promotion sequence holds singleton DO lock, asserts (a) source==current primary, (b) no other region in Primary state, (c) target was in Replica state | CRITICAL | `corelink-replication-coordinator::Coordinator::promote` holds Mutex; rejects with `SplitBrainAttempted` if any other region is already Primary; audit-emit-BEFORE-state-mutation fail-CLOSED so a denied promotion leaves the cluster unchanged | Property test `prop_coordinator::at_most_one_primary` (2000 cases) + 3 e2e scenarios (`split_brain_reject::*` covering registration-time + promote-time + audit-fail-closed paths) | **`replica_failover.tla` ✅ GREEN** (Wave 15 — 2026-05-15) — `InvAtMostOnePrimary` proves the singleton-primary invariant; complements `failover_no_split_brain.tla` (write-lease side) |
| **INV-ERASURE-ATTESTATION-SIGNED** | Erasure de BYOK tenant produz attestation Ed25519-signed verifiable | HIGH | Customer + auditor exigem proof; NIST SP 800-88 Rev.1 compliant | Per-erasure attestation + 7y retention + verify endpoint | S-14 |
| **INV-ONBOARD-DPA-FIRST** | Subscription activation requires DPA signed primeiro | HIGH | Race condition prevented; nenhum customer billed sem DPA | D1 lock + transactional check + property test 10k concurrent + **TLA+ runbook em `specs/03_architecture/tla+/runbooks/signup_atomic.tla`** (DPAFirstHolds; CI-wired via DEBT-014 FT-8 2026-05-16) + **TLA+ em `specs/tla/signup_resignup.tla`** (re-signup boundary; DEBT-014 FT-9 closed 2026-05-16) | S-19 |
| **INV-ONBOARD-ATOMIC-PROVISIONING** | Tenant provisioning atomic | HIGH | Tenant + DPA + Stripe customer ID em single tx; failure rollback all | D1 transaction + chaos test Stripe outage + **TLA+ runbook `signup_atomic.tla`** + **TLA+ `signup_resignup.tla`** (DEBT-014 FT-8 + FT-9 closed 2026-05-16) | S-19 |
| **INV-SIGNUP-RESIGNUP-IDEMPOTENT** | Re-signup with same email after a compensated failure produces independent atomic outcome; no orphan Clerk/D1/Stripe state from prior attempt; late Stripe webhook arriving after compensation is a no-op | HIGH | Idempotency-key tombstone for Stripe webhook handler; StartAttempt guard rejects re-attempt when prior attempt is in_progress or succeeded; compensation rollback covers all 4 systems (Clerk, D1 tenant, D1 dpa, Stripe) | Integration test re-signup + chaos test late-webhook delivery + **TLA+ em `specs/tla/signup_resignup.tla`** (DEBT-014 FT-9 closed 2026-05-16; InvResignupNoCrossLeak + InvAtMostOneSuccessPerEmail + InvLateWebhookNoOp; TLC 561 distinct states + 3 temporal branches; counterexample found and fixed during dispatch — uniqueness guard added to StartAttempt) | S-19 |
| **INV-CAS-SIDE-CHANNEL-INDISTINGUISHABLE** | Timing distribution **across all 404 MissReason variants (`NeverExisted` × `Tombstoned` × `R2OrphanRow` per `corelink-reapi::read::MissReason` + ADR-0028 v1.1.0 runtime fold; the conflated `CrossTenantMasked` arm folds into `NeverExisted` at the orchestrator surface)** statistically indistinguishable; 3-arm parity model | HIGH | Constant-time middleware + jitter; **pairwise Mann-Whitney U** ALL 9 tests must `p > sidak_per_test_alpha(0.05, 9)` ≈ 0.005 685 8 (3 trials × 3 pairs full conjunction at familywise α = 0.05) com **Šidák correction**; 10k samples per arm × 3 arms; criterion benchmark **|Δmedian| ≤ 1 ms point estimate AND `ci_upper` ≤ 1 ms across pairs**; cycle 13 SEAL math correction (earlier '0.000125' was incorrect); cycle 14 SEAL canonical-MWU substitute for non-existent `statrs::stats_tests::mann_whitney_u` | Adversarial test S-02 (3-arm methodology) + criterion CI | S-02 |
| **INV-EXEC-IDEMPOTENT** | Replay of same `(tenant_id, exec_id)` workflow execution is no-op pós-verify; `result_hash` IMMUTABLE pós-first-completion (mirror INV-AC-IDEMPOTENT §3.15 scoped to action-execution layer; surfaces as `DASH-EXEC.json` panel 10 canary "Same action_digest executed concurrently > 1 = race; > 0 sustained = SEV-2"; promoted from drift to canonical via Lote 10.9bis wave 17 per R4-P1-1 + R5-P1-S1 remediation matrix; S-17 execution race detector at worker scheduler boundary owns runtime enforcement) | HIGH | `INSERT ON CONFLICT (tenant_id, exec_id) DO UPDATE SET last_hit_at = excluded.last_hit_at`; `result_hash` NUNCA mutável; property test 10k iter `prop_exec_replay_idempotent` + integration test re-execute mesmo exec_id; INV-CAS-SIDE-CHANNEL parity para timing leak resistance | Property test 10k iter + integration test re-execute mesmo exec_id; canary panel `DASH-EXEC.json` panel 10 (must=0 sustained) + S-17 worker scheduler race detector (PLANNED) | S-09 |
| **INV-LGPD-AUTO-SUSPEND-FORBIDDEN** | Tenant accounts NEVER auto-suspended sem explicit human decision; LGPD Art. 20 + GDPR Art. 22 right against solely-automated decisions com legal effect significant ao titular; surfaces as `DASH-SECURITY.json` panel "INV-LGPD-AUTO-SUSPEND-FORBIDDEN canary (MUST = 0; SEV-1 alert; LGPD Art. 20 + GDPR Art. 22)" + `DASH-TENANT.json` abuse-score panel cross-reference; promoted from drift to canonical via Lote 10.9bis wave 17 per R4-P1-1 + R5-P1-S8 remediation matrix; S-11/S-13 abuse heuristic gating owns runtime enforcement | HIGH | All suspension actions exigem dual-approval (INV-ADMIN-DUAL-APPROVAL S-13) + explicit `human_reviewer_id` em audit emit; NO path from automated abuse signal (S-08 4-feature abuse score, even Malicious arm at threshold ≥ 0.8) directly into `tenant_state=suspended` transition sem human confirm step; canary panel must = 0 sustained | Integration test asserts no `tenant_state=suspended` transition sem `human_reviewer_id` set + property test 10k iter on automated abuse paths + canary panel `DASH-SECURITY.json` (must=0; SEV-1 alert) | S-09 |

### 3.13 Key management (domain KEY) — Lote 9.4

Invariantes que governam crypto key lifecycle. Definidas inicialmente em `key_management.md §3.2`; promovidas formalmente ao registry em Lote 9.4 (Opus C-02 finding).

| ID | Nome | Severidade | Descrição | Enforcement | TLA+ file |
|---|---|---|---|---|---|
| **INV-KEY-NO-SKIP** | Writes nunca usam key em state inválido | HIGH | Writes nunca em state `{pending, rotated, retired, destroyed}`; retorno 503 se única key disponível for inválida | State machine em `corelink-key` crate + property test rotation flow + INV-KEY-AUDIT trail | (planned `key_lifecycle.tla`) |
| **INV-KEY-OVERLAP** | Rotation overlap respeitado per asset class | HIGH | Tabela canonical `key_management.md §3.2.1`: PAT 24h / audit 24h / TDK 7d / BYOK 7d / Ed25519 attest 30d. Hard upper 30d sem ADR | Property test per asset class + rotation worker S-13 | (planned `key_lifecycle.tla`) |
| **INV-KEY-AUDIT** | Toda transição emite EVT-047 + EVT-028 | HIGH | State machine transition append-only audit; chain integrity verified daily | Audit emit em rotation worker + S-09 hash chain | Coberto por `audit_immutability.tla` |

**Aliases históricos:** nenhum. Estes IDs sempre estiveram em `key_management.md §3.2` desde Lote 4; Lote 9.4 promove ao registry sem rename.

**Cross-reference**: `ADR-0018-key-overlap-per-asset.md` documenta a decisão de per-asset overlap (vs single 24h global proposto inicialmente).

---

### 3.14 Auth domain (domain AUTH) — Lote 10.3 (S-03 sprint)

Invariantes que governam auth lifecycle: JWT validation (Clerk), PAT lifecycle (Argon2id), Tower middleware orchestration, revocation propagation, schema RLS, WebAuthn ceremonies, audit emission. Promovidas ao registry em Lote 10.3bis (P0 fix dos reviews agent-r4-s03-part1+part2).

| ID | Nome | Severidade | Descrição | Enforcement | TLA+ file |
|---|---|---|---|---|---|
| **INV-AUTH-JWT-VALIDATE-RS256-ONLY** | JWT validation rejects all non-RS256 alg | CRITICAL | `jsonwebtoken` 9.x `Validation::new(Algorithm::RS256)` enforce; rejeita alg=none + key confusion HS-with-public-pem | Property test 10k iter + adversarial regression em `corelink-clerk/tests/adversarial.rs` (CVE-2015-9235 + CVE-2018-0114) | **TLA+ em `specs/tla/auth_jwt_validation.tla`** (DEBT-005 closure 2026-05-15) |
| **INV-AUTH-CLOCK-SKEW-BOUND** | Clock skew leeway ≤ 60s em todos JWT validation paths | HIGH | RFC 7519 §4.1.4 industry standard ±60s; código-gated constant em `ClerkAdapter::new`; uniform exp/nbf/iat | Static check em construtor + integration test boundary | N/A (constant invariant) |
| **INV-AUTH-ISS-EXACT-MATCH** | Issuer compared exact match (não prefix) | CRITICAL | Allowlist `Vec<String>` exact eq; previne `https://clerk.corelink.dev.attacker.com` confusion | Property test 10k iter random origin/issuer combinations | **TLA+ em `specs/tla/auth_jwt_validation.tla`** (DEBT-005 closure 2026-05-15) |
| **INV-AUTH-KID-RESOLUTION** | KID miss triggers single refresh + retry; no infinite loop | HIGH | Lazy JWKS refresh + 1 retry max; timeout fail → KidNotInJwks | Integration test KID rotation chaos + counter assert | N/A (state machine) |
| **INV-AUTH-PAT-HASH-ARGON2ID-2024** | PAT hashes use Argon2id m≥65536/t≥3/p≥4 (OWASP 2024) | CRITICAL | All hashes em PHC string format `$argon2id$v=19$m=65536,t=3,p=4$...`; verify rejects underprovisioned params | Static check em deploy gate + cargo-deny version pin `argon2 = "0.5"` | N/A (cripto invariant) |
| **INV-AUTH-PAT-PLAINTEXT-NEVER-PERSISTED** | PAT plaintext nunca em DB / logs / traces / errors | CRITICAL | `PatPlaintext` newtype sem Display/Debug/Serialize; único `into_string()` em mint() return | CI grep gate + clippy custom lint + static analysis | N/A (compile-time enforced) |
| **INV-AUTH-PAT-VERIFY-CONSTANT-TIME** | Mann-Whitney 3-prong + power analysis sustained em CI nightly | CRITICAL | N≥10000 per arm + power 1−β≥0.80 + Šidák 3-trial + bootstrap 95% CI sobre \|Δmedian\| ≤ 5ms | CI nightly job + alert se p < 0.05 sustained 3 trials | N/A (statistical test) |
| **INV-AUTH-PAT-SALT-PER-TOKEN** | Each PAT mint generates unique 16-byte salt via getrandom | HIGH | mint() chama `getrandom` independente per token; PHC string embeds salt | Property test 100k unique salts | N/A (cripto invariant) |
| **INV-AUTH-PAT-SCOPE-DB-IS-SOT** | Scope nunca inferido from PAT prefix string; DB column é fonte | HIGH | Server-side reads `pat.scopes` BIGINT column; PAT prefix é hint apenas | Property test cross-tenant scope spoofing rejection | N/A (architecture invariant) |
| **INV-AUTH-TENANTCTX-IMMUTABLE** | TenantCtx fields private; nenhum mutation path | CRITICAL | Builder pattern em `corelink-worker/middleware/tenant_ctx.rs`; `#[non_exhaustive]`; cargo-deny lints `unsafe = "deny"` | Compile-time enforcement + chaos test layer-mismatch detection | **TLA+ em `specs/tla/tenant_ctx_propagation.tla`** (DEBT-005 closure 2026-05-15) |
| **INV-AUTH-5-LAYER-ORDERING** | Middleware layer order é canonical (auth → ctx → scope → rate-limit → audit pre → handler → audit post) | CRITICAL | Tower `ServiceBuilder` composition fixed; integration test asserts ordering via fail-closed assertions | Chaos test 5-layer scramble + fail-closed assertions | **TLA+ em `specs/tla/tenant_ctx_propagation.tla`** (DEBT-005 closure 2026-05-15) |
| **INV-AUTH-SESSION-CACHE-KEY-CT** | Session cache key compare é constant-time via `subtle::ConstantTimeEq` | HIGH | KV key construction sha256 truncated 16-byte; lookup compara via subtle | Mann-Whitney timing test em key compare path | N/A (cripto invariant) |
| **INV-AUTH-SCOPE-MIDDLEWARE-LEVEL** | Scope check enforced via Tower layer; routes sem layer = explicit `allow_unauthenticated()` opt-out | HIGH | `axum::routing` requires `require_scope(scope)` OR `allow_unauthenticated`; deny-by-default | Compile-time route definition + integration test | N/A (architecture invariant) |
| **INV-AUTH-AUDIT-PRE-POST-ORDERING** | Pre-handler `auth.token.validated` antes; post-handler `auth.{ok,denied}` depois; same outbox transaction | HIGH | Outbox INSERT em D1 batch atomic com TenantCtx commit (reuse WI-S01-005 pattern) + `tokio::catch_unwind` panic recovery | Property test pré/post pair completeness + chaos test panic recovery | Coberto parcial por `audit_immutability.tla` |
| **INV-AUTH-REVOCATION-IDEMPOTENT** | Retry revoke = single audit event + same revoked_at timestamp | CRITICAL | DO storage idempotent ingest dedup via `(pat_id, revoked_at)` UNIQUE constraint | Property test 100k retries com same pat_id assert single audit event | (planned `auth_revocation.tla`; PLANNED) |
| **INV-AUTH-REVOCATION-SLO-60S** | Cross-region propagation ≤ 60s p99 sustained 72h | CRITICAL | DO + CF Queue at-least-once; tiered alert SEV-2 em > 30s, SEV-1 em > 60s | SLO measurement + chaos test cross-region propagation stress | (planned `auth_revocation.tla`) |
| **INV-AUTH-NEON-IS-SOT** | **Neon `pat.revoked_at IS NULL`** filter authoritative em verify path; DO storage + KV session cache são hot-path optimization (não SoT) | HIGH | Verify cold path query Neon antes de approving; DO `is_revoked()` é admin orchestration path; KV é session cache only. Cycle 4 codex SEAL canonical alignment per data_model.md §4.1. | Architecture review + integration test verify-vs-revoke race | N/A (architecture invariant) |
| **INV-AUTH-MASS-REVOKE-ATOMIC** | Mass revoke UPDATE phase é all-or-none via Postgres transaction; outbox INSERT phase chunked (≤ 1000 rows per batch; eventual consistency aceitável) | CRITICAL | Phase 1: single `UPDATE pat SET revoked_at WHERE tenant_id = X` atomic em Neon transaction (canonical SoT). Phase 2: audit_outbox INSERT chunked em batches of 1000 (queue tolerates partial commit + retry). Cycle 4 codex SEAL fix. | Property test 10k mass revoke assert UPDATE all-or-none + outbox eventual completeness within 5min | (planned `auth_revocation.tla`) |
| **INV-AUTH-PROPAGATION-AT-LEAST-ONCE** | CF Queue at-least-once + consumer dedup via `(pat_id, revoked_at)` | HIGH | Queue retry policy 5×; DLQ; consumer idempotent ingest | Chaos test queue outage + consumer offline | N/A (queue semantic) |
| **INV-AUTH-SCHEMA-RLS-DEFAULT-ON** | All auth tables have RLS enabled (account/tenant/user_account/membership/**pat**/webauthn_credentials/revocation_log; cycle 4 codex SEAL: pat → pat canonical) | CRITICAL | `ALTER TABLE ... ENABLE ROW LEVEL SECURITY` em migration 002; CI gate verifies `pg_class.relrowsecurity = true` | CI test query `SELECT relrowsecurity FROM pg_class WHERE relname IN (...)` | N/A (DB invariant) |
| **INV-AUTH-PAT-HMAC-SIG-VERIFIED** | PAT verify path step 3a (HMAC sig check fast-fail) MUST execute before token_id lookup; reject 401 com NO DB hit + NO Argon2 cost se sig mismatch | CRITICAL | Constant-time compare `hmac_sig` vs `HMAC-SHA256(pat_signing_key, token_id\|\|"."\|\|random_secret)[:22]` em middleware; cycle 9 SEAL decision (a) hybrid HMAC + Argon2id; phishing + DDoS defense | Property test 100k random sig adversarial → 0 false-pass | (planned `auth_pat_hybrid.tla`) |
| **INV-AUTH-PII-ENCRYPTED** | email + webauthn keys store as BYTEA (ciphertext) via pgcrypto | CRITICAL | `pgp_sym_encrypt_bytea` em INSERT; raw text never persisted | CI test verify ciphertext format em rows; backup leak chaos | N/A (cripto + DB invariant) |
| **INV-AUTH-MIGRATION-ADDITIVE** | No DROP TABLE/COLUMN ou ALTER COLUMN destructive em migrations | HIGH | `scripts/check_migrations_additive.py` CI gate diff vs main | CI gate em PR | N/A (governance invariant) |
| **INV-AUTH-CASCADE-DSR-COMPLETE** | account DELETE cascades tenant + user_account + membership + pat + webauthn_credentials | HIGH | FK `ON DELETE CASCADE` policies em DDL; integration test full cascade | DSR cascade integration test | N/A (DB invariant) |
| **INV-AUTH-AUDIT-PSEUDONYMIZATION** | Audit chain retains pseudonymous IDs (sha256 prefix); DSR cascade não touches audit chain | CRITICAL | Pseudonymization em emit time; chain integrity preserved post-erasure | DSR integration test + LGPD Art. 18 compliance | Coberto por `audit_immutability.tla` |
| **INV-AUTH-WEBAUTHN-UV-REQUIRED-ADMIN** | Admin step-up requires UV=1 (biometric/PIN); UV=0 rejected | CRITICAL | `WebAuthnAdapter::finish_authentication` checks `flags & UV != 0` em admin paths | Adversarial regression test + CI integration test | N/A (W3C spec compliance) |
| **INV-AUTH-WEBAUTHN-ATTESTATION-VERIFIED** | Registration verifies attestation chain; AAGUID em allowlist | CRITICAL | `webauthn-rs::start_registration` + `finish_registration` enforces; AAGUID lookup em config | Cargo-fuzz CBOR/COSE 1h + adversarial test | N/A (W3C spec compliance) |
| **INV-AUTH-WEBAUTHN-SIGN-COUNT-MONOTONIC** | sign_count strictly increasing; first regression = SEV-2 alert (W3C-compliant policy per Lote 10.3-tris P0-R5-002b + cycle 6 codex SEAL); forensic confirmation escalates SEV-1 | HIGH | Server checks `response.sign_count > stored.sign_count`; W3C accepts sign_count=0 (não monotonic em passkey ecosystem); first regression triggers SEV-2; forensic confirmation escalates SEV-1 | Property test 1k random ceremonies + chaos test replay | N/A (W3C spec compliance) |
| **INV-AUTH-WEBAUTHN-ORIGIN-EXACT** | Origin allowlist exact match (no prefix bypass); rejects subdomain spoof | CRITICAL | `Vec<Url>` exact eq compare; W3C §13.4.9 origin matching | Adversarial regression test (origin spoof) + CI test | N/A (W3C spec compliance) |
| **INV-AUTH-WEBAUTHN-RP-ID-CANONICAL** | RP ID = "corelink.dev" eTLD+1 (not subdomain) | CRITICAL | `WebAuthnAdapter::new` validates rp_id é eTLD+1; fails se subdomain | Static check em construtor | N/A (W3C spec compliance) |
| **INV-AUDIT-NO-RAW-PII** | Zero raw PII em chain (email, raw principal_id, raw pat_id) | CRITICAL | `redact_pat!` macro mandatory; `hash_principal_id` 64-bit prefix; CI lint enforces | CI lint custom binary `tools/audit_pii_lint/` + grep CI gate | N/A (compile-time + CI lint) |
| **INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER** | Audit outbox INSERT em mesma D1 batch que TenantCtx commit; on-failure handler MUST surface the failure observably (no silent drops on fail-CLOSED paths) | CRITICAL | `db.batch([INSERT blob_meta OR TenantCtx, INSERT audit_outbox])` atomic; failure rolls back. **W26-P2-08 clarification (2026-05-16):** the "with handler" suffix ALSO covers synthetic-emit paths (`AuditSink::emit_synthetic`) used by orchestration-level fail-CLOSED arms. When the recorder backend can fault (e.g. poisoned mutex on test target), the handler MUST record telemetry via a side-channel that itself cannot re-fault — atomic counter (`AUDIT_MUTEX_POISON_TOTAL_PREFETCH_FAIL_CLOSED` in `corelink-clerk-cf::audit_sink`) + structured `tracing::error!` line. Silent drops on the audit-emit failure are forbidden because the operator loses observability into whether the 503 fail-CLOSED arm completed its audit row. The recording side-channel MUST NOT re-acquire the faulted resource nor call back into the audit chain (no double-fault). | Chaos test D1 batch failure + assert no orphan TenantCtx; **W26-P2-08**: `crates/corelink-clerk-cf/tests/audit_sink_mutex_poison_telemetry.rs` poisons the recorder mutex via thread-panic and asserts (a) `emit_synthetic` does not panic, (b) the atomic counter increments, (c) the recorder buffer remains empty (event was telemetered, not silently dropped). | **`audit_emit_atomic.tla` ✅ GREEN** (DEBT-005 batch 2 — 2026-05-15) — strengthens prior partial coverage via `audit_immutability.tla` to a direct batch-pairing biconditional `InvAtomicPairing` plus `InvNoOrphanOutbox` + `InvNoOrphanDomain`. W26-P2-08 clarification (2026-05-16) extends the registered surface to synthetic-emit side-channels; the TLA spec covers the D1-batch pairing which is the primary invariant; the side-channel telemetry is asserted by the integration test (lock-free atomic + tracing; not amenable to TLC modelling). |
| **INV-AUDIT-CHAIN-HASH-DETERMINISTIC** | content_hash deterministic via canonical JSON (RFC 8785 / serde_jcs) | HIGH | `serde_jcs` crate (JCS) + Unicode NFC; property test serialize twice byte-equal | Property test deterministic JSON 1000 events | N/A (cripto invariant) |
| **INV-AUDIT-EVENT-TYPE-EXHAUSTIVE** | All AuthEventType variants têm AuthEventData payload impl + serde tag | HIGH | Rust `match` exhaustive em internal handlers; `#[non_exhaustive]` em external surface | Compile-time + property test enum exhaustive | N/A (compile-time) |
| **INV-AUDIT-RETENTION-HINT-ACCURATE** | Event retention_hint matches tenant.tier (Solo30d/Team90d/Business1y/Enterprise7y) | HIGH | Lookup-time em emit; property test verify hint matches tier | Property test 4 tier types + integration test | N/A (architecture invariant) |
| **INV-NEG-CACHE-MONOTONIC** | Negative cache writes use monotonic version_stamp; older stamps rejected silently | HIGH | KV value envelope inclui version_stamp u64; put_miss compara antes write | Property test concurrent put_miss vs invalidate_on_write | N/A (KV invariant) |
| **INV-NO-BODY-IN-LOGS** | Body bytes nunca em audit/logs/error messages | CRITICAL | `redact_pat!` + clippy custom lint + grep CI gate | Static analysis + CI gate | N/A (compile-time + CI lint) |
| **INV-NO-PII-IN-LOGS** | Raw PII (email, principal_id) nunca em logs/traces; hashed prefix only | CRITICAL | `hash_principal_id` macro + tracing field redaction | Static analysis + CI gate | N/A (compile-time + CI lint) |
| **INV-AUTH-CONSTANT-TIME-COLD-PAD** | PAT verify cold-path latency é indistinguishable from warm-path via dummy Argon2id pad on every failure branch (parse fail, sig mismatch, token_id absent, hash mismatch) | CRITICAL | Middleware in `corelink-worker::middleware::auth` calls `corelink_pat::dummy_verify_for_constant_time` on every cold-path failure; attacker observing latency envelope cannot distinguish "token doesn't exist" from "token exists but mismatch" | Mann-Whitney 3-prong timing test + adversarial regression (DEBT-004 promotion) | N/A (cripto invariant; covered by `INV-AUTH-PAT-VERIFY-CONSTANT-TIME` Mann-Whitney; sibling refinement for cold-path) |

**Cross-references**:
- `ADR-0024..ADR-0033` documentam decisões S-03 (vide `scripts/validate_references.py` whitelist).
- TLA+ specs PLANNED em §4.2 (auth_jwt_validation, tenant_ctx_propagation, auth_revocation) — implementação em sprints S-09 ou S-12.
- `auth_model.md §8.1` (5-layer defense) é fonte canonical para INV-AUTH-5-LAYER-ORDERING.
- `key_management.md §3.13` documenta INV-AUTH-PAT-* details.
- `compliance_matrix.md` mapeia INV-AUTH-* para LGPD/GDPR/SOC 2/NIST AAL3 controls.

**Aliases históricos:** nenhum. Estes 38 IDs introduzidos em Lote 10.3 (sprint S-03 spec) e promovidos ao registry em Lote 10.3bis (P0 fix Agent R4 review remediation).

---

### 3.15 Action Cache domain (domain AC) — Lote 10.4 (S-04 sprint)

Invariantes que governam Action Cache lifecycle: REAPI handlers + idempotency, Merkle dual-side verify, HKDF digest signing, TTL infrastructure + tenant-scoped eviction, bounded parser. Promovidas ao registry em Lote 10.4bis (P0 fix Agent R4 review remediation r4-s04-part1+part2).

| ID | Nome | Severidade | Descrição | Enforcement | TLA+ file |
|---|---|---|---|---|---|
| **INV-AC-IDEMPOTENT** | UpdateActionResult em mesmo `(tenant_id, action_digest)` é no-op pós-verify | HIGH | INSERT ON CONFLICT (tenant_id, action_digest) DO UPDATE SET last_hit_at = excluded.last_hit_at; result_hash NUNCA mutável | Property test 10k iter `prop_ac_update_idempotent` + integration test re-update mesmo digest | (planned `cas_integrity.tla` AC variant; PLANNED) |
| **INV-AC-RESULT-HASH-IMMUTABLE** | Mismatch on re-update → 409 + audit emit; nunca silent overwrite | HIGH | Handler detect excluded.result_hash != current.result_hash → 409 COR_AC_RESULT_HASH_MISMATCH; ON CONFLICT clause não inclui result_hash em UPDATE list | Property test 10k iter mismatch detection + Gherkin scenario | N/A (architecture invariant) |
| **INV-AC-NEG-CACHE-INVALIDATED-ON-UPDATE** | KV ac_neg invalidated atomicamente em UPDATE success (post D1 INSERT, pre response) | HIGH | KV.delete `ac_neg:<tenant_prefix>:<action_digest>` step [9] em UpdateActionResult flow | Property test `prop_ac_negative_cache_invalidate` + integration test populate→update→hit | N/A (architecture invariant) |
| **INV-AC-MERKLE-VALID** | envelope.merkle_root binds tree structure; verify_structure rejects tampering 100% | CRITICAL | `corelink-ac::verify_structure` re-build tree from envelope.result; compare to envelope.merkle_root; mismatch → MerkleError::RootMismatch | Property test 10k iter `prop_merkle_tampering_detected` + chaos test envelope byte flip | **TLA+ em `specs/tla/merkle_integrity.tla`** (DEBT-005 closure 2026-05-15; AC envelope flavour) |
| **INV-AC-MERKLE-DETERMINISTIC** | Same input → same Merkle root byte-identical; lex sort + canonical encoding enforced | CRITICAL | Builder iterates outputs in lex sort by digest; canonical encoding (serde_jcs RFC 8785 sobre JSON projection OR result_hash = merkle_root direct per ADR-0037 Lote 10.4bis decision) | Property test 1000 ActionResults × 100 builds = 100% byte-identical | N/A (cripto invariant) |
| **INV-AC-BOUNDED-PARSER** | depth ≤ 32, fanout ≤ 4096, node_count ≤ 100k, payload ≤ 1 MiB enforced at decode time | HIGH | `corelink-ac::bounds` constants; decoder reject early; cargo-fuzz 1h CI nightly | Property test `prop_bounds_enforcement` 10k iter + cargo-fuzz harness | N/A (parser invariant) |
| **INV-AC-CYCLE-FREE** | Visited set rejects cycles; bounded recursion em output_directories | HIGH | `corelink-ac::merkle::verifier` tracks visited node digests; cycle → MerkleError::CycleDetected | Chaos test 100 crafted Directory cycle protos + property test | N/A (parser invariant) |
| **INV-AC-DUAL-SIDE-VERIFY** | Server pre-persist + client post-download both invokeable; verify_structure independent of sig (defense-in-depth para partial chave compromise) | HIGH | Server-side pre-persist em UpdateActionResult; client-side post-download via SDK (S-15 Rust SDK; non-Rust clients per spec doc reference vectors) | Integration test dual-side; chaos test partial chave compromise scenario | N/A (architecture invariant) |
| **INV-AC-DIGEST-SIGNED** | All envelopes signed com HKDF; verify mandatory em GET path; bypass = security control violation | CRITICAL | `corelink-ac::sig::HkdfVerifier::verify_sig` invoked em handler step [4]; canonical_bytes inclui BLAKE3(serialized_result) binding (Lote 10.4bis fix WI-S04-004 P0 #1) | Property test 10k iter sign-verify roundtrip + Mann-Whitney 3-prong cripto-grade | N/A (cripto invariant) |
| **INV-AC-SIG-CONSTANT-TIME** | Verify path constant-time via subtle::ConstantTimeEq; Mann-Whitney 3-prong cripto-grade | HIGH | `subtle::ConstantTimeEq::ct_eq` em sig compare; clippy custom lint forbids `==` em sig module; CI byte-equal gate em info string | Mann-Whitney N≥10000 com Šidák 3-trial + bootstrap 95% CI; cripto-grade target |Δmedian| ≤ 0.5ms | N/A (cripto invariant) |
| **INV-AC-SIG-INFO-FIXED** | HKDF info=`b"ac-sig"` fixed; salt=sig_key_id.to_le_bytes() per Lote 10.4bis fix WI-S04-004 P0 #2 | HIGH | Constant em código + CI byte-equal test asserts; commit hook validates | CI test grep + assert + ADR-0021 documents (ratificada Lote 10.4bis) | N/A (cripto invariant) |
| **INV-AC-KEY-ROTATION-GRACE** | Verifier accepts current + 1 previous key_id; post-grace rejects with KeyIdUnknown | HIGH | `accepted_key_ids: Vec<u32>` includes current + 1 prev; rotation event invalidates in-memory cache; integration test simulates rotation | Property test `prop_key_rotation_grace` + integration test cutover | N/A (cripto invariant) |
| **INV-AC-TDK-ZEROIZED** | TDK bytes Zeroizing wrapped; on drop memory cleared | MEDIUM | `Zeroizing<Vec<u8>>` wrap em TdkHandle::fetch return; never Display/Debug; redact macros from S-09 | Chaos test post-drop memory inspection (test environment: wasmtime host; CF Workers production env caveat) | N/A (cripto hygiene invariant) |
| **INV-AC-CANONICAL-BYTES-STABLE** | canonical_bytes layout fixed (extended em Lote 10.4bis para 121 bytes incl. result_hash binding); deterministic | HIGH | Layout: version (1) + tenant_id (16) + action_digest (32) + merkle_root (32) + result_hash (32) + created_at_ms (8) = 121 bytes; ADR-0021 ratificada Lote 10.4bis | Property test 1000 envelope variations byte-stable | N/A (cripto invariant) |
| **INV-AC-EVICT-TENANT-SCOPED** | DELETE strict tenant_id filter; cross-tenant impossible | CRITICAL | SQL `DELETE WHERE tenant_id = ? AND action_digest = ?` strict; CI grep gate em ttl module forbids DELETE sem tenant_id clause | Property test 10k iter `prop_ttl_tenant_isolation` + chaos #2 cross-tenant attempt | **`ac_eviction_isolation.tla` ✅ GREEN** (DEBT-005 batch 2 — 2026-05-15) — `InvDeletesTenantScoped` + adversarial cross-tenant action proved unreachable. |
| **INV-AC-EVICT-CONSISTENCY** | R2 DELETE before D1 DELETE; orphan R2 < orphan D1 ref; reconcile diário S-06 catches drift | HIGH | Cron flow: R2 DELETE → on success → D1 DELETE → KV invalidate → audit emit; if R2 fails, abort batch row + preserve D1; metric corelink.ac.ttl.r2_delete_failed_total | Chaos test #3 R2 outage + integration test cycle | N/A (architecture invariant) |
| **INV-AC-TTL-MONOTONIC** | Refresh-on-hit increments last_hit_at + extends expires_at; never decreases | HIGH | Handler refresh logic threshold 60s gate; `UPDATE ac_meta SET last_hit_at = $1, expires_at = $1 + tier_ttl WHERE …` | Property test `prop_ac_ttl_refresh_monotonic` + integration test storm | N/A (architecture invariant) |
| **INV-AC-PATH-KEY-MATERIALIZED** | tenant_prefix BLOB(16) materialized em ac_meta column (Lote 10.4bis fix; resolves cron TDK access expansion) | HIGH | Schema column `tenant_prefix BLOB(16) NOT NULL`; pre-computed em handler INSERT step [7]; cron worker reads column directly without TDK access | CI test schema column exists; integration test prefix consistency | N/A (architecture invariant) |
| **INV-AC-PATH-SIG-KEY-VERSION-INDEPENDENT** | `sig_key_id` and `path_key_id` MAY diverge per row (Lote 10.4-tris P0-R5-006); no atomic snapshot required across rotation; valid state, NOT invariant violation; handler INSERT uses current values at write time | HIGH | Two key-version columns in ac_meta (Lote 10.4bis WI-S04-002 schema); rotation procedures independent (WI-S04-004 §30.1); GET handler reads `tenant_prefix` from materialized column NOT recomputed from TDK | Property test `prop_path_sig_key_independent` simulates concurrent rotation between path-INSERT and sig-INSERT steps; integration test asserts no error on divergence | N/A (architecture invariant; ADR-0021 §RotationProcedures) |
| **INV-AC-ORPHAN-R2-CLEANUP-EVENTUAL** | Orphan R2 envelopes (R2 PUT succeeds, D1 INSERT fails) cleaned within 24h via S-06 reconcile cron | HIGH (forward-dependency S-06) | S-06 reconcile cron diário cross-checks R2 vs D1; orphan rate alert metric `corelink.ac.r2.orphan_rate` (alert if >1% of UPDATE rate) | S-06 forward; chaos test handler crash mid-flight; metric monitoring | N/A (architecture invariant; eventual consistency) |
| **INV-AC-OUTPUTS-VALID-EVENTUAL-CONSISTENCY** | INV-AC-OUTPUTS-VALID is point-in-time best-effort; TOCTOU race between handler outputs check and S-06 GC tombstone window accepted | HIGH (forward-dependency S-06) | SQLite/D1 has no row-level locking; Postgres `SELECT ... FOR SHARE` unavailable; reconcile diário (S-06 forward) é authoritative drift detection mechanism; orphan_rate metric monitors | S-06 forward reconcile cron; chaos test #8 (TTL eviction race + S-06 GC race); 24h SLA bound on drift correction | N/A (eventual consistency tier) |
| **INV-AC-TTL-REFRESH-MONOTONIC** | TTL refresh em `corelink-ac` infrastructure preserves monotonicity: refresh-on-hit clamps `last_hit_at` and `expires_at` to never move backward (sibling refinement of `INV-AC-TTL-MONOTONIC` covering the schema/sim layer) | HIGH | `corelink-ac-schema::sim` + `corelink-worker::reapi::ac::meta` clamp `last_hit_at` and `expires_at` via `MAX(prev, now)`; property test asserts cross-call monotonicity | Property test `prop_ac_ttl_refresh_monotonic` (10k iter) em `corelink-worker/tests/prop_ac_handlers.rs`; sim layer mirrored em `corelink-ac-schema/src/sim.rs` (DEBT-004 promotion) | N/A (architecture invariant; sibling INV-AC-TTL-MONOTONIC) |
| **INV-AC-REGION-PINNED** | AC handler rejects requests whose `region` mismatches the bound tenant region (handler-level region pinning before TTL/eviction execution) | HIGH | `corelink-worker::reapi::ac` handler checks `region == tenant.primary_region` strict; mismatch → 4xx + audit emit. Supports INV-DATA-RESIDENCY at the AC entry point. | Integration test `reapi_v2_ac_conformance::scenario region mismatch rejected` (DEBT-004 promotion) | N/A (architecture invariant; subsumed by INV-DATA-RESIDENCY) |
| **INV-AC-EVICT-REGION-PINNED** | TTL-eviction cron workers are pinned to a single region; per-region shards never touch other regions' rows via `select_expired_for_region` + `delete_tenant_scoped` (cross-region pollution structurally impossible) | CRITICAL | `corelink-worker::reapi::ac::ttl` enforces `region` mandatory in every cron query; SQL `WHERE region = ?` strict; CI grep gate forbids eviction queries sem `region` clause | Property test cross-region cron isolation + integration test region mismatch (DEBT-004 promotion) | **`ac_eviction_isolation.tla` ✅ GREEN** (DEBT-005 batch 2 — 2026-05-15) — `InvDeletesRegionScoped` + adversarial cross-region action proved unreachable. |
| **INV-AC-EVICT-AUDIT-EMITTED** | Every successful AC TTL row eviction emits a typed audit record via `AuditSink` trait (`AcEventType::EvictTtlExpired`); audit emit failure surfaces as per-row error keeping the row alive | HIGH | `corelink-worker::reapi::ac::ttl` calls `AuditSink::emit` after each row delete; emit failure → row preserved + per-row error logged; next pass retries (PAT-RETRY-IDEMPOTENT-001) | Chaos test simulate audit sink failure + integration test re-eviction (DEBT-004 promotion) | N/A (architecture invariant; sibling INV-GC-SWEEP-AUDIT-FAIL-CLOSED for AC TTL scope) |

**Cross-references**:
- `ADR-0021, ADR-0035, ADR-0036, ADR-0037` documentam decisões S-04 (vide `scripts/validate_references.py` whitelist).
- ADR-0021 ratificada em Lote 10.4bis (HKDF vs Ed25519 + canonical_bytes binding extension + salt parameterization).
- ADR-0019 ratificada em Lote 9.5b (TTL handoff S-04 → S-07).
- TLA+ specs PLANNED em §4.2 (cas_integrity.tla AC variant, tenant_isolation.tla AC eviction variant) — implementação em sprints S-09 ou S-12.
- `data_model.md §4.2` schema canonical (tenant_prefix column adicionada Lote 10.4bis).
- `error_taxonomy.md §3.2` mapeia AC errors (12 codes pós-Lote 10.4bis amendment).
- `compliance_matrix.md` mapeia INV-AC-* para LGPD/GDPR/SLSA L3 alignment.

**Aliases históricos:** nenhum. Estes 19 IDs introduzidos em Lote 10.4 (sprint S-04 spec) e promovidos ao registry em Lote 10.4bis (P0 fix Agent R4 review remediation r4-s04-part1+part2); 4 adicionais (INV-AC-TTL-REFRESH-MONOTONIC, INV-AC-REGION-PINNED, INV-AC-EVICT-REGION-PINNED, INV-AC-EVICT-AUDIT-EMITTED) promovidos em DEBT-004 closure (2026-05-15). **CI gate enforcement**: `scripts/validate_inv_promotion.py` valida que todo INV declarado em WI sob `specs/04_sprints/SXX/work_items/` existe nesta seção (closes 4-sprint persistent gap flagged em S-01/S-02/S-03 R4 reviews).

---

### 3.16 Multipart Upload + Chunking domain — Lote 10.5 (S-05 sprint)

Invariantes que governam multipart upload, chunking determinism, manifest dual-side verify, R2 multipart adapter, sweeper orphan abort. **Promovidas preemptivamente em Lote 10.5** (consistency com lesson Lote 10.4bis CI gate validate_inv_promotion.py — INVs declared em WIs DEVEM existir em registry pré-SEAL); refinements possíveis em Lote 10.5bis pós-Agent R4 review remediation.

| ID | Nome | Severidade | Descrição | Enforcement | TLA+ file |
|---|---|---|---|---|---|
| **INV-MULTIPART-IDEMPOTENT** | SplitBlob mesma `(tenant_id, blob_digest)` retorna existing manifest_digest; cas_blobs.is_chunked flag previne re-chunking; chunks ON CONFLICT increments refcount; multipart_sessions UNIQUE in_progress | HIGH | Handler step [3.5] short-circuits if is_chunked=true; D1 INSERT ON CONFLICT DO UPDATE refcount += 1; UNIQUE `(tenant_id, blob_digest, state='in_progress')` em multipart_sessions | Property test 10k iter `prop_split_idempotent` + integration test re-Split | (planned `cas_integrity.tla` chunked variant; PLANNED) |
| **INV-MULTIPART-MANIFEST-SIGNED** | Manifest envelope signed com HKDF info=`b"manifest-sig"` separated domain `b"ac-sig"` (WI-S04-004 ADR-0021 reuse pattern) | HIGH | `corelink-manifest::sig` HKDF info constant; CI byte-equal test asserts; ADR-0021/0038/0041 documents domain separation | CI test grep + assert + integration test cross-domain replay rejection | N/A (cripto invariant) |
| **INV-MULTIPART-CONCURRENCY-BOUNDED** | Per-tenant semaphore caps SplitBlob concurrency (default 4); 429 if exhausted; tunable per-tier S-13 forward | HIGH | `tokio::sync::Semaphore::new(4)` per-tenant; ConcurrencyLimitReached error → 429 + Retry-After; metric alert | Property test concurrency storm + chaos test 1000 parallel + **TLA+ em `specs/tla/multipart_finalize.tla`** (DEBT-014 FT-6 closed 2026-05-16; InvConcurrencyBounded proven) | N/A (architecture invariant) |
| **INV-MULTIPART-CHUNK-DETERMINISTIC** | FastCDC mask seeds fixed em ChunkerConfig::default(); same input → same chunks byte-identical (Fixed and FastCDC) | CRITICAL | `corelink-chunker` mask_s/mask_l constants; ADR-0022 stability commitment; test vectors Annex; CI byte-equal | Property test 1000 random × 100 chunkings = 100% byte-identical; ADR-0022 ratificada Lote 10.5 | N/A (cripto invariant) |
| **INV-MULTIPART-BOUNDED-PARSER** | Chunker MAX_BLOB_SIZE 160 GiB; MAX_CHUNKS_PER_BLOB 81920; manifest MAX_CHUNK_COUNT 81920 (Lote 10.5-tris P1-SR5-001/004 cross-crate alignment; 160 GiB / 2 MiB = 81920 exact); MAX_TOTAL_SIZE 160 GiB; enforced at decode time | HIGH | `corelink-chunker::bounds` + `corelink-manifest::bounds` constants share single source of truth; reject early; cargo-fuzz harness 1h CI nightly | Property test + cargo-fuzz × 3 targets (chunker fixed + chunker fastcdc + manifest decode) | N/A (parser invariant) |
| **INV-MULTIPART-STREAMING-MEMORY** | Per-request stack ≤ 4 MiB (chunker buffer 2 MiB + headroom); zero-allocation Iterator pattern | HIGH | `corelink-chunker::Chunker::feed` returns `impl Iterator<Item = Chunk<'a>>` borrowing internal buffer; lifetime-bounded; integration test 1 GiB blob no OOM | Integration test + valgrind/MSAN no leak | N/A (architecture invariant) |
| **INV-MULTIPART-ORPHAN-DETECTABLE** | `MultipartAdapter::list_orphans(bucket, max_age)` enumerates sessions with `last_activity_at < now - max_age` (default 7d); sweeper consumes; **guarantee holds across D1 shard splits** (sweeper multi-shard aware during dual-write window D-10 to D-3 per ADR-0040 §A1; Lote 10.5-tris P0-SR5-004 fix) | HIGH | D1 SELECT multipart_sessions WHERE state='in_progress' AND last_activity_at < now - max_age + R2 ListMultipartUploads; sweeper queries BOTH old + new shards during shard-split dual-write window; deduplicates by session_id | Sweeper cron DO test (WI-S05-006); RB-FM-060 dry-run; chaos test "mid-shard-split orphan detection" (Lote 10.5-tris) | N/A (architecture invariant; FM-060 mitigation; ADR-0040 §A1) |
| **INV-MULTIPART-PATH-TENANT-SCOPED** | R2 object_key inclui tenant_prefix (Layer 4); never trusts client-provided path components | CRITICAL | `MultipartAdapter` constructs object_key from tenant_prefix BLOB(16) materialized em chunks/multipart_sessions; CI grep gate forbids client-path concat | Property test 10k iter cross-tenant path attempt rejection | (planned `tenant_isolation.tla` multipart variant; PLANNED) |
| **INV-MULTIPART-STATE-MONOTONIC** | multipart_sessions state in_progress → completed OR aborted; never reverse | HIGH | Handler-level enforcement (não CHECK em D1 — CHECK doesn't model state transitions); INV documented + integration test asserts + **TLA+ em `specs/tla/multipart_finalize.tla`** (DEBT-014 FT-6 closed 2026-05-16; InvStateMonotonic + InvNoDoubleTerminal) | Integration test reverse transition rejected; chaos test | N/A (architecture invariant) |
| **INV-MULTIPART-PATH-KEY-MATERIALIZED** | tenant_prefix BLOB(16) materialized em chunks + multipart_sessions columns; cron worker reads sem TDK access (lesson Lote 10.4bis WI-S04-005) | HIGH | Schema columns `tenant_prefix BLOB NOT NULL` + `path_key_id INTEGER`; CHECK length=16; pre-computed em handler INSERT | CI test schema column exists; integration test prefix consistency | N/A (architecture invariant) |
| **INV-MULTIPART-MANIFEST-VALID** | manifest.merkle_root binds chunk tree; verify_structure rejects 100% tampered manifests | CRITICAL | `corelink-manifest::verify_structure` re-builds tree from manifest.chunks; compare to manifest.merkle_root; mismatch → MerkleError::RootMismatch | Property test 10k iter `prop_manifest_tampering_detected` + chaos test envelope tampering | **TLA+ em `specs/tla/merkle_integrity.tla`** (DEBT-005 closure 2026-05-15; manifest flavour) |
| **INV-MULTIPART-DUAL-SIDE-VERIFY** | Server pre-persist + client post-download both invokeable; verify_structure independent of sig (defense-in-depth para partial chave compromise) | HIGH | Server-side em UpdateActionResult-equivalent (SplitBlob handler); client-side via SDK (S-15 Rust SDK; non-Rust clients per spec doc reference vectors) | Integration test dual-side; chaos test partial chave compromise | N/A (architecture invariant) |
| **INV-MULTIPART-STREAMING-VERIFY-FAIL-FAST** | Mid-stream tampered chunk catches before chunk N+1 processed (SpliceBlob streaming verify) | HIGH | `corelink-manifest::verify_streaming` per-chunk hash verify inline; fail-fast em mismatch; signal handler abort | Property test 100 manifests × 1 tampered chunk → 100% caught at chunk N | N/A (architecture invariant) |
| **INV-MULTIPART-FINALIZE-IRREVOCABLE** | A finalized multipart session cannot be aborted (preserves the sealed manifest); a finalized session is the only Live → Finalized terminal state — abort on Finalized rejected with `SessionAlreadyFinalized` (HTTP 409); finalize on Aborted rejected with `Aborted` (HTTP 410); idempotent re-finalize on the same `(session_id, manifest_digest)` is a no-op echo | HIGH | `corelink-worker::reapi::cas::session::SessionStore::abort` returns `AlreadyFinalized` on `SessionState::Finalized` arm; `finalize` returns `Aborted` on `SessionState::Aborted`; `INV-MULTIPART-STATE-MONOTONIC` reinforces the broader monotone graph | Lib unit test `finalize_then_abort_rejected` + property test `prop_abort_safety` 1k iter (PR; nightly 100k via `PROPTEST_CASES`) + **TLA+ em `specs/tla/multipart_finalize.tla`** (DEBT-014 FT-6 closed 2026-05-16; InvFinalizeIrrevocable + InvFinalizedDigestStable; TLC 869 distinct states depth 6) | N/A (architecture invariant; first surfaced in WI-S05-001 SEAL 2026-05-01) |

**Cross-references**:
- `ADR-0022` ratificada Lote 10.5 (chunk vs part decoupling).
- `ADR-0038, ADR-0039, ADR-0040, ADR-0041` documentam decisões S-05 (vide `scripts/validate_references.py` whitelist).
- ADR-0040 sharding strategy (per-tenant_tier OR per-region; trigger 80% D1 10 GB hard limit).
- TLA+ specs PLANNED (cas_integrity.tla chunked variant, tenant_isolation.tla multipart variant) — implementação em sprints S-09 ou S-12.
- `data_model.md §4.X` schema canonical (chunks + manifest_chunks + multipart_sessions adicionadas Lote 10.5).
- `error_taxonomy.md §3.X` mapeia COR_MULTIPART_* errors.
- `compliance_matrix.md` mapeia INV-MULTIPART-* para LGPD/GDPR/SLSA L3 alignment.

**Aliases históricos:** nenhum. Estes 13 IDs introduzidos em Lote 10.5 (sprint S-05 spec) e **promovidos preemptivamente em Lote 10.5** (consistency com lesson Lote 10.4bis CI gate validate_inv_promotion.py); refinements possíveis em Lote 10.5bis pós-Agent R4 review.

---

### 3.17 Garbage Collection domain — Lote 10.6 (S-06 sprint)

Invariantes que governam GC mark-sweep + refcount reconciliation + TLA+ formal verification CI gate. **Promovidas preemptivamente em Lote 10.6** (consistency com lesson Lote 10.5/10.5bis); refinements possíveis em Lote 10.6bis pós-Agent R4 review.

| ID | Nome | Severidade | Descrição | Enforcement | TLA+ file |
|---|---|---|---|---|---|
| **INV-GC-IDEMPOTENT-RERUN** | Worker crashed mid-phase; resume from checkpoint = same final state | HIGH | gc_run table tracks status='running' / 'crashed'; idempotent re-run via PAT-RETRY-IDEMPOTENT-001; checkpoint per batch boundary | Property test 10k iter `prop_gc_run_idempotent_resume` + integration test crash mid-phase | (planned `gc_correctness.tla` already verified Lote 5.13) |
| **INV-GC-SINGLE-RUNNING-PER-TENANT-REGION** | Partial UNIQUE WHERE status='running' previne race | HIGH | `CREATE UNIQUE INDEX uq_gc_run_running ON gc_run(tenant_id, region) WHERE status='running'` (lesson Lote 10.5bis partial UNIQUE) | Integration test concurrent INSERT same (tenant, region) → second rejected | N/A (DB invariant) |
| **INV-GC-PHASE-MONOTONIC** | Valid transitions: idle→mark→sweep→physical_delete→reconcile→completed | HIGH | Handler enforces; reverse rejected; CHECK constraint inline em gc_run.phase | Property test phase transitions + integration test reverse rejection | N/A (architecture invariant) |
| **INV-GC-MARK-STARTED-AT-IMMUTABLE** | mark_started_at_ms captured ONCE atomically; immutable post-capture | CRITICAL | SQL `UPDATE gc_run SET mark_started_at_ms = unix_ms() WHERE run_id=? AND mark_started_at_ms IS NULL`; idempotent | TLA+ obligation `MarkPhaseStart` action; property test concurrent capture | Coberto por `gc_correctness.tla` |
| **INV-GC-DEGRADE-MODE-PROBE-PER-BATCH** | Worker probes degrade-mode at every batch boundary; abort ≤ 100ms | HIGH | DO config-singleton query per batch loop iteration; PAT-DEGRADE-001 alignment | Chaos test enable gc-pause mid-phase; abort latency bounded | N/A (architecture invariant) |
| **INV-GC-MARK-STARTED-AT-ATOMIC** | SQL UPDATE WHERE IS NULL atomic capture; idempotent re-run preserves | CRITICAL | Same SQL WHERE IS NULL; race-free; TLA+ obligation | Property test 1000 concurrent capture attempts | Coberto por `gc_correctness.tla` |
| **INV-GC-REACHABLE-SET-COMPLETE** | Mark phase 3-pass scan covers `union(blob_meta + ac_meta + manifest_chunks)` | CRITICAL | `corelink-gc::mark` 3 distinct passes; reachable superset-safe | Property test 10k random tenant states; reachable identified correctly | Coberto por `gc_correctness.tla` (InvGCReachableNeverDeleted; canonical TLA invariant name; was `InvGCNeverDeleteReachable` typo Lote 10.6 cycle 5 fix) |
| **INV-GC-MARK-TENANT-SCOPED** | All mark queries WHERE tenant_id = ctx.tenant_id; sqlx prepared; clippy lint | CRITICAL | sqlx prepared statements; clippy custom lint forbids `&str` SQL literals; TenantCtx-only enforcement | CI lint + integration test cross-tenant injection rejected | N/A (architecture invariant; Lote 10.4bis lesson) |
| **INV-GC-MARK-PHASE-BUDGETED** | Mark phase ≤ 10 min p99 @ 1M blobs; PhaseBudgetExceeded error if exceeds | HIGH | Criterion benchmark CI gate; SEV-2 alert if exceeds | Benchmark `bench_mark_1m_blobs` + integration test 5M blobs PhaseBudgetExceeded | N/A (SLO invariant; SLO-FRESH-GC) |
| **INV-GC-MARK-D1-BOUNDED-BATCH** | 250 rows/batch + 100ms jitter (Lote 10.4bis D1 100KB limit lesson) | HIGH | Hard-coded batch size; D1 throttle adaptive halve | Integration test batch boundary; D1 100KB constraint validated | N/A (architecture invariant) |
| **INV-GC-SWEEP-AUDIT-FAIL-CLOSED** | Audit emit failure → sweep ROLLBACK; preserves INV-GC-001 + INV-OBS-AUDIT-CHAIN-INTEGRITY | CRITICAL | D1 batch atomic (blob_meta UPDATE + gc_candidate UPDATE + audit_outbox INSERT); both succeed or both fail; SweepError::AuditEmissionFailed | Chaos test simulate audit_outbox INSERT failure; integration test asserts no soft-delete persists | **`gc_sweep_audit_fail_closed.tla` ✅ GREEN** (DEBT-005 batch 2 — 2026-05-15) — `InvSweepAtomicPairing` + `InvNoOrphanSweepAudit`. |
| **INV-GC-SWEEP-IDEMPOTENT** | Re-run on already-swept candidate = AlreadySwept no-op | HIGH | `gc_candidate.status` UPDATE atomic; AlreadySwept SweepDecision; PAT-RETRY-IDEMPOTENT-001 | Property test 1000 sweep + re-sweep same candidate | N/A (architecture invariant) |
| **INV-GC-SWEEP-TENANT-SCOPED** | All sweep queries tenant-scoped strict; cross-tenant impossible | CRITICAL | sqlx prepared + tenant_id NOT NULL + clippy lint | Property test 1000 concurrent sweeps different tenants; CI lint | N/A (architecture invariant; Lote 10.4bis lesson) |
| **INV-GC-GRACE-RESPECTED** | Physical-delete (WI-S06-004) only fires post-grace; CAP-GC-002 reversibility | HIGH | SQL filter `WHERE blob_meta.deleted_at < (now - grace_period)` strict `<`; env-config grace 72h CAS / 24h AC | Integration test boundary; chaos test reversibility | N/A (architecture invariant) |
| **INV-GC-PHYSICAL-DELETE-IDEMPOTENT** | Re-run on already-deleted = no-op (PAT-RETRY-IDEMPOTENT-001) | HIGH | R2 DeleteObject S3-compatible idempotent; D1 row purge idempotent | Property test 10k iter `prop_physical_delete_idempotent` | N/A (architecture invariant) |
| **INV-GC-GRACE-BOUNDARY-STRICT** | SQL `<` (NOT `<=`) grace boundary; reversibility window respected | CRITICAL | Hard-coded `<` em SQL filter; integration test boundary `deleted_at = exact boundary` → NOT deleted | Chaos test #1 boundary; integration test asserts | N/A (architecture invariant) |
| **INV-GC-R2-D1-ORDERING** | R2 DeleteObject before D1 purge; orphan recoverable; reconcile detects | HIGH | Atomic R2-then-D1 ordering (Lote 10.4bis WI-S04-005 lesson); chaos test R2 fail preserves D1 | Chaos test #2 R2 outage + integration test recovery | N/A (architecture invariant) |
| **INV-GC-DSR-BYPASS-AUTHORIZED** | DSR signal verified pre-bypass grace; S-11 forward auth | HIGH | DSR signal authentication mandatory; bypass path isolated | S-11 forward integration test stub | N/A (architecture invariant) |
| **INV-GC-RECONCILE-AUTO-FIX-BOUNDED** | Auto-fix gate: `drift_count ≤5 AND drift_percent ≤0.01%` per tenant (scale-invariant percentage-floor + absolute-floor; Lote 10.6bis P0-6 + Lote 10.6-tris OPUS-MISS-4); > 5 records OR > 0.01% → manual review + SEV-1; auto-fix failure mode: 3-attempt exponential backoff → `gc_drift_pending` table → SEV-2 (NOT SEV-1) | HIGH | Dual-condition gate (configurable env); SEV-1 alert + paused state if either condition exceeded | Property test `prop_auto_fix_scale_invariant` boundary at 10/1k/100k tenant sizes; integration test SEV escalation; `prop_auto_fix_failure_drift_pending` D1 throttle resilience | N/A (architecture invariant) |
| **INV-GC-RECONCILE-AUDIT-FAIL-CLOSED** | Audit emit failure → reconcile ROLLBACK | CRITICAL | D1 batch atomic; SweepError::AuditEmissionFailed analog | Chaos test simulate audit fail; integration test asserts | **`gc_sweep_audit_fail_closed.tla` ✅ GREEN** (DEBT-005 batch 2 — 2026-05-15) — `InvReconcileAtomicPairing` + `InvNoOrphanReconcileAudit`. |
| **INV-GC-CI-GATE-ENFORCED** | TLA+ CI gate blocks merge se TLC red; override via ADR + Architect + Crypto SME | HIGH | GitHub branch protection required status check; PR fail logic | CI workflow `.github/workflows/tla_check.yml` (Lote 10.11.0-bis-prime cycle 3 canonical); integration test PR weakening obligation rejected | (planned `gc_correctness.tla` Lote 5.13 verified) |
| **INV-GC-PROPERTY-TEST-CROSS-VALIDATED** | Rust property test 100k iter cross-validates TLA+ obligations (Mark + UpdateActionResult interleavings) | HIGH | `prop_gc_004_race_mark_update_ar_100k` em CI nightly; deterministic seeds; 0 violations sustained | CI nightly green sustained 30d (S-20 GA gate) | (cross-validation; gc_correctness.tla aligned) |
| **INV-GC-30D-SUSTAINED-VERIFICATION** | 30d sustained TLA+ verde + chaos zero violations gate pre-S-20 GA | HIGH | CI workflow `tla-30d-sustained.yml` (PLANNED — WI-S06-006 deliverable; not yet in tree) daily aggregate; chaos test 30d staging continuous | Sprint contract DoD §10.s06.4 + Critério Promoção | (governance invariant; Lote 10.6 ship gate) |
| **INV-GC-DEGRADE-CORRECT** | Worker preserves canonical correctness invariants (INV-GC-001 + INV-GC-004) under degrade-mode back-off ramp + overload-detector; alias for the cumulative degrade-mode contract aggregating `INV-GC-DEGRADE-MODE-PROBE-PER-BATCH` + `INV-GC-IDEMPOTENT-RERUN` + `INV-GC-PHASE-MONOTONIC` (WI-S06-007 §10.s06.007.7 cumulative INV §3.17 promotion + Lote 10.6bis P0-W7-2 count alignment 22→23) | HIGH | DO config-singleton probe + back-off ramp + per-instance overload detector; preserves correctness across mark/sweep/physical-delete/reconcile under load | Chaos test enable gc-pause mid-phase + property test 10k iter idempotent re-run + integration test back-off ramp boundary | N/A (architecture invariant; Lote 10.6 ship gate cumulative alias) |

**Cross-references**:
- `ADR-0042` documenta worker scheduler design + degrade-mode contract (vide `scripts/validate_references.py` whitelist).
- `specs/tla/gc_correctness.tla` (Lote 5.13 + 7.1 fixes) — formal verification baseline.
- `failure_modes.md FM-300/305/404` — runbook RB-FM-300/404/305 dry-runs em WI-S06-007.
- `security_model.md CTRL-GC-001/002` — control alignment.
- `slo_catalog.md SLO-CORRECT-GC + SLO-FRESH-GC` — operational metrics.

**Aliases históricos:** `INV-GC-DEGRADE-CORRECT` cumulative alias added in WI-S06-007 SEAL Lote 10.6 ship gate (§10.s06.007.7 cumulative INV §3.17 promotion + Lote 10.6bis P0-W7-2 count alignment 22→23). Estes 23 IDs (22 canonical + 1 cumulative alias) introduzidos em Lote 10.6 (sprint S-06 spec) e **promovidos preemptivamente em Lote 10.6** (consistency com lesson Lote 10.4bis CI gate validate_inv_promotion.py + lesson Lote 10.5 §3.16 promovida preemptive); refinements possíveis em Lote 10.6bis pós-Agent R4 review.

### 3.18 Dedup + Eviction + Quota domain — Lote 10.7 (S-07 sprint)

INVs introduced by Sprint S-07 (Dedup + Eviction Policy; STANDARD lane). Promovidas preemptivamente em Lote 10.7 (consistency com lesson Lote 10.4bis CI gate validate_inv_promotion.py); refinements possíveis em Lote 10.7bis pós-Agent R4/R5 reviews.

| INV | Description | Severity | Mechanism | Validation | TLA+ |
|---|---|---|---|---|---|
| **INV-EVICT-SOFT-DELETE-FIRST** | Eviction sets `blob_meta.deleted_at`; NEVER R2 DELETE direct (reuses S-06 GC grace 72h via WI-S06-004 physical-delete cron) | HIGH | WI-S07-002 §6.1.7 soft-delete batch; physical delete delegated to S-06 | Chaos test #1 + property `prop_evict_idempotent`; 30d sustained zero INV-GC-001 violations | N/A (architecture invariant; INV-GC-001 inheritance chain) |
| **INV-EVICT-CASCADE-PREVENTED** | Pre-evict reachable check refuses if blob is referenced via `ac_meta.blob_refs` (S-07 BLOB-scope); chunk-level reachability owned by S-06 GC via `chunks.refcount` + sweep (Lote 10.7bis P0-8 scope-reduce); canonical `json_each` SQL idiom | HIGH | WI-S07-002 §6.1.6 reachable check; SQL `SELECT COUNT(*) FROM ac_meta a, json_each(a.blob_refs) j WHERE a.tenant_id = ? AND j.value = ? AND a.deleted_at IS NULL AND a.created_at < ?` (race-protection contrapositive of INV-GC-004 protect-if->=); chunks lifecycle = S-06 GC scope NOT S-07 | Chaos test #2 + property `prop_evict_cascade_prevention` | N/A (cascade prevention; INV-GC-003 + INV-DEDUP-CONSISTENCY combined) |
| **INV-EVICT-TTL-CAP-RESPECTED** | Enterprise TTL ≤ 730d hard cap (CAP-EVICT-002 boundary); admin override > cap rejected by validator | MEDIUM | WI-S07-002 §6.1.3 `ttl_for_tier` hard cap em config validator | Chaos test #7 (override rejection) + integration test boundary | N/A (config invariant) |
| **INV-LRU-CONSISTENCY** | Eviction respects authoritative `last_accessed_at` via DO buffered + D1 base UNION lookup; eviction NEVER deletes blob accessed within tier_lru_window **modulo bounded DO fire-and-forget queue latency (≤ 100ms p99)**. Note: `record_access` DO buffer-add is fire-and-forget (non-awaited); a concurrent eviction worker that reads DO buffer within this window may see a stale value. The `evict_started_at_ms` watermark (INV-GC-004 inheritance pattern; WI-S07-002 §6.1.6) bounds the impact — blobs re-referenced after `evict_started_at_ms` are protected. Residual race window = DO actor queue latency (bounded; LRU drift acceptable per policy). Documented per S-07 R5 P2-1. | HIGH | WI-S07-004 §6.1.7 `last_accessed_at_authoritative` MAX(DO_buffered, D1_base); WI-S07-002 reachable check consumes with `evict_started_at_ms` watermark | Property test 10k iter `prop_lru_eviction_race` (sprint contract §6 DoD GC+Evict race) + chaos test #1 | N/A (architecture invariant; bounded-drift correctness) |
| **INV-QUOTA-RESERVATION-TTL** | Pending reservations auto-release after size-proportional TTL `min(7d, max(60s, request_bytes / 1MB/s × 2))` (canonical Lote 10.7bis R5 P0-2; floor 60s, ceiling 7d); eliminates FM-059 race window (concurrent writes at quota boundary); no quota leak | HIGH | WI-S07-003 §6.2 reservation lifecycle + DO alarm cleanup; sprint contract §15 R-S07-001 mitigation | Chaos test #4 + property `prop_quota_ttl_release` | N/A (DO actor model; race-free serialization) |

**Cross-references**:
- `ADR-0019` documenta TTL ownership boundary S-04 → S-07 supersedes (per-tier defaults).
- `ADR-0020` documenta Quota ownership boundary S-07 (≤95% trigger eviction) → S-08 (100% hard-block).
- `failure_modes.md FM-059, FM-300, FM-305` — runbook RB-FM-059 + RB-FM-305 dry-runs em WI-S07-005.
- `security_model.md CTRL-ISO-005, CTRL-QUOTA-001` — control alignment.
- `slo_catalog.md SLO-DEDUP-RATIO` (forward; defined in WI-S07-005 dashboard).

**Aliases históricos:** nenhum. Estes 5 IDs introduzidos em Lote 10.7 (sprint S-07 spec) e **promovidos preemptivamente em Lote 10.7** (consistency com lesson Lote 10.4bis CI gate validate_inv_promotion.py); refinements possíveis em Lote 10.7bis pós-Agent R4/R5 reviews.

---

### 3.19 Sub-Processor Transparency domain — Lote 10.11 (S-11 sprint WI-S11-005)

INVs introduced by Sprint S-11 WI-S11-005 (Sub-Processor Register + 30d Broadcast + Objection Flow; HIGH_RISK lane). Promovidas preemptivamente em Lote 10.11 per lesson Lote 10.4bis CI gate + Lote 10.8bis P1-13 (INV §3.X positions canonical verified pre-merge).

| INV | Description | Severity | Mechanism | Validation | TLA+ |
|---|---|---|---|---|---|
| **INV-SUB-PROCESSOR-BROADCAST-IDEMPOTENT** | UNIQUE constraint `(broadcast_id, tenant_id, recipient_email_hash, notification_type)` prevents duplicate 30d advance notice emails to the same recipient for the same broadcast event; idempotent retry safe | HIGH | D1 `sub_processor_broadcast_log` UNIQUE constraint (migration N+4); `InMemoryBroadcastStore` enforces at trait surface | Property test `prop_broadcast_idempotency_unique_constraint` 10k iter (PROPTEST_CASES=10000 verified); UNIQUE SQL constraint CI green | N/A (UNIQUE constraint; single-table idempotency; algorithmic) |
| **INV-SUB-PROCESSOR-BROADCAST-ALL-PLANS** | ALL 5 canonical plans (free/solo/team/business/enterprise) receive mandatory sub-processor notifications; `sub_processor_notifications` purpose has `legal_obligation` basis (privacy_model.md §5.6.1); NOT opt-out-able via consent_revoke; tier-gating FORBIDDEN | HIGH | ADR-S11-008 v2 + broadcast cron worker seeds ALL subscribed tenants without tier filter; `legal_obligation` purpose basis enforced at consent_ledger level | Regression test `test_mandatory_all_plans_no_tier_gating` (5 canonical plans all seeded); ADR-S11-008 Privacy Officer + Legal sign-off | N/A (policy invariant; ADR enforcement) |
| **INV-SUB-PROCESSOR-DKIM-TENANT-SCOPED** | DKIM signing key is derived per-tenant via HKDF-SHA256 (info=`corelink/v1/dkim-broadcast`); cross-tenant key isolation: tenant A's DKIM key NEVER equals tenant B's for any distinct tenant pair; prevents cross-tenant email spoofing | HIGH | `corelink-privacy-sub-processor-emit::dkim::derive_dkim_key` HKDF derivation with salt=tenant_id; security_model.md §374 inheritance | Property test `prop_dkim_cross_tenant_isolation` 10k random tenant pairs → 0 key collisions (PROPTEST_CASES=10000 verified) | N/A (HKDF algorithmic isolation; statistical property; non-distributed) |
| **INV-SUB-PROCESSOR-OBJECTION-STATE-MACHINE** | Objection ticket state machine has 5 canonical states (Pending/InReview/Accepted/Terminated/Withdrawn); valid transitions enforced; terminal states (Accepted/Terminated/Withdrawn) NEVER transition to any other state; state machine is monotonically progressing | HIGH | `ObjectionTicketStatus::can_transition_to` at trait surface; `InMemoryObjectionStore::update_status` enforces via state check | Property test `prop_objection_state_machine_valid_transitions` 10k iter all valid + all invalid transitions verified | N/A (state machine; algorithmic) |
| **INV-SUB-PROCESSOR-AUDIT-FAIL-CLOSED** | Sub-processor CloudEvent audit emission MUST succeed BEFORE any state mutation; if audit emit fails, operation aborts and state remains UNCHANGED; CD pipeline aborts on emit failure; aligns with INV-AUDIT-APPEND-ONLY §3.6 L116 | CRITICAL | `InMemorySubProcessorEmitter::publish/record_change/file_objection` audit-BEFORE-mutate ordering; `FailingSubProcessorAuditSink` test path verifies state unchanged | Regression tests `test_publish_fail_closed_on_audit_failure`, `test_change_fail_closed_on_audit_failure`, `test_objection_fail_closed_on_audit_failure` verify state UNCHANGED on audit failure; AC-007 covered | Covered by INV-AUDIT-APPEND-ONLY `audit_immutability.tla` ✅ GREEN inheritance |

**Cross-references**:
- `INV-AUDIT-APPEND-ONLY` (§3.6 L116): parent invariant — all 3 CloudEvents stored in R2 audit-`<region>` Object Lock 7y.
- `failure_modes.md FM-453` — broadcast miss detection + RB-SUB-PROCESSOR-BROADCAST-MISS mapping.
- `ADR-S11-008 v2` — mandatory all-plans rationale (GDPR Art. 28.2 + LGPD Art. 39).
- `crates/corelink-privacy-sub-processor-emit/` — trait surface + property tests.
- `legal/sub-processors.md` — source-of-truth YAML frontmatter.
- `migrations/N4__sub_processor_tables.sql` — D1 UNIQUE constraint DDL.

**Aliases históricos:** nenhum. Estes 5 IDs introduzidos em Lote 10.11 (sprint S-11 WI-S11-005) e **promovidos preemptivamente em Lote 10.11** (consistency com lesson Lote 10.4bis CI gate + Lote 10.8bis P1-13).

---

### 3.20 Backup verification domain (domain BACKUP) — DEBT-004 closure (2026-05-15)

INVs introduced by the `corelink-backup-verify` crate (R-prep continuous-verification scaffold). Promoted to registry in DEBT-004 closure pass — code-referenced but previously orphan in registry. Cross-references `RB-BACKUP-VERIFICATION.md` (forward) for operator workflow.

| ID | Nome | Severidade | Descrição | Enforcement | TLA+ file |
|---|---|---|---|---|---|
| **INV-BACKUP-FRESH** | Snapshot freshness gate: `verify_freshness` rejects any snapshot whose `age_seconds > rpo_seconds(tier)` | HIGH | `corelink-backup-verify::lib::verify_freshness` strict comparison; tier-driven RPO budget table; rejection emits typed error preserving the existing snapshot generation | Property test boundary at exact RPO; integration test stale-snapshot rejected (DEBT-004 promotion) | N/A (algorithmic invariant; tier-table-driven) |
| **INV-BACKUP-INTEGRITY-SAMPLE-CAP** | Integrity sampler never exceeds `MAX_INTEGRITY_SAMPLES_PER_TENANT` per tenant per cycle (avoids accidental O(catalog) hot loops) | HIGH | `MAX_INTEGRITY_SAMPLES_PER_TENANT = 100` constant em `corelink-backup-verify::lib`; sampler iterator is bounded; cargo-deny pinned bounds | Property test sampler bound enforcement + chaos test very-large-catalog (DEBT-004 promotion) | N/A (parser/bound invariant) |
| **INV-BACKUP-RESTORE-EPHEMERAL** | `sample_restore` always tags the restored namespace `ephemeral = true`; trait contract forbids restoring into a production-named namespace | CRITICAL | Real handler (production env) enforces ephemeral-namespace tag; trait contract type-state encodes ephemeral lifecycle; CI grep gate forbids restore-to-prod paths | Property test cross-namespace pollution rejection + integration test ephemeral-cleanup (DEBT-004 promotion) | **`backup_restore_ephemeral.tla` ✅ GREEN** (DEBT-005 batch 2 — 2026-05-15) — `InvRunningIsEphemeral` + `InvNoProdRestore` + liveness `EphemeralEventuallyTornDown` under WF. |

**Cross-references**:
- `RB-BACKUP-VERIFICATION.md` (forward; operator runbook).
- `failure_modes.md FM-BACKUP-*` (forward).
- `compliance_matrix.md` mapeia INV-BACKUP-* para SOC 2 CC9.1 (recovery testing) + LGPD Art. 46.

**Aliases históricos:** nenhum. Promoted in DEBT-004 closure pass (2026-05-15) from `corelink-backup-verify` crate orphan refs.

---

### 3.21 Billing portal domain (domain BILLING-PORTAL) — DEBT-004 closure (2026-05-15)

INVs introduced by the `corelink-stripe-real::portal` module (Stripe Billing Portal session creation contract). Promoted to registry in DEBT-004 closure pass.

| ID | Nome | Severidade | Descrição | Enforcement | TLA+ file |
|---|---|---|---|---|---|
| **INV-BILLING-PORTAL-URL-SINGLE-USE** | Every successful `create_session` call returns a `PortalSessionUrl` that has never been returned before, even for the same `(customer_id, return_url)` input | HIGH | Stripe issues a unique `bps_*` session id per call; the trait contract preserves this; in-memory test harness mirrors via HashSet uniqueness | Property test `portal_urls_unique_https_audit_consistent` (proptest 32 iterations per case; PROPTEST_CASES nightly) (DEBT-004 promotion) | N/A (algorithmic invariant; opacity guarantee) |
| **INV-BILLING-PORTAL-URL-HTTPS** | Every portal URL is HTTPS and embeds an opaque session id (the URL is bearer-equivalent — leaking it implies leaking a session) | HIGH | Trait contract requires `https://` prefix + `/p/session/bps_` path component; constructor validates; bearer-equivalent treatment documented in callers | Property test URL shape enforcement + adversarial regression (DEBT-004 promotion) | N/A (cripto + URL hygiene invariant) |
| **INV-BILLING-PORTAL-AUDIT** | Implementations MUST emit `corelink.billing.portal_session_created` to their bound audit sink before returning success | HIGH | `corelink-stripe-real::portal::BillingPortalSessionCreator` trait contract; audit emit BEFORE `Ok(url)` return; integration test asserts emit ordering | Integration test ordering + property test audit count = success count (DEBT-004 promotion) | N/A (architecture invariant; sibling of audit-fail-closed) |
| **INV-BILLING-PORTAL-AUDIT-FAIL-CLOSED** | Any path that does NOT return `Ok(url)` MUST NOT have recorded an audit row; any path returning `Ok(url)` MUST have exactly one audit row | HIGH | Trait contract enforces audit-emit-BEFORE-mutation discipline; failing emit aborts session creation (no orphan audit row); successful emit precedes URL return | Property test `portal_urls_unique_https_audit_consistent` audit-count assertion (10k iter nightly) (DEBT-004 promotion) | Covered by `audit_immutability.tla` (split-tier ADR-S11-002 inheritance) |

**Cross-references**:
- `audit_immutability.tla` ✅ GREEN — INV-AUDIT-APPEND-ONLY parent.
- `ADR-S11-002` split-tier fail-CLOSED discipline.
- `compliance_matrix.md` mapeia INV-BILLING-PORTAL-* para SOC 2 CC6.1 (access control to billing self-service).

**Aliases históricos:** nenhum. Promoted in DEBT-004 closure pass (2026-05-15) from `corelink-stripe-real` crate orphan refs.

---

### 3.22 Rate-limit response body domain (domain RATE-BODY) — DEBT-004 closure (2026-05-15)

INVs introduced by the `corelink-rate-headers` crate (RFC 9331 rate-limit header / JSON-body envelope contract). Promoted to registry in DEBT-004 closure pass.

| ID | Nome | Severidade | Descrição | Enforcement | TLA+ file |
|---|---|---|---|---|---|
| **INV-BODY-HEADER-MIRROR-1** | Rate-limit body `kind` discriminator equals the `x-rate-limit-type` header on every 429 response | HIGH | Renderer derives `body.kind` and `h.x_rate_limit_type` from the same internal `RateLimitKind`; serialization roundtrip preserves equality | Property test `prop_rate_headers` 10k iter asserts equality on every shape (DEBT-004 promotion) | N/A (algorithmic invariant; serializer mirror) |
| **INV-BODY-HEADER-MIRROR-2** | Rate-limit body `retry_after_seconds` equals the `Retry-After` header on every 429 response | HIGH | Renderer derives both from the same internal duration; integer-second formatting deterministic | Property test `prop_rate_headers` retry-after equality (DEBT-004 promotion) | N/A (algorithmic invariant; serializer mirror) |
| **INV-BODY-HEADER-MIRROR-3** | Body `limit`/`remaining`/`reset` mirror RFC 9331 `RateLimit` header fields exactly | HIGH | Single source of truth in `RateLimitHeaders` struct; renderer projects both views; RFC 9331 §2 conformance | Property test `prop_rate_headers` triple equality on RFC fields (DEBT-004 promotion) | N/A (algorithmic invariant; RFC conformance) |
| **INV-BODY-HEADER-MIRROR-4** | Vendor-prefix fields (`corelink-tier`, `corelink-quota-reset-utc`) mirror exactly between body and header | HIGH | Vendor extension fields share the same `RateLimitHeaders` source; deterministic projection | Property test `prop_rate_headers` vendor-field equality (DEBT-004 promotion) | N/A (algorithmic invariant; vendor namespace mirror) |
| **INV-BODY-FROZEN-URLS** | Rate-limit body `tier_upgrade_url` and `docs_url` are frozen constants (`TIER_UPGRADE_URL`, `DOCS_URL`) — GA contract stability | HIGH | Constants em `corelink-rate-headers`; renderer never templates; CI byte-equal test asserts | Property test `prop_rate_headers` constant-URL equality + GA stability commitment (DEBT-004 promotion) | N/A (architecture invariant; GA stability) |
| **INV-BODY-STABLE-CODE** | At GA, the 429 error envelope has exactly one stable error code: `ERROR_CODE_RATE_LIMIT_EXCEEDED` | HIGH | `body.code` is a frozen constant; CI byte-equal test; error_taxonomy.md alignment | Property test `prop_rate_headers` code equality (DEBT-004 promotion) | N/A (error taxonomy invariant) |
| **INV-BODY-RENDER-WELL-FORMED** | Rendered JSON envelope (`render_json()`) is well-formed: balanced braces, valid JSON, RFC 8259 conformance | HIGH | `serde_json` based renderer; envelope prefix `{"error":{` asserted; integration tests parse the output back | Property test `prop_rate_headers` parse-roundtrip (DEBT-004 promotion) | N/A (serializer invariant) |

**Cross-references**:
- `error_taxonomy.md §3.X` — 429 envelope canonical form.
- RFC 9331 §2 — `RateLimit` header policy.
- `compliance_matrix.md` mapeia INV-BODY-* para API stability commitment (GA gate L23 envelope-stability sub-row).

**Aliases históricos:** nenhum. Promoted in DEBT-004 closure pass (2026-05-15) from `corelink-rate-headers` crate orphan refs.

---

### 3.23 CAS handler SLI domain (domain HANDLER-SLI) — DEBT-004 closure (2026-05-15)

INVs introduced by the `corelink-handler-cas` crate (CAS handler entry/correctness + SLI emission). Promoted to registry in DEBT-004 closure pass.

| ID | Nome | Severidade | Descrição | Enforcement | TLA+ file |
|---|---|---|---|---|---|
| **INV-HANDLER-SLI-EMIT-ENTRY** | Every CAS handler entry emits one `SliObserver::observe_*` call BEFORE returning (covers `Sli::AvailCasGet` / `Sli::AvailCasPut` availability counters regardless of outcome) so the multi-burn-rate alert evaluator never misses a request | HIGH | `corelink-handler-cas::handler` invokes observer in a guard at the top of the handler body; CI grep gate forbids early-return paths sem observer call; property test asserts observer-count = request-count even on error paths | Property test `prop_handler_cas` 10k iter request-count vs observe-count equality (DEBT-004 promotion) | N/A (architecture invariant; SLO-AVAIL-CAS-GET / SLO-AVAIL-CAS-PUT measurement integrity) |
| **INV-CAS-CORRECTNESS** | Every CAS read returns either bytes whose hash matches the requested key OR a hash-mismatch error (`Sli::CorrectnessCas` failure observation) — never silently returns wrong bytes | CRITICAL | `corelink-handler-cas::handler` verifies hash on read path; mismatch → `Sli::CorrectnessCas` failure observation + error return; fakes mirror this for proptest coverage | Property test 10k iter hash-mismatch injection + integration test correctness counter (DEBT-004 promotion) | (subsumido por `cas_integrity.tla` ✅ GREEN — InvPoisoningRejected) |

**Cross-references**:
- `cas_integrity.tla` ✅ GREEN — INV-CAS-INTEGRITY parent (poisoning rejection).
- `slo_catalog.md SLO-AVAIL-CAS-GET + SLO-AVAIL-CAS-PUT + SLO-CORRECT-CAS` — operational metrics.
- `compliance_matrix.md` mapeia INV-HANDLER-SLI-* para SOC 2 CC7.1 (system monitoring) + multi-burn-rate alert canonical S-09.

**Aliases históricos:** nenhum. Promoted in DEBT-004 closure pass (2026-05-15) from `corelink-handler-cas` crate orphan refs.

---

### 3.24 Observability export domain (domain OBS-EXPORT) — DEBT-004 closure (2026-05-15)

INVs introduced by the `corelink-otel-export` crate (R-prep enterprise observability — Datadog/Grafana/AWS forward). Promoted to registry in DEBT-004 closure pass.

| ID | Nome | Severidade | Descrição | Enforcement | TLA+ file |
|---|---|---|---|---|---|
| **INV-OBS-EXPORT-FAIL-OPEN** | Every exporter fails-OPEN — when the customer's observability stack is down (HTTP 5xx, network unreachable, auth failure), the worker continues serving requests and emits an audit-style event to the local sink rather than blocking the data path | HIGH | `corelink-otel-export::lib + ::audit + ::error` enforces fail-OPEN at every export call; integration test simulates vendor outage; audit emit on every failure path; CI grep gate forbids `?` operator on export error in handler-path | Property test `prop_otel_export` 10k iter handler-availability under vendor outage (DEBT-004 promotion) | N/A (architecture invariant; availability over consistency for forward-only telemetry) |
| **INV-OBS-NO-PII** | Every metric / trace / log forwarded to a third-party MUST be PII-free (sprint contract §10.s09.6; cross-link `observability_model.md §10`) | CRITICAL | `corelink-otel-export::metric + ::lib` allowlist of label keys; PII patterns rejected at construction; `redact_*` macros + clippy custom lint; CI grep gate | Property test `prop_otel_export` 10k iter random labels asserts allowlist + adversarial PII injection rejection (DEBT-004 promotion) | N/A (compile-time + CI lint; subsumido por INV-NO-PII-IN-LOGS family) |
| **INV-OBS-CT-SECRET-EQ** | API-key + password equality goes through `secret::constant_time_secret_eq` (constant-time over the underlying bytes, with a length-mismatch short-circuit that does NOT leak the secret length) | HIGH | `corelink-otel-export::secret` uses `subtle::ConstantTimeEq`; clippy custom lint forbids `==` on secret types; CI byte-equal gate | Mann-Whitney 3-prong timing test em `prop_otel_export` (DEBT-004 promotion) | N/A (cripto invariant; sibling of INV-AUTH-PAT-VERIFY-CONSTANT-TIME family) |
| **INV-OBS-CONFIG-NON-EXHAUSTIVE** | Every per-vendor config struct is `#[non_exhaustive]` so adding a new auth field (mTLS, OAuth2 client credentials, AWS SigV4 for Datadog AWS) lands additively without breaking downstream consumers | HIGH | `#[non_exhaustive]` attribute on every public config struct em `corelink-otel-export`; CI grep gate; ADR documents additive evolution discipline | Compile-time enforcement + CI grep gate (DEBT-004 promotion) | N/A (architecture invariant; API stability) |

**Cross-references**:
- `observability_model.md §10` — PII policy parent.
- `compliance_matrix.md` mapeia INV-OBS-EXPORT-* para SOC 2 CC7.1 + LGPD Art. 18 (transferência internacional via forward telemetry).
- INV-NO-PII-IN-LOGS / INV-NO-BODY-IN-LOGS — sibling family in §3.14.

**Aliases históricos:** nenhum. Promoted in DEBT-004 closure pass (2026-05-15) from `corelink-otel-export` crate orphan refs.

---

### 3.25 Tenant offboarding domain (domain OFFBOARDING) — DEBT-004 closure (2026-05-15)

INVs introduced by the `corelink-tenant-offboarding` crate (R-prep tenant-offboarding orchestrator scaffold). Promoted to registry in DEBT-004 closure pass.

| ID | Nome | Severidade | Descrição | Enforcement | TLA+ file |
|---|---|---|---|---|---|
| **INV-OFFBOARDING-GRACE-RESPECTED** | Every state advances only after the canonical timer threshold (or operator force-advance with audit trail); orchestrator validates the timer at the trait boundary; a request that asks for an advance before the threshold returns `TenantOffboardingError::IllegalTransition` | HIGH | `corelink-tenant-offboarding::orchestrator` enforces `now >= state_started_at + grace_for(state)` on every advance call; force-advance path required to emit a typed audit row | Property test `prop_tenant_offboarding` 10k iter boundary + integration test force-advance audit (DEBT-004 promotion) | N/A (architecture invariant; sibling of INV-GC-GRACE-RESPECTED) |
| **INV-OFFBOARDING-AUDIT-COMPLETE** | Every state mutation is preceded by the canonical audit row (per ADR-S11-002 split-tier fail-CLOSED discipline); audit emit failure aborts the transition; durable store remains at pre-call state | CRITICAL | `corelink-tenant-offboarding::lib` audit-emit-BEFORE-mutate ordering; trait contract requires emit success precondition; `FailingAuditSink` test path verifies state unchanged | Property test `prop_tenant_offboarding` audit-mutation ordering 10k iter + chaos test simulate audit failure (DEBT-004 promotion) | Covered by `audit_immutability.tla` ✅ GREEN inheritance |

**Cross-references**:
- `audit_immutability.tla` ✅ GREEN — INV-AUDIT-APPEND-ONLY parent.
- `ADR-S11-002` split-tier fail-CLOSED discipline.
- `compliance_matrix.md` mapeia INV-OFFBOARDING-* para LGPD Art. 18 (DSR erasure) + GDPR Art. 17 (right to be forgotten).

**Aliases históricos:** nenhum. Promoted in DEBT-004 closure pass (2026-05-15) from `corelink-tenant-offboarding` crate orphan refs.

---

### 3.26 Progressive rollout domain (domain ROLLOUT) — DEBT-004 closure (2026-05-15)

INVs introduced by the `corelink-rollout-controller` crate (WI-S13-005 progressive rollout + 3-trigger auto-rollback). Promoted to registry in DEBT-004 closure pass.

| ID | Nome | Severidade | Descrição | Enforcement | TLA+ file |
|---|---|---|---|---|---|
| **INV-ROLLOUT-SINGLE-ACTIVE** | At most one active rollout per env at any time; concurrent start returns `RolloutInFlight` | HIGH | D1 `UNIQUE (status='active')` partial index per `(env)`; controller checks pre-start; `RolloutInFlight` error on conflict | Property test `prop_rollout` 10k iter concurrent-start rejection + integration test (DEBT-004 promotion) | (planned `rollout_state_machine.tla`; PLANNED) |
| **INV-ROLLOUT-NO-STAGE-SKIP** | Stage advance enforces `RolloutStage::next()`; skip → 403 + audit emit | HIGH | `corelink-rollout-controller::state_machine + ::controller + ::types` enforce monotonic `next()`; skip path returns typed `StageSkip` error + audit emit; CI grep gate | Property test `prop_rollout` 10k iter stage transitions + integration test (DEBT-004 promotion) | (planned `rollout_state_machine.tla`; PLANNED) |
| **INV-ROLLOUT-COSIGN-GATE** | Deploy without a valid Cosign signature is rejected at `start()` (S-12 supply chain herdada) | CRITICAL | `corelink-rollout-controller::lib::start` verifies Cosign signature inclusion in deployable; failure → typed `CosignMissing` error + audit emit (subsumido por INV-SUPPLY-SIGNED-DEPLOY at the rollout entry point) | Integration test sig-missing rejection + adversarial test (DEBT-004 promotion) | **`rollout_cosign_gate.tla` ✅ GREEN** (DEBT-005 batch 2 — 2026-05-15) — `InvActiveIsSigned` + `InvRejectionAudited` + `InvActivationAudited`; adversarial `AttemptUnsignedActivate` enumerated as FALSE-guarded unreachable trace. Also discharges runtime side of INV-SUPPLY-SIGNED-DEPLOY via `InvSupplySignedDeploy`. |
| **INV-ROLLOUT-BUDGET-CAP** | Auto-rollback consumes ≤ 30% monthly error budget; exceedance → freeze + SEV-2 | HIGH | `corelink-rollout-controller::controller + ::lib` tracks budget consumption per rollback fire; exceedance → freeze state + SEV-2 alert + audit emit | Property test `prop_rollout` budget-cap boundary + integration test SEV escalation (DEBT-004 promotion) | N/A (architecture invariant; SLO budget invariant) |
| **INV-ROLLOUT-AUTO-ROLLBACK-TRIGGERS** | 3-trigger auto-rollback (error-rate > baseline+3σ; SLO burn-rate > 14.4 1h window; p99 latency > baseline+50%) only fires after `SUSTAINED_THRESHOLD_SECS = 300s` continuous observation (filters transient variance); detection p99 ≤ 360s (6 probes × 60s) | HIGH | `corelink-rollout-controller::auto_rollback::SustainedTrigger` tracks per-trigger elapsed; reset to 0 on clear; fire only at ≥ 300s. Google SRE Workbook Ch 16 alignment. | Property test `prop_rollout` 10k iter sustained-threshold boundary + chaos test transient-spike-no-fire (DEBT-004 promotion) | (planned `rollout_state_machine.tla` auto-rollback variant; PLANNED) |

**Cross-references**:
- WI-S13-005 §6.1.3 (auto-rollback design).
- Google SRE Workbook Ch 16 — multi-window multi-burn-rate alerting.
- INV-SUPPLY-SIGNED-DEPLOY (§3.10) — parent of INV-ROLLOUT-COSIGN-GATE.
- `compliance_matrix.md` mapeia INV-ROLLOUT-* para SOC 2 CC8.1 (change management) + SLSA L3 (provenance gate).

**Aliases históricos:** nenhum. Promoted in DEBT-004 closure pass (2026-05-15) from `corelink-rollout-controller` crate orphan refs.

---

### 3.27 Operational discipline domain (domain OPS / S-17) — Wave-23 invariant-draft sweep (2026-05-16)

INVs introduced in `specs/04_sprints/S17/_spec_contract.md §8 "Cross-WI invariants (S-17 operational discipline)"` (Wave-19 sprint expansion) but never promoted to the canonical registry. Promoted here from S-17 `_spec_contract` declarations as part of the Wave-23 invariant-draft sweep audit. Severity HIGH (operational discipline → ambiguous SLO attribution / oncall capacity / GA-blocking chaos-in-prod).

| ID | Nome | Severidade | Descrição | Enforcement | TLA+ file |
|---|---|---|---|---|---|
| **INV-S17-OPS-EXCLUSIVITY** | At most one of {chaos experiment, DR drill, runbook drill} active per region at any time | HIGH | Shared `ops_event_lock` D1 row checked by each scheduler's pre-flight; aborts with `LockHeld` if violated. Rationale: concurrent chaos + DR drill produces ambiguous SLO impact attribution | Integration test concurrent-start rejection + scheduler pre-flight lock check; chaos test attempting concurrent ops | N/A (operational invariant; D1 row-level lock semantics) |
| **INV-S17-SEV1-DRILL-PAUSE** | Runbook drills, chaos experiments, and DR drills auto-deferred during active global SEV-1 incident | HIGH | `incident_active` flag check in scheduler `should_run()`; emits `corelink.ops.drill_deferred` audit event when active. Rationale: drills during real incidents starve oncall capacity | Integration test SEV-1 flag-set scheduler short-circuit + audit emit verification | N/A (operational invariant; flag-gated scheduler semantics) |
| **INV-S17-CHAOS-STAGING-ONLY** | Chaos experiments MUST NEVER target production environment | HIGH | `DrillEnv::require_staging()` (`corelink-chaos-scheduler`) AND `[env.prod]` wrangler config omits `[triggers]`; audit fail-CLOSED `Aborted{reason="prod_target"}` if attempted. Rationale: chaos in prod is GA-blocking per S-17 spec contract §10 | Two-layer defense: runtime `require_staging()` reject + wrangler config absence; adversarial test attempting prod target → expected `Aborted` | N/A (operational invariant; layered defense pattern) |
| **INV-S17-ONCALL-FATIGUE-AUTOROTATE** | Primary oncall hitting hard thresholds auto-handoff to backup | HIGH | `corelink-oncall::threshold` module observes Sev1>3/week, Sev2>8/week, total pages>15/non-rotation week; triggers `PagerDutyClient::handoff_to_backup`; emits audit before rotation. Rationale: alerts alone insufficient — observed fatigue without escape valve causes silent quality decline | Integration test threshold-cross handoff + audit emit ordering check | N/A (operational invariant; observation-driven handoff) |

**Cross-references**:
- `specs/04_sprints/S17/_spec_contract.md §8` — origin declarations (Wave-19 sprint expansion).
- `specs/03_architecture/resilience_patterns.md` — PAT-RUNBOOK-DRILL-001 + PAT-CORRELATION-ID-001 sibling operational patterns.
- `compliance_matrix.md` — operational discipline mapped to SOC 2 CC7.3 (system operations) + SRE-grade GA gate (S-17 §10).

**Aliases históricos:** nenhum. Declared in S-17 `_spec_contract.md` Wave-19 and promoted to registry in Wave-23 invariant-draft sweep (2026-05-16).

---

### 3.28 PAT revocation domain (domain AUTH-PAT) — Wave-23 invariant-draft sweep (2026-05-16)

INV introduced in `apps/docs/static/openapi-corelink-v1.yaml` + 4 i18n MDX endpoint references (`apps/docs/.../delete-v1-pats-by-pat_id.mdx`) for the `DELETE /v1/pats/{pat_id}` endpoint 204 response. Promoted here from public API contract docs as part of the Wave-23 invariant-draft sweep audit. Complements the existing `INV-AUTH-PAT-*` family (§3.14) which covers mint / verify / hash invariants; this INV covers the **revocation propagation** lifecycle.

| ID | Nome | Severidade | Descrição | Enforcement | TLA+ file |
|---|---|---|---|---|---|
| **INV-PAT-REVOKE-PROPAGATION** | Subsequent uses of a revoked PAT MUST fail closed (401) within propagation window | CRITICAL | Revocation writes `revoked_at` timestamp + audit emit; verify path checks `revoked_at IS NULL` in D1 query (no edge cache lookahead); propagation window ≤ 60s end-to-end (inherits S-03 admin role revocation pattern per WI-S13-002 §7 L161). Failure mode: stale token usage post-revocation returns 401, never 200/204 | Integration test mint → revoke → verify-rejection + property test 10k iter rapid mint/revoke race + audit emit ordering check (revoke audit BEFORE response 204) | `specs/tla/auth_pat_revoke.tla` + `.cfg` (PR) + `_nightly.cfg` — **TLA-VERIFIED** (Wave-24 R-PREP 2026-05-16) — `InvRevokedTokenNeverValidates` + `InvRevokeAuditAtomic` + `InvRevokeIsIdempotent` + `InvRegionImpliesSoTRevoked` + liveness `InvRevokeAtLeastOncePropagation`; sibling of `auth_pat_hybrid.tla` for verify path |

**Cross-references**:
- `apps/docs/static/openapi-corelink-v1.yaml` — DELETE /v1/pats/{pat_id} 204 response semantics.
- `apps/docs/docs/reference/api/endpoints/delete-v1-pats-by-pat_id.mdx` (+ 3 i18n: pt-BR / de / es-419) — customer-facing fail-closed contract.
- INV-AUTH-PAT-* family (§3.14) — sibling mint/verify/hash invariants.
- WI-S13-002 §7 L161 — runtime admin_role revocation check (60s propagation window pattern).

**Aliases históricos:** nenhum. Declared in public OpenAPI + 4 i18n MDX endpoint docs (apps/docs) and promoted to registry in Wave-23 invariant-draft sweep (2026-05-16).

### 3.29 Pilot signup token idempotency domain (domain SIGNUP-TOKEN) — Wave-30 stream-4 R-PREP (2026-05-16)

INV introduced in `apps/server/src/routes/signup.rs` `insert_or_existing` (wave-29 stream-1 commit `b3c359f`) for the `POST /v1/signup/pilot/{token}` pilot-slot reservation endpoint. Promoted here from DRAFT → PROMOTED + TLA-VERIFIED as part of the Wave-30 stream-4 R-PREP audit, closing the wave-29 stream-10 closure §6.2 deferred candidate. Sibling of `INV-SIGNUP-RESIGNUP-IDEMPOTENT` (§4.2 row, covered by `signup_resignup.tla` DEBT-014 FT-9) which models the **production tenant-provisioning** path with Stripe webhook + DPA-first; THIS INV covers the **pilot pre-Stripe RESERVED** path with HMAC-verified mint tokens + insert-or-existing SoT.

| ID | Nome | Severidade | Descrição | Enforcement | TLA+ file |
|---|---|---|---|---|---|
| **INV-SIGNUP-TOKEN-IDEMPOTENT** | Consumption of a pilot signup token MUST be exactly-once across replays | HIGH | A successful tenant allocation per email or per token fires at most ONCE; subsequent replays (same email with fresh token, or same token regardless of email) MUST return the original tenant_id and emit `exit_status="duplicate"` audit row, never a second `reserved` allocation. Failed audit emit fails CLOSED (503; INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER inheritance). Failure mode: a single pilot email forks into two tenants and consumes two pilot slots from the cohort cap | Integration tests `apps/server/tests/signup_pilot.rs::{duplicate_email_returns_original_tenant_id, happy_path_valid_token_returns_201, audit_emit_failure_returns_503_fail_closed}` + unit test `signup::tests::in_memory_store_dedupes_on_email` + property at the SoT level via `insert_or_existing` iter-find-first-match semantics | `specs/tla/signup_token_idempotent.tla` + `.cfg` (PR) + `_nightly.cfg` — **TLA-VERIFIED** (Wave-30 R-PREP 2026-05-16) — `InvSignupIdempotentByEmail` + `InvSignupTokenSingleUse` + `InvAuditEmitAtomic` + `InvAuditTenantIsAllocated` + `InvSotCoherent`; liveness `InvInFlightDrains` under WF |

**Cross-references**:
- `apps/server/src/routes/signup.rs` — wave-29 stream-1 commit `b3c359f`, `insert_or_existing` (line 585-610) canonical SoT.
- `apps/server/tests/signup_pilot.rs::duplicate_email_returns_original_tenant_id` — wire-level integration test for the idempotency claim.
- `specs/tla/signup_resignup.tla` (DEBT-014 FT-9) — disjoint sibling covering the S-19 production-tenant onboarding path.
- `specs/tla/audit_emit_atomic.tla` (DEBT-005 batch 2) — inherits INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER pattern; THIS spec proves the signup-route-specific binding (`reserved` vs `duplicate` exit_status pairing).
- `specs/_audits/2026-05-16-wave29-closure.md §6.2` — DRAFT candidate text + deferral rationale.
- `specs/_audits/2026-05-16-inv-signup-token-tla.md` — Wave-30 R-PREP dispatch audit (TLC verification ledger).

**Aliases históricos:** nenhum. Declared inline in `signup.rs` `insert_or_existing` doc comment + wave-29 stream-10 closure §6.2 candidate registration, promoted to registry in Wave-30 stream-4 R-PREP (2026-05-16).

---

## 4. TLA+ coverage matrix

CRITICAL invariantes **DEVEM** ter TLA+ spec + model check verde no CI (CTRL-FORMAL-001 em `security_model.md §6.9`).

### 4.1 Specs verdes (CI green)

| Invariante | TLA+ spec | Status |
|---|---|---|
| INV-TENANT-ISOLATION | `specs/tla/tenant_isolation.tla` + `.cfg` | ✅ GREEN (Lote 5.13) — spec com 5 camadas de defesa + adversarial path-guess action |
| INV-CAS-INTEGRITY | `specs/tla/cas_integrity.tla` + `.cfg` | ✅ GREEN (Lote 5.13) — modelo write-path reject + bit rot adversarial |
| INV-CAS-IDEMPOTENCY | Coberto por `cas_integrity.tla` via Hash determinístico | ✅ GREEN propriedade algorítmica verificada |
| INV-CAS-IMMUTABILITY | Coberto por `cas_integrity.tla` (InvCASImmutability) | ✅ GREEN |
| INV-AC-TENANT-SCOPED | Deriva de INV-TENANT-ISOLATION | ✅ GREEN via TLA+ de isolation |
| INV-GC-001 | `specs/tla/gc_correctness.tla` + `.cfg` | ✅ GREEN (Lote 5.13) — mark+sweep+grace+mark_started_at-aware |
| INV-GC-004 | Coberto por `gc_correctness.tla` (InvGCReRefProtected) | ✅ GREEN |
| INV-AUDIT-APPEND-ONLY | `specs/tla/audit_immutability.tla` + D1 schema + daily verify | ✅ GREEN (Lote 6.2) |
| INV-DIGEST-VERIFICATION | Coberto por `cas_integrity.tla` (InvPoisoningRejected) | ✅ GREEN |
| INV-AUTH-REVOCATION-IDEMPOTENT | `specs/tla/auth_revocation.tla` + `.cfg` (PR) + `_nightly.cfg` | ✅ GREEN (R-PREP 2026-05-15 audit `specs/_audits/2026-05-15-tla-coverage-audit.md`) — TLC: 6 785 distinct states, ~2 s local; `InvRevocationIdempotent`. |
| INV-AUTH-REVOCATION-SLO-60S | Coberto por `auth_revocation.tla` (`RevokedEventuallyConverges` temporal property, WF on Deliver) | ✅ GREEN (R-PREP 2026-05-15) — topological convergence proved; 60s wall-clock budget enforced separately by chaos tests + SLO alerts. |
| INV-AUTH-MASS-REVOKE-ATOMIC | Coberto por `auth_revocation.tla` (`InvMassRevokeAtomicOutbox` + atomic `MassRevoke` action) | ✅ GREEN (R-PREP 2026-05-15) — single Neon UPDATE flips all unrevoked PATs of tenant in one step. |
| INV-AUTH-PROPAGATION-AT-LEAST-ONCE | Coberto por `auth_revocation.tla` (`InvRegionMonotonic` + WF Deliver) | ✅ GREEN (R-PREP 2026-05-15). |
| INV-ROLLOUT-COSIGN-GATE | `specs/tla/rollout_cosign_gate.tla` + `.cfg` (PR) + `_nightly.cfg` | ✅ GREEN (DEBT-005 batch 2 — 2026-05-15) — `InvActiveIsSigned` + `InvRejectionAudited` + `InvActivationAudited`. Closes runtime side of INV-SUPPLY-SIGNED-DEPLOY too via `InvSupplySignedDeploy`. |
| INV-BACKUP-RESTORE-EPHEMERAL | `specs/tla/backup_restore_ephemeral.tla` + `.cfg` (PR) + `_nightly.cfg` | ✅ GREEN (DEBT-005 batch 2 — 2026-05-15) — `InvRunningIsEphemeral` + `InvNoProdRestore` + `InvRejectedHasNoNamespace` + liveness `EphemeralEventuallyTornDown` under WF. |
| INV-GC-SWEEP-AUDIT-FAIL-CLOSED | `specs/tla/gc_sweep_audit_fail_closed.tla` + `.cfg` (PR) + `_nightly.cfg` | ✅ GREEN (DEBT-005 batch 2 — 2026-05-15) — `InvSweepAtomicPairing` + `InvSweepOutcomeConsistent` + `InvNoOrphanSweepAudit`. |
| INV-GC-RECONCILE-AUDIT-FAIL-CLOSED | `specs/tla/gc_sweep_audit_fail_closed.tla` (twin reconcile path) | ✅ GREEN (DEBT-005 batch 2 — 2026-05-15) — `InvReconcileAtomicPairing` + `InvReconcileOutcomeConsistent` + `InvNoOrphanReconcileAudit`. |
| INV-AC-EVICT-REGION-PINNED | `specs/tla/ac_eviction_isolation.tla` + `.cfg` (PR) + `_nightly.cfg` | ✅ GREEN (DEBT-005 batch 2 — 2026-05-15) — `InvDeletesRegionScoped` proves every logged delete uses the row's region; adversarial cross-region action explicitly enumerated and unreachable. |
| INV-AC-EVICT-TENANT-SCOPED | `specs/tla/ac_eviction_isolation.tla` (twin tenant scoping) | ✅ GREEN (DEBT-005 batch 2 — 2026-05-15) — `InvDeletesTenantScoped`. |
| INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER | `specs/tla/audit_emit_atomic.tla` + `.cfg` (PR) + `_nightly.cfg` | ✅ GREEN (DEBT-005 batch 2 — 2026-05-15) — strengthens prior partial coverage via `audit_immutability.tla` to a direct batch-pairing biconditional `InvAtomicPairing` plus `InvNoOrphanOutbox` + `InvNoOrphanDomain`. |
| INV-OBS-NO-PII | `specs/tla/obs_no_pii.tla` + `.cfg` (PR) + `_nightly.cfg` | ✅ GREEN (DEBT-005 batch 3 — 2026-05-15) — `InvNoPiiInSink` + `InvSinkEqAdmitted` + `InvPendingNotInSink`. Adversarial `AttemptRejectedReachSink` (guard FALSE) exhibits unreachability. |
| INV-OFFBOARDING-AUDIT-COMPLETE | `specs/tla/offboarding_audit_complete.tla` + `.cfg` (PR) + `_nightly.cfg` | ✅ GREEN (DEBT-005 batch 3 — 2026-05-15) — strengthens prior "audit_immutability inheritance" to direct state-machine pairing: `InvAuditPairsTransition` + `InvAuditRowsAreValidTransitions`. |
| INV-OFFBOARDING-GRACE-RESPECTED | `specs/tla/offboarding_audit_complete.tla` (twin grace path) | ✅ GREEN (DEBT-005 batch 3 — 2026-05-15) — `InvGraceRespected`; `AttemptEarlyAdvance` rejected by grace gate. |
| INV-BILLING-CHAIN-INTEGRITY | `specs/tla/billing_chain_integrity.tla` + `.cfg` (PR) + `_nightly.cfg` | ✅ GREEN (DEBT-005 batch 3 — 2026-05-15) — Bitcoin block-header pattern; `InvChainExtendOnly` + `InvChainHeadVerifiable` + `InvNoFork`. `Tamper` + `AttemptForkChain` guards FALSE. |
| INV-GC-GRACE-BOUNDARY-STRICT | `specs/tla/gc_grace_boundary.tla` + `.cfg` (PR) + `_nightly.cfg` | ✅ GREEN (DEBT-005 batch 3 — 2026-05-15) — `InvGraceBoundaryStrict` enforces `age > GraceWindow` (strict). `AttemptSweepAtBoundary` (guard FALSE) exhibits unreachability. |
| INV-GC-MARK-TENANT-SCOPED | `specs/tla/gc_grace_boundary.tla` (twin mark path) | ✅ GREEN (DEBT-005 batch 3 — 2026-05-15) — `InvMarkTenantScoped`; `AttemptMarkCrossTenant` guard FALSE. Owner function enumerated by TLC across `[Blobs -> Tenants]`. |
| INV-GC-SWEEP-TENANT-SCOPED | `specs/tla/gc_grace_boundary.tla` (twin sweep path) | ✅ GREEN (DEBT-005 batch 3 — 2026-05-15) — `InvSweepTenantScoped`; `AttemptSweepCrossTenant` guard FALSE. |
| INV-BACKUP-FRESH | `specs/tla/backup_fresh.tla` + `.cfg` (PR) + `_nightly.cfg` | ✅ GREEN (DEBT-005 batch 3 — 2026-05-15) — `InvLatestAcceptedFresh`; `AttemptStaleSwap` (guard FALSE) exhibits no-swap-to-stale. |
| INV-BACKUP-INTEGRITY-SAMPLE-CAP | `specs/tla/backup_fresh.tla` (twin sampler cap) | ✅ GREEN (DEBT-005 batch 3 — 2026-05-15) — `InvSampleCapPerCycle`; `AttemptSampleOverCap` guard FALSE. |
| INV-AUTH-AUDIT-PSEUDONYMIZATION | `specs/tla/auth_audit_pseudonymization.tla` + `.cfg` (PR) + `_nightly.cfg` | ✅ GREEN (DEBT-005 batch 6 FINAL — 2026-05-15) — `InvAuthAuditPseudonymization` + `InvAuditChainErasureSafe`; adversarial `AttemptDsrAuditDelete` + `AttemptRawSubjectInChain` guards FALSE. Parent: `audit_immutability.tla`. |
| INV-AUTH-CONSTANT-TIME-COLD-PAD | `specs/tla/auth_constant_time_cold_pad.tla` + `.cfg` (PR) + `_nightly.cfg` | ✅ GREEN (DEBT-005 batch 6 FINAL — 2026-05-15) — `InvAuthConstantTimeColdPad` + `InvColdPathResultUniform`; every cold-path branch (parse_fail, sig_mismatch, token_id_absent, hash_mismatch) records `pad_invoked = TRUE`. Sibling: `auth_pat_hybrid.tla` (INV-AUTH-PAT-VERIFY-CONSTANT-TIME). |
| INV-CONSENT-PROOF-VERIFIABLE | `specs/tla/consent_proof_verifiable.tla` + `.cfg` (PR) + `_nightly.cfg` | ✅ GREEN (DEBT-005 batch 6 FINAL — 2026-05-15) — `InvConsentProofVerifiable` + `InvVerifierSound`; standalone hash/HMAC verifier soundness. Co-proof: `dsr_erasure_atomicity.tla` (InvConsentSymmetry, S-11 WI-S11-008). |
| INV-GC-REACHABLE-SET-COMPLETE | `specs/tla/gc_reachable_set_complete.tla` + `.cfg` (PR) + `_nightly.cfg` | ✅ GREEN (DEBT-005 batch 6 FINAL — 2026-05-15) — `InvGcReachableSetComplete` + `InvReachableSetMonotone`; 3-pass marker (blob_meta + ac_meta + manifest_chunks) is superset-safe. Parent: `gc_correctness.tla` (InvGCReachableNeverDeleted). |
| INV-SUB-PROCESSOR-AUDIT-FAIL-CLOSED | `specs/tla/sub_processor_audit_fail_closed.tla` + `.cfg` (PR) + `_nightly.cfg` | ✅ GREEN (DEBT-005 batch 6 FINAL — 2026-05-15) — `InvSubProcessorAuditFailClosed` + `InvAuditBeforeMutate`; every (publish / record_change / file_objection) op emits audit BEFORE state mutation; audit failure leaves state UNCHANGED. Parent: `audit_immutability.tla`; sibling: `audit_emit_atomic.tla`. |
| INV-PAT-REVOKE-PROPAGATION | `specs/tla/auth_pat_revoke.tla` + `.cfg` (PR) + `_nightly.cfg` | ✅ GREEN (Wave-24 R-PREP — 2026-05-16; promoted from §3.28 PLANNED via Wave-23 invariant-draft sweep) — `InvRevokedTokenNeverValidates` proves "no edge cache lookahead" (admit_200 + sot_at_attempt=1 trace unreachable) + `InvRevokeAuditAtomic` (audit row durable before 204 response) + `InvRevokeIsIdempotent` (≤ 1 `auth.token.revoked` audit row per PAT regardless of retry count) + `InvRegionImpliesSoTRevoked` (SoT precedes region cache) + temporal `InvRevokeAtLeastOncePropagation` under WF on PropagateToRegion. Sibling of `auth_pat_hybrid.tla` (mint-path) and disjoint from `auth_revocation.tla` (queue-side). |
| INV-SIGNUP-TOKEN-IDEMPOTENT | `specs/tla/signup_token_idempotent.tla` + `.cfg` (PR) + `_nightly.cfg` | ✅ GREEN (Wave-30 R-PREP — 2026-05-16; promoted from §3.29 DRAFT via Wave-29 stream-10 closure §6.2 deferral) — `InvSignupIdempotentByEmail` proves ≤ 1 `reserved` audit row per email (per-email exactly-once allocation) + `InvSignupTokenSingleUse` proves ≤ 1 `reserved` audit row per token (token single-use claim) + `InvAuditEmitAtomic` proves audit-row-per-request bijection (`Len(audit_log) = request_count`) + `InvAuditTenantIsAllocated` proves no synthesised tenant_id in audit (defense-in-depth) + `InvSotCoherent` proves token_tenant ↔ email_tenant slice agreement + liveness `InvInFlightDrains` under WF on RouteCompletion. PR-lane TLC: 1 017 distinct states, depth 9, 5 s wall-clock. Sibling of `signup_resignup.tla` (DEBT-014 FT-9, production-onboarding path) and disjoint from it (pilot pre-Stripe path); inherits `audit_emit_atomic.tla` (DEBT-005 batch 2) atomic-pairing pattern. |

### 4.2 Specs PLANNED (Lote 9.4 obligation matrix — pré-condição S-10/S-11/S-13/S-14/S-19 implementation)

CRITICAL/HIGH invariants adicionados em §3.12 + §3.13 que requerem TLA+ pelo enforcement table §2:

| Invariante | TLA+ spec planejado | Status | Sprint owner | Justificativa TLA+ |
|---|---|---|---|---|
| INV-BILLING-RECONCILE-3-LAYER | `specs/tla/billing_atomicity.tla` | 📋 PLANNED | S-10 | atomicity event→counter→invoice; concurrent reconcile races |
| INV-BILLING-REPLAYABLE-FROM-EVENTS | Coberto por `billing_atomicity.tla` (InvReplayDeterministic) | 📋 PLANNED | S-10 | replay determinism com state space rico |
| INV-DATA-ERASURE-COMPLETE | `specs/tla/dsr_erasure_atomicity.tla` | ✅ GREEN (Lote 10.11.0-bis-bis V2 — 2026-05-15-tla-coverage-audit §3 confirms; first CI run TLC verde sustained; 10 actions + 5 state invariants + 3 temporal properties via TLC v1.8.0 SHA-pinned ADR-0042 §A1) | S-11 | cross-backend atomic OR compensating-rollback; **12 backends canonical (8 effective + 4 pseudonymized — Lote 10.11.0-bis)** |
| INV-CONSENT-PROOF-VERIFIABLE | `specs/tla/consent_proof_verifiable.tla` (DEBT-005 batch 6 FINAL) + co-proof `dsr_erasure_atomicity.tla` (InvConsentSymmetry) | ✅ GREEN (DEBT-005 batch 6 FINAL + Lote 10.11.0-bis-bis V2 — 2026-05-15; standalone hash/HMAC verifier soundness PR-lane + nightly; dsr_erasure_atomicity InvConsentSymmetry action proven via tla-coverage-audit §3) | S-11 | consent grant/revoke ledger symmetric (Lote 9.4 H-05); 6-field proof canonical |
| INV-DATA-RESIDENCY | `specs/tla/dsr_erasure_atomicity.tla` (InvResidencyPinned + InvResidencyMonotonic) + `specs/tla/region_residency.tla` (full cross-region routing) + property test 20k | ✅ GREEN (Lote 10.11.0-bis-bis V2 — 2026-05-15-tla-coverage-audit §3; dual-spec coverage: S-11 partial + S-14 full landed early via R-prep wave) | S-11 (partial) + S-14 (full landed early) | tenant.primary_region pinning + monotonic (no cross-region migration) + custom domain routing fail-CLOSED + cross-region routing semantics |
| INV-BYOK-CRYPTO-SOVEREIGNTY | `specs/tla/byok_envelope_aad.tla` (AAD binding + cache-TTL liveness; DEBT-005 batch 1) + `specs/tla/byok_dek_race.tla` (per-region DEK cache eviction race; DEBT-014 FT-7 2026-05-16) + runbook `specs/03_architecture/tla+/runbooks/byok_kill_switch.tla` | ✅ GREEN (DEBT-014 FT-7 CRITICAL upgrade closed 2026-05-16; per-region cache race lifted from single-cache abstraction; TLC 3906 distinct states + 2 temporal branches) | S-14 | DEK cache TTL 5 min hard + KMS revocation propagation + per-region independent eviction (no cross-region cache copy) |
| INV-REGION-NO-CROSS-LEAK | `specs/tla/region_residency.tla` | ✅ GREEN (Lote 10.11.0-bis-bis V2 — 2026-05-15-tla-coverage-audit §3; region_residency.tla landed early via R-prep wave per Lote 10.14 canonical) | S-14 | tenant region pinning property test 30k + cross-region write rejection invariant |
| INV-ONBOARD-DPA-FIRST | `specs/03_architecture/tla+/runbooks/signup_atomic.tla` (DPAFirstHolds — WI-S20-007 runbook 1/4, CI-wired via DEBT-014 FT-8 2026-05-16) + `specs/tla/signup_resignup.tla` (re-signup boundary; DEBT-014 FT-9 2026-05-16) | ✅ GREEN (DEBT-014 FT-9 closed 2026-05-16; re-signup same-email + late-Stripe-webhook idempotency proven; TLC 561 distinct states + 3 temporal branches) | S-19 | DPA-first ordering; race condition impossibility; re-signup tombstone idempotency |
| INV-ONBOARD-ATOMIC-PROVISIONING | `specs/03_architecture/tla+/runbooks/signup_atomic.tla` (AtomicSignup, NoOrphanIdentity, NoOrphanBilling) + `specs/tla/signup_resignup.tla` (InvAtomicPerAttempt + InvLateWebhookNoOp) | ✅ GREEN (DEBT-014 FT-9 closed 2026-05-16) | S-19 | tenant + DPA + Stripe atomic; per-attempt atomicity across re-signup retries |
| INV-SIGNUP-RESIGNUP-IDEMPOTENT | `specs/tla/signup_resignup.tla` (InvResignupNoCrossLeak + InvAtMostOneSuccessPerEmail + InvLateWebhookNoOp) | ✅ GREEN (DEBT-014 FT-9 closed 2026-05-16; new HIGH invariant registered same day; TLC 561 distinct states) | S-19 | re-signup same email after compensated failure produces independent atomic outcome; late Stripe webhook is no-op |
| INV-MULTIPART-FINALIZE-IRREVOCABLE / INV-MULTIPART-STATE-MONOTONIC / INV-MULTIPART-CONCURRENCY-BOUNDED | `specs/tla/multipart_finalize.tla` (InvFinalizeIrrevocable + InvStateMonotonic + InvConcurrencyBounded + InvNoDoubleTerminal + InvFinalizedDigestStable) | ✅ GREEN (DEBT-014 FT-6 closed 2026-05-16; concurrent abort+finalize race + idempotent re-finalize + per-tenant semaphore cap formalised; TLC 869 distinct states / depth 6) | S-05 | session state machine live → {finalized, aborted} with mismatched-digest re-finalize rejection |
| INV-KEY-NO-SKIP / INV-KEY-OVERLAP | `specs/tla/key_lifecycle.tla` | 📋 PLANNED | S-13 | rotation state machine per asset class |
| INV-ADMIN-DUAL-APPROVAL | Coberto por `key_lifecycle.tla` (InvCallerNeqApprover) | 📋 PLANNED | S-13 | dual-approval state machine |

### 4.3 Specs sem TLA+ requirement (HIGH severity mas non-distributed)

Lote 9.5c expansion: catalogadas todas as invariantes HIGH cuja semantics não justifica TLA+ (algorithmic + non-distributed + check coberto por outras validações: property test, schema constraint, CI gate, single-table reconcile, etc.).

| Invariante | Severidade | Justificativa não-TLA+ |
|---|---|---|
| INV-AC-OUTPUTS-VALID | HIGH | Reconcile diário schema-level (FK em D1); cross-doc S-06 GC + reconcile cron; não distributed semantics |
| INV-GC-003 | HIGH | Refcount consistency; reconcile diário CTRL-GC-002; covered by property test S-06 |
| INV-DATA-MONOTONIC-TS | MEDIUM | Writer enforces via `MAX(now, prev_value)`; algorithmic não-distributed |
| INV-DATA-BILLING-RECONCILE | HIGH | Subsumed por INV-BILLING-RECONCILE-3-LAYER (S-10 PLANNED `billing_atomicity.tla`) |
| INV-AUDIT-RETENTION | HIGH | Object Lock hardware enforcement; quarterly audit (não invariant runtime) |
| INV-CONF-AT-REST | HIGH | R2 SSE + D1/Neon SSE config; quarterly config audit (EVT-028); não runtime invariant |
| INV-CONF-IN-FLIGHT | HIGH | TLS 1.3 enforced em CF Edge + Workers; SSL Labs A+ check (EVT-037); não runtime semantics |
| INV-AVAIL-ISOLATION | HIGH | Coberto indiretamente por TLA+ tenant_isolation (5-layer defense) + property test bulkhead PAT-BULKHEAD-001 |
| INV-BILLING-NO-LOSS | HIGH | Subsumed por INV-BILLING-RECONCILE-3-LAYER + planned `billing_atomicity.tla` (S-10 PLANNED) |
| INV-BILLING-NO-DUP | HIGH | Idempotency-Key + (tenant_id, request_id) UNIQUE; coberto por `billing_atomicity.tla` planned |
| INV-SUPPLY-SIGNED-DEPLOY | HIGH | CI gate + Cosign verify pre-rollout; build-time check, não runtime |
| INV-SUPPLY-SBOM-PRESENT | HIGH | CI gate; build-time |
| INV-SUPPLY-PROVENANCE-IN-REKOR | HIGH | CI gate Rekor inclusion proof; build-time |
| INV-SUPPLY-NO-YANKED | HIGH | cargo-deny CI gate; build-time |
| INV-SUPPLY-LICENSE-ALLOWLIST | HIGH | cargo-deny CI gate; build-time |
| INV-QUOTA-ENFORCEMENT | HIGH | DO atomic counter per-tenant + write path check (CTRL-QUOTA-001); covered by property test S-08 |
| INV-DATA-RESIDENCY | CRITICAL (Lote 10.11.0-bis Schrems II) | **PARTIAL coverage S-11** via `dsr_erasure_atomicity.tla` (InvResidencyPinned + temporal InvResidencyMonotonic — Lote 10.11.0-bis-prime cycle 3) + **FULL coverage NOW LANDED** via `region_residency.tla` (cross-region routing actions — Lote 10.11.0-bis-bis V2 2026-05-15: spec landed early via R-prep wave; sprint owner S-14 preserved for provenance); subsumido por INV-REGION-NO-CROSS-LEAK (✅ GREEN per §4.2 status cell). |
| INV-DEDUP-CONSISTENCY | HIGH | Algorithmic property; UNIQUE index enforces; covered by property test S-07 |
| INV-RATE-LIMIT-PROPORTIONALITY | HIGH | DO atomic counter; covered by property test 10k iter |
| INV-OBS-CARDINALITY-BUDGET | HIGH | Static budget validator CI (`cardinality_check.py`); non-distributed |
| INV-OBS-AUDIT-CHAIN-INTEGRITY | HIGH | Hash chain; coberto por `audit_immutability.tla` indiretamente; daily verifier job |
| INV-ADMIN-MFA-FRESHNESS | HIGH | Middleware timestamp check; non-distributed |
| INV-ERASURE-ATTESTATION-SIGNED | HIGH | Signature verification; algorithmic; per-region Ed25519 key |
| INV-CONSENT-PROOF-VERIFIABLE | CRITICAL (Lote 10.11.0-bis com TLA+ symmetry) | Coberto por `dsr_erasure_atomicity.tla` ✅ GREEN (S-11 WI-S11-008; Lote 10.11.0-bis-bis V2 2026-05-15: first CI run TLC verde sustained per 2026-05-15-tla-coverage-audit §3); via InvConsentSymmetry action. |
| INV-CAS-SIDE-CHANNEL-INDISTINGUISHABLE | HIGH | Statistical algorithmic property (Mann-Whitney U); criterion benchmark + adversarial test 10k samples; não state-machine distributed |
| INV-KEY-AUDIT | HIGH | Coberto por `audit_immutability.tla` |
| INV-KEY-OVERLAP | HIGH | Per-asset table canonical em `key_management.md §3.2.1` + ADR-0018; covered by `key_lifecycle.tla` PLANNED (S-13 §4.2 entry) |
| INV-SUB-PROCESSOR-AUDIT-FAIL-CLOSED | CRITICAL | Coberto por `audit_immutability.tla` ✅ GREEN inheritance + ADR-S11-002 split-tier; `FailingSubProcessorAuditSink` regression tests verify state UNCHANGED on audit failure (S-11 WI-S11-005) |
| INV-SUB-PROCESSOR-BROADCAST-IDEMPOTENT | HIGH | UNIQUE constraint `(broadcast_id, tenant_id, recipient_email_hash, notification_type)` em D1 + property test 10k iter; algorithmic schema-level |
| INV-SUB-PROCESSOR-BROADCAST-ALL-PLANS | HIGH | Policy invariant; ADR-S11-008 v2 + cron worker; regression test enforces no tier-gating |
| INV-SUB-PROCESSOR-DKIM-TENANT-SCOPED | HIGH | HKDF-SHA256 algorithmic statistical property; per-tenant key derivation; cross-tenant 10k random pairs → 0 collisions property test |
| INV-SUB-PROCESSOR-OBJECTION-STATE-MACHINE | HIGH | Algorithmic state machine; 5 states + valid transitions enforced at trait surface; covered by 10k iter property test |

### 4.4 CI obligation gate (Lote 9.4)

**Regra**: pre-S-20 GA gate, todo INV CRITICAL com `Status: PLANNED` deve transitar para `GREEN`. CI script `scripts/check_tla_obligations.py` (criar pós-Lote 9.4) lê esta matrix e falha PR se invariante CRITICAL declarado num sprint contract não tem TLA+ status `GREEN | PLANNED com sprint_owner`.

---

## 5. Aliases históricos (deprecated names)

Por 6 meses (até 2026-10-24), estes aliases continuam referenciáveis mas disparam warning no `validate_references.py`. Após 2026-10-24, são removidos do índice e qualquer referência falha CI.

| Alias histórico | ID canônico |
|---|---|
| `INV-TenantIsolation` | INV-TENANT-ISOLATION |
| `INV-AuditLogImmutability` | INV-AUDIT-APPEND-ONLY |
| `INV-CASIdempotency` | INV-CAS-IDEMPOTENCY |
| `INV-QuotaEnforcement` | INV-QUOTA-ENFORCEMENT |
| `INV-DigestVerification` | INV-DIGEST-VERIFICATION |
| `INV-DataResidency` | INV-DATA-RESIDENCY |
| `INV-DATA-TENANT-ISOLATION` (de `data_model.md §7`) | INV-TENANT-ISOLATION |
| `INV-DATA-AUDIT-CHAIN` | INV-AUDIT-APPEND-ONLY |
| `INV-DATA-BLOB-HASH` | INV-CAS-INTEGRITY |
| `INV-DATA-REFCOUNT` | INV-GC-003 |
| `INV-DATA-BLOB-NO-ZOMBIE` | INV-GC-002 |
| `INV-DATA-AC-REFS-EXIST` | INV-AC-OUTPUTS-VALID |
| `INV-DATA-TENANT-ISOLATION` | INV-TENANT-ISOLATION |
| `INV-CAS-DIGEST-INTEGRITY` (drift surfaced em S-09 dashboards; Lote 10.9bis wave 17 rename per R4-P1-1 + R5-P1-S1) | INV-CAS-INTEGRITY |
| `INV-AUDIT-CHAIN` (shortened form in S-03 `_spec_contract.md` v1.3.2 + PRR-S03 R-S03-007; surfaced Wave-23 sweep) | INV-AUDIT-APPEND-ONLY |
| `INV-AUDIT-EMIT-ATOMIC` (shortened form in Wave-20+ crates: `corelink-statuspage-real`, `corelink-slack-real`, `corelink-region`, `corelink-rotation-adapters`, `corelink-drata-sync`; surfaced Wave-23 sweep) | INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER |
| `INV-AUTH-WEBAUTHN` (family shorthand in `crates/corelink-webauthn/README.md`; refers to 5-INV WebAuthn family §3.14 — UV-REQUIRED-ADMIN / ATTESTATION-VERIFIED / SIGN-COUNT-MONOTONIC / ORIGIN-EXACT / RP-ID-CANONICAL; surfaced Wave-23 sweep) | §3.14 AUTH-WEBAUTHN family (no single canonical; alias is family-collective shorthand) |
| `INV-BLAKE3-256-LOWER-HEX-64` (S-06 R4 review recommendation `specs/04_sprints/S06/_review_R4_opus_part1.md §P3-002-1`; never canonicalized as separate INV — digest canonical-form constraint subsumed by `INV-CAS-INTEGRITY` write-time hash check + `INV-CAS-IDEMPOTENCY` BLAKE3 deterministic enforcement; surfaced Wave-23 sweep) | INV-CAS-INTEGRITY (digest canonical form subsumed) |

---

**Fim de INVARIANT-REGISTRY.** Adição ou rename de invariante requer: (a) entry aqui, (b) update em `data_model.md §7` + canonical source relevante, (c) se CRITICAL, spec `.tla`, (d) ADR minor no framework se mudar severity ou enforcement.
