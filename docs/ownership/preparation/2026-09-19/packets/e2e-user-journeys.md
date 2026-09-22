# Preparação: e2e-user-journeys

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `tests/e2e-user-journeys/Cargo.toml`. **Baseline:** `37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

<a id="fatos"></a>
## Fatos

- Package `e2e-user-journeys` em `tests/e2e-user-journeys/Cargo.toml`; 1 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 35 arquivos Rust rastreados, 15194 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `tests/e2e-user-journeys/src/main.rs`.

<a id="targets"></a>
## Targets

1 targets enumerados no [censo completo](../census.json), registro cujo `manifest` é `tests/e2e-user-journeys/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`tests/e2e-user-journeys/src/main.rs`](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/tests/e2e-user-journeys/src/main.rs)

<a id="relacoes"></a>
## Relações

6 declarações de dependência e 0 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: nenhum na população Cargo examinada.

<a id="okf"></a>
## OKF

- Nenhum conceito OKF declara diretamente fonte própria deste package no levantamento de source_files; não equivale a ausência de documentação semântica.
- Roteador existente: `python3 scripts/okf_context.py --file tests/e2e-user-journeys/src/main.rs --full`; ausência de match deve permanecer explícita.

<a id="riscos"></a>
## Riscos

- Fronteira a conferir: `tests/e2e-user-journeys/src/harness.rs:713` contém `#[cfg(test)]`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `tests/e2e-user-journeys/src/journeys/shared_cache.rs:161` contém `let brew_path = match std::env::var(PUBLIC_BREW_PATH_ENV)`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `tests/e2e-user-journeys/src/main.rs:126` contém `let min_pass: usize = std::env::var("CORELINK_E2E_MIN_PASS")`; provar seleção e efeito, não inferir runtime do nome.
- Nenhum consumidor Cargo entre os packages elegíveis nesta seleção declarada. Verificar CLI, FFI, workflows, dados e clientes externos antes de alegar isolamento.

<a id="comandos"></a>
## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file tests/e2e-user-journeys/src/main.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path tests/e2e-user-journeys/Cargo.toml -p e2e-user-journeys --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

<a id="fontes"></a>
## Fontes

- [tests/e2e-user-journeys/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/tests/e2e-user-journeys/Cargo.toml); blob `3cae29b6a03d8baa2e0f56c3bbd9e4fec7cc0218`.
- [tests/e2e-user-journeys/src/main.rs](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/tests/e2e-user-journeys/src/main.rs); blob `70f466a699b5964347b0a90c6ddadce8fe2abff2`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
