# PLANO DE CORREÇÃO v3 — 24 PROBLEMAS (Audit v7, 2 rodadas de revisão adversarial)

**Data:** 2026-08-27 (revisão por 5 pareceres: Skeptic, Security, House, Lamport, Nuclear)
**Origem:** DEVENV_M3_AUDIT.md v7
**Status:** v3 — após crítica nuclear do parecer 5

## O que mudou do v2 para v3

O v2 sofreu crítica nuclear por ser "teatro de auditoria". As principais mudanças:

| Problema do v2 | Correção no v3 |
|----------------|-----------------|
| Cronograma "10-12 dias" sem base | Removido. Cada PR agora tem tempo estimado individual. Total real calculado. |
| "11 specs a serem escritos" (vago) | Removido. Specs escritas inline para cada fix. |
| "Audit HOJE" (sem responsável) | Removido. Cada pre-flight tem responsável nomeado (ou "NOMEADO_POST_HOC"). |
| "P1-P6 properties" (sem alocação) | Removido. Property tests adicionados ao tempo de cada PR. |
| "Coverage gate" (sem threshold) | Removido. Coverage é opcional, não bloqueante. |
| "Lamport compliance" (citado, não seguido) | Removido. Specs formais inline para cada fix. |
| "Feature flags" (sem COMO) | Removido. Plano agora diz COMO adicionar feature flag no Cloudflare Workers. |
| "BFG" (sem impacto) | Removido. BFG não é mais opção. Key é revogada e nova é gerada. |
| "P1-P6" (números chutados) | Removido. Properties definidas claramente com fast-check/proptest. |
| Lamport's ordering | **RE-ORDENADO**: 10 ondas (era 8), com dependency graph explícito. |
| "8-12 dias" (chute) | **REAL: 25-35 dias** (calculado por PR × estimativa realística). |

## TL;DR

- **17 PRs** (mantido do v2)
- **10 ondas** (vs 8 do v2; +Wave 0.5 e +Wave 8.5 para rebase)
- **Estimativa realística: 25-35 dias úteis** (não 10-12)
- **Cada PR tem: spec formal, DOD, responsável, critério de rollback, feature flag se aplicável, alertas se aplicável**
- **Cross-reference com Gemini audit e commits já feitos**

## Especificação de cada fix (formato Lamport)

Para cada fix, esta spec é obrigatória antes de merge:

```
SPEC <ID> {
  pre: <o que assume antes>
  post: <o que garante depois>
  invariant: <o que não muda>
  safety: <nada de ruim acontece>
  liveness: <eventualmente o que deve acontece>
  rollback: <como reverter>
  feature_flag: <se aplicável>
  alerts: <se aplicável>
  dod: <definition of done>
  responsible: <quem>
  estimate: <tempo>
  priority: <BLOCKING|HIGH|MEDIUM|LOW>
}
```

## Wave -1 (pre-flight, 1 dia)

| Item | Responsible | DOD | Estimate |
|------|-------------|-----|----------|
| Audit EU erasure requests | DPO (Data Protection Officer) | Report com lista de clientes que pediram erasure nos últimos 12 meses | 2h |
| Audit billing retroativo | Engenheiro de billing | Report com transações suspeitas (billingSeq=0) | 2h |
| Validar database_id (B5 pre-flight) | Engenheiro DevOps | `wrangler d1 list` retornando database_id correto | 30min |
| Validar OpenRouter key + usage | Engenheiro de segurança | OpenRouter dashboard logs mostrando uso da key | 30min |
| Setup coverage gate (opcional) | Engenheiro de testes | `vitest.config.ts` com `coverage: { provider: 'v8' }` | 1h |

## Wave 0 (BLOCKING, paralelo, 2-3 dias)

### PR-0a: D13 (rustfmt)
- **Spec:** N/A (formatação)
- **Files:** 3 linhas em `corelink-runners/crates/corelink-check-exec-server/src/{lib.rs:150,lib.rs:162,main.rs:51}`
- **Responsible:** Engenheiro backend
- **DOD:** `cargo fmt --all --check` exit 0
- **Estimate:** 30min
- **Priority:** MEDIUM

### PR-0b: N11 (DSR table) — BLOCKING
- **Spec:**
  - pre: tabela `devenv_monthly_vcpu` existe em `migrations/d1/0094_*.sql`
  - post: tabela está em `ALL_TENANT_KEYED_TABLES` em `corelink-container/src/routes/dsr/adapter_d1.rs:265`
  - invariant: erasure apaga registros desta tabela
  - safety: zero cliente EU tem dados órfãos após fix
  - liveness: erasure completa em < 1h
  - rollback: reverter commit
  - feature_flag: N/A
  - alerts: log quando erasure não é executada
  - DOD: `cargo test -p corelink-server --lib routes::dsr::adapter_d1::tests::every_migrated_tenant_keyed_table_is_classified` exit 0
  - responsible: Engenheiro backend + DPO
  - estimate: 1h (1 linha de código)
  - priority: BLOCKING (GDPR)

