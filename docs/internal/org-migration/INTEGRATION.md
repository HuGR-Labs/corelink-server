# Integração e rastreabilidade

## Caminhos canônicos

Esta migração usa somente `.claude/skills/corelink-org-migration/SKILL.md`, `docs/internal/org-migration/` e `scripts/org_migration_{audit,gate_check}.py` com testes focais adjacentes. Os caminhos anteriores `.claude/skills/migrate-server-organization`, `docs/operations/repository-org-migration` e `scripts/repository_org_migration*` foram substituídos para não manter contratos concorrentes.

## Backlog e issue

B-374 permanece `OPEN`, aponta para esta integração preparatória e mantém verify local. A issue guarda-chuva #1702 continua aberta porque inventário atual, consumidores/projeções, controles, restore, ensaio, GO, transferência, comportamento e observação são gates futuros. O merge deste kit não fecha #1702 e não declara WP-01–WP-07 concluídos.

## Verificação focal

Executar após mudança nos artefatos do kit:

```sh
python3 scripts/test_org_migration_audit.py -v
python3 scripts/test_org_migration_gate_check.py -v
```

O relatório de auditoria deve usar SHA explícito. Para novo inventário, gravar saída em diretório privado e registrar perfil, source SHA, timestamp, tool/rules hashes e cobertura. Não sobrescrever o inventário histórico sem nova revisão.

## Revisão de integração

Revisar links locais e paths, política `python-tests.yml` para docs/skill/tool changes, e status B-374 no backlog. Uma revisão independente compara o SHA exato com #1702, cobre efeitos/negativas e confirma que o PR não fecha issue. CI remoto e aceites operacionais pertencem aos responsáveis da integração; recibos locais não os substituem.
