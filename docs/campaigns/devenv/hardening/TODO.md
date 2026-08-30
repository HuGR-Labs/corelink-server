# Hardening TODO — 25 itens para chegar a 100% confiança

**Data:** 2026-08-29
**Status:** PLANEJAMENTO
**Critério de "pronto":** TODOS os 25 itens marcados ✅

---

## Categoria 1: Validação em CI real (3 itens)

### [ ] 1. Rodar tests em CI real (não só local)
- Owner: M3
- Tempo: 30min
- Comandos:
  - `gh run watch <PR>` para PRs merged
  - Validar gates passaram em CI
- Acceptance: PRs merged com CI all-green

### [ ] 2. Rodar mutation testing
- Owner: M3
- Tempo: 1h
- Comandos:
  - `cargo install cargo-mutants`
  - `cargo mutants --workspace --no-shuffle --minimum-test-timeout=600`
  - Para corredores: ~4h (não vou rodar)
- Acceptance: 80% mutants caught

### [ ] 3. Latência baseline (P1)
- Owner: M3
- Tempo: 30min
- Comandos:
  - `wrk -t2 -c10 -d30s https://corelink-api.humangr.com/v1/customer/devenv`
  - OU `cargo bench` local
- Acceptance: P99 latência < 200ms

## Categoria 2: Hardening de código (5 itens)

### [ ] 4. Cache local 60s em N4 (P1)
- Owner: M3
- Tempo: 2h
- Arquivo: `corelink-runners/deploy/cloudflare/src/durable_objects/runner_dev_env.ts`
- Approach: cache `max_concurrent` em `instance.local` (memory), TTL 60s
- Acceptance: P99 D1 query rate < 5% do original

### [ ] 5. Circuit breaker em N5 (P1)
- Owner: M3
- Tempo: 2h
- Arquivo: `corelink-runners/deploy/cloudflare/src/durable_objects/runner_dev_env.ts`
- Approach: circuit breaker pattern em `pushUsageEvent` (3 fails → open, 30s → half-open)
- Acceptance: zero billing outage em 1h outage

### [ ] 6. BLAKE3-256 idem_key em N5 (P3/P5)
- Owner: M3
- Tempo: 2h
- Arquivo: `corelink-runners/deploy/cloudflare/src/durable_objects/runner_dev_env.ts`
- Approach: replace `devenv:session:0` com `BLAKE3-256(tenant_uuid|container|startedAt)`
- Acceptance: idem_key uniqueness test passa; 64-char hex format

### [ ] 7. Atomicity em N4 (P5)
- Owner: M3
- Tempo: 3h
- Arquivo: `corelink-server/worker/src/lib/devenv_guard.ts`
- Approach: `INSERT OR IGNORE` com retry pattern; ou Lua-script D1 (server-side)
- Acceptance: race test (1000 concurrent requests) → 0 duplicates, 0 lost

### [ ] 8. Linearizability em N5 (P5)
- Owner: M3
- Tempo: 4h
- Arquivo: `corelink-runners/deploy/cloudflare/src/durable_objects/runner_dev_env.ts`
- Approach: outbox pattern (write to local first, async sync to remote)
- Acceptance: zero lost records em 1h D1 outage

### [ ] 9. Input validation em N3 (P5)
- Owner: M3
- Tempo: 1h
- Arquivo: `corelink-runners/deploy/cloudflare/src/durable_objects/runner_dev_env.ts`
- Approach: reject argv > 100 items, reject total length > 4KB
- Acceptance: 11KB payload rejected com 413

### [ ] 10. State machine spec formal (P5)
- Owner: M3
- Tempo: 2h
- Arquivo: `docs/campaigns/devenv/specs/STATE_MACHINE.md`
- Approach: write TLA+ spec, run TLC checker
- Acceptance: TLC passes; doc em main

## Categoria 3: Correções de spec (2 itens)

### [ ] 11. `getWebSocketAutoResponseTimestamp` em N8 (P4)
- Owner: M3
- Tempo: 2h
- Arquivo: `corelink-runners/deploy/cloudflare/src/durable_objects/runner_dev_env.ts`
- Approach: em `webSocketMessage`, register listener again no CF SDK
- Acceptance: WS mantém conexão após hibernation

### [ ] 12. Hardening do fallback 404 no DO (P2)
- Owner: M3
- Tempo: 1h
- Arquivo: `corelink-runners/deploy/cloudflare/src/durable_objects/runner_dev_env.ts`
- Approach: replace "Fallback: proxy to container" com `return new Response('not found', 404);`
- Acceptance: SSRF scan passa; nenhum proxy indevido

## Categoria 4: Cobertura enforced (2 itens)

### [ ] 13. Coverage threshold enforced em CI
- Owner: M3
- Tempo: 2h
- Arquivos: `vitest.config.ts` (ambos repos)
- Approach: `thresholds: { lines: 50, branches: 40, functions: 50 }` no `coverage`
- Acceptance: CI falha se coverage < 50%

### [ ] 14. Tests N11 rodados após merge
- Owner: M3
- Tempo: 5min (build cache)
- Comandos: `cargo test -p corelink-server --lib routes::dsr`
- Acceptance: 6/6 DSR tests passing

## Categoria 5: PRs de patches (3 itens)

### [ ] 15. PR separado com patches 1-14
- Owner: M3
- Tempo: 2h
- Approach: branch `hardening-v1`, 1 commit por patch
- Acceptance: 17 PRs merged; CI all-green

### [ ] 16. PR com FIX_PLAN v3 + preflights + status
- Owner: M3
- Tempo: 30min
- Approach: cherry-pick b01c7cac
- Acceptance: docs merged

### [ ] 17. PRs com pareceres 1-5
- Owner: M3
- Tempo: 1h
- Approach: docs de audit em `docs/audits/`
- Acceptance: pareceres merged

## Categoria 6: Problemas pré-existentes (5 itens)

### [ ] 18. WP-04 vs WP-06 inconsistency
- Owner: M3
- Tempo: 1h
- Approach: WP-04 §3.1.1 corrigido para 9090
- Acceptance: docs merge

### [ ] 19. `HARD_MAX_SESSION_MS` 8h vs limits.md 24h
- Owner: M3
- Tempo: 30min
- Approach: corrigir `limits.md` para "8 hours"
- Acceptance: doc merge

### [ ] 20. 100 concurrent WS não implementado
- Owner: M3
- Tempo: 1h
- Approach: implementar limite ou remover do doc
- Acceptance: code merge + doc

### [ ] 21. BLAKE3-256 idem_key vs `devenv:session:0`
- Owner: M3
- Tempo: covered by item 6
- Acceptance: ver item 6

### [ ] 22. Limpar docs pendentes
- Owner: M3
- Tempo: 30min
- Approach: deletar untracked, commitar real
- Acceptance: working tree limpo

## Categoria 7: Verificação final (3 itens)

### [ ] 23. Re-rodar tudo (cargo + vitest + tsc + wrangler)
- Owner: M3
- Tempo: 1h
- Acceptance: zero failures

### [ ] 24. Medir cobertura final
- Owner: M3
- Tempo: 30min
- Acceptance: 80%+ ambos

### [ ] 25. Atualizar FIX_PLAN com status final
- Owner: M3
- Tempo: 1h
- Acceptance: 25/25 marked

---

## Resumo

- **Total:** 25 itens
- **Tempo estimado:** 25-35h (3-4 dias úteis)
- **Critério "pronto":** 25/25 marked ✅
- **Sequência:** 1-7 (validation), 8-12 (hardening), 13-14 (coverage), 15-17 (PRs), 18-22 (pre-existing), 23-25 (verify)
