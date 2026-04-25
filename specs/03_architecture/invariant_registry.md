---
id: "INVARIANT-REGISTRY"
type: "invariant"
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
tags: ["architecture", "invariants", "registry", "tla"]
---

# Invariant Registry — Catálogo Canônico de INV-XXX

> **doc_status:** DRAFT
> **Versão:** 0.1.0
> **Última atualização:** 2026-04-24
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

- `<DOMAIN>` ∈ `{TENANT, CAS, AC, GC, DATA, AUDIT, CONF, AVAIL, BILLING, SUPPLY}` (ver §3).
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
| **INV-GC-004** | Mark-phase-aware re-ref safe | CRITICAL | Blob re-referenciado via AC update após `mark_started_at` não é deletado mesmo que Mark não o tenha visto | Implementação: sweep checa `ac.created_at > mark_started_at` antes de delete | Coberto por `gc_correctness.tla` |

### 3.5 Data Integrity (domain DATA)

| ID | Nome | Severidade | Descrição | Enforcement |
|---|---|---|---|---|
| **INV-DATA-MONOTONIC-TS** | Timestamps monotonic | MEDIUM | `last_accessed_at`, `updated_at` nunca regridem | Writer enforces via `MAX(now, prev_value)` |
| **INV-DATA-BILLING-RECONCILE** | Usage counter ≈ Σ(usage events) | HIGH | Drift ≤ 0.1% entre `usage_counter` agregado e eventos emitidos | Reconcile diário (PAT-RECONCILE-001) |
| **INV-DATA-ERASURE-COMPLETE** | DSR erasure é efetiva | HIGH | Após DSR-erasure resolved, nenhum backend retorna dado do subject (exceto legal hold) | E2E test (EVT-042) |

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

### 3.9 Billing (domain BILLING)

| ID | Nome | Severidade | Descrição | Enforcement |
|---|---|---|---|---|
| **INV-BILLING-NO-LOSS** | Todo billable event contabilizado | HIGH | Nenhum evento billable é perdido; at-least-once delivery + dedup | Queue com durável + reconciliation (PAT-RECONCILE-001) |
| **INV-BILLING-NO-DUP** | Nenhum evento billable contabilizado 2x | HIGH | Idempotency key + dedup window 24h | PAT-IDEMPOTENCY-001 |

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
| **INV-DATA-RESIDENCY** (alias histórico: `INV-DataResidency`) | Dado de tenant fica em região pinned | HIGH | R2 bucket com locationHint; D1 primary na região escolhida; DO stickiness | CTRL-PRIV-031; quarterly config audit |

### 3.12 Sprint-driven invariants (Lote 9.1 SOTA elevation, expandido em Lote 9.4)

Invariantes introduzidas via SOTA elevation dos sprint contracts S-07..S-19 (Lotes 9.1 + 9.4). Todas adicionadas com sprint_origin column para traceability.

