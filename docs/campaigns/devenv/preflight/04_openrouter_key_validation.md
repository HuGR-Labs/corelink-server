# Pre-flight Audit 04: OpenRouter Key
**Data:** 2026-08-27
**Status:** ANÁLISE ESTÁTICA
**Responsible:** Engenheiro de segurança

## Contexto
- D4: key hardcoded em `live-agent-openrouter.mjs:8` (commits 6aa866a, 44d2aaf, 19eb2e1, e8c0ddae).
- Key: **[REDACTED on preservation]** — the literal value was quoted here in the
  original artifact; it is a live-format OpenRouter key and is NOT carried into
  git history. Recover it, if still needed, from the OpenRouter dashboard or the
  commits named above. It must be rotated regardless (see step 3).

## Análise estática
- **Key exposta em 4 commits**. Mesmo se o repo for privado, está em clones locais.
- OpenRouter dashboard tem `Activity` que mostra uso da key.
- **Quem pode acessar o dashboard**: SRE / segurança.

## Ações a tomar antes de D4
1. **SRE acessa OpenRouter dashboard** → Activity → verifica uso da key.
2. **Se houve uso**: anotar timestamps + endpoints chamados. Pode indicar vazamento de dados.
3. **Rotacionar key** no dashboard (revoke old + create new).
4. **Atualizar wrangler secret** `OPENROUTER_API_KEY` no Cloudflare Workers dashboard (ou via `wrangler secret put`).
5. **Wave 0 PR-0c (D4)**: modificar `live-agent-openrouter.mjs` para usar env var.

## Achado
- **D4 é HIGH mas pré-requisito é ação manual de SRE** (OpenRouter dashboard).
- **SRE deve fazer ANTES do Wave 0 PR-0c ser mergeado** (senão key velha ainda está ativa).
- **BFG/filter-repo NÃO é opção** (muda git history, quebra forks). **Solução: rotate + revoke + new key + clean up.**

## Validação
- [ ] OpenRouter dashboard verificado (atividade da key).
- [ ] Key velha revogada no OpenRouter.
- [ ] Nova key criada.
- [ ] `wrangler secret put OPENROUTER_API_KEY` no Cloudflare Workers dashboard.
- [ ] PR-0c (D4) merged.
