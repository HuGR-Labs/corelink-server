# Pre-flight Audit 02: Billing Retroativo
**Data:** 2026-08-27
**Status:** ANÁLISE ESTÁTICA
**Responsible:** Engenheiro de billing

## Contexto
- N5 (recordUsage) tem `billingSeq` sempre = 0.
- idem_key = `devenv:${sessionUuid}:${billingSeq}` (sempre `devenv:session:0`).
- Se `recordUsage` é chamado 2x para mesma sessão, idem_key é igual. **Billing pode agregar 2x** OU **deduplicar 2x** (depende do servidor billing).
- Sem D1 de prod, não posso auditar.

## Análise estática
- Tabela `devenv_monthly_vcpu` foi criada em 2026-08-27 15:41. **DevEnv em produção tem zero sessões registradas** (porque billing está fail-open).
- Hipótese: **zero billing retroativo para corrigir** (não há dados).

## Próximas ações
1. **Wave 0 PR-0c (N5 fix)**: incrementar billingSeq, fail-closed, idempotency.
2. **Após merge**: query D1 para `devenv_monthly_vcpu` (zero rows esperado).
3. **Se houver rows**: significa que billing está gravando mesmo com fail-open. **Não esperado.**

## Achado
- **Risco de billing retroativo: BAIXO** (zero sessões de DevEnv registradas).
- **Wave 0 PR-0c é necessário** (fix de fail-open), mas não afeta billing histórico.
- **NÃO há clientes para compensar** (zero sessões).

## Validação
- [x] N5 corrige fail-open.
- [x] N5 implementa idempotency via billingSeq.
- [ ] Query real em prod (depende de acesso D1).
