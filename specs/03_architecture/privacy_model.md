---
id: "PRIVACY-MODEL"
type: "privacy_model"
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
tags: ["architecture", "privacy", "linddun", "lgpd", "gdpr"]
---

# Privacy Model — LINDDUN, Data Subject Rights, Residency

> **doc_status:** DRAFT
> **Versão:** 0.1.0
> **Última atualização:** 2026-04-23
> **Owner:** Gustavo Schneiter
> **Aprovador Final:** Gustavo Schneiter
> **Revisores:** *(a definir — Privacy Officer, Legal, Security Lead)*
> **Supersedes:** —
> **Superseded By:** —

> **Propósito:** fonte canônica (Nível 3) do modelo de privacidade. Complementa `security_model.md` (STRIDE) com análise LINDDUN (privacy threats) e operacionaliza LGPD/GDPR. Consumido por:
> - `specs/_templates/work_item.md §26` (privacy threat model delta)
> - `specs/_templates/sprint_contract.md §16` (privacy sprint delta)
> - `specs/_templates/production_readiness_review.md §12` (privacy controls in production)
>
> Regra: qualquer WI que processe dado pessoal, atravesse fronteira jurídica, ou afete direitos do titular **DEVE** declarar `inherits_from: ["PRIVACY-MODEL"]` e referenciar CTRL-PRIV-XXX.

---

## Sumário

