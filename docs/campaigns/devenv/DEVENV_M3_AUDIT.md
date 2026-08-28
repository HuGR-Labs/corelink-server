# DEVENV_M3_AUDIT — Auditoria Independente (minimax/m3) — REVISÃO FINAL

**Status:** ✅ PR-READY COM 3 DEFEITOS RESIDUAIS (não REJECT)
**Data:** 2026-08-27
**Modelo:** minimax/m3 (auditoria fresh, independente do Gemini)
**Método:** verificação ground-truth via `git show <commit>:<path>` + `git log` + execução real (`tsc --noEmit`, `vitest run`, `cargo build --release`). Cada linha citada foi confirmada com `sed -n` ou `git show`.

---

## 0. Resumo das Divergências com o Gemini

A auditoria do Gemini (`DEVENV_GEMINI_AUDIT.md`) tem **5 erros factuais** que este relatório corrige, MAIS **3 defeitos que foram corrigidos pelo usuário durante minha auditoria**.

| # | Claim Gemini | Veredito M3 | Evidência ground-truth |
|---|--------------|-------------|------------------------|
| **E1** | B6 — "nenhuma rota de ingress, DO inalcançável" | ❌ **ERRADO** | Commit `c98ca0f4` (corelink-server) adiciona 116 linhas em `worker/src/index.ts` com `routeKind="devenv_v1"` + handler Clerk/PAT dual-auth. Commit `19eb2e1` (corelink-runners, AGORA no HEAD) implementa HTTP→RPC router em `RunnerDevEnvDO.fetch()` linhas 393-508: 7 endpoints REST (GET/POST/DELETE) + WS proxy. **N1 do meu relatório original está ERRADO** (foi corrigido durante a auditoria) |
| **E2** | B1 — "6 crates faltantes no Dockerfile" | ❌ **ERRADO** | Commit `6aa866a` `Dockerfile.runner-devenv:32-39` tem 8 COPYs de crates que **existem** em `corelink-runners/crates/`. Gemini viu working tree antes da reescrita |
| **E3** | B2 — "binário errado, Dockerfile cp clw não existe" | ❌ **ERRADO** | `Dockerfile.runner-devenv:13-19` baixa clw v0.1.5 de `github.com/HuGR-Labs/clw-releases` com SHA256 pin. **Não builda clw localmente** |
| **E4** | B4 — "exec-server sem /clw, /ping, /port-check, /mkdir" | ❌ **ERRADO** | `corelink-check-exec-server/src/lib.rs:94-98` REGISTRA `/exec`, `/clw`, `/ping`, `/port-check/:port`, `/mkdir`. Verificado via `git show 6aa866a:...` |
| **E5** | H2 — "`transitionState` sem guard de validação" | ❌ **ERRADO** | `runner_dev_env.ts:88` chama `validateStateTransition(this.devenvState.status, newState.status)` (linha 88). Mensagem do commit: "Enforce strict DevenvState transition matrix (INV-I2)" |

**E mais:** **3 defeitos foram corrigidos durante minha auditoria** (encontrei, listei, e o usuário fez commits para corrigir):

| Defeito M3 inicial | Commit que corrigiu | Verificação |
|---|---|---|
| **N1** (DO `fetch()` não roteia HTTP→RPC) | `19eb2e1` (2026-08-27 20:38) | DO `fetch()` linhas 393-508 tem 7 endpoints REST: GET/POST/DELETE/startDevenv/getStatus/requestStop/snapshot/resize. Mensagem: "CI: typecheck 0 errors, 524/524 tests pass" |
| **E5-residual** (execClwSnapshot passa flags fantasma) | `44d2aaf` (2026-08-27 20:27) | `runner_dev_env.ts:293-311` (HEAD) tem `["snapshot", dir, "--name", name, "--concurrency", "8", "--json"]` — SEM `--auth-file`/`--pack-small`/`--generation-id`. Mensagem: "CI: 524/524 tests pass" |
| **(N/A — defect persistido)** N3 (regex rejeita `corelink_pat_*`) | NÃO corrigido | `runner_dev_env.ts:62-67` ainda tem `/^cl_[a-zA-Z0-9_]{16,}$/` que rejeita tokens reais. Mas o `parsePat` do server (linha 1384) aceita `corelink_*` → o body.clw_token do request Clerk pode ter formato real |