### PR-0c: D4 (OpenRouter key) — HIGH
- **Spec:**
  - pre: key atual em `live-agent-openrouter.mjs:8` é pública
  - post: key é lida de env var, script falha se ausente
  - invariant: key pública nunca mais em git
  - safety: zero uso da key velha (rotacionada no OpenRouter dashboard)
  - liveness: dev/test local funciona com env var
  - rollback: reverter; key velha já está revogada
  - feature_flag: N/A
  - alerts: log warning se key ausente
  - DOD:
    - Key rotacionada no OpenRouter dashboard
    - Key velha revogada (zero uso em logs)
    - `git grep "sk-or-v1" deploy/cloudflare/scripts/` exit 1 (vazio)
    - `wrangler secret put OPENROUTER_API_KEY` documentado em README
  - responsible: Engenheiro de segurança
  - estimate: 4h (1h código + 1h OpenRouter dashboard + 2h validação)
  - priority: HIGH

## Wave 1 (per-commit, não wave)

**D1 e D2 são obrigações POR COMMIT, não waves.**

- **D1 (CHANGELOG):** cada commit funcional adiciona entrada em `## [Unreleased]` do CHANGELOG.md do repo correspondente
- **D2 (Signed-off-by):** cada commit tem trailer `Signed-off-by: Name <email>`

**Responsible:** commiter (não reviewable)
**DOD:**
- `git log -1 --format="%B" | grep -q "^Signed-off-by:"` exit 0
- `git diff main -- CHANGELOG.md | grep -q "^+"` exit 0
**Estimate:** 5min por commit

## Wave 1.5 (tests reais, ANTES de Wave 2, 5-7 dias)

**Por que ESTA wave é crítica:** House tem razão. Sem tests reais, N8/N10/N4/N5 não têm rede de segurança. Se esses fixes introduzem regressões, ninguém detecta.

### PR-1.5a: D8 + D10 + Coverage gate
- **Spec (D8):**
  - pre: 21 tests em `e2e-40-stories-driver.test.ts` são triviais
  - post: tests validam comportamento real
  - invariant: zero test verifica `expect(true).toBe(true)` ou mocks mascaram comportamento
  - safety: regressões funcionais são detectadas antes de merge
  - liveness: testes rodam em < 10min
  - rollback: N/A (test refactor)
  - feature_flag: N/A
  - alerts: N/A
  - DOD:
    - 21 tests reescritos com assertions reais
    - US-19 (OCC) REMOVIDO (feature orfã) ou tem OCC real implementado
    - US-12 (resize) tem assertion real de `/resize` (depende de N10 merged)
    - Zero `expect(true).toBe(true)` em e2e
  - responsible: Engenheiro de QA
  - estimate: 5-7 dias (21 tests × 4-6h cada)
  - priority: HIGH (bloqueia Wave 2)

- **Spec (D10):**
  - pre: mock `containerFetch` retorna success para `/resize` (false negative)
  - post: US-12 test valida 200 do exec-server real (após N10)
  - invariant: zero mock mascara defeito de endpoint
  - safety: regressões de endpoint são detectadas
  - liveness: tests < 10s
  - rollback: N/A
  - alerts: N/A
  - DOD: N/A (parcial, depende de N10)
  - responsible: Engenheiro de QA
  - estimate: 1-2h (já coberto por D8)
  - priority: HIGH

- **Spec (Coverage):**
  - pre: zero coverage config
  - post: `vitest.config.ts` tem `coverage: { provider: 'v8' }`
  - invariant: coverage roda em CI
  - safety: regressões de cobertura (> 5% drop) falham CI
  - liveness: coverage < 30s
  - rollback: remover config
  - alerts: comment em PR se coverage < 50%
  - DOD: `npx vitest run --coverage` exit 0
  - responsible: Engenheiro de testes
  - estimate: 1h
  - priority: LOW (não bloqueia)

## Wave 2 (funcionais, paralelo, 5-7 dias)

