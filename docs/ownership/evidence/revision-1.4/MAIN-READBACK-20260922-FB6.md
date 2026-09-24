# Readback de `main` — 2026-09-22 (`fb611330`)

Este registro é somente SOURCE/reanchor. Não é aprovação de package, prova de
runtime ou autorização para publicar issues.

## Observação

- Remoto: `origin/main`
- Pin observado: `fb61133083a163e422f2ec21d28ffac6a0a8c623`
- Pin anterior comparado: `743317c4ae66408f82f113d6bbf21fdffcf005be`
- Delta: 220 arquivos, `+71.906 / -1.185` linhas.

## Impacto nos cinco pilotos

O delta contém quatro fontes do package `corelink-server`:

- `crates/corelink-container/src/routes/dsr/access.rs`
- `crates/corelink-container/src/routes/dsr/adapter_d1.rs`
- `crates/corelink-container/src/routes/dsr/adapter_d1_registry.rs`
- `crates/corelink-container/src/routes/dsr/adapter_d1_tests.rs`

Nenhum caminho de `corelink-hash`, `corelink-billing`, `corelink-cf-bindings`
ou `tests/e2e-billing-flow` apareceu nesse delta. O material alterado do
`corelink-server` é suficiente para invalidar a ponta SOURCE daquele piloto:
relações DSR, contratos de rota, testes e qualquer claim derivado desses
arquivos exigem novo readback antes de review ou freeze.

O restante do delta é predominantemente workflow, runbook, script, evidência,
documentação e Terraform de outras frentes. Esses arquivos não são promovidos
automaticamente a relações de ownership dos cinco pilotos; permanecem fora do
escopo até descoberta específica.

No reanchor cumulativo desde a baseline, foram confirmados dois drifts
adicionais fora do delta `743317c4` → `fb611330`: `corelink-hash/Cargo.toml`
alterou somente a URL `repository`, e o teste de quota CAS do billing alterou
seu predicado de p99 de `<5 ms` para `≤5 ms`. Ambos foram registrados nos
artefatos correspondentes como `SOURCE`; nenhum teste foi executado.

## Decisão

Preservar os artefatos atuais e registrar `corelink-server` como **STALE_SOURCE**.
Não rebasear ou editar os quatro pilotos restantes por inferência. A próxima
reconciliação deve ler os quatro arquivos DSR no pin `fb611330`, atualizar as
relações afetadas e invalidar qualquer review que dependa dos bytes antigos.