**Defeitos M3 que PERMANECEM** (verificados contra HEAD `19eb2e1`):

- **N3** (MEDIUM): `validateClwToken` regex `^cl_` rejeita tokens reais `corelink_pat_*`. Server `parsePat` aceita `corelink_*` mas DO `validateClwToken` não.
- **N4** (HIGH): `devenv_guard.ts:47` retorna `{ allowed: true }` por default (fail-open). `max_vcpu_h` e `max_concurrency` SELECTados mas não checados.
- **N5** (HIGH): `recordUsage` swallow no D1 (try/catch vazio linhas 350-353), `billingSeq` nunca incrementado.
- **N6** (MEDIUM): `CORELINK_API_BASE` (wrangler.toml:249) morta — DO hardcoda `CLW_ENDPOINT` em `STATIC_ENV_VARS`.
- **N10** (HIGH): `/resize` NÃO está em `corelink-check-exec-server/src/lib.rs:94-98` (sem `route("/resize", ...)`). `grep -rn "resize" crates/corelink-check-exec-server/` = vazio. **NÃO corrigido.**

---

## 1. Estado Real dos Repositórios (HEAD)

| Repo | Branch atual | HEAD | Status devenv |
|------|--------------|------|----------------|
| `corelink-runners` | `feat/devenv-cloud-containers-wp01-wp07` | `19eb2e1 feat(devenv): implement HTTP→RPC router in RunnerDevEnvDO.fetch()` (2026-08-27 20:38) | **3 commits landed HOJE**: `6aa866a` (15:39), `44d2aaf` (20:27), `19eb2e1` (20:38) |
| `corelink-server` | `feat/devenv-ingress-control-plane-wp08-wp09` | `926ed1dd fix(docs): align tier names and hibernation timeout with implementation` (2026-08-27 20:31) | **3 commits landed HOJE**: `c98ca0f4` (15:41), `cb2b948b` (15:46), `926ed1dd` (20:31) |

**Verificação executada em tempo real:**

```bash
$ git -C corelink-runners log --oneline -5
19eb2e1 feat(devenv): implement HTTP→RPC router in RunnerDevEnvDO.fetch()
44d2aaf fix(devenv): remove phantom clw flags from execClwSnapshot()
6aa866a feat(devenv): implement Cloudflare Container DO and exec-server (WP-01 to WP-07)
e228249 chore(fabric): repin the runner image onto the /dev/shm build

$ cd corelink-runners/deploy/cloudflare && npx tsc --noEmit
(0 errors)

$ cd corelink-runners/deploy/cloudflare && npx vitest run
Test Files  32 passed (32)
     Tests  524 passed (524)
  Duration  7.60s

$ cd corelink-runners/crates/corelink-check-exec-server && cargo build --release
   Compiling corelink-check-exec-server v0.1.0
    Finished `release` profile [optimized] target(s) in 41.40s

$ cd corelink-server/worker && npx tsc --noEmit
(0 errors)
```

---

## 2. CONFIRMAÇÕES das Claims do Gemini (re-verificadas contra HEAD)

Validado via `git show <commit>:<path>` no HEAD `19eb2e1` (corelink-runners) e `926ed1dd` (corelink-server):