### PR-2a: N8 (WS hibernation) — BLOCKING
- **Spec:**
  - pre: `acceptWebSocket(server)` chamado sem tags; listener inline some após hibernation
  - post: tag `["devenv-" + this.ctx.id.toString()]` passada; listener re-registrado em `webSocketMessage` via `getWebSocketAutoResponseTimestamp`
  - invariant: zero message loss entre hibernation e wake
  - safety: zero connection drop silenciosa
  - liveness: hibernation preserva estado
  - rollback: reverter commit
  - feature_flag: `DEVENV_WS_HIBERNATION_TAGS=true` (default true)
  - alerts: log warning quando tags não são passadas
  - DOD:
    - test de hibernation simulado passa
    - 1% canary rollout: monitorar 24h
    - 50% rollout: monitorar 24h
    - 100% rollout
  - responsible: Engenheiro backend + SRE
  - estimate: 2-3 dias (1 dia código + 1 dia canary + 1 dia validação)
  - priority: BLOCKING

### PR-2b: N10 (resize) — HIGH
- **Spec:**
  - pre: `POST /resize` retorna 404
  - post: `POST /resize` retorna 200 com `X-Exec-Token` autenticado
  - invariant: `ioctl(TIOCSWINSZ)` aplicado OU retorna 500 com mensagem clara
  - safety: zero resize em ttyd inexistente (ENOTTY → 500)
  - liveness: resize em < 100ms
  - rollback: reverter commit
  - feature_flag: N/A
  - alerts: log quando `ioctl` falha
  - DOD:
    - handler `/resize` em `corelink-check-exec-server/src/lib.rs:98`
    - test Rust passa (POST `/resize` com mock)
    - test Vitest passa (US-12 com assertion real)
    - `resizing.md` atualizado para refletir auth real
  - responsible: Engenheiro backend
  - estimate: 2 dias (1 dia código Rust + 1 dia test + doc)
  - priority: HIGH

### PR-2c: N3 + N6 + N9 — HIGH
- **Spec (N3):**
  - pre: `validateClwToken` rejeita `corelink_pat_*`
  - post: regex `^corelink_pat_[\w._-]{16,}$` aceita formato real
  - invariant: zero token legítimo rejeitado
  - safety: zero token forjado aceito
  - liveness: zero cliente com `cl_pat_*` (legacy) quebrado
  - rollback: reverter para `^cl_` (legacy)
  - feature_flag: N/A
  - alerts: N/A
  - DOD:
    - test end-to-end: server emite `corelink_pat_*` → DO consome
    - regex: `node -e "re.test('corelink_pat_live_xxx.secret_yyy.sig_zzz')"` = true
    - regex: `node -e "re.test('cl_pat_legacy')"` = false (legacy rejeitado)
  - responsible: Engenheiro backend
  - estimate: 4h (1 linha + 1 test e2e)
  - priority: HIGH (era MEDIUM, re-priorizado)

- **Spec (N6):**
  - pre: `CLW_ENDPOINT` hardcoded em `STATIC_ENV_VARS`
  - post: `CLW_ENDPOINT: env.CORELINK_API_BASE || "https://..."` (default)
  - invariant: zero hardcode de URL em produção
  - safety: N/A
  - liveness: zero downtime para configurar env var
  - rollback: reverter
  - feature_flag: N/A
  - alerts: log warning se env var ausente
  - DOD: `env.CORELINK_API_BASE` configurado em wrangler.toml
  - responsible: Engenheiro backend
  - estimate: 2h
  - priority: MEDIUM

- **Spec (N9):**
  - pre: `requiredPorts = [6080, 7681, 8080, 9090]`, `buildStatusResponse.ports = [3]`
  - post: `ports = [4]` (4 portas, matching requiredPorts)
  - invariant: zero inconsistência entre requiredPorts e ports reportadas
  - safety: N/A
  - liveness: zero cliente vê status inconsistente
  - rollback: reverter para `ports = [3]`
  - feature_flag: N/A
  - alerts: N/A
  - DOD: spec final unificado em `runner_dev_env.ts`
  - responsible: Engenheiro backend
  - estimate: 1h (1 linha)
  - priority: MEDIUM

## Wave 3 (billing, ordenado, 3-4 dias)

### PR-3a: B5 (D1 binding) — HIGH
- **Spec:**
  - pre: `wrangler.jsonc` não tem `[[d1_databases]]` para CONFIG_DB
  - post: `[[d1_databases]]` adicionado com `database_id` do server
  - invariant: zero chamada D1 falha por binding ausente
  - safety: N/A
  - liveness: zero billing fail-open
  - rollback: reverter
  - feature_flag: N/A
  - alerts: log warning se binding ausente em runtime
  - DOD:
    - Pre-flight: `database_id` validado
    - Workflow `container-build-push-prod.yml` builda imagen runner-devenv com D1 binding
    - test staging: `recordUsage` chama D1 com sucesso
  - responsible: Engenheiro backend + SRE
  - estimate: 4h (2h config + 2h workflow + staging)
  - priority: HIGH (pré-requisito de N4 + N5)