| ID | Nome | Severidade | Descrição | Enforcement | Sprint origin |
|---|---|---|---|---|---|
| **INV-DEDUP-CONSISTENCY** | Dedup chunk_body 1:1 dentro do tenant | HIGH | `(tenant_id, chunk_digest) → chunk_body` é 1:1; nenhum dup chunk_body para mesmo digest dentro de tenant | UNIQUE index D1 + property test (PAT-DEDUP-CHECK-001) | S-07 |
| **INV-RATE-LIMIT-PROPORTIONALITY** | Rate limit refill_rate × window consistente com tenant plan | HIGH | `refill_rate × window` sempre consistente com tenant plan; mudança de plan reflete em ≤ 5 min via DO config sync | DO atomic update + sync test (CTRL-RATE-001) | S-08 |
| **INV-OBS-CARDINALITY-BUDGET** | Cardinality de métrica respeita budget | HIGH | Nenhuma métrica excede 20k séries únicas; total ≤ 100k. Reference: `observability_model.md §11.2` | Cardinality validator CI + Grafana Mimir tenant limit | S-09 |
| **INV-OBS-AUDIT-CHAIN-INTEGRITY** | Audit events R2 hash chain unbroken | HIGH | Hash chain de audit events em R2 é unbroken; daily verifier alerta em break | Background daily job + INV-AUDIT-APPEND-ONLY | S-09 |
| **INV-BILLING-RECONCILE-3-LAYER** | 3-layer reconciliation diária events ↔ counters ↔ Stripe | HIGH | Drift > 0.1% em qualquer layer = SEV-2; bloqueia close-of-month até resolution. Strengthens INV-DATA-BILLING-RECONCILE | Cron daily + 3-layer compare + alert escalation | S-10 |
| **INV-BILLING-REPLAYABLE-FROM-EVENTS** | Invoice reconstrutível byte-a-byte de events | HIGH | Qualquer invoice deve poder ser reconstruída byte-a-byte a partir de events R2; replay endpoint role-protected | POST /v1/billing/replay endpoint + monthly CI test + audit-grade | S-10 |
| **INV-CONSENT-PROOF-VERIFIABLE** | Consent records têm notice_text_hash verifiable post-facto | HIGH | SHA-256 do notice + HMAC; tampering detected via signature | Verify endpoint + GDPR Art. 7 alignment | S-11 |
| **INV-SUPPLY-PROVENANCE-IN-REKOR** | Provenance attestation publicada em Rekor | HIGH | Toda provenance attestation deve estar em Rekor transparency log; release sem inclusion proof = blocked | Deploy webhook checks Rekor inclusion antes rollout | S-12 |
| **INV-SUPPLY-NO-YANKED** | Zero yanked deps em Cargo.lock main | HIGH | Author retired pode ser segurança ou bug; usar = risco | cargo-deny policy + CI gate | S-12 |
| **INV-SUPPLY-LICENSE-ALLOWLIST** | Zero deps fora da license allowlist | HIGH | Allowlist: MIT/Apache-2.0/BSD/ISC/MPL-2.0/Unicode-DFS-2016. GPL/AGPL/SSPL banned | cargo-deny enforces; quarterly Legal review | S-12 |
| **INV-ADMIN-DUAL-APPROVAL** | Destructive admin op tem 2 distinct signatures | HIGH | Caller + approver enforced D1 hard-check; 0 bypasses em property test 10k attempts | D1 hard-check + property test + audit emission | S-13 |
| **INV-ADMIN-MFA-FRESHNESS** | Admin op exige MFA timestamp ≤ 30 min | HIGH | Stale MFA = 401 + force re-MFA (CTRL-AUTH-010) | Middleware check + signed timestamp + clock-skew tolerance ≤ 60s | S-13 |
| **INV-BYOK-CRYPTO-SOVEREIGNTY** | Customer revoga CMK → cache inacessível ≤ 5 min | CRITICAL | Nenhum bypass via cached unwrapped DEK > 5 min; customer real control | DEK cache TTL 5 min hard + KMS access check 60s + INV propagation | S-14 |
| **INV-REGION-NO-CROSS-LEAK** | Blob/AC/billing tagged com region; cross-region read = 403 | CRITICAL | Schrems II + LGPD Art. 33 baseline; 30k property test 0 leaks | Insert checks + property test + custom domain routing | S-14 |
| **INV-ERASURE-ATTESTATION-SIGNED** | Erasure de BYOK tenant produz attestation Ed25519-signed verifiable | HIGH | Customer + auditor exigem proof; NIST SP 800-88 Rev.1 compliant | Per-erasure attestation + 7y retention + verify endpoint | S-14 |
| **INV-ONBOARD-DPA-FIRST** | Subscription activation requires DPA signed primeiro | HIGH | Race condition prevented; nenhum customer billed sem DPA | D1 lock + transactional check + property test 10k concurrent | S-19 |
| **INV-ONBOARD-ATOMIC-PROVISIONING** | Tenant provisioning atomic | HIGH | Tenant + DPA + Stripe customer ID em single tx; failure rollback all | D1 transaction + chaos test Stripe outage | S-19 |

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

