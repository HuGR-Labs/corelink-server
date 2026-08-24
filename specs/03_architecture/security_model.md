---
id: "SECURITY-MODEL"
type: "security_model"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "0.1.0"
created: "2026-04-23"
updated: "2026-04-23"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["architecture", "security", "threat-model", "stride", "controls"]
---

# Security Model — STRIDE, Trust Boundaries, Control Catalog

> **doc_status:** DRAFT
> **Versão:** 0.1.0
> **Última atualização:** 2026-04-23
> **Owner:** Gustavo Schneiter
> **Aprovador Final:** Gustavo Schneiter
> **Revisores:** ⚠️ **staffing-blocked** — promoção a `doc_status: FROZEN` bloqueada até ≥ 2 reviewers nomeados conforme roles indicados (endereça F-09 audit Lote 3+4)
> **Supersedes:** —
> **Superseded By:** —

> **Propósito:** fonte canônica (Nível 3) do modelo de segurança do CoreLink. Endereça audit v1 requisito de STRIDE completo + controle formal. Consumido por:
> - `specs/_templates/work_item.md §26` (Trust Boundaries)
> - `specs/_templates/sprint_contract.md §16` (Threat Model Delta)
> - `specs/_templates/production_readiness_review.md §11` (Security Controls)
>
> Referência normativa: todo WI/Sprint/PRR que introduza novo trust boundary, novo asset em produção, ou novo control **DEVE** declarar `inherits_from: ["SECURITY-MODEL"]` e referenciar IDs de controle (`CTRL-XXX`) em vez de redefini-los.

> **🚦 Phase boundary (F-12 audit Lote 3+4):**
> CoreLink GA inicial cobre **Fase 1 — Remote Cache** (CAS + AC + GC + REAPI cache-only).
> **Fase 2 — Remote Execution** (`execute-action`, executor identity, sandbox runtime) é **futuro** (roadmap pós-GA).
> Seções/CTRLs/SLOs/labels marcados com `(Fase 2)` ou `execute-action` referem-se a planejamento; em GA inicial podem ser omitidos do scope mínimo.



---

## Sumário

