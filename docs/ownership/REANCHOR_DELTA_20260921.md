---
schema: corelink-ownership/1.1
document: campaign_reanchor_delta
baseline: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
observed_main: 122fc2fc07ec21b81c1dc2418e7c70181009261e
previous_observation: 1177dad2ca2a9f21c29b5a118aa7944b77147798
observed_at: 2026-09-21
state: selective_reconciliation_required
---

# Delta de reancoragem — 2026-09-21

Esta leitura compara a baseline da campanha com o SHA remoto de `main`
observado em 2026-09-21. Preserva o histórico de autoria/revisão na baseline;
não faz rebase, checkout de `main`, publicação ou alegação sobre execução.

## Método e população

`git ls-remote origin refs/heads/main` retornou `1177dad2ca2a9f21c29b5a118aa7944b77147798`.
Depois, `git fetch --no-tags origin main` trouxe esse objeto para leitura local,
sem mover a branch da campanha ou alterar o checkout de trabalho do usuário.
O commit anterior observado era `cbbac2341944d4766e3457774623225075fb9564`.

Entre a baseline `cca798ff5bc2df660ecf2570ed243eb9775ff3d0` e `main`, o diff
tem 138 arquivos (`5458` inserções, `427` remoções). O `Cargo.toml` raiz do
workspace permaneceu igual; nenhum manifest foi adicionado ou removido.
Cinco manifests first-party existentes e `Cargo.lock` mudaram.
`docs/knowledge`,
`docs/internal/okf-wiki`, `docs/ownership` e a skill de ownership não mudaram.
Assim, nenhuma mudança de população ou de política compartilhada foi observada
neste recorte; o censo de 107 manifests permanece provisório, não certificado.
Os manifestos/fontes Rust dos seis packages da onda 014 não tiveram mudança
direta neste delta. Os quatro packages W013 corelink-cli, corelink-dt-cli,
corelink-dt-reconcile e corelink-dt-webhook, além de corelink-openapi, também
não tiveram fonte/manifesto alterado; `sbom-publish` teve duas fontes alteradas.

## Impacto conhecido para ownership

| Package / área | Delta observado | Efeito documental |
|---|---|---|
| `corelink-hash` | `Cargo.toml` troca `repository` de `HuGR-Labs` para `HuGR-dev` | identidade do pacote permanece; metadado de referência e revisão histórica não aprovam a ponta atual |
| `corelink-client-verify` | mesma troca de `repository` | atualizar o metadado citado antes de freeze; a mudança não prova alteração de consumidores/runtime |
| `corelink-rate-headers` | mesma troca de `repository` | atualizar o metadado citado antes de freeze |
| `corelink-tenant-path` | mesma troca de `repository` | atualizar o metadado citado antes de freeze |
| `corelink-ops` | nova dependência normal declarada `regex = "1"` | atualizar referência e relação de dependência do pacote; resolução/uso/runtime seguem desconhecidos |
| `corelink-audit` | `src/link_hash.rs` mudou | revisar as afirmações/invariantes de hashing ligadas a este módulo |
| `corelink-rate-headers` | README mudou junto ao metadado `repository` | reconciliar qualquer contrato/documentação local que cite o README |
| `sbom-publish` | `src/purl.rs` altera `WORKSPACE_VCS_URL` para `https://github.com/HuGR-dev/corelink-server`; teste adversarial atualiza a expectativa | artefatos SOURCE feitos na baseline precisam reconciliar PURL/contrato fonte e revisão independente renovada |
| Composition / Worker | migrations D1 BYOK, autorização e mint de runner, signup-worker e `wrangler.toml` mudaram | reabrir relações de composição/runtime em `corelink-server` e os pilotos que as citam; ler código/migrations, não inferir operação |
| OKF / padrão | nenhuma mudança encontrada no diff de caminho | manter OKF como referência designada; não há motivo para redefinir política |

