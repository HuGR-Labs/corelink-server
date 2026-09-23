# Readback de `main` — 2026-09-22 (`7f039b4f`)

## Pin observado

`git ls-remote origin refs/heads/main` retornou:

`7f039b4f87ef6a0347ec5f8ebf7b9ce7e543ea1f`

## Delta desde o pin anterior

Comparação reproduzível:

```text
git diff --shortstat fb61133083a163e422f2ec21d28ffac6a0a8c623..7f039b4f87ef6a0347ec5f8ebf7b9ce7e543ea1f
165 files changed, 2706 insertions(+), 906 deletions(-)
```

O delta é dominado por workflows/CI, documentação pública, backlog e testes
de contratos. A busca por `Cargo.toml`, `Cargo.lock`, os cinco pilotos, os
bindings SDK e `tests/` não encontrou manifests, fontes dos pilotos ou lockfile
alterados; os cinco pilotos não recebem novo drift de fonte neste movimento.

Isso não fecha a campanha: workflows e testes são superfícies de blast radius
para vários packages e precisam permanecer como relações SOURCE/UNKNOWN até
serem reconciliados nos documentos afetados. Este readback não é evidência de
execução, reachability, aprovação, freeze ou publicação.
