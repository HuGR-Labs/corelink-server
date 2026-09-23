# Censo Cargo no `main` — 2026-09-22 (`fb611330`)

Este censo usa objetos Git do pin indicado; não depende do checkout local, não
compila e não prova runtime/deploy.

## Resultado reproduzível

- Comando-base: `git ls-tree -r --name-only fb611330 -- '*Cargo.toml'`.
- `Cargo.toml` rastreados: **107**.
- Packages com `[package].name`: **106**.
- Manifest virtual sem `[package]`: **1**, a raiz `Cargo.toml`.
- Packages no registry da campanha: **105**.
- Para os 105 registros, package name e manifest path coincidem exatamente
  com `[package].name` e o caminho do objeto Git.
- Diferença explicada: **1** package em `_archive/wi-s11-002-partial/Cargo.toml`,
  package `corelink-erasure`, classificado como archive e excluído da população
  elegível; não recebe issue de ownership.
- Packages do registry ausentes no pin: **0**.
- Erros de parse TOML: **0**.

## Decisão de identidade

O conjunto elegível do registry reconcilia exatamente com os 105 packages
preparados: 95 packages de workspace e 10 fuzz independentes. O package
`corelink-erasure` não é tratado como vendor, fixture ou package elegível; sua
exclusão é somente pela localização histórica `_archive/` e deve permanecer
explícita em qualquer censo futuro.

Este resultado fecha a contagem de identidade **neste pin**. Qualquer novo
`Cargo.toml`, alteração de `[package].name`, remoção do archive ou mudança de
workspace invalida o resultado e exige novo censo antes de publicação.
