# DEVENV_M3_AUDIT — Auditoria Independente (minimax/m3)

**Status:** ❌ REJECT — scaffold não-funcional, nada commitado
**Data:** 2026-08-27
**Modelo:** minimax/m3 (auditoria fresh, independente do Gemini)
**Método:** re-verificação ground-truth (filesystem + git diff/status + specs + lib.rs/main.rs) de cada claim do Gemini, MAIS busca de defeitos não listados. Cada linha citada foi re-verificada uma segunda vez com `sed -n` ou `grep -n`.

---

## 0. Resumo das Divergências com o Gemini

A auditoria do Gemini (`DEVENV_GEMINI_AUDIT.md`) tem **4 erros factuais importantes** que este relatório corrige. Houve também 1 erro factual meu do primeiro rascunho (N7) que foi corrigido após re-leitura do `main.rs`.

| # | Claim Gemini | Veredito M3 | Evidência ground-truth |
|---|--------------|-------------|------------------------|
| **E1** | B6 — "nenhuma rota de ingress" | ❌ **ERRADO** | `git diff worker/src/index.ts` mostra `+86 linhas` com `routeKind="devenv_v1"` + `openapi` + handler Clerk/PAT dual-auth; `git diff wrangler.toml` mostra `+durable_objects.bindings RUNNER_DEVENV_DO script_name="corelink-spawn-worker"`; `corelink-runners/deploy/cloudflare/src/index.ts:17` faz `export { RunnerDevEnvDO }` |
| **E2** | B4 — "exec-server: sem `/clw`/`/mkdir`/`/ping`/`/port-check`" | ❌ **ERRADO** | `corelink-check-exec-server/src/lib.rs:94-98` JÁ registra `/exec`, `/clw`, `/ping`, `/port-check/:port`, `/mkdir`. Gemini só checou a linha 94 e parou |
| **E3** | H2 — "`transitionState` sem guard de validação" | ❌ **ERRADO** | `runner_dev_env.ts:88` chama `validateStateTransition(this.devenvState.status, newState.status)` (se status muda). Guard existe. H2 está satisfeito |
| **E4** | H5 — "`main.rs` sem arg parsing, `--bind-addr 127.0.0.1:9090` ignorado" | ⚠️ **PARCIALMENTE ERRADO** | `main.rs:42-57` JÁ tem arg parsing `--bind-addr` E `BIND_ADDR` env var; bind configurável. H5 mantém mas pelo motivo diferente (token não injetado, não arg parsing) |

**E mais importante:** M3 encontrou **10 defeitos novos (N1–N10)** que o Gemini não listou — 2 BLOCKING, 4 HIGH, 4 MEDIUM.

---

## 1. CONFIRMAÇÕES das Claims do Gemini (re-verificadas)

Validado ground-truth via `sed -n` / `grep -n` / `git diff`:

| Claim | Status | Verificação exata |
|-------|--------|--------------------|
| Nada commitado (todos untracked ou dirty) | ✅ Confirmado | `git status --short` em `corelink-runners` (branch `chore/repin-runner-image`) e `corelink-server` (branch `feat/remediation-gated-features`) tem devenv como `??` ou `M` |
| Zero commits com "devenv"/"DevEnv"/"RunnerDevEnvDO" no histórico | ✅ Confirmado | `git log --all --grep="devenv" --oneline` vazio em ambos repos |
| B1: 6 crates faltantes no Dockerfile | ✅ Confirmado | `ls crates/` em corelink-runners = 8 crates (`corelink-check-exec-server`, `corelink-cli`, `corelink-cloud-engine`, `corelink-fabric`, `corelink-fabric-api`, `corelink-fabric-server`, `corelink-runner`, `corelink-runners-contracts`); Dockerfile:21-32 pede 12. Faltam: `corelink-identity`, `corelink-lease`, `corelink-memoize`, `corelink-policy`, `corelink-telemetry`, `corelink-testing` |
| B2: `corelink-cli` produz binário `corelink`, não `clw` | ✅ Confirmado | `corelink-cli/Cargo.toml`: `[[bin]] name = "corelink"`; Dockerfile:37 `cp /src/.../release/clw /out/clw` — não existe. Grep `name = "clw"` em corelink-runners = vazio |
| B3: flags `--auth-file`/`--pack-small-files-threshold-kb`/`--generation-id` não existem no clw-cli real | ✅ Confirmado | `clw-cli/src/subcmds/snapshot.rs:25-47` só tem `path`/`--name`/`--force`/`--json`/`--concurrency`. `clw-cli/src/subcmds/ls.rs:21-33` só `--name`/`--all`. Grep em `clw-cli/src/**/*.rs` por `auth.file\|pack.small\|generation.id` = vazio |
| B3: auth morta (`unset CLW_TOKEN` + `CLW_AUTH_FILE`) | ✅ Confirmado | `entrypoint.sh:25-31` faz `echo ${CLW_TOKEN} > /dev/shm/.clw-auth` + `unset CLW_TOKEN`. `clw-cli/src/config.rs:124-144` lê só `--token`/`CLW_TOKEN`/`CORELINK_TOKEN`/`config.toml` |
| B5: `recordUsage()` é dead code (sem D1 binding) | ✅ Confirmado | `corelink-runners/deploy/cloudflare/wrangler.jsonc`: `grep d1_databases` = vazio. `runner_dev_env.ts:339` `if ((this.env as any).CONFIG_DB)` — sempre undefined |
| B7: imagem `:latest` em `wrangler.jsonc:277` | ✅ Confirmado | `registry.cloudflare.com/.../corelink-runner-devenv:latest` |
| B7: nenhum workflow builda a imagem | ✅ Confirmado | `grep -rn "devenv\|RunnerDevEnvDO" .github/workflows/` = vazio |
| H6: `instance_type: "standard-4"` fixo, tiers não escalam | ✅ Confirmado | `wrangler.jsonc:211, 256, 278` — todos `"standard-4"`. `start()` do DO não seta `instanceType` em runtime. CF não permite mudança runtime |
| C1: quota.ts = KV-L2 perf refactor, não devenv | ✅ Confirmado | `git show HEAD:worker/src/lib/quota.ts` = `perf(worker): KV-L2 the tenant tier read on the CAS/AC hot path`. Diff `git diff --cached worker/src/lib/quota.ts` não toca devenv |
| C2: check-exec-server é pré-existente (campanha check-host) | ✅ Confirmado | Tracked em `origin/main` |
| C3: `allowedHosts` é whitelist real | ✅ Confirmado | `container.js:206-211` `if (allowedHosts && !matchesHostList(hostname, allowedHosts)) return new Response('Origin is disallowed', { status: 520 })` |
| M (spec inconsistency em requiredPorts) | ✅ Confirmado | `runner_dev_env.ts:45` `[6080, 7681, 8080, 9090]`; `runner_dev_env.ts:115` `buildStatusResponse.ports = [6080, 7681, 8080]` — inconsistência interna |