O delta contém ainda documentação e ferramentas de migração organizacional,
mudanças de runbook, CI e configuração. Essas mudanças não alteram a identidade
Cargo por si só; antes da emissão futura de issues, confirmar qual identidade
remota/repositório está autorizada no backlog canônico.

## Decisão de campanha

Manter a baseline imutável e não rebasear a branch. Ondas em andamento usam
baselines próprios e registram evidência SOURCE naqueles commits. Antes de
congelar o standard ou publicar issues, reconciliar os sete packages/áreas da
tabela, repetir somente as revisões afetadas por fontes/metadados alterados e
atualizar o censo se novos manifests ou membros surgirem. Aprovações da
baseline nunca serão apresentadas como aprovação de `1177dad2c`.

Esta consulta foi somente Git read/fetch. Não houve push, build, teste,
deploy, runtime observado, busca de issue ou escrita externa.

## Reancoragem adicional — `7211aa22f88e35ffa4ccf2685ee34e9bd3b907dd`

Em nova leitura autenticada de `origin/main`, o remoto avançou de
`1177dad2ca2a9f21c29b5a118aa7944b77147798` para
`7211aa22f88e35ffa4ccf2685ee34e9bd3b907dd`. O delta contém 64 arquivos,
3824 adições e 538 remoções. Não há mudança em `docs/ownership`, no standard
candidate ou no OKF, mas há mudanças materiais em workflows, `Cargo.lock`,
`corelink-billing-stripe-traits`, `corelink-container`/package
`corelink-server`, `corelink-runner-aggregate`, `corelink-stripe-real`,
migrations D1 e `worker/`.

Consequência: os artefatos desses packages e qualquer piloto/relação que cite
`.github/workflows/nightly.yml`, semgrep, billing ingest, webhook inbox,
runner aggregate, Stripe real ou worker precisam de reconciliação SOURCE e
cold rereview dos bytes afetados antes de freeze. Esta leitura não executou
Cargo, CI, migrations, deploy ou runtime e não rebaseou a campanha.

## Reancoragem adicional — `be37ee65c98a5a6e8ce9c19ad8536e6118e28501`

Em nova leitura autenticada de `origin/main`, o remoto avançou de
`7211aa22f88e35ffa4ccf2685ee34e9bd3b907dd` para
`be37ee65c98a5a6e8ce9c19ad8536e6118e28501` (`ci: consolidate public CI lanes on canonical repo`).
O delta inclui mudanças de CI, Worker, `wrangler.toml`, testes e identidade do
clone público. Também há uma divergência documental material: `main` não
contém os 336 caminhos de ownership presentes na branch da campanha (skills
`own-*` e `docs/ownership/**`), com aproximadamente 52 mil linhas de diferença
entre as pontas.

Isso é bloqueio de integração, não autorização para apagar os artefatos. Um PR
futuro precisa decidir explicitamente se reintroduz o framework/documentos no
repositório canônico e reconciliar os contratos novos de CI/Worker; a branch de
campanha não pode ser aplicada cegamente. Até essa decisão, o standard é
candidato, os documentos continuam presos aos pins declarados e nenhuma issue
é publicada.

Esta leitura foi somente `git ls-remote`/`git fetch`/diff local. Não houve
push, build, CI, deploy, runtime ou escrita externa.

## Reancoragem adicional — `122fc2fc07ec21b81c1dc2418e7c70181009261e`

Em 2026-09-22, `origin/main` avançou de `d39988d291ce22ada73a1649a98f1ac7c8052ac6`
para `122fc2fc07ec21b81c1dc2418e7c70181009261e`. O intervalo inclui suites CI
de campanha, reconciliações de billing/owner, correções de observabilidade e
mudanças de auditoria, auth, runner e workflows. As mudanças reforçam que
`corelink-server`, billing, runner, worker, auth, telemetry e os pilotos/e2e
que citam CI ou composição precisam de novo SOURCE readback antes do freeze.

