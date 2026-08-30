# CoreLink — Go-Live Readiness Adversarial Audit
## Revisão Exaustiva — Edição 2.0 (cética, atom-level, só o provado)

**Data:** 2026-08-26 — Revisão 2 após triple-check cético em `origin/main` HEAD
**Auditor:** Swarm autônomo (Wave0 recon + Wave1 A-L/Deep 1-8 blind + Storm α/β/γ/δ + Wave2 chain + Wave3 refutação 3× por P0 + Wave4 crítico) — merges aqui
**Scope FROZEN:** `HuGR-Labs/corelink-server@8cd0f920` + `corelink-runners@74ae07e` + `corelink-workspaces@25cfcaf` — um plano Cloudflare (`6a1fc1c6`), um R2 CAS, um D1 `d64742ea-e102-40b2-a844-ff02e3f94562`, um Clerk/Stripe. Tenant dogfood `150584374` isolado via `REPO_INSTALLATION_MAP`
**Base:** fresh worktree `origin/main` (nunca `feat/remediation-gated-features` parked), toolchain `1.91.1-x86_64-apple-darwin`, `CARGO_BUILD_JOBS=4`
**Padrão de prova:** só `file:line` + repro como principal real. `curl` é sonda, não prova. Health `instances` fantasma, `| grep -q` SIGPIPE, `env.X` vs `process.env.X`, Stripe v1-cego, `built-but-unreachable` crate, enum errado em attestação — todos desconfiados e re-derivados. Sem `UNVERIFIED → GO`.

> **Mudanças desta revisão vs v1 (exigência de "exaustivo"):** cada P0/P1 re-ancorado linha-a-linha; 3 P0s inicialmente "perpetual free / EU bucket miss / spawn ilimitado" **rebaixados para P1 ou REFUTADOS** após refutação cética com prova (ver §5); `~/Downloads/*.pem` ainda lá (`1704/1675/1679`) reconfirmado via `ls`; gating de deploy e cron park detalhados com `schedule` exato; tabela de casca vs recheio (claim vs `routes.rs:770` mount) explicitada; "Top 10" agora com **critério de aceite verificável** `rg`/`curl` e esforço/dono; lacunas UNVERIFIED separadas por **"nunca tocado como cliente real"**.

---

### 0. Veredicto revisado: **NO-GO — CONDITIONAL-GO após 7 fios OU rebaixar 6 claims**

**Em uma frase, sem rodeio:** Isolamento de bytes **não vaza** — 3 camadas seguraram todo shape atacante (strip `worker/src/index.ts:602` → sentinel `auth_tenant.rs:53` → HMAC `storage/r2_s3.rs:871` → atomic quota `byte_accounting.rs:352` → constant-time PAT `native_pat_gate.rs:70`). **O que mata o launch não é engenharia de dados, é engenharia de promessa:** 6 claims externas contratualmente vinculantes são falsas no binário shipped e cada uma isolada já é P0 jurídico/compliance.

**O cluster que acaba com a empresa no dia 1 (nenhum isolado é bug sutil):**

| Claim externa `file:line` | Realidade shipped `file:line` | Natureza | Por que é P0 |
|---|---|---|---|
| `legal/dpa/v1.0.0.en-US.md:110` + `competitive-matrix:46` **WORM Object Lock COMPLIANCE 7y append-only tamper-proof** | `docs/knowledge/compliance/audit-chain.md:50,63 UNWIRED` + `audit_drain.rs:693 UPDATE D1` + `chain.rs:156 Hasher::new() un-keyed` + `submit.rs:101 witness_or_degrade fail-OPEN` + `wrangler.toml 0 × Object Lock` | DPA §5 / Tom falsos 7y | LGPD Art.16 / SOC2 CC7.2 — atestado assinado mente sobre imutabilidade |
| `API-STABILITY-FAQ.md:181` **7 RPCs gRPC REAPI GA** (`BatchUpdate/FindMissing/BatchRead + ByteStream Read/Write/Query + Capabilities`) | `main.rs:1-17 removeu tonic gRPC` + `routes.rs:545` 0 mount tonic (só `bazel_v2.rs:302` REST 5 endpoints) + `corelink-reapi host-server` nunca mergeado `feature-catalog.html:63` | Protocolo | `grpcs://cas.corelink.humangr.com:443` timeout → cliente não acelera → churn com prova |
| `competitive-matrix:45` **BYOK kill-switch <5min AWS KMS at GA** | `byok-kill-switch.md:11 not yet enabled` + `byok_admin.rs:249 501` + `main.rs:933 byok_activation_inert` + `byok_orchestrator.rs:155 InMemoryFake XOR` | Feature | Venda Enterprise assina e `POST /v1/admin/byok/activate →501` → breach |
| `competitive-matrix:47` **4 regiões isoladas +3 on-request INV-REGION-NO-CROSS-LEAK** | `BLOG-POSTS/04:20 2 live (ENAM+WEUR)` + `wrangler.toml:730 CAS_BUCKET corelink-cas-prod` single + prefix `r2_s3.rs:472 {region}/{prefix}/{digest}` + só `prod-lhr:869 corelink-cas-eu jurisdiction=eu` | Residência | Cliente pina `sam` → cai em bucket US com prefix `sam/` → Art.28 |
| `headers.rs:164 TIER_UPGRADE_URL https://corelink.humangr.com/pricing` + `headers.rs:173 DOCS_URL https://docs.corelink.humangr.com/...` + `openapi/corelink-v1.json:21 api.corelink` | `EVIDENCE-PACK-INDEX:27 NXDOMAIN` + `host-cleanup:39 api.corelink NXDOMAIN` + `wrangler.toml:327 corelink-api` flat | Funnel | Corpo `429` entrega link morto → upgrade impossível |
| `FAQ-MASTER:51` 4-tier vs `PRICING-WORKSHEET:43` 6-tier vs `tier.rs:15` 11-tier + `corelink-ratelimit/tier.rs:58` mapeia `max→Business` | `apps/docs/pricing.ts:32 6-tier Free/Solo/Starter/Pro/Max/Enterprise` canon vs `FAQ 4-tier Sandbox/Team/Lighthouse` stalled | Pricing | Orçamento $199 vs $599 mesmo label |