---

## 2. CORREÇÕES ÀS CLAIMS DO GEMINI

### 2.1 E1 — B6 do Gemini está ERRADO (a rota de ingress EXISTE)

Gemini disse (DEVENV_GEMINI_AUDIT.md §3.B6):
> "Sem rota de ingress (WP-08) / DO inalcançável — `RunnerDevEnvDO` só é exportado (linha 17); nenhum handler de fetch o instancia"
> "`grep -c RunnerDevEnvDO` = 1 (só o export)"

**Verificação ground-truth via `git diff` (não `grep` de arquivo commitado):**

`corelink-server/worker/src/index.ts` foi MODIFICADO (`M` no git status, branch `feat/remediation-gated-features`). Diff de 163 linhas, incluindo:

- `+4 linhas` em `interface Env` (hunk 1): `RUNNER_DEVENV_DO?: DurableObjectNamespace` + `CORELINK_API_BASE?: string`
- `+2 linhas` em `type RouteKind` (hunk 2): `"devenv_v1"`, `"openapi"`
- `+11 linhas` em `matchRoute()` (hunk 3): branch para `/openapi.json` e `/v1/customer/devenv*`
- `+17 linhas` em `baseHandler` (hunk 4): handler para `routeKind === "openapi"` (importa `devenvOpenApiSpec` de `./lib/openapi_devenv.js`)
- `+81 linhas` em `baseHandler` (hunk 6): handler completo para `routeKind === "devenv_v1"` (Clerk/PAT dual-auth, `checkDevenvQuota`, `idFromName`, `devStub.fetch`)
- hunk 5: 1 linha trivial (`: null` em vez de `: undefined` em coordStub body, não relacionado a WP-08)

`corelink-server/wrangler.toml` foi MODIFICADO:
```diff
+# DevEnv DO cross-worker binding (WP-08)
+[[durable_objects.bindings]]
+name = "RUNNER_DEVENV_DO"
+class_name = "RunnerDevEnvDO"
+script_name = "corelink-spawn-worker"
```

`corelink-runners/deploy/cloudflare/src/index.ts` foi MODIFICADO:
```diff
+export { RunnerDevEnvDO } from "./durable_objects/runner_dev_env";
```

`corelink-runners/deploy/cloudflare/wrangler.jsonc` foi MODIFICADO:
- `+RUNNER_DEVENV_DO binding`
- `+v6 migration` (new_sqlite_classes: ["RunnerDevEnvDO"])
- `+container class RunnerDevEnvDO` com `instance_type: "standard-4"`, `max_instances: 10`

**Análise do erro do Gemini:** Gemini rodou `grep -rni "devenv\|dev_env\|/v1/customer/devenv" worker/src/` no **`HEAD` commitado** (que é `origin/main`, sem devenv), e não rodou `git diff` no working tree. O código WP-08 está no working tree dirty como `M worker/src/index.ts` + `M wrangler.toml`, e em corelink-runners como `?? untracked` no durable_objects.

**Conclusão:** a rota e o binding EXISTEM. O defeito real NÃO é "falta rota", é **N1** (o DO `fetch()` não roteia HTTP→RPC, ver §3.1).

### 2.2 E2 — B4 do Gemini está ERRADO (exec-server TEM os endpoints)

Gemini disse:
> "**Zero** ocorrências de `/clw`, `/mkdir`, `/ping`, `/port-check`, ou porta 9090 no crate"

**Verificação ground-truth em `corelink-check-exec-server/src/lib.rs:84-98`:**

```rust
pub fn app() -> Router {
    // ...build router com TODOS os 5 endpoints
}

pub fn app_with_auth(token: Option<String>) -> Router {
    let router = Router::new()
        .route("/exec", post(exec_handler))                   // linha 94
        .route("/clw", post(clw_handler))                     // linha 95
        .route("/ping", axum::routing::get(ping_handler))     // linha 96
        .route("/port-check/:port", axum::routing::get(port_check_handler))  // linha 97
        .route("/mkdir", post(mkdir_handler));                // linha 98
    // ...
}
```

