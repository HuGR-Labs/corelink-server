# Preparação: e2e-byok-revoke

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `tests/e2e-byok-revoke/Cargo.toml`. **Baseline:** `37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

<a id="fatos"></a>
## Fatos

- Package `e2e-byok-revoke` em `tests/e2e-byok-revoke/Cargo.toml`; 9 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 10 arquivos Rust rastreados, 2129 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `tests/e2e-byok-revoke/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `tests/e2e-byok-revoke/src/lib.rs:60` — `pub mod helpers;`; `tests/e2e-byok-revoke/src/lib.rs:62` — `pub use helpers::{`

<a id="targets"></a>
## Targets

9 targets enumerados no [censo completo](../census.json), registro cujo `manifest` é `tests/e2e-byok-revoke/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`tests/e2e-byok-revoke/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/tests/e2e-byok-revoke/src/lib.rs)

<a id="relacoes"></a>
## Relações

7 declarações de dependência e 0 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: nenhum na população Cargo examinada.

<a id="okf"></a>
## OKF

- Nenhum conceito OKF declara diretamente fonte própria deste package no levantamento de source_files; não equivale a ausência de documentação semântica.
- Contexto herdado de dependência: `docs/knowledge/crates/adapter-hosts.md` — Adapter-host crate cluster (surfaces + KMS)
- Contexto herdado de dependência: `docs/knowledge/storage/byok-envelope-encryption.md` — BYOK envelope encryption at rest (CAS+AC wired, real KMS boundary; owner-runtime gated)
- Contexto herdado de dependência: `docs/knowledge/ops/sre-operations-hub.md` — SRE operations hub (corelink-ops + satellites)
- Roteador existente: `python3 scripts/okf_context.py --file tests/e2e-byok-revoke/src/lib.rs --full`; ausência de match deve permanecer explícita.

<a id="riscos"></a>
## Riscos

- Fronteira a conferir: `tests/e2e-byok-revoke/src/helpers.rs:569` contém `impl InMemoryTenantStatusStore {`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `tests/e2e-byok-revoke/src/helpers.rs:592` contém `impl TenantStatusStore for InMemoryTenantStatusStore {`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `tests/e2e-byok-revoke/src/lib.rs:56` contém `#![forbid(unsafe_code)]`; provar seleção e efeito, não inferir runtime do nome.
- Nenhum consumidor Cargo entre os packages elegíveis nesta seleção declarada. Verificar CLI, FFI, workflows, dados e clientes externos antes de alegar isolamento.

<a id="comandos"></a>
## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file tests/e2e-byok-revoke/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path tests/e2e-byok-revoke/Cargo.toml -p e2e-byok-revoke --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

<a id="fontes"></a>
## Fontes

- [docs/knowledge/crates/adapter-hosts.md](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/docs/knowledge/crates/adapter-hosts.md); blob `2b74eba18fcf4969549a225dc346e9dd8e5768d8`.
- [docs/knowledge/ops/sre-operations-hub.md](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/docs/knowledge/ops/sre-operations-hub.md); blob `3b4bc52a8504cbf4daee27b37d5e94306f2c80bc`.
- [docs/knowledge/storage/byok-envelope-encryption.md](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/docs/knowledge/storage/byok-envelope-encryption.md); blob `167900440af55dc8bfe0f74614557cb80c260864`.
- [tests/e2e-byok-revoke/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/tests/e2e-byok-revoke/Cargo.toml); blob `eb9e290d9a6f71f06fefc6a23156d43db2a52df2`.
- [tests/e2e-byok-revoke/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/tests/e2e-byok-revoke/src/lib.rs); blob `3ffdcf943a870b426d312d8fd306a12dd8e38c59`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
