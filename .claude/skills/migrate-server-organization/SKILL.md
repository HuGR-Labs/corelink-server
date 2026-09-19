---
name: migrate-server-organization
version: 1.0.0
description: Mapear, preparar e verificar uma mudança de organização GitHub do corelink-server com evidências por commit, gates e identidades independentes. Usar para pedidos de migration, transfer repository, trocar org, preflight ou postflight do server. Padrão somente descoberta e planejamento; transferência exige autorização específica posterior.
---

# Migração de organização do corelink-server

## Quando usar e escopo

Aplicar a pedidos de mapa, documentação, preparação ou verificação da transferência do **repository ID 1232040291**, nome `corelink-server`. Não é skill de migração Cloudflare, dados, billing, GitHub App ou repos irmãos. `corelink-runners` e `corelink-workspaces` têm frentes separadas; `corelink-cli` é destino de distribuição independente.

Pedido de documentação autoriza produzir documentos/ferramentas de preparação no repo; não autoriza transferir, publicar releases, implantar código, mudar credenciais ou mutar os irmãos. Se o destino estiver ausente, produzir o mapa e o plano parametrizado e registrar o bloqueio, sem inventar owner/ID ou parar toda a entrega.

## Contexto mínimo e referências

Ler primeiro o `CLAUDE.md` atual e o [índice da campanha](../../../docs/operations/repository-org-migration/README.md). Depois carregar conforme a etapa:

| Necessidade | Referência |
| --- | --- |
| Escopo e dependências | [MAP](../../../docs/operations/repository-org-migration/MAP.md) |
| Adaptadores e confiança | [PORTABILITY](../../../docs/operations/repository-org-migration/PORTABILITY.md) |
| Ações e recuperação | [RUNBOOK](../../../docs/operations/repository-org-migration/RUNBOOK.md) |
| Trabalho por critério | [WORK-PACKAGES](../../../docs/operations/repository-org-migration/WORK-PACKAGES.md) |
| Gates e registro | [ACCEPTANCE](../../../docs/operations/repository-org-migration/ACCEPTANCE.md) |
| População e limites | [EVIDENCE](../../../docs/operations/repository-org-migration/EVIDENCE.md) |
| Âncoras imutáveis e fontes oficiais | [SOURCE-ANCHORS](../../../docs/operations/repository-org-migration/SOURCE-ANCHORS.md) |

O snapshot de 19/09/2026 é baseline, não verdade atual. Conferir repo por API e commit antes de mudar algo. Usar o conector GitHub autorizado ou `gh` autenticado; erro de ferramenta não autoriza inferir estado. Reconsultar documentação oficial de transferência/OIDC/Packages na data da execução. Recuperar os conceitos OKF dos arquivos realmente alterados; ausência de match não autoriza inventar documentação.

## Entradas obrigatórias por etapa

Descoberta: fonte canônica, ID esperado, ref/SHA confirmado e escopo. Plano: manifesto válido, login/ID do destino conferidos e papéis independentes. Preparação de código: política aprovada, work packages, escopo de arquivos e provas. Corte: G01–G08, FREEZE_SHA, hash do manifesto, janela/limites, operador/revisor e aprovação explícita de origem→destino. Verificação pós-corte: freeze pack, readbacks, runs e provas de comportamento.

## Procedimento

1. **Ancorar.** Ler identidade/permissions e SHA atual; conferir `id=1232040291`, owner real, private=true e nome. Inventariar dirty state. Usar worktree/branch isolado; preservar o trabalho alheio e nunca ampliar o escopo pelo que estiver aberto no Mac.
2. **Descobrir.** Executar scanner por commit e snapshot somente GET. Enumerar source e settings, incluindo páginas adicionais e superfícies não observáveis. Conferir dependências por ID, separando owner do server, dos irmãos, da CLI e da App. Não confundir quantidade de references com quantidade de patches.
3. **Planejar.** Produzir manifesto, mapa M01–M12, disposições e registro G01–G10. `plan` não valida acesso nem aprova transferência. Explicitar todo dado faltante com teste/responsável necessário para resolvê-lo.
4. **Preparar apenas o autorizado.** Para documentação, entregar o kit sem ativar mudanças de produção. Para normalização aprovada, executar WP-02 com adaptadores e testes, preservando o funcionamento na origem. Não aplicar substituição textual global.
5. **Revisar.** Revisão do próprio trabalho deve ser identificada como self-review; revisão independente exige outra pessoa/agente e evidência própria. Rodar testes e gates no head exato. Não declarar CI verde se só testes locais passaram.
6. **Publicar.** Commit com DCO, branch e PR, inventário/limites/resultados descritos. Não fazer merge com checks pendentes/falhando nem bypass do pre-merge gate. Repo privado continua privado; não enviar código/evidência a terceiros.
7. **Executar somente sob novo escopo explícito.** Seguir RUNBOOK integralmente. Falha/unknown aplicável bloqueia antes do corte. Resposta ambígua exige readback, não retry. Emitir uma única transferência após G08. Nenhum helper deste kit possui comando de transferência.
8. **Comprovar e encerrar.** G09/G10 exigem server real no novo owner, contratos dos irmãos, políticas criptográficas e observação; artefatos sintéticos pré-corte não substituem isso. Organizar PRs/branches e registrar encerramento sem confundir kit entregue com migração concluída.

## Comandos de descoberta

Executar na raiz do repo, usando um diretório temporário novo para outputs. Não escrever relatórios sobre arquivos de configuração existentes.

```bash
python3 scripts/repository_org_migration.py scan --revision "$CONFIRMED_SHA"
python3 scripts/repository_org_snapshot.py --owner "$CONFIRMED_SOURCE_OWNER"
python3 scripts/repository_org_migration.py plan \
  --target-owner "$CONFIRMED_TARGET_OWNER" \
  --target-owner-id "$CONFIRMED_TARGET_OWNER_ID"
python3 scripts/repository_org_migration.py compare "$BEFORE_SCAN" "$AFTER_SCAN"
python3 -m unittest discover -s tests -p test_repository_org_migration.py -v
```

`scan` opera em blobs Git, não no working tree. `compare` só relata delta. `plan` deixa todos os gates UNVERIFIED. Exit 0 dessas ferramentas não é readiness; exit 2 do snapshot preserva falhas de observação. Não mascarar resultados negativos.

## Invariantes obrigatórios

Repository ID/nome/private e objetos Git inventariados permanecem; cloud accounts/recursos/domínios/tenants não se movem. CLI, Go module e App owner não derivam automaticamente do owner do server. Credenciais aparecem apenas por nome/escopo, nunca por valor. Artefatos e assinaturas históricas são preservados.

Guardas não viram tautologias nem curingas. Política esperada não pode vir do objeto sendo validado. Claims OIDC atuais devem ser observados: owner ID pode mudar mesmo com repo ID estável. Não assumir que segredo persistente, runner online ou HTTP 200 provam consumo correto.

Não usar tag/release como smoke: o server tem encadeamento para deploy de cinco regiões e migrações de D1. Não iniciar builds pesados no Mac para validar documentação. Não mudar runner para hosted pago, desativar monitores/backups em massa ou reiniciar serviços de outras frentes.

## Formato de saída

Entregar: modo executado; identidade/commit/timestamp; arquivos/PRs produzidos; evidências e comandos realmente rodados; tabela de gates e bloqueios; o que não foi executado; próximo work package com entrada e prova exigidas. Para documentação, finalizar claramente com “kit documentado/publicado” e “transferência não executada”. Nunca chamar revisão própria de independente ou prometer observação posterior sem ferramenta/agendamento autorizado.