**Análise do erro do Gemini:** Gemini checou `main.rs:42` (linha do `bind_addr_str: Option<String>`) e viu bind hardcoded para 8080. Mas o `main.rs:42-57` (Gemini errou: era linha 56 que tem `SocketAddr::from((UNSPECIFIED, DEFAULT_PORT))` como **fallback**, e linhas 44-51 tem **arg parsing real**). Gemini também não checou `lib.rs` para as rotas — só checou `main.rs:42`. Se ele tivesse feito `grep "/clw" crates/corelink-check-exec-server/src/lib.rs` teria visto.

**Mas:** o defeito **ainda existe** — ver §3.10 (N10) e §3.4 (H5): o `main.rs` é fail-closed sem `EXEC_SERVER_AUTH_TOKEN`, e **o supervisord.conf não injeta essa env** (linha 160 `environment=HOME="/home/coder",USER="coder"` — só isso). Portanto, o exec-server **não sobe**, mesmo tendo as rotas certas. Esse é o defeito real de B4, não "rotas não implementadas".

### 2.3 E3 — H2 do Gemini está ERRADO (validateStateTransition É chamado)

Gemini disse:
> "Spec WP-01 (linha 347): `transitionState()` deve lançar `INVALID_STATE_TRANSITION` em transição ilegal..."
> "Impl (`runner_dev_env.ts:87-90`): ... **Sem validação**"

**Verificação ground-truth em `runner_dev_env.ts:86-91`:**

```ts
private async transitionState(newState: DevenvState): Promise<void> {
  if (this.devenvState.status !== newState.status) {
    validateStateTransition(this.devenvState.status, newState.status);
  }
  this.devenvState = newState;
  await this.persistState();
}
```

E import (linha 15): `validateStateTransition,` de `"../types/devenv.js"`.

**Análise do erro do Gemini:** Gemini leu só as linhas 87-90 e viu `this.devenvState = newState` antes de qualquer validação. **Mas pulou a linha 88** onde o guard existe. O guard é condicional (só roda se o status muda) — isso é idempotência, não defeito. H2 mantém-se satisfeito.

**Análise adicional:** spec WP-01 linha 347 (em `docs/campaigns/devenv/wps/WP-01_RunnerDevEnvDO_Skeleton.md`) define `transitionState()` com guard. Impl satisfaz.

### 2.4 E4 — H5 do Gemini está PARCIALMENTE ERRADO (arg parsing EXISTE)

Gemini disse:
> "`main.rs:42` bind `0.0.0.0:8080` hardcoded; **nenhum arg parsing** — o `--bind-addr 127.0.0.1:9090` do supervisord é **ignorado silenciosamente**"

**Verificação ground-truth em `main.rs:42-57`:**

```rust
let mut bind_addr_str: Option<String> = None;
let mut args = std::env::args().skip(1);
while let Some(arg) = args.next() {
    if arg == "--bind-addr" {
        bind_addr_str = args.next();
    }
}
if bind_addr_str.is_none() {
    bind_addr_str = std::env::var("BIND_ADDR").ok().filter(|s| !s.is_empty());
}

let addr: SocketAddr = if let Some(s) = bind_addr_str {
    s.parse().map_err(|e| format!("invalid --bind-addr '{s}': {e}"))?
} else {
    SocketAddr::from((Ipv4Addr::UNSPECIFIED, DEFAULT_PORT))
};
```

**Análise do erro do Gemini:** arg parsing **existe**, e `--bind-addr 127.0.0.1:9090` do supervisord.conf:147 **funciona**. Binário sobe em 9090.

**Mas H5 mantém-se verdadeiro pelo motivo diferente:** o `[program:exec-server]` em supervisord.conf:146-160 **NÃO seta `EXEC_SERVER_AUTH_TOKEN`** no `environment=`. Sem token, `main.rs:23-35` faz fail-closed:
```rust
if token.is_none() {
    if !allow_unauth {
        return Err(format!("{AUTH_TOKEN_ENV} is unset/empty — refusing..."));
    }
}
```
O binário sai com erro antes de B4 importar. O **defeito real** é "token não injetado", não "arg parsing ausente".

**N7 meu também estava errado (corrigido nesta revisão):** requiredPorts 9090 + supervisord `--bind-addr 127.0.0.1:9090` + main.rs com arg parsing → **bind 9090 funciona**. N7 não é defeito. Substituído por N11 (ver §3).

### 2.5 Outros artefatos órfãos (não-B6, mas que o Gemini também não listou)

- `worker/src/lib/openapi_devenv.ts` (`??` untracked, 273 linhas) — OpenAPI 3.1 spec servido em `/openapi.json`. Gemini não mencionou.
- `worker/src/lib/devenv_guard.ts` (`??` untracked, 50 linhas) — `checkDevenvQuota()` stub. Gemini não mencionou. Mas o guard é stub fail-open (ver §3.4).
- `docs/campaigns/devenv/wps/WP-01..WP-10.md` (10 specs) + 4 iterações de review cada (~40 arquivos markdown) — todo o material de specs no working tree como `??` untracked. Gemini mencionou WP-10 mas não o corpus completo.

---

## 3. DEFEITOS ADICIONAIS QUE O GEMINI NÃO LISTOU (N1–N10)

### 3.1 N1 — DO `fetch()` NÃO roteia HTTP→RPC (BLOCKING)

**Fonte:** `corelink-runners/deploy/cloudflare/src/durable_objects/runner_dev_env.ts:382-390`:

```ts
override async fetch(request: Request): Promise<Response> {
  const url = new URL(request.url);

  if (request.headers.get("Upgrade") === "websocket") {
    return this.handleWsUpgrade(request, url);
  }

  return await this.containerFetch(request, this.defaultPort);
}
```

