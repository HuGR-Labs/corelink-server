# Pre-flight Audit 05: Coverage Gate
**Data:** 2026-08-27
**Status:** PENDENTE (NÃO BLOQUEANTE)
**Responsible:** Engenheiro de testes

## Contexto
- Plano v3 (FIX_PLAN.md) diz "Coverage gate (c8/v8) é OPCIONAL, não bloqueante".
- Mas o Agent 3 (House) apontou que sem coverage, os 524 vitest tests + 1278 Rust tests podem estar exercitando 5% do código.
- v8 é nativo do V8 (Node), funciona com vitest sem instalar nada.

## Ações (opcional, não bloqueia Wave 0)
1. Adicionar em `corelink-runners/deploy/cloudflare/vitest.config.ts`:
   ```ts
   test: {
     alias: {...},
     coverage: {
       provider: 'v8',
       reporter: ['text', 'html'],
       thresholds: { lines: 50, branches: 40, functions: 50 }  // chute, refinar depois
     }
   }
   ```
2. Adicionar em `corelink-server/apps/admin-ui/vitest.config.ts` (se existir).
3. **NÃO adicionar para Rust** (cargo-llvm-cov leva 4-8h setup).
4. **Threshold inicial: 50/40/50** (chute, refinar com baseline real).

## Achado
- **Coverage é nice-to-have, não bloqueante.**
- **House tem razão** que sem coverage não sabemos o que os tests pegam.
- **Solução pragmática**: configurar coverage NO Wave 1.5 (junto com D8), não Wave -1.
- **Coverage vai virar bloqueante APÓS** Wave 1.5 (se coverage < 50% em código novo, fail PR).

## Validação
- [ ] vitest.config.ts tem `coverage: { provider: 'v8' }`.
- [ ] `npx vitest run --coverage` exit 0.
- [ ] Wave 1.5 PR falha se coverage < 50% em código novo (criar novo PR para isso).
