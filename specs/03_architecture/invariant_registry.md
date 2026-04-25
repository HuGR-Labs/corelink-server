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
| **INV-CAS-SIDE-CHANNEL-INDISTINGUISHABLE** | Timing distribution 404 vs 403 statistically indistinguishable | HIGH | Constant-time middleware + jitter; Mann-Whitney p > 0.05 em 10k samples; criterion benchmark p99 diff < 5ms | Adversarial test S-02 + criterion CI | S-02 |

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
| **INV-AUTH-JWT-VALIDATE-RS256-ONLY** | JWT validation rejects all non-RS256 alg | CRITICAL | `jsonwebtoken` 9.x `Validation::new(Algorithm::RS256)` enforce; rejeita alg=none + key confusion HS-with-public-pem | Property test 10k iter + adversarial regression em `corelink-clerk/tests/adversarial.rs` (CVE-2015-9235 + CVE-2018-0114) | (planned `auth_jwt_validation.tla`; PLANNED) |
| **INV-AUTH-CLOCK-SKEW-BOUND** | Clock skew leeway ≤ 60s em todos JWT validation paths | HIGH | RFC 7519 §4.1.4 industry standard ±60s; código-gated constant em `ClerkAdapter::new`; uniform exp/nbf/iat | Static check em construtor + integration test boundary | N/A (constant invariant) |
| **INV-AUTH-ISS-EXACT-MATCH** | Issuer compared exact match (não prefix) | CRITICAL | Allowlist `Vec<String>` exact eq; previne `https://clerk.corelink.dev.attacker.com` confusion | Property test 10k iter random origin/issuer combinations | N/A (regression test) |
| **INV-AUTH-KID-RESOLUTION** | KID miss triggers single refresh + retry; no infinite loop | HIGH | Lazy JWKS refresh + 1 retry max; timeout fail → KidNotInJwks | Integration test KID rotation chaos + counter assert | N/A (state machine) |
| **INV-AUTH-PAT-HASH-ARGON2ID-2024** | PAT hashes use Argon2id m≥65536/t≥3/p≥4 (OWASP 2024) | CRITICAL | All hashes em PHC string format `$argon2id$v=19$m=65536,t=3,p=4$...`; verify rejects underprovisioned params | Static check em deploy gate + cargo-deny version pin `argon2 = "0.5"` | N/A (cripto invariant) |
| **INV-AUTH-PAT-PLAINTEXT-NEVER-PERSISTED** | PAT plaintext nunca em DB / logs / traces / errors | CRITICAL | `PatPlaintext` newtype sem Display/Debug/Serialize; único `into_string()` em mint() return | CI grep gate + clippy custom lint + static analysis | N/A (compile-time enforced) |
| **INV-AUTH-PAT-VERIFY-CONSTANT-TIME** | Mann-Whitney 3-prong + power analysis sustained em CI nightly | CRITICAL | N≥10000 per arm + power 1−β≥0.80 + Šidák 3-trial + bootstrap 95% CI sobre \|Δmedian\| ≤ 5ms | CI nightly job + alert se p < 0.05 sustained 3 trials | N/A (statistical test) |
| **INV-AUTH-PAT-SALT-PER-TOKEN** | Each PAT mint generates unique 16-byte salt via getrandom | HIGH | mint() chama `getrandom` independente per token; PHC string embeds salt | Property test 100k unique salts | N/A (cripto invariant) |
| **INV-AUTH-PAT-SCOPE-DB-IS-SOT** | Scope nunca inferido from PAT prefix string; DB column é fonte | HIGH | Server-side reads `api_tokens.scopes` BIGINT column; PAT prefix é hint apenas | Property test cross-tenant scope spoofing rejection | N/A (architecture invariant) |
| **INV-AUTH-TENANTCTX-IMMUTABLE** | TenantCtx fields private; nenhum mutation path | CRITICAL | Builder pattern em `corelink-worker/middleware/tenant_ctx.rs`; `#[non_exhaustive]`; cargo-deny lints `unsafe = "deny"` | Compile-time enforcement + chaos test layer-mismatch detection | (planned `tenant_ctx_propagation.tla`; PLANNED) |
| **INV-AUTH-5-LAYER-ORDERING** | Middleware layer order é canonical (auth → ctx → scope → rate-limit → audit pre → handler → audit post) | CRITICAL | Tower `ServiceBuilder` composition fixed; integration test asserts ordering via fail-closed assertions | Chaos test 5-layer scramble + fail-closed assertions | (planned `tenant_ctx_propagation.tla`) |
| **INV-AUTH-SESSION-CACHE-KEY-CT** | Session cache key compare é constant-time via `subtle::ConstantTimeEq` | HIGH | KV key construction sha256 truncated 16-byte; lookup compara via subtle | Mann-Whitney timing test em key compare path | N/A (cripto invariant) |
| **INV-AUTH-SCOPE-MIDDLEWARE-LEVEL** | Scope check enforced via Tower layer; routes sem layer = explicit `allow_unauthenticated()` opt-out | HIGH | `axum::routing` requires `require_scope(scope)` OR `allow_unauthenticated`; deny-by-default | Compile-time route definition + integration test | N/A (architecture invariant) |
| **INV-AUTH-AUDIT-PRE-POST-ORDERING** | Pre-handler `auth.token.validated` antes; post-handler `auth.{ok,denied}` depois; same outbox transaction | HIGH | Outbox INSERT em D1 batch atomic com TenantCtx commit (reuse WI-S01-005 pattern) + `tokio::catch_unwind` panic recovery | Property test pré/post pair completeness + chaos test panic recovery | Coberto parcial por `audit_immutability.tla` |
| **INV-AUTH-REVOCATION-IDEMPOTENT** | Retry revoke = single audit event + same revoked_at timestamp | CRITICAL | DO storage idempotent ingest dedup via `(pat_id, revoked_at)` UNIQUE constraint | Property test 100k retries com same pat_id assert single audit event | (planned `auth_revocation.tla`; PLANNED) |
| **INV-AUTH-REVOCATION-SLO-60S** | Cross-region propagation ≤ 60s p99 sustained 72h | CRITICAL | DO + CF Queue at-least-once; tiered alert SEV-2 em > 30s, SEV-1 em > 60s | SLO measurement + chaos test cross-region propagation stress | (planned `auth_revocation.tla`) |
| **INV-AUTH-D1-IS-SOT** | D1 `revoked_at IS NULL` filter authoritative em verify path; DO storage é orchestration optimization | HIGH | Verify cold path query D1 antes de approving; DO `is_revoked()` é admin path only | Architecture review + integration test verify-vs-revoke race | N/A (architecture invariant) |
| **INV-AUTH-MASS-REVOKE-ATOMIC** | Mass revoke é all-or-none via D1 batch | CRITICAL | Single `UPDATE api_tokens SET revoked_at WHERE tenant_id = X` atomic; D1 transactional | Property test 10k mass revoke assert all-or-none | (planned `auth_revocation.tla`) |
| **INV-AUTH-PROPAGATION-AT-LEAST-ONCE** | CF Queue at-least-once + consumer dedup via `(pat_id, revoked_at)` | HIGH | Queue retry policy 5×; DLQ; consumer idempotent ingest | Chaos test queue outage + consumer offline | N/A (queue semantic) |
| **INV-AUTH-SCHEMA-RLS-DEFAULT-ON** | All auth tables have RLS enabled (account/tenant/user_account/membership/api_tokens/webauthn_credentials/revocation_log) | CRITICAL | `ALTER TABLE ... ENABLE ROW LEVEL SECURITY` em migration 002; CI gate verifies `pg_class.relrowsecurity = true` | CI test query `SELECT relrowsecurity FROM pg_class WHERE relname IN (...)` | N/A (DB invariant) |
| **INV-AUTH-PII-ENCRYPTED** | email + webauthn keys store as BYTEA (ciphertext) via pgcrypto | CRITICAL | `pgp_sym_encrypt_bytea` em INSERT; raw text never persisted | CI test verify ciphertext format em rows; backup leak chaos | N/A (cripto + DB invariant) |
| **INV-AUTH-MIGRATION-ADDITIVE** | No DROP TABLE/COLUMN ou ALTER COLUMN destructive em migrations | HIGH | `scripts/check_migrations_additive.py` CI gate diff vs main | CI gate em PR | N/A (governance invariant) |
| **INV-AUTH-CASCADE-DSR-COMPLETE** | account DELETE cascades tenant + user_account + membership + api_tokens + webauthn_credentials | HIGH | FK `ON DELETE CASCADE` policies em DDL; integration test full cascade | DSR cascade integration test | N/A (DB invariant) |
| **INV-AUTH-AUDIT-PSEUDONYMIZATION** | Audit chain retains pseudonymous IDs (sha256 prefix); DSR cascade não touches audit chain | CRITICAL | Pseudonymization em emit time; chain integrity preserved post-erasure | DSR integration test + LGPD Art. 18 compliance | Coberto por `audit_immutability.tla` |
| **INV-AUTH-WEBAUTHN-UV-REQUIRED-ADMIN** | Admin step-up requires UV=1 (biometric/PIN); UV=0 rejected | CRITICAL | `WebAuthnAdapter::finish_authentication` checks `flags & UV != 0` em admin paths | Adversarial regression test + CI integration test | N/A (W3C spec compliance) |
| **INV-AUTH-WEBAUTHN-ATTESTATION-VERIFIED** | Registration verifies attestation chain; AAGUID em allowlist | CRITICAL | `webauthn-rs::start_registration` + `finish_registration` enforces; AAGUID lookup em config | Cargo-fuzz CBOR/COSE 1h + adversarial test | N/A (W3C spec compliance) |
| **INV-AUTH-WEBAUTHN-SIGN-COUNT-MONOTONIC** | sign_count strictly increasing; regression = SEV-1 alert | HIGH | Server checks `response.sign_count > stored.sign_count`; W3C accepts sign_count=0 (não monotonic em passkey ecosystem); regression triggers SEV-1 | Property test 1k random ceremonies + chaos test replay | N/A (W3C spec compliance) |
| **INV-AUTH-WEBAUTHN-ORIGIN-EXACT** | Origin allowlist exact match (no prefix bypass); rejects subdomain spoof | CRITICAL | `Vec<Url>` exact eq compare; W3C §13.4.9 origin matching | Adversarial regression test (origin spoof) + CI test | N/A (W3C spec compliance) |
| **INV-AUTH-WEBAUTHN-RP-ID-CANONICAL** | RP ID = "corelink.dev" eTLD+1 (not subdomain) | CRITICAL | `WebAuthnAdapter::new` validates rp_id é eTLD+1; fails se subdomain | Static check em construtor | N/A (W3C spec compliance) |
| **INV-AUDIT-NO-RAW-PII** | Zero raw PII em chain (email, raw principal_id, raw pat_id) | CRITICAL | `redact_pat!` macro mandatory; `hash_principal_id` 64-bit prefix; CI lint enforces | CI lint custom binary `tools/audit_pii_lint/` + grep CI gate | N/A (compile-time + CI lint) |
| **INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER** | Audit outbox INSERT em mesma D1 batch que TenantCtx commit | CRITICAL | `db.batch([INSERT blob_meta OR TenantCtx, INSERT audit_outbox])` atomic; failure rolls back | Chaos test D1 batch failure + assert no orphan TenantCtx | Coberto parcial por `audit_immutability.tla` |
| **INV-AUDIT-CHAIN-HASH-DETERMINISTIC** | content_hash deterministic via canonical JSON (RFC 8785 / serde_jcs) | HIGH | `serde_jcs` crate (JCS) + Unicode NFC; property test serialize twice byte-equal | Property test deterministic JSON 1000 events | N/A (cripto invariant) |
| **INV-AUDIT-EVENT-TYPE-EXHAUSTIVE** | All AuthEventType variants têm AuthEventData payload impl + serde tag | HIGH | Rust `match` exhaustive em internal handlers; `#[non_exhaustive]` em external surface | Compile-time + property test enum exhaustive | N/A (compile-time) |
| **INV-AUDIT-RETENTION-HINT-ACCURATE** | Event retention_hint matches tenant.tier (Solo30d/Team90d/Business1y/Enterprise7y) | HIGH | Lookup-time em emit; property test verify hint matches tier | Property test 4 tier types + integration test | N/A (architecture invariant) |
| **INV-NEG-CACHE-MONOTONIC** | Negative cache writes use monotonic version_stamp; older stamps rejected silently | HIGH | KV value envelope inclui version_stamp u64; put_miss compara antes write | Property test concurrent put_miss vs invalidate_on_write | N/A (KV invariant) |
| **INV-NO-BODY-IN-LOGS** | Body bytes nunca em audit/logs/error messages | CRITICAL | `redact_pat!` + clippy custom lint + grep CI gate | Static analysis + CI gate | N/A (compile-time + CI lint) |
| **INV-NO-PII-IN-LOGS** | Raw PII (email, principal_id) nunca em logs/traces; hashed prefix only | CRITICAL | `hash_principal_id` macro + tracing field redaction | Static analysis + CI gate | N/A (compile-time + CI lint) |

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
| **INV-AC-MERKLE-VALID** | envelope.merkle_root binds tree structure; verify_structure rejects tampering 100% | CRITICAL | `corelink-ac::verify_structure` re-build tree from envelope.result; compare to envelope.merkle_root; mismatch → MerkleError::RootMismatch | Property test 10k iter `prop_merkle_tampering_detected` + chaos test envelope byte flip | (planned `cas_integrity.tla` AC variant; PLANNED) |
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
| **INV-AC-EVICT-TENANT-SCOPED** | DELETE strict tenant_id filter; cross-tenant impossible | CRITICAL | SQL `DELETE WHERE tenant_id = ? AND action_digest = ?` strict; CI grep gate em ttl module forbids DELETE sem tenant_id clause | Property test 10k iter `prop_ttl_tenant_isolation` + chaos #2 cross-tenant attempt | (planned `tenant_isolation.tla` AC eviction variant; PLANNED) |
| **INV-AC-EVICT-CONSISTENCY** | R2 DELETE before D1 DELETE; orphan R2 < orphan D1 ref; reconcile diário S-06 catches drift | HIGH | Cron flow: R2 DELETE → on success → D1 DELETE → KV invalidate → audit emit; if R2 fails, abort batch row + preserve D1; metric corelink.ac.ttl.r2_delete_failed_total | Chaos test #3 R2 outage + integration test cycle | N/A (architecture invariant) |
| **INV-AC-TTL-MONOTONIC** | Refresh-on-hit increments last_hit_at + extends expires_at; never decreases | HIGH | Handler refresh logic threshold 60s gate; `UPDATE ac_meta SET last_hit_at = $1, expires_at = $1 + tier_ttl WHERE …` | Property test `prop_ac_ttl_refresh_monotonic` + integration test storm | N/A (architecture invariant) |
| **INV-AC-PATH-KEY-MATERIALIZED** | tenant_prefix BLOB(16) materialized em ac_meta column (Lote 10.4bis fix; resolves cron TDK access expansion) | HIGH | Schema column `tenant_prefix BLOB(16) NOT NULL`; pre-computed em handler INSERT step [7]; cron worker reads column directly without TDK access | CI test schema column exists; integration test prefix consistency | N/A (architecture invariant) |
| **INV-AC-PATH-SIG-KEY-VERSION-INDEPENDENT** | `sig_key_id` and `path_key_id` MAY diverge per row (Lote 10.4-tris P0-R5-006); no atomic snapshot required across rotation; valid state, NOT invariant violation; handler INSERT uses current values at write time | HIGH | Two key-version columns in ac_meta (Lote 10.4bis WI-S04-002 schema); rotation procedures independent (WI-S04-004 §30.1); GET handler reads `tenant_prefix` from materialized column NOT recomputed from TDK | Property test `prop_path_sig_key_independent` simulates concurrent rotation between path-INSERT and sig-INSERT steps; integration test asserts no error on divergence | N/A (architecture invariant; ADR-0021 §RotationProcedures) |
| **INV-AC-ORPHAN-R2-CLEANUP-EVENTUAL** | Orphan R2 envelopes (R2 PUT succeeds, D1 INSERT fails) cleaned within 24h via S-06 reconcile cron | HIGH (forward-dependency S-06) | S-06 reconcile cron diário cross-checks R2 vs D1; orphan rate alert metric `corelink.ac.r2.orphan_rate` (alert if >1% of UPDATE rate) | S-06 forward; chaos test handler crash mid-flight; metric monitoring | N/A (architecture invariant; eventual consistency) |
| **INV-AC-OUTPUTS-VALID-EVENTUAL-CONSISTENCY** | INV-AC-OUTPUTS-VALID is point-in-time best-effort; TOCTOU race between handler outputs check and S-06 GC tombstone window accepted | HIGH (forward-dependency S-06) | SQLite/D1 has no row-level locking; Postgres `SELECT ... FOR SHARE` unavailable; reconcile diário (S-06 forward) é authoritative drift detection mechanism; orphan_rate metric monitors | S-06 forward reconcile cron; chaos test #8 (TTL eviction race + S-06 GC race); 24h SLA bound on drift correction | N/A (eventual consistency tier) |