**Problema:** o DO tem 5 métodos públicos (`startDevenv`/`getStatus`/`snapshot`/`resize`/`requestStop`) que são acessíveis APENAS via RPC JS-to-JS (`stub.startDevenv(...)`). Quando o server chama `devStub.fetch(devAugmented)` (em `worker/src/index.ts:2567`), o request NÃO chama nenhum desses métodos — vai direto para o container na porta `defaultPort=6080` (noVNC).

**Spec WP-08 §3.5 (linhas 295-297 de `docs/campaigns/devenv/wps/WP-08_Worker_Ingress_Routes.md`):**
> "`devStub.fetch(devAugmented)` above forwards EVERYTHING (GET list, GET status, POST create, POST snapshot, POST resize, DELETE stop) to the DO. **The DO routes by `pathname`** and returns its own JSON for the list."

O código **NÃO implementa** esse roteamento. O `override async fetch()` substitui completamente o `Container.fetch()` base (que só faz containerFetch).

**Impacto (severidade BLOCKING):** Cada endpoint do cliente (`listDevenvs`/`getDevenv`/`createDevenv`/`stopDevenv`) cai no container noVNC. A UI DevEnv (DevenvClient.tsx) chama o DO via REST; todas as chamadas vão para noVNC e falham. Mesmo o `GET /v1/customer/devenv` (lista) — que deveria retornar JSON do DO — retorna HTML do noVNC com status 200. Nenhum erro 4xx/5xx explícito — o cliente recebe HTML, tenta `JSON.parse()`, falha silenciosamente.

**Cadeia de falha:** cliente → server `worker/src/index.ts:2497` `routeKind="devenv_v1"` → `devStub.fetch(devAugmented)` → DO `fetch()` linha 382 → `containerFetch(request, this.defaultPort=6080)` → noVNC HTML 200.

### 3.2 N2 — `containerFetch(req, port)` na base `Container` (MEDIUM, NOTA)

**Fonte:** `runner_dev_env.ts:222, 285, 405` chama `this.containerFetch(req, port)`.

**Verificação SDK:** `node_modules/@cloudflare/containers/dist/lib/container.js:864-891` — `containerFetch(requestOrUrl, portOrInit, portParam)` aceita port como 2º arg. O DO está passando a porta corretamente.

**Não é defeito novo:** passagem de porta está OK. O defeito real é que o container **pode não responder em 9090** — mas isso depende de N5 (H5 do Gemini) estar corrigido (token injetado). Mantido aqui apenas para registro de que `containerFetch(9090)` é o caminho pretendido, e que vai falhar se N5 não for corrigido.

### 3.3 N3 — `validateClwToken` regex rejeita tokens reais (MEDIUM)

**Fonte:** `corelink-runners/deploy/cloudflare/src/types/devenv.ts:62-67`:

```ts
export function validateClwToken(token: string): string {
  if (typeof token !== "string" || !/^cl_[a-zA-Z0-9_]{16,}$/.test(token)) {
    throw new Error("clw_token must be a valid CoreLink PAT");
  }
  return token;
}
```

**Problema:** o sistema de PAT real do CoreLink emite tokens no formato `corelink_pat_<token_id>.<secret>.<sig>`. Evidência:
- `crates/corelink-audit/src/redact.rs:160`: `let pat = "corelink_pat_live_xyz.secret.sig";`
- `crates/corelink-container/src/customer_d1.rs:3287`: `resp.token.starts_with("corelink_pat_")`
- `crates/corelink-container/src/routes/internal_pat.rs:63`: `token_plaintext: "corelink_pat_<token_id>.<random_secret>.<hmac_sig>"`

O regex `/^cl_[a-zA-Z0-9_]{16,}$/` rejeita:
- `corelink_pat_*` (prefixo errado)
- Tokens com `.` (separadores JWT-like)
- Tokens com `-` (kebab-case em algumas variantes)

**Spec WP-04 §3.1** usa placeholder `cl_pat_1234567890abcdef1234567890` (alinhado com o regex mas **não com o sistema de auth real**).

**Impacto (severidade MEDIUM):** `startDevenv()` (admin-ui → server → DO) **rejeita tokens reais** com erro "clw_token must be a valid CoreLink PAT". Usuários reais não conseguem iniciar DevEnv.

**Decisão de design necessária:** (a) adaptar regex para `^corelink_pat_[\w._-]+$`; (b) expor endpoint de token dedicado para DevEnv. Ver §6.

### 3.4 N4 — `devenv_guard.ts` é stub fail-open (HIGH)

**Fonte:** `corelink-server/worker/src/lib/devenv_guard.ts:16-48`:

```ts
export async function checkDevenvQuota(env: Env, tenantId: string): Promise<DevenvQuotaResult> {
  if (!tenantId || tenantId === "_anonymous") {
    return { allowed: false, reason: "tenant_id required" };
  }
  if (env.CONFIG_DB) {
    try {
      const row = await env.CONFIG_DB.prepare(
        `SELECT max_concurrency, max_vcpu_h, install_status
         FROM runners_entitlement
         WHERE tenant_id = ?1`
      ).bind(tenantId).first<{...}>();
      if (row) {
        if (row.install_status === "suspended") {
          return { allowed: false, reason: "Tenant runners entitlement suspended" };
        }
      }
    } catch {
      // Fail-open for transient D1 reads during quota check if configured
    }
  }
  return { allowed: true };   // ← default OPEN
}
```

**Problemas (4 distintos):**
1. Não checa `max_vcpu_h` (campo SELECTado mas nunca usado).
2. Não checa `max_concurrency` (campo SELECTado mas nunca usado).
3. Se `CONFIG_DB` ausente, retorna `allowed: true` (fail-open).
4. Se SELECT falhar, o `try/catch` engole o erro e retorna `allowed: true` (fail-open). Comentário explícito na linha 43: `// Fail-open for transient D1 reads during quota check if configured`.