1. [Objetivos de segurança (CIA+A)](#1-objetivos-de-segurança-cia--a)
2. [Assets protegidos](#2-assets-protegidos)
3. [Trust boundaries](#3-trust-boundaries)
4. [Threat actors](#4-threat-actors)
5. [STRIDE — análise por asset × trust boundary](#5-stride--análise-por-asset--trust-boundary)
6. [Control catalog (CTRL-XXX)](#6-control-catalog-ctrl-xxx)
7. [Criptografia (algoritmos, chaves, rotação)](#7-criptografia-algoritmos-chaves-rotação)
8. [Supply chain security (SLSA L3)](#8-supply-chain-security-slsa-l3)
9. [Hardening de runtime](#9-hardening-de-runtime)
10. [Testes de segurança obrigatórios](#10-testes-de-segurança-obrigatórios)
11. [Mapeamento STRIDE → CTRL → Evidência](#11-mapeamento-stride--ctrl--evidência)
12. [Referências + concorrentes SOTA](#12-referências--concorrentes-sota)

---

## 1. Objetivos de segurança (CIA + A)

CoreLink é um **shared cache multi-tenant** com blast radius *cross-tenant* caso comprometido. Os objetivos, em ordem de prioridade estrita:

| # | Objetivo                     | Definição operacional                                                                                                                   | Invariante associada          |
|---|------------------------------|------------------------------------------------------------------------------------------------------------------------------------------|-------------------------------|
| 1 | **Integridade do artefato**  | Nenhum blob servido diverge do hash requisitado. Envenenar o cache = produzir binário alterado em CI do cliente = catastrófico.         | INV-CAS-INTEGRITY             |
| 2 | **Isolamento de tenant**     | Tenant A **NUNCA** lê/escreve/enumera blobs do tenant B; mesmo com credencial roubada, blast radius limitado ao tenant comprometido.     | INV-TenantIsolation (TLA+)    |
| 3 | **Confidencialidade at-rest/in-flight** | Blobs e metadados cifrados em repouso (AES-256-GCM via R2 SSE) e em trânsito (piso TLS 1.2, 1.3 negociado por todo cliente capaz; mTLS entre planes) — ver ADR-0072.                 | INV-CONF-AT-REST, INV-CONF-IN-FLIGHT |
| 4 | **Disponibilidade**          | Ataque DoS a um tenant não afeta SLO dos demais (bulkheading por namespace + rate limits per-tenant).                                    | INV-AVAIL-ISOLATION           |
| 5 | **Auditabilidade**           | Toda operação write/admin gera evento imutável em audit log (append-only, retention ≥ 7 anos para SOC 2).                                | INV-AUDIT-APPEND-ONLY         |

> **Prioridade 1 > Prioridade 2 > … > Prioridade 5.** Em conflito, integridade sempre vence. Exemplo: se choice for *servir blob suspeito de ser poisoned* vs *retornar 503*, **servir 503**.

---

## 2. Assets protegidos

Taxonomia de assets (referenciada por CTRLs e por `work_item §26`):

| ID           | Asset                          | Categoria          | Criticidade | Localização                      | Notas |
|--------------|--------------------------------|--------------------|-------------|-----------------------------------|-------|
| AST-BLOB     | CAS blob (bytes do artefato)  | Data               | CRITICAL    | R2 bucket `cas-<region>`         | Imutável; conteúdo pode ser código, binário compilado, ou segredo leaked pelo cliente (CVE-pattern). |
| AST-AC       | Action Cache entry             | Data               | CRITICAL    | R2 bucket `ac-<region>` + KV metadata | Envenenamento aqui ⇒ build do cliente usa resultado falso. |
| AST-META     | Metadata (ref count, size, ts) | Data               | HIGH        | D1/Neon                           | Consistência forte; fonte de verdade para GC. |
| AST-TOKEN    | PAT / CI token / service cred  | Credential         | CRITICAL    | Vault (emissão) + hash em D1 (verify) | Ver `auth_model.md §5`. |
| AST-AUDIT    | Audit log                      | Log                | HIGH        | R2 append-only `audit-<region>`  | Retention 7 anos; imutabilidade garantida por Object Lock. |
| AST-BILLING  | Billing counters (bytes, reqs) | Data               | HIGH        | D1 tabela `usage_counters`       | Integridade crítica para faturamento; adulteração = fraude. |
| AST-TENANT-KEY | Tenant-scoped derivation key (HKDF salt) | Key     | CRITICAL    | Cloudflare Secrets (KMS-backed)  | Usada para HMAC de paths cross-tenant; rotação anual. |
| AST-CODE     | Código do CoreLink (Worker+Container+Containers) | Supply chain | CRITICAL | GitHub + Cloudflare deploy | SLSA L3 provenance obrigatória. |
| AST-CONFIG   | Flags, policies, rate limits   | Config             | HIGH        | DO `config-singleton` + KV cache | Mudança audita + aprovação dual. |
| AST-TLA-MODEL | TLA+ specs checadas            | Artifact           | MEDIUM      | Repo `specs/**/*.tla`            | Evidência obrigatória para INV CRITICAL. |

> **Asset × ameaça vs. Asset × controle:** §5 mapeia STRIDE por asset; §6 catálogo de controles; §11 matriz integral.

---

## 3. Trust boundaries

Cada boundary é um **ponto onde os pressupostos de confiança mudam** — e portanto onde enforcement obrigatório.

```
┌──────────────────────────────────────────────────────────────────────────┐
│ TB-0: Internet pública ──► Cloudflare edge                               │
│   • TLS terminating (piso 1.2 — ADR-0072); rate limit IP; WAF rules      │
└──────────────────────────────────────────────────────────────────────────┘
                              │
┌──────────────────────────────────────────────────────────────────────────┐
│ TB-1: Cloudflare edge ──► Worker (control plane) / Container (data)       │
│   • Assinatura Cloudflare + mTLS binding (service binding, não público)   │
└──────────────────────────────────────────────────────────────────────────┘
                              │
┌──────────────────────────────────────────────────────────────────────────┐
│ TB-2: Worker ──► Durable Object / R2 / D1 / Neon / KV                    │
│   • Cloudflare bindings (não token); isolamento por account              │
│   • Neon via connection string em Cloudflare Secrets; pooled; mTLS       │
└──────────────────────────────────────────────────────────────────────────┘
                              │
┌──────────────────────────────────────────────────────────────────────────┐
│ TB-3: Tenant A namespace ──► Tenant B namespace                           │
│   • Prefixo de path derivado via HMAC(tenant_key, tenant_id)              │
│   • AuthZ check no Worker + segunda camada no storage (bucket policy)     │
│   • INV-TenantIsolation (TLA+ obrigatório)                                │
└──────────────────────────────────────────────────────────────────────────┘
                              │
┌──────────────────────────────────────────────────────────────────────────┐
│ TB-4: Code plane (user-submitted action) ──► Execution sandbox           │
│   • Containers ephemeral, sem egress para Internet (allowlist only)       │
│   • gVisor/Firecracker-grade isolation; CPU/mem/io limits hard            │
│   • Apenas para REAPI execute-action — NUNCA para cache read/write        │
└──────────────────────────────────────────────────────────────────────────┘
                              │
┌──────────────────────────────────────────────────────────────────────────┐
│ TB-5: Operador humano ──► Plano de controle (admin API)                   │
│   • MFA obrigatório; dual-approval para ações destructive                 │
│   • Todas as ações logadas em AST-AUDIT com ator + justificativa          │
└──────────────────────────────────────────────────────────────────────────┘
```

> **Regra:** todo WI que crie novo boundary **DEVE** documentar em §26, citar esta seção, e adicionar TLA+ check caso atravesse TB-3 ou TB-4.

---

## 4. Threat actors

Taxonomia **Kiwicon-style**; ordem = capability ascendente.

| ID    | Ator                                      | Capabilities                                                                 | Motivação típica                              | Mitigação primária                 |
|-------|-------------------------------------------|------------------------------------------------------------------------------|-----------------------------------------------|-------------------------------------|
| TA-0  | Script kiddie (Internet)                  | Scan de portas, credential stuffing, DoS baixo volume                        | Mischief, extorsão                             | CF WAF + rate limit + MFA           |
| TA-1  | Developer hostil com PAT roubado          | AuthN válido, conhece API; pode ler blobs do tenant comprometido             | Exfil de artefatos proprietários               | Scope mínimo, revogação 60s, audit  |
| TA-2  | Tenant malicioso (pagante)                | Full API access no seu namespace; pode tentar extravasar para outros         | Espionagem industrial; free-riding             | TB-3 + TLA+ INV-TenantIsolation     |
| TA-3  | CI comprometido (supply chain → cliente)  | Injeta payload em builds; pode fazer cache poisoning se não houver integridade | Sabotagem; criptominer embutido              | BLAKE3 verification no client; CAS content-addressable |
| TA-4  | Insider (colaborador HuGR)                | Acesso legítimo a planos de controle; pode ler audit logs                    | Curiosidade, retaliação, coerção                | Dual-approval; audit imutável; BYOK opcional |
| TA-5  | Supply chain attacker (dep do CoreLink)   | Contribui código malicioso para dep transitiva                               | Watering hole                                   | SLSA L3 + SBOM + pin + cargo-audit em CI |
| TA-6  | Nation-state                              | Capabilities praticamente ilimitadas; exploits 0-day reservados              | Espionagem sistemática                          | Defense in depth; BYOK; residency enforcement |

> **Threat model default:** proteger contra TA-0..TA-5 sem compromissos. TA-6 → mitigação parcial (residency + BYOK), sem claim absoluto.

---

## 5. STRIDE — análise por asset × trust boundary

Cada célula abaixo é uma **ameaça específica** com ID `THR-<STRIDE>-<NNN>`. Mitigações referenciam CTRLs do §6.

### 5.1 Spoofing (S)

| ID         | Ameaça                                                                              | Asset alvo   | Boundary | CTRLs                                  |
|------------|--------------------------------------------------------------------------------------|--------------|----------|-----------------------------------------|
| THR-S-001  | Atacante se passa por tenant legítimo via token forjado                            | AST-TOKEN    | TB-1     | CTRL-AUTH-001 (signed PAT), CTRL-AUTH-004 (HMAC) |
| THR-S-002  | Replay de request assinado com nonce reutilizado                                    | AST-TOKEN    | TB-1     | CTRL-AUTH-007 (nonce + window 60s)     |
| THR-S-003  | DNS hijack → MITM no domínio `cache.hugr.dev`                                       | AST-BLOB     | TB-0     | CTRL-NET-001 (HSTS preload), CTRL-NET-002 (CAA) |
| THR-S-004  | Service binding spoofed entre Worker e Container                                   | AST-BLOB     | TB-1     | CTRL-NET-003 (CF service binding auth) |
| THR-S-005  | Operador se passa por outro no admin API                                           | AST-CONFIG   | TB-5     | CTRL-AUTH-010 (MFA + session bind)     |

### 5.2 Tampering (T)

| ID         | Ameaça                                                                              | Asset alvo   | Boundary | CTRLs                                  |
|------------|--------------------------------------------------------------------------------------|--------------|----------|-----------------------------------------|
| THR-T-001  | **Cache poisoning** — blob alterado em repouso                                      | AST-BLOB     | TB-2     | CTRL-CAS-001 (content-addressable), CTRL-CAS-002 (BLAKE3 verify on read) |
| THR-T-002  | AC entry adulterado (OutputFiles apontam para blob controlado pelo atacante)       | AST-AC       | TB-2     | CTRL-AC-001 (Merkle verify), CTRL-AC-002 (digest assinado) |
| THR-T-003  | Metadata tamper (ref count zerado → GC deleta blob vivo)                           | AST-META     | TB-2     | CTRL-META-001 (checksum linha), CTRL-GC-001 (grace period 24h) |
| THR-T-004  | Audit log adulterado retroativamente                                                | AST-AUDIT    | TB-5     | CTRL-AUDIT-001 (R2 Object Lock + hash chain) |
| THR-T-005  | Billing counter manipulado (fraude)                                                  | AST-BILLING  | TB-2     | CTRL-BILLING-001 (append-only events + reconciliation) |
| THR-T-006  | Código CoreLink modificado no pipeline (supply chain)                              | AST-CODE     | TB-1     | CTRL-SUPPLY-001 (SLSA L3), CTRL-SUPPLY-002 (signed release) |
| THR-T-007  | TLA+ model checked ≠ model deployed                                                 | AST-TLA-MODEL| TB-1     | CTRL-FORMAL-001 (CI mandatório + evidence EVT-022) |

### 5.3 Repudiation (R)

| ID         | Ameaça                                                                              | Asset alvo   | Boundary | CTRLs                                  |
|------------|--------------------------------------------------------------------------------------|--------------|----------|-----------------------------------------|
| THR-R-001  | Tenant nega ter escrito blob depois de investigação de vazamento                   | AST-BLOB     | TB-3     | CTRL-AUDIT-002 (write log com token hash) |
| THR-R-002  | Insider nega ação admin destructive                                                 | AST-CONFIG   | TB-5     | CTRL-AUDIT-003 (MFA attestation + session recording) |
| THR-R-003  | Cloudflare nega ter processado request (billing dispute)                           | AST-BILLING  | TB-1     | CTRL-AUDIT-004 (reconciliation com CF logs) |

### 5.4 Information Disclosure (I)

| ID         | Ameaça                                                                              | Asset alvo   | Boundary | CTRLs                                  |
|------------|--------------------------------------------------------------------------------------|--------------|----------|-----------------------------------------|
| THR-I-001  | **Cross-tenant read** — Tenant A lê blob do Tenant B via path guessing             | AST-BLOB     | TB-3     | CTRL-ISO-001 (HMAC path), CTRL-ISO-002 (AuthZ check), CTRL-ISO-003 (bucket policy) |
| THR-I-002  | Side-channel timing exposing existência de blob (3-arm 404 MissReason parity per ADR-0028) | AST-BLOB     | TB-3     | CTRL-ISO-004 (constant-time 404 MissReason parity per ADR-0023; pairwise Mann-Whitney + Šidák) |
| THR-I-003  | Log contém PII/secret do tenant                                                     | AST-AUDIT    | TB-2     | CTRL-PRIV-001 (redact + schema allowlist) |
| THR-I-004  | Dedup cross-tenant revela que outro tenant tem o mesmo blob (existence oracle)     | AST-BLOB     | TB-3     | CTRL-ISO-005 (dedup tenant-local DEFAULT; cross-tenant apenas com ADR) |
| THR-I-005  | Error message expõe storage backend internals                                       | AST-CONFIG   | TB-1     | CTRL-NET-004 (error envelope sanitizer) |
| THR-I-006  | Neon connection string vazada em crash dump                                        | AST-TOKEN    | TB-2     | CTRL-CRED-001 (no-secret-in-log lint) |
| THR-I-007  | DNS exfil via subdomain query                                                       | AST-BLOB     | TB-4     | CTRL-NET-005 (Container egress allowlist) |

### 5.5 Denial of Service (D)

| ID         | Ameaça                                                                              | Asset alvo   | Boundary | CTRLs                                  |
|------------|--------------------------------------------------------------------------------------|--------------|----------|-----------------------------------------|
| THR-D-001  | Flood de requests de um tenant degrada SLO dos demais                              | AST-BLOB     | TB-2     | CTRL-RATE-001 (per-tenant token bucket) |
| THR-D-002  | Blob gigante enche quota e bloqueia writes                                         | AST-BLOB     | TB-2     | CTRL-QUOTA-001 (max blob size + tenant quota) |
| THR-D-003  | Slow-loris no Container execute-action prende slot                                 | AST-CODE     | TB-4     | CTRL-EXEC-001 (deadline hard + idle timeout) |
| THR-D-004  | Compressão bomb (zstd decompression ratio)                                         | AST-BLOB     | TB-2     | CTRL-COMP-001 (max ratio 100× enforcement) |
| THR-D-005  | R2 rate limit hit pelo próprio CoreLink (self-DoS)                                  | AST-BLOB     | TB-2     | CTRL-BACKOFF-001 (jittered exp backoff) |

### 5.6 Elevation of Privilege (E)

| ID         | Ameaça                                                                              | Asset alvo   | Boundary | CTRLs                                  |
|------------|--------------------------------------------------------------------------------------|--------------|----------|-----------------------------------------|
| THR-E-001  | PAT read-only executa ação de write via parameter tamper                          | AST-BLOB     | TB-1     | CTRL-AUTHZ-001 (scope check on verb)   |
| THR-E-002  | Escape do Container execute-action para Worker                                     | AST-CODE     | TB-4     | CTRL-EXEC-002 (gVisor/Firecracker), CTRL-EXEC-003 (seccomp profile) |
| THR-E-003  | Path traversal escreve em namespace de outro tenant                                | AST-BLOB     | TB-3     | CTRL-INPUT-001 (path canonicalize + allowlist) |
| THR-E-004  | SQL injection via metadata field                                                    | AST-META     | TB-2     | CTRL-INPUT-002 (parameterized queries + schema validation) |
| THR-E-005  | Deserialization RCE (ex: bincode / serde arbitrary)                                | AST-CODE     | TB-1     | CTRL-INPUT-003 (no untrusted deser; use canonical + explicit types) |
| THR-E-006  | Confused deputy: Worker usa binding para fazer ação em nome do tenant errado       | AST-BLOB     | TB-3     | CTRL-AUTHZ-002 (tenant_id explícito em toda call + assertion) |

---

## 6. Control catalog (CTRL-XXX)

Cada CTRL **DEVE** ter: descrição, implementação, owner (time), evidência obrigatória, teste de conformidade, frequência de revalidação.

### 6.1 Autenticação & Autorização

| ID           | Controle                             | Implementação                                             | Evidence     | Revalidação |
|--------------|--------------------------------------|-----------------------------------------------------------|--------------|-------------|
| CTRL-AUTH-001 | PAT assinado (HMAC-SHA256)          | Token = `b64(prefix.payload.sig)`; sig = HMAC(secret, payload) | EVT-002 | Anual |
| CTRL-AUTH-004 | Path HMAC por tenant                | Prefix = `b64(HMAC(tenant_key, tenant_id))[:16]`          | EVT-002 | Semestral (key rotation) |
| CTRL-AUTH-007 | Nonce + replay window               | Requests assinados válidos por ±60s; nonce em DO          | EVT-002 | Anual |
| CTRL-AUTH-010 | MFA + session binding para admin    | WebAuthn; session bound a UA+IP+PKCE                      | EVT-025 | Anual |
| CTRL-AUTHZ-001| Scope check by verb                 | Middleware valida scope antes de atingir handler          | EVT-002 | Contínuo |
| CTRL-AUTHZ-002| Explicit tenant_id + assertion      | Todo handler recebe `tenant_id` param; assertion dupla no storage | EVT-022 | Por mudança |

### 6.2 Criptografia & Integridade

| ID           | Controle                            | Implementação                                              | Evidence     | Revalidação |
|--------------|-------------------------------------|------------------------------------------------------------|--------------|-------------|
| CTRL-CAS-001 | Content-addressable naming          | Path = `<algo>/<hash>`; escrita rejeita se `hash(body) ≠ <hash>` | EVT-002 | Contínuo |
| CTRL-CAS-002 | BLAKE3 verify on read               | Client lib opt-out via header `X-Trust-Server: never` (default) | EVT-027 | Por release |
| CTRL-AC-001  | Merkle verify                       | AC entry carrega digest de raiz; client valida             | EVT-002 | Por release |
| CTRL-AC-002  | AC digest signing                   | AC digest assinado com tenant_key; verify no client        | EVT-004 (overhead) | Por release |
| CTRL-CRYPTO-001 | TLS 1.2 floor, 1.3 preferido     | CF edge config (`min_tls_version=1.2` na zona `humangr.com`); HSTS preload; CAA pin. **Não é mais 1.3-only** — ver ADR-0072 | EVT-037 (SSL Labs A+) **reexecução pendente: o scan citado é anterior à queda do piso** | Trimestral |
| CTRL-CRYPTO-002 | AES-256-GCM at rest              | R2 SSE-S3 default + envelope per-tenant (HKDF)             | EVT-013 | Trimestral |
| CTRL-CRYPTO-003 | Key rotation annual              | Automation via CF API; re-wrap envelope                    | EVT-001 | Anual |

### 6.3 Isolamento

| ID           | Controle                            | Implementação                                              | Evidence     | Revalidação |
|--------------|-------------------------------------|------------------------------------------------------------|--------------|-------------|
| CTRL-ISO-001 | HMAC tenant prefix                  | Ver CTRL-AUTH-004; lib `tenant_path::derive_prefix(tenant_id)` única; property test garante 2 tenants distintos → 2 prefixes distintos | EVT-002 (property test cross-tenant) + EVT-022 (TLA+ INV-TENANT-ISOLATION) | Semestral (alinhado com TDK rotation, ver `key_management.md §3`) |
| CTRL-ISO-002 | AuthZ check on storage call         | Worker valida tenant_id == prefix HMAC(tenant_key, caller) | EVT-022 | Contínuo |
| CTRL-ISO-003 | R2 bucket policy enforcement        | IAM policy + pre-signed URL com path fixo                  | EVT-005 | Trimestral |
| CTRL-ISO-004 | Constant-time 404 MissReason parity (per ADR-0023 + ADR-0028) | Tower middleware (`TimingPaddingLayer`) uniformiza latência across 3 arms (NotFound × CrossTenantMasked × Tombstoned per ADR-0028); pairwise Mann-Whitney U + Šidák correction; |Δmedian| ≤ 1ms gate; alert SEV-2 > 5ms 5min | EVT-025 + EVT-040 | Anual |
| CTRL-ISO-005 | Dedup tenant-local default          | Cross-tenant dedup apenas via ADR com BYOE                 | EVT-029 | Por mudança |

### 6.4 Supply Chain

| ID           | Controle                            | Implementação                                              | Evidence     | Revalidação |
|--------------|-------------------------------------|------------------------------------------------------------|--------------|-------------|
| CTRL-SUPPLY-001 | SLSA Level 3                      | GitHub Actions + provenance attestation via sigstore        | EVT-011 | Por release |
| CTRL-SUPPLY-002 | Signed release + verified deploy  | Cosign sign; CF deploy verifica assinatura                 | EVT-001 | Por release |
| CTRL-SUPPLY-003 | SBOM mandatório                    | CycloneDX gerado em build; publicado em release            | EVT-010 | Por release |
| CTRL-SUPPLY-004 | Dependency pinning + audit         | `Cargo.lock` committed; `cargo-audit` CI; deny unmaintained | EVT-001 | Diário (CI) |
| CTRL-SUPPLY-005 | No dynamic loading                 | Sem WASM carregada em runtime; sem `dlopen`                | EVT-005 | Por release |
| CTRL-SUPPLY-006 | License allowlist                  | `cargo-deny` license enforcement (MIT/Apache-2.0/BSD/ISC/MPL-2.0; deny GPL/AGPL/SSPL); CI gate | EVT-001 | Por PR + diário (CI) |
| CTRL-SUPPLY-007 | Yanked dep block                   | `cargo-deny` rejects yanked deps in `Cargo.lock`; INV-SUPPLY-NO-YANKED enforced | EVT-001 | Por PR |
| CTRL-SUPPLY-008 | Reproducible build verification    | 2-runner parallel build + SHA-256 diff; document non-determinism sources via ADR-0015 | EVT-027 | Por release |

### 6.5 Input Handling

| ID           | Controle                            | Implementação                                              | Evidence     | Revalidação |
|--------------|-------------------------------------|------------------------------------------------------------|--------------|-------------|
| CTRL-INPUT-001 | Path canonicalization             | Rejeita `..`, `\0`, UTF-8 inválido, empty, `>4KiB`         | EVT-002 | Contínuo |
| CTRL-INPUT-002 | Parameterized queries              | `sqlx::query!` macros apenas; deny raw strings              | EVT-005 | Contínuo |
| CTRL-INPUT-003 | Explicit deserialization           | `serde_with::deserialize_as` + `#[serde(deny_unknown_fields)]` | EVT-002 | Contínuo |
| CTRL-INPUT-004 | Content-Length cap                 | Max 5GiB por blob single-put; multipart > 5GiB             | EVT-002 | Contínuo |

### 6.6 Runtime Hardening

| ID           | Controle                            | Implementação                                              | Evidence     | Revalidação |
|--------------|-------------------------------------|------------------------------------------------------------|--------------|-------------|
| CTRL-EXEC-001 | Deadline hard em execute-action    | Container mata processo após deadline API-set (max 60min) | EVT-002 | Contínuo |
| CTRL-EXEC-002 | gVisor/Firecracker                 | Cloudflare Containers já provê; verificar em runbook       | EVT-017 | Trimestral |
| CTRL-EXEC-003 | seccomp strict                     | Allowlist curta de syscalls; deny all default              | EVT-005 | Por release |
| CTRL-NET-005 | Egress allowlist no Container       | Apenas `*.r2.cloudflarestorage.com` + `registries allowlist` | EVT-028 | Trimestral |

### 6.7 Rate Limiting & Quotas

| ID           | Controle                            | Implementação                                              | Evidence     | Revalidação |
|--------------|-------------------------------------|------------------------------------------------------------|--------------|-------------|
| CTRL-RATE-001 | Per-tenant token bucket            | DO `rate-limiter-<tenant>`; refill rate = plano            | EVT-024 | Mensal |
| CTRL-QUOTA-001| Storage quota per-tenant           | D1 tabela `tenant_quota`; enforcement na write path        | EVT-002 | Mensal |
| CTRL-COMP-001 | Max decompression ratio             | Zstd param `max_window_size`; aborta se ratio > 100×       | EVT-002 | Contínuo |
| CTRL-BACKOFF-001 | Exponential backoff com jitter   | Lib interna `backoff::jittered`; tests de convergência     | EVT-002 | Contínuo |

### 6.8 Auditoria & Forensics

| ID           | Controle                            | Implementação                                              | Evidence     | Revalidação |
|--------------|-------------------------------------|------------------------------------------------------------|--------------|-------------|
| CTRL-AUDIT-001 | R2 Object Lock + hash chain      | Audit log appended com hash do evento anterior; bucket em Governance Mode | EVT-001 | Trimestral |
| CTRL-AUDIT-002 | Write events ricos               | `{actor, tenant_id, token_hash, op, path_hash, timestamp, request_id, outcome}` | EVT-026 | Contínuo |
| CTRL-AUDIT-003 | MFA attestation para admin ops   | WebAuthn signature embedded in audit record                | EVT-025 | Anual |
| CTRL-AUDIT-004 | Reconciliation com CF analytics  | Diária; alert se drift > 0.1%                              | EVT-013 | Diário |
| CTRL-AUDIT-005 | Retention 7 anos (SOC 2)         | Lifecycle rule R2; test de recovery trimestral             | EVT-017 | Trimestral |

### 6.9 Formal Verification

| ID           | Controle                            | Implementação                                              | Evidence     | Revalidação |
|--------------|-------------------------------------|------------------------------------------------------------|--------------|-------------|
| CTRL-FORMAL-001 | TLA+ obrigatório para INV CRITICAL | CI falha se invariante CRITICAL não tem model check verde | EVT-022 | Por mudança |
| CTRL-FORMAL-002 | Proof artifacts versionados      | `.tla` + `.cfg` em repo; CI roda TLC; evidence anexada à PR | EVT-001 | Por PR |

> **Coverage audit (R-PREP 2026-05-15):** see `specs/_audits/sealed/2026-05-15-tla-coverage-audit.md` for the full pre-GA gap analysis (11 specs surveyed) and `specs/_audits/sealed/tla-followup-tickets.md` for the 9 remaining gap tickets. That audit closed 3 CRITICAL gaps in the auth-revocation cluster via the new `specs/tla/auth_revocation.tla`.

### 6.10 Privacy (coordenação — ver `privacy_model.md`)

| ID           | Controle                            | Implementação                                              | Evidence     | Revalidação |
|--------------|-------------------------------------|------------------------------------------------------------|--------------|-------------|
| CTRL-PRIV-001 | Log redaction + allowlist schema  | Lib `log-schema`; deny unknown fields; structured JSON     | EVT-005 | Contínuo |

### 6.11 Credentials (lifecycle de tokens & secrets — ver `auth_model.md` + `key_management.md`)

Adicionado em Lote 5.4 endereçando audit S-10. Mantém referências cruzadas com `auth_model.md §5` e `key_management.md §3`.

| ID           | Controle                            | Implementação                                              | Evidence     | Revalidação |
|--------------|-------------------------------------|------------------------------------------------------------|--------------|-------------|
| CTRL-CRED-001 | No secret in logs / dump | Lint pre-commit detecta padrões (PAT prefix, JWT, AWS keys); Worker nunca emite headers `Authorization`; crash dumps redatados via Sentry sanitizer | EVT-005 (SAST regex) + EVT-002 | Contínuo |
| CTRL-CRED-002 | PAT hashed at rest | Apenas hash (Argon2id-friendly KDF) é persistido; full token só existe em emissão (via UI) e no client | EVT-002 + EVT-022 (TLA+ secret-not-stored) | Contínuo |
| CTRL-CRED-003 | Token rotation enforcement | Tokens admin: 90d; CI tokens: 1y; user PATs: 1y default; expiry hard | EVT-001 (rotation drill log) + EVT-035 | Trimestral |
| CTRL-CRED-004 | Revocation propaga em ≤ 60s | KV `pat_invalid:<hash>` set imediato; D1 update; cache invalidation broadcast via DO | EVT-024 (load test rev), EVT-002 | Trimestral |

### 6.12 Network Perimeter

Adicionado em Lote 5.4 endereçando audit S-10. Cobre `CTRL-NET-001..004` que estavam dangling.

| ID           | Controle                            | Implementação                                              | Evidence     | Revalidação |
|--------------|-------------------------------------|------------------------------------------------------------|--------------|-------------|
| CTRL-NET-001 | HSTS preload | Domain `cache.corelink.humangr.com` em [hstspreload.org](https://hstspreload.org); header `Strict-Transport-Security: max-age=63072000; includeSubDomains; preload` | EVT-037 (SSL Labs A+) | Trimestral |
| CTRL-NET-002 | CAA record pinned | DNS CAA record permite apenas Let's Encrypt + Cloudflare-managed para emitir cert; bloqueia rogue CA | EVT-001 (DNS check) | Trimestral |
| CTRL-NET-003 | CF service binding auth | Worker → Container via service binding (não público); binding nomes em manifest; CF account-level isolation | EVT-028 (config snapshot) | Trimestral |
| CTRL-NET-004 | Error envelope sanitizer | Middleware converte panics/upstream errors em envelope `{error:{code,request_id}}` sem leak de stack trace ou DB internals | EVT-002 + EVT-005 | Contínuo |

### 6.13 Data Integrity (anti-tamper de metadata + GC)

Adicionado em Lote 5.4 endereçando audit S-10 (CTRL-META-001 + CTRL-GC-001 dangling).

| ID           | Controle                            | Implementação                                              | Evidence     | Revalidação |
|--------------|-------------------------------------|------------------------------------------------------------|--------------|-------------|
| CTRL-META-001 | Metadata row checksum | Cada row em `blob_meta` carrega `sha256(canonical_form(row))` em coluna; trigger D1 verifica antes de UPDATE; drift = alerta + freeze | EVT-002 + EVT-022 (TLA+ INV-GC-003) | Trimestral |
| CTRL-GC-001 | GC grace period 72h + soft-delete | Sweep não deleta blob com `last_referenced_at < now - 72h` E `mark_started_at`-aware (ver `remote_cache_product_profile.md §9.3`) | EVT-022 (TLA+ INV-GC-001) + EVT-002 + EVT-023 (chaos GC race) | Por mudança em algoritmo GC |
| CTRL-GC-002 | Reconcile diário refcount | Job `gc-reconcile-worker` recomputa refcount de eventos vs `blob_meta.refcount`; alert se drift > 0.1% | EVT-013 (drift dashboard) + EVT-001 | Diário |

### 6.14 Billing Integrity

Adicionado em Lote 5.11 (cross-ref validator) endereçando CTRL-BILLING-001 dangling em THR-T-005.

| ID           | Controle                            | Implementação                                              | Evidence     | Revalidação |
|--------------|-------------------------------------|------------------------------------------------------------|--------------|-------------|
| CTRL-BILLING-001 | Append-only events + reconciliation diária | `usage_events` table append-only (CHECK constraint UPDATE/DELETE rejeitado); `usage_counter` agregado é derivado de eventos; reconcile diário detecta drift | EVT-002 (test reconcile) + EVT-013 (drift dashboard) + EVT-001 | Diário |
| CTRL-BILLING-002 | Idempotency em billing API | Stripe API calls usam Idempotency-Key (PAT-IDEMPOTENCY-001); evita double-charge em retry | EVT-002 | Contínuo |

---

## 7. Criptografia (algoritmos, chaves, rotação)

### 7.1 Algoritmos aprovados

| Uso                         | Algoritmo                        | Tamanho/parametro              | Biblioteca (Rust)      |
|-----------------------------|----------------------------------|--------------------------------|-------------------------|
| Hash de conteúdo primário   | BLAKE3                           | 32 bytes                       | `blake3 = "1"`          |
| Hash fallback (REAPI legacy) | SHA-256                         | 32 bytes                       | `sha2`                  |
| HMAC (path, nonces)         | HMAC-SHA256                      | 32 bytes key                   | `hmac`                  |
| Key derivation              | HKDF-SHA256                      | info = "corelink/v1/<ctx>"    | `hkdf`                  |
| Cifra simétrica at-rest     | AES-256-GCM                      | 96-bit nonce; random            | R2 SSE-S3               |
| TLS                         | Piso 1.2, 1.3 negociado (ADR-0072) | AES-256-GCM / ChaCha20-Poly1305 | CF edge (config)        |
| Assinatura de release       | Ed25519 via sigstore/cosign      | — | `cosign`                |
| Proof of possession (admin) | WebAuthn (Ed25519 / ES256)       | —                              | `webauthn-rs`           |

> **Banidos:** MD5, SHA-1, DES/3DES, RC4, ECB, RSA < 2048, crypto artesanal. Qualquer uso requer ADR com justificativa *e* waiver expiração ≤ 90 dias.

### 7.2 Chaves: hierarquia e rotação

```
Root KMS (Cloudflare Workers Secrets, HSM-backed)
  └─ Tenant Derivation Key (TDK) — 1 por tenant
       ├─ Path-HMAC key            (HKDF info = "corelink/v1/path")              → CTRL-AUTH-004
       ├─ AC-Signature key         (HKDF info = "corelink/v1/ac-sig")            → CTRL-AC-002
       ├─ Envelope key             (HKDF info = "corelink/v1/envelope")          → CTRL-CRYPTO-002
       ├─ DSR receipt JWS key      (HKDF info = "corelink/v1/dsr-receipt")       → CTRL-PRIV-DSR-RECEIPT (S-11)
       ├─ Consent HMAC key         (HKDF info = "corelink/v1/consent-hmac")      → CTRL-PRIV-CONSENT-003 (S-11)
       └─ Erasure salt             (HKDF info = "corelink/v1/erasure-salt")      → CTRL-PRIV-ERASE-PSEUDO (S-11; pseudonymize subject_id em audit retained)
  └─ Audit chain key               (HKDF info = "corelink/v1/audit-chain")        → CTRL-AUDIT-001
  └─ Audit pseudonym key           (HKDF info = "corelink/v1/audit-pseudonym")    → CTRL-PRIV-014 + CTRL-AUDIT-001 (S-11; subject_id → erased_<hash> em audit log post-DSR)
  └─ DKIM broadcast key            (HKDF info = "corelink/v1/dkim-broadcast")     → CTRL-PRIV-SUBPROCESSOR (S-11; sub-processor 30d notice DKIM-signed)
  └─ Release signing key (Ed25519, offline-HSM)                                  → CTRL-SUPPLY-002
```

| Key                | Lifetime | Rotation trigger                           | Re-wrap necessário?             |
|--------------------|----------|--------------------------------------------|---------------------------------|
| Root               | 5 anos   | Calendário / incidente                      | Sim (todos TDKs)                |
| TDK                | 1 ano    | Calendário / suspeita de vazamento          | Sim (envelope blobs do tenant)  |
| Release signing    | 2 anos   | Calendário / offboarding                    | —                               |
| Audit chain        | 1 ano    | Calendário                                  | Não (append forward only)       |

### 7.3 Break-glass & incident

- **Key compromise confirmado:** inicia runbook `RB-KEY-COMPROMISE` (`specs/05_quality/runbooks/RB-KEY-COMPROMISE.md`); rotaciona imediatamente; invalida tokens emitidos com a chave vazada; audit log marcado com `integrity_hold=true`.
- **HSM unavailable:** modo read-only automático; writes retornam 503; alerta PagerDuty P0.

---

## 8. Supply chain security (SLSA L3)

### 8.1 Build provenance

- Build em **GitHub Actions runners hospedados GitHub** (não self-hosted, para evitar TA-4 comprometer runner).
- Provenance attestation via **sigstore/cosign** assinando `subject = { name, digest }` com **GitHub OIDC → Fulcio**.
- Verificação no deploy: CF Worker deploy roda `cosign verify-blob` contra transparency log.

### 8.2 SBOM

- Formato canônico (ADR-0014): **CycloneDX 1.5+ JSON** (preferido) via `cargo-cyclonedx`; **SPDX 2.3+ JSON/YAML** também aceito quando stakeholder externo exigir (converter via `cyclonedx-cli convert`). Framework `§35.7 EVT-010` normativo.
- Gerado em CI em todo build de release.
- Assinado com cosign (CTRL-SUPPLY-002).
- Publicado como release asset *e* enviado para Dependency-Track (self-hosted).
- PRR **BLOCKED** se SBOM ausente.

### 8.3 Dependency hygiene

| Checagem            | Tool            | Frequência   | Threshold                    |
|---------------------|-----------------|--------------|-------------------------------|
| Known CVE           | `cargo-audit`   | Daily + PR   | Zero HIGH/CRITICAL            |
| License compliance  | `cargo-deny`    | PR           | Apenas MIT/Apache/BSD/ISC/MPL-2.0 |
| Unmaintained        | `cargo-audit`   | Daily        | Warn; PR-block se CRITICAL path |
| Yanked              | `cargo-audit`   | Daily        | Block                         |
| Typosquatting       | Manual (review) | PR           | Heurística + sign-off         |

### 8.4 Reproducible builds

- **Goal state:** Binários reproducíveis bit-a-bit.
- **Current gap:** Rust ainda tem sources de non-determinism (timestamps, paths embedded). Abordagem interim: comparar hashes entre 2 runners independentes; alertar se diverge.
- Owner: Build Engineer.

---

## 9. Hardening de runtime

### 9.1 Worker

- Sem `eval` / `new Function` / dynamic import de URL externa.
- `Content-Security-Policy: default-src 'none'` em todas as respostas HTML (admin UI).
- Headers: `Strict-Transport-Security`, `X-Content-Type-Options`, `Referrer-Policy: no-referrer`.
- Panics → 500 com body `{"error":"internal"}` (sanitizado); detalhe vai pro log estruturado.

### 9.2 Container (execute-action)

- Filesystem: `/tmp` tmpfs isolado; `/sandbox` read-only mount de inputs; sem `/etc/passwd` válido para uid arbitrário.
- Network: egress via proxy CF com allowlist por tenant (default deny).
- Capabilities: drop ALL; adicionar individualmente se ação requer.
- Syscalls: seccomp strict (allowlist ≈ 80 syscalls).
- Orçamento: CPU shares, RSS cap, I/O throttle; kill em deadline hard.

### 9.3 Config management

- Mudanças em policies/feature flags via PR; CI aplica diff em DO `config-singleton`.
- Dual-approval obrigatório para: rate limits, quotas, egress allowlist, retention, crypto params.
- Rollback automático se error rate > 2× baseline por 10 min após deploy.

---

## 10. Testes de segurança obrigatórios

| Tipo                   | Gatilho                  | Evidence                      | Ownership           |
|------------------------|--------------------------|-------------------------------|---------------------|
| SAST (semgrep + clippy -D warnings) | Todo PR     | EVT-005                 | Dev (self)          |
| Dependency audit       | Daily CI + PR            | EVT-001                    | Security Lead       |
| Secret scanning        | Todo PR (gitleaks)       | EVT-001                    | Security Lead       |
| Fuzz testing (parsers) | Nightly; 1h per target   | EVT-008               | Dev do componente   |
| Integration tests with TLA+ link | PR que toca INV CRITICAL | EVT-022 | Architect           |
| Pentest externo        | Anual + pós mudança arq. | EVT-025            | Security Lead + 3P  |
| Red team exercise      | Anual                    | EVT-019 (exercise) | Security Lead + 3P |
| Chaos engineering      | Semanal em staging       | EVT-023              | SRE                 |
| Bug bounty             | Contínuo (HackerOne)     | EVT-030         | Security Lead       |

> **Threshold para freeze:** qualquer finding HIGH/CRITICAL bloqueia release até fix + post-mortem.

---

## 11. Mapeamento STRIDE → CTRL → Evidência

Matriz compacta (completa em `_audits/matrix-stride-ctrl.csv`):

| Category | THR count | CTRLs primários                                               | Evidência "must-have" em PRR |
|----------|-----------|---------------------------------------------------------------|-------------------------------|
| S        | 5         | CTRL-AUTH-001, -004, -007, -010; CTRL-NET-001, -002, -003     | EVT-025            |
| T        | 7         | CTRL-CAS-001, -002; CTRL-AC-001, -002; CTRL-AUDIT-001; CTRL-SUPPLY-001 | EVT-022 + EVT-011 |
| R        | 3         | CTRL-AUDIT-002, -003, -004                                    | EVT-026         |
| I        | 7         | CTRL-ISO-001..005; CTRL-PRIV-001; CTRL-NET-004                | EVT-022 (INV-TenantIsolation) + EVT-025 |
| D        | 5         | CTRL-RATE-001; CTRL-QUOTA-001; CTRL-COMP-001; CTRL-BACKOFF-001; CTRL-EXEC-001 | EVT-024 + EVT-023 |
| E        | 6         | CTRL-AUTHZ-001, -002; CTRL-EXEC-002, -003; CTRL-INPUT-001..004 | EVT-025 + EVT-005 |

**Rule of thumb:** se um WI toca asset X e não aplica o CTRL listado em X na §6, requer **waiver com compensating control** (ver `waiver.md` template).

---

## 12. Referências + concorrentes SOTA

### 12.1 Frameworks & Standards

- **STRIDE** — Swiderski & Snyder, *Threat Modeling* (Microsoft Press 2004).
- **LINDDUN** — KU Leuven; usado em conjunção (ver `privacy_model.md`).
- **SLSA** v1.0 — https://slsa.dev/
- **OWASP ASVS 4.0** — application security verification standard.
- **NIST SP 800-207** — Zero Trust Architecture.
- **SOC 2 Trust Services Criteria** (2017, revisão 2022).
- **ISO/IEC 27001:2022** — ISMS requisitos.

### 12.2 Cloudflare-specific

- Cloudflare Workers Security model.
- R2 encryption at rest (SSE-S3 / SSE-C / customer-supplied).
- Containers isolation (gVisor under the hood; verificar no runbook).

### 12.3 Concorrentes (como fazem)

- **NativeLink** — foca em CAS integrity; menos ênfase em multi-tenant isolation (monotenant self-hosted é comum).
- **BuildBuddy Enterprise** — SSO/RBAC robusto; audit log; SOC 2 Type II; mTLS. Benchmark principal.
- **JFrog Artifactory** — maduro em compliance (SOC 2, ISO 27001); supply chain via JFrog Xray.
- **bazel-remote** — sem opinião de segurança (trust boundary = operador do cluster).

### 12.4 Ataques públicos relevantes (aprendizado)

- **SolarWinds (2020)** — motivação para SLSA L3 + reproducible builds.
- **Codecov bash-uploader (2021)** — motivação para signed release + signature verify at deploy.
- **npm `event-stream` (2018)** — motivação para dependency audit + pinning.
- **Log4Shell (2021)** — motivação para SBOM + rápida triagem via Dependency-Track.

---

**Fim de SECURITY-MODEL.** Mudanças nesta matriz afetam **todo WI/Sprint/PRR** que herda; qualquer alteração **DEVE** passar por ADR + dual-approval (Security Lead + Architect) e bump minor version mínimo.