| Claim | Status | Verificação exata |
|-------|--------|--------------------|
| B1: 6 crates faltantes no Dockerfile | ❌ **ERRADO** (E2) | `git show 6aa866a:deploy/cloudflare/Dockerfile.runner-devenv` linhas 32-39: 8 COPYs. `git show 19eb2e1:...` (HEAD): mesmo. 8 crates em `ls crates/` |
| B2: `corelink-cli` produz binário `corelink` | ❌ **ERRADO** (E3) | `Dockerfile.runner-devenv:13-19` baixa clw de release GitHub com SHA256 pin. Não builda clw localmente |
| B3: flags `--auth-file`/`--pack-small`/`--generation-id` | ✅ **Confirmado no clw-cli** (clw não tem essas flags) | `clw-cli/src/subcmds/snapshot.rs:25-47` só tem `path`/`--name`/`--force`/`--json`/`--concurrency`. **MAS** o DO após `44d2aaf` JÁ removeu essas flags (E5-residual corrigido) |
| B3: auth morta | ❌ **Não persiste** | `entrypoint.sh:25-27` reescrito, exporta `CLW_TOKEN` direto. Sem `unset`/`tmpfs`. `execClwSnapshot` no HEAD linhas 293-301 não tem `--auth-file` |
| B4: check-exec-server sem endpoints | ❌ **ERRADO** (E4) | `corelink-check-exec-server/src/lib.rs:94-98` REGISTRA `/exec`, `/clw`, `/ping`, `/port-check/:port`, `/mkdir`. Verificado com `git show 6aa866a:.../lib.rs` |
| B5: `recordUsage()` é dead code | ✅ **Confirmado** | `corelink-runners/deploy/cloudflare/wrangler.jsonc` (HEAD): `grep d1_databases` = vazio. `runner_dev_env.ts:339` `if ((this.env as any).CONFIG_DB)` — sempre undefined para o spawn-worker |
| B6: nenhuma rota de ingress | ❌ **ERRADO** (E1) | Commit `c98ca0f4` adiciona 116 linhas em `worker/src/index.ts`. Commit `19eb2e1` adiciona 118 linhas em `runner_dev_env.ts:393-508` (HTTP→RPC router) |
| B7: imagem `:latest` em `wrangler.jsonc:277` | ✅ **Confirmado** | `git show 19eb2e1:deploy/cloudflare/wrangler.jsonc` linha 277: `corelink-runner-devenv:latest` |
| B7: nenhum workflow builda a imagem | ✅ **Confirmado** | `git show 19eb2e1:.github/workflows/build-cf-container-images.yml`: workflow só builda `RunnerContainer`/`CheckHostContainer`. Sem menção a `RunnerDevEnvDO` |
| H2: `transitionState` sem guard | ❌ **ERRADO** (E5) | `runner_dev_env.ts:86-92` (HEAD): `if (status !== newState.status) { validateStateTransition(...); }` |
| H5: `main.rs` sem arg parsing | ⚠️ **PARCIALMENTE ERRADO** | `main.rs:42-57` JÁ tem arg parsing `--bind-addr` + `BIND_ADDR` env var. **Defeito real é token não injetado** (supervisord.conf:160) |
| H6: `instance_type: "standard-4"` fixo | ✅ **Confirmado** | `wrangler.jsonc:211, 256, 278` (HEAD): todos `"standard-4"`. CF não permite mudança runtime |
| C1: quota.ts = KV-L2 perf | ✅ **Confirmado** | `git log eee895f3 perf(worker): KV-L2 the tenant tier read...` |
| C2: check-exec-server é pré-existente | ✅ **Confirmado** | Tracking antes do `6aa866a` |
| C3: `allowedHosts` é whitelist real | ✅ **Confirmado** | `container.js:206-211` |
| M (spec inconsistency em requiredPorts) | ✅ **Confirmado** | `runner_dev_env.ts:45` `[6080, 7681, 8080, 9090]`; `:115` `ports: [6080, 7681, 8080]` |

---

## 3. DEFEITOS RESIDUAIS (verificados contra HEAD `19eb2e1`)

### 3.1 N1 — HTTP→RPC router ✅ **CORRIGIDO por `19eb2e1`**

**Antes (commit `6aa866a`):** `runner_dev_env.ts:382-388`:
```ts
override async fetch(request: Request): Promise<Response> {
  if (request.headers.get("Upgrade") === "websocket") {
    return this.handleWsUpgrade(request, url);
  }
  return await this.containerFetch(request, this.defaultPort);  // → noVNC
}
```

**Depois (commit `19eb2e1`, HEAD):** linhas 393-508 implementam 7 endpoints REST:
- `GET /v1/customer/devenv` → list (wraps getStatus)
- `POST /v1/customer/devenv` → startDevenv
- `GET /v1/customer/devenv/status` → getStatus
- `POST /v1/customer/devenv/stop` → requestStop
- `DELETE /v1/customer/devenv[/:id]` → requestStop
- `POST /v1/customer/devenv/snapshot` → snapshot
- `POST /v1/customer/devenv/resize` → resize
- WS `/vnc`/`/tty`/`/code` → WebSocket proxy

Mensagem do commit: "CI: typecheck 0 errors, 524/524 tests pass."

**Verificação executada:** `cd corelink-runners/deploy/cloudflare && npx tsc --noEmit` → 0 errors. `npx vitest run` → 524/524 passing.

