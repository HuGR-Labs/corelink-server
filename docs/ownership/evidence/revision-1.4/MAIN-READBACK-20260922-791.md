# Readback de `main` — 2026-09-22 (`791f474d`)

`git ls-remote origin refs/heads/main` retornou:

`791f474df449227e4bc13b024508ce5f12a44924`

Desde `a0eb612c`, o delta reproduzível é de 21 arquivos, 276 linhas
adicionadas e 104 removidas. Ele toca workflows/CI, backlog, documentação de
handoff, OpenAPI e scripts/testes de contratos. Não houve alteração nos cinco
pilotos, nos manifests Cargo ou no lockfile observados nesta comparação.

As superfícies CI/OpenAPI permanecem relações `SOURCE/UNKNOWN` nos documentos
afetados. Este readback não é evidência de execução, reachability, aprovação,
freeze ou publicação.