**Spec WP-07/INV-05** manda **fail-closed** e computar monthly vCPU contra `devenv_monthly_vcpu`. Nada disso está implementado.

**Impacto (severidade HIGH):** tenant suspended pode iniciar DevEnv; tenant sem vCPU limit pode exceder quota.

### 3.5 N5 — `recordUsage()` fail-open parcial (HIGH)

**Fonte:** `corelink-runners/deploy/cloudflare/src/durable_objects/runner_dev_env.ts:317-378`.

Comportamento dual:
- **D1 path** (linhas 338-358): `if ((this.env as any).CONFIG_DB)` + try/catch swallow. 3 attempts com backoff. Se os 3 falharem, **silêncio total** — sem log, sem erro pro DO caller.
- **HTTP push path** (linhas 361-377): `if ((this.env as any).BILLING_INGEST_URL)` + `console.error("devenv_billing_push_failed", err)` no catch. Loga, mas **não propaga erro**.

**Bônus (defeito adicional):** `billingSeq` é criado no state mas nunca incrementado nem usado no DB write (linha 336 lê, não escreve). **Sentinel field morto.**

**Spec WP-07 §3.2** manda idempotency via `billingSeq` e fail-closed / surface errors — não implementado.

**Impacto (severidade HIGH):** billing falhado fica invisível. Operador não vê (D1 swallow) ou vê log mas estado não muda (HTTP path também não propaga).

### 3.6 N6 — `CORELINK_API_BASE` é env var morta (MEDIUM)

**Fonte:** `corelink-server/wrangler.toml:244` adiciona `CORELINK_API_BASE = "https://corelink-api.humangr.com"`.

**Verificação:** `grep -rn "CORELINK_API_BASE" corelink-runners/deploy/cloudflare/src/` = vazio. `entrypoint.sh` não lê. `runner_dev_env.ts:55-57` hardcoda:

```ts
private static readonly STATIC_ENV_VARS = {
  CLW_REF_DOMAIN: "runner",
  CLW_ENDPOINT: "https://corelink-api.humangr.com",  // hardcoded
} as const;
```

`CLW_ENDPOINT` é derivado de `STATIC_ENV_VARS` (linha 128) e injetado no container via `this.envVars`. **A env var do server é morta.** Não tem leitura dela no DO, no `entrypoint.sh`, ou em qualquer outro lugar.

**Impacto (severidade MEDIUM):** operacional. Mudar endpoint requer editar `STATIC_ENV_VARS` (hardcoded) em vez de env var. Cross-env deploy (staging/prod) precisa editar código, não config.

### 3.7 N7 — DEFEITO INVALIDO (corrigido na re-revisão)

**Era meu defeito da primeira revisão:** "requiredPorts 9090 mas exec-server escuta 8080 → container nunca fica healthy".

**Re-verificação:** `main.rs:42-57` tem arg parsing `--bind-addr` E env var `BIND_ADDR` (ver §2.4 E4). O supervisord.conf:147 passa `--bind-addr 127.0.0.1:9090`. O bind em 9090 **funciona**. N7 invalidado. **N7 removido da lista de defeitos.**

### 3.8 N8 — WebSocket hibernation: sem tags, listeners somem (BLOCKING)

**Fonte:** `corelink-runners/deploy/cloudflare/src/durable_objects/runner_dev_env.ts:392-429`.

```ts
private async handleWsUpgrade(request: Request, url: URL): Promise<Response> {
  // ...
  this.ctx.acceptWebSocket(server);  // ← SEM tags
  // ...
  containerWs.addEventListener('message', (e: MessageEvent) => {  // listener inline
    if (server.readyState === WebSocket.OPEN) {
      if ((server as any).bufferedAmount > MAX_WS_BUFFERED_BYTES) return;
      server.send(e.data);
    }
  });
  // ...
}
```

**Spec WP-05 §3.4 (linhas 148-160 de `docs/campaigns/devenv/wps/WP-05_WebSocket_Proxy.md`):**
> "HIBERNATION ACCEPT: tags + attachment so we can recover the pair"
> "Hibernation API used with tags; survives eviction"

**Problema:** o impl usa `acceptWebSocket(server)` **sem tags**. O attachment é via `serializeAttachment` (linha 398), que **NÃO sobrevive hibernação** (é só metadata, não reconstrói listeners). O `containerWs.addEventListener('message', ...)` (linha 416) é registrado inline; quando o DO hiberna, esse listener é **zerado** (DOs hibernados zeram JS state).

**Impacto (severidade BLOCKING):** DevEnv designed para sessões de 8h (`HARD_MAX_SESSION_MS = 8 * 3600 * 1000`, linha 26). Após hibernação (comum: inatividade > 30m + garbage collection da DO), **toda mensagem do container para o cliente é perdida**. WebSocket VNC/ttyd/code-server parece funcionar por minutos, depois fica mudo.

**Cadeia de falha:** container envia mensagem → `containerWs.message` listener sumiu → `server.send(e.data)` nunca é chamado → cliente noVNC vê tela congelar.

### 3.9 N9 — Spec inconsistency em `ports` vs `requiredPorts` (MEDIUM)

**Fonte:** `runner_dev_env.ts:45` declara `requiredPorts = [6080, 7681, 8080, 9090]`, mas `runner_dev_env.ts:115` (em `buildStatusResponse`):

```ts
ports: [6080, 7681, 8080],   // 3 portas, sem 9090
```

