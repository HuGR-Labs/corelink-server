# Pre-flight Audit 03: database_id (B5)
**Data:** 2026-08-27
**Status:** ANÁLISE ESTÁTICA
**Responsible:** Engenheiro backend + SRE

## Contexto
- B5 adiciona `[[d1_databases]]` em `wrangler.jsonc` (corelink-runners).
- database_id precisa ser o mesmo do server (`corelink-prod-d1`).
- Se errado: billing grava em DB errado.

## Análise estática
- Server `wrangler.toml:195-199`:
  ```
  [[d1_databases]]
  binding = "CONFIG_DB"
  database_id = "PLACEHOLDER_D1_CONFIG_DB_ID"
  preview_database_id = "PLACEHOLDER_D1_CONFIG_DB_PREVIEW_ID"
  ```
- `PLACEHOLDER` indica que database_id REAL precisa ser obtido via `wrangler d1 list`.
- Sem acesso a Cloudflare, não posso confirmar.

## Comando a executar antes de B5
```bash
cd corelink-server
wrangler d1 list
# Output esperado:
# corelink-config-d1 (ou similar) - database_id REAL
# Anotar esse database_id
```

## Próximas ações
1. **SRE obtém database_id real** via `wrangler d1 list`.
2. **SRE atualiza** `wrangler.toml:198` com o ID real.
3. **SRE valida** que o ID aparece no `wrangler d1 info <id>`.
4. **Wave 3 PR-3a (B5)** adiciona esse ID em `corelink-runners/deploy/cloudflare/wrangler.jsonc`.

## Achado
- **Pré-requisito bloqueante** para Wave 3 PR-3a.
- Sem database_id real, B5 não pode ser merged.

## Validação
- [ ] database_id real obtido via `wrangler d1 list`.
- [ ] database_id aparece no wrangler.toml do server.
- [ ] B5 PR pode ser mergeado.