**Cross-references**:
- `ADR-0021, ADR-0035, ADR-0036, ADR-0037` documentam decisões S-04 (vide `scripts/validate_references.py` whitelist).
- ADR-0021 ratificada em Lote 10.4bis (HKDF vs Ed25519 + canonical_bytes binding extension + salt parameterization).
- ADR-0019 ratificada em Lote 9.5b (TTL handoff S-04 → S-07).
- TLA+ specs PLANNED em §4.2 (cas_integrity.tla AC variant, tenant_isolation.tla AC eviction variant) — implementação em sprints S-09 ou S-12.
- `data_model.md §4.2` schema canonical (tenant_prefix column adicionada Lote 10.4bis).
- `error_taxonomy.md §3.2` mapeia AC errors (12 codes pós-Lote 10.4bis amendment).
- `compliance_matrix.md` mapeia INV-AC-* para LGPD/GDPR/SLSA L3 alignment.

**Aliases históricos:** nenhum. Estes 19 IDs introduzidos em Lote 10.4 (sprint S-04 spec) e promovidos ao registry em Lote 10.4bis (P0 fix Agent R4 review remediation r4-s04-part1+part2). **CI gate enforcement**: `scripts/validate_inv_promotion.py` valida que todo INV declarado em WI sob `specs/04_sprints/SXX/work_items/` existe nesta seção (closes 4-sprint persistent gap flagged em S-01/S-02/S-03 R4 reviews).