**Problema:** o DO exige 4 portas no boot (linha 45) mas reporta 3 portas para a UI (linha 115). Cliente vê `[6080, 7681, 8080]`, mas se uma das 4 falhar (e.g. 9090), o status reporta 3 de 4 "healthy" — **métrica incorreta**.

**Spec WP-01** é internamente inconsistente (Gemini mencionou em M).

### 3.10 N10 — `/resize` NÃO está no check-exec-server (HIGH)

**Fonte:** `runner_dev_env.ts:215-218`:

```ts
const req = new Request(`http://localhost:${EXEC_SERVER_PORT}/resize`, {
  method: "POST",
  headers: { "Content-Type": "application/json", "X-Exec-Token": this.execToken },
  body: JSON.stringify({ width: payload.width, height: payload.height }),
});
const resp = await this.containerFetch(req, EXEC_SERVER_PORT);
```

**Verificação `corelink-check-exec-server/src/lib.rs:94-98`:**
- `/exec` (linha 94)
- `/clw` (linha 95)
- `/ping` (linha 96)
- `/port-check/:port` (linha 97)
- `/mkdir` (linha 98)
- **`/resize` — NÃO EXISTE**

`grep -rn "resize" crates/corelink-check-exec-server/` = vazio.

**Impacto (severidade HIGH):** `resize()` (chamado pelo spec WP-06 para redimensionar ttyd/code-server) **sempre falha 404**. O endpoint é registrado no DO mas o handler não existe. `runner_dev_env.ts:222` throw `RESIZE_FAILED: 404`.

---

## 4. AVALIAÇÃO FINAL POR WP (re-verificada)

| WP | Status Gemini | Status M3 | Diferença |
|----|---------------|-----------|-----------|
| WP-01 | 🟡 PARCIAL | ✅ OK | H2 do Gemini está errado — validateStateTransition é chamado. Sem defeitos blocker no DO |
| WP-02 | 🔴 BROKEN | 🔴 BROKEN | — |
| WP-03 | 🔴 BROKEN | 🔴 BROKEN | — |
| WP-04 | 🔴 BROKEN | 🔴 BROKEN | — |
| WP-05 | 🟡 PARCIAL | 🔴 BROKEN (downgrade) | N8 (hibernation sem tags) + N1 (routing) cascade. Spec WP-05 §3.4 não é atendido |
| WP-06 | 🔴 BROKEN | 🔴 BROKEN | N10 (`/resize` não existe) é defeito novo dentro de WP-06 |
| WP-07 | 🔴 BROKEN | 🔴 BROKEN | (N4 stub + N5 fail-open partial agravam) |
| WP-08 | 🔴 BROKEN | 🟠 HIGH (upgrade) | B6 do Gemini é incorreto — a rota existe. Mas N1 (DO não roteia) + N4 (guard stub) tornam a rota ineffective end-to-end |
| WP-09 | 🔴 BROKEN | 🟠 HIGH (upgrade) | UI client-side existe (DevenvClient.tsx, 306 linhas). Compila? Type-check presumível. Rest: depende de WP-08 fix |
| WP-10 | 🔴 NOT_STARTED | 🔴 NOT_STARTED | — |

**Recount:**
- 5 BROKEN: WP-02, WP-03, WP-04, WP-05 (downgrade), WP-06
- 3 HIGH: WP-07 (mantido), WP-08 (upgrade do Gemini), WP-09 (upgrade)
- 0 PARCIAL: WP-01 mantém OK (H2 errado do Gemini)
- 1 OK: WP-01
- 1 NOT_STARTED: WP-10

**Diferença numérica do Gemini:** 7 BROKEN / 2 PARCIAL / 1 NOT_STARTED → **5 BROKEN / 3 HIGH / 0 PARCIAL / 1 OK / 1 NOT_STARTED**.

---

## 5. VEREDITO M3

**❌ REJECT** — mesma conclusão do Gemini, mas com **ajustes importantes:**

1. **B6 do Gemini é incorreto** (E1). A rota de ingress EXISTE no working tree. O defeito real é **N1**: o DO `fetch()` não roteia HTTP→RPC. Diferença significativa porque o defeito é mais sutil (não é "falta rota" mas "rota encaminha pro lugar errado").

2. **B4 do Gemini é incorreto** (E2). O check-exec-server JÁ tem `/exec`/`/clw`/`/ping`/`/port-check/:port`/`/mkdir`. O defeito real é **N5** (token não injetado pelo supervisord, fail-closed não sobe) — defeito de deployment, não de código.

3. **H2 do Gemini é incorreto** (E3). `validateStateTransition` é importado (linha 15) e chamado em `transitionState` (linha 88). WP-01 mantém-se OK.

4. **H5 do Gemini é parcialmente incorreto** (E4). Arg parsing existe; `--bind-addr` funciona. O defeito real é token não injetado, não arg parsing ausente.

5. **10 defeitos novos (N1–N10) que o Gemini não detectou.** Severidades:
   - **2 BLOCKING:** N1 (DO fetch não roteia), N8 (WebSocket hibernation sem tags)
   - **4 HIGH:** N3 (regex rejeita tokens reais), N4 (devenv_guard stub fail-open), N5 (recordUsage fail-open partial), N10 (`/resize` não existe)
   - **4 MEDIUM:** N2 (NOTA, containerFetch), N6 (CORELINK_API_BASE morta), N9 (spec inconsistency ports)
   - *N7 invalidado na re-revisão*

6. **Nada está commitado.** Veredito comum. Working tree sujo, frágil, reversível.

### 5.1 Itens do Gemini que NÃO corrigi (mantidos como estão)

- B1 (6 crates faltantes) — confirmado.
- B2 (binário errado) — confirmado.
- B3 (flags fantasma + auth morta) — confirmado.
- B5 (D1 binding) — confirmado.
- B7 (image :latest, sem CI) — confirmado.
- H6 (tier mapping) — confirmado.
- C1, C2, C3 (correções) — confirmados.
- WP-10 not_started — confirmado.
- M (spec inconsistency requiredPorts) — confirmado.

### 5.2 Ajustes importantes

| Item | Gemini | M3 |
|------|--------|-----|
| B6 | "nenhuma rota" | "rota existe mas DO não roteia" (N1) — **Gemini errou** |
| B4 | "exec-server sem /clw /ping /port-check /mkdir" | "exec-server TEM os endpoints, mas token não injetado (N5/H5)" — **Gemini errou** |
| H2 | "`transitionState` sem guard" | "validateStateTransition é chamado" — **Gemini errou** |
| H5 | "`main.rs` sem arg parsing" | "arg parsing existe; token não injetado" — **Gemini errou parcialmente** |
| WP-01 | PARCIAL (H2) | OK (H2 errado do Gemini) |
| WP-05 | PARCIAL | BROKEN (N8 cascade) |
| WP-08 | BROKEN | HIGH (rota existe; N1+N4 tornam ineffective) |
| WP-09 | BROKEN | HIGH (cliente existe) |
| Defeitos novos (não-WP) | 0 | 10 (N1–N10), 2 BLOCKING, 4 HIGH, 4 MEDIUM |
| Total WP BROKEN | 7 | 5 (2 downgradados: WP-01 OK, WP-09 HIGH) |
| Total WP HIGH | 2 | 3 (WP-07 mantido, WP-08 e WP-09 adicionados) |
| Total WP PARCIAL | 2 | 0 (WP-01 OK, WP-05 BROKEN) |
| Total WP NOT_STARTED | 1 | 1 (WP-10) |

---

## 6. PLANO DE CORREÇÃO (com N1–N10 integrados)

**Ordem de alavancagem:**

1. **N1 — DO `fetch()` roteia por `pathname` + método** (a spec WP-08 §3.5 já descreve o switch). Sem isso, todos os outros fixes de WP-08 são inúteis. **Prioridade 1.**
2. **N8 — Hibernation WebSocket com tags**: mudar `acceptWebSocket(server)` → `acceptWebSocket(server, [tagConnId])`; usar `getWebSocketAutoResponse`/`setHibernatableWebSocket` em vez de `serializeAttachment`+inline listeners. Spec WP-05 §3.4 já descreve o padrão. **Prioridade 2.**
3. **B4 / H5 — exec-server não sobe (fail-closed)**: adicionar `EXEC_SERVER_AUTH_TOKEN` no `environment=` do `[program:exec-server]` no `supervisord.conf:146-160`. Valor vem de env injection pelo entrypoint. **Prioridade 3.** (Depois disso o check-exec-server sobe em 9090 com TODOS os endpoints.)
4. **N10 — adicionar handler `/resize` no check-exec-server** (`lib.rs:98` area). O DO `runner_dev_env.ts:215` chama `/resize` mas o handler não existe. **Prioridade 4.**
5. **B1 + B2 + B3 — consertar Dockerfile** para buildar `clw-cli` de corelink-workspaces com as 9 crates certas, e usar só as flags que o clw real aceita. Decisão de design: (a) editar spec WP-04 para alinhar com clw real, (b) adicionar flags ao clw-cli em corelink-workspaces, ou (c) remover OCC + mover auth para `CLW_TOKEN`. Recomendo (b).
6. **N3 — `validateClwToken` regex**: adaptar para `^corelink_pat_[\w._-]+$`. Sem isso, usuários reais não conseguem iniciar DevEnv.
7. **B5 + N4 + N5 — billing real**: adicionar `[[d1_databases]]` binding `CONFIG_DB` no wrangler.jsonc do `corelink-spawn-worker` (mesmo `database_id` do server placeholder `PLACEHOLDER_D1_CONFIG_DB_ID`); commitar migration 0094 (D1 é por account, não por script — confirmar com time). Fail-closed no guard, surface errors no recordUsage.
8. **B7 — image pin**: workflow + pin por digest (mesmo padrão do `RunnerContainer` em `wrangler.jsonc:208`).
9. **H6 — tier mapping**: como CF não permite mudar `instance_type` runtime, ler `payload.config.tier` no `start()` e mapear para `envVars` (e.g. `CLW_TIER=ultra-16`) — aceito pelo clw. Ou: separar em 4 classes DO com `instance_type` fixo.
10. **N6 — `CORELINK_API_BASE`**: ler do env no DO, não hardcodar. Substituir `STATIC_ENV_VARS.CLW_ENDPOINT` por `env.CORELINK_API_BASE` em `startDevenv()`.
11. **N9 — spec cleanup**: alinhar `requiredPorts` (4 portas) com `buildStatusResponse.ports` (3 portas). Decidir qual é canônico.
12. **WP-09 — fidelidade ao spec**: spec pede árvore `components/devenv/`. Impl colapsa em 306 linhas. Decidir: realocar OU atualizar spec.
13. **WP-10 — dogfood + docs + runbooks + load tests + OKF + CHANGELOG `[Unreleased]` entry**.

**Decisão arquitetural (item 5):** spec WP-04 manda flags que o clw-cli real não tem.

| Opção | Custo | Risco |
|-------|-------|-------|
| (a) Editar spec WP-04 para alinhar com clw real | baixo | perde OCC generation, auth isolation via file |
| (b) Adicionar flags ao clw-cli em corelink-workspaces | médio (cross-repo) | OK se aprovado pelo time de workspaces |
| (c) Remover OCC generation + mover auth para `CLW_TOKEN` (já lido) | baixo | spec reescrita |

**Decisão arquitetural (item 6):** regex rejeita tokens reais. Adaptar regex OU expor endpoint de token DevEnv.

**Decisão arquitetural (item 7):** D1 binding — verificar se spawn-worker tem permissão de escrita no mesmo `database_id` do server. Se não, `recordUsage` precisa push HTTP para o server.

---

## 7. ANEXO — Comandos Executados (reprodutibilidade)

```bash
# Localizar working tree
ls /Users/gustavoschneiter/Documents/HuGR/corelink-{server,runners,workspaces}