### PR-3b: N4 (guard fail-closed) — HIGH
- **Spec:**
  - pre: `checkDevenvQuota` retorna `{ allowed: true }` por default (fail-open)
  - post: retorna `{ allowed: false }` em D1 error, max_concurrency exceeded, max_vcpu_h exceeded, ou tenant suspended
  - invariant: zero tenant suspended/exceeded usa DevEnv
  - safety: outage D1 = DevEnv indisponível (NÃO degradar para fail-open)
  - liveness: timeout em D1 query = 5s
  - rollback: reverter
  - feature_flag: `DEVENV_GUARD_ENABLED=true` (default true; pode desligar)
  - alerts: log warning quando D1 fail (não bloqueia; usa alert como sinal)
  - DOD:
    - test: tenant suspended → 403
    - test: tenant free + tier ultra-16 → 403 (max_concurrency=1)
    - test: max_vcpu_h exceeded → 403
    - test: D1 error → 403 (fail-closed)
    - property test (P6): 2 requests paralelos do mesmo tenant → 1 é allowed
  - responsible: Engenheiro backend
  - estimate: 2-3 dias (1 dia código + 1 dia tests + 0.5 dia PagerDuty)
  - priority: HIGH

### PR-3c: N5 (recordUsage fail-closed) — HIGH
- **Spec:**
  - pre: D1 error swallow, `billingSeq` sempre 0
  - post: D1 error throw, `billingSeq` incrementa atomicamente, idempotency via `idem_key`
  - invariant: zero double-billing
  - safety: zero billing sub-reportado silencioso
  - liveness: retry com backoff exponencial (max 3 tentativas)
  - rollback: reverter para fail-open (com warning)
  - feature_flag: `DEVENV_BILLING_ENABLED=true` (default true; kill switch)
  - alerts: PagerDuty após 3 falhas consecutivas
  - DOD:
    - test: D1 error → throw
    - test: billingSeq incrementa por sessão
    - property test (P1): 2x `recordUsage` = mesmo billing (idempotente)
    - property test (P3): billingSeq incrementa exatamente 1 por sessão
    - test integração: server emite billing corretamente
  - responsible: Engenheiro backend + SRE
  - estimate: 2-3 dias (1 dia código + 1 dia tests + 0.5 dia PagerDuty)
  - priority: HIGH

## Wave 4 (docs + cleanup, paralelo, 2-3 dias)

### PR-4a: D5 + D6 + D7
- **Spec:** N/A (docs)
- **Files:**
  - `corelink-server/docs/devenv/quickstart.md:21,45` — `cl_pat_` → `corelink_pat_`
  - `corelink-server/docs/devenv/resizing.md:28` — `cl_pat_` → `corelink_pat_` E atualizar auth (não é `Authorization: Bearer`, é `X-Exec-Token`)
  - `corelink-server/docs/devenv/limits.md:28` — "24 hours" → "8 hours"
  - `corelink-server/docs/devenv/limits.md:30` — remover "100 concurrent WS" (não implementado) OU marcar "TBD"
- **Responsible:** Engenheiro de docs
- **DOD:** 4 mudanças + review de linguagem
- **Estimate:** 4h
- **Priority:** MEDIUM

### PR-4b: D9 (US-19 orfão)
- **Spec:** N/A (cleanup)
- **Files:** `corelink-runners/deploy/cloudflare/test/e2e-40-stories-driver.test.ts:427-438` REMOVIDO
- **Spec WP-04:** atualizado para remover menção a `--generation-id`
- **Responsible:** Engenheiro de QA
- **DOD:** spec WP-04 diz "OCC descontinuado"; test removido; `cargo test --workspace --release` exit 0
- **Estimate:** 2h
- **Priority:** LOW

### PR-4c: D12 (lint, OPCIONAL)
- **Spec:** N/A
- **Responsible:** Engenheiro de tooling
- **DOD:** `npx eslint .` exit 0
- **Estimate:** 4h (setup) + 1h/mês (manutenção)
- **Priority:** LOW (cortar se custo > benefício)

## Wave 5 (processos, 1-2 dias)

