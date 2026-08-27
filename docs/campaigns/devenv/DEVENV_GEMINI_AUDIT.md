# DEVENV_GEMINI_AUDIT — Auditoria da Implementação DevEnv (Gemini)

**Status:** ❌ REJECT — scaffold não-funcional, nada commitado
**Data:** 2026-08-27
**Escopo:** Auditoria código-grounded contra o working tree + histórico de commits/PRs (`git log --all`, `origin/main`) de 3 repos: `corelink-server`, `corelink-runners`, `corelink-workspaces`.
**Método:** Cada claim foi verificada contra fonte (arquivos) E contra histórico (git). Claims sem prova dupla foram retiradas. Três leituras anteriores (C1 quota.ts; C2 exec-server; C3 egress) foram CONTRADITAS e corrigidas aqui — ver §2.

---

## 0. Sumário Executivo (TL;DR)

A implementação do DevEnv feita pelo Gemini é **scaffold de 10 work-packages (WP), 100% não-commitado**, que **não compila, não roda, não é roteado, e não tem imagem**. Dos 10 WPs:

| WP | Status | Defeito central |
|----|--------|-----------------|
| WP-01 RunnerDevEnvDO Skeleton | 🟡 PARCIAL | DO existe mas sem guard de transição (INV I2 violado) |
| WP-02 Dockerfile | 🔴 BROKEN | 6 crates COPY inexistentes + binário errado (`corelink` ≠ `clw`) |
| WP-03 Entrypoint/Supervisord | 🔴 BROKEN | flags clw fantasma + `ls` posicional errado + auth morta |
| WP-04 clw Integration | 🔴 BROKEN | binário clw errado + 3 flags inexistentes + `CLW_AUTH_FILE` ignorada |
| WP-05 WebSocket Proxy | 🟡 PARCIAL | lógica existe no DO, mas DO inalcançável (sem rota) + com falha de contrato 9090 |
| WP-06 DO Lifecycle + exec-server | 🔴 BROKEN | contract `/clw /mkdir /ping /port-check` em 9090 NÃO implementado; reaproveita binário check-host (8080/`/exec`) |
| WP-07 Billing Metering | 🔴 BROKEN | `recordUsage()` dead code — sem binding D1; tabela `0094` nunca commitada |
| WP-08 Worker Ingress Routes | 🔴 BROKEN | nenhuma rota de ingress para o DO; `RunnerDevEnvDO` inalcançável |
| WP-09 Dashboard UI | 🔴 BROKEN | UI chama `/v1/customer/devenv` que não existe em nenhum server |
| WP-10 Dogfood/Testes/Docs | 🔴 NOT_STARTED | só o plano; nenhum deliverable, nenhum CHANGELOG |

**Veredicto:** 7 WPs BROKEN, 2 PARCIAL, 1 NÃO-INICIADO. Nada em `main`. O feature-end deliverable não existe.

---

## 1. O Fato Decisivo: Nada Está Commitado

Verificado contra o histórico — **os arquivos devenv existem SOMENTE no working tree, todos untracked (`??`) ou como dirty edits em arquivos trackeados.** Nunca passaram por commit, começam em nenhuma branch, não estão em `main`.

### corelink-runners (branch atual `chore/repin-runner-image`)
| Arquivo | Estado git |
|---------|-----------|
| `deploy/cloudflare/src/durable_objects/runner_dev_env.ts` | `??` untracked |
| `deploy/cloudflare/src/types/devenv.ts` | `??` untracked |
| `deploy/cloudflare/Dockerfile.runner-devenv` | `??` untracked |
| `deploy/cloudflare/entrypoint.sh` | `??` untracked |
| `deploy/cloudflare/supervisord.conf` | `??` untracked |
| `deploy/cloudflare/test/devenv-do.test.ts` (+ 2 outros) | `??` untracked |
| `RunnerDevEnvDO` export em `src/index.ts` | só working tree — **`git show HEAD` = 0, `git show origin/main` = 0** |
| `RUNNER_DEVENV_DO` binding em `wrangler.jsonc` | só working tree — idem |