1. [Escopo + princípios privacy-by-design](#1-escopo--princípios-privacy-by-design)
2. [Categorias de dado e classificação](#2-categorias-de-dado-e-classificação)
3. [Data map — fluxo e armazenamento](#3-data-map--fluxo-e-armazenamento)
4. [LINDDUN — análise de ameaças](#4-linddun--análise-de-ameaças)
5. [Controles (CTRL-PRIV-XXX)](#5-controles-ctrl-priv-xxx)
6. [Direitos do titular (DSRs) — SLAs](#6-direitos-do-titular-dsrs--slas)
7. [Data residency + transferências internacionais](#7-data-residency--transferências-internacionais)
8. [Retenção e erasure](#8-retenção-e-erasure)
9. [Consentimento, base legal, legitimate interest](#9-consentimento-base-legal-legitimate-interest)
10. [Papel de controlador × operador (LGPD Art. 5 / GDPR Art. 4)](#10-papel-de-controlador--operador-lgpd-art-5--gdpr-art-4)
11. [Incident response — data breach](#11-incident-response--data-breach)
12. [Testes de privacy](#12-testes-de-privacy)
13. [Referências](#13-referências)

---

## 1. Escopo + princípios privacy-by-design

CoreLink processa três classes de dado que requerem atenção:

1. **Dados de conta do usuário** (emails, nomes de dev que operam a conta). Controlador = HuGR.
2. **Dados de telemetria** (IPs, user-agents, request_ids). Controlador = HuGR (pseudonimizado).
3. **Conteúdo de artefato** (blobs uploaded pelo tenant). Controlador = tenant; HuGR = **operador**.

> **Distinção crítica:** quando tenant sobe um blob que contém PII (ex.: dataset de ML com dados de clientes), HuGR é operador. As obrigações LGPD/GDPR recaem no tenant, mas HuGR assume compromissos contratuais (DPA) para dar suporte a DSRs e breach notification.

**Princípios (mandatórios, RFC 2119 **DEVE**):**

1. **Minimização**: coletar apenas o necessário; cada campo tem justificativa em data map.
2. **Purpose limitation**: cada campo tem `purpose_tag`; uso fora do propósito requer ADR + reconsent.
3. **Pseudonimização by default**: `tenant_id` e `user_id` são UUIDv7; nome comercial vive só no billing plane.
4. **Encryption everywhere**: at-rest (CTRL-CRYPTO-002) e in-flight (CTRL-CRYPTO-001).
5. **Data residency honest**: documentar claramente onde o dado reside e onde trafega; sem surpresas.
6. **User control**: DSR self-service via API + UI; SLA publicado.
7. **Auditability**: todo acesso a dado pessoal gera evento (CTRL-AUDIT-002).

---

## 2. Categorias de dado e classificação

| Categoria                         | Classificação LGPD/GDPR | Exemplos                                     | Sensibilidade | Retention default         |
|-----------------------------------|-------------------------|----------------------------------------------|---------------|----------------------------|
| Dados de conta (billing)          | Dado pessoal            | Email, nome, endereço de cobrança            | HIGH          | Enquanto conta ativa + 5y (fiscal) |
| Credenciais                       | Dado pessoal            | Password hash, PAT hashes, WebAuthn pubkey   | CRITICAL      | Enquanto válido + 90d audit |
| Telemetria técnica                | Pseudonimizada          | IP, UA, request_id                           | MEDIUM        | 30d (IP truncado); UA enum |
| Conteúdo de artefato (blob body)  | **Variável** (depende do tenant) | Pode conter código, dados, PII          | TENANT-DEFINED | Conforme política do tenant |
| Metadata de artefato              | Pseudonimizada           | Digest, size, ts, tenant_id (UUID)           | LOW           | Mesmo que o blob           |
| Audit log                         | Pseudonimizada           | Actor hash, op, digest hex8                  | HIGH          | 7y (SOC 2)                 |
| DSR tickets                       | Dado pessoal             | Nome do solicitante, natureza do pedido      | HIGH          | 5y após resolução (legal)  |
| **Dados sensíveis** (Art. 5, II LGPD) | **NÃO PROCESSAMOS intencionalmente** | Saúde, religião, biometria (salvo WebAuthn pubkey)   | CRITICAL      | N/A                        |

> **Regra:** se uma nova categoria surgir, ADR obrigatório *antes* da coleta começar.

---

## 3. Data map — fluxo e armazenamento

```
┌─────────────────┐      ┌──────────────┐       ┌───────────────┐
│ User (dev)      │ HTTPS│ CF Edge       │ signed│ Worker-CP     │
│  - email         │─────►│ (region near)│──────►│ authn/authz   │
│  - name          │      │ TLS 1.3       │       │ audit event  │
│  - billing addr  │      └──────────────┘       └──────┬────────┘
└─────────────────┘                                      │
                                                         │ tenant_id = UUID
                                                         ▼
                                    ┌─────────────────────────────────┐
                                    │ Storage plane (by region)       │
                                    │  R2: bucket `cas-<region>`      │
                                    │  R2: bucket `ac-<region>`       │
                                    │  R2: bucket `audit-<region>`    │
                                    │  D1: operational metadata        │
                                    │  Neon: control-plane + billing  │
                                    │  KV: pre-signed caches           │
                                    │  DO: rate/quota/config singles   │
                                    └─────────────────────────────────┘
                                                         │
                                                         ▼
                                    ┌─────────────────────────────────┐
                                    │ Observability plane              │
                                    │  Grafana Cloud (EU or US?)       │
                                    │  Logpush → R2 (same region)       │
                                    └─────────────────────────────────┘
```

### 3.1 Inventário por backend (ligar com `storage_semantics_matrix.md`)

| Backend      | Contém                             | Dado pessoal? | Local físico                                | Cross-border? |
|--------------|------------------------------------|---------------|---------------------------------------------|----------------|
| R2           | Blobs, AC, audit events            | Sim (potencialmente, pelo conteúdo) | Região selecionada pelo tenant | Não (if pinned) |
| D1           | Operational metadata                | Sim (PII pseudo via UUID)      | Região                       | Não            |
| Neon         | Billing, accounts                   | Sim (email, nome)                | US or EU (configurável)       | Possivelmente (depende)      |
| KV           | Caches curtos, nonces                | Não (hashes)                   | Global (replicação CF)        | Sim (design)   |
| DO           | Singletons (config, rate)            | Pseudonimizada                  | Região (DO migrations ciente) | Se migrar      |
| Grafana Cloud| Metrics, logs (logpush)              | Pseudonimizada                  | Região escolhida              | Possivelmente  |

> **KV é global por design** → não armazenar nada que exija residência estrita. Nonce/cache curtíssimo ok; email/PAT NÃO.

---

## 4. LINDDUN — análise de ameaças

LINDDUN é o análogo privacy do STRIDE. Cada letra = categoria de threat.

### 4.1 L — Linkability

| ID          | Threat                                                             | Asset            | CTRLs                       |
|-------------|---------------------------------------------------------------------|------------------|------------------------------|
| PRV-L-001   | Correlacionar atividade de 1 dev entre contas via timing/size        | Telemetria       | CTRL-PRIV-010 (jitter + bucketing) |
| PRV-L-002   | Cross-tenant link via dedup oracle                                  | CAS              | CTRL-ISO-005 (dedup tenant-local default) |
| PRV-L-003   | Link dev + email + IP via logs                                      | Logs             | CTRL-PRIV-001 (redact)       |

### 4.2 I — Identifiability

| ID          | Threat                                                             | Asset            | CTRLs                       |
|-------------|---------------------------------------------------------------------|------------------|------------------------------|
| PRV-I-001   | Reverse engineer `tenant_id` → nome comercial                        | Metrics/logs     | CTRL-PRIV-011 (tenant_id = UUIDv7, não sequential) |
| PRV-I-002   | Reidentificar usuário via user-agent + padrão de uso                | Telemetria       | CTRL-PRIV-012 (UA enum top-20) |
| PRV-I-003   | Digest de blob contendo PII é indexável                             | CAS              | CTRL-PRIV-013 (digest opcional encrypted envelope com per-tenant key) |

### 4.3 N — Non-repudiation (privacy-adverse)

| ID          | Threat                                                             | Asset            | CTRLs                       |
|-------------|---------------------------------------------------------------------|------------------|------------------------------|
| PRV-N-001   | Usuário não consegue "desassinar" ação (excesso de audit = sobrecoleta) | Audit      | CTRL-PRIV-014 (minimização em audit: só actor_hash, não nome) |

> Note: a maioria dos frameworks trata Non-repudiation como *positivo* (STRIDE-R). Em LINDDUN, excess audit vira threat para o *usuário*, não para o sistema. Balancear com §10 security_model.

### 4.4 D — Detectability

| ID          | Threat                                                             | Asset            | CTRLs                       |
|-------------|---------------------------------------------------------------------|------------------|------------------------------|
| PRV-D-001   | Existência de conta detectável via enumeration                       | Auth             | CTRL-PRIV-015 (constant-time signup response) |
| PRV-D-002   | Existência de blob detectável via side-channel                       | CAS              | CTRL-ISO-004 (constant-time 404 vs 403) |

### 4.5 D2 — Disclosure of information

| ID          | Threat                                                             | Asset            | CTRLs                       |
|-------------|---------------------------------------------------------------------|------------------|------------------------------|
| PRV-D2-001  | Tenant A lê blob contendo PII do Tenant B                           | CAS              | CTRL-ISO-001..005            |
| PRV-D2-002  | Log inclui email em crash dump                                       | Logs             | CTRL-PRIV-001                |
| PRV-D2-003  | Support tool permite SRE ler blob do tenant sem audit rigoroso      | CAS              | CTRL-PRIV-016 (tenant consent required + audit + MFA) |

### 4.6 U — Unawareness

| ID          | Threat                                                             | Asset            | CTRLs                       |
|-------------|---------------------------------------------------------------------|------------------|------------------------------|
| PRV-U-001   | Usuário não sabe onde os dados residem                             | Residency        | CTRL-PRIV-020 (data map público + badge na UI) |
| PRV-U-002   | Usuário não sabe quais sub-processadores HuGR usa                  | DPA              | CTRL-PRIV-021 (lista pública + notificação de mudança) |
| PRV-U-003   | Usuário não sabe como exercer DSR                                  | UX               | CTRL-PRIV-022 (UI dedicada + API documentada) |

### 4.7 N2 — Non-compliance

| ID          | Threat                                                             | Asset            | CTRLs                       |
|-------------|---------------------------------------------------------------------|------------------|------------------------------|
| PRV-N2-001  | Reter dado após DSR-erasure                                          | Storage           | CTRL-PRIV-030 (pipeline erasure com SLA + evidence) |
| PRV-N2-002  | Transferir dado para fora da região sem SCC/Padrão                  | Residência        | CTRL-PRIV-031 (residency pinning + diplomatic block) |
| PRV-N2-003  | Falhar breach notification (GDPR 72h)                                | Breach            | CTRL-PRIV-032 (runbook + oncall + legal path) |

---

## 5. Controles (CTRL-PRIV-XXX)

Esta seção **estende** o control catalog de `security_model.md §6` com controles privacy-específicos.

### 5.1 Minimização e classificação

| ID              | Controle                                  | Implementação                                            | Revalidação |
|------------------|-------------------------------------------|-----------------------------------------------------------|-------------|
| CTRL-PRIV-001   | Log redaction + allowlist schema         | Lib `log-schema`; deny unknown fields                     | Contínuo   |
| CTRL-PRIV-002   | Data classification tags                  | Toda tabela/campo tem `@classification=...`                | Trimestral |
| CTRL-PRIV-003   | Purpose limitation tag                    | Toda coleta tem `purpose_tag ∈ enum`                       | Semestral |
| CTRL-PRIV-004   | DLP scan de R2 (opcional)                  | Job trimestral varre logs por padrões PII (opt-in)         | Trimestral |

### 5.2 Pseudonimização & anonimização

| ID              | Controle                                  | Implementação                                            | Revalidação |
|------------------|-------------------------------------------|-----------------------------------------------------------|-------------|
| CTRL-PRIV-010   | Jitter + bucketing em métricas            | Histograms com buckets fixos; latência medida pp10ms      | Contínuo   |
| CTRL-PRIV-011   | UUIDv7 para identificadores internos      | `uuid::v7`; nunca sequential                              | Contínuo   |
| CTRL-PRIV-012   | UA top-20 enum                            | Classificador pré-deploy; overflow → `other`              | Mensal    |
| CTRL-PRIV-013   | Envelope encryption per-tenant para CAS   | HKDF derive per-tenant key; R2 SSE-C opcional             | Anual (key rot) |

### 5.3 User control

| ID              | Controle                                  | Implementação                                            | Revalidação |
|------------------|-------------------------------------------|-----------------------------------------------------------|-------------|
| CTRL-PRIV-020   | Data map público                          | Página `/privacy/data-map`; versionada                   | Semestral |
| CTRL-PRIV-021   | Sub-processor list + notificação         | `/privacy/sub-processors`; email aviso ≥ 30d antes      | Por mudança |
| CTRL-PRIV-022   | DSR self-service                          | API + UI `/privacy/requests`                              | Trimestral |

### 5.4 Erasure & retention

| ID              | Controle                                  | Implementação                                            | Revalidação |
|------------------|-------------------------------------------|-----------------------------------------------------------|-------------|
| CTRL-PRIV-030   | Erasure pipeline                          | `dsr-erasure-worker`; propaga para todos backends (§8)    | Por execução |
| CTRL-PRIV-031   | Residency pinning                         | Bucket R2 com locationHint; DO com placement policy        | Trimestral |
| CTRL-PRIV-032   | Breach notification runbook               | `RB-BREACH-NOTIF`; dry-run semestral                       | Semestral |
| CTRL-PRIV-033   | Legal hold override                       | Flag `legal_hold=true` preserva mesmo DSR; audit + approval | Por evento |

### 5.5 Suporte a operações

| ID              | Controle                                  | Implementação                                            | Revalidação |
|------------------|-------------------------------------------|-----------------------------------------------------------|-------------|
| CTRL-PRIV-015   | Constant-time signup response             | Middleware delay padroniza 200ms ±20ms                    | Anual    |
| CTRL-PRIV-016   | Support read access requer consent         | Consent UI + MFA + time-boxed (15min) + audit rico         | Por evento |

---

## 6. Direitos do titular (DSRs) — SLAs

### 6.1 Direitos suportados

| Direito (LGPD / GDPR)                 | Suporte    | SLA         | Evidência                       | Owner             |
|---------------------------------------|-----------|--------------|----------------------------------|-------------------|
| Confirmação de tratamento (LGPD 18 I / GDPR 15) | Self-service | 5 dias úteis (SLO 95%) | EVT-001 (DSR) | Privacy Officer  |
| Acesso (LGPD 18 II / GDPR 15)                  | Self-service | 15 dias úteis          | EVT-001 (DSR) | Privacy Officer  |
| Correção (LGPD 18 III / GDPR 16)               | Self-service | 5 dias úteis           | EVT-001 (DSR) | Privacy Officer  |
| Anonimização/bloqueio/eliminação (LGPD 18 IV / GDPR 17) | Self-service | 30 dias             | EVT-017 | Privacy Officer  |
| Portabilidade (LGPD 18 V / GDPR 20)            | Self-service | 15 dias úteis          | EVT-001       | Privacy Officer  |
| Revogação de consentimento (LGPD 18 VI / GDPR 7) | Self-service | Imediato             | EVT-001       | Privacy Officer  |
| Oposição (GDPR 21)                              | Manual (email) | 15 dias úteis       | EVT-001       | Privacy Officer  |

### 6.2 Pipeline de DSR (erasure exemplo)

```
1. Usuário solicita via UI/API → DSR ticket criado (D1 table `dsr_tickets`)
2. Verificar identidade (MFA re-auth + attestation)  → audit event `dsr.verified.v1`
3. Job enfileira `dsr-erasure-worker`                → audit event `dsr.queued.v1`
4. Worker propaga:
   a. D1: delete de accounts, DSR ticket preservado
   b. Neon: delete billing detail (excepto dados fiscais, legal hold 5y)
   c. R2 (audit): NÃO deletar (legal hold 7y); marcar como `subject_erased` em index
   d. R2 (CAS): depende se blob é só do tenant. Se shared: não deletar; se dedicated: propagar tombstone + GC
   e. KV: invalidate
   f. Grafana/Loki: apply log deletion API (Loki `/loki/api/v1/delete`)
5. Evento final `dsr.completed.v1` com hash dos steps                      → SLO counter
6. Notificação ao usuário (email + in-app)
```

### 6.3 Casos especiais

- **Dado em audit log após erasure:** mantido (legal hold), mas pseudonimizado — `subject_id` substituído por `erased_<hash>`.
- **Dado em backups:** rotação natural expira em 30d; durante o período, backup marcado com `erasure_pending` para aplicar replay de tombstone em restore.
- **Dado solicitado por tenant pra outro tenant:** NÃO atendido (não há vínculo; HuGR é operador do solicitante).

---

## 7. Data residency + transferências internacionais

### 7.1 Regiões suportadas

| Código    | Local                        | Legal framework relevante       | Estado no roadmap   |
|-----------|------------------------------|----------------------------------|---------------------|
| `wnam`    | Western North America (CA)   | PIPEDA, CCPA                     | Fase 1              |
| `enam`    | Eastern North America (US)   | CCPA, HIPAA opcional              | Fase 1              |
| `weur`    | Western Europe (NL/DE)        | GDPR                              | Fase 1              |
| `sam`     | South America (BR)           | LGPD                              | Fase 1 (cliente-âncora HuGR) |
| `apac`    | Asia-Pacific (SG)            | PDPA SG, APPI JP                  | Fase 2              |
| `afr`     | Africa (ZA)                  | POPIA                             | Fase 3              |

### 7.2 Pinning por tenant

- No signup, tenant **DEVE** escolher região primária.
- CAS + AC + D1 + audit ficam pinned.
- Metadata global (rate limits, flags) pode replicar cross-region (pseudonimizado).
- Mudança de região = pedido formal + migração + cooldown 30d.

### 7.3 Transferências internacionais

- **Default: sem transferências.** Se fluxo exige (ex: billing via Neon US quando tenant é EU) → SCC (Standard Contractual Clauses) assinadas.
- **Sub-processadores:** lista pública com região e SCC ratus. Cloudflare, Neon, Grafana Labs, Stripe, etc.
- **Schrems II compliance:** para tenants EU, apresentar TIA (Transfer Impact Assessment) sob demanda.

### 7.4 Exceções controladas

- Tenants `enterprise` podem solicitar **BYOK (bring-your-own-key)** e **residência dual** (dois regions) com SLA de replicação.
- Exceção exige ADR + assinatura de DPA customizada + revisão do Legal.

---

## 8. Retenção e erasure

### 8.1 Retention table (concreta)

| Categoria                      | Default retention | Hard ceiling (legal hold)    | Automation                  |
|--------------------------------|--------------------|------------------------------|-----------------------------|
| Conta ativa                    | Enquanto ativo    | —                             | Background check diário    |
| Conta desativada               | 30d grace          | 5y fiscal (billing)           | Job `account-expiry-worker` |
| CAS blobs (por tenant)         | Política do tenant | 7y (legal hold opcional)      | GC conforme ref count       |
| AC entries                     | 90d default        | 7y legal                      | GC                           |
| Audit log                      | 7y (SOC 2)         | —                             | R2 Object Lock              |
| Logs ops                       | 90d warm / 400d cold | 7y legal                    | Logpush lifecycle           |
| Traces                         | 30d                | —                             | Tempo retention             |
| Métricas high-res              | 30d                | —                             | Mimir                        |
| Métricas downsampled           | 400d               | —                             | Mimir                        |
| DSR tickets                    | 5y pós resolução    | —                             | Legal retention              |

### 8.2 Erasure automation

- **Unit test de erasure** no CI: gera fake user, faz signup, usa app, requisita erasure, verifica que **todos os backends** retornam empty/erased em < SLA.
- **Audit trimestral** de completude (sample 100 erasures).

---

## 9. Consentimento, base legal, legitimate interest

### 9.1 Bases legais usadas (LGPD Art. 7 / GDPR Art. 6)

| Base legal                                        | Dado coberto                         | Notas                                       |
|---------------------------------------------------|--------------------------------------|----------------------------------------------|
| Execução de contrato                              | Conta, billing, uso normal           | Principal base; minimização obrigatória     |
| Obrigação legal                                   | Retenção fiscal (5y)                 | Explícita; não negociável                    |
| Legítimo interesse                                | Telemetria, abuse detection          | TIA (Legitimate Interest Assessment) obrigatória |
| Consentimento                                     | Marketing, analytics opcional        | Opt-in explícito; revogável                  |

### 9.2 LIA template

Todo legitimate interest requer LIA em `_audits/`:

- Propósito específico.
- Necessidade (por que não existe alternativa menos intrusiva).
- Balanceamento com direitos do titular.
- Revisão anual.

---

## 10. Papel de controlador × operador (LGPD Art. 5 / GDPR Art. 4)

### 10.1 Matriz

| Dado                            | Controlador      | Operador       | Notas                                           |
|---------------------------------|------------------|-----------------|-------------------------------------------------|
| Conta do dev (email, nome)     | HuGR             | (nenhum)       | HuGR responsável direto                         |
| Conteúdo de blob do tenant     | Tenant            | HuGR            | HuGR assume responsabilidade via DPA; tenant decide se conteúdo tem PII |
| Telemetria (IP, UA)             | HuGR             | CF (infra)       | DPA com Cloudflare                              |
| Billing (email, CC hash)        | HuGR             | Stripe           | DPA com Stripe                                   |
| Audit logs do tenant            | HuGR (compliance) | —                | Legal hold                                       |

### 10.2 DPA padrão

- Template em `legal/dpa/v1.md` (a criar — dep futura).
- Pontos cobertos: sub-processadores, breach notification 48h, DSR support, auditoria.

---

## 11. Incident response — data breach

### 11.1 Definition of breach

Qualquer dos seguintes é considerado breach:

- Acesso não autorizado a dado pessoal (storage/log/metric).
- Perda de integridade (apagamento, corrupção) afetando dado pessoal.
- Indisponibilidade > 24h de dado pessoal (GDPR cita "loss of availability").

### 11.2 Timeline obrigatória

| Passo                                    | SLA                     | Owner              |
|------------------------------------------|--------------------------|---------------------|
| Detecção → declaração interna            | ≤ 1h                    | Oncall (SEV-1)      |
| Declaração → triage + containment        | ≤ 4h                    | Security Lead + SRE |
| Triage → notificação à ANPD (LGPD) ou DPA (GDPR) | ≤ 48h (interno) / 72h (legal) | Privacy Officer + Legal |
| Notificação aos titulares afetados        | ≤ 72h após autoridade    | Privacy Officer + Comms |
| Post-mortem público                       | ≤ 14 dias                | Privacy Officer + Security Lead |

### 11.3 Runbook

- `RB-BREACH-NOTIF` (a criar): contatos ANPD/DPA, templates de notificação, checklist legal/PR/eng.

---

## 12. Testes de privacy

| Teste                                             | Frequência     | Evidence                              |
|---------------------------------------------------|----------------|---------------------------------------|
| Redaction test (inject PII → verificar que não vaza em logs/metrics/traces) | CI every PR | EVT-002      |
| Erasure end-to-end (fake user lifecycle)           | CI every PR   | EVT-002            |
| Residency integration test (upload em EU → verificar que R2 key é EU) | Nightly | EVT-002 |
| DSR SLO report                                     | Mensal        | EVT-013                |
| Sub-processor register diff check                  | Semanal       | EVT-001                            |
| LIA review                                         | Anual         | EVT-044                      |
| Breach drill (tabletop exercise)                   | Semestral     | EVT-019 (exercise)    |
| DPIA (Data Protection Impact Assessment) novo feature grande | Por WI HIGH_RISK | EVT-045                         |

---

## 13. Referências

### 13.1 Legislação

- **LGPD — Lei 13.709/2018** (Brasil). Artigos críticos: 5, 6, 7, 18, 48.
- **GDPR — Regulamento (UE) 2016/679**. Artigos críticos: 4, 5, 6, 15-22, 32, 33, 34.
- **CCPA / CPRA** (Califórnia).
- **PIPEDA** (Canadá).

### 13.2 Frameworks

- **LINDDUN** — https://linddun.org/
- **NIST Privacy Framework** 1.0.
- **ISO/IEC 27701:2019** — PIMS.
- **Google Privacy by Design** principles.

### 13.3 Concorrentes

- **BuildBuddy** — DPA clara; SOC 2 Type II; multi-region; modelo self-hosted + cloud.
- **NativeLink** — self-hosted default (escopo de privacidade ao cliente).
- **JFrog** — maduro em SOC 2 + ISO 27001; questões de data residency históricas.

---

**Fim de PRIVACY-MODEL.** Mudanças nesta matriz que afetem DSR SLA ou retention **DEVEM** gerar notificação aos titulares conforme CTRL-PRIV-021 + aprovação do Privacy Officer + Legal.