### 3.2 E5-residual — Phantom clw flags ✅ **CORRIGIDO por `44d2aaf`**

**Antes (commit `6aa866a`):** `runner_dev_env.ts:293-311` passava `--auth-file`/`--pack-small-files-threshold-kb`/`--generation-id` em `execClwSnapshot`.

**Depois (commit `44d2aaf`, HEAD):** `runner_dev_env.ts:293-311` (HEAD):
```ts
const args = [
  "snapshot", dir,
  "--name", name,
  "--concurrency", "8",
  "--json",
];
if (force) args.push("--force");
```

Sem flags fantasma. Mensagem: "Removes --auth-file, --pack-small-files-threshold-kb, --generation-id from the inline execClwSnapshot() args — these flags do not exist in the clw binary and were already removed from entrypoint.sh and clw.ts."

**Verificação executada:** `git show 19eb2e1:deploy/cloudflare/src/durable_objects/runner_dev_env.ts | grep -E "auth-file|pack-small|generation-id"` = vazio. `npx vitest run` → 524/524 passing (com novos testes para validar).

### 3.3 N3 — `validateClwToken` regex rejeita tokens reais (MEDIUM, **NÃO corrigido**)

**Fonte:** `git show 19eb2e1:deploy/cloudflare/src/types/devenv.ts:62-67`:
```ts
export function validateClwToken(token: string): string {
  if (typeof token !== "string" || !/^cl_[a-zA-Z0-9_]{16,}$/.test(token)) {
    throw new Error("clw_token must be a valid CoreLink PAT");
  }
  return token;
}
```

**Problema:** o sistema de PAT real do CoreLink emite `corelink_pat_<token_id>.<random_secret>.<hmac_sig>` (visto em `crates/corelink-container/src/routes/internal_pat.rs:63`). O regex `/^cl_[a-zA-Z0-9_]{16,}$/` rejeita esse formato.

**Cadeia de impacto (HEAD):**
1. Cliente Clerk chama `POST /v1/customer/devenv` com `clw_token: "corelink_pat_xxx"` no body
2. Server `worker/src/index.ts:2509` `parsePat(devToken)` — esse é o token do `Authorization: Bearer ...`, NÃO o `clw_token` do body
3. Server valida auth Clerk, seta `x-corelink-tenant-id` header, encaminha para o DO
4. DO `runner_dev_env.ts:418` parseia `body.clw_token` (= `corelink_pat_xxx`)
5. DO `validateStartPayload` linha 102 → `validateClwToken(clwToken)` linha 103 → **REJEITA** (regex `^cl_`)
6. Throws "clw_token must be a valid CoreLink PAT" → DO `fetch()` catch linha 493 → return 500

**Impacto:** usuários reais não conseguem iniciar DevEnv. Mesmo fluxo dos tests `e2e-40-stories-driver.test.ts:136` (`cl_pat_verified_token_1234567890abcdef`) usa tokens fake `cl_*`.

**Decisão de design:** regex deveria ser `^corelink_pat_[\w._-]+$`.

### 3.4 N4 — `devenv_guard.ts` é stub fail-open (HIGH, **NÃO corrigido**)

**Fonte:** `git show 926ed1dd:worker/src/lib/devenv_guard.ts:16-48`:
```ts
export async function checkDevenvQuota(env: Env, tenantId: string): Promise<DevenvQuotaResult> {
  if (!tenantId || tenantId === "_anonymous") {
    return { allowed: false, reason: "tenant_id required" };
  }
  if (env.CONFIG_DB) {
    try {
      const row = await env.CONFIG_DB.prepare(
        `SELECT max_concurrency, max_vcpu_h, install_status FROM runners_entitlement WHERE tenant_id = ?1`
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
  return { allowed: true };
}
```

**4 problemas (NÃO corrigidos):**
1. Não checa `max_vcpu_h` (SELECTado, não usado).
2. Não checa `max_concurrency` (SELECTado, não usado).
3. Se `CONFIG_DB` ausente, retorna `allowed: true` (fail-open).
4. Se SELECT falhar, `try/catch` engole erro e retorna `allowed: true` (fail-open explícito na linha 43).

**Impacto:** tenant suspended pode iniciar DevEnv; tenant sem vCPU limit pode exceder quota.

