# RELATÓRIO DEFINITIVO — AUDITORIA COMPLETA CORELINK SERVER

**Data:** 2026-08-25 · **Versão:** 2.0 (consolida 3 waves de auditoria + verificação independente + confirmação manual)
**Alcance:** segurança, corretude, dinheiro, conformidade, higiene, CI/build, arquitetura distribuída, performance end-to-end, DX/SDK, lifecycle.
**Padrão epistêmico:** toda afirmação com `file:line`. Dúvida → flag. Refutações documentadas com o mesmo rigor das confirmações.

---

# PARTE 0 — COMO LER

| Você é… | Leia |
|---|---|
| Founder c/ 3 min | Parte I + tabela IV.5 |
| Eng lead c/ 30 min | Parte II + III.A/B + IV |
| Executor | Parte III inteira + IV + VII |
| Cético | Apêndice C (reprodução) |

**IDs:** `C*` crítico · `H*` high · `M*` medium · `L*` low · `P-*` perf. **Status:** ✅ confirmado · ⚠️ escopo corrigido · ❌ refutado · 🆕 wave posterior.

---

# PARTE I — SUMÁRIOS EXECUTIVOS

## I.1 Founder (3 minutos)

A espinha de segurança é genuinamente excelente: **zero vazamentos cross-tenant encontrados em toda a superfície**, constant-time compares consistentes, checkout sem tampering possível, audit chain BLAKE3+Ed25519 real e testada. O fundamento aguenta lançamento.

Seis feridas impedem "impecável" — todas localizadas, todas com correção conhecida:

1. **O GC nunca executa (C1)** — os 3 crons de produção disparam contra handler inexistente. Cache sem eviction = append-only; tenant estoura cota → 402 eterno. Value prop não-shippada.
2. **Seed Ed25519 real em plaintext no wrangler.toml (C2)** — commitada no HEAD. Rotacionar hoje.
3. **Gate de secrets VERMELHO no main agora (C3)** enquanto o CLAUDE.md jura que passa.
4. **Repo não limpo (C4)** — 57 staged + 15 untracked, incluindo o módulo GC inteiro só no seu disco.
5. **Billing ressuscita assinatura cancelada** num race out-of-order (C5) — janela estreita, mas dinheiro.
6. **3 requests ruins congelam todos os writes** (C6) — failover sem sample floor.

Performance: **os 80% são reais e mapeados até o byte.** Top alavancas: (a) AWS SDK calcula CRC32 sobre cada byte up/down *em cima* do blake3 já pago — CPU dobrada no recurso mais escaro de meia-core (P-S2); (b) blobs só começam a ser servidos após download completo — TTFB 10s p/ 500MB onde 0,3s é possível (P-S1); (c) npm/pip/cargo/sccache pagam Argon2id 64MiB por request sem o cache que Bazel/Turbo/native já têm (P-S3).

## I.2 Eng lead — cinco lições sistêmicas

1. **Proof ≠ production**: código puro bonito cujo wiring prod usa collaborators fracos (audit in-memory, console.log, constantes simuladas, coordenador advisory).
2. **Docs afirmando controles inexistentes**: revoke KV-delete (4 sites), cadências CI, gate verde, comentários sobre o próprio comportamento (`pat_verify_cache.ts:97`; `durable_object.ts:119`; `index.ts:3133` vs `cas.rs:802`).
3. **Silent no-ops são a pior classe**: GC/DSR/heartbeat mortos — nada falha, nada alerta.
4. **Assimetrias entre superfícies irmãs = drift**: cancel-guard no worker e não no materializer; PAT-cache em 3 superfícies e não nas adapters; ac_create_only numa só.
5. **Latência vem de decisões invisíveis**: sync traits + block_in_place, D1-over-HTTP pra tudo, zero deadline propagation, buffer-first.

## I.3 Números-âncora

| Métrica | Hoje | Após plano |
|---|---|---|
| CAS GET far-region p50 | ~600ms–1,2s | ~350–650ms |
| FindMissingBlobs (500 digests) | 30–90s | <2s |
| TTFB artifact 500MB | ~10s (download completo) | ~100–300ms |
| Checkout p50 | ~1,0–1,3s | ~700–900ms |
| CPU por blob byte | blake3 ×1–2 **+ CRC32 ×1** | blake3 ×1 |
| Adapter auth cost | Argon2id 64MiB/req | µs (cache) |
| Mac-min/sem. CI desnecessário | ~800 recuperáveis | ~0 |

---

# PARTE II — METODOLOGIA

```
WAVE 1 — Descoberta (7 agents paralelos, territórios disjuntos)
  auth/PAT · billing · cache · infra/CI · worker/storage · perf-latência · perf-throughput
      ↓
VERIFICAÇÃO — 5 agents independentes (zero herança) + confirmação manual
  de TODO ponto fraco/parcial/refutado/vazio. Git archaeology inclusa.
      ↓
WAVE 2 — Deep-dive perf (5 agents, território novo, exclusão total do conhecido)
  edge TS · runtime container · micro-custos · padrões I/O · protocol surfaces
      ↓
WAVE 3 — Agressiva (8 agentes simultâneos)
  build graph · CI wall-clock · lifecycle · DO concurrency · D1 write-amp
  hop topology · SDK/client-side · money-path
```

**Contabilidade:** 20 agents auditoria + 5 verificação + dezenas de comandos manuais. Wave-1: 31 claims → 24 confirmados integralmente, 5 escopo corrigido, 2 refutados. Waves 2–3: ~60 achados novos com evidência citada.