### 4.2 Specs PLANNED (Lote 9.4 obligation matrix — pré-condição S-10/S-11/S-13/S-14/S-19 implementation)

CRITICAL/HIGH invariants adicionados em §3.12 + §3.13 que requerem TLA+ pelo enforcement table §2:

| Invariante | TLA+ spec planejado | Status | Sprint owner | Justificativa TLA+ |
|---|---|---|---|---|
| INV-BILLING-RECONCILE-3-LAYER | `specs/tla/billing_atomicity.tla` | 📋 PLANNED | S-10 | atomicity event→counter→invoice; concurrent reconcile races |
| INV-BILLING-REPLAYABLE-FROM-EVENTS | Coberto por `billing_atomicity.tla` (InvReplayDeterministic) | 📋 PLANNED | S-10 | replay determinism com state space rico |
| INV-DATA-ERASURE-COMPLETE | `specs/tla/dsr_erasure_atomicity.tla` | 📋 PLANNED | S-11 | cross-backend atomic OR compensating-rollback; 7 backends |
| INV-CONSENT-PROOF-VERIFIABLE | Coberto por `dsr_erasure_atomicity.tla` (InvConsentSymmetry) | 📋 PLANNED | S-11 | consent grant/revoke symmetry |
| INV-BYOK-CRYPTO-SOVEREIGNTY | `specs/tla/byok_sovereignty.tla` | 📋 PLANNED | S-14 | DEK cache TTL 5 min hard + KMS revocation propagation |
| INV-REGION-NO-CROSS-LEAK | `specs/tla/region_residency.tla` | 📋 PLANNED | S-14 | tenant region pinning property test 30k |
| INV-ONBOARD-DPA-FIRST | `specs/tla/onboarding_atomicity.tla` | 📋 PLANNED | S-19 | DPA-first ordering; race condition impossibility |
| INV-ONBOARD-ATOMIC-PROVISIONING | Coberto por `onboarding_atomicity.tla` (InvAtomicTx) | 📋 PLANNED | S-19 | tenant + DPA + Stripe atomic |
| INV-KEY-NO-SKIP / INV-KEY-OVERLAP | `specs/tla/key_lifecycle.tla` | 📋 PLANNED | S-13 | rotation state machine per asset class |
| INV-ADMIN-DUAL-APPROVAL | Coberto por `key_lifecycle.tla` (InvCallerNeqApprover) | 📋 PLANNED | S-13 | dual-approval state machine |

### 4.3 Specs sem TLA+ requirement (HIGH severity mas non-distributed)

| Invariante | Justificativa não-TLA+ |
|---|---|
| INV-DEDUP-CONSISTENCY | Algorithmic property; UNIQUE index enforces; covered by property test |
| INV-RATE-LIMIT-PROPORTIONALITY | DO atomic counter; covered by property test 10k iter |
| INV-OBS-CARDINALITY-BUDGET | Static budget validator CI; non-distributed |
| INV-OBS-AUDIT-CHAIN-INTEGRITY | Hash chain; coberto por `audit_immutability.tla` indiretamente |
| INV-SUPPLY-* | Build-time checks; non-runtime distributed semantics |
| INV-ADMIN-MFA-FRESHNESS | Middleware timestamp check; non-distributed |
| INV-ERASURE-ATTESTATION-SIGNED | Signature verification; algorithmic |
| INV-KEY-AUDIT | Coberto por `audit_immutability.tla` |

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

---

**Fim de INVARIANT-REGISTRY.** Adição ou rename de invariante requer: (a) entry aqui, (b) update em `data_model.md §7` + canonical source relevante, (c) se CRITICAL, spec `.tla`, (d) ADR minor no framework se mudar severity ou enforcement.
