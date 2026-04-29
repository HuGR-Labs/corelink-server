---
id: "AUTH-MODEL"
type: "auth_model"
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
tags: ["architecture", "security", "auth", "multi-tenant"]
---

# Auth Model — Principals, Scopes, Rotation, Revocation

> **doc_status:** DRAFT
> **Versão:** 0.1.0
> **Última atualização:** 2026-04-24
> **Owner:** Gustavo Schneiter
> **Aprovador Final:** Gustavo Schneiter
> **Revisores:** ⚠️ **staffing-blocked** — promoção a `doc_status: FROZEN` bloqueada até ≥ 2 reviewers nomeados conforme roles indicados (endereça F-09 audit Lote 3+4)
> **Supersedes:** —
> **Superseded By:** —

> **Propósito:** fonte canônica (Nível 3) do modelo de autenticação/autorização do CoreLink. Endereça audit v1 Gap 7: "falta modelo sério de auth principal, key scopes, rotação, revogação, propagação, read-only vs write, executor identity".
>
> Referência normativa: ADRs/WIs/PRRs **DEVEM** citar este documento ao tocar identidade, escopo, token, rotação ou revogação. `inherits_from: ["AUTH-MODEL"]` obrigatório quando aplicável.

> **🚦 Phase boundary (F-12 audit Lote 3+4):**
> CoreLink GA inicial cobre **Fase 1 — Remote Cache** (CAS + AC + GC + REAPI cache-only).
> **Fase 2 — Remote Execution** (`execute-action`, executor identity, sandbox runtime) é **futuro** (roadmap pós-GA).
> Seções/CTRLs/SLOs/labels marcados com `(Fase 2)` ou `execute-action` referem-se a planejamento; em GA inicial podem ser omitidos do scope mínimo.



---

## Sumário

