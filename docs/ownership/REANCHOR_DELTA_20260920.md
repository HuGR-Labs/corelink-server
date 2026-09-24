---
schema: corelink-ownership/1.1
document: campaign_reanchor_delta
baseline: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
observed_main: cbbac2341944d4766e3457774623225075fb9564
observed_at: 2026-09-20
state: reconciliation_required
---

# Delta de reancoragem — 2026-09-20

Este registro compara a baseline documental com a ponta remota observada de
main. Não faz checkout, rebase, publicação ou afirmação sobre execução da
ponta remota.

## Método

    git ls-remote origin refs/heads/main
    git fetch --no-write-fetch-head origin cbbac2341944d4766e3457774623225075fb9564
    git diff --name-status cca798ff5 cbbac234 -- Cargo.toml Cargo.lock '**/Cargo.toml' docs/knowledge docs/internal/okf-wiki docs/ownership .claude/skills

main avançou de cca798ff para cbbac234. O diff total contém 120 arquivos; os
resultados abaixo delimitam somente o impacto conhecido para a campanha.

## Impacto classificado

| Área | Evidência de delta | Efeito na campanha |
|---|---|---|
| Workspace | Cargo.toml raiz não mudou | população não foi recontada nesta ponta |
| Manifests | cinco manifests e Cargo.lock mudaram | reconciliação seletiva exigida |
| corelink-hash | somente repository mudou para HuGR-dev/corelink-server | quatro aprovações históricas permanecem evidência da baseline, mas não aprovam bytes da ponta atual |
| Outros pilotos | sem diff direto em corelink-container, billing, CF bindings ou e2e-billing-flow | continuar como rascunhos na baseline; confirmar antes de freeze |
| OKF/ownership | nenhum diff em docs/knowledge, docs/internal/okf-wiki ou docs/ownership | não há mudança de política observada neste recorte |
| Contexto transversal | nova skill e documentos de migração organizacional | considerar identidade remota na emissão; não altera identidade Cargo por si |

Os outros manifests modificados são corelink-client-verify, corelink-ops,
corelink-rate-headers e corelink-tenant-path. corelink-ops também declarou
regex; os quatro restantes mudaram somente o campo repository neste delta.

## Decisão provisória

Preservar cca798ff como baseline imutável das revisões e documentos já
escritos. Não rebasear a campanha enquanto o framework comum ainda é
candidato e os três pilotos restantes não estão completos. Antes de freeze ou
emissão, recensear a ponta escolhida, reconciliar os manifests alterados e
refazer qualquer review invalidada pelos bytes ou fontes selecionados.

GitHub/backlog não foi consultado nesta reancoragem; unicidade de issue e
estado remoto continuam desconhecidos.