### corelink-server (branch atual `feat/remediation-gated-features`)
| Arquivo | Estado git |
|---------|-----------|
| `apps/admin-ui/src/app/[locale]/(authenticated)/customer/devenv/` | `??` untracked |
| `apps/admin-ui/src/components/customer/DevenvClient.tsx` | `??` untracked; **`git cat-file origin/main:…` = NOT on main** |
| `apps/admin-ui/tests/devenv-client.test.tsx` | `??` untracked |
| `migrations/d1/0094_devenv_monthly_vcpu.sql` | `??` untracked; **`git log --all` = vazio (nunca commitado)** |
| `worker/src/lib/quota.ts` | `M` modificado — **MAS não é trabalho devenv** (ver §C1) |

**Conclusão:** a "implementação" é inteiramente material não-commitado. Não há PR, não há commit, não há merge. Qualquer claim de "o Gemini já implementou" refere-se a um working tree sujo — frágil e reversível a qualquer momento.

---

## 2. Correções Nesta Revisão (erros que EU cometi e consertei)

Transparência: das minhas leituras anteriores, **duas claims estavam erradas** e foram corrigidas após cheque contra fonte + histórico.

### C1 — quota.ts NÃO é trabalho WP-07 ❌ (retirado)
Inicialmente insinuei que a modificação de `worker/src/lib/quota.ts` poderia ser o WP-07. **Errado.** Verificação:
- O diff introduz `runQuotaBatch` / "wdb latency fix" — refactor de D1 batching.
- `git log --all --grep="wdb\|QuotaBatch\|runQuotaBatch"` = branches **`perf/…` commitadas** (PRs #1131, #1191, #1283, #1311).
- `git diff --cached worker/src/lib/quota.ts | grep -i devenv/vcpu/monthly_vcpu` = **vazio**.
→ É um refactor de performance pré-existente e não-relacionado. WP-07 real = `recordUsage()` no DO (dead code, §B5).

### C2 — Exec-server `corelink-check-exec-server` é PRÉ-EXISTENTE ✅ (confirmado)
Está **tracked e em `origin/main`** (campanha check-host: commits #203, #245, #332, #349). Ou seja, B4 não é "o Gemini escreveu um exec-server errado" — é **"o Gemini reaproveitou o binário errado (o de check-host) como se fosse o exec-server do DevEnv"**. O contrato WP-06 nunca foi implementado como binário novo.

### C3 — egress / `enableInternet` ❌ (retirado definitivamente)
Inicialmente afirmei que `enableInternet = true` abriria egress irrestrito, violando INV-05. **Verificado contra `node_modules/@cloudflare/containers@0.3.7/dist/lib/container.js:202-259` — WRONG:**
- `allowedHosts` é um **whitelist gate real**: "when set, acts as a whitelist gate — only matching hosts pass."
- O DO seta `allowedHosts = [corelink-api.humangr.com, *.cloudflarestorage.com, *.r2.cloudflarestorage.com]`.
- Logo o egress É restrito a essa lista, independente de `enableInternet:true`.
→ **Retirado.** A isolamento de egress via `allowedHosts` está correta.

---

## 3. Defeitos BLOCKING (confirmados por fonte + histórico)

### B1 — Dockerfile.runner-devenv NÃO compila: 6 crates COPY inexistentes 🔴
**Fonte:** `ls crates/` vs. `grep "^COPY crates/" Dockerfile.runner-devenv`.
O Dockerfile copia 12 crates; **6 não existem** em `corelink-runners/crates/`:

| COPY pede | Existe? |
|-----------|---------|
| `corelink-check-exec-server` | ✅ |
| `corelink-cli` | ✅ |
| `corelink-cloud-engine` | ✅ |
| `corelink-fabric` | ✅ |
| `corelink-fabric-api` | ✅ |
| `corelink-identity` | ❌ **MISSING** |
| `corelink-lease` | ❌ **MISSING** |
| `corelink-memoize` | ❌ **MISSING** |
| `corelink-policy` | ❌ **MISSING** |
| `corelink-runner` | ✅ |
| `corelink-telemetry` | ❌ **MISSING** |
| `corelink-testing` | ❌ **MISSING** |

**Histórico:** os 6 crate faltantes nunca existiram (só 8 crates no repo, `git ls-files`). Docker `COPY` de diretório-fonte ausente é **erro de build duro**. Nenhum CI builda este Dockerfile.

### B2 — Builda o binário ERRADO (`corelink`, não `clw`) 🔴
**Fonte + histórico:**
- `crates/corelink-cli/Cargo.toml` (em `origin/main`): `[[bin]] name = "corelink"` — um binário, o fabric CLI.
- `corelink-cli/src/main.rs` dispatch: comandos `run` / `smoke` / `verify` — **sem `snapshot`/`hydrate`/`ls`**.
- Dockerfile linha 37: `cp /src/target/.../release/clw /out/clw` — **não existe arquivo `clw`** → `cp` falha.
- Grep em todos os `crates/*/Cargo.toml` por `name = "clw"` = **zero** no corelink-runners.

**O `clw` verdadeiro** (snapshot/hydrate/ls) está em **corelink-workspaces** (`crates/clw-cli/Cargo.toml`: `[[bin]] name = "clw"`, subcommands em `src/subcmds/`). O Dockerfile **nunca copia nem builda** corelink-workspaces.

**Resultado:** o container DevEnv nunca teria o `/usr/local/bin/clw` esperado pelo entrypoint — e, mesmo que tivesse, seria o CLI errado.

### B3 — Flags e auth do clw fantasma 🔴
**Fonte:** leitura exata do clw (corelink-workspaces).

**Flags `--auth-file`, `--pack-small-files-threshold-kb`, `--generation-id` NÃO EXISTEM em lugar nenhum do clw-cli:**
- `clw snapshot` (`subcmds/snapshot.rs:25-47`): args = `path` (posicional), `--name`, `--force`. **Só.**
- Grep `auth.file|pack.small|generation.id` em `crates/clw-cli/src/` = **vazio**.

**Uso incorreto:**
| Chamada | Problema |
|---------|----------|
| `clw ls <nome>` (entrypoint, posicional) | `ls` usa `--name`/`--all` (`ls.rs:21-33`) → clap "unexpected argument" → detecta ref errado |
| `clw snapshot <dir> --name X --pack-small-files-threshold-kb 128` (entrypoint) | flag não existe → snapshot falha (engolido por `\|\| true`) |
| `clw snapshot … --auth-file … --generation-id N` (DO `execClwSnapshot`) | 3 flags não existem → clap erro → `throw` |

**Auth morta:** clw lê token de `--token` / `CLW_TOKEN` / `CORELINK_TOKEN` / `~/.clw/config.toml` (`config.rs:124-144`). **NÃO lê `CLW_AUTH_FILE`.** O entrypoint faz `echo token > /dev/shm/.clw-auth`, seta `CLW_AUTH_FILE`, e depois `unset CLW_TOKEN` → **clw fica sem token → todo request que exija auth falha** (o cookie da sessão do DevEnv nunca é transmitido ao clw, quebra o fluxo snapshot/hydrate autenticado).

### B4 — Contrato do exec-server (WP-06 §3.1.1) NÃO implementado 🔴
**Spec** (WP-06 §3.1.1) exige um exec-server in-container que o DO chama via `containerFetch` na porta **9090**, com endpoints:
- `/clw` POST, `/mkdir` POST, `/ping` GET, `/port-check/:port` GET
- 9090, NÃO 8080 (8080 é do code-server).

**Fonte + histórico:** o único exec-server que existe (`crates/corelink-check-exec-server`, **em `origin/main`** — pré-existente, check-host campaign) :
- `lib.rs:38` `DEFAULT_PORT = 8080`; `lib.rs:94` `Router::new().route("/exec", …)` — **só `/exec`**.
- `main.rs:42` bind `0.0.0.0:8080` hardcoded; **nenhum arg parsing** — o `--bind-addr 127.0.0.1:9090` do supervisord é **ignorado silenciosamente**.
- **Zero** ocorrências de `/clw`, `/mkdir`, `/ping`, `/port-check`, ou porta 9090 no crate.

**Cadeia de falha:** DO `containerExec()` → `POST :9090/clw` → nada escuta em 9090 → `EXEC_RPC_FAILED`. `resize()` → `POST :9090/resize` → 404. Todo snapshot/exec/resize do DO falha.
**Bônus (H5):** `main.rs` **fail-closed**: sem `EXEC_SERVER_AUTH_TOKEN` o processo **sai no boot**. O supervisord não injeta essa env → o "exec-server" nem sobe.

### B5 — Metering (WP-07) é dead code 🔴
**Fonte + histórico:**
- `runner_dev_env.ts:334` `if ((this.env as any).CONFIG_DB) {` e `:341` `CONFIG_DB.prepare(...)`.
- **`Env`** (`index.ts`) **não tem `CONFIG_DB`**. **`wrangler.jsonc` não tem binding D1** (`git log --all -S "d1_databases"` em `wrangler.jsonc` = **vazio** — nunca houve).
- A tabela `devenv_monthly_vcpu` está em **corelink-server** (`migrations/d1/0094_*.sql`), repo/account D1 distintos — e a migration **nunca foi commitada**.
- → `CONFIG_DB` é sempre `undefined` → o UPSERT `INSERT INTO devenv_monthly_vcpu` **nunca roda**. Billing do DevEnv = zero.

### B6 — Sem rota de ingress (WP-08) / DO inalcançável 🔴
**Fonte (grep direto, não subagente):**
- `corelink-runners/deploy/cloudflare/src/index.ts`: `RunnerDevEnvDO` só é **exportado** (linha 17); **nenhum** handler de fetch o instancia (`idFrom`/`get(id).startDevenv`). `grep -c RunnerDevEnvDO` = 1 (só o export).
- `corelink-server`: `grep -rni "devenv\|dev_env\|/v1/customer/devenv" crates/corelink-container/src/ worker/src/` = **zero**. `routes.rs` (linhas 42-275) lista módulos `customer`, `customer_runners`, `workspaces` — **sem `devenv`**.

→ O DO não é alcançável por nenhuma requisição Worker. Nenhum endpoint o acorda.

### B7 — Imagem `corelink-runner-devenv:latest` não-pinnada e nunca buildada 🔴
**Fonte + histórico:**
- `wrangler.jsonc:277` `"image": "registry.cloudflare.com/6a1fc1c626fc2628823e60b9db01f5cd/corelink-runner-devenv:latest"` — tag `:latest`, não `@sha256` (viola a disciplina X4/pin do próprio repo, documentada no mesmo arquivo).
- `git grep -i "runner-devenv\|Dockerfile.runner-devenv"` sobre `git rev-list --all` (histórico) = **vazio** → nenhum workflow builda/push essa imagem.

### WP-10 — Nenhum deliverable 🔴 NOT_STARTED
- Só o plano `WP-10_Dogfood_Testing_Docs.md` (header `NOT_STARTED`).
- Ausentes: `docs/devenv/`, dogfood reports, `tests/load/k6/scenarios/devenv-concurrent.js`, `scripts/run_devenv_load.sh`, runbooks `RB-DEVENV-*`, conceitos OKF, `marketing/launch/DEVENV-PRICING.md`.
- `grep -i devenv CHANGELOG.md` (ambos repos) = **vazio**. Nenhum commit em `git log --all` menciona devenv.

---

## 4. Defeitos HIGH / MÉDIOS

### H2 — Sem guard de transição de estado (INV I2 violado) 🟠
Spec WP-01 (linha 347): `transitionState()` deve **lançar** `INVALID_STATE_TRANSITION` em transição ilegal, listando as permitidas.
Impl (`runner_dev_env.ts:87-90`):
```ts
private async transitionState(newState: DevenvState): Promise<void> {
  this.devenvState = newState;
  await this.persistState();
}
```
**Sem validação** — qualquer transição passa. Estado impossível representável. Confirmado.

### H5 — exec-server fail-closed no boot (ver B4) 🟠
Sem `EXEC_SERVER_AUTH_TOKEN` injetado pelo supervisord, o binário check-host sai do boot antes de B4 importar.

### H6 — Tiers não escalam de verdade 🟠
`RunnerDevEnvDO` aceita `standard-2/4/power-8/ultra-16` (types/devenv.ts + StartPayload), mas o container é **sempre provisionado `instance_type: "standard-4"`** no wrangler.jsonc. O vCPU é fixo; tiers superiores não mudam `<instance_type>`. Notional, não funcional.

### M — Discrepâncias de spec (auto-contraditórias, nota não-culpa) 🟡
- **requiredPorts:** impl `[6080,7681,8080,9090]`; spec linha 277 diz o mesmo, mas linha 779/943 testa `[6080,7681,8080]`. **Spec interna inconsistente** — não imputo como defeito claro da impl.
- **WP-09 component-split:** spec pede árvore `components/devenv/` (List/Detail/Modal/ResizeControls/hooks/detail page). Impl colapsa tudo em `DevenvClient.tsx` (306 linhas) em `components/customer/`. Divergência estrutura real.

---

## 5. O Que Está REALMENTE OK ✅

Ser justo — nem tudo é lixo:
- **`types/devenv.ts`**: discriminated-union `DevenvState` (5 estados) + validators puros (ts zero-dep). Sólido.
- **Estrutura de ciclo de vida do DO** (`onStart`/`onStop`/`onError`): internamente coerente.
- **Testes `devenv-do.test.ts` / `deep-step-by-step-audit.test.ts`**: exercitam o DO real via mock de `@cloudflare/containers`; plauzível passarem (config vitest existe sob `deploy/cloudflare/`).
- **`allowedHosts` egress**: correto (C3).

---

## 6. Veredicto & Plano de Correção Recomendado

**Veredicto: ❌ REJECT.** Nada commitado; working tree não builda (B1/B2), não autentica/snapshot (B3), não exec (B4/H5), não mete (B5), não é roteado (B6), sem imagem (B7), WP-10 não iniciado.

### Ordem de correção (maior alavancagem primeiro)
1. **B1/B2/B3 — corrigir o build do clw**: mudar o Dockerfile para buildar `clw-cli` de **corelink-workspaces** (não `corelink-cli` de runners), copiar só os crates existentes, e corrigir as flags usadas por entrypoint/DO para o surface real do clw (`--name`, `--force`; `ls --name`; token via `CLW_TOKEN`, não `CLW_AUTH_FILE`).
2. **B4/H5 — implementar o exec-server do DevEnv** (`/clw /mkdir /ping /port-check` em 9090) como binário novo; não reutilizar o check-host.
3. **B6 — rotear o DO**: adicionar rota de ingress que instancie `RunnerDevEnvDO` + rota `/v1/customer/devenv` no `routes.rs` do server.
4. **B5 — ligar metering**: binding D1 `CONFIG_DB` no wrangler do spawn-worker + commit da migration na BANCO certa.
5. **B7 — image**: build CI + pin por digest.
6. **H2 — guard de transição**, **H6 — tier mapping**, **WP-09** fidelidade ao spec.

---

## 7. Anexo: Evidências (file:line)
| Claim | Evidência |
|-------|-----------|
| Nada commitado | `git status --short` (todos `??`); `RunnerDevEnvDO` ausente em HEAD/main |
| B1 | `ls crates/` vs `grep "^COPY crates/"` (6 MISSING) |
| B2 | `crates/corelink-cli/Cargo.toml:12-13` `name="corelink"`; Dockerfile:37 `cp .../clw`; `clw` só em workspaces |
| B3 | `snapshot.rs:25-47`, `ls.rs:21-33`, `config.rs:124-144`; grep flags = vazio |
| B4 | `check-exec-server/lib.rs:38,94`; `main.rs:42`; spec WP-06 §3.1.1 |
| B5 | `runner_dev_env.ts:334,341`; `wrangler.jsonc` sem D1; `0094` untracked |
| B6 | `index.ts:17` só export; `routes.rs` sem módulo devenv |
| B7 | `wrangler.jsonc:277` `:latest`; `git grep` histórico = vazio |
| WP-10 | `WP-10*.md` NOT_STARTED; CHANGELOG sem devenv |
| C1 | `git log --grep=wdb` = branches perf; diff quota.ts sem devenv |
| C2 | `check-exec-server` em origin/main (#203/#245/#332/#349) |
| C3 | `@cloudflare/containers/container.js:202-259` `allowedHosts` whitelist |
