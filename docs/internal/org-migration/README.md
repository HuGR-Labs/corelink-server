# CoreLink server organization migration kit

Este kit reutilizável organiza auditoria, desenho e verificação de uma possível mudança de organização do repositório `corelink-server` (ID `1232040291`; nome e visibilidade `corelink-server`/private). A origem registrada no perfil é `HuGR-Labs` / owner ID `311862110`; o destino deve ser preenchido e validado por leitura autenticada antes de qualquer operação.

O kit não executa nem autoriza transferência. G00–G19 começam `UNKNOWN`; o template mantém `migration_ready:false` e `authorization_verified:false`. Identidade, evidência e autorização devem ser revalidadas para cada janela de operação.

## Índice

| Caminho | Uso |
| --- | --- |
| [MAP](MAP.md) | Identidades, fronteiras e inventário de superfícies. |
| [DESIGN](DESIGN.md) | Perfis independentes, auditoria e projeções. |
| [RUNBOOK](RUNBOOK.md) | Modos, sequência, paradas e recuperação. |
| [OPERATION-CONTRACT](OPERATION-CONTRACT.md) | Contrato fail-closed de G00–G19 e ledger schema 2. |
| [WORKPACKAGES](WORKPACKAGES.md) | Critérios e dependências WP-00–WP-07. |
| [ADVERSARIAL-REVIEW](ADVERSARIAL-REVIEW.md) | Contraexemplos e condições de bloqueio. |
| [topology.example.json](topology.example.json) | Exemplo de topologia; não é configuração ativa. |
| [gate-ledger.template.json](gate-ledger.template.json) | Ledger novo com 20 gates desconhecidos e sem autorização. |
| [Skill](../../../.claude/skills/corelink-org-migration/SKILL.md) | Protocolo para agentes e operadores. |

## Ferramentas locais

`scripts/org_migration_audit.py` lê objetos de um commit Git e emite caminhos, spans em bytes, hashes e limitações; não consulta rede, arquivos não rastreados ou alvos de symlink. `scripts/org_migration_gate_check.py` valida consistência estrutural de um ledger schema 2. O checker não autentica autoridade, verdade de evidências nem GO.

Os testes focais são `python3 scripts/test_org_migration_audit.py -v` e `python3 scripts/test_org_migration_gate_check.py -v`. Um código de saída zero do checker significa apenas registro consistente; nunca encadeie sua saída a uma transferência.

## Estado

O perfil preserva nome, ID e visibilidade privada. Runners, Workspaces e CLI são peers independentes. GitHub App, Actions, OIDC, credenciais, runners físicos, artefatos, dados, Cloudflare, DNS, Workers, D1, R2, billing e tenants exigem provas próprias. Nenhuma destas ferramentas os modifica.

A auditoria do repositório lê apenas objetos de um commit Git explícito. Ela não é censo de configurações live, permissões, superfícies hosted ou consumidores externos; consultar [MAP](MAP.md) e [RUNBOOK](RUNBOOK.md) para a matriz de controles a preencher.