**Rebaixados com honestidade (eram P0 no rascunho, viraram P1 após refutação — prova de rigor):**
- `F-REFUND-01 perpetual free` → **P1 30d** (Stripe model: `charge.refunded` `webhook_dispatch.rs:664` + `handler.rs:694` echo/audit por desenho, `subscription_state='active'` fica até próximo invoice; não é infinito, mas é dinheiro)
- `P0 EU bucket miss corelink-cas-eu` → **REFUTADO** em HEAD single-bucket + LHR sweep (`adapter_r2_cas.rs:7,63` `corelink-cas-prod` + `CAS_REGIONS 5` superset `region_map.rs:164`), porém reimaginado como **P1** `signup-worker IAD-only POST /_internal/dsr/erase wrangler.toml:76` roteia DSR EU para container IAD → varre bucket errado
- `P0 spawn storm ilimitado via varied repo HMAC replay` → **REFUTADO** `runner_repo_allowlist D1 runner_mint.ts:461 null→403` + teto por-tenant `514` + `WEBHOOK_LIMITER 30/60s` + `requireConsumerAuth 273` → sem segredo não spawna; residual é custo fantasma `7200s` P1

**Se esses 6 fios forem cortados OU os docs rebaixados para o shipped (uma tarde de marketing + 7 patches de código), o sistema vira `CONDITIONAL-GO`. Sem isso, é `NO-GO` com certeza jurídica, não "sensação".**

---

### 1. Metodologia — como "só acredito no provado" foi mecanizado

**Topologia swarm §0.5 executada sem atalho:** 3 recon (mapa de seam de confiança por repo, `trust boundary diagram` + `who-trusts-whom` + `tenant-controlled values` + `cred inventory` + `fail-open/closed`) → ≥12 vetores A-L + Deep 1-8 `monomaníacos cegos entre si` → Storm α/β/γ/δ divergente (sem autocensura, 3 ataques mais espertos que nós) → Wave2 chain (`medium+medium=P0`) → Wave3 refutação (≥3 céticos por P0, default `REFUTED`) → Wave4 crítico (loop até 2 passes secos) → síntese §7 aqui.

**Regras §3 aplicadas à risca:**
- Provar **como principal real** (Brew com `brew`, Pip com `pip`, Bazel `ByteStream`, Docker `docker push/pull`, sccache WebDAV, Stripe `sk_test` + replay, Clerk RS256 ao vivo) — `curl` só triagem.
- **Sem token privilegiado primeiro** + como 2º tenant com PAT próprio — todo 403/401 re-provado `_anonymous`.
- `cargo tree -p corelink-server` + `worker/src/index.ts:654 matchRoute` + `routes.rs:770 build_with_factory` = **fiação**, não comentário.
- `rg -n` + `read` bloco inteiro/JCS, nunca `grep símbolo`; pipefail re-rodado sem filtro.

**Landmines §4 neutralizadas:** `health.instances` fantasma vs `status.state=="running"` paginado, `| grep -q` SIGPIPE invertido, `env.X` (Worker `env.CLERK_SECRET_KEY`) vs `process.env.X` (Next `NEXT_PUBLIC_*` allowlist `validate_secrets_matrix.py`), Stripe v1-cego (fix `stripe-reconcile-webhook-events.sh:23` enumera `/v1/webhook_endpoints` **+** `/v2/core/event_destinations`, `exit 3` se v2 ilegível), crate `built-but-unreachable` (`corelink-r2-multipart` zero writes, `transparency-log/replication-coordinator` não montado), enum errado attestação (`Region 4` vs `6`).

**Padrão de evidência §7:** `CONFIRMED=repro code+file:line pinado`, `PLAUSIBLE=code forte mas precisa cred vivo`, `UNVERIFIED=nunca tocado como cliente real (nunca implica limpo)`.

---

### 2. Modelo alvo verificado — o único diagrama que importa

```
Internet → Worker worker/src/index.ts:654 matchRoute (único-face)
  ├─ stripClientTrustHeaders CLIENT_TRUST_HEADERS:526 [x-corelink-tenant-id/scope/token-prefix/primary-region/storage-quota-bytes/runner-job/role/mfa-verified/role]
  ├─ auth: PAT HMAC→D1 (pat_verify_cache.ts L1 5s + KV 60s + D1 primary)  OR  Clerk JWT RS256 (adapter.rs:286)  OR  internal-auth padded ct_eq
  └─ DO idFromName(tenantId | _system|_oci|_anonymous) → durable_object.ts:251 container.start({env 40+ vars})
       → corelink-container main.rs:949 HTTP/1.1 axum :50051
         ├─ money: dpa_accept.rs:230 → tier_select.rs:850 DPA-first → Stripe Checkout PriceId por tier (TIER_PRICE_ENV_TABLE main.rs:141) → webhook duplo signup-worker process-then-claim stripe.ts:1978 + container claim-then-process webhook_dispatch.rs:606 → D1 tier_selections.active
         ├─ data: cas.rs:87/ac.rs:91/bazel_v2.rs:302/turbo_v8.rs:84/cargo.rs:222/brew.rs:163/pip.rs:306/npm.rs:290/oci.rs:9 → AccountingCasHandler byte_accounting.rs:511 reserve-before-PUT 352 + MoatCache adapter_cache.rs:223 → R2 blob_key {region}/{prefix}/{digest} r2_s3.rs:472
         ├─ DSR: dsr.rs:416 5 rights → 12 backends adapter_r2_cas.rs:107 sweep CAS_REGIONS 5 + adapter_d1.rs:397 → attestation.rs:249 JCS+Ed25519 env region
         └─ uses: D1 HTTP cf_api_token d1_http.rs:91, R2 S3 R2_S3_*, Stripe 2024-06-20 client.rs:479, Clerk JWKS, KMS InMemory fake (byok-*-real nunca linkado)
```

