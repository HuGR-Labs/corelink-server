# Fontes e proveniência

## Baseline local

O inventário versionado [source-inventory.json](source-inventory.json) refere-se ao commit documental `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`, identificado na issue #1702. Ele foi lido da árvore Git, sem working tree; contém hashes/spans, nunca linhas brutas. Não cobre o candidato atual, settings live, consumers externos, branches que não constem naquela árvore ou conteúdo de blob opaco.

Baseline de identidade da issue: `HuGR-Labs` owner ID `311862110`; `corelink-server` repo ID `1232040291`; destino candidato `HuGR-dev` ID `331432289`. Confirmar tudo outra vez no momento da execução. Tratar o perfil como exemplo e evidência como dados privados.

## Fontes oficiais a reconferir no corte

| Fonte | Uso |
| --- | --- |
| [Transferir um repositório GitHub](https://docs.github.com/en/repositories/creating-and-managing-repositories/transferring-a-repository) | Elegibilidade, owners/permissions, namespace, redirects e consequências atuais. |
| [API REST: transfer repository](https://docs.github.com/en/rest/repos/repos#transfer-a-repository) | Autorização, request, resposta assíncrona e readback; nunca retry baseado só em 202/timeout. |
| [Referência OIDC do GitHub Actions](https://docs.github.com/en/actions/reference/security/oidc) | Claims emitidos, issuer/audience e identidade imutável que o consumer realmente valida. |
| [Permissões GitHub Packages](https://docs.github.com/en/packages/learn-github-packages/about-permissions-for-github-packages) | Owner e ACL variam conforme registry/pacote; verificar pacote e consumer reais. |
| [Reusable workflows](https://docs.github.com/en/actions/how-tos/reuse-automations/reuse-workflows) | Resolução suportada de `uses`, refs e workflows internos/externos. |
| [Visibilidade de GitHub Apps](https://docs.github.com/en/apps/creating-github-apps/registering-a-github-app/making-a-github-app-public-or-private) | Visibilidade e estratégia de instalação; código `public:false` não prova registration real. |

Regras de plataforma, API version, issuer OIDC, opções de runner e permissões podem mudar. Os links não atestam configuração da organização, sucesso de transferência ou aprovação. Cada claim operacional precisa de leitura autenticada e evidência do sistema real na janela de uso.