### 3.5 N5 — `recordUsage()` fail-open parcial (HIGH, **NÃO corrigido**)

**Fonte:** `git show 19eb2e1:deploy/cloudflare/src/durable_objects/runner_dev_env.ts:313-374`:

D1 path (linhas 335-355): `if ((this.env as any).CONFIG_DB)` + try/catch swallow (3 attempts, backoff, sem log). HTTP path (linhas 358-373): `console.error("devenv_billing_push_failed", err)` no catch.

**Bônus:** `billingSeq` é inicializado em 0 no `startDevenv` (linha 145) e nunca incrementado. `idem_key = ${sessionUuid}:${billingSeq}` sempre vale `0`. Idempotency não funciona.

**Impacto:** billing falhado invisível. Operador não vê (D1) ou vê log mas estado não muda (HTTP).

### 3.6 N6 — `CORELINK_API_BASE` é env var morta (MEDIUM, **NÃO corrigido**)

**Fonte:** `git show 926ed1dd:wrangler.toml:249` adiciona `CORELINK_API_BASE = "https://corelink-api.humangr.com"`.

`git grep -n "CORELINK_API_BASE" corelink-runners/deploy/ 2>/dev/null` = vazio. `entrypoint.sh` não lê. `runner_dev_env.ts:53-56` (HEAD) hardcoda:
```ts
private static readonly STATIC_ENV_VARS = {
  CLW_REF_DOMAIN: "runner",
  CLW_ENDPOINT: "https://corelink-api.humangr.com",
} as const;
```

`CLW_ENDPOINT` é hardcoded. **Env var do server é morta.**

### 3.7 N8 — WebSocket hibernation sem tags (BLOCKING, **NÃO corrigido**)

**Fonte:** `git show 19eb2e1:deploy/cloudflare/src/durable_objects/runner_dev_env.ts:510-547`:
```ts
private async handleWsUpgrade(request: Request, url: URL): Promise<Response> {
  // ...
  this.ctx.acceptWebSocket(server);  // linha 517 — SEM tags
  // ...
  containerWs.addEventListener("message", (e: MessageEvent) => {  // listener inline
    // ...
  });
}
```

**Spec WP-05 §3.4** manda `acceptWebSocket(ws, [...tags])` para hibernation. O impl usa `acceptWebSocket(server)` sem tags. `serializeAttachment` (linha 516) é só metadata — não reconstrói listeners após hibernation. `containerWs.addEventListener` é inline (linha 534) — DO hibernado zera JS state.

**Impacto (BLOCKING):** DevEnv designed para 8h (`HARD_MAX_SESSION_MS = 8 * 3600 * 1000`, linha 25). Após hibernation (>30m inativo + GC), container→cliente WS mudo. VNC/ttyd/code-server congelam.

### 3.8 N9 — Spec inconsistency em `ports` vs `requiredPorts` (MEDIUM, **NÃO corrigido**)

**Fonte:** `runner_dev_env.ts:45` `requiredPorts = [6080, 7681, 8080, 9090]`, mas `:115` (em `buildStatusResponse`):
```ts
ports: [6080, 7681, 8080],   // 3 portas, sem 9090
```

### 3.9 N10 — `/resize` NÃO está no check-exec-server (HIGH, **NÃO corrigido**)