**Lições de processo:** claims de ausência exigem grep negativo explícito; agent vazio obrigou re-verificação manual que confirmou tudo; o seed C2 passou por TODOS os 7 agents da wave 1 e só apareceu num `git log -S` manual — a rodada de mãos próprias não é opcional.

---

# PARTE III — CATÁLOGO UNIFICADO (segurança/corretude/dinheiro/higiene)

## III.A CRÍTICOS

### C1 — GC/eviction NUNCA executa; TRÊS crons de produção mortos ✅

`wrangler.toml:296-306` · `worker/src/index.ts:1752,3256` · `lib.rs` · `gc_worker/*`

**Mastigado:** cron CF entrega evento `scheduled`; worker principal exporta só `fetch` (único `async scheduled` do repo está no signup-worker — crate errado). Os 3 crons prod (GC diário, DSR horário, heartbeat */5) nascem e morrem sem destinatário. Mesmo se chegassem: `/_internal/gc/run` existe só em 2 comentários do próprio wrangler.toml; nenhum INSERT/UPSERT em `blob_meta` em código compilado (writers vivem no módulo não-declarado no `lib.rs`; `commit_put` zero call sites); eviction engine/chunker/multipart library-only.

**Negócio:** blob vive pra sempre; tenant bate cap → **402 permanente**. Storage governance unshipped. DSR sweep morto = compliance silenciosamente inexistente.

**Fix:** handler `scheduled()` no worker (um fix, 3 crons) + declarar `mod gc_worker` + `blob_meta` transacional no PUT + alerta cron→404.

### C2 — Seed Ed25519 REAL plaintext no git 🆕✅

`wrangler.toml [env.prod]`: `AUDIT_CHAIN_SIGNING_SEED_HEX="c4bdb99f…d7141"` — `git log -S` → entrou em `0a5e3349` (**HEAD**). Checklist row 164 prescreve UNSET ou `wrangler secret put`. Portador da seed forja tamper-evidence da audit chain. Gitleaks não pega (hex sem keyword = gap). **Fix:** rotacionar → remover var → política de history purge → regra gitleaks custom.

### C3 — Gate secrets VERMELHO no main ✅

Executado: `EXIT:1`, `code_only=10` (`AUDIT_ARCHIVE_BATCH_LIMIT`, `AUDIT_DRAIN_BATCH_LIMIT`, `AUDIT_DRAIN_LEASE_ENABLED`, `CORELINK_ERASE_AUTH_KEY_PREVIOUS`, `CORELINK_ORIGIN_TIMING_DETAIL`, `EDGE_ASYNC_METER`, `EDGE_DO_METER`, `EDGE_PUBLIC_READ`, `OCI_PUBLIC_DEDUP_ENABLED`, `OCI_UPSTREAM_ON_MISS`). CLAUDE.md jura `code_only=0`. Fix: rows/allowlist justificadas + doc verdadeiro.

### C4 — Repo sujo ✅

55 staged (51M+4A) + 2 unstaged + 15 untracked — incluindo `gc_worker/` (13 arquivos Rust que o C1 precisa!) existindo só no disco. Zero backup. Fix: split em PRs (gc_worker primeiro) ou discard documentado.

### C5 — Materializer ressuscita cancelada (race out-of-order) ⚠️

Guard de payload EXISTE (`handler.rs:519-521`, só `active|trialing`; test pin :1190). O buraco: upsert cego ao banco —

```sql
-- d1.rs:173
ON CONFLICT(tenant_id) DO UPDATE SET ... subscription_state='active' ...
```

— e guard lê SÓ o payload. `updated(active)` STALE pós-`deleted` passa e regrava `'active'`. Twin writer TEM guard SQL-level exatamente pra isso (`stripe.ts:860-864`, comentário :832-835 nomeia o cenário). Fix: espelhar guard por subscription-id.

### C6 — Failover congela writes com 3 requests ruins ✅

```rust
// health.rs:241-245 — lido diretamente
let health = if triggers.len() >= 3 { RegionHealth::Degraded } ...
```

Único guard: `total==0`. Teste próprio tripa com 5 amostras (:448-458). Degraded → 503 readonly todo write (:347-367). Nuances: por-instância (não global switch), mas outage regional tripа todas; sinal auto-referencial = brownout amplifier. Fix: `N_MIN`~50 + histerese + separar sinal infra/app.

## III.B HIGH

### H1 — Revoke sem KV-delete; 4 docs afirmam que sim ✅

Grep meu: único hit de delete+patrow é o **comentário mentiroso** (`tenant_suspend_gate.ts:46`). Claims falsos: `pat-moat.md:210`, ADR-0070 ×2, `pat_verify_cache.ts:97`. Revoke escreve só D1 → edge ≤60s+EC. Dentro do SLO ADR-0030, mas o mecanismo "imediato" que compensa fail-open do suspend-gate não existe. Fix: KV-delete com waitUntil OU corrigir docs até shippar.

### H2 — Webhook preso pós-falha transitória; DLQ in-memory; replay morto ✅ (pior)

Dedup commita ANTES de materializar (:606); falha transitória → retry Stripe bate `AlreadyProcessed` → 200 skip **pra sempre**. Verificação: (a) DLQ prod = `InMemoryWebhookDlqStore` (`main.rs:883`) — redeploy evapora quarentena; (b) replay script existe mas morre: assinatura fabricada → 401 no HMAC (:560-576). Fix: DLQ durável + replay autenticado bypass-token + alerta idade.

### H3 — ~~Sem portal customer-facing~~ ❌ REFUTADO

