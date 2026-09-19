# Âncoras e fontes

## Código imutável

Todas as linhas abaixo referem-se ao commit `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`. Para reencontrar um trecho: `git show COMMIT:caminho`; conferir hashes no [inventário](evidence/source-inventory.json). Prefixo dos workflows da tabela: `.github/workflows/`. O inventário completo complementa, não é substituído pela seleção de hotspots.

| Grupo | Caminho e linhas | Evidência |
| --- | --- | --- |
| M01 | `cas_foundation.yml:285-305` | Guarda repo/ref/evento/proteção. |
| M01 | `mutation-nightly.yml:285-355`; `semgrep.yml:126-140` | Nome canônico fixo em guardas. |
| M01 | `real-ignored-harnesses.yml:43-57` | Comparação shell de GITHUB_REPOSITORY. |
| M02 | `perf-production-evidence.yml:65-122,181-215` | APIs do próprio repo, SAN e attestation verification. |
| M02 | `scripts/collect_b102_b108_context.py:60-83`; `scripts/verify_b102_b108_evidence.py:28-49` | Autoridade de repo para evidências. |
| M03 | [`crates/corelink-ops/src/deploy/types.rs:152-195`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-ops/src/deploy/types.rs#L152-L195) | Construtor e matcher fixam owner legado; matcher não usa o campo pattern. |
| M04 | [`release-cli.yml:329-350`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/.github/workflows/release-cli.yml#L329-L350); `438-485` | Repo de distribuição separado e texto de origem. |
| M04 | `sign-linux.yml:145,224,243,258`; `sign-windows.yml:164,270,287,301`; `notarize-macos.yml:174,327,343,357` | Destino CLI explícito nas lanes de assinatura/notarização. |
| M05 | [`release-slsa3.yml:235-280`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/.github/workflows/release-slsa3.yml#L235-L280) | Caller/callee e builder identity derivados do contexto atual. |
| M06 | `apps/signup-worker/src/webhooks/github_app_manifest.ts:117-145` | Owner da App por variável, com fallback legado. |
| M08 | `infra/ci-runners/linux/docker-compose.yml:25-44` | URL de registro e labels do bootstrap. |
| M08 | `scripts/runner_wedge_watchdog.py:55-70,178` | Nome de serviço local e repo default. |
| M09 | `.github/dependabot.yml:178,207,236,266,317,355,392,428,463,495,525,554` | Doze reviewers `HumanGuardrail/security`. |
| M09 | `subprocessors-sync.yml:212-224`; `.github/CODEOWNERS:1-55` | Reviewer legal legado; CODEOWNERS usa principal individual. |
| M10 | `apps/docs/docusaurus.config.ts:24-36`; `apps/docs/tests/config.test.ts:1-20` | Org docs e expectativa do teste. |
| M10 | `corelink-go/go.mod:1`; `corelink-go/corelink_test.go:21` | Module/import namespace independente. |
| M10 | `crates/{corelink-client-verify,corelink-hash,corelink-rate-headers,tenant-path}/Cargo.toml:9` | URLs de metadata dos crates. |
| M11 | `scripts/cut-v1-0-0-ga-tag.sh:588-604`; `scripts/check_runner_fleet.py:40-56`; `scripts/check_workflow_state.py:55-70` | Ferramentas operacionais dependentes do owner. |
| M12 | `scripts/verify_owner_action_packets.py:1214-1230`; `scripts/verify_b155_batch_g.py:590-610,721`; `scripts/verify_d03_graduation.py:133,233,872` | Verificadores que codificam a identidade de origem. |
| Handoff | `workflow-state-guard.yml:92-109` | Consulta explícita ao repo Runners, sem inferir acesso cross-private. |
| CLI | `apps/get-corelink-worker/wrangler.toml:55-84` | RELEASE_ORIGIN da CLI pública e domínio estável. |
| Deploy | [`cf-deploy-prod.yml:29-113`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/.github/workflows/cf-deploy-prod.yml#L29-L113) | Encadeamento de release, confirmação e matriz das cinco regiões. |
| Images | `container-build-push-prod.yml:31-75`; `wrangler.toml:515,783,959,1129,1295` | Pipeline Cloudflare e image pins de produção. |
| Infra | `wrangler.toml:303-378`; `apps/signup-worker/wrangler.toml:73-120`; `infra/terraform/main.tf:9-24` | Recursos/bindings/rotas/state não dependem semanticamente da org GitHub. |

## Leituras GitHub

O [snapshot](evidence/github-snapshot.json) registra endpoint, status e timestamp por superfície. Rotas: `repos/HuGR-Labs/corelink-server` e subrecursos de refs, Actions, secrets/variables por nome, environments, OIDC, webhooks, keys, releases, issues, PRs e acesso; `orgs/HuGR-Labs` para settings, runners/groups e installations. Valores não fazem parte do relatório.

O [readback externo](evidence/dependency-identities.json) contém somente metadata dos três repos dependentes. A seleção dessas leituras não autoriza modificar aqueles repos. A ausência de uma superfície na coleta é uma limitação, não uma observação negativa.

## Fontes oficiais

Consultadas em 19/09/2026. Revalidar na data de execução, especialmente políticas de transferência/OIDC e capacidades de planos.

| Fonte | Uso no procedimento |
| --- | --- |
| [GitHub — Transferring a repository](https://docs.github.com/en/repositories/creating-and-managing-repositories/transferring-a-repository) | Elegibilidade, defaults do destino, preservação de objetos/settings, redirects, LFS e restrições de namespace/retorno. |
| [GitHub REST — Transfer a repository](https://docs.github.com/en/rest/repos/repos#transfer-a-repository) | Endpoint, autorização, input new_owner e resposta assíncrona. |
| [GitHub Actions — OIDC reference](https://docs.github.com/en/actions/reference/security/oidc) | Claims, IDs imutáveis, owner/repo IDs, audience e contexto. |
| [GitHub Packages — Permissions](https://docs.github.com/en/packages/learn-github-packages/about-permissions-for-github-packages) | Ownership/granularidade por registry e acesso do workflow ao pacote. |
| [GitHub Actions — Runner access](https://docs.github.com/en/actions/how-tos/manage-runners/self-hosted-runners/manage-access) | Grupos e elegibilidade de repos/workflows a runners. |

Os cenários de falha, gates, prazos operacionais e divisão de patches são decisões propostas para este projeto, não garantias documentadas do fornecedor. Regras externas devem ser confrontadas com readback e prova local do caso concreto.
