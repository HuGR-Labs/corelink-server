---
id: "KEY-MANAGEMENT"
type: "protocol"
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
tags: ["architecture", "security", "kms", "byok", "byoe", "encryption"]
---

# Key Management — KMS, BYOK, BYOE, Rotation

> **doc_status:** DRAFT
> **Versão:** 0.1.0
> **Última atualização:** 2026-04-24
> **Owner:** Gustavo Schneiter
> **Aprovador Final:** Gustavo Schneiter
> **Revisores:** ⚠️ **staffing-blocked** — promoção a `doc_status: FROZEN` bloqueada até ≥ 2 reviewers nomeados conforme roles indicados (endereça F-09 audit Lote 3+4)
> **Supersedes:** —
> **Superseded By:** —

> **Propósito:** fonte canônica (Nível 3; ADR-0013) do modelo de gestão de chaves do CoreLink. Endereça audit finding F-13 (BYOK/enterprise gaps vs concorrentes BuildBuddy Enterprise / JFrog / NativeLink). Coordena com `security_model.md §7` (cripto algorithms) e `privacy_model.md §7` (residency).

---

## Sumário

1. [Princípios](#1-princípios)
2. [Hierarquia de chaves](#2-hierarquia-de-chaves)
3. [Lifecycle (create → active → rotate → retire → destroy)](#3-lifecycle-create--active--rotate--retire--destroy)
4. [BYOK (Bring Your Own Key) — tier enterprise](#4-byok-bring-your-own-key--tier-enterprise)
5. [BYOE (Bring Your Own Encryption) — plano futuro](#5-byoe-bring-your-own-encryption--plano-futuro)
6. [Erasure attestation](#6-erasure-attestation)
7. [Break-glass + incident response](#7-break-glass--incident-response)
8. [Controles (CTRL-KEY-XXX)](#8-controles-ctrl-key-xxx)

---

## 1. Princípios

1. **Envelope encryption** — dados cifrados com DEK; DEK cifrada com KEK; KEK cifrada com Root KEK em HSM.
2. **Per-tenant isolation** — cada tenant tem Tenant Derivation Key (TDK) distinta derivada via HKDF.
3. **Separation of duties** — devops não acessa chaves de produção; break-glass requer dual-approval + audit rico.
4. **Rotation by default** — todas as chaves rotacionam; compromisso detectado ⇒ rotação imediata + re-wrap.
5. **BYOK opt-in** — customer enterprise pode trazer suas próprias KEKs (CMK em seu KMS externo).
6. **Transparent to customer** — mudanças de chave não quebram operações; re-wrap online.
7. **Auditable** — toda operação com chave (use, rotate, revoke) gera EVT-047 (AUDIT_EVENT).

---

## 2. Hierarquia de chaves

```
Root HSM (Cloudflare Workers Secrets, FIPS 140-2 L3 backed)
 ├─ KEK-GLOBAL (annual rotation)
 │   └─ TDK-<tenant_id>  (annual rotation; HKDF(KEK-GLOBAL, salt=tenant_id))
 │       ├─ Path-HMAC key       (HKDF info="corelink/v1/path")          → CTRL-AUTH-004
 │       ├─ AC-Sig key          (HKDF info="corelink/v1/ac-sig")        → CTRL-AC-002
 │       ├─ Envelope key        (HKDF info="corelink/v1/envelope")      → CTRL-CRYPTO-002
 │       ├─ DSR receipt JWS key (HKDF info="corelink/v1/dsr-receipt")   → CTRL-PRIV-DSR-RECEIPT (S-11)
 │       ├─ Consent HMAC key    (HKDF info="corelink/v1/consent-hmac")  → CTRL-PRIV-CONSENT-003 (S-11)
 │       └─ Erasure salt        (HKDF info="corelink/v1/erasure-salt")  → CTRL-PRIV-ERASE-PSEUDO (S-11)
 ├─ Audit chain key           (HKDF info="corelink/v1/audit-chain")     → CTRL-AUDIT-001
 ├─ Audit pseudonym key       (HKDF info="corelink/v1/audit-pseudonym") → CTRL-PRIV-014 (S-11; subject_id pseudonymize)
 ├─ DKIM broadcast key        (HKDF info="corelink/v1/dkim-broadcast")  → CTRL-PRIV-SUBPROCESSOR (S-11)
 └─ Release signing           (Ed25519, offline-HSM)                    → CTRL-SUPPLY-002

BYOK-CUSTOMER-<tenant_id>  (optional; replaces KEK-GLOBAL envelope step)
 └─ Customer CMK em AWS KMS / GCP KMS / Azure Key Vault / HashiCorp Vault
      └─ Envelope wrap/unwrap via customer KMS API
```

| Key layer | Lifetime | Rotation trigger | Re-wrap necessário |
|---|---|---|---|
| Root HSM | 5 anos | Calendar ou incident | Sim (todos KEKs) |
| KEK-GLOBAL | 1 ano | Calendar | Sim (TDKs) |
| TDK | 1 ano | Calendar ou suspeita vazamento | Sim (envelope dos blobs do tenant) |
| Release signing | 2 anos | Calendar ou offboarding | Não |
| Audit chain | 1 ano | Calendar | Não (forward-only) |
| BYOK CMK | Customer-controlled | Customer-controlled | Disparado pelo customer |

---

## 3. Lifecycle (create → active → rotate → retire → destroy)

### 3.1 States

1. **pending** — criada, ainda não ativa.
2. **active** — em uso para writes + reads.
3. **rotated** — suplantada por nova; usada apenas em reads de dados wrapped antes da rotação.
4. **retired** — não mais usada; data já re-wrapped; preservada por 90d para roll-back.
5. **destroyed** — deleted from HSM; irreversível.

### 3.2 Invariantes

- `INV-KEY-NO-SKIP`: writes nunca usam key em state `{pending, rotated, retired, destroyed}`.
- `INV-KEY-OVERLAP`: durante rotation, ambas keys (old + new) são válidas para reads por **período definido per asset class** (ver tabela 3.2.1 abaixo). O upper bound default é 24h; classes específicas têm overlap maior por trade-off de blast radius vs operational throughput.
- `INV-KEY-AUDIT`: toda transição de state gera EVT-047 (AUDIT_EVENT) + EVT-028.

#### 3.2.1 Overlap period per asset class (canonical)

Tabela autoritativa para `INV-KEY-OVERLAP` — sprints downstream **devem** referenciar este valor, não inventar números soltos. Mudança requer ADR (ver ADR-0018).

| Asset class | Overlap target | Justificativa | Sprint primário |
|---|---|---|---|
| **PAT signing key** (HMAC-SHA256; hybrid HMAC + Argon2id per S-03 cycle 9 SEAL decision (a)) | 24h | Curto blast radius; PAT verifica HMAC sig fast-fail (≤100µs) ANTES Argon2id PHC verify (defense-in-depth); rotation 24h overlap; multi-key support via signing_key_id column (data_model.md §4.1) | S-03 |
| **Audit chain key** (per-region) | 24h | Hash chain integrity precisa rotation rápida; janela de tampering minimal | S-09 |
| **Admin signing key** (HMAC-SHA256; per-region; usada por dual-approval para assinar `op_payload \|\| nonce \|\| ts`) | 24h | Curto blast radius (igual ao PAT signing); admin op é dual-approver-bound; rotation frequente reduz surface de bypass via signature replay | S-13 |
| **TDK** (tenant derivation key) | 7d | Re-wrap de envelope CAS é background job ≥ TB-scale; 24h causa starvation | S-01, S-13 |
| **BYOK customer CMK** | 7d (CoreLink-side cache) | Customer trigger; CoreLink mantém DEK cache até CMK access expira; overlap = customer notification window | S-14 |
| **Ed25519 attestation key** (per-region erasure) | 30d | Long-lived signing key; attestations 7y retention; rotation overlap garante verifiability passada | S-14 |
| **Erasure salt** (HKDF info="corelink/v1/erasure-salt") | **30d** rotation, **7y retention** | Pseudonymize subject_id em audit retained: `erased_<HMAC(salt, subject_id)>`; rotation re-pseudonymize lazy on read; salt itself retained 7y para forward verifiability (retention ≠ overlap; ADR-S11-014 separação semântica). | S-11 |
| **DSR receipt JWS key** | **30d** rotation, **90d verify-grace** | Per-region; receipt JWS verificável post-facto; rotation 30d hard upper bound; verify-grace 90d permite receipts antigos continuarem verificáveis pós rotation (separate from overlap window). | S-11 |
| **Consent HMAC key** | **30d** rotation, **7y verify-grace** | Long-lived verify-grace permite re-derive HMAC post-facto sobre records 7y retention; rotation overlap (active period) cap **30d** hard upper bound. | S-11 |
| **DKIM broadcast key** | **30d** rotation, **DNS TTL overlap** | Email DKIM signing; rotation overlap respeita DNS record TTL (publish new selector + retire after old DNS records expire). | S-11 |

**Hard upper bound (Lote 10.11.0-bis canonical clarification):** nenhum asset class pode exceder **30d de overlap window** sem ADR explícito + Security lead sign-off. Justificativa: NIST SP 800-57 Pt 1 Rev 5 §5.3 recomenda overlap < 90d; CoreLink adopta 30d defense-in-depth.

**Distinção semântica importante** (ADR-S11-014 NEW): há TRÊS conceitos distintos para chaves de longo período:
- **Rotation overlap** (active dual-use): cap **30d** absoluto (hard upper bound).
- **Verify-grace** (key retired mas usable for verification only): pode estender (90d JWS / 7y HMAC + audit chain) — não é "overlap" no sentido write-key.
- **Salt retention** (salt material kept para re-pseudonymize lazy): pode estender 7y — não é key rotation, é forward verifiability material.

WI-S11 pre-bis usava 365d como "overlap" — semantically incorrect; corrigido para distinguir overlap (30d cap) vs verify-grace/retention.

**Property test obrigatório por sprint que toca rotation**: simular rotation start → completion, verificar que durante overlap período old + new são válidas para reads, mas writes vão exclusivamente para new (INV-KEY-NO-SKIP).

### 3.3 Online rotation

1. Criar nova key (state: pending).
2. Promover nova key para active; old vira rotated.
3. Re-wrap envelope de blobs (background job; progress em `corelink_key_rewrap_progress_ratio`).
4. Quando 100% re-wrapped, old vira retired.
5. Após 90d em retired, destroy.

Objetivo: customer **nunca percebe** rotation.

---

## 4. BYOK (Bring Your Own Key) — tier enterprise

### 4.1 O que é

Customer enterprise fornece CMK (Customer Master Key) hospedada **no seu próprio KMS** (AWS KMS / GCP KMS / Azure Key Vault / Vault). CoreLink envelope-wraps DEKs usando a CMK do customer via API (KMS `Encrypt`/`Decrypt`).

### 4.2 Benefícios para customer

- **Controle de acesso:** customer vê todos os unwraps nos próprios audit logs do KMS.
- **Revocation:** customer pode revogar acesso do CoreLink à CMK; isso **torna o cache inacessível em < 5 minutos** (cache invalidation forçada). Efetivamente um "kill switch" sob controle do customer.
- **Jurisdiction:** customer pode manter CMK em região específica por compliance.
- **Compliance:** FedRAMP, HIPAA BAA, FINRA, etc.

### 4.3 Requisitos

- Tier `enterprise`.
- Customer precisa suportar AWS KMS / GCP KMS / Azure Key Vault / HashiCorp Vault Enterprise.
- IAM role/service account do CoreLink com grants específicos (`Encrypt`, `Decrypt`, `DescribeKey`).
- Latência: cada write/read adiciona 1 round-trip ao KMS do customer (10-30 ms p99 típico).
- Setup: 1-2 horas com customer admin + CoreLink support.

### 4.4 Limites

- BYOK não cifra metadata (size, refcount, created_at) — só conteúdo dos blobs.
- BYOK com customer offline por > 1 hora vira cache outage (por design; customer tem kill switch efetivo).
- BYOK rotation é controlada pelo customer; CoreLink apenas re-wraps quando sinalizado.

### 4.5 CTRLs (§8)

CTRL-KEY-010, CTRL-KEY-011, CTRL-KEY-012.

---

## 5. BYOE (Bring Your Own Encryption) — plano futuro

### 5.1 O que é

Customer cifra blobs **client-side** antes de enviar para CoreLink. CoreLink armazena blob já cifrado; apenas digest (hash do ciphertext) é visível. Customer controla completamente a chave; CoreLink nunca vê plaintext.

### 5.2 Trade-offs

**Pró:**
- CoreLink fica "zero-knowledge": mesmo com breach total, customer está protegido.
- Ideal para workloads altamente regulados (health, defense).
- Supply chain attack em CoreLink não compromete data at rest.

**Contra:**
- Dedup cross-tenant impossível (digests diferentes para mesmo plaintext entre tenants).
- Dedup dentro do tenant requer convergent encryption (cria trade-off de segurança).
- Customer responsável por key loss = data loss.

### 5.3 Roadmap

- **Fase 1 (GA)**: Não suportado.
- **Fase 2 (6-12m pós-GA)**: Suportado para tier enterprise com convergent encryption opcional.
- **Fase 3 (12m+)**: SDKs em Rust/Python/Go com envelope transparente.

### 5.4 CTRLs

CTRL-KEY-030..032 (placeholders Fase 2 BYOE; renumerados em Lote 6.6 endereçando G-09 do re-audit — CTRL-KEY-020..022 são audit/dual-approval ATIVOS em §8.5, não placeholders).

---

## 6. Erasure attestation

Customer enterprise pode requisitar **attestation criptográfica** de erasure:

1. Customer submete DSR erasure (via API).
2. CoreLink executa erasure (privacy_model §6.2 pipeline).
3. CoreLink gera attestation assinada contendo:
   - Tenant ID
   - Timestamp de conclusão
   - Hash dos backends afetados
   - Lista de sub-processors notificados
   - Hash da pre-state (via Merkle root)
4. Attestation assinada com chave Ed25519 CoreLink (long-term) + timestamp RFC 3161.
5. Customer pode verificar assinatura com chave pública publicada.

Endereça F-13 (SOTA gap vs concorrentes). Evidence: EVT-045 (DPIA-like) + EVT-047.

---

## 7. Break-glass + incident response

### 7.1 Cenários

| Cenário | Resposta |
|---|---|
| Root HSM unavailable | Automatic read-only mode; writes retornam 503; alert P0 |
| Suspected key compromise | Immediate rotate + re-wrap; revoke PATs emitidos com a key vazada; audit log flagged |
| BYOK customer revokes | Cache torna-se inacessível em < 5 min (by design); notify customer + support ticket automático |
| Dual-approval break-glass (admin override) | MFA + 2-person + audit event rico (CTRL-AUDIT-003) + expiração 1h |

### 7.2 Runbooks

- `RB-KEY-COMPROMISE` (criado Lote 5.12).
- `RB-HSM-UNAVAILABLE` (criado Lote 5.12).
- `RB-BYOK-REVOKE` (criado Lote 5.12).

---

## 8. Controles (CTRL-KEY-XXX)

Estende o catálogo de `security_model.md §6`.

### 8.1 Key hierarchy & isolation

| ID | Controle | Implementação | Evidence | Revalidação |
|---|---|---|---|---|
| CTRL-KEY-001 | Root key in HSM (FIPS 140-2 L3) | CF Workers Secrets HSM-backed | EVT-040 (vendor attestation) | Anual |
| CTRL-KEY-002 | TDK per-tenant via HKDF | HKDF-SHA256(KEK-GLOBAL, salt=tenant_id, info=...) | EVT-002 (test vectors) | Anual |
| CTRL-KEY-003 | No key export from HSM | HSM policy + audit | EVT-047 | Trimestral |
| CTRL-KEY-004 | Separation of duties | IAM roles: kms-admin ≠ app-deploy ≠ devops | EVT-035 | Trimestral |

### 8.2 Rotation

| ID | Controle | Implementação | Evidence | Revalidação |
|---|---|---|---|---|
| CTRL-KEY-005 | Annual TDK rotation | Automation via CF API + re-wrap background job | EVT-047 + EVT-013 | Anual (per key) |
| CTRL-KEY-006 | Overlap durante rotation per asset class (ver §3.2.1 canonical table; TDK 7d / PAT signing 24h / Audit chain 24h / Admin signing 24h / BYOK CMK 7d / Ed25519 attestation 30d) | Ambas keys válidas em reads durante overlap window | EVT-002 | Por rotation |
| CTRL-KEY-007 | Emergency rotation | Runbook RB-KEY-COMPROMISE; SLA 1h para revoke + 24h para re-wrap completo | EVT-017 | Semestral (drill) |

### 8.3 BYOK

| ID | Controle | Implementação | Evidence | Revalidação |
|---|---|---|---|---|
| CTRL-KEY-010 | BYOK multi-cloud support | Adapters AWS KMS, GCP KMS, Azure KV, Vault | EVT-002 (per adapter) | Por release |
| CTRL-KEY-011 | Customer kill switch < 5 min | Cache invalidation em revocation detectada | EVT-024 (chaos test) | Semestral |
| CTRL-KEY-012 | BYOK audit export | Customer vê todos unwraps no KMS log + CoreLink espelha | EVT-047 | Contínuo |

### 8.4 Erasure attestation

| ID | Controle | Implementação | Evidence | Revalidação |
|---|---|---|---|---|
| CTRL-KEY-015 | Signed erasure receipt | Ed25519 sign + RFC 3161 timestamp | EVT-011 | Por DSR |
| CTRL-KEY-016 | Public verifiability | Chave pública publicada em `/.well-known/corelink-erasure-pubkey` | EVT-047 | Por release |

### 8.5 Audit

| ID | Controle | Implementação | Evidence | Revalidação |
|---|---|---|---|---|
| CTRL-KEY-020 | All key ops logged | Evento `key.{created,rotated,retired,destroyed,used}` em CloudEvents | EVT-047 | Contínuo |
| CTRL-KEY-021 | Dual-approval destructive ops | `kms:ScheduleKeyDeletion` etc requer 2-person CoreLink + MFA | EVT-015 + EVT-016 | Por evento |

---

**Fim de KEY-MANAGEMENT.** Mudanças que tocam hierarchy de keys ou rotation policy requerem ADR + approval Security Lead + Cryptography Reviewer + Compliance Officer.