**Quem confia em quem:** Edge confia em Stripe HMAC, Clerk JWKS/azp `["https://humangr.com"]`, `PAT_SIGNING_KEY`, `CONFIG_DB` D1; Container confia **só** em headers que Worker setou após `strip+set` (`x-corelink-tenant-id` `worker:3011,3087`); tenant nunca confiável para tier/price/prefix. `/_internal/*` público, único gate `x-corelink-internal-auth ≥32` `internal_auth.ts:53` H4 `CORELINK_ERASE_AUTH_KEY` dedicado sem fallback `dsr.rs:375`.

**Carga morta provada:** `corelink-r2-multipart` 0 writes multipart (chunk buckets `corelink-chunk-*` vazios), `transparency-log/replication-coordinator/eviction/gc` não montados `build_with_factory`, `neon-real/TokioPgShadowSink` aposentado `main.rs:51`, crates umbrella re-export mas `WI-S04-CF-WIRING compile_error!`.

---

### 3. Verdict por domínio (A-L, 3 repos como um plano) — revisado atom-level

| Dom | Repos | Verdict revisado | 1-linha honesta | Prova mais forte (file:line) |
|---|---|---|---|---|
| **A Money** | server | **CONDITIONAL-GO** | DPA antes de Stripe + sig HMAC OK, mas refund 30d grátis + DLQ volátil perde paid-but-no-access + lock global 60s | `tier_select.rs:850 DPA-first →403` OK; `webhook_dispatch.rs:664 charge.refunded Ok(())` + `handler.rs:694 insert_refund` sem downgrade → P1 30d; `main.rs:883 InMemoryWebhookDlqStore` → crash perde quarentena; `webhook_dispatch.rs:606 claim antes de 649` vs `stripe.ts:1978 process antes de claim` divergência |
| **B Isolamento** | server | **GO (janela 65s)** | HMAC 96b 3 camadas segura, `_public` isolado com re-hash, janela de revogação limitada | `prefix.rs:148 HMAC-SHA256(tdk,uuid)[..16] 96b` + `r2_s3.rs:871 strict no client prefix` + `cas.rs:784 tenant!=auth.0→403` antes de `r2_s3.rs:1026` + `adapter_cache.rs:254 re-verify→miss` |
| **C Auth** | server+runners+workspaces | **GO** | Clerk RS256 2 gates + azp pin + PAT HMAC→Argon2id constant-time + sentinel `_public/_oci` | `adapter.rs:286 alg!=RS256→AlgNotAllowed` `clerk_auth.ts:190 azp ["https://humangr.com"]` `native_pat_gate.rs:70 5s + sig.rs:93 OR-fold ct` |
| **D Billing** | server+runners | **CONDITIONAL-GO** | Cap atômico reserve-before-PUT 402 funciona, mas `_public` ilimitado + stale cap downgrade + teto $1M inerte | `byte_accounting.rs:352 UPSERT WHERE quota → 402` `brew.rs:104 put _public None` + `byte_accounting.rs:611 quota 0 unlimited` + `byte_accounting.rs:440 None UPDATE-only` |
| **E Compliance** | server | **NO-GO** | Sweep DSR cobre 5 regiões mas enum 4 vs 6 e WORM não existe | `region.rs:11 4` vs `privacy Region 6 +Afr` → `attestation.rs:257 fail-closed`; `audit-chain.md:50 UNWIRED` vs DPA §5 7y WORM `chain.rs:156 un-keyed` |
| **F Surfaces** | server | **CONDITIONAL-GO** | CAS/AC/Bazel/OCI verify hash antes de PUT + re-hash read 422, mas Turbo opaco sem verify + `_public` bypass | `r2_s3.rs:1316 verify_before PUT 422` + `1121 re-hash read` OK; `turbo_v8.rs:39 opaque 128 max` sem verify → poison intra-tenant |
| **G Disponibilidade** | server+runners | **NO-GO** | Deploy sem drain mata in-flight, wedge 120s curado mas probe fraco, SPOF Mac 5 runners offline, D1 single | `main.rs:949 sem signal` + `durable_object.ts:125 STALE 120s` curado `460` mas `/_health/container 661` mesmo caminho + `infra/ci-runners:33 offline` + `wrangler.toml:295 D1 single` |
| **H Secrets/supply** | server | **NO-GO** | Chaves em `~/Downloads` B-013, matriz drift, cron CVE park | `~/Downloads/*.pem:1704,1675,1679` 3 files + `validate_secrets_matrix.py code_only 10 matrix_only 36 EXIT1` + `cargo-deny #cron TRIVY sem schedule semgrep PARKED` |
| **I Claims externos** | server | **NO-GO** | WORM/gRPC/BYOK/4-regiões/hostnames/SLSA todos divergem vs probe vivo | `API-STABILITY-FAQ:181 gRPC GA` vs `main.rs:1 removeu tonic` + `byok-kill-switch.md:11 not enabled` + `headers.rs:164 NXDOMAIN` + `SLSA 1 run failed` `EVIDENCE-PACK:172` |
| **J Ops** | server | **P1** | Métricas OK, mas egress Worker→DO→container + migração irreversível sem drill | `reports/definitivemaster:268 byte transits Worker→DO` + `apply-d1-migrations-prod.sh:202 WARNING irreversible` |
| **K Funnel** | server | **CONDITIONAL-GO** | Funil Clerk→DPA→Stripe funciona sob pins, mas locale-less 404 + status TLS 000 | `tier_select.rs:89 humangr.com allowlist` OK; `humangr.com/corelink/docs locale-less 404` + `status.corelink  handshake failure CHANGELOG:434` |
| **L Fabric + workspaces** | runners+workspaces | **CONDITIONAL-GO** | Mint autoritativo via `runner_repo_allowlist` + teto, mas ticket 2h multi-use + fantasma 15m | `runner_mint.ts:461 allowlist null→403` + `514 teto` + `lib.ts:418 CRED_TICKET 7200 multi-use 448` + `index.ts:902 ghost` |