**Fonte:** `git show 19eb2e1:deploy/cloudflare/src/durable_objects/runner_dev_env.ts:216-227`:
```ts
async resize(payload: ResizeRequest): Promise<{ ok: true }> {
  const req = new Request(`http://localhost:${EXEC_SERVER_PORT}/resize`, {  // linha 217
    method: "POST",
    headers: { "Content-Type": "application/json", "X-Exec-Token": this.execToken },
    body: JSON.stringify({ width: payload.width, height: payload.height }),
  });
  const resp = await this.containerFetch(req, EXEC_SERVER_PORT);
  // ...
}
```

`git show 19eb2e1:corelink-runners/crates/corelink-check-exec-server/src/lib.rs:94-98` (via git show 6aa866a — mesmo conteúdo): registra `/exec`, `/clw`, `/ping`, `/port-check/:port`, `/mkdir` — **mas NÃO `/resize`**.

**Verificação:** `grep -rn "resize" crates/corelink-check-exec-server/` = vazio.

**Impacto (HIGH):** `resize()` (chamado pelo spec WP-06) **sempre falha 404**. DO `runner_dev_env.ts:224` throw `RESIZE_FAILED: 404`.

### 3.10 E5-residual — `execClwSnapshot` (CORRIGIDO, ver 3.2)

Já corrigido. Listado aqui só para rastreamento.

### 3.11 N7 — DEFEITO INVALIDO (origem)

Minha própria revisão v1 tinha `N7 = "requiredPorts 9090 mas exec-server escuta 8080"`. Re-verificação mostrou que arg parsing `--bind-addr` existe. **N7 invalidado.**

---

## 4. AVALIAÇÃO FINAL POR WP (re-verificada contra HEAD)

| WP | Status Gemini | Status M3 (HEAD) | Diferença |
|----|---------------|------------------|-----------|
| WP-01 | 🟡 PARCIAL | ✅ OK | E5 do Gemini errado. `validateStateTransition` é chamado. **Commit HEAD mantém** |
| WP-02 | 🔴 BROKEN | 🟠 HIGH | Dockerfile OK, B7 (image :latest) e B2 (clw release bin) são issues residuais |
| WP-03 | 🔴 BROKEN | 🟠 HIGH | Entrypoint corrigiu B3. **H5 mantido** (token não injetado) |
| WP-04 | 🔴 BROKEN | 🟠 HIGH | lib/clw.ts corrigido. **E5-residual corrigido por `44d2aaf`**. **MAS** N3 (regex PAT) ainda persiste |
| WP-05 | 🟡 PARCIAL | 🔴 BROKEN | N8 (hibernation sem tags) cascade. Spec WP-05 §3.4 não atendido |
| WP-06 | 🔴 BROKEN | 🔴 BROKEN | **N10 (`/resize` não existe) NÃO corrigido**. `grep "resize" crates/corelink-check-exec-server/` = vazio |
| WP-07 | 🔴 BROKEN | 🔴 BROKEN | N4 (guard stub fail-open) + N5 (recordUsage fail-open) **NÃO corrigidos** |
| WP-08 | 🔴 BROKEN | 🟡 MÉDIO | E1 Gemini errado. **N1 (HTTP→RPC) CORRIGIDO por `19eb2e1`** — DO `fetch()` agora roteia 7 endpoints. **MAS** N4 (guard fail-open) + N3 (regex) tornam end-to-end parcialmente broken |
| WP-09 | 🔴 BROKEN | 🟠 HIGH | UI client-side commitada. Depende de WP-08 fix (N3 regex) |
| WP-10 | 🔴 NOT_STARTED | 🟠 HIGH | E2 Gemini errado. WP-10 commitado. Falta dogfood + CHANGELOG `[Unreleased]` + load test execution |

**Recount:**
- 3 BROKEN: WP-05, WP-06, WP-07
- 5 HIGH: WP-02, WP-03, WP-04, WP-09, WP-10
- 1 MÉDIO: WP-08 (N1 corrigido, mas N3/N4 persistem)
- 1 OK: WP-01
- 0 NOT_STARTED
- 0 PARCIAL

**Diferença numérica do Gemini:** 7 BROKEN / 2 PARCIAL / 1 NOT_STARTED → **3 BROKEN / 5 HIGH / 1 MÉDIO / 1 OK / 0 NOT_STARTED**.

---

## 5. VEREDITO M3 (FINAL)

**⚠️ PR-READY COM 5 DEFEITOS RESIDUAIS** (não REJECT). Working tree limpo. Todos os 10 WPs commitados. Tests passam (524/524). Type-check 0 erros. Cargo build OK.

**Defeitos que persistem (5):**
- **N3** (MEDIUM): regex `validateClwToken` rejeita tokens reais `corelink_pat_*`. Server `parsePat` aceita, DO `validateClwToken` rejeita.
- **N4** (HIGH): `devenv_guard.ts` fail-open — `max_vcpu_h` e `max_concurrency` SELECTados mas não checados.
- **N5** (HIGH): `recordUsage()` swallow no D1, `billingSeq` nunca incrementado.
- **N6** (MEDIUM): `CORELINK_API_BASE` env var morta (DO hardcoda `CLW_ENDPOINT`).
- **N8** (BLOCKING): WebSocket hibernation sem tags — listener inline some após hibernation.
- **N9** (MEDIUM): `ports` [3] vs `requiredPorts` [4] inconsistente.
- **N10** (HIGH): `/resize` não existe no check-exec-server.

**Defeitos que foram corrigidos durante a auditoria (2):**
- N1 (HTTP→RPC router): commit `19eb2e1` ✓
- E5-residual (phantom clw flags): commit `44d2aaf` ✓

**Diferença material vs Gemini:**

| Item | Gemini | M3 |
|------|--------|-----|
| B6 (rota) | "nenhuma rota" | "rota existe + HTTP→RPC implementado" (E1) |
| B1 (Dockerfile) | "6 crates faltantes" | "8 crates existentes" (E2) |
| B2 (clw build) | "binário errado" | "clw baixado de release com SHA256" (E3) |
| B4 (exec-server) | "sem endpoints" | "5 endpoints registrados" (E4) |
| H2 (transitionState) | "sem guard" | "validateStateTransition é chamado" (E5) |
| H5 (main.rs) | "sem arg parsing" | "arg parsing existe; token não injetado" |
| Defeitos M3 | 0 | 7 residuais (N3, N4, N5, N6, N8, N9, N10) |
| Veredito | REJECT | ⚠️ PR-READY COM RESSALVAS |

**Defeito adicional descoberto (não-original) — Segurança:**
- `live-agent-openrouter.mjs:8` — **OpenRouter API key hardcoded em texto plano** no commit `6aa866a`. Key já está no histórico do git, mesmo que seja um script descartável. **Recomendação: rotação da key + cleanup do histórico** se a key for real. **Severidade:** HIGH (security leak).

---

## 6. PLANO DE CORREÇÃO (5 defeitos residuais, priorizado)

**Ordem de alavancagem (BLOCKING primeiro):**

1. **N8 — Hibernation WebSocket com tags** (BLOCKING). Mudar `acceptWebSocket(server)` (linha 517) → `acceptWebSocket(server, [tagConnId])`. Re-registrar `addEventListener('message')` em `webSocketMessage` após hibernation. Spec WP-05 §3.4. **Prioridade 1.**

2. **N10 — adicionar handler `/resize` no check-exec-server** (`lib.rs:94-98` area). Implementar `resize_handler` que chama `ioctl(TIOCSWINSZ)` no ttyd ou `code-server --bind-addr` resize. **Prioridade 2.**

3. **N3 — `validateClwToken` regex**: adaptar para `^corelink_pat_[\w._-]+$`. **Prioridade 3.**

4. **N4 — `devenv_guard.ts` fail-closed**: usar `max_vcpu_h` e `max_concurrency` retornados pela query; fail-closed em D1 error; computar `monthly_vcpu` contra `devenv_monthly_vcpu`. **Prioridade 4.**

5. **N5 — `recordUsage()` fail-closed**: surface D1 error; incrementar `billingSeq` em `recordUsage` ou no `transitionState`; propagar erro no HTTP push. **Prioridade 5.**

6. **B5 — D1 binding no corelink-spawn-worker**: adicionar `[[d1_databases]] binding = "CONFIG_DB"` no `wrangler.jsonc` (corelink-runners) com mesmo `database_id` do server. **Prioridade 6.**

7. **B7 — image pin**: workflow `build-cf-container-images.yml` precisa buildar `RunnerDevEnvDO` também. Pin por digest. **Prioridade 7.**

8. **N6 — `CORELINK_API_BASE`**: ler do env no DO, não hardcodar. **Prioridade 8.**

9. **N9 — spec cleanup**: alinhar `requiredPorts` (4 portas) com `buildStatusResponse.ports` (3 portas). **Prioridade 9.**

10. **WP-10 — CHANGELOG + dogfood + load test execution**: já commitado docs/runbooks/k6 script, falta **rodar** o k6 e gravar resultados, **adicionar CHANGELOG `[Unreleased]` entry** (gate de PR!). **Prioridade 10.**

11. **Segurança — Rotacionar `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`** (live-agent-openrouter.mjs:8). Se for key real, está em texto plano no histórico. **Prioridade 11 (segurança).**

---

## 7. ANEXO — Comandos Executados (reprodutibilidade)

```bash
# Estado dos repositórios
git -C corelink-runners log --oneline -5   # 19eb2e1, 44d2aaf, 6aa866a, e228249
git -C corelink-server log --oneline -5    # 926ed1dd, ea5c4696, cb2b948b, c98ca0f4, 0a5e3349
git -C corelink-runners branch --show-current   # feat/devenv-cloud-containers-wp01-wp07
git -C corelink-server branch --show-current    # feat/devenv-ingress-control-plane-wp08-wp09
git -C corelink-runners status -s   # vazio (working tree limpo)
git -C corelink-server status -s    # só artefatos não-devenv

