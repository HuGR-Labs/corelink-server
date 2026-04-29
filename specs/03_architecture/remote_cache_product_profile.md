---
id: "REMOTE-CACHE-PRODUCT-PROFILE"
type: "protocol"
doc_status: "REVIEW"
audit_status: "ACTIVE"
version: "0.2.0"
created: "2026-04-24"
updated: "2026-04-24"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["architecture", "cas", "remote-cache", "product-profile", "reapi"]
---

# Remote Cache Product Profile — CAS/AC, Dedup, Eviction, GC, Poisoning

> **doc_status:** DRAFT
> **Versão:** 0.1.0
> **Última atualização:** 2026-04-24
> **Owner:** Gustavo Schneiter
> **Aprovador Final:** Gustavo Schneiter
> **Revisores:** ⚠️ **staffing-blocked** — promoção a `doc_status: FROZEN` bloqueada até ≥ 2 reviewers nomeados conforme roles indicados (endereça F-09 audit Lote 3+4)
> **Supersedes:** —
> **Superseded By:** —

> **Propósito:** fonte canônica (Nível 3) de **requisitos específicos de produto remote cache / CAS**. Endereça audit v1 Gap 2: "framework genérico demais; faltam seções obrigatórias para CAS/AC semantics, dedup, eviction, namespace partitioning, negative caching, cache poisoning, protocol conformance, compression, 'build without the bytes', blob/chunk GC, manifest integrity e read/write path modeling."
>
> ADRs/WIs/PRRs que tocam CAS, AC, GC ou manifest **DEVEM** `inherits_from: ["REMOTE-CACHE-PRODUCT-PROFILE"]`.
>
> Competidores SOTA: NativeLink, BuildBuddy, bazel-remote, Buildbarn, JFrog Artifactory. Padrões de referência: [REAPI v2](https://github.com/bazelbuild/remote-apis).

---

## Sumário

1. [Content-Addressable Storage (CAS)](#1-content-addressable-storage-cas)
2. [Action Cache (AC)](#2-action-cache-ac)
3. [Digest algorithms + multi-hash](#3-digest-algorithms--multi-hash)
4. [Chunking e Merkle decomposition](#4-chunking-e-merkle-decomposition)
5. [Deduplication (dedup semântica)](#5-deduplication-dedup-semântica)
6. [Compression (Zstd)](#6-compression-zstd)
7. [Namespace partitioning por tenant](#7-namespace-partitioning-por-tenant)
8. [Eviction policy](#8-eviction-policy)
9. [Garbage collection (GC) correctness](#9-garbage-collection-gc-correctness)
10. [Cache poisoning resistance](#10-cache-poisoning-resistance)
11. [Negative caching](#11-negative-caching)
12. [Protocol conformance requirements](#12-protocol-conformance-requirements)
13. [Build-without-the-bytes (lazy fetch)](#13-build-without-the-bytes-lazy-fetch)
14. [Capability matrix vs competidores](#14-capability-matrix-vs-competidores)
15. [Invariants críticos](#15-invariants-críticos)
16. [Change Log](#16-change-log)

---

## 1. Content-Addressable Storage (CAS)

### 1.1 Contrato

**CAS** armazena blobs indexados por seu hash (digest). Propriedades:

- **Immutability:** blob referenciado por `digest` é imutável — modificar requer novo `digest`.
- **Dedup inherent:** mesmo conteúdo → mesmo digest → armazenado 1 vez.
- **Content verification:** cliente pode verificar blob recebido recomputando hash.
- **Idempotency:** upload do mesmo blob N vezes = 1 armazenamento (INV-CASIdempotency).

### 1.2 API (REAPI v2 compat)

Métodos obrigatórios:

| Método | Descrição | Latência target p99 |
|---|---|---|
| `FindMissingBlobs` | Lookup de existência de múltiplos digests | ≤ 50ms pra batch de 1000 |
| `BatchUpdateBlobs` | Upload em batch de blobs pequenos (< 4MiB) | ≤ 200ms pra 10 blobs de 100KiB |
| `BatchReadBlobs` | Download em batch | ≤ 200ms pra 10 blobs de 100KiB |
| `GetTree` | Traversal de Merkle tree (pra diretórios) | ≤ 100ms per level |
| `ByteStream::Read` | Stream download pra blobs > 4MiB | ≥ 100 MiB/s throughput |
| `ByteStream::Write` | Stream upload pra blobs > 4MiB | ≥ 50 MiB/s throughput |
| `SplitBlob` | Content-defined chunking (v2.3+) | ≤ 500ms pra 1GB |
| `SpliceBlob` | Reassembly de chunks → blob | ≤ 500ms pra 1GB |

### 1.3 Request body size limits

| Operação | Limit | Justificativa |
|---|---|---|
| `BatchUpdateBlobs` total request | 4 MiB | REAPI spec — blobs > 4MiB usam ByteStream |
| `BatchReadBlobs` total response | 4 MiB | Idem |
| ByteStream chunk | 64 KiB typical (custom) | Streaming em chunks de 64KiB é ótimo pra rede |

---

## 2. Action Cache (AC)

### 2.1 Contrato

**AC** mapeia `action_digest → ActionResult` (resultado de execução de build action).

- Escopo: per-tenant.
- Key: digest do `Action` proto (inputs + command).
- Value: `ActionResult` proto (outputs + stderr + stdout + exit code).
- Consistência: última escrita vence (last-write-wins) — warning log se override.

### 2.2 API

| Método | Descrição | Latência target p99 |
|---|---|---|
| `GetActionResult` | Lookup por action_digest | ≤ 30ms |
| `UpdateActionResult` | Insere/atualiza AC entry | ≤ 100ms |

### 2.3 AC vs CAS

AC armazena **metadata do resultado** (quais blobs compõem os outputs). Os blobs reais vivem no CAS. Um AC hit ainda requer fetching dos blobs do CAS (a menos que já em cache local do cliente).

---

## 3. Digest algorithms + multi-hash

### 3.1 Algoritmos suportados

| Algoritmo | Status | Uso |
|---|---|---|
| **BLAKE3** | **Primary** | Default para novos blobs; 5-10× mais rápido que SHA-256; incrementally hashable |
| **SHA-256** | Fallback | Compatibilidade total com Bazel/Buck2 padrão |
| **BLAKE3ZCC** | Opcional (Buildbarn-style) | Zero Chunk Counter para dedup intra-file; avaliação futura |

### 3.2 Multi-hash (REAPI v2+)

REAPI v2 permite múltiplos digest functions simultaneamente. CoreLink **DEVE** expor `Capabilities::GetCapabilities` com:

```protobuf
digest_functions: [BLAKE3, SHA256]
```

Cliente escolhe; servidor armazena blobs indexados por ambos os digests quando necessário (duplicação controlada pra interoperabilidade).

### 3.3 Colisão handling

Probabilidade de colisão em BLAKE3/SHA-256 é astronômica (~ 2^-128). Ainda assim:

- **REG-DIGEST-001:** Ao receber upload com digest já existente mas bytes diferentes (colisão real ou ataque), servidor **DEVE** logar WARN, rejeitar o upload, e abrir incident SEV-2.
- **REG-DIGEST-002:** Verificação de digest do conteúdo recebido é **obrigatória** antes de persistir (previne corruption silently).

---

## 4. Chunking e Merkle decomposition

### 4.1 Problema que resolve

Blobs grandes (> 10MiB) causam problemas:
- Upload/download não-paralelizável.
- Dedup inexistente entre blobs com regiões idênticas (ex: 2 tarballs que compartilham 90% do conteúdo).
- Load balancing ruim (1 blob = 1 shard).
- Partial reads ineficientes.

### 4.2 Solução: Merkle decomposition

Blob > threshold (default 2 MiB) é decomposto em **chunks** de tamanho fixo, organizados em árvore Merkle. Mesmo chunk em blobs diferentes = armazenado 1x.

Inspiração: Buildbarn CAS Decomposition ADR ([ref](https://github.com/buildbarn/bb-adrs/blob/main/0003-cas-decomposition.md)).

### 4.3 Estrutura

```
Root digest = hash(manifest)
Manifest = [chunk_digest_0, chunk_digest_1, ...]  (or nested tree for >N chunks)
Each chunk = 2 MiB of data (último pode ser menor)
```

### 4.4 REAPI support

REAPI v2.3+ tem `SplitBlob` (client solicita decomposição) e `SpliceBlob` (server reassemble pra download cliente legacy).

### 4.5 Regras

- **REG-CHUNK-001:** Default chunk size = 2 MiB. Configurável por tenant via ADR futura.
- **REG-CHUNK-002:** Blob > 2 MiB **DEVE** ser automaticamente decomposto.
- **REG-CHUNK-003:** Manifest é armazenado como blob CAS (recursivo — manifest de manifests possível).
- **REG-CHUNK-004:** Cliente que não suporta chunking recebe blob reassembled via `ByteStream::Read` transparentemente.

---

## 5. Deduplication (dedup semântica)

### 5.1 Tipos de dedup

| Nível | Como | Benefício esperado |
|---|---|---|
| **Tenant-local dedup** | Mesmo blob uploaded 2x pelo mesmo tenant = 1 armazenamento | Economia baseline |
| **Cross-tenant dedup (pending ADR)** | Blob público (ex: npm package, Docker layer base) compartilhado entre tenants | **Potencial 10-100× economia** em package mirrors |
| **Intra-file dedup (via chunks)** | Chunks idênticos em blobs diferentes compartilhados | Variável, alto em tarballs similares |

### 5.2 Cross-tenant dedup — tradeoff

**Prós:**
- Economia massiva em package mirror (npm, PyPI, Cargo) — mesmos pacotes baixados por milhares de users.
- Reduz egress via Cloudflare (zero egress para R2 interno; mas economia de storage é real).

**Contras:**
- **Vazamento de existência:** tenant A pode inferir que tenant B tem blob X (pela performance de download). Side-channel timing attack.
- Complexidade de billing (quem "paga" pelo blob compartilhado?).
- Compliance: blob compartilhado precisa respeitar retention/deletion mais restritivo entre todos tenants.

### 5.3 Regra atual

- **REG-DEDUP-001:** Tenant-local dedup é **padrão e obrigatório**. Mesmo blob 2x no mesmo tenant = 1 cópia no R2.
- **REG-DEDUP-002:** Cross-tenant dedup **EXIGE ADR** específico e **NÃO É** default. Candidatos futuros: blobs com tag `public-content-addressable` (ex: checksums de packages conhecidos).
- **REG-DEDUP-003:** Se cross-tenant dedup for habilitado, compensating controls: (a) constant-time lookup path pra prevenir timing oracle, (b) retention do storage = máximo entre todos tenants que referenciam.

---

## 6. Compression (Zstd)

### 6.1 Contrato

REAPI v2 suporta compressed-blobs via Zstandard.

- **REG-COMPRESS-001:** ByteStream uploads/downloads **DEVEM** suportar `zstd` compression (level 3 default; configurável).
- **REG-COMPRESS-002:** Blobs armazenados em R2 **PODEM** ser compressed ou raw, por decisão interna (hoje: raw; compressão futura via ADR avaliando custo CPU vs economia de storage).
- **REG-COMPRESS-003:** `Accept-Encoding: zstd` em request → response comprimido. Ausência → raw.

### 6.2 Economia esperada

| Tipo de conteúdo | Razão de compressão Zstd |
|---|---|
| Código fonte / text | 3-5× |
| Binários ELF/Mach-O | 1.5-2× |
| Já comprimido (.tar.gz, .zip) | ~1× (sem ganho) |
| Images (.png, .jpg) | ~1× |

---

## 7. Namespace partitioning por tenant

### 7.1 Layout R2 (com HMAC tenant prefix — CTRL-AUTH-004)

R2 keys **NUNCA** usam `tenant_id` em plaintext. Toda chave é prefixada por HMAC(tenant_key, tenant_id)[:16] para que guessing de path por atacante com credencial de Tenant A não consiga derivar key de Tenant B. Formato canônico (corrigido S-06 do audit Lote 3+4; alinha com `data_model.md §5.1` e `storage_semantics_matrix.md §3.9`):

```
cas-<region>/
  <HMAC16>/blake3/<hex[0:2]>/<hex[2:4]>/<hex>
  <HMAC16>/sha256/<hex[0:2]>/<hex[2:4]>/<hex>

chunks-<region>/
  <HMAC16>/blake3/<chunk_hex>

manifests-<region>/
  <HMAC16>/blake3/<manifest_hex>

ac-<region>/
  <HMAC16>/blake3/<action_hex>.json

audit-<region>/
  date=<YYYY-MM-DD>/<hour>/part-<ulid>.ndjson.gz   (Object Lock)
```

Onde `HMAC16 = b64(HMAC_SHA256(tenant_derivation_key, tenant_id_bytes))[0:16]`. Tenant derivation key (TDK) vem do KMS (ver `key_management.md`); rotação anual. Com HMAC, a segurança não depende apenas de R2 IAM — mesmo com bucket policy mal configurada, um atacante precisa quebrar HMAC-SHA256 pra gerar prefix de outro tenant.

### 7.2 Regras

- **REG-NAMESPACE-001:** Todo R2 key **DEVE** começar com `<HMAC(tenant_key, tenant_id)[:16]>/<digest_fn>/<hex-shards>/<hex>`. Plaintext `tenant_id` em key é **proibido** (viola CTRL-AUTH-004 + CTRL-ISO-001).
- **REG-NAMESPACE-002:** Nenhum code path pode construir key sem derivar HMAC do `tenant_id` via lib central `tenant_path::derive_prefix(tenant_id)` (property test obrigatório).
- **REG-NAMESPACE-003:** List operation (S3 `ListObjectsV2`) **DEVE** sempre usar prefix `<HMAC16>/` — nunca scan global, nunca prefix derivado de outra forma.
- **REG-NAMESPACE-004:** Se cross-tenant dedup for habilitado (REG-DEDUP-002, requer ADR com BYOE), layout separado: `shared-public/blake3/<digest>` com ACL própria; HMAC prefix é omitido apenas neste namespace explicitamente shared.
- **REG-NAMESPACE-005:** Shards de 2 níveis (`<hex[0:2]>/<hex[2:4]>/`) para evitar hotspot em LIST + facilitar scrub paralelo por prefix.

---

## 8. Eviction policy

### 8.1 Modelo

- **CAS blobs:** tenant-specific LRU com TTL configurável por tier.
- **AC entries:** TTL-based (default 30 dias, refreshed em `GetActionResult` hit).

### 8.2 Policy por tier

| Tier | CAS storage limit | AC TTL | Eviction trigger |
|---|---|---|---|
| Free | 10 GB | 7 dias | 95% do limit OU idade > 30 dias sem access |
| Solo | 100 GB | 30 dias | 95% do limit OU idade > 90 dias |
| Team | 500 GB | 90 dias | 95% do limit OU idade > 180 dias |
| Business | 2 TB | 365 dias | 95% do limit OU idade > 365 dias |
| Enterprise | custom | custom | custom |

### 8.3 Regras

- **REG-EVICT-001:** Eviction **DEVE** preservar blobs referenciados por AC entries ativas (reachable from AC not evicted).
- **REG-EVICT-002:** Antes de evict, tentar promover AC entry (last_accessed update) pra ver se é blob ainda em uso.
- **REG-EVICT-003:** Eviction de blob retorna 404 pra `FindMissingBlobs` subsequente → cliente re-uploads → blob refrescado.
- **REG-EVICT-004:** Eviction emite evento `cas.evicted` com `{tenant_id, digest, last_accessed, age_days}`.

### 8.4 Eviction fairness

Em tier compartilhado (caso teórico), eviction **DEVE** ser fair entre tenants — não apenas LRU global. Evita hot tenant evicting outros.

---

## 9. Garbage collection (GC) correctness

### 9.1 Problema

Blob em R2 sem referência em D1 index é órfão. D1 entry sem blob em R2 é ghost. Ambos problemas silenciosos que crescem ao longo do tempo.

### 9.2 Reachability model

Blob é **reachable** se ao menos UM dos:

1. Tem entry em `cas_blobs` table (D1) com `tenant_id` + `digest`.
2. Referenciado por `action_cache.result_proto` ativo (expandido, blobs nos outputs).
3. Referenciado por `manifest_chunks` (chunk de manifest ativo).
4. Ancestral em Merkle tree de manifest ativo.

Blob **não-reachable** = órfão = candidato a GC.

### 9.3 GC algorithm (mark & sweep)

```
Phase 1 — Mark (read-only):
  Iterate D1 cas_blobs → set M of (tenant_id, digest) reachable direto
  Iterate D1 action_cache → parse ActionResult proto → add referenced digests to M
  Iterate D1 manifest_chunks → add chunk_digest and parent_digest to M

Phase 2 — Sweep (with grace period):
  For each R2 object key `<HMAC16(tenant_id)>/<digest_fn>/<hex-shards>/<digest>`:
    derive (tenant_id, digest) from index (D1 blob_meta row)
    if (tenant_id, digest) NOT in M AND
       NOT recently_re_referenced (AC entry created after mark_started_at; ver §9.4 INV-GC-004):
      age = now - r2_object.created_at
      if age > GC_GRACE_PERIOD (72h default pra CAS; mark-phase-aware):
        SOFT-DELETE (tombstone em D1 com deleted_at) — ver PAT-SOFT-DELETE-001
  After grace period dentro da janela tombstone (24h), physical DELETE r2 object
        Emit event `cas.gc.deleted`
      else:
        skip (too young, may be in-flight upload)
```

### 9.4 Safety properties (INVs)

- **INV-GC-001 (CRITICAL):** GC **NUNCA** deleta blob reachable. Formal spec em TLA+ obrigatória (§13 framework).
- **INV-GC-002:** GC respeita grace period — blob recém-criado (< 24h) nunca deletado.
- **INV-GC-003:** GC é idempotente — rodar 2x consecutivas produz mesmo resultado.
- **INV-GC-004:** Race entre upload e GC é safe: concorrent upload completa PRIMEIRO → D1 insert → GC sweep então enxerga reachable.

### 9.5 Cadência

- **REG-GC-001:** GC roda diariamente via cron DO, horário off-peak.
- **REG-GC-002:** Emite métricas `gc_deleted_count`, `gc_deleted_bytes`, `gc_duration_seconds`.
- **REG-GC-003:** GC em tenant específico (manual) disponível via admin API.

### 9.6 Tombstoning (opcional)

- Deleted blobs podem ter tombstone em D1 por 30 dias (pra auditoria: "blob foi deletado quando e por quê").
- Reduz overhead de auditoria forense.

---

## 10. Cache poisoning resistance

### 10.1 Vetor de ataque

Atacante com credenciais válidas uploads blob malicioso com digest forjado (ex: modifica um jar mas reporta digest de versão segura). Downstream builds fetcham digest "confiável" e recebem bytes alterados.

### 10.2 Mitigações obrigatórias

| Mitigação | Implementação |
|---|---|
| **Server-side digest verification** | Ao receber upload, CoreLink **DEVE** recomputar digest dos bytes. Se diverge do reported, reject 400. Previne type 1 poisoning. |
| **Cryptographic tenant isolation** | Poisoning por tenant A não afeta tenant B (REG-NAMESPACE-001). |
| **Content verification em cliente** | Cliente REAPI (Bazel/Buck2) faz próprio check após download. Se servidor retorna bytes != digest, falha. |
| **Audit log de uploads** | Todo `UpdateActionResult` gera `audit.cas.uploaded` com `principal_id`, `tenant_id`, `digest`, `size`. Detecta padrões suspeitos. |
| **Rate limiting per-tenant** | Previne uploads em massa de conteúdo malicioso. |
| **Optional: signed digest lists** | Tenant pode opt-in a `signed-provenance`: apenas digests em allow-list assinada são aceitos (futuro). |

### 10.3 Regra

- **REG-POISON-001:** CoreLink **DEVE** recomputar e verificar digest de todo byte stream recebido antes de persistir. Sem verificação = falha de design.
- **REG-POISON-002:** Digest mismatch em upload gera evento `cas.digest_mismatch.detected` severity SEV-2 (pode indicar ataque).

---

## 11. Negative caching

### 11.1 Problema

Cliente faz `FindMissingBlobs` pra digest X → servidor responde "missing". Cliente upload X. Outro cliente faz `FindMissingBlobs` pra X antes de KV propagar (eventual consistency). Segundo cliente recebe "missing" e re-upload — waste.

### 11.2 Solução

- **REG-NEGATIVE-001:** `FindMissingBlobs` **DEVE** sempre fallback a R2 HEAD em miss de KV, pra não depender de eventual consistency do KV.
- **REG-NEGATIVE-002 (revised cycle 7 SEAL Lote 10.2bis):** Negative cache entries ("blob X não existe") **PODEM** ser cacheadas em KV **somente para `GetBlob` short-circuit path** (single-digest read; per WI-S02-005 §6.1.3); cliente retry pattern + TTL bound (≤ 300s) resolve eventual consistency staleness. **`FindMissingBlobs` continua mandatorily fall-through to R2 HEAD per REG-NEGATIVE-001** (não pode usar negative cache short-circuit). HMAC16 key canonical (`ac_neg:<region>:<HMAC16>:<digest_hex>`); cross-tenant cache poisoning impossível por construction. ADR-0028 freeze: todos MissReason variants → 404 uniform.

---

## 12. Protocol conformance requirements

### 12.1 REAPI v2

CoreLink **DEVE** passar suite oficial de conformance do Bazel:

- Referência: [REAPI v2 test suite](https://github.com/bazelbuild/remote-apis-sdks)
- Cadência: CI em todo PR que toca `src/reapi/`
- Falha = merge bloqueado

### 12.2 Métodos obrigatórios

| Método REAPI | Status |
|---|---|
| `Capabilities.GetCapabilities` | ✅ obrigatório |
| `ContentAddressableStorage.FindMissingBlobs` | ✅ |
| `ContentAddressableStorage.BatchUpdateBlobs` | ✅ |
| `ContentAddressableStorage.BatchReadBlobs` | ✅ |
| `ContentAddressableStorage.GetTree` | ✅ |
| `ContentAddressableStorage.SplitBlob` | ⚠️ opcional REAPI v2.3+ |
| `ContentAddressableStorage.SpliceBlob` | ⚠️ opcional |
| `ActionCache.GetActionResult` | ✅ |
| `ActionCache.UpdateActionResult` | ✅ |
| `ByteStream.Read` | ✅ |
| `ByteStream.Write` | ✅ |
| `Execution.Execute` | ⛔ Fase 2 (Remote Execution além de cache) |
| `Execution.WaitExecution` | ⛔ Fase 2 |

### 12.3 Package mirrors (HTTP protocols)

Pacotes não-REAPI suportados no produto completo (fase multiprotocolo):

| Protocol | Spec | Status |
|---|---|---|
| npm registry | [npm spec](https://github.com/npm/registry) | Fase 2+ |
| PyPI simple + JSON | [PEP 503](https://peps.python.org/pep-0503/), [PEP 691](https://peps.python.org/pep-0691/) | Fase 2+ |
| Cargo sparse + crates.io | [Cargo sparse](https://doc.rust-lang.org/cargo/reference/registry-index.html) | Fase 2+ |
| Go GOPROXY | [Go module proxy](https://go.dev/ref/mod#goproxy-protocol) | Fase 2+ |
| OCI Distribution | [OCI Distribution Spec](https://github.com/opencontainers/distribution-spec) | Fase 2+ |
| Homebrew bottles | HTTP via domínio configurável | Fase 2+ |
| Maven Central | HTTP GET + POM | Fase 3+ |

---

## 13. Build-without-the-bytes (lazy fetch)

### 13.1 Conceito

Bazel/Buck2 têm modo `--remote_download_minimal` onde cliente baixa APENAS output final, não intermediários. Intermediários ficam no CAS remoto e são baixados sob demanda.

### 13.2 Implicação pra CoreLink

- **REG-BWB-001:** CAS **DEVE** servir byte ranges via `ByteStream::Read` com `read_offset` arbitrário — permite cliente buscar só parte do blob sem baixar todo.
- **REG-BWB-002:** Partial reads **NÃO DEVEM** exigir re-download completo do blob no servidor — usar R2 Range reads diretamente.
- **REG-BWB-003:** Latência de `FindMissingBlobs` é crítica pra modo BWB — cliente faz muitos lookups rápidos. p99 ≤ 50ms ou client stalls.

---

## 14. Capability matrix vs competidores

| Feature | **CoreLink (target)** | NativeLink | BuildBuddy | bazel-remote | JFrog Artifactory |
|---|---|---|---|---|---|
| REAPI v2 | ✅ | ✅ | ✅ | ✅ (HTTP + gRPC) | ⛔ |
| BLAKE3 | ✅ primary | ✅ | ⚠️ SHA-256 primary | ⚠️ SHA-256 | ⛔ |
| Merkle chunking | ✅ target | ✅ | ⚠️ partial | ⛔ | ⛔ |
| Zstd compression | ✅ | ✅ | ✅ | ⚠️ opcional | ⛔ |
| Multi-tenant native | ✅ | ⚠️ (self-hosted) | ✅ | ⛔ (single-tenant) | ✅ |
| Per-tenant rate limit | ✅ | ⚠️ | ✅ | ⛔ | ✅ |
| Real-time cache events | ✅ (Fase 2) | ⛔ | ⛔ | ⛔ | ⛔ |
| Remote Execution | ⛔ (Fase 2+) | ✅ | ✅ | ⛔ | ⛔ |
| Package mirrors unified | ✅ (Fase 2+) | ⛔ | ⛔ | ⛔ | ✅ |
| Build-without-the-bytes | ✅ target | ✅ | ✅ | ✅ | N/A |
| Cross-tenant dedup | ⚠️ optional, ADR-pending | ⛔ | ⛔ | N/A | ⚠️ |
| Signed provenance (SLSA L3+) | ✅ target GA | ⚠️ | ⚠️ | ⛔ | ✅ |

### 14.1 Diferenciais que importam

1. **Multi-protocol unified** (REAPI + npm/PyPI/OCI/Cargo/Go): único player no espaço comunitário.
2. **Real-time cache events via WebSocket**: nenhum concorrente oferece hoje.
3. **Cloudflare-native** (zero-egress R2 + edge): diferencial de latência/custo que on-prem players não conseguem.
4. **BLAKE3 primary**: diferencial de performance ainda raro.

---

## 15. Invariants críticos

| Invariant | Severidade | Forcing Factor | Verificação |
|---|---|---|---|
| INV-CASIdempotency | CRITICAL | FF-HR-002 (tenant isolation não aplica aqui, mas correção sim) | Property test em Rust |
| INV-TenantIsolation | CRITICAL | FF-HR-002 | Property test + TLA+ |
| INV-AuditLogImmutability | HIGH | FF-HR-005 | Integration test + schema constraint |
| INV-GC-001 (reachable never deleted) | CRITICAL | FF-HR-011 (ADR-0012) | TLA+ model checking (CTRL-FORMAL-001) |
| INV-QuotaEnforcement | HIGH | — | DO atomic test |
| INV-DigestVerification | CRITICAL | FF-HR-005 | Integration test |

---

## 16. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 0.1.0 | 2026-04-24 | Gustavo (via Claude Opus 4.7) | Versão inicial (Lote 4). Cobre CAS + AC + digest multi-hash + Merkle chunking + dedup + compression + namespace + eviction + GC correctness + poisoning + negative caching + conformance + BWB + capability matrix vs 4 competidores SOTA + invariants críticos. |

---

**Fim de REMOTE-CACHE-PRODUCT-PROFILE.**