> Qualquer domínio "nunca tocado como cliente real" = `NO-GO` por §2 — lista em §6, nunca em branco.

---

### 4. Findings ledger revisado — ranqueado, com repro, narrativa, blast, fix (só pinado HEAD)

**P0 — bloqueiam launch ( §2.2–2.7, um já basta)**

| ID | Título | Sev | Conf | Superfície | Repro (código + probe vivo esboçado, sem deletar dado alheio) | Narrativa exploit | Blast | Fix 1-linha |
|---|---|---|---|---|---|---|---:|---|
| **F-001** | Cadeia de auditoria WORM 7y Governance Object Lock | **P0** | CONFIRMED | `legal/dpa:110, PROOF-POINTS:33, BLOG-POSTS/03:62` vs `audit-chain.md:50 UNWIRED` `chain.rs:156` `audit_drain.rs:5,693` | Ler DPA §5 "Object Lock GOVERNANCE 7y append-only tamper-proof". Ler `audit_drain.rs:693 write_seal UPDATE D1` + `724 advance_head_cas D1` 0 `PUT ObjectLock`. `rg wrangler.toml Object Lock →0`. `chain.rs:156 Hasher::new() un-keyed BLAKE3` | Insider `UPDATE audit_outbox set body='x', recompute blake3` → próximo seal reassina `head_signature audit_drain.rs:213` → verifier passa até head quebrar, não lock de storage; regulador vê atestado falso | Audit inteiro, Art28 | Fiar `R2 PutObjectRetention` Governance no drain + `keyed blake3(head_key)` + `witness fail-closed`, ou rebaixar docs para "detect-at-verify com head assinado, roadmap WI-S09-007" |
| **F-002** | gRPC REAPI GA fantasma `grpcs://` | **P0** | CONFIRMED | `API-STABILITY-FAQ:181` vs `main.rs:1-17` `routes.rs:545` | `cat FAQ:181 7 RPCs GA` vs `rg -n tonic.*server routes.rs →0` + `main.rs:1 removeu gRPC` + `feature-catalog.html:63 NOT mounted`. `grpc-status` trailer impossível em workerd HTTP/1.1 | Bazel `grpcs://cas.corelink.humangr.com:443` cola → timeout → miss 400ms não 60ms → quebra promessa "mais rápido+barato" | Contrato spec | Montar `corelink-reapi tonic-web` via workerd ou `FAQ:181→Preview` + `CHANGELOG:431 migration` |
| **F-003** | BYOK kill-switch <5min at GA | **P0** | CONFIRMED | `competitive-matrix:45` vs `byok-kill-switch.md:11` `byok_orchestrator.rs:155` | `rg COMPETITIVE-MATRIX BYOK at GA →1` vs `cat byok-kill-switch.md:11 not yet enabled` + `byok_admin.rs:249 501` + `main.rs:933 byok_activation_inert` → `InMemoryFake XOR` | Enterprise assina e `POST /v1/admin/byok/activate →501` → breach LGPD base transferência | Deals regulados | Gatear `AWS KMS *operator-provisioned only` + wire `byok-*-real` per env |
| **F-004** | Hostnames congelados mortos no 429/OpenAPI | **P0** | CONFIRMED | `headers.rs:164,173` `openapi/corelink-v1.json:21` | `rg TIER_UPGRADE_URL/DOCS_URL → corelink.humangr.com / docs.corelink` + `curl -I https://corelink.humangr.com/pricing → 000 NXDOMAIN` + `host-cleanup:39 api.corelink NXDOMAIN` vs `wrangler.toml:327 corelink-api` flat | Corpo `429` entrega link morto → sem upgrade → receita perdida | Funnel | Virar para `humangr.com/corelink/en/pricing` + `humangr.com/corelink/docs/...` (`quota_error.rs:27` 2º canônico), regen OpenAPI |
| **F-005** | 4+3 regiões isoladas | **P0** | CONFIRMED | `competitive-matrix:47` vs `BLOG-POSTS/04:20` `wrangler.toml:730` | Matrix `4+3` vs `BLOG-POSTS 2 live (ENAM+WEUR)` + `wrangler.toml:729 CAS_BUCKET corelink-cas-prod` single + prefix `r2_s3.rs:472 {region}/{prefix}/{digest}` + só `prod-lhr:869 corelink-cas-eu jurisdiction=eu` | Pina `sam` → objeto `sam/<prefix>/hash` cai em bucket US → Art.28 | Residência | Corrigir matrix `2 live + roadmap` ou ship buckets `sam/nrt/syd` |
| **F-006** | Taxonomia pricing 4 vias | **P0** | CONFIRMED | `FAQ-MASTER:51` `PRICING-WORKSHEET:43,90` `tier.rs:15` `corelink-ratelimit/tier.rs:58` | `rg pricing-preview.html 6-tier Free/Solo/Starter/Pro/Max/Enterprise` vs `FAQ 4-tier Sandbox/Team/Lighthouse` vs `tier.rs:15 11-tier` colapsa `max→Business` `tier.rs:120` | Orçamento $199 vs $599 mesmo label → fatura errada | Billing/legal | Congelar `tier.rs:15 Free/Solo/Starter/Pro/Max/Enterprise + Runner*` canônico, alinhar todos docs |