`routes/customer.rs:211` monta `POST /v1/customer/billing/portal` wired em prod (:711 → `StripePortalSessions::create`; test :1740). Residual LOW: trait PortalIssuer sem callers; 409 axis-scoped em tier_select (:882-884).

### H4 — AC GET-compare-PUT sem conditional PUT ✅

`r2_s3.rs:2295-2329`: zero hits IfNoneMatch repo-wide; PUT overwrite puro (:143-160). Dois writers divergentes concorrentes = last-writer-wins sobre ActionResult provada. Relacionado: `ac_create_only` SÓ na superfície nativa (`ac.rs:648,663`) — bypass via Bazel REST/alias. Fix: put_if_none_match + hoist helper nas 3 superfícies.

### H5 — Auto-promotion acredita em ts client-supplied ✅

`replication_coordinator_do.ts:581` `Number(b.ts_ms ?? now_ms)`; lag também; alarm :467-502 promove primary "stale" em ≤30s. Mesma shared key de todo `/_internal/*`. Fix: receipt-time server-side + chave dedicada `/_repl/*`.

### H6 — LRU eviction reseta bucket drenado pra FULL ✅ (risco aceito documentado)

`limiter.rs:358-373,421-423,466-471`. Drain próprio bucket + flood keys distintas = multiplicador sustentado. Doc argumenta defesa quantitativa (:79-85) — válida, não eliminação. Fix: tombstone deny-state separado.

### H7 — Failover audita em Mutex<Vec> ilimitado ✅

`failover.rs:286` wiring prod (`routes.rs:979-983`); backing push bare (`audit.rs:134`). Eventos SEV fora da chain. Fix: ring buffer ou outbox durável.

### H8 — Cadência CI documentada ≠ realidade ✅ (park autorizado; doc não)

coverage/cas_foundation/reproducible-build crons COMENTADOS (dispatch-only); ffi/s10/codeql vivos. coverage.yml:32: "PARKED 2026-08-10 owner-authorized" — decisão tua; a podre é CLAUDE.md afirmar cadência inexistente. Fix: doc verdadeiro + política dispatch-before-release explícita.

### H9 🆕 — audit_outbox cresce pra sempre + FULL SCAN sealed-tail

Indexes partial (`WHERE emitted_at IS NULL`) mas drain consulta `IS NOT NULL` (`audit_drain.rs:635-638`) → fora de ambos os indexes = scan completo POR HORA contra tabela sem DELETE nenhum (grep limpo; retention consumer unbuilt `audit/src/retention.rs:3`). `canonical_jcs` duplica payload (~2× row width). **Fix minutos:** índice additive `WHERE emitted_at IS NOT NULL AND sequence_number IS NOT NULL`. Depois: archive→R2_AUDIT_BUCKET + DELETE (safe: chain_head carrega resume hash+seq).

### H10 🆕 — Hot-row: UPSERT contador TODO request serializa write lock D1

`quota.ts:599-608` UPSERT `(tenant,year_month)` sincrono todo request (`index.ts:2835`). SQLite single-writer → fila de lock entre isolates do mesmo tenant. Medição própria: 152–163ms far-primary. runQuotaBatch NÃO resolve (mesmo UPSERT conteso no batch). Fixes: shard rows %16 / agregação in-memory flush 5s / DO-backed counter.

*(Perf-graves P-S1..S3 cross-ref Parte IV pela gravidade de produto.)*

## III.C MEDIUM

**Auth:** M-A1 main-key valida só length≥64 vs siblings hex+parity+503 (`index.ts:1084-1106` vs `1215-1229`) → misconfig=401 fleet-wide silencioso ✅ · M-A2 JWKS kid-miss sem cooldown (`clerk/adapter.rs:306-337`; grep cooldown=0) → DoS amplification não-autenticada ✅ · M-A3 timing pad DOIS donos — middleware padA E trait-doc exige verifier padar → cold 2× warm se conforme; INV estruturalmente inalcançável como specced (`auth.rs:145-150,557-565`) ✅ · M-A4 scope/find_only/runner-marker ≤60s staleness sem trade-off registrado (L2 cacheia ROW inteira) ✅

**Billing:** M-B1 materializer audit trail in-memory ao lado de writer durável (`main.rs:835`) ✅ · M-B2 status ausente fail-OPEN container (`handler.rs:304-310` defaulta "active") vs fail-CLOSED worker ✅ · M-B3 refund/dispute ⚠️: container trata dispute.created (:660-662), worker nem isso; **refund não revoga em NENHUM writer** — chargeback=serviço grátis · M-B4 sem reconciliação entitlement (usage only; Layer 3 perna-vácuo flat-SKU) ✅

**Failover/Replicação:** M-F1 TOCTOU route_write lock-drop :293, zero fencing (teórico, skeleton) ✅ · M-F2 failback sem catch-up gate — cooldown 24h→Primary independente de lag ✅ · M-F3 "sustained 5s window" não implementado + Down never produced consumido por dashboards ✅ · M-F4 audit-drain Vec inteiro + ok:true com partitions_failed>0 ✅ · M-F5 sha256_hex=FNV-1a-64 zero-padded fingindo SHA-256 (`router.rs:232-240`, li o corpo) ✅ · M-F6 promoção região auditada via console.log (:446-448) ✅

**RL/Config/DO:** M-R1 tier congelado lifetime do container + tier NÃO-resolvível TAMBÉM marca planned (`ratelimit_layer.rs:215-218,364-389`) ✅ · M-R2 config-do BTreeMaps crescem pra sempre (`store.rs:186-193`) ✅ · M-D1 `_system` singleton DO chokepoint control-plane (burst mint fila na frente de webhook pagamento) 🆕