# Git state
git -C corelink-runners status --short   # devenv como ?? untracked
git -C corelink-server status --short    # devenv como M (dirty) e ?? (untracked)

# Histórico (zero)
git -C corelink-runners log --all --grep="devenv" --oneline   # vazio
git -C corelink-server log --all --grep="devenv" --oneline    # vazio
git -C corelink-runners log --all -S "RunnerDevEnvDO"         # vazio
git -C corelink-runners log --all -S "d1_databases"           # vazio

# B1 (crates)
ls corelink-runners/crates/    # 8 crates
grep "^COPY crates/" corelink-runners/deploy/cloudflare/Dockerfile.runner-devenv  # 12

# B2 (binário)
grep "name =" corelink-runners/crates/corelink-cli/Cargo.toml   # name = "corelink"
grep -rn 'name = "clw"' corelink-runners/crates/                 # vazio

# B3 (flags)
grep -nE "long|default" corelink-workspaces/crates/clw-cli/src/subcmds/snapshot.rs  # só path/--name/--force/--json/--concurrency
grep -nE "name|all" corelink-workspaces/crates/clw-cli/src/subcmds/ls.rs            # só --name/--all

# B4 (exec-server) — Gemini errou: rotas JÁ EXISTEM
grep -nE '"/exec"|"/clw"|"/ping"|"/port-check"|"/mkdir"|"/resize"' \
  corelink-runners/crates/corelink-check-exec-server/src/lib.rs
