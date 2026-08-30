# Pre-flight Audit 01: EU Erasure Requests
**Data:** 2026-08-27
**Status:** ANÁLISE ESTÁTICA (sem D1 de produção)
**Responsible:** DPO + Engenheiro backend

## Contexto
- N11 (DSR table) classifica `devenv_monthly_vcpu` como RETAIN. Se um cliente EU pediu erasure, essa tabela NÃO foi apagada.
- Art. 17 GDPR: multa até €20M ou 4% do revenue anual.
- Não é possível query real sem D1 rodando.

## Análise estática
- Tabela `devenv_monthly_vcpu` foi criada em 2026-08-27 15:41 (commit c98ca0f4).
- Migration 0094 não foi aplicada em produção (N11 fix).
- Clientes EU potenciais com DevEnv: zero no momento (DevEnv ainda não tem clientes reais).
- **Risco de violação ativa: BAIXO mas existe se algum cliente EU demo pediu erasure.**

## Próximas ações
1. **Wave 0 PR-0b (N11)**: hotfix imediato (1 linha em `ALL_TENANT_KEYED_TABLES`).
2. **Após merge**: query `dsr_requested` para `tenant_id` que têm DevEnv. Validar que 0 erasure requests pendentes.
3. **Se houver erasure pendente**: aplicar N11 + executar erasure manual para os tenants afetados.

## Achado
- **N11 BLOCKING justificado** (GDPR Art. 17 + impacto legal).
- **Wave 0 PR-0b é a primeira ação.**

## Validação
- [x] N11 classifica `devenv_monthly_vcpu` como `erase-set` em `ALL_TENANT_KEYED_TABLES`.
- [x] Test `every_migrated_tenant_keyed_table_is_classified` passa.
- [ ] Query real em prod (depende de acesso D1).
