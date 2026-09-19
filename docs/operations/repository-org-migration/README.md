# Migração de organização — corelink-server

**Escopo:** propriedade GitHub de `corelink-server`; manter nome, ID e visibilidade privada. **Transferência não autorizada nem executada.** Destino e seu ID ainda não foram fornecidos.

Base auditada: `cca798ff5bc2df660ecf2570ed243eb9775ff3d0` (`main`). Origem: `HuGR-Labs`, owner ID `311862110`; repository ID `1232040291`. Coleta GitHub em 19/09/2026, com timestamps individuais no snapshot. Redirecionamento não comprova a organização atual.

## Conteúdo

| Documento | Função |
| --- | --- |
| [MAP.md](MAP.md) | Dependências, identidades, pontos de mudança e limites desta frente. |
| [PORTABILITY.md](PORTABILITY.md) | Desenho dos adaptadores para mudanças futuras de owner. |
| [RUNBOOK.md](RUNBOOK.md) | Sequência operacional, gates, interrupção e recuperação. |
| [WORK-PACKAGES.md](WORK-PACKAGES.md) | Pacotes com completeness, success, quality, DoD e invariants. |
| [ACCEPTANCE.md](ACCEPTANCE.md) | Matriz de aceitação, casos negativos e registro de execução. |
| [SOURCE-ANCHORS.md](SOURCE-ANCHORS.md) | Âncoras no commit e documentação oficial consultada. |
| [EVIDENCE.md](EVIDENCE.md) | Cobertura, medições, testes, limites e estado da entrega. |
| [migration.json](migration.json) | Manifesto parametrizável de preparação; não configura produção. |
| [Skill](../../../.claude/skills/migrate-server-organization/SKILL.md) | Protocolo reutilizável para agentes. |

## Contrato de escopo

Trocar a organização GitHub não move conta Cloudflare, D1, R2, KV, Durable Objects, DNS, tenants, Stripe, Clerk, GitHub App, registries ou namespaces de pacotes. `corelink-runners` e `corelink-workspaces` têm frentes separadas. A CLI pública, `HuGR-Labs/corelink-cli`, é outra dependência e não entra nesta transferência.

O kit contém scanner por commit, comparação de árvores, plano parametrizado e coletor GitHub somente GET, com paginação e exclusão de valores de credenciais. Nenhuma ferramenta aplica mudanças de produção. O manifesto **ainda não é consumido pelos workflows, pelo Rust ou pelos aplicativos**: a normalização de produção descrita em WP-02 precisa ser implementada e provada antes do corte.

## Uso e estado

Executar da raiz: `python3 -m unittest discover -s tests -p 'test_repository_org_migration*.py' -v`. Python 3.10+ e Git bastam para testes/scan/plan; o snapshot exige `gh` autenticado em `github.com`. [RUNBOOK.md](RUNBOOK.md) fornece comandos que gravam relatórios em um diretório temporário novo.

Exit codes: scan/plan/compare retornam `0` quando produziram dados, não quando aprovaram uma migração; erro retorna `2`. Snapshot retorna `2` se houver superfície não verificada, preservando o relatório quando possível. Um plano nasce com todos os gates `UNVERIFIED`. A entrega do kit não fecha a migração: G01–G08 antecedem o corte; G09–G10 comprovam identidade, comportamento e observação posterior.

## Reutilizar numa próxima mudança

Este diretório e seus snapshots são evidência histórica desta campanha. Não
sobrescrever a origem de 2026 para simular uma segunda transferência. Criar
uma nova cópia do manifesto para a campanha seguinte, conferir o owner/owner ID
atual e o novo baseline SHA por API/Git e atualizar esses campos na cópia.
Preservar o repository ID e os papéis independentes; incluir owners anteriores
na triagem `legacy_owners`, sem transformá-los em autorização de assinatura.

Passar a cópia como argumento global: `python3 scripts/repository_org_migration.py --manifest "$NEXT_MANIFEST" plan --target-owner "$TARGET_OWNER" --target-owner-id "$TARGET_OWNER_ID"`.
Os novos scans/snapshots, disposições e registro de execução ficam na nova
campanha. Revalidar todos os gates e handoffs; o aceite anterior não autoriza
a próxima mudança. O catálogo de produção, quando implementado em WP-02, é
separado desses registros históricos.