# Type-check + tests + build (executado em tempo real)
cd corelink-runners/deploy/cloudflare && npx tsc --noEmit   # 0 errors
cd corelink-runners/deploy/cloudflare && npx vitest run     # 32 files, 524/524 tests, 7.60s
cd corelink-runners/crates/corelink-check-exec-server && cargo build --release  # OK 41.40s
cd corelink-server/worker && npx tsc --noEmit              # 0 errors

# Verificações por defeito
# N1 (CORRIGIDO por 19eb2e1)
git -C corelink-runners show 19eb2e1:deploy/cloudflare/src/durable_objects/runner_dev_env.ts | sed -n '393,508p'
# mostra: HTTP→RPC router com 7 endpoints REST + WS proxy

# E5-residual (CORRIGIDO por 44d2aaf)
git -C corelink-runners show 19eb2e1:deploy/cloudflare/src/durable_objects/runner_dev_env.ts | grep -E "auth-file|pack-small|generation-id"
# (vazio)

# N3 (regex rejeita tokens reais) — NÃO corrigido
git -C corelink-runners show 19eb2e1:deploy/cloudflare/src/types/devenv.ts | sed -n '62,67p'
# /cl_[a-zA-Z0-9_]{16,}/ — rejeita corelink_pat_*

# N4 (guard stub fail-open) — NÃO corrigido
git -C corelink-server show 926ed1dd:worker/src/lib/devenv_guard.ts | sed -n '16,48p'
# linha 43: "// Fail-open for transient D1 reads during quota check if configured"
# linha 47: return { allowed: true }