**Cache:** M-C1 QueryWriteStatus UNIMPLEMENTED + BLAKE3-only advertising rejeita stock Bazel + split keyspaces sha256/blake3 derrota cross-surface dedup (moat!) ✅ · M-C2 turbo sentinel list local divergida do helper compartilhado ✅

**Infra/CI:** M-I1 colisões migrations (2× 0044_d1; 2× 013_raiz; strays alien) ✅ · M-I2 crons redundantes violando regra própria (bazel/buck2-starter, proptest-density, gc-ship-gate Sat, mutation-nightly triplo, sbom duplo) ✅ · M-I3 21 workflows sem concurrency group (nightly.yml pior: SEM cancel-in-progress sobre 5 runners) ✅ · M-I4 pre-merge REQUIRED_PRESENT sem changelog-validate ✅ · M-I5 root rot (TODO.md abril Postgres/sqlx; _archive crate morto trackado) ✅

**OKF/docs:** M-K1 conceito flagship descreve OPPOSTO do wiring (`replication-failover.md` "designed-not-wired" vs layer live montada `routes.rs:979-983`) + anchor drift operations.md ✅ · M-K2 CLAUDE.md paths ambíguos ⚠️

**Runtime wave2/3:** M-X1 D1HttpClient single-statement → persist_pending_checkout 2 writes NÃO-atômicos (drift window admitida; `/raw` batch existe na CF API) 🆕 · M-X2 reaper mata transfer >30min (lastActivityMs só no START; pre-destroy lê mesmo valor stale) 🆕 · M-X3 sem graceful shutdown → deploy hard-kill 502 burst (rollout dodge manual `--containers-rollout none`) 🆕 · M-X4 PagerDuty AWAITED no cold-start path, fetch sem timeout (0,2–2s TODO cold boot) 🆕

## III.D LOW (completo)

**Auth (11):** iter-and-discard morto argon.rs:133 · dummy PHC duplicado + fallback mata pad em regime degradado · type assertion morta mint.rs:223 · arithmetic errada NO arquivo que define scopes ("13+51"; real 12 bits/52 reserved) · empty-if "defensive check" sem efeito · chunking ownership: 3 arquivos 3 histórias · from_hash sem validação · worker native sem cold-pad (justificável HMAC-first, invariant não-documentado) · base64url non-canonical TS aceita/Rust rejeita (validade plane-dependente) · rotation adapters = simulação s/ key material, retire() aceita overlap instantâneo, WI-S13-006 ABERTA · JWT cold-pad exemption c/ justificativa tecnicamente falsa

**Billing (5):** idem-key colisão params 24h→502 opaco · 4 impls signature divergentes · cancel shape divergente writers · analytics at-least-once documentado · scaffold headers claiming todo!() em código implementado

**Cache (4):** legacy OciBlobBridge fraco dead-but-compiling · gc_worker imports 15××4 · ByteStream Read bufferiza antes de slicar range · inventário inertão (reapi tonic 7.7k linhas, ac Merkle, eviction engine, cas chunker/fastcdc, r2-multipart — duas stacks CAS paralelas mantidas c/ gates, zero efeito prod)