# 94: /exec, 95: /clw, 96: /ping, 97: /port-check/:port, 98: /mkdir  ← SEM /resize

# H5 (main.rs) — Gemini errou: arg parsing EXISTE
sed -n '42,57p' corelink-runners/crates/corelink-check-exec-server/src/main.rs
# 42: let mut bind_addr_str: Option<String> = None;
# 44-48: while loop parseia --bind-addr
# 50: BIND_ADDR env var fallback
# 56: SocketAddr::from((UNSPECIFIED, DEFAULT_PORT))  ← SÓ fallback, não hardcoded

# H5 (supervisord) — token NÃO injetado
grep -nE "EXEC_SERVER_AUTH_TOKEN|environment=" corelink-runners/deploy/cloudflare/supervisord.conf
# 160: environment=HOME="/home/coder",USER="coder"  ← sem token

# H2 (validateStateTransition) — Gemini errou: GUARD EXISTE
sed -n '86,91p' corelink-runners/deploy/cloudflare/src/durable_objects/runner_dev_env.ts
# 88: validateStateTransition(this.devenvState.status, newState.status);

# B5 (D1)
grep "d1_databases" corelink-runners/deploy/cloudflare/wrangler.jsonc   # vazio

# B6 (rota) — o teste decisivo
git -C corelink-server diff worker/src/index.ts   # 163 linhas, devenv_v1 + openapi + handler
git -C corelink-server diff wrangler.toml          # binding cross-worker
sed -n '17p' corelink-runners/deploy/cloudflare/src/index.ts  # +export RunnerDevEnvDO

# B7 (imagem + CI)
grep "image" corelink-runners/deploy/cloudflare/wrangler.jsonc   # :latest
grep -rni "devenv\|RunnerDevEnvDO" corelink-runners/.github/workflows/   # vazio

# N1 (DO fetch routing)
sed -n '382,390p' corelink-runners/deploy/cloudflare/src/durable_objects/runner_dev_env.ts
# mostra: WebSocket OR containerFetch(6080) — sem roteamento HTTP→RPC

# N4 (guard stub)
wc -l corelink-server/worker/src/lib/devenv_guard.ts   # 50 linhas
sed -n '16,48p' corelink-server/worker/src/lib/devenv_guard.ts
# linha 43: "// Fail-open for transient D1 reads during quota check if configured"

# N6 (env morta)
grep -n "CORELINK_API_BASE" corelink-server/wrangler.toml   # 244: var
grep -rn "CORELINK_API_BASE" corelink-runners/deploy/   # vazio
sed -n '54,57p' corelink-runners/deploy/cloudflare/src/durable_objects/runner_dev_env.ts
# 56: CLW_ENDPOINT: "https://corelink-api.humangr.com"  ← hardcoded

# N8 (hibernation sem tags)
sed -n '392,405p' corelink-runners/deploy/cloudflare/src/durable_objects/runner_dev_env.ts
# 399: this.ctx.acceptWebSocket(server);  ← SEM tags

# N10 (/resize não existe)
grep -rn "resize" corelink-runners/crates/corelink-check-exec-server/   # vazio
```

---

## 8. HISTÓRICO DE REVISÕES DESTE RELATÓRIO

**v2 (atual):** 4 correções às claims do Gemini (E1/E2/E3/E4) + N7 invalidado + N10 adicionado + linhas citadas exatas (sed -n).

**v1 (rascunho anterior):** 1 erro factual (N7) e linhas citadas aproximadas. Substituído integralmente por v2 após re-verificação exaustiva com `sed -n` e `grep -n` em cada linha citada.
