# Portabilidade por contrato

Este é o desenho de implementação de WP-02. Os adaptadores abaixo são propostos; **não estão ativados pela existência do manifesto da campanha**.

## 1. Autoridades independentes

Usar um pequeno catálogo versionado, schema estrito, adaptadores nos pontos de leitura e testes de consistência. Não criar serviço de descoberta, banco ou framework genérico. Separar `server`, `runners`, `workspaces`, `cli_distribution`, owner da GitHub App, recursos cloud e política de assinatura por geração. Os quatro primeiros já são discriminados no manifesto de preparação.

Repository ID reconhece o mesmo repo antes/depois; não autoriza sozinho qualquer ato. Owner aprovado, evento, branch/tag, ambiente, workflow e revisão continuam sendo condições. Transferir ownership continua sendo uma mudança de confiança.

## 2. Routing não é autorização

Dentro de uma lane confiável, construir URLs para seu run/deployment com contexto GitHub validado (`GITHUB_REPOSITORY`, `github.repository`, `github.server_url`). Ferramentas locais recebem `--repo` explícito ou catálogo e confirmam por GET o ID canônico. Um redirect localiza; não prova ownership antigo.

Guardas privilegiadas preservam evento, ref, proteção exigida e ID conhecido. Não substituir uma igualdade por `github.repository == github.repository`, nem aceitar qualquer owner. Uma política lida do branch não confiável de um PR não pode autorizar o próprio branch. Guardas avaliadas antes do checkout não podem depender de arquivo ainda não carregado: manter pequenas âncoras no YAML confiável e testar sua concordância com o catálogo.

O produtor de proveniência emite identidade atual. O consumidor valida política independente: issuer, repo/owner autorizado, workflow, ref e digests. Nunca derivar o valor esperado apenas do bundle sendo verificado.

## 3. Histórico criptográfico

Preservar bytes, hashes, bundles e metadados assinados de releases existentes. Identidades antigas podem ser admitidas somente para o conjunto histórico inventariado e aprovado, não para novas publicações. Fronteira de geração exige refs/digests e evidência criptográfica verificável; uma data declarada num JSON não basta. Não reescrever Git nem renovar assinaturas antigas para aparentar nova autoria.

A configuração OIDC observada já tem `use_default: true`, `use_immutable_subject: true`, prefixo `repo:HuGR-Labs@311862110/corelink-server@1232040291`. Isso não é token real. Segundo a [referência oficial](SOURCE-ANCHORS.md#fontes-oficiais), transferências posteriores a 15/07/2026 usam formato imutável com owner/repo IDs. Validar novamente na execução. O owner ID muda; o repo ID é preservado. O prefixo gerado por `plan` é uma previsão, nunca prova de configuração do emissor.

Capturar apenas claims selecionados do canário, não o JWT: `iss`, `aud`, `sub`, `repository`, `repository_id`, `repository_owner_id`, `ref`, `workflow_ref` e claims do workflow reutilizável quando presentes. Confirmar audience explícita e suffix de environment/branch/tag/PR. G06 pré-corte prepara política e prova a identidade antiga/fixtures; a emissão efetiva pelo server no novo owner só pode ser comprovada após a transferência, em G09/G10. Não criar dependência circular exigindo essa prova antes do corte.

## 4. Adaptadores previstos

| Consumidor | Entrada proposta | Prova necessária |
| --- | --- | --- |
| API do próprio run/deployment | Contexto GitHub validado | Routing canônico sem relaxar escopo. |
| Guardas YAML pré-checkout | ID server + owner ID aprovado + evento/ref | Base confiável; consistência com catálogo; negativos. |
| Python/shell operacional | `--repo`/catálogo + readback do ID | Recusar repo errado, alias inesperado e configuração ausente. |
| Rust `CosignIdentityPattern` | Política tipada independente | Alterar construtor **e** `matches_simple`; hoje este ignora `self.pattern`. Exercitar todos os entry points. |
| B102/B108/B155/D03 e owner-action packets | Política de geração | Fixtures antigas fixadas; novas evidências com política nova. |
| Docs, Cargo metadata e templates | Papel `server` | Testes contra reintrodução de literals; links respeitam repo privado. |
| Release/sign/notarize/get-CLI | Papel `cli_distribution` | Destino e token não acompanham owner do server. |
| Bootstrap/watchdog runners | Instância, ID e URL autorizados | Mudança por host/serviço, não global no Mac. |
| GitHub App manifest | Owner da App, independente | Decisão explícita; não recriar App ou presumir novo installation ID. |
| Go module e pacotes publicados | Namespace de distribuição próprio | Nenhuma mudança implícita nesta frente. |

Evitar espalhar dezenas de novas variáveis de ambiente: adaptar os poucos entry points compartilhados. Não usar fallback silencioso para o owner antigo quando faltar configuração privilegiada.

## 5. Critérios anti-drift da implementação

O scanner do kit lê uma árvore Git, não arquivos não rastreados, `.env.local` ou alvos de symlink. É triagem textual, não parser de dependências de todas as linguagens; strings concatenadas e settings externos exigem revisão semântica. Binary/LFS/gitlink/limites são declarados no relatório.

A implementação de WP-02 deve enumerar os consumidores ativos e manter disposições por caminho/trecho: patch, histórico imutável, dependência independente ou falso positivo com justificativa. Novo literal ativo não classificado deve falhar. Exceções exigem trecho/hash, motivo, responsável e prova; não permitir exceções amplas por diretório.

O gate futuro precisa rodar ao editar o catálogo, qualquer consumidor, a skill ou a própria regra. Não disparar só por `scripts/**`, pois uma alteração isolada do manifesto também importa. Testar owner simulado em fixture: routing muda, IDs/recursos cloud permanecem, CLI fica e controles não relaxam. O coletor desta entrega nunca transforma geração de dados em aprovação.

## 6. Estratégia de patches

Patch A: catálogo, routing, bootstrap e metadata com testes. Patch B: guardas, identidade de evidência, consumidor Rust e compatibilidade histórica com revisão de segurança. Patch C: docs, handoffs e verificação de população. A/C podem ser agrupados; B deve anteceder qualquer publicação no destino.

Preparação deve funcionar ainda na origem. Um patch que só funciona após transferir precisa de estado de transição explícito e testado; não pode quebrar a origem antecipadamente. Atualizar a geração aprovada não equivale a derivar trust do próprio run.

Depois dessa preparação, uma mudança de owner fica reduzida a parâmetros, coleta/delta, gates, handoffs e uma transferência. Acesso, plano, integração e comportamento continuam precisando de prova em cada destino. O objetivo é eliminar trabalho manual repetitivo, não eliminar controles.