---

### 3.16 Multipart Upload + Chunking domain — Lote 10.5 (S-05 sprint)

Invariantes que governam multipart upload, chunking determinism, manifest dual-side verify, R2 multipart adapter, sweeper orphan abort. **Promovidas preemptivamente em Lote 10.5** (consistency com lesson Lote 10.4bis CI gate validate_inv_promotion.py — INVs declared em WIs DEVEM existir em registry pré-SEAL); refinements possíveis em Lote 10.5bis pós-Agent R4 review remediation.

| ID | Nome | Severidade | Descrição | Enforcement | TLA+ file |
|---|---|---|---|---|---|
| **INV-MULTIPART-IDEMPOTENT** | SplitBlob mesma `(tenant_id, blob_digest)` retorna existing manifest_digest; cas_blobs.is_chunked flag previne re-chunking; chunks ON CONFLICT increments refcount; multipart_sessions UNIQUE in_progress | HIGH | Handler step [3.5] short-circuits if is_chunked=true; D1 INSERT ON CONFLICT DO UPDATE refcount += 1; UNIQUE `(tenant_id, blob_digest, state='in_progress')` em multipart_sessions | Property test 10k iter `prop_split_idempotent` + integration test re-Split | (planned `cas_integrity.tla` chunked variant; PLANNED) |
| **INV-MULTIPART-MANIFEST-SIGNED** | Manifest envelope signed com HKDF info=`b"manifest-sig"` separated domain `b"ac-sig"` (WI-S04-004 ADR-0021 reuse pattern) | HIGH | `corelink-manifest::sig` HKDF info constant; CI byte-equal test asserts; ADR-0021/0038/0041 documents domain separation | CI test grep + assert + integration test cross-domain replay rejection | N/A (cripto invariant) |
| **INV-MULTIPART-CONCURRENCY-BOUNDED** | Per-tenant semaphore caps SplitBlob concurrency (default 4); 429 if exhausted; tunable per-tier S-13 forward | HIGH | `tokio::sync::Semaphore::new(4)` per-tenant; ConcurrencyLimitReached error → 429 + Retry-After; metric alert | Property test concurrency storm + chaos test 1000 parallel | N/A (architecture invariant) |
| **INV-MULTIPART-CHUNK-DETERMINISTIC** | FastCDC mask seeds fixed em ChunkerConfig::default(); same input → same chunks byte-identical (Fixed and FastCDC) | CRITICAL | `corelink-chunker` mask_s/mask_l constants; ADR-0022 stability commitment; test vectors Annex; CI byte-equal | Property test 1000 random × 100 chunkings = 100% byte-identical; ADR-0022 ratificada Lote 10.5 | N/A (cripto invariant) |
| **INV-MULTIPART-BOUNDED-PARSER** | Chunker MAX_BLOB_SIZE 160 GiB; MAX_CHUNKS_PER_BLOB 81920; manifest MAX_CHUNK_COUNT 81920 (Lote 10.5-tris P1-SR5-001/004 cross-crate alignment; 160 GiB / 2 MiB = 81920 exact); MAX_TOTAL_SIZE 160 GiB; enforced at decode time | HIGH | `corelink-chunker::bounds` + `corelink-manifest::bounds` constants share single source of truth; reject early; cargo-fuzz harness 1h CI nightly | Property test + cargo-fuzz × 3 targets (chunker fixed + chunker fastcdc + manifest decode) | N/A (parser invariant) |
| **INV-MULTIPART-STREAMING-MEMORY** | Per-request stack ≤ 4 MiB (chunker buffer 2 MiB + headroom); zero-allocation Iterator pattern | HIGH | `corelink-chunker::Chunker::feed` returns `impl Iterator<Item = Chunk<'a>>` borrowing internal buffer; lifetime-bounded; integration test 1 GiB blob no OOM | Integration test + valgrind/MSAN no leak | N/A (architecture invariant) |
| **INV-MULTIPART-ORPHAN-DETECTABLE** | `MultipartAdapter::list_orphans(bucket, max_age)` enumerates sessions with `last_activity_at < now - max_age` (default 7d); sweeper consumes; **guarantee holds across D1 shard splits** (sweeper multi-shard aware during dual-write window D-10 to D-3 per ADR-0040 §A1; Lote 10.5-tris P0-SR5-004 fix) | HIGH | D1 SELECT multipart_sessions WHERE state='in_progress' AND last_activity_at < now - max_age + R2 ListMultipartUploads; sweeper queries BOTH old + new shards during shard-split dual-write window; deduplicates by session_id | Sweeper cron DO test (WI-S05-006); RB-FM-060 dry-run; chaos test "mid-shard-split orphan detection" (Lote 10.5-tris) | N/A (architecture invariant; FM-060 mitigation; ADR-0040 §A1) |
| **INV-MULTIPART-PATH-TENANT-SCOPED** | R2 object_key inclui tenant_prefix (Layer 4); never trusts client-provided path components | CRITICAL | `MultipartAdapter` constructs object_key from tenant_prefix BLOB(16) materialized em chunks/multipart_sessions; CI grep gate forbids client-path concat | Property test 10k iter cross-tenant path attempt rejection | (planned `tenant_isolation.tla` multipart variant; PLANNED) |
| **INV-MULTIPART-STATE-MONOTONIC** | multipart_sessions state in_progress → completed OR aborted; never reverse | HIGH | Handler-level enforcement (não CHECK em D1 — CHECK doesn't model state transitions); INV documented + integration test asserts | Integration test reverse transition rejected; chaos test | N/A (architecture invariant) |
| **INV-MULTIPART-PATH-KEY-MATERIALIZED** | tenant_prefix BLOB(16) materialized em chunks + multipart_sessions columns; cron worker reads sem TDK access (lesson Lote 10.4bis WI-S04-005) | HIGH | Schema columns `tenant_prefix BLOB NOT NULL` + `path_key_id INTEGER`; CHECK length=16; pre-computed em handler INSERT | CI test schema column exists; integration test prefix consistency | N/A (architecture invariant) |
| **INV-MULTIPART-MANIFEST-VALID** | manifest.merkle_root binds chunk tree; verify_structure rejects 100% tampered manifests | CRITICAL | `corelink-manifest::verify_structure` re-builds tree from manifest.chunks; compare to manifest.merkle_root; mismatch → MerkleError::RootMismatch | Property test 10k iter `prop_manifest_tampering_detected` + chaos test envelope tampering | (planned `cas_integrity.tla` chunked variant; PLANNED) |
| **INV-MULTIPART-DUAL-SIDE-VERIFY** | Server pre-persist + client post-download both invokeable; verify_structure independent of sig (defense-in-depth para partial chave compromise) | HIGH | Server-side em UpdateActionResult-equivalent (SplitBlob handler); client-side via SDK (S-15 Rust SDK; non-Rust clients per spec doc reference vectors) | Integration test dual-side; chaos test partial chave compromise | N/A (architecture invariant) |
| **INV-MULTIPART-STREAMING-VERIFY-FAIL-FAST** | Mid-stream tampered chunk catches before chunk N+1 processed (SpliceBlob streaming verify) | HIGH | `corelink-manifest::verify_streaming` per-chunk hash verify inline; fail-fast em mismatch; signal handler abort | Property test 100 manifests × 1 tampered chunk → 100% caught at chunk N | N/A (architecture invariant) |

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
| **INV-GC-REACHABLE-SET-COMPLETE** | Mark phase 3-pass scan covers `union(blob_meta + ac_meta + manifest_chunks)` | CRITICAL | `corelink-gc::mark` 3 distinct passes; reachable superset-safe | Property test 10k random tenant states; reachable identified correctly | Coberto por `gc_correctness.tla` (InvGCNeverDeleteReachable) |
| **INV-GC-MARK-TENANT-SCOPED** | All mark queries WHERE tenant_id = ctx.tenant_id; sqlx prepared; clippy lint | CRITICAL | sqlx prepared statements; clippy custom lint forbids `&str` SQL literals; TenantCtx-only enforcement | CI lint + integration test cross-tenant injection rejected | N/A (architecture invariant; Lote 10.4bis lesson) |
| **INV-GC-MARK-PHASE-BUDGETED** | Mark phase ≤ 10 min p99 @ 1M blobs; PhaseBudgetExceeded error if exceeds | HIGH | Criterion benchmark CI gate; SEV-2 alert if exceeds | Benchmark `bench_mark_1m_blobs` + integration test 5M blobs PhaseBudgetExceeded | N/A (SLO invariant; SLO-FRESH-GC) |
| **INV-GC-MARK-D1-BOUNDED-BATCH** | 250 rows/batch + 100ms jitter (Lote 10.4bis D1 100KB limit lesson) | HIGH | Hard-coded batch size; D1 throttle adaptive halve | Integration test batch boundary; D1 100KB constraint validated | N/A (architecture invariant) |
| **INV-GC-SWEEP-AUDIT-FAIL-CLOSED** | Audit emit failure → sweep ROLLBACK; preserves INV-GC-001 + INV-OBS-AUDIT-CHAIN-INTEGRITY | CRITICAL | D1 batch atomic (blob_meta UPDATE + gc_candidate UPDATE + audit_outbox INSERT); both succeed or both fail; SweepError::AuditEmissionFailed | Chaos test simulate audit_outbox INSERT failure; integration test asserts no soft-delete persists | N/A (cripto + governance invariant) |
| **INV-GC-SWEEP-IDEMPOTENT** | Re-run on already-swept candidate = AlreadySwept no-op | HIGH | `gc_candidate.status` UPDATE atomic; AlreadySwept SweepDecision; PAT-RETRY-IDEMPOTENT-001 | Property test 1000 sweep + re-sweep same candidate | N/A (architecture invariant) |
| **INV-GC-SWEEP-TENANT-SCOPED** | All sweep queries tenant-scoped strict; cross-tenant impossible | CRITICAL | sqlx prepared + tenant_id NOT NULL + clippy lint | Property test 1000 concurrent sweeps different tenants; CI lint | N/A (architecture invariant; Lote 10.4bis lesson) |
| **INV-GC-GRACE-RESPECTED** | Physical-delete (WI-S06-004) only fires post-grace; CAP-GC-002 reversibility | HIGH | SQL filter `WHERE blob_meta.deleted_at_ms < (now - grace_period)` strict `<`; env-config grace 72h CAS / 24h AC | Integration test boundary; chaos test reversibility | N/A (architecture invariant) |
| **INV-GC-PHYSICAL-DELETE-IDEMPOTENT** | Re-run on already-deleted = no-op (PAT-RETRY-IDEMPOTENT-001) | HIGH | R2 DeleteObject S3-compatible idempotent; D1 row purge idempotent | Property test 10k iter `prop_physical_delete_idempotent` | N/A (architecture invariant) |
| **INV-GC-GRACE-BOUNDARY-STRICT** | SQL `<` (NOT `<=`) grace boundary; reversibility window respected | CRITICAL | Hard-coded `<` em SQL filter; integration test boundary `deleted_at = exact boundary` → NOT deleted | Chaos test #1 boundary; integration test asserts | N/A (architecture invariant) |
| **INV-GC-R2-D1-ORDERING** | R2 DeleteObject before D1 purge; orphan recoverable; reconcile detects | HIGH | Atomic R2-then-D1 ordering (Lote 10.4bis WI-S04-005 lesson); chaos test R2 fail preserves D1 | Chaos test #2 R2 outage + integration test recovery | N/A (architecture invariant) |
| **INV-GC-DSR-BYPASS-AUTHORIZED** | DSR signal verified pre-bypass grace; S-11 forward auth | HIGH | DSR signal authentication mandatory; bypass path isolated | S-11 forward integration test stub | N/A (architecture invariant) |
| **INV-GC-RECONCILE-AUTO-FIX-BOUNDED** | Auto-fix gate: `drift_count ≤5 AND drift_percent ≤0.01%` per tenant (scale-invariant percentage-floor + absolute-floor; Lote 10.6bis P0-6 + Lote 10.6-tris OPUS-MISS-4); > 5 records OR > 0.01% → manual review + SEV-1; auto-fix failure mode: 3-attempt exponential backoff → `gc_drift_pending` table → SEV-2 (NOT SEV-1) | HIGH | Dual-condition gate (configurable env); SEV-1 alert + paused state if either condition exceeded | Property test `prop_auto_fix_scale_invariant` boundary at 10/1k/100k tenant sizes; integration test SEV escalation; `prop_auto_fix_failure_drift_pending` D1 throttle resilience | N/A (architecture invariant) |
| **INV-GC-RECONCILE-AUDIT-FAIL-CLOSED** | Audit emit failure → reconcile ROLLBACK | CRITICAL | D1 batch atomic; SweepError::AuditEmissionFailed analog | Chaos test simulate audit fail; integration test asserts | N/A (cripto + governance invariant) |
| **INV-GC-CI-GATE-ENFORCED** | TLA+ CI gate blocks merge se TLC red; override via ADR + Architect + Crypto SME | HIGH | GitHub branch protection required status check; PR fail logic | CI workflow `tla-ci-gate.yml`; integration test PR weakening obligation rejected | (planned `gc_correctness.tla` Lote 5.13 verified) |
| **INV-GC-PROPERTY-TEST-CROSS-VALIDATED** | Rust property test 100k iter cross-validates TLA+ obligations (Mark + UpdateActionResult interleavings) | HIGH | `prop_gc_004_race_mark_update_ar_100k` em CI nightly; deterministic seeds; 0 violations sustained | CI nightly green sustained 30d (S-20 GA gate) | (cross-validation; gc_correctness.tla aligned) |
| **INV-GC-30D-SUSTAINED-VERIFICATION** | 30d sustained TLA+ verde + chaos zero violations gate pre-S-20 GA | HIGH | CI workflow `tla-30d-sustained.yml` daily aggregate; chaos test 30d staging continuous | Sprint contract DoD §10.s06.4 + Critério Promoção | (governance invariant; Lote 10.6 ship gate) |

**Cross-references**:
- `ADR-0042` documenta worker scheduler design + degrade-mode contract (vide `scripts/validate_references.py` whitelist).
- `specs/tla/gc_correctness.tla` (Lote 5.13 + 7.1 fixes) — formal verification baseline.
- `failure_modes.md FM-300/305/404` — runbook RB-FM-300/404/305 dry-runs em WI-S06-007.
- `security_model.md CTRL-GC-001/002` — control alignment.
- `slo_catalog.md SLO-CORRECT-GC + SLO-FRESH-GC` — operational metrics.

**Aliases históricos:** nenhum. Estes ~22 IDs introduzidos em Lote 10.6 (sprint S-06 spec) e **promovidos preemptivamente em Lote 10.6** (consistency com lesson Lote 10.4bis CI gate validate_inv_promotion.py + lesson Lote 10.5 §3.16 promovida preemptive); refinements possíveis em Lote 10.6bis pós-Agent R4 review.

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
| INV-DATA-RESIDENCY | HIGH | Subsumed por INV-REGION-NO-CROSS-LEAK (S-14 PLANNED `region_residency.tla` / `byok_sovereignty.tla`) |
| INV-DEDUP-CONSISTENCY | HIGH | Algorithmic property; UNIQUE index enforces; covered by property test S-07 |
| INV-RATE-LIMIT-PROPORTIONALITY | HIGH | DO atomic counter; covered by property test 10k iter |
| INV-OBS-CARDINALITY-BUDGET | HIGH | Static budget validator CI (`cardinality_check.py`); non-distributed |
| INV-OBS-AUDIT-CHAIN-INTEGRITY | HIGH | Hash chain; coberto por `audit_immutability.tla` indiretamente; daily verifier job |
| INV-ADMIN-MFA-FRESHNESS | HIGH | Middleware timestamp check; non-distributed |
| INV-ERASURE-ATTESTATION-SIGNED | HIGH | Signature verification; algorithmic; per-region Ed25519 key |
| INV-CONSENT-PROOF-VERIFIABLE | HIGH | Coberto por `dsr_erasure_atomicity.tla` planned (S-11) via InvConsentSymmetry |
| INV-CAS-SIDE-CHANNEL-INDISTINGUISHABLE | HIGH | Statistical algorithmic property (Mann-Whitney U); criterion benchmark + adversarial test 10k samples; não state-machine distributed |
| INV-KEY-AUDIT | HIGH | Coberto por `audit_immutability.tla` |
| INV-KEY-OVERLAP | HIGH | Per-asset table canonical em `key_management.md §3.2.1` + ADR-0018; covered by `key_lifecycle.tla` PLANNED (S-13 §4.2 entry) |

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