**Infra (7):** .gitignore dupes · semgrepignore sem .open-next/.wrangler ⚠️(.next/dist/build presentes) · specs dois homes runbook · wrangler PLACEHOLDER ids staging · custo probes (72 canary/mo hosted) · validate_specs PT-BR · reports/** trackers (perf baselines load-bearing, keep)

**Worker/storage (7):** ratelimiter hardcoded "test"/duration_us=0 (SLO structurally zero qd sink real ligar) · overhead_ms:5 // simulated consumido por SLO · write_mode probe ts=0 · 429 body errado OCI path · proptest density container 2!/540 tests ⚠️(MÁXIMO repo-wide — cliff absoluto thin, relativo não) · ~~sem OpenAPI~~ ❌REFUTADO (corelink-v1.json: 3.1.0, 44 paths, workflow validate) · SDK narrow CAS+AC only

**Wave2/3 menores:** Clerk fallback 2 queries seriais (UNION ALL possible) · token-prefix SHA antes de validar formato/HMAC (garbage flood compra digest/probe) · fanout marker verificado 2×/request · TextEncoder por call ×7 sites · Digest Display aloca 64B/log line · parse_digest format→re-split · batch manifest json! churn/digest · CoreLinkDO escreve lifecycle byte-idêntico + health full-blob cada tick 30s · DO ctor restores 2 serial gets · quickstart JS pokes private member · event-log binding 12 envs ZERO callers vivos · reqwest "blocking" inline ×4 crates derrota controle central

---

# PARTE IV — PERFORMANCE: CATÁLOGO COMPLETO

## IV.1 TIER S — os três maiores alavancas

### P-S1 — ZERO STREAMING: primeiro byte só após download completo

```
r2_s3.rs:183-189   output.body.collect().await…into_bytes().to_vec()
r2_s3.rs:1121      verify_content_hash(blake3 TOTAL) ← antes de servir
cas.rs:855         (StatusCode::OK, resp.bytes)      ← blob inteiro
```

R2 body → Vec completo → hash total → response. Transporte worker↔cliente streama bem (duplex half verificado), mas first-byte preso no download INTEIRO + rehash.

**Matemática:** 500MB @ 50MB/s ⇒ TTFB ≈10s. Streamed/ranged ⇒ ~100–300ms. **Win 30–70×** + metade da memória por GET concorrente (128MB Workers deixa de ser teto funcional).

**Fix:** ByteStream→axum::Body passthrough + blake3 incremental + ranged reads.
**⚠️ DECISÃO DE DONO:** serve-after-full-verify É o INV-CAS-INTEGRITY. Streaming aborta mid-stream em mismatch. Alternativas: HEAD pre-check + sampled verify; verify-first-chunk; waiver. É ADR, não patch silencioso.

### P-S2 — AWS SDK queima CPU por BYTE nos dois sentidos + zero deadlines + retry ×3

```rust
// r2_s3.rs:123-129 — TUDO que está configurado:
aws_sdk_s3::Config::builder().behavior_version(BehaviorVersion::latest())…
```

1. `BehaviorVersion::latest()` ⇒ checksums `when_supported` = **CRC32 sobre TODO upload + validação CRC32 de TODO download**, EM CIMA do blake3 write+read que já pagamos. Em 0.5 vCPU, recurso mais escaro queimado 2× por blob.
2. Zero TimeoutConfig (grep=0) — conexão R2 travada parks task pra sempre.
3. Retry default ×3 exp+jitter — tail ×3.

**Fix (~5 linhas):** WhenRequired nos dois checksums + max_attempts(2) + TimeoutConfig(2s/30s/60s). **Maior win único de CPU do audit.** Mesma classe: `d1_http.rs:95-97` reqwest bare sem timeout nenhum (hung call come worker thread via block_in_place) → connect 5s/timeout 15s/nodelay/pool 8.

### P-S3 — npm/pip/cargo/sccache: Argon2id 64MiB POR REQUEST sem o cache das irmãs

`npm.rs:257`·`pip.rs:259`·`cargo.rs:406` → `verify_capability` (`adapter_pat.rs:653`): HMAC→D1→**Argon2id m=64MiB t=3p4 TODO REQUEST**. Só dummy-burn anti-oracle existe. Bazel/Turbo/native usam NativePatGate (fingerprint 5s + single-flight). Custo ~50–80ms CPU + 64MiB transiente → µs; Workers cobra CPU-ms = **margem direta**; 16 permits em meia-core satura com ~5–10 verifies/s. Fix: reusar padrão EXATO do NativePatGate ao redor do resolver compartilhado.

## IV.2 TIER A

| ID | Achado | Win | Evidência | Fix |
|----|--------|-----|-----------|-----|
| **P-A1** | Audit sink: 2 INSERTs síncronos inline/op | ~100–400ms/op; batches minutos | `d1_audit_sink.rs:157` block_on HTTPS; emitters r2_s3.rs:1044,1139,1298,1432,2087,2145 serializados c/ `.map_err(AuditFailed)?` fail-closed provando await no path | Batch multi-row INSERT; flusher background c/ sync-flush SÓ em error paths; async trait end-to-end |
| **P-A2** | FindMissingBlobs serial | 500 digests: 30–90s→<2s | for-loop :147-167; fanout pattern BATCH_READ_FANOUT=16 existe unused no mesmo crate | Fanout adapter-layer + matar audit per-probe |
| **P-A3** | runQuotaBatch escrito+medido, NUNCA wired | ~150–300ms p50/req (MEDIDO) | def quota.ts:697 zero call sites; chain serial index.ts:2835→2852→2877→2937; comment :656-662 documenta 277/284/302ms fase wdb | **Uma edit no call site** |
| **P-A4** | Byte-accounting: reserve D1 POR OBJETO antes do R2, serializado | batch ingest −80%; single −10–20ms | byte_accounting.rs:826 block_on_accrue → D1 .query() :354 dentro do chokepoint único; batch route 2000 objs = ~2000 reserves ≈20–40s | Espelhar LeasedQuotaStore (`tenant_quota.rs:435-512`): pre-reserve chunks 64MiB durável, serve in-memory, reconcile drain |
| **P-A5** | Tombstone D1 RTT + Argon2id TODO GET no container | +50–250ms/GET removível; ceiling GET ×10 | cas.rs:822 (D1 REST todo GET via d1_http sem timeout) + cas.rs:802 Argon2id (worker comment index.ts:3133 jura que skipa — mente); 16-permit ceiling processo inteiro | Negative-cache (tenant,digest)→clean TTL + invalidação por erase upsert; cache verify result curto-TTL |
| **P-A6** | CryptoKey importKey POR REQUEST ×keys | ~0,5–2ms CPU/auth | index.ts:1598-1610 hexDecode+importKey por chamada; até 3× c/ rotation siblings | Map<keyHex, Promise<CryptoKey>> module-scope |
| **P-A7** | Replica session não threadada p/ tier/residency | ~100ms+ p99 far-region no KV-miss | session scoped na extractAuth (:1259); :2852/:2937 usam PRIMARY handle — pathology #99 de volta no miss-path | Criar session no topo do fetch, threadar pros 2 (cast pattern já usado p/ METADATA_KV) |
| **P-A8** | Quad serializável: suspend→meter→tier→storageSUM→residency | 1–2 RTTs (~10–150ms) todo cold/quota path | suspend :1310 dentro extractAuth; meter :2835; só storage-SUM depende mesmo de tier | Promise.all(suspend,meter,tier,residency) → conditional SUM; rt-nuclear #24 ordering preservado |
| **P-A9** | Checkout: 9 RTTs HTTPS seriais em volta do Stripe | −25–35% (~200–450ms) no momento $ | tier_select.rs:779-954: audit+lock-evict+lock-insert+DPA+active-sub+[Stripe ~400ms]+audit+persist ×2+release ≈600–700ms D1 + Stripe | join! DPA∥active-sub; CTE single-statement lock (DELETE expirados+INSERT RETURNING); release_lock spawn pós-response (best-effort documentado) |
| **P-A10** | SDKs ZERO métodos batch; server TEM 3 rotas (2000 objs) | custom integrations 500× round-trips | rg "batch" sdks/ = zero hits; server cas.rs:99-106 frozen contract x-ndjson | putBatch/getBatch/existsBatch ambos SDKs (plumbing; testes server existem) |
| **P-A11** | JS stat() baixa O BLOB INTEIRO p/ reportar tamanho | probe O(bytes)→O(1) | client.ts:168-176 doc-comment MENTIROSO ("container exposes no lighter probe") vs python client.py:241 HEAD+Content-Length (axum auto-HEAD) | Trocar p/ HEAD, espelhar python |

## IV.3 TIER B

**Container runtime:** tokio unpinned vs 0.5 vCPU (bare #[tokio::main], workers=parallelism sobre meia-core = scheduler thrash; max_blocking_threads=512 × 2MB stack dentro de 4GiB c/ block_in_place pervasivo → pin worker_threads 1–2, blocking 16) · glibc allocator default em workload high-churn (mimalloc swap, 2 linhas, 5–15% alloc-heavy) · Nagle listener raw (set_nodelay) · env::var por request na residency middleware (OnceLock boot) · 9 pools reqwest separados pro MESMO host D1 (consolidar Arc<D1HttpClient>)

**Worker edge:** verifyPatHmacMulti loop serial de keys (Promise.all fold = MAX não SUM, disciplina timing preservada) · KV gets sem {type:"json",cacheTtl} nos 4 caches (interface KvReader dropa options; entries 60s = cacheTtl mínimo exato) · Clerk fallback serial (UNION ALL)

**Data plane:** to_vec adicional bazel_v2.rs:658,788,993,1113 + turbo_v8.rs:1300 (até 100MiB; doc confessa "transiently DOUBLES the body" e dimensiona GLOBAL_TURBO_PUT_PERMITS=16 em cima disso — Bytes end-to-end pode PERMITIR subir permits depois) · Bazel PUT sha256 DUPLA (boundary :646 + durable gate r2_s3.rs:1316 — boundary compra só 422 mais cedo; threadar hex pré-computado ou dropar pass da borda)

**DO/lifecycle:** EventLogDO append 2 puts seriais (put multi-key = atômico E 2× mais rápido, mata orphan arm) · tmeta split 3 KV keys/tsusp+tres+ttier (unificar `tmeta:<tenant>` JSON: −2 KV RT warm-cold, −2 replica RT full-cold SAM ~60–240ms; flush unificado revoke/suspend) · runner_mint 3 reads D1 independentes seriais (:452-480 → db.batch) · customer_d1 dashboards 4–5 RTTs HTTPS seriais/página (overview ≈0.3–0.7s/hop ×5; join_all atrás da bridge) · ReplicationCoordinator alarm 30s eterno sobre feature inerte (heartbeat feed nunca shipou): 2880 warns/day/env ×6 envs; mutation paths já re-armam sozinhos → backoff ladder + log on change only · Presigned-R2 escape inexistente: todo byte de blob transita hop Worker→DO→container (stream limpo, mas DO vira ceiling de throughput do produto core + GB-s billed; fix maior: sign+redirect acima de size floor)

**Build:** [profile.dev] debug=true FULL (→ line-tables-only: −20–40% relink, backtraces mantêm file:line) · cf-bindings puxa stack wasm INTEIRA no build nativo (worker/web-sys/js-sys/wasm-bindgen ~25–40 crates desperdiçados — mover p/ [target.'cfg(target_arch="wasm32")'] já existente) · corelink-ops umbrella arrasta rusqlite-BUNDLED (C compile 40–90s!) + clap + tracing-subscriber + tokio-test pro grafo do SERVER (feature-gate) · aws-config feature "sso" compila aws-sdk-sso+ssooidc+sts unused · testcontainers+bollard+postgres dev-deps de um crate pago em todo --workspace test

**CI (Mac = founder machine):** canary trio e2e-clerk/stripe/admin-render 6h-interval mac-pinned (zero dependência macOS; ubuntu-latest) + terraform-drift diário mac-pinned + sbom chain 5 jobs seriais mac-pinned + stale/i18n/license = **~800 Mac-min/semana recuperáveis** com flips de runs-on + collapse de job-graph. Heavy Rust lanes JÁ têm rust-cache/timeouts/concurrency (verificado limpo).

**Money-path runtime:** webhook claim fold (process-then-claim = 2 RTTs onde db.batch([writes,claim]) = 1, exactly-once intacto) · D1HttpClient timeout (p99 guard do momento receita)

## IV.4 MICRO (grátis)

Nagle · OnceLock env · TextEncoder shared · fanout marker 1× · token-prefix SHA pós-validação · parse_digest direto · manifest push_str capacity · Digest Display stack buf · KV json/cacheTtl · acquire timeout batch-read semaphore (turbo já tem 250ms) · rand trio unificação (bump worker→rand 0.10) · reqwest workspace-normalize · aws-config drop "sso"

## IV.5 MASTER RANKING (impacto ÷ esforço)

| # | Fix | Classe win | Esforço |
|---|-----|-----------|---------|
| 1 | S2 checksums/timeouts/retry aws-sdk + d1_http timeout | CPU/byte ↑↓ + tails mortos | horas |
| 2 | A11 stat→HEAD (linha) + region docs (endpoints regionais EXISTEM deployados sam/lhr/nrt/syd; SDKs/docs apontam só global — SAM customer cruza oceano porque ninguém conta) | cliente 10–50× | horas |
| 3 | S3 pat-gate cache adapters | 50–80ms+64MiB→µs/req | dia |
| 4 | A1 audit sink batch + H10 shard contador | remove 300–550ms/op | dias |
| 5 | A3 wire runQuotaBatch + A2 find_missing fanout | já medido/pronto | horas |
| 6 | S1 streaming (**ADR teu**) | TTFB 10s→0.3s | dias |
| 7 | A9 checkout trims (3 mudanças seguras) | −25–35% momento $ | meio dia |
| 8 | H9 índice additive audit_outbox | evita degradação mensal | minutos |
| 9 | A6/A8 caches auth (CryptoKey + tombstone/argon negative-cache) | ms×todas as reqs | dia |
| 10 | DO split route-kind (_system chokepoint) | p99 dinheiro desacoplado | dia |
| 11 | CI flips ubuntu-latest | 13h Mac/semana | horas |
| 12 | Build diet (debug=lto-tables, cf-bindings gate, ops diet) | −5–8min cold build | dia |

## IV.6 VEREDITO FINAL DOS 80%

Com wave 2+3 somadas à wave 1, o claim fica **folgadamente plausível por superfície**: metadata-bound >85% (A1+A2+A3+A5 somam 400–700ms removíveis de ops que custam isso hoje); large-blob TTFB 30–70× (S1); CPU/byte −33% a −50% (S2); far-region aggregate **~70% end-to-end** realista. O que era "otimista" na wave 1 virou alcançável quando S1/S2/S3 apareceram — os três atacam o que antes parecia custo estrutural (transporte, SDK default, cripto por request).

---

# PARTE V — O QUE ESTÁ VERIFICADAMENTE EXCELENTE (não mexer)

**Isolamento tenant (audit exaustivo, zero leakage paths):** HMAC prefix derivation; fail-CLOSED prefix não-derivável (`r2_s3.rs:915`, `r2_kv.rs:122`); instance==caller double-check; uniform-404; HEAD-only findMissingBlobs; reserved sentinels; digest re-verify server-side write E read.

**Crypto hygiene:** subtle::ConstantTimeEq em TODA comparação sensível (token_id, issuer, audience, env, smuggled-tenant, internal-auth padded s/ length oracle); PatPlaintext zeroize-on-drop, Debug redacted, sem Display/Serialize; blake3 SIMD defaults on; sha2 SHA-NI; tenant-prefix derive clean (fixed buffer).

**Stripe:** signatures constant-time ±5min raw-body-first multi-v1 (4 impls corretas hoje); preço 100% server-side (cliente nunca manda amount; metadata↔price cross-check fail-loud); checkout: lock 60s + UNIQUE partial + idem key determinística = double-charge fechado; unpaid não ativa; redirects allowlisted; webhook worker = SOTA (terminal-cancel guards, dunning-reactivation guarded, process-then-claim exactly-once analytics, ZERO outbound Stripe calls por webhook — deliberado e documentado :349-354).

**Auth caches:** L1/L2/L3 exatamente como ADR-0070 documenta (waitUntil write-behind correto pós-#859); NativePatGate single-flight sharded; QuotaGate warm-lease 16:1; tombstone bloom-fronted.

**Audit chain:** BLAKE3 math + genesis/link/tamper vectors sound; CF-6 Ed25519 heads fail-CLOSED unsigned-resume; drain crash-safety sealed-tail-authoritative + CAS checkpoint; audit-outbox emitter fail-CLOSED.

**Distributed correctness onde importa:** token-bucket math (monotonic clamp, NaN defense, deny-no-decrement); DO split-brain guarantee dentro do DO real; inflight guards Drop-clean; meta SQL bound-only c/ test-gate anti-interpolação; residency guard presente; boot watchdogs must-arm rigorosos; analytics cardinality-disciplined sem PII.

**Edge TS:** streams preservados em todo forward (`duplex:half`, zero buffering hot-plane); KV write-behind waitUntil consistente; 5 caches bounded com eviction sane; Error-construction fora do happy path; Sentry sampled 0.1.

**Infra boa:** dependabot policy c/ rationale excelente; patches pinned justificados; Dockerfile digest-pinned non-root slim (~15–30MB, apt lists removidos, cargo/target fora das layers via cache mounts); gitleaks posture correta (exceto gap do seed); pre-merge script logic sólida fail-closed; heavy Rust lanes JÁ têm rust-cache/timeouts/concurrency; OpenAPI spec viva c/ validate+sync workflow.

**Runtime clean-checks wave2/3:** zero MutexGuard-across-await prod; zero N+1 além dos flagados; zero missing-index nas hot tables; statement builders &'static str; multipart thresholds sane (16MiB vs 5MiB min); R2 read-side sem head+get pairs; list pagination correta; connection pooling ok; compression ABSENTE = decisão CORRETA (negativa em meia-core); retry stacking multiplicativo NÃO existe; health poll 500ms fine; keep-warm matemática absurda ($36/mo vs 2.5s cold); lock contention checkout não tem storm por construção (409 instantâneo); admin-ui sem waterfall real (Promise.all nos clients, App Router auto-split).

# PARTE VI — EPSTEMOLOGIA: CORREÇÕES ENTRE WAVES

| Claim | Destino | Lição |
|---|---|---|
| Portal inexistente (H3 w1) | ❌ rota wired em prod | "trait sem callers" ≠ "feature inexistente" — grep negativo precisa cobrir implementações alternativas |
| Sem OpenAPI (L33) | ❌ spec 44 paths viva | glob antes de afirmar ausência |
| Cold start ~10–15s/client (P4 w1) | ⚠️ IMDS removido May → ms | magnitude claims envelhecem; commit archaeology valida |
| Materializer resurrect incondicional (C5) | ⚠️ guard payload existe; race out-of-order estreito real | ler o guard ANTES de crer no agent |
| 2 crons mortos | 🆕 3 (heartbeat) | enumerar TODOS os triggers, não os citados |
| DLQ sem replay tooling | 🆕 script existe mas morre no HMAC + DLQ in-memory | "existe X" exige testar se X funciona |
| Dispute não tratado | ⚠️ container trata created; worker não; refund ninguém | superfícies múltiplas = checar todas |
| — | 🆕 seed Ed25519 plaintext (HEAD) — perdido por TODOS os 7 agents | `git log -S` manual é irredutível |
| — | 🆕 "worker native skipa argon2 no read" (comment index.ts:3133) MENTE — gate wired em cas.rs:802 | comentário ≠ código |
| Agent C1-C4 retornou VAZIO | re-verificação manual CONFIRMOU tudo | agent vazio = re-executar, nunca assumir |

# PARTE VII — PLANO DE EXECUÇÃO

**Fase 0 — hoje (segurança/truth-pass):**
1. Rotacionar AUDIT_CHAIN_SIGNING_SEED_HEX → secret put; remover var; regra gitleaks custom
2. Secrets matrix: 10 rows/allowlist justificadas; CLAUDE.md truth-pass (gates reais, paths, cadência)
3. Land/discard staged+untracked (gc_worker PRIMEIRO)

**Fase 1 — esta semana (blockers):**
4. scheduled() handler + mod gc_worker + blob_meta writes + rota GC (=C1+C4-crons)
5. KV-delete revoke (H1) · billing resurrect guard (C5) · failover N_MIN (C6)
6. stat()→HEAD + region guidance docs (A11+A5-região)
7. aws-sdk config lines (S2) + d1_http timeout
8. Índice additive audit_outbox (H9-minutos)

**Fase 2 — sprint seguinte (latência):**
9. Wire runQuotaBatch (A3) · find_missing fanout (A2) · pat-gate adapters (S3)
10. Audit sink batch redesign (A1) · shard contador mensal (H10)
11. Checkout trims (A9) · CryptoKey cache (A6) · replica session threading (A7) · quad parallel (A8)
12. DLQ durável + replay funcional (H2) · AC conditional PUT (H4) · main-key validation + JWKS cooldown

**Fase 3 — estrutural (ADR/waiver quando aplicável):**
13. Streaming (S1 — decisão INV-CAS-INTEGRITY) · Bytes end-to-end · LeasedQuotaStore accounting (A4)
14. DO split route-kind (_system) · D1HttpClient /raw batch (mata drift checkout) · presigned-R2 escape
15. Retention jobs (audit_outbox archive→R2+DELETE) · graceful shutdown · reaper inflight counter · PagerDuty waitUntil
16. SDK batch methods · negative-cache tombstone/argon read-path

**Fase 4 — contínuo:**
17. CI flips ubuntu-latest (~800 Mac-min/sem) · kill crons redundantes · concurrency nos 21
18. Build diet (debug=line-tables-only, cf-bindings wasm-gate, ops feature-diet, drop aws-sso)
19. OKF reconcile (replication-failover, operations) · migrations cleanup · root rot · mimalloc/tokio-pinning

# APÊNDICE A — CONTAGEM FINAL

| Categoria | N | Status |
|---|---|---|
| Críticos | 6 | 5 ✅ + 1 ⚠️(escopo afiado, buraco real) + 🆕seed |
| High | 10 | 8 ✅ + 1 ❌(H3) + 2 🆕(H9,H10) |
| Medium | 30 | ~26 ✅ + refinamentos refund/proptest/CLAUDE.md |
| Low | ~46 | 2 refutados (portal-residual, OpenAPI), resto confirmado |
| Perf Tier S/A/B/µ | 12+12+~25+12 | todos c/ evidência citada |

**Total substantivo: ~104 achados.** Refutações honestas: H3-headline, L33, P4-magnitude, M-K2-parcial, L22-parcial, L32-nuance.

# APÊNDICE B — REPRODUÇÃO (comandos-chave)

```bash
# C1: crons mortos
grep -n "scheduled" worker/src/index.ts            # só fetch; único async scheduled está no signup-worker
grep -rn '_internal/gc/run' --include='*.ts' .     # só comentários wrangler.toml
# C2: seed no git
git log -S 'AUDIT_CHAIN_SIGNING_SEED_HEX' --all
git log -S '<valor-hex>' --all                     # → HEAD 0a5e3349
# C3: gate vermelho
python3 scripts/validate_secrets_matrix.py; echo $?
# C4: repo sujo
git status --porcelain | awk '{print substr($0,1,2)}' | sort | uniq -c
# H1: revoke sem KV-delete
grep -rn 'patrow' worker/src --include='*.ts' | grep -i delete
# H9: indexes partial vs query
grep -n 'emitted_at IS NULL' migrations/d1/*.sql; sed -n '635,638p' crates/corelink-container/src/routes/audit_drain.rs
# S2: config mínima do client S3
sed -n '123,129p' crates/corelink-container/src/storage/r2_s3.rs
# A3/P-A11: batch SDK ausente / stat bug
rg -n 'batch' sdks/js/src sdks/python/corelink; sed -n '168,176p' sdks/js/src/client.ts; sed -n '241,250p' sdks/python/corelink/client.py
```

---
*Gerado de 20 agents auditoria + 5 verificação + confirmação manual. Toda flag com file:line. Refutações na Parte VI. Supersede o relatório v1 de mesma data.*