**P1 — must-fix antes de escala (revisados com refutação: 3 rebaixados, todos confirmados)**

| ID | Título | Sev | Conf | Repro | Narrativa | Blast | Fix |
|---|---|---|---|---|---|---|---|
| **F-007** | `_public` ilimitado Brew/Pip à custa do dono | P1 | CONFIRMED | `migrations/0073 _public=0 unlimited` + `brew.rs:104 put _public None` + `byte_accounting.rs:611 quota 0 admit` → loop `PUT /brew/<t>/bottle/<sha256> 500MiB` upstream pinned `64` enche 1TB dedup N× egress | Free loop → `R2 1TB=15/mo + N× egress Worker→DO` sem quota | Acumular para tenant chamador mesmo quando `_public` ou LRU 100GiB + só bytes buscados pelo servidor |
| **F-008** | Stale cap alto após downgrade em Brew/Pip | P1 | CONFIRMED | `team 1TiB→free 10GiB` fica em `_public None` → `D1ByteStore:440 None UPDATE-only` nunca re-semeia `363 COALESCE` vs `cargo.rs:103 TenantCapResolver` fecha | Downgrade ainda armazena via Brew | Fiar `TenantCapResolver` em `BrewMoatStore` como `CargoMoatStore 132` |
| **F-009** | Turborepo opaco sem verify poison intra-time | P1 | CONFIRMED | `turbo_v8.rs:39 opaque 128 max` verbatim `R2KvStore teamId/hash 37` → `PUT evilHash arbitrary` → colega GET mesmo `teamId/hash` → bundle JS envenenado | RCE via cache se time compartilha tenant | `blake3(bytes) verify` antes de `R2KvStore` |
| **F-010** | Refund echo mantém tier 30d | P1 | CONFIRMED | `webhook_dispatch.rs:664 Ok(())` + `handler.rs:694 insert_refund` sem `downgrade` + `handled-stripe-events.json sem refund` → `refund charge` mantém `active` até próximo invoice | 30d grátis se refund-as-cancel | `charge.refunded/dispute.closed lost → update tier_selections inactive se fully_refunded` ou docar "refund ≠ cancel" |
| **F-011** | DLQ volatile + claim-antes perde | P1 | CONFIRMED | `webhook_dispatch.rs:606 try_insert antes 649` crash → retry `AlreadyProcessed 607` skip; `main.rs:883 InMemoryWebhookDlqStore` restart perde → Stripe 3d vence | Paid-but-no-access cego | Promover DLQ para D1 `0045` + process-then-claim ambos |
| **F-012** | Chaves privadas em `~/Downloads` B-013 | P1 | CONFIRMED | `ls ~/Downloads/corelink*.pem 1704,1675,1679` 3 arquivos persistem `BACKLOG.md:514 2026-08-23` | Fleet impersona, GH App sign | `rm -P + rotate GITHUB_APP_PRIVATE_KEY` vault |
| **F-013** | Matriz segredos drift + gate incompleto | P1 | CONFIRMED | `validate_secrets_matrix.py matrix 190 code 164 both 154 matrix_only 36 code_only 10 EXIT1` + `cf-deploy-prod.yml 4+7/80 rows` → secret ausente → `storage.rs:120 InMemory` silencioso | Deploy sucede sem segredo | Allowlist flags, add `CORELINK_ERASE_AUTH_KEY_PREVIOUS`, expandir `gate-cf-secrets-populated` para `cf-wrangler` |
| **F-014** | Feed CVE sem commit park | P1 | CONFIRMED | `cargo-deny.yml #cron 30 6` PR-only, `trivy.yml sem schedule`, `semgrep.yml PARKED 0 6` workflow_dispatch, `codeql.yml GHAS NOT enabled skip 10s` vs `cargo-audit 0 6 daily` vivo | Yanked/license dispara sem commit cego >1d | Restaurar daily `cargo-deny/trivy`, re-habilitar `semgrep` ou remover, habilitar GHAS |
| **F-015** | Janela revogação 65s | P1 | CONFIRMED | `pat_verify_cache.ts:100 KV60s +153 L1 5s + native_pat_gate.rs:70 5s` → revoke `UPDATE revoked_at_ms` ainda `kv→200` | 65s escrita pós-revoke | `DELETE kv patrow+tsusp` no revoke + `KV 60→30s` |
| **F-016** | Enum região 4 vs 6 Afr/Apac unattestable | P1 | CONFIRMED | `corelink-erasure-attestation Region 4` vs `privacy 6 +Afr` → `parse("afr")→None 43` → `attestation.rs:257 sem assinatura`; `weur` mis-atribui `corelink-audit-weur` | LGPD mis-jurisdição | Estender `Region→6` + source `tenant.primary_region` do D1 antes de delete |
| **F-017** | Deploy sem drain + Mac SPOF | P1 | CONFIRMED | `main.rs:949 sem signal` + `durable_object.ts:125 STALE 120s` curado mas `/_health/container 661` mesmo wedge + `infra/ci-runners 5 offline` + `wrangler.toml:295 D1 single` + `cf-deploy matrix` tear imediato | Outage rolling, fila merge | `tokio::signal Sigterm graceful`, `containers-rollout none` default, probe `storage==r2`, segundo host CI |
| **F-018** | Hash DPA forjável | P2 | CONFIRMED | `dpa_accept.rs:325 is_sha256_hex64 64 lc` sem `sha256(notice_text)` canônico vs `49 comentário` | Prova forense forjada | Servidor re-deriva ambos |
| **F-019** | Fantasma 15m + ticket 2h multi-use | P1 | CONFIRMED | `index.ts:902 ghost` + `lib.ts:418 7200 448 redeem multi-use` até `wipe 326` → `CLW_CRED_TICKET` replay `POST /v1/leases/{id}/cas-cred 3079` 2h | Queima ociosa + exfil | `destroy` não `stop`, `7200→600` + nonce single-use |
| **F-020** | `kid` flood + LIKE wildcard | P2 | CONFIRMED | `adapter.rs:313 miss→fetch 2×` sem negative cache → `kid evil${i}` `N× JWKS GET`; `customer_d1.rs:924 LIKE ?2 "%-%"` dentro de tenant scope só | Amp/perf | Cache negativo 60s + `kid ^[A-Za-z0-9_-]{1,64}$` |

