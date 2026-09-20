# CoreLink server organization migration kit

Este é o kit preparatório de `corelink-server` para a issue [#1702](https://github.com/HuGR-Labs/corelink-server/issues/1702). A origem é `HuGR-Labs` / owner ID `311862110`; o repositório é ID `1232040291`, nome `corelink-server`, privado. `HuGR-dev` / ID `331432289` é o destino candidato registrado na issue. Revalidar ambos por leitura autenticada imediatamente antes de qualquer operação.

**Este PR integra documentação e ferramentas locais. Não executa nem autoriza transferência.** G00–G19 começam `UNKNOWN`; o template mantém `migration_ready:false` e `authorization_verified:false`. B-374 permanece `OPEN` e vinculado à #1702. A integração deste kit não fecha a issue de migração.

## Índice

| Caminho | Uso |
| --- | --- |
| [MAP](MAP.md) | Identidades, fronteiras e inventário de superfícies. |
| [DESIGN](DESIGN.md) | Perfis independentes, auditoria e projeções. |
| [RUNBOOK](RUNBOOK.md) | Modos, sequência, paradas e recuperação. |
| [OPERATION-CONTRACT](OPERATION-CONTRACT.md) | Contrato fail-closed de G00–G19 e ledger schema 2. |
| [WORKPACKAGES](WORKPACKAGES.md) | Critérios e dependências WP-00–WP-07. |
| [SOURCES](SOURCES.md) | Baseline e fontes externas a reconferir. |
| [ADVERSARIAL-REVIEW](ADVERSARIAL-REVIEW.md) | Contraexemplos e condições de bloqueio. |
| [REVIEW](REVIEW.md) | Resultado e limites da revisão desta entrega. |
| [INTEGRATION](INTEGRATION.md) | Caminhos canônicos, backlog e comandos focais. |
| [topology.example.json](topology.example.json) | Exemplo de topologia; não é configuração ativa. |
| [gate-ledger.template.json](gate-ledger.template.json) | Ledger novo com 20 gates desconhecidos e sem autorização. |
| [source-inventory.json](source-inventory.json) | Triagem somente leitura da baseline documental indicada. |
| [Skill](../../../.claude/skills/corelink-org-migration/SKILL.md) | Protocolo para agentes e operadores. |

## Ferramentas locais

`scripts/org_migration_audit.py` lê objetos de um commit Git e emite caminhos, spans em bytes, hashes e limitações; não consulta rede, arquivos não rastreados ou alvos de symlink. `scripts/org_migration_gate_check.py` valida consistência estrutural de um ledger schema 2. O checker não autentica autoridade, verdade de evidências nem GO.

Os testes focais são `python3 scripts/test_org_migration_audit.py -v` e `python3 scripts/test_org_migration_gate_check.py -v`. Um código de saída zero do checker significa apenas registro consistente; nunca encadeie sua saída a uma transferência.

## Estado

O perfil preserva nome, ID e visibilidade privada. Runners, Workspaces e CLI são peers independentes. GitHub App, Actions, OIDC, credenciais, runners físicos, artefatos, dados, Cloudflare, DNS, Workers, D1, R2, billing e tenants exigem provas próprias. Nenhuma destas ferramentas os modifica.

O inventário versionado é uma referência histórica de `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`, não censo do candidato atual nem estado live. Ver [REVIEW](REVIEW.md) e [SOURCES](SOURCES.md) para cobertura e lacunas.
