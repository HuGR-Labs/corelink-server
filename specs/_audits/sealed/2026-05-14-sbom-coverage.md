---
id: "AUDIT-R7-1-SBOM-CONSOLIDATED-2026-05-14"
type: "audit"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
sprint: "R7-1"
parent_wi: "R7-1-SUPPLY-QUALITY-ROLLUP"
owner: "Gustavo Schneiter"
tags: ["audit", "sbom", "cyclonedx", "supply-chain", "r7-1"]
---

# Consolidated workspace SBOM — coverage statement (R7-1)

## Escopo

Define a **visão consolidada** de SBOM CycloneDX 1.6 que cobre todo o
workspace CoreLink (`crates/` + `apps/` + tools publicados) num único
documento, complementando o per-binary publisher (`tools/sbom-publish`,
WI-S12-002) que emite SBOMs individuais com NTIA-validation + RFC 3161 TSA +
Dependency-Track ingestion.

| Pergunta                                   | Resposta                                                |
| ------------------------------------------ | ------------------------------------------------------- |
| Formato canônico?                          | CycloneDX 1.6 JSON                                      |
| Top-level component?                       | `pkg:cargo/corelink-workspace@<git-describe>`           |
| Per-binary SBOM continua existindo?        | sim — WI-S12-002 (`tools/sbom-publish`) inalterado      |
| Diferença em relação ao per-binary?        | escopo (workspace inteiro) + dedup transitivo por purl  |
| Gate CI?                                   | `.github/workflows/sbom-consolidated.yml` em release    |
| Retenção do artefato?                      | 365 dias (release evidence)                             |
| Cobre wasm32 + host-server targets?        | sim — cargo-cyclonedx itera todos workspace members     |
| Inclui tools/?                             | sim, exceto crates marcados `publish = false`           |

## Procedimento

```bash
cargo install cargo-cyclonedx --locked   # local; CI usa SHA-pinned install-action
bash scripts/sbom-aggregate.sh
# → target/sbom/corelink-workspace.cdx.json
```

Estrutura emitida (top-level):

```json
{
  "bomFormat": "CycloneDX",
  "specVersion": "1.6",
  "serialNumber": "urn:uuid:<random>",
  "version": 1,
  "metadata": {
    "timestamp": "<iso8601>",
    "tools": [{ "vendor": "HumanGuardrail", "name": "corelink-sbom-aggregate", "version": "<git-describe>" }],
    "component": {
      "bom-ref": "pkg:cargo/corelink-workspace@<git-describe>",
      "type": "application",
      "name": "corelink-workspace",
      "version": "<git-describe>",
      "purl": "pkg:cargo/corelink-workspace@<git-describe>"
    }
  },
  "components": [ /* dedup by purl across all per-crate boms */ ],
  "dependencies": [ /* dedup by ref */ ]
}
```

## Dedup semantics

`scripts/sbom-aggregate.sh` chama `cargo cyclonedx --workspace --format json`
(que emite `bom.json` ao lado de cada `Cargo.toml`), depois usa `jq` para:

1. concatenar `.components[]` de todos os boms;
2. deduplicar por `purl` (fallback `name@version` se purl ausente — defensive);
3. concatenar `.dependencies[]` e deduplicar por `ref`.

Isso satisfaz INV-SUPPLY-PURL-UNIQUE: cada componente aparece exatamente uma
vez no consolidated bom, mesmo que esteja referenciado por múltiplos crates
do workspace.

## Cobertura — quem está dentro

- Todos crates em `crates/` (workspace members).
- Todos apps em `apps/` que herdam `workspace = true`.
- `tools/sbom-publish` e demais tools workspace.

## Cobertura — quem está fora (intencional)

- `examples/` (não shipam para produção).
- `tests/` integration-only fixtures.
- Crates marcados `publish = false` que **não** sejam workspace members
  (caso futuro — atualmente não há).

## Integração com WI-S12-002

| Aspecto              | per-binary (S12-002)         | consolidated (R7-1)             |
| -------------------- | ---------------------------- | -------------------------------- |
| Trigger              | release published            | release tag + workflow_dispatch  |
| Formato              | CycloneDX 1.5+               | CycloneDX 1.6                    |
| Escopo               | um binário por SBOM          | workspace inteiro                |
| NTIA validation      | sim (fail-CLOSED)            | herdado de per-binary upstream   |
| RFC 3161 TSA         | sim                          | aplicável via re-publish         |
| Dependency-Track     | ingestion automática         | upload manual / regulator pull   |
| Retenção             | indefinido (DT)              | 365d GHA artifact + release      |

Os dois caminhos coexistem: per-binary é o "machine-to-machine" para o
pipeline de detecção de vulnerabilidades contínuo; consolidated é o
"human-to-human" para customer security review, regulator request e
evidence pack annexes.

## Referências

- `scripts/sbom-aggregate.sh`
- `.github/workflows/sbom-consolidated.yml` — gate CI (SHA-pinned)
- `tools/sbom-publish/` — WI-S12-002 per-binary publisher
- INV-SUPPLY-SBOM-PRESENT, INV-SUPPLY-PURL-UNIQUE
- CycloneDX 1.6 spec: https://cyclonedx.org/specification/overview/