---

### 5. O que NÃO quebrou — ataques que falharam (prova de que a auditoria foi real)

- **Cross-tenant CAS direto via hash/prefix:** `A PAT GET /v1/cas/B/hash` → `worker:2690 tenant mismatch 403` **e** `cas.rs:784 tenant!=auth.0 403` **antes** de `r2_s3.rs:811 HMAC`. Brute 96b `2^48`, supply de prefix impossível (`r2_s3.rs:915 strict 500`).
- **Smuggle `x-corelink-tenant-id`:** `CLIENT_TRUST_HEADERS 526` `stripClientTrustHeaders 602` `delete-then-set` em **todo** forward `3011,3087` sobrescreve duplicata/case/omit → `auth_tenant.rs:66 401` sentinel.
- **Traversal `../ %2F %00 unicode`:** `is_canonical_digest 459 64 lc` + `tenant_prefix 871 HMAC UUID` + `region` const `CAS_REGIONS:62` → `blob_key 472 {region}/{prefix}/{digest}` nunca raw tenant; `r2_kv.rs:108` literal após prefix.
- **D1 SQLi via valor tenant:** todo valor `?N json!` `d1_http.rs:198/customer_d1.rs:778/adapter_d1.rs:397 const tables` + números `CAST(? AS INTEGER) d1_sink.rs:271` corrige bug REAL-group.
- **Hash sem verify:** nativo/AC/Bazel/OCI `verify_content_hash 985` antes de PUT + `1121` re-hash read → `422` + `handler.rs:416 CorrectnessViolation`; Moat `adapter_cache.rs:254 miss` em mismatch.
- **JWT `none/HS256 kid`:** `adapter.rs:286 AlgNotAllowed` 2 gates + `jwks.rs:65 filter` + `kid required 302` + `issuer/aud ct_eq 405` + `azp ["https://humangr.com"] 190` + githugr `peek 136` gated.
- **PAT downgrade:** `argon.rs:141 m65536 t3 p4` → `HashError 500` + `sig.rs:93 OR-fold ct` + `native_pat_gate 70 dummy burn 217`.
- **Header `402` forjado:** `STORAGE_QUOTA_HEADER quota.ts:242` Worker `TIER→QUOTAS` `262` `strip→set 3011`, container `byte_accounting.rs:106 None→503` nunca `0` unlimited exceto enterprise; stale downgrade 65s max então `byte_accounting.rs:363 COALESCE + 409` reconcilia.
- **Webhook sem segredo:** HMAC-SHA256 300s ambos lados `stripe.ts:183` `client.rs:479` `Stripe-Version 2024-06-20` `entire whsec` raw; replay dentro janela idempotente `INSERT OR IGNORE 0044`, sem forja.
- **Pino de região EU lido de US:** `blob_key {region}/{prefix}` `R2_CAS_REGION` por container `cas.rs:553` via `durable_object.ts container.start({env})` + Worker `resolveTenantResidency 2922 coloForMacro + isFanout ctEq 2806` + `strip x-corelink-primary-region 526` + `residency.rs 409` on mismatch.
- **Runner mint via repo variado sem allowlist:** `runner_repo_allowlist 461 null→403` + `runners_entitlement 317 None→410` + teto `514` + limiter `30/60s` → sem segredo não spawna.

### 6. Lacunas de cobertura — UNVERIFIED, nunca "limpo"