1. [Taxonomia de Principals](#1-taxonomia-de-principals)
2. [Credenciais e Tokens](#2-credenciais-e-tokens)
3. [Scopes (o que cada credencial pode fazer)](#3-scopes-o-que-cada-credencial-pode-fazer)
4. [Authorization: permission model](#4-authorization-permission-model)
5. [Rotation (rotação de credenciais)](#5-rotation-rotação-de-credenciais)
6. [Revocation (revogação imediata)](#6-revocation-revogação-imediata)
7. [Audit trail obrigatório](#7-audit-trail-obrigatório)
8. [Tenant Isolation enforcement](#8-tenant-isolation-enforcement)
9. [Testes de conformidade](#9-testes-de-conformidade)
10. [Referências + concorrentes SOTA](#10-referências--concorrentes-sota)
11. [Change Log](#11-change-log)

---

## 1. Taxonomia de Principals

Um **principal** é qualquer entidade autenticada que executa operação no CoreLink. Todas pertencem a um `tenant_id`.

### 1.1 User (humano)

| Aspecto | Valor |
|---|---|
| Identity provider | Clerk (SSO, email, social) |
| Auth token | JWT curto (1h) — refresh via Clerk session |
| Uso típico | Dashboard, management API (criar token, configurar tenant) |
| Cross-tenant? | **Não** — user autenticado pertence a exatamente 1 tenant (+ org mgmt via Clerk multi-org se habilitado) |
| Revogação | Logout Clerk + invalidate session |

### 1.2 CLI / Developer token

| Aspecto | Valor |
|---|---|
| Identity | API token gerado pelo user via dashboard |
| Format | `corelink_pat_<base64>` (Personal Access Token) |
| Storage client-side | `~/.corelink/credentials` (mode 0600) |
| Rotation | Manual ou scheduled (ver §5) |
| Default lifetime | 90 dias, renovável |
| Scopes | Configuráveis, default `read+write` no tenant do user |
| Revogação | Via dashboard ou API `DELETE /tokens/{id}` (propagação <1min) |

### 1.3 CI token (machine)

| Aspecto | Valor |
|---|---|
| Identity | Token dedicado pra CI (GitHub Actions, GitLab CI) |
| Format | `corelink_ci_<base64>` |
| Storage | Secret manager do CI (never commited) |
| Rotation | 180 dias (menos frequente — é usado em volume) |
| Scopes | Tipicamente `cache-rw` (read-write em cache); não tem permissão admin |
| Revogação | Mesma UI do user token |

### 1.4 Read-only token

| Aspecto | Valor |
|---|---|
| Identity | Token dedicado sem permissão de escrita |
| Format | `corelink_ro_<base64>` |
| Uso típico | Forks externos, mirror setups, pipelines read-heavy |
| Scopes | Apenas `cache-r` e `cache-find-missing` |
| Revogação | Idem |

### 1.5 Executor identity (REAPI Remote Execution — futuro)

| Aspecto | Valor |
|---|---|
| Identity | Executor autenticado que reporta resultados de build |
| Format | mTLS cert + token |
| Scopes | `execute-action` + `report-result` + `cache-rw` |
| Uso típico | Fase 2 do produto (Remote Execution além de Remote Cache) |

### 1.6 Service identity (inter-service)

| Aspecto | Valor |
|---|---|
| Identity | Service-to-service: CoreLink Container → R2/D1/KV/DO |
| Implementação | Cloudflare bindings (não são tokens explícitos; são capabilities runtime) |
| Security | Binding é controlado pelo wrangler config — nunca exposto fora de Cloudflare |

### 1.7 Proibidos

- **Shared credentials** entre múltiplos usuários: proibido. Cada user tem token próprio.
- **Long-lived tokens sem rotation**: proibido. Default 90d com aviso em 70d.
- **Tokens em URL paths**: proibido. Sempre em `Authorization: Bearer` header ou gRPC metadata.
- **Tokens em logs**: obrigatoriedade de mascarar antes de emit.

---

## 2. Credenciais e Tokens

### 2.1 Formato canônico

Todo token CoreLink tem formato:

```
corelink_<env>_<token_id>.<random_secret> (cycle 7 codex SEAL canonical; replaces pre-fix base64 form)
```

Onde `<env>` é um dos: `pat` (user-emitted), `ci` (CI runner token), `ro` (read-only token), `exec` (Fase 2 executor; reserved). Format canonical: `corelink_<env>_<token_id>.<random_secret>` (per WI-S03-002 §6.1; cycle 6 codex SEAL alignment com spec_contract §5 R-S03-3 + WI-S03-005 PAT lookup primitive).

### 2.2 Geração

- Server-side via `secrets` (CSPRNG) → 32 bytes random → base64.
- PAT format canonical = `corelink_<env>_<token_id>.<random_secret>` (per WI-S03-002 §6.1; cycle 6 codex SEAL): `token_id` = 16-char deterministic index key (UUIDv7 short form OR SHA-256 prefix); `random_secret` = 32 bytes random base64url.
- **Argon2id PHC string** (com salt) armazenado em **Neon `pat.token_hash`** (canonical SoT per data_model.md §4.1; nunca plaintext); per OWASP 2024 PAT hashing best practice. Lookup é deterministic via `token_id` index (NÃO por token_hash que é salted; constant-time compare via Argon2 verify após row found by token_id).
- Plaintext retornado 1x ao user (no momento da criação); não recuperável depois.

### 2.3 Validação

Cada request autenticada:
1. Parse `Authorization: Bearer <token>` ou gRPC metadata.
2. Hash SHA-256 do token.
3. **Step 3a**: Parse PAT format → extract `token_id` (deterministic prefix; per WI-S03-002 §6.1 PAT format canonical).
3. **Step 3b**: Lookup em **Neon `pat`** WHERE `token_id = :tid AND revoked_at IS NULL AND (expires_at IS NULL OR expires_at > now())` (deterministic indexed lookup; ≤ 10ms p99 per WI-S03-005 §10).
3. **Step 3c**: Argon2id verify(`random_secret_provided`, `pat.token_hash`) com constant-time compare; reject se mismatch (canonical SoT per data_model.md §4.1).
4. Load scopes associados.
5. Populate request context com `principal_id`, `tenant_id`, `scopes`.
6. Atomic update `last_used_at` (async, batched).

### 2.4 JWT (Clerk — users apenas)

JWT validação via JWKS fetch cache:
- Verify signature via Clerk JWKS (cached 1h).
- Verify `exp`, `iss`, `aud`.
- Extract `sub` (user ID) + `org_id` → resolve `tenant_id`.
- Scopes derivados do role Clerk (admin vs member).

---

## 3. Scopes (o que cada credencial pode fazer)

### 3.1 Taxonomia canônica

Scopes seguem padrão `<resource>-<action>`. Lista completa:

| Scope | Descrição | Quem tipicamente tem |
|---|---|---|
| `cache-r` | Read CAS blobs + AC results | read-only token |
| `cache-w` | Write CAS blobs + AC results | CI token, dev token |
| `cache-rw` | Abrev: `cache-r` + `cache-w` | CI token, dev token |
| `cache-find-missing` | Executar `FindMissingBlobs` (não implica download) | read-only token, CI |
| `cache-delete` | Delete blobs (rare, admin) | admin token |
| `admin-tenant-read` | Listar config do tenant | admin dashboard |
| `admin-tenant-write` | Mudar config do tenant | admin dashboard |
| `admin-tokens` | Gerar/revogar tokens | admin dashboard |
| `admin-billing` | Ver/mudar billing | admin dashboard |
| `admin-audit` | Ler audit logs do tenant | admin dashboard + compliance |
| `admin-users` | Gerenciar users do tenant | admin dashboard |
| `execute-action` | (Fase 2) Executor pode reportar action start | executor |
| `report-result` | (Fase 2) Executor pode reportar result | executor |

### 3.2 Escopos são **aditivos**

Token pode ter múltiplos scopes; permission check é "token tem scope X?".

### 3.3 Princípio: Least Privilege (PRINC-020)

- Default de CLI/CI tokens: `cache-rw` + `cache-find-missing` (não têm admin).
- Admin tokens **apenas** pra usuários com role "admin" no tenant.
- Read-only tokens **nunca** têm `cache-w`.

### 3.4 Scope expansion requer PRR

Adicionar scopes novos é mudança de superfície de segurança. **DEVE** seguir:
1. ADR propondo novo scope.
2. Security review.
3. PRR antes de GA (FF-HR-005 do §33.5.3 aplica — força HIGH_RISK).

---

## 4. Authorization: permission model

### 4.1 Tenant isolation é **primeiro filtro**

Toda operação autenticada tem `tenant_id` derivado do principal. Scope é aplicado **dentro** do tenant.

```pseudocode
// Generic auth boundary (S-03 middleware):
if request.tenant_id != principal.tenant_id:
    return 403  // PAT scope/identity mismatch — non-CAS-read paths

// CAS read handlers (S-02 ADR-0028 override): cross-tenant blob access masked as 404 uniform
// (existence oracle closed; per ADR-0028 MissReason → 404 freeze):
if request.path.starts_with("/cas/") AND blob_meta.tenant_id != principal.tenant_id:
    return 404 with COR_CAS_BLOB_NOT_FOUND  // CrossTenantMasked variant
    + audit emit "corelink.cas.cross_tenant_attempt" (forensics retain reason)
```

Isso é tão crítico que merece invariant dedicado: **INV-TenantIsolation** (CRITICAL, forcing factor FF-HR-002). Per ADR-0028 (S-02 GA freeze), CAS read handlers retornam 404 uniform across MissReason variants (NotFound × CrossTenantMasked × Tombstoned) para fechar enumeration oracle; 403 reservado para PAT scope failures (não cross-tenant blob access).

### 4.2 Scope check

Após tenant check, verificar se principal tem scope necessário:

```
required_scope = operation.required_scope()  # ex: "cache-w"
if required_scope not in principal.scopes:
    return 403
```

### 4.3 Rate limiting é **último filtro**

Após tenant + scope OK, verificar rate limit via DO:

```
if rate_limit_exceeded(tenant_id):
    return 429
```

Rate limit é per-tenant, não per-principal — tenant inteiro compartilha quota.

### 4.4 Denied actions audited

Permission denies (403, 429) **DEVEM** ser loggadas com:

- `principal_id`, `tenant_id`, `scopes` atuais
- `required_scope`, `operation`
- `timestamp`, `source_ip`
- Evento: `auth.denied.{tenant,scope,rate_limit}`

Logs de denied são importantes para detectar ataque (ex: token vazado sendo usado fora do padrão).

---

## 5. Rotation (rotação de credenciais)

### 5.1 Default lifetimes

| Tipo | Default | Max permitido |
|---|---|---|
| Clerk JWT | 1h | 1h (config Clerk) |
| PAT (dev token) | 90d | 365d |
| CI token | 180d | 365d |
| Read-only token | 365d | 730d |
| Executor cert | 30d | 90d (curto pra blast radius) |

### 5.2 Rotation process

1. User/owner cria novo token (sem revogar o antigo).
2. Período de coexistência: ≥ 7 dias.
3. Migra clients pro novo token.
4. Revoga antigo quando confirmado zero uso (via `last_used_at`).

### 5.3 Scheduled reminders

- Dashboard **DEVE** mostrar warning aos 70% do lifetime.
- Email automatizado aos 80% e 95%.
- Token bloqueado automaticamente em `expires_at` (dia zero).

### 5.4 Emergency rotation

Em caso de suspected compromise:

1. Revoke token via API ou dashboard (efeito < 1min conforme §6).
2. Auditar uso do token via `audit_log` (últimos 90 dias).
3. Criar novo token.
4. Investigar raiz (leak em log? commit acidental? vulnerability em client?).

---

## 6. Revocation (revogação imediata)

### 6.1 Semântica

Revogação **DEVE** efetivar em **≤ 60 segundos** em todas as réplicas globalmente.

### 6.2 Implementação

```
1. User clica "Revoke" no dashboard
2. API `DELETE /tokens/{id}`:
   a. D1 UPDATE pat (canonical Neon SoT per data_model.md §4.1) SET revoked_at = NOW() WHERE id = :id
   b. Broadcast a todos DOs de revocation via DO message
   c. Invalidate token em KV cache
3. Próxima request com esse token:
   a. Container busca KV (miss ou mark revoked) → D1 (confirmed revoked)
   b. Returns 401 `token_revoked`
```

### 6.3 Garantias

- **Eventual consistency com SLA de 60s**: em ≤ 60s todas instâncias negarão o token.
- **Strict consistency local**: instância que atende dashboard revogador para de aceitar imediatamente.
- **Audit trail**: revogação gera evento `auth.token.revoked` com `principal_id`, `revoked_by`, `reason` (se fornecido).

### 6.4 Mass revocation (incident response)

Em caso de data breach ou leak suspected:

1. `POST /admin/tokens/revoke-all` (admin tenant scope `admin-tokens`).
2. Todos tokens do tenant revogados em batch (D1 UPDATE WHERE tenant_id).
3. Usuários precisam re-autenticar e gerar novos tokens.
4. Event `auth.mass_revocation` criticou com severity `SEV-1`.

---

## 7. Audit trail obrigatório

### 7.1 Eventos auditados

Todos os eventos abaixo **DEVEM** gerar entry em `audit_log` (D1, imutável via schema):

| Evento | Campos | Severity |
|---|---|---|
| `auth.login.success` | principal_id, tenant_id, source_ip, user_agent | INFO |
| `auth.login.failure` | attempted_principal, tenant_id, source_ip, reason | WARN |
| `auth.token.created` | principal_id (creator), new_token_id, scopes, expires_at | INFO |
| `auth.token.revoked` | token_id, revoked_by, reason | INFO |
| `auth.token.expired` | token_id (auto, scheduled) | INFO |
| `auth.denied.tenant` | principal_id, attempted_tenant_id, operation | WARN |
| `auth.denied.scope` | principal_id, tenant_id, required_scope, operation | WARN |
| `auth.denied.rate_limit` | tenant_id, rate_limit_key | WARN |
| `auth.admin.config_changed` | admin_id, tenant_id, old_value, new_value, field | INFO |

### 7.2 Retention (tiered)

Audit log vive em 3 tiers com SLAs distintos, mas **retenção total ≥ 7 anos** para SOC 2 / ISO 27001 / LGPD Art. 16:

| Tier | Backend | Duração | Propósito |
|---|---|---|---|
| Hot | D1 / Neon | 90 dias | Query operacional rápida (Worker lookup, alert) |
| Warm | R2 `audit-<region>/` via Logpush | até 1 ano | Análise / export para SIEM |
| **Cold (archive)** | R2 Object Lock Governance Mode | **≥ 7 anos** | Compliance, forensics, legal hold |

Pipeline: hot (90d) → warm (1y) → cold (≥7y, hash-chained). Total ≥ 7 anos conforme CTRL-AUDIT-005 (`security_model.md §6.8`). Corrigido S-03/F-03 do audit Lote 3+4.

### 7.3 Immutability

- D1 schema: audit_log sem UPDATE/DELETE permitido (CHECK constraint + app-level rejection).
- R2 archive com Object Lock Governance Mode + hash-chain diário (CTRL-AUDIT-001 + PAT-AUDIT-VERIFY-001).
- Invariante canônico: INV-AUDIT-APPEND-ONLY (ver `invariant_registry.md`).

---

## 8. Tenant Isolation enforcement

### 8.1 Camadas de defesa (defense-in-depth)

1. **Camada 1 (Middleware gRPC/HTTP):** JWT/token validated → `tenant_id` extracted → attached ao request context. Todo handler recebe request context já com tenant_id.
2. **Camada 2 (Handler):** Handler explicitly references `ctx.tenant_id` em todo query (`WHERE tenant_id = :tid`).
3. **Camada 3 (Repository):** Repo layer **não aceita** queries sem filtro tenant_id (compile-time check em Rust via type-state pattern).
4. **Camada 4 (Database):** D1/Postgres com row-level security (RLS) quando suportado; ou app-level enforcement obrigatório.
5. **Camada 5 (R2):** keys always prefixed by tenant_id; sem scan cross-prefix.

### 8.2 Property test obrigatório

Test Rust `proptest`:

```rust
proptest! {
    #[test]
    fn cross_tenant_never_accessible(
        tenant_a in arb_tenant(),
        tenant_b in arb_tenant(),
        digest in arb_digest(),
    ) {
        prop_assume!(tenant_a.id != tenant_b.id);
        // Tenant A uploads blob
        server.upload(&tenant_a, &digest, random_bytes()).unwrap();
        // Tenant B tries to read
        let result = server.read(&tenant_b, &digest);
        // Must fail with 404 or 403, never return tenant_a's blob
        assert!(matches!(result, Err(AuthError::NotFound | AuthError::Forbidden)));
    }
}
```

### 8.3 TLA+ spec (**OBRIGATÓRIO** — CRITICAL invariant)

INV-TenantIsolation é CRITICAL; TLA+ model check **é obrigatório** (CTRL-FORMAL-001 em `security_model.md §6.9`), sem exceção (corrigido S-02/F-03 do audit Lote 3+4). CI falha se invariante CRITICAL não tem TLC verde. Spec vive em `specs/tla/tenant_isolation.tla`:

```tla
---- MODULE TenantIsolation ----
CONSTANTS Tenants, Blobs, Principals
VARIABLES ownership, access_attempts

Init == /\ ownership = [b \in {} |-> {}]
        /\ access_attempts = {}

AccessIsGranted(att) ==
  /\ att.principal.tenant = ownership[att.blob].tenant
  /\ att.scope \in att.principal.scopes

TenantIsolation ==
  \A att \in access_attempts:
    att.granted => AccessIsGranted(att)

Spec == Init /\ [][Next]_<<ownership, access_attempts>>

INVARIANT TenantIsolation
====
```

---

## 9. Testes de conformidade

### 9.1 Suite obrigatória antes de GA

- [ ] **Two-tenant property test** (§8.2)
- [ ] **Token lifecycle test**: create → use → revoke → use-again-should-fail
- [ ] **Scope enforcement test**: read-only token fails on write
- [ ] **JWT expiration test**: expired JWT retorna 401
- [ ] **JWKS rotation test**: Clerk JWKS key rotation não quebra validation
- [ ] **Rate limit per-tenant test**: tenant A saturado não afeta tenant B
- [ ] **Audit log immutability test**: UPDATE em audit_log row falha
- [ ] **Mass revocation test**: revoke-all em tenant X invalida todos tokens
- [ ] **Cross-tenant R2 prefix test**: tenant A não pode listar R2 `cas/{tenant_b}/*`

### 9.2 Red team review (semestral)

Ver framework §38 Red Team e Adversarial Review. Auth model **DEVE** ser alvo de pen-test semestral simulando:

- Token leak + acesso cross-tenant
- JWT forgery attempt
- Rate limit bypass
- Privilege escalation (read-only → write via request manipulation)
- Audit log tampering

---

## 10. Referências + concorrentes SOTA

### 10.1 Concorrentes que fazem bem

- **BuildBuddy** — [Authentication Guide](https://www.buildbuddy.io/docs/guide-auth/) — read-only keys, executor keys, API keys com scopes, CMEK option.
- **JFrog Artifactory** — token taxonomy com reference tokens vs identity tokens, scopes granulares.
- **Cloudflare R2** — IAM com scoped tokens (read/write por bucket).
- **GitHub PAT fine-grained** — scopes explícitos por repo + organization.

### 10.2 Padrões adotados

- RFC 6749 (OAuth 2.0) — framework conceitual (mesmo que CoreLink não use OAuth flow completo)
- JWT (RFC 7519) — format de tokens de sessão
- OWASP ASVS §2 (Authentication) e §4 (Access Control) — checklist de controles

---

## 11. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 0.1.0 | 2026-04-24 | Gustavo (via Claude Opus 4.7) | Versão inicial (Lote 4). 7 tipos de principal + 13 scopes + 5 camadas de tenant isolation + rotation/revocation com SLA 60s + audit trail completo. |

---

**Fim de AUTH-MODEL.**