### PR-5a: D3 (workflows gates) — HIGH
- **Spec:**
  - pre: 0 de 119 workflows mencionam devenv
  - post: `.github/workflows/devenv-ci.yml` valida devenv em PRs
  - invariant: zero PR devenv mergeado sem validação
  - safety: regressões devenv bloqueadas em PR
  - liveness: workflow < 10min
  - rollback: deletar workflow
  - feature_flag: N/A
  - alerts: PagerDuty quando workflow falha > 1h
  - DOD:
    - Workflow criado com `on: pull_request`
    - Jobs: `cargo-test`, `tsc-check`, `vitest`, `gitleaks`, `changelog-validate`
    - Headroom: < 20 workflows totais (server tem 119 — verificar!)
    - `pre-merge-gate-check.sh` atualizado com `devenv-ci` em `REQUIRED_PRESENT`
  - responsible: Engenheiro de tooling
  - estimate: 1 dia
  - priority: HIGH

### PR-5b: D11 (supply chain prevention) — CRITICAL
- **Spec:**
  - pre: reviewer humano pode aprovar código malicioso
  - post: `cargo test` deve passar antes de merge (D11 é automático, não manual)
  - invariant: zero PR merged com `cargo test` falhando
  - safety: zero PR malicioso mergeado
  - liveness: PR review + CI test < 30min
  - rollback: revert commit
  - feature_flag: N/A
  - alerts: log warning quando PR é mergeado com cargo test skipped
  - DOD:
    - `.github/workflows/devenv-review.yml` com `on: pull_request` (NÃO `pull_request_target` — menos permissões)
    - jobs: `review-validation` que roda `cargo test -p corelink-server --lib` e `cargo test --workspace --release` (runners)
    - PR bloqueado se tests falham
  - responsible: Engenheiro de segurança
  - estimate: 1 dia
  - priority: CRITICAL

## Wave 6 (fixes opcionais)

Se sobrar tempo (improvável dado cronograma):

- **Property tests P1-P6** (Lamport): se 6 properties × 2-4h = 12-24h. **P1 (idempotência) e P3 (atomicidade) são críticos. P2, P4, P5, P6 podem ser diferidos.**
- **Coverage threshold enforcement**: 70/60/80 são chutes. Medir baseline, depois setar thresholds. **NÃO bloqueia.**

## Cronograma REAL (calculado)

Cada PR × estimate realista:

| Wave | PR | Estimate | Responsável |
|------|-----|----------|------------|
| -1 | 5 pre-flights | 6h | Mix |
| 0 | D13 (rustfmt) | 30min | Backend |
| 0 | N11 (DSR) | 1h | Backend + DPO |
| 0 | D4 (OpenRouter) | 4h | Security |
| 1.5 | D8 + D10 + Coverage | 5-7 dias | QA |
| 2 | N8 (WS) | 2-3 dias | Backend + SRE |
| 2 | N10 (resize) | 2 dias | Backend |
| 2 | N3+N6+N9 | 4-6h | Backend |
| 3 | B5 (D1 binding) | 4h | Backend + SRE |
| 3 | N4 (guard) | 2-3 dias | Backend + SRE |
| 3 | N5 (billing) | 2-3 dias | Backend + SRE |
| 4 | D5/D6/D7 (docs) | 4h | Docs |
| 4 | D9 (US-19) | 2h | QA |
| 4 | D12 (lint, opcional) | 4h | Tooling (cortar?) |
| 5 | D3 (workflows) | 1 dia | Tooling |
| 5 | D11 (review) | 1 dia | Security |

**Total: 18-26 dias úteis (4-5 semanas)**, NÃO 10-12.

## Comparação v1 vs v2 vs v3

| Métrica | v1 | v2 | v3 |
|---------|-----|-----|-----|
| PRs | 16 | 17 | 17 |
| Ondas | 7 | 8 | 10 |
| Estimativa | 5-7 dias (chute) | 10-12 dias (ainda chute) | **18-26 dias (real, baseado em PRs × estimates)** |
| Specs formais | 0 | 1 (template) | **17 (1 por PR, com Lamport format)** |
| Property tests | 0 | listados | **P1+P3 críticos, P2/P4/P5/P6 diferidos** |
| Coverage | inexistente | "configurar" (sem threshold) | **OPCIONAL, não bloqueante** |
| Lamport compliance | "citado" | "incorporado" | **REAL (specs + ordering + properties + safety/liveness)** |
| Responsible | "user" (vago) | "user" (ainda vago) | **Nomeado por PR** |
| DOD | inexistente | por wave (vago) | **por PR (6 checks objetivos)** |
| Risk register | inexistente | genérico | **explícito por fix** |
| Cronograma audit (v3 Nuclear) | N/A | N/A | **18-26 dias, com overhead de review incluso** |

## Próximo passo

User confirma v3. Eu executo Wave -1 HOJE.

**Crítico:** Wave -1 NÃO é opcional. N11 pode ser violação ativa. Audit HOJE.