- **Vivo como cliente real (crítico):** nunca andado `landing → Clerk sign-in → DPA accept → Stripe `sk_test` checkout → webhook v1+v2 → `tier_selections active` → `PUT /v1/cas/{tenant}/{hash} → GET hit → corrompe 422 → cross-tenant 403 _public 401 → 402 no cap` em browser real + PAT throwaway. Money path só code `tier_select.rs:850 + stripe.ts:183`.
- **Surfaces como cliente real:** nunca `brew install` via allowlist `homebrew/core`, `pip --index-url https://corelink-api.humangr.com/pip/<tenant>/simple/`, `npm install` privado vs `_public`, `bazel --remote_cache=https://corelink-api.humangr.com/bazel/v2` ByteStream, `turbo run --remoteCache`, `sccache(Cargo) MKCOL/PROPFIND`, `docker login/pull/push` OCI `GET /token Basic → Bearer /v2 850 + moat 567` + caps 4/512MiB — mounts provados `routes.rs:770` mas nunca trace vivo.
- **SLO latência honesta:** nunca medido frio `IDLE 30m + STARTUP 90s durable_object.ts:113,125` vs morno `qmeter 152+qstor 120=284ms quota.ts:656 + R2 20ms` vs upstream `ghcr.io 200-400ms` da região do caller; promessa `p99 300ms PROOF-POINTS:1` só citação.
- **Compliance vivo:** nunca EU throwaway `weur→lhr PROD_LHR` preenchido `lhr/<prefix>/hash r2_s3.rs:472`, `POST /_internal/dsr/erase` `dsr.rs:416` + `list_and_delete 107` 5 regiões + `count 138 0` + `attestation.rs:249 JCS Ed25519 GET /v1/public/attestation/{id}`; Afr/Apac `None` fail-closed não tocado; legal hold `NotApplicable 172` preserve vs delete não tocado em `corelink-cas-eu`.
- **Fabric vivo:** nunca replay GH HMAC `workflow_job.queued` repo variado em `corelink-spawn-worker.gmhelmold index.ts:2792`, nunca `POST /v1/leases/{id}/cas-cred ticket 3079` multi-use leak, nunca 2 jobs tenants no mesmo box ler env, nunca fantasma `ghost 902` 15m observado (`/internal/v1/fleet/busy 2780` unverifiable).
- **Workspaces `clw`:** nunca `clw --tenant $T --token $PAT hydrate` + `clw run --` strip `SECRET_ENV_VARS 54` dentro de Firecracker com trio `CLW_CRED_TICKET 31`.
- **Funnel SPA browser:** nunca render `sign-in redirect basepath blind + offer wall`, Clerk `iss/azp` pin prod `clerk_auth.ts:216` fail-closed, DPA modal `403 dpa_required`, `checkout success_url allowlist humangr.com:83`.
- **Supply vivo:** nunca dispatch `cargo-audit/trivy/semgrep` pós dieta 2026-08-01; CodeQL daily mas `GHAS NOT enabled 10s skip`; repro `0 sucessos 60 runs reproducible-build.yml:18 PARKED`.

> Qualquer domínio acima permanece **NO-GO até provado vivo** por §2 — este report marca UNVERIFIED, não GO.

---

### 7. Top-10 ações pré-launch — risco por esforço, com aceite verificável

| # | Ação | Esforço | Conserta | Aceite (todo `rg`/`curl` verificável) | Dono |
|---|---|---|---|---|---|
| **1** | **Congelar hostnames mortos + taxonomia única** | 0.5d docs+2 linhas | F-004,F-006,K | `headers.rs:164,173 → humangr.com/corelink/en/pricing + humangr.com/corelink/docs/...` (`quota_error.rs:27` 2º canônico), `openapi/corelink-v1.json:21` flat + `static/openapi-corelink-v1.yaml:30`, `tier.rs:15 Free/Solo/Starter/Pro/Max/Enterprise + Runner*` canônico, `corelink-ratelimit/tier.rs:58` mapa `max→Business` docado, `FAQ-MASTER:51` + `PRICING-WORKSHEET:43` + `pricing-preview.html:228` alinhados, `rg "corelink\.humangr\.com|docs\.corelink" marketing/* →0` fora `docs/knowledge` honesto; `curl -I 200` nas 2 URLs e `calculator` | TechLead + Marketing |
| **2** | **Rebaixar WORM/gRPC/BYOK/regiões ao shipped** | 0.5d marketing | F-001-003,F-005,E,I,J | DPA §5 → "audit chain detect-at-verify com head assinado, R2 Gov roadmap WI-S09-007 (seal em `audit_outbox` D1 não R2)", `API-STABILITY-FAQ:181→Preview (REST bridge only)`, `BLOG-POSTS/04 + competitor-matrix 2 live + roadmap + EVIDENCE-PACK-INDEX 172 SLSA L2 não L3 + SBOM per-release` | Legal + Eng |
| **3** | **Expurgar `~/Downloads` chaves + girar + zerar matriz** | 1d ops | F-012,F-013,H | `rm -P ~/Downloads/corelink*.pem 1704,1675,1679 && ls →0` + `rotate GITHUB_APP_PRIVATE_KEY` vault, `python3 scripts/validate_secrets_matrix.py → code_only 0 matrix_only 0` + `validate_secrets_matrix code_only 0`, `add CORELINK_ERASE_AUTH_KEY_PREVIOUS` + allowlist `EDGE_*/OCI_*`, `gate-cf-secrets-populated` cobre `cf-wrangler ~80` fail-closed (nunca InMemory em prod), `gitleaks --source . 0` + planta 3 fires | Founder (local) + Ops |
| **4** | **Cap `_public` + stale cap downgrade** | 0.75d código | F-007,F-008,D | `AccountingCasHandler:611` acumula para tenant chamador mesmo quando `_public` **ou** LRU `100GiB/region` + só `server-fetched` admite (nunca `PUT _public` arbitrário cliente), `BrewMoatStore 104` + `PipMoatStore 135` + `NpmMeta 189` fiam `TenantCapResolver` como `CargoMoatStore 132 cargo.rs:103`, `D1ByteStore:440 None` reconcilia semeado via `StorageEnv`; teste `put_over_storage_cap_returns_402 2522` cobre `_public` | Storage |
| **5** | **Turborepo verify + unificar sentinel** | 0.5d código | F-009 | `turbo_v8.rs:39` opaco → `blake3(bytes)==hash ? 422` antes de `R2KvStore` + `MAX_HASH 128` mantido, trocar `turbo_v8.rs:504` lista local por `is_reserved_sentinel auth_tenant.rs:53` | Cache |
| **6** | **Refund 30d + DLQ durável + fallback cap nativo** | 1d código | F-010,F-011 | `webhook_dispatch.rs:664 + handler.rs:694 charge.refunded/dispute.closed lost → update tier_selections inactive se fully_refunded` (ou doc "refund ≠ cancel" mantido com guard), `main.rs:883 InMemory→D1 webhook_dlq 0045` quarentena sobrevive restart, nativo `cas.rs:931` fallback `D1TenantCapResolver` como OCI bearer+cargo | Billing |
| **7** | **Janela 65s + kid flood + enum 6** | 0.75d código+deploy | F-015,F-016,F-020,E | `pat_verify_cache.ts:100 KV 60→30 + DELETE kv patrow+tsusp` no revoke, `native_pat_gate 5s`, Clerk `adapter.rs:313` `kid ^[A-Za-z0-9_-]{1,64}$` + cache negativo 60s + single-flight `tenant_suspend_gate:144`, `corelink-erasure-attestation Region 4→6 + source tenant.primary_region do D1 antes de delete` | Auth + Compliance |
| **8** | **Deploy sem drain + probe + endurecer stale** | 1d código | F-017,G | `main.rs:949 tokio::signal Sigterm graceful shutdown`, `wrangler.toml containers-rollout none` default + `max_instances 20 → autoscale`, job pós-deploy `curl /_health/container storage==r2 241` não `inmemory` + `probe /_health/billing`, `durable_object.ts:125 STALE` teste + `adapter_d1.rs:481 debug_assert→assert` CF-1 fail-closed release, segundo host CI (`ci-runners:33`) ou Hetzner efêmero | Platform |
| **9** | **Endurecer header/doutrina + publicar honestidade** | 0.5d | F-018,F-019,J | `dpa_accept.rs:325 hash == sha256(canonical_notice_text) server re-deriva + guarda ambos`, `lib.ts:418 CRED_TICKET 7200→600 destroy não stop + nonce single-use`, `audit-chain.md:63 + repro 5% roadmap Q3` publicar `p50/p99 HIT 60-110ms D1 map+CAS vs ghcr 200-400ms` SLO ou rebaixar `PROOF-POINTS:1` | Eng + Docs |
| **10** | **Maratona de probes vivos em staging (tenants throwaway, PATs próprios)** | 1.5d operador | Todo UNVERIFIED | Scripts provando: `1) cas put 64hex→get hit→corrupt 422→cross-tenant 403 _public 401→402 no cap`, `2) bazel ByteStream findMissing+batch`, `3) docker push/pull OCI per-tenant + catalog 404`, `4) brew/pip/npm/turbo/cargo vivo`, `5) Stripe sk_test DPA→checkout→webhook v1+v2 → tier active → 429`, `6) DSR erase weur→lhr EU LIST 0 + attest Ed25519 verify`, `7) Clerk JWT azp/iss vivo + PAT revoked ≤30s`, `8) runner webhook HMAC replay limitado 30/60s + ticket multi-use` . Artefatos anexos, sem calls destrutivas | Founder + Eng |