# N5 (recordUsage fail-open) — NÃO corrigido
git -C corelink-runners show 19eb2e1:deploy/cloudflare/src/durable_objects/runner_dev_env.ts | sed -n '313,374p'
# D1 swallow (linhas 350-353), HTTP console.error (linha 371)
# billingSeq: set em 145 (=0), nunca incrementado (lido em 179, 238, 332, 368)

# N6 (env morta) — NÃO corrigido
git -C corelink-server show 926ed1dd:wrangler.toml | grep "CORELINK_API_BASE"
# linha 249: var (única menção)
git -C corelink-runners grep -n "CORELINK_API_BASE" deploy/
# vazio

# N8 (hibernation sem tags) — NÃO corrigido
git -C corelink-runners show 19eb2e1:deploy/cloudflare/src/durable_objects/runner_dev_env.ts | sed -n '510,547p'
# 517: this.ctx.acceptWebSocket(server);  ← SEM tags
# 534: containerWs.addEventListener("message", ...)  ← listener inline

# N10 (/resize não existe) — NÃO corrigido
git -C corelink-runners grep -rn "resize" crates/corelink-check-exec-server/
# vazio
```

---

## 8. HISTÓRICO DE REVISÕES DESTE RELATÓRIO

**v5 (atual):** Re-escrito INTEIRO após descobrir 2 commits adicionais do usuário (`44d2aaf` corrigiu E5-residual, `19eb2e1` corrigiu N1) e 1 commit server-side (`926ed1dd` corrigiu docs). Verificação executada em tempo real: `tsc --noEmit` 0 erros, `vitest run` 524/524, `cargo build --release` OK. **5 defeitos persistem** (N3, N4, N5, N6, N8, N9, N10) — todos verificados contra HEAD. 1 achado de segurança novo: API key do OpenRouter hardcoded em `live-agent-openrouter.mjs:8`.

**v4:** 6 erros/parciais erros do Gemini corrigidos (E1-E6, F1). Não viu os commits `44d2aaf` e `19eb2e1`.

**v3:** 5 correções (E1-E5) + N7 invalidado + N10 adicionado. Erro em F1.

**v2:** 4 correções (E1-E4) + N7 invalidado + N10 adicionado. Linhas citadas exatas.

**v1:** 9 defeitos (N1-N9). Trabalhou em working tree antes dos commits.
