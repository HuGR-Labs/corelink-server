---
schema: corelink-ownership/1.3
document: census_readback
source_commit: f78a034de63684db90226aa61d7b8005b47a77d0
state: CAMPAIGN_BRANCH_CERTIFIED_MAIN_RECONCILIATION_REQUIRED
---

# Censo reproducido — 2026-09-21

O censo foi executado em checkout limpo da branch da campanha, sem compilar ou
testar Rust:

```text
python3 docs/ownership/tools/prepare_census.py \
  --repo-root /tmp/corelink-ownership-census \
  --expected-commit f78a034de63684db90226aa61d7b8005b47a77d0 \
  --output-dir /tmp/corelink-census-output-20260921 \
  --scope-decisions docs/ownership/inventory/scope-decisions.json
```

Resultado: 95 membros workspace, 10 packages independentes de fuzz, 105
packages elegíveis e 12 manifests fora do conjunto principal (10 independentes,
1 raiz virtual e 1 arquivo histórico `archive`). A classificação completa foi
aceita pelo tool (`first_party_scope_fully_classified: true`). O JSON canônico
tem SHA-256 `e21d547278e56a227e8cceef18e9d8b774b17dd14a4d7d5708a007acc47ff5b6`.

Este resultado certifica somente a árvore da branch no commit indicado. Como o
`origin/main` atual é `be37ee65c98a5a6e8ce9c19ad8536e6118e28501`, o censo precisa
ser repetido depois da decisão de integração/rebase; não é prova de população
atual do remoto nem autorização para publicação.