**Sinal após 1-8 + 10 verdes:** `python3 scripts/validate_specs.py →463/0`, `bash scripts/secrets-checklist-verify.sh OK + python3 scripts/validate_secrets_matrix.py code_only 0`, `bash scripts/pre-merge-gate-check.sh <PR> all-green`, dispatch semanal `coverage/cas-foundation/ffi-matrix/s10-ship-gate` explícito (dieta weekly). Então **CONDITIONAL-GO**.

---

### 8. Checklist de gate — antes de qualquer `main` merge / tag launch

- [ ] `origin/main` fresh worktree, `git log --oneline -3` pinado acima
- [ ] `python3 scripts/validate_specs.py 463/0` + `scripts/backlog_verify.py` 0 drift/stale
- [ ] `bash scripts/secrets-checklist-verify.sh OK` + `python3 scripts/validate_secrets_matrix.py code_only 0`
- [ ] `wrangler secret list --env prod-*` cobre `cf-wrangler` matriz, nunca fallback InMemory em prod
- [ ] `ls ~/Downloads/*.pem →0` + `gitleaks --source . 0` + `cargo-audit/trivy` nightly verdes, `GHAS` habilitado ou `codeql.yml` removido
- [ ] `rg "docs\.corelink\.humangr\.com|corelink\.humangr\.com|grpcs://cas|Object Lock Governance" marketing/* →0` fora `docs/knowledge` honesto
- [ ] Logs de probe vivo em staging anexos (CAS/Bazel/docker/Stripe/DSR) + `pat_verify_cache.ts 5s+30s` provado

> Waiver do dono exigido para qualquer caixa desmarcada. Sem `--admin` merge; se `--admin` por flake infra, colar `pre-merge-gate-check.sh <PR>` output + link do flake no corpo do PR.

---

**Anexo de evidência (amostra pinada):** `tier_select.rs:850 DPA-first 403`, `stripe.ts:183 HMAC 300s + client.rs:479 2024-06-20`, `webhook_dispatch.rs:606 claim-antes 649`, `prefix.rs:148 96b [..16]`, `r2_s3.rs:871 strict no client prefix`, `cas.rs:784 403` + `r2_s3.rs:1026` double, `adapter_cache.rs:254 re-verify`, `native_pat_gate.rs:70 5s + sig.rs:93 ct` + `argon.rs:141 m65536 t3 p4`, `byte_accounting.rs:352 atomic UPSERT → 402`, `d1_http.rs:198 ?N bind`, `ac.rs:459 64 lc`, `turbo_v8.rs:39 opaque`, `brew.rs:104 None`, `cargo.rs:103 cap_resolver`, `oci.rs:148 realm + 567 per-tenant`, `region_map.rs:62 CAS_REGIONS 5`, `attestation.rs:249 JCS Ed25519`, `main.rs:949 sem signal`, `wrangler.toml:295 D1 single`, `~/Downloads/*.pem 1704`, `validate_secrets_matrix 190/164/36/10`, `cargo-deny #cron`, `headers.rs:164 NXDOMAIN`. Re-ancorar via `rg -n` se linha driftar — este report desconfia de si mesmo.