O framework/documentos de ownership continuam ausentes desse `main`; a branch
da campanha permanece isolada e não é rebased automaticamente. Nenhuma dessas
mudanças prova build, CI, deploy ou runtime. Esta consulta foi somente
`git fetch`/diff local.

O diff acumulado da baseline `cca798ff` até este `main` tem **282 arquivos**,
`15555` inserções e `1804` remoções. As maiores superfícies first-party são
`corelink-container` (24 arquivos), `corelink-ops` (10),
`corelink-terraform-drift-consumer` (5), `corelink-handler-cas` (5),
`corelink-billing-stripe-materializer` (5), `corelink-stripe-real` (4),
`corelink-tier-selection` (3), `corelink-runner-aggregate` (3), além de
`corelink-hash`, `corelink-client-verify`, `corelink-audit`, `corelink-rate-headers`,
`corelink-billing-stripe-traits` e `corelink-tenant-path`. Há também mudanças
extensas em `worker/`, `wrangler.toml`, CI e testes de campanha. Portanto a
revisão atual precisa ser seletiva por superfície, mas não pode tratar os pins
da baseline como aprovação do `main`.

## Reancoragem adicional — `b28f362dbd9b0ec7c206fdc5bd8da86af05533a3`

O remoto avançou de `be37ee65c98a5a6e8ce9c19ad8536e6118e28501` para
`b28f362dbd9b0ec7c206fdc5bd8da86af05533a3` (merge de PR #1754). O delta novo é
de seis arquivos: workflow de billing aggregate runner, novo workflow de
campaign CI, migration D1 `0134_runner_aggregate_durable_state.sql` e testes
de estado durável/multi-period. Isso reabre as relações de
`corelink-runner-aggregate`, `corelink-server`, migrations e CI que citam esses
surfaces; não prova execução, deploy ou runtime.

O standard/OKF continuam fora desse delta. A branch da campanha não é rebased;
os artefatos permanecem presos aos pins declarados até reconciliação específica.

## Reancoragem adicional — `d39988d291ce22ada73a1649a98f1ac7c8052ac6`

O remoto avançou de `b28f362dbd9b0ec7c206fdc5bd8da86af05533a3` para
`d39988d291ce22ada73a1649a98f1ac7c8052ac6` (merge de PR #1753). O delta novo
altera seis arquivos de billing Stripe materializer e `corelink-container`,
incluindo D1 HTTP, handlers e testes E2E. Isso reabre as relações de billing,
D1 e composição do piloto `corelink-server`/`corelink-billing`; não prova
execução, deploy ou runtime.

## Reancoragem adicional — `c118d59a78d4c7636402bfed5a5d08e792aaf8fb`

Em 2026-09-22, `origin/main` avançou de `122fc2fc07ec21b81c1dc2418e7c70181009261e`
para `c118d59a78d4c7636402bfed5a5d08e792aaf8fb`. O delta acumulado desde a
baseline passou a **342 arquivos**, com **19328 adições** e **2291 remoções**.
Além de CI/campaign gates e documentação, o avanço altera materialmente
`corelink-container`, billing Stripe/materializer/traits, runner aggregate,
Stripe real, hash/client-verify/rate-headers/tenant-path, Cargo.lock, migrations
D1, Worker, Wrangler e suites de teste.

Consequência: os artefatos desses packages, os cinco pilotos e qualquer relação
que cite essas superfícies continuam exigindo SOURCE readback e cold rereview
contra o novo pin antes de freeze. O `main` remoto ainda não contém os 336
caminhos de ownership desta branch; não houve rebase, push, build, CI, deploy,
migration, runtime ou publicação.

## Reancoragem adicional — `cacc44fc43ee2f481e933419ba88b9a19ac6c8e8`

Após `c118d59a`, o remoto avançou mais **19 arquivos** (+1110/-4), incluindo
workflows de campaign/CI e contratos de capacidade regional, D1, SLA, PagerDuty
e cache-cost, além de verificadores e evidências correspondentes. Isso reabre
relações de CI, capacidade, contratos operacionais e evidência externa; não
prova execução, deploy ou runtime. Os 336 caminhos de ownership continuam
ausentes de `main`; a branch não foi rebased e não houve publicação.

## Reancoragem adicional — `5006fb2fe73f6da0a57be23c13322a2c7bf097d6`

O remoto avançou de `cacc44fc43ee2f481e933419ba88b9a19ac6c8e8` para
`5006fb2fe73f6da0a57be23c13322a2c7bf097d6` com uma alteração em
`.github/workflows/campaign-ci.yml` que adiciona uma sonda explícita de
`adapter_cache`. Isso reabre a relação CI/cache do piloto `corelink-hash` e de
adapters; não prova execução, deploy ou alcance de runtime.

## Reancoragem adicional — `ba5ecee8dd0f79ce73b9a16d07c5ff786b5ef92b`

O remoto avançou de `5006fb2fe73f6da0a57be23c13322a2c7bf097d6` para
`ba5ecee8dd0f79ce73b9a16d07c5ff786b5ef92b` com apenas duas alterações:
`.github/workflows/dco-check.yml` e `.github/workflows/rustfmt.yml`. O commit
move checks obrigatórios para hosted runners; nenhum manifesto Cargo, fonte
dos cinco pilotos ou standard/OKF mudou nesse intervalo. Relações de CI ficam
reancoradas como declaração SOURCE, sem resultado de execução ou prova de
alcance. A branch de campanha não foi rebased e não houve publicação.

## Reancoragem adicional — `7cef578c0081d56b0b159284152709c1fd39ceaa`

O remoto avançou além de `ba5ecee8dd0f79ce73b9a16d07c5ff786b5ef92b` com quatro
arquivos: ajuste de `campaign-ci.yml`, novo workflow
`staging-provision-plan.yml`, mudança em
`corelink-terraform-drift-consumer/src/event.rs` e novo
`scripts/plan_staging_provider.py`. O delta reabre relações de drift,
staging/provider, scripts e CI; não altera os cinco pilotos nem prova
execução, provisionamento, deploy ou runtime. A branch de campanha permanece
isolada e sem publicação.

## Reancoragem adicional — `b2b9fc0d218f7eb6842b57e9419ccedad2313445`

O remoto avançou de `7cef578c0081d56b0b159284152709c1fd39ceaa` para
`b2b9fc0d218f7eb6842b57e9419ccedad2313445` com a expansão de
`.github/workflows/campaign-ci.yml` (+177/-2), adicionando handlers do bundle
hosted de evidências. Isso reabre somente a superfície de CI/campanha; não
altera os cinco pilotos nem prova execução, publicação ou reachability. A
branch de campanha permanece isolada e sem publicação.

## Reancoragem adicional — `e55e73f626a28599aa6628621a90fec6b77b28b9`

O remoto avançou além de `b2b9fc0d218f7eb6842b57e9419ccedad2313445` com lanes
hosted de auditoria/staging/worker, probes read-only, fixtures Workerd e
ajustes de fuzz/CI; também mudou
`crates/corelink-billing/tests/quota_cas_prop_quota_cas.rs`. O teste reabre o
SOURCE readback do piloto billing; os demais são CI/test harness. Nada disso
prova execução, deploy, runtime ou publicação.

## Reancoragem adicional — `4b9a8469ec3e713b6df3b7562dfd70373121590b`

O remoto avançou além de `e55e73f626a28599aa6628621a90fec6b77b28b9` com **94
arquivos** (+3174/-709), incluindo `Cargo.toml`, `Cargo.lock`,
`corelink-container` (`adapter_cache`/auditoria), `wrangler.toml`, documentação
OKF e testes. O delta reabre as superfícies dos pilotos hash/server e relações
de composição; nenhum artefato da campanha foi rebased e nenhuma execução,
deploy ou runtime foi observada.
