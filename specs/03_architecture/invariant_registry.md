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
| **INV-AUDIT-APPEND-ONLY** (alias histórico: `INV-AuditLogImmutability`) | Audit log é append-only, cadeia de hash íntegra | CRITICAL | UPDATE/DELETE em audit_log rejeitado; hash chain verifica continuamente | D1 CHECK constraint + R2 Object Lock + daily chain verify (PAT-AUDIT-VERIFY-001) |
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

---

## 4. TLA+ coverage matrix

CRITICAL invariantes **DEVEM** ter TLA+ spec + model check verde no CI (CTRL-FORMAL-001 em `security_model.md §6.9`).

| Invariante | TLA+ spec | Status |
|---|---|---|
| INV-TENANT-ISOLATION | `specs/tla/tenant_isolation.tla` | ⚠️ A criar (blocker para GA) |
| INV-CAS-INTEGRITY | `specs/tla/cas_integrity.tla` | ⚠️ A criar (property test cobre interim) |
| INV-CAS-IDEMPOTENCY | N/A (propriedade algorítmica do BLAKE3/SHA-256) | ✅ coberto por algoritmo + test |
| INV-CAS-IMMUTABILITY | Simples — cobertura por test | ✅ |
| INV-AC-TENANT-SCOPED | Deriva de INV-TENANT-ISOLATION | ✅ via TLA+ de isolation |
| INV-GC-001 | `specs/tla/gc_correctness.tla` | ⚠️ A criar (blocker; endereça race S-12) |
| INV-GC-004 | Coberto por gc_correctness.tla | ⚠️ |
| INV-AUDIT-APPEND-ONLY | Cobertura D1 schema + daily verify | ✅ |

---

## 5. Aliases históricos (deprecated names)

Por 6 meses (até 2026-10-24), estes aliases continuam referenciáveis mas disparam warning no `validate_references.py`. Após 2026-10-24, são removidos do índice e qualquer referência falha CI.

| Alias histórico | ID canônico |
|---|---|
| `INV-TenantIsolation` | INV-TENANT-ISOLATION |
| `INV-AuditLogImmutability` | INV-AUDIT-APPEND-ONLY |
| `INV-CASIdempotency` | INV-CAS-IDEMPOTENCY |
| `INV-DATA-TENANT-ISOLATION` (de `data_model.md §7`) | INV-TENANT-ISOLATION |
| `INV-DATA-AUDIT-CHAIN` | INV-AUDIT-APPEND-ONLY |
| `INV-DATA-BLOB-HASH` | INV-CAS-INTEGRITY |
| `INV-DATA-REFCOUNT` | INV-GC-003 |
| `INV-DATA-BLOB-NO-ZOMBIE` | INV-GC-002 |
| `INV-DATA-AC-REFS-EXIST` | INV-AC-OUTPUTS-VALID |
| `INV-DATA-TENANT-ISOLATION` | INV-TENANT-ISOLATION |

---

**Fim de INVARIANT-REGISTRY.** Adição ou rename de invariante requer: (a) entry aqui, (b) update em `data_model.md §7` + canonical source relevante, (c) se CRITICAL, spec `.tla`, (d) ADR minor no framework se mudar severity ou enforcement.
