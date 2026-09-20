# Mapa de identidade e impacto

## Identidade e escopo

| Papel | Identidade inicial | Regra |
| --- | --- | --- |
| Repositório desta frente | `corelink-server`, ID `1232040291` | Somente este repositório pode mudar de owner nesta campanha. |
| Origem | `HuGR-Labs`, owner ID `311862110` | Confirmar por ID e leitura autenticada; redirect não prova identidade. |
| Destino candidato | `HuGR-dev`, owner ID `331432289` | Confirmado como candidato na #1702; fazer novo readback de identidade, elegibilidade, namespace e autoridade. |
| Visibilidade / nome | privado / `corelink-server` | Invariantes; não renomear nem ampliar audiência. |
| Runners | `corelink-runners`, ID `1266754321` | Frente e propriedade independentes; provar a fleet que executa CI do server. |
| Workspaces | `corelink-workspaces`, ID `1259579816` | Frente independente; manter contrato de API/auth/DevEnv. |
| Distribuição CLI | `corelink-cli`, ID `1251605593` | Destino de releases independente; token continua mínimo e restrito. |

O ID do repositório identifica continuidade; não concede autoridade. Separar endereço atual, peer, destino de publicação, identidade de segurança e evidência histórica. Não criar `ORG` global nem fazer replace global.

## Dentro e fora

Dentro: inventariar o server, seus consumidores e metadados; preparar projeções explícitas; verificar controles, App, OIDC, credenciais, CI, recuperação e contratos de peers; ensaiar e registrar provas. Fora: transferir peers; mover automaticamente GitHub App, registries ou contas cloud; renomear; publicar/deployar; alterar produtos, DNS, issuer Clerk, recursos Cloudflare, billing ou tenants.

Mover o código do repositório Runners não mantém automaticamente a fleet do CI do server. Configuração Actions, secrets/vars, runners, Apps, rulesets, environments, packages, hooks, Git integrations, refs, tags, releases, LFS, assets, wiki/projetos e permissões precisam de leituras próprias, paginação e prova de capacidade. `403`, `404`, truncamento, redirect e busca vazia significam `UNKNOWN` quando não há prova alternativa.

## Matriz de superfícies de controle e hospedagem

Cada linha é uma leitura do plano live, fora do inventário Git. Estado inicial é `UNKNOWN`; nomear owner e evidência autenticada, datada, paginada e de leitura independente antes de aceitar um gate. `null` significa nenhuma evidência anexada ainda. Não coletar valores secretos.

| Superfície | Status inicial | Owner | Evidência inicial / readback exigido |
| --- | --- | --- | --- |
| CODEOWNERS, branch protection, rulesets, bypass e required checks | `UNKNOWN` | `TBD` | `null`; arquivos efetivos + settings dos dois lados, IDs e revisão |
| Repositório base permission, collaborators, teams e reviewers | `UNKNOWN` | `TBD` | `null`; grants paginados, principal IDs, acesso efetivo e negativa |
| Política de tokens org/repo, PAT, SSO enforcement e token approval | `UNKNOWN` | `TBD` | `null`; política e scopes por metadata, nunca token values |
| GitHub Apps: registro, owner/visibilidade, installations, permissions, webhooks | `UNKNOWN` | `TBD` | `null`; App/installation IDs, callback/webhook targets e prova de autoridade |
| Actions workflows, policies, environments, approvals, secrets/vars por nome/scope | `UNKNOWN` | `TBD` | `null`; censo por repo/ref, proteção e consumidores, sem secret values |
| Hosted/self-hosted runners, groups, labels, imagens, services e runner bootstrap | `UNKNOWN` | `TBD` | `null`; mapa runner→repo/job, owner, saúde e canário isolado autorizado |
| Schedules e producers locais: Actions schedule, cron, launchd, queues e timers | `UNKNOWN` | `TBD` | `null`; produtor, host/owner, pausa seletiva, fila drenada e readback |
| Pages, wiki, Projects, Discussions, domains, redirects e metadata pública/privada | `UNKNOWN` | `TBD` | `null`; settings, audience, DNS/domain ownership e conteúdo/redirect readback |
| Packages e registries: GHCR, npm, PyPI, Go proxy e caches | `UNKNOWN` | `TBD` | `null`; package IDs, ACLs, digest, consumer e pull/push negatives |
| Git refs/branches/tags/signatures, releases, assets, LFS e archives | `UNKNOWN` | `TBD` | `null`; manifests completos por ID/digest, parity e restore testado |
| OIDC claims, trust policies, signing keys/certificates, attestations/SLSA provenance | `UNKNOWN` | `TBD` | `null`; claims reais sem JWT, policy independente e bundles históricos/new |
| External integrations, OAuth/Git integrations, hooks, SSO/SCIM e callbacks | `UNKNOWN` | `TBD` | `null`; instalação/principal IDs, endpoint ownership e probes negativos |
| Cloudflare account/zone, DNS, Workers, R2, D1, KV, Durable Objects e runtime | `UNKNOWN` | `TBD` | `null`; account/resource IDs, versions/digests e probes read-only |
| Product/API/Auth/UI, Clerk issuer, Stripe/billing, tenants e customer data | `UNKNOWN` | `TBD` | `null`; contratos, issuer/consumer IDs, tenant isolation e zero data export |
| Inbound/outbound webhooks, event consumers, CI clients e peer repositories | `UNKNOWN` | `TBD` | `null`; owner handoffs, callback consumers e prova dos três topologies |
| Credential consumers, rotation, source/destination readers e source bridge | `UNKNOWN` | `TBD` | `null`; grants por principal, revocation negatives, retained-reader TTL e bridge readback |
| Backup/restore, disaster recovery, billing/limits e operational alerting | `UNKNOWN` | `TBD` | `null`; independent recovery access, parity, impact budgets e alert routes |

Essa matriz inclui os controles GitHub, hosted/self-hosted e externos nomeados na #1702. `source-inventory.json` é exclusivamente Git-tree-only; não descobre nem certifica qualquer linha live acima, permissões atuais, hosted plane ou consumers externos. Busca vazia, `403`/`404`, truncamento, redirect ou falha de ferramenta permanecem `UNKNOWN`.

## Inventário e incerteza

[source-inventory.json](source-inventory.json) documenta somente a árvore Git de `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`. É triagem textual; caminhos, extensões, blobs opacos, histórico não alcançado, settings live e consumidores externos requerem revisão. Cada achado fica `UNRESOLVED` até disposição por ocorrência, owner, evidência e revisor. Não inferir ausência de acesso negado.

Topologias que devem funcionar após a preparação: todos os peers na origem; server no destino com peers ainda na origem; peers em organizações distintas. Mudança de peer, credencial ou scope durante a janela invalida aceite dependente e exige nova coleta.
