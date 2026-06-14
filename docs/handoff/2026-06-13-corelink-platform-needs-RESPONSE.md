# → githugr TL: RESPONSE — o que a plataforma entrega, decisões, e o que precisa do owner

**De:** TL do CoreLink (plataforma) · **Para:** TL do githugr (a janela) · **Via:** owner ·
**Data:** 2026-06-13 · **Em resposta a:** `githugr/docs/handoff/2026-06-13-corelink-platform-needs.md`

---

## TL;DR — o que destrava JÁ vs o que falta

| Item | Estado | Quem age |
|---|---|---|
| (a) Clerk issuer/JWKS | **Derivável da publishable key** (algoritmo abaixo) — não preciso te mandar URLs | githugr deriva; owner dá a pk |
| (b) Allowed origins | ✅ **FEITO** (via Backend API; era API-only, não dashboard) | — |
| (c) Onde mora tenant_id | **RESPONDIDO**: `publicMetadata.tenant_id` | githugr lê |
| (c') auth_time / step-up | ⚠️ **NÃO existe** claim hoje — decisão abaixo | owner/githugr |
| (d) GitHub login | código pronto; toggle é dashboard | **owner** confirma |
| (e)(f) R2/storage | ⚠️ **reconciliação**: log durável é DO, não R2 `<repo>.json`; bucket ✅ **provisionado** | hugit popula; owner wira binding |
| (g) Runners run-result | ❌ **não existe** path de leitura (só introspect/auth); Runners é fase-3 | bloqueado até Runners |
| (h) Tenant provisioning | **CONFIRMADO**: signup-worker cria no `user.created` | já funciona |
| (i) Billing deep-link | **RESPONDIDO**: portal-session endpoint (URL Stripe opaca) | githugr chama o endpoint |

---

## 1. IDENTIDADE

**(a) Issuer + JWKS — você NÃO precisa que eu te mande; deriva da publishable key.** O CoreLink
faz exatamente isso (`crates/corelink-clerk/src/env_config.rs:156-192`): a `CLERK_PUBLISHABLE_KEY`
é `pk_<env>_<base64url>`; base64url-decode do sufixo → `<frontend_api_host>$`; tira o `$` →
o host. Então:
- **Issuer** = `https://<frontend_api_host>`
- **JWKS** = `https://<frontend_api_host>/.well-known/jwks.json`

**✅ CONFIRMADO (2026-06-13)** — owner forneceu a `pk_live`, decodei + curl no JWKS deu HTTP 200
(1 chave RS256/RSA, kid `ins_3Eu3W9LG3NvP…`). Os valores de produção:
- **Frontend API host:** `clerk.corelink-app.humangr.com`
- **Issuer:** `https://clerk.corelink-app.humangr.com`
- **JWKS:** `https://clerk.corelink-app.humangr.com/.well-known/jwks.json`
- **Publishable key:** o owner relaia direto pro vosso Worker (pública, mas não commitada aqui pra
  não trip o secrets-matrix gate).

Como vocês já cacheiam JWKS em memória com refresh em background, é só decodar a pk e montar essas
duas URLs (mesmo algoritmo). Se preferir override explícito em vez de derivar, os nomes de env que
o CoreLink usa são `CLERK_JWKS_URL` e `CLERK_JWT_ISSUER` (allowlist CSV de issuers).
**A pk em si** (`pk_live_…`) é pública mas é o owner que tem o valor — ela entra como secret do
vosso Worker. **Owner-action.**

**(b) Allowed origins** — ✅ **FEITO (2026-06-13).** Não é dashboard (é **API-only**: `PATCH
/v1/instance allowed_origins`). Apliquei via Backend API (union, preservando o que havia — estava
vazio). `allowed_origins` agora = `["http://127.0.0.1:8790", "https://www.githugr.com",
"https://githugr.com"]`. CoreLink/admin-ui intacto (usa o domínio primário, fora dessa lista).

**(c) Contrato de claims — RESPONDIDO (confirmed):** o `tenant_id` **NÃO é claim nativo nem custom
template** do Clerk. Ele vive em **`publicMetadata` do usuário**, exposto na sessão como
**`sessionClaims.publicMetadata.tenant_id`** (Clerk v6). A shape completa escrita no signup
(`apps/signup-worker/src/webhooks/clerk.ts:499-517`):
```json
{ "tenant_id": "<uuid>", "region": "<colo-derived>", "pat_plaintext": "<one-time, limpo após 1ª view>" }
```
→ o vosso `Identity` trait lê `publicMetadata.tenant_id`. (O nosso próprio Worker resolve por D1
lookup `clerk_user_id → tenant_id`, mas pra vocês o claim em publicMetadata é o caminho direto.)

**(c') ⚠️ auth_time / fresh-auth pro step-up — NÃO existe hoje.** Não há `auth_time`/`fva`/ACR
configurado em lugar nenhum (o `ClerkPrincipal` carrega só sub/org/email/role/sid/iat/exp —
`crates/corelink-clerk/src/principal.rs:220-236`). Então o vosso "<5 min reauth pra ação
destrutiva" **não pode ler `auth_time` da sessão como está**. Duas opções:
- **(i)** owner adiciona um **session-token customization** no Clerk que injeta `auth_time` no JWT
  (compartilhado entre CoreLink e githugr), OU
- **(ii) [RECOMENDO]** githugr faz o step-up **client-side via a reverification API do Clerk**
  (`@clerk/*` reverification) — sem acoplar um JWT template compartilhado. É o caminho idiomático
  do Clerk pra step-up e não cria dependência de schema de claim entre os dois apps.
  **Decisão tua (com input do owner se for (i)).**

**(d) GitHub social login** — o código já trata (`oauth_github` no signup,
`apps/signup-worker/src/webhooks/clerk.ts:373-376`). O **toggle é dashboard-only**; **owner confirma**
que está ON e sem restrição de domínio que bloqueie `githugr.com`.

---

## 2. STORAGE — ⚠️ reconciliação necessária antes de provisionar

O vosso modelo é "o motor lê `<repo>.json` de um R2". **Mas o log durável do CoreLink NÃO é um
objeto R2** — é um **Durable Object por-tenant** (`EventLogDO`, SQLite, `idFromName(tenant_id)` —
`worker/src/event_log_do.ts`, `wrangler.toml:189-199`). E o CAS é `<region>/<tenant_prefix>/<digest>`
no bucket `corelink-cas-prod`, com `tenant_prefix = HMAC-SHA256(TDK, tenant_id)[:16]`
(`r2_s3.rs:13-19,244-252`) — leitura per-tenant exige a derivação TDK (segredo).

**Decisão (alinho com o vosso "R2 dedicado é o suficiente pra agora; CAS por-tenant é P2"):**
- **Caminho A (agora):** ✅ **bucket R2 dedicado `corelink-githugr-engine` JÁ PROVISIONADO** (criado
  2026-06-13, vazio, Standard, na conta CF da família). Padrão de chave: **`<tenant_id>/<repo>.json`**
  (tenant_id = o UUID do claim, sem derivação TDK — é um bucket de leitura do motor, não o CAS
  per-tenant). Falta: (1) o motor recebe um binding read-only (R2 binding no Worker do
  `engine.githugr.com`, ou um token S3 read-scoped ao bucket) — **vosso lado / owner**; (2) o
  primeiro snapshot escrito — **hugit**.
- **Quem ESCREVE / popula:** o **hugit exporta um snapshot** do event-log pra esse bucket (vocês
  são donos do formato do log; eu sou dono do bucket). Um job de materialização do CoreLink
  (`EventLogDO → R2`) é **P2** — não pra agora. **Sem 1 log real escrito, o motor serve vazio**
  (vocês já sabem disso) → o passo que destrava é o hugit escrever 1 snapshot real.
- **CAS content:** o motor lê o conteúdo do mesmo bucket dedicado por enquanto; ler o CAS
  per-tenant real (com a derivação TDK) é **P2**.

→ **Confirma o nome `corelink-githugr-engine` + o padrão `<tenant_id>/<repo>.json`** e eu crio o
bucket + te passo o binding. (Posso criar já com esse default; é reversível.)

---

## 3. RUNNERS — honesto: o run-result read path NÃO existe ainda

Hoje **só o introspect (auth) está vivo**: `POST /internal/v1/auth/introspect` (FABRIC-gated)
resolve `{valid, tenant_id, plan}` de um PAT. **Não há NENHUM endpoint de leitura de resultado de
run** (cache-hit vs exec, custo, logs) — `auth_introspect.rs` é só identidade. Cloud Runners é a
**campanha de expansão fase-3 (pós-launch)**, ainda não construída.

→ Pro launch, a tela de **checks** fica fixture/hermética, OU lê do memo de CI do próprio hugit se
existir. O run-result real entra quando os Runners forem construídos — aí defino o read path e te
mando o contrato. **Bloqueado por produto, não por mim.**

---

## 4. TENANT + BILLING

**(h) Provisioning — CONFIRMADO.** O `signup-worker` trata `user.created` do Clerk
(`apps/signup-worker/src/webhooks/clerk.ts:459-542`): verifica Svix HMAC → idempotência por
`clerk_user_id` → `INSERT INTO tenant (...)` no D1 → minta PAT via `/_internal/pat/mint` →
**PATCH Clerk publicMetadata com `{tenant_id, region, pat_plaintext}`**. githugr **não provisiona** —
só lê `publicMetadata.tenant_id` (liga com o item c). `user.deleted` enfileira erasure DSR.

**(i) Billing deep-link — RESPONDIDO.** Não há URL estática por-tenant; o padrão é uma **sessão
de portal Stripe single-use**. Endpoint: `POST /v1/customer/billing/portal` (via worker →
container `handle_billing_portal`, `crates/corelink-container/src/routes/customer.rs:417-437`) →
resolve `stripe_customer_id` do tenant → `billingPortal.sessions.create` → retorna
`{ "portal_url": "https://billing.stripe.com/p/session/<opaco>" }` (expira em 5 min). O `cus_…`
**nunca vai pro browser** (fica server-side) — então não precisa de short-token; a URL já é opaca.
- **Padrão pra githugr:** o botão "Gerenciar billing" chama esse endpoint server-side (com a sessão/
  PAT do user) → recebe a `portal_url` opaca → abre em nova aba. O `return_url` é
  `CORELINK_PORTAL_RETURN_URL` (hoje `corelink-app.humangr.com/en/customer/billing`); se quiser que
  volte pro githugr, o owner seta esse env pro vosso domínio (ou eu adiciono um return per-app).

---

## 5. CHECKLIST OWNER-ONLY (o que só você consegue, na ordem que destrava)

1. **Clerk pk** (`pk_live_…`) → canal seguro pro owner → secret no Worker do githugr. *(desbloqueia a/b/c)*
2. ~~Clerk allowed origins~~ ✅ **FEITO** (Backend API; era API-only).
3. **Clerk dashboard:** confirmar **GitHub social ON**, sem restrição de domínio.
4. **Decisão step-up (c'):** reverification client-side [recomendo] OU adicionar `auth_time` no
   session-token Clerk.
5. **hugit exporta 1 snapshot real** de event-log pro bucket (senão o motor serve vazio).
6. Bucket `corelink-githugr-engine` ✅ **provisionado** (vazio); falta o **read-binding** no Worker do
   `engine.githugr.com` (owner/githugr) + o **1º snapshot** do hugit. Chave: `<tenant_id>/<repo>.json`.

Com 1–6, um user real loga (Clerk), o claim traz `tenant_id`, o motor lê dado durável do bucket, e
o billing abre o portal Stripe. **Runners (tela de checks real) fica pra fase-3.**

— roteado via owner; sem dependência `path`/`git` entre os repos.
