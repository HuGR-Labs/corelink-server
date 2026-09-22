# Preparação: sbom-publish

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `tools/sbom-publish/Cargo.toml`. **Baseline:** `37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

<a id="fatos"></a>
## Fatos

- Package `sbom-publish` em `tools/sbom-publish/Cargo.toml`; 7 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 14 arquivos Rust rastreados, 2645 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `tools/sbom-publish/src/lib.rs`, `tools/sbom-publish/src/main.rs`.
- Declarações de navegação (amostra, não API completa): `tools/sbom-publish/src/lib.rs:30` — `pub mod dt;`; `tools/sbom-publish/src/lib.rs:31` — `pub mod error;`; `tools/sbom-publish/src/lib.rs:32` — `pub mod metrics;`; `tools/sbom-publish/src/lib.rs:33` — `pub mod ntia;`

<a id="targets"></a>
## Targets

7 targets enumerados no [censo completo](../census.json), registro cujo `manifest` é `tools/sbom-publish/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`tools/sbom-publish/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/tools/sbom-publish/src/lib.rs)
- [`tools/sbom-publish/src/main.rs`](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/tools/sbom-publish/src/main.rs)

<a id="relacoes"></a>
## Relações

16 declarações de dependência e 0 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: nenhum na população Cargo examinada.

<a id="okf"></a>
## OKF

- Nenhum conceito OKF declara diretamente fonte própria deste package no levantamento de source_files; não equivale a ausência de documentação semântica.
- Roteador existente: `python3 scripts/okf_context.py --file tools/sbom-publish/src/lib.rs --full`; ausência de match deve permanecer explícita.

<a id="riscos"></a>
## Riscos

- Fronteira a conferir: `tools/sbom-publish/examples/generate.rs:19` contém `#![forbid(unsafe_code)]`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `tools/sbom-publish/examples/ingest_dt_retry.rs:17` contém `#![forbid(unsafe_code)]`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `tools/sbom-publish/examples/validate_ntia.rs:13` contém `#![forbid(unsafe_code)]`; provar seleção e efeito, não inferir runtime do nome.
- Nenhum consumidor Cargo entre os packages elegíveis nesta seleção declarada. Verificar CLI, FFI, workflows, dados e clientes externos antes de alegar isolamento.

<a id="comandos"></a>
## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file tools/sbom-publish/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path tools/sbom-publish/Cargo.toml -p sbom-publish --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

<a id="fontes"></a>
## Fontes

- [tools/sbom-publish/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/tools/sbom-publish/Cargo.toml); blob `d314f370f0d40e2aed897a340d72ae17ee2db611`.
- [tools/sbom-publish/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/tools/sbom-publish/src/lib.rs); blob `39b8df63b2560125204d129faec1afff82b76803`.
- [tools/sbom-publish/src/main.rs](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/tools/sbom-publish/src/main.rs); blob `8f6386b5753e8596bcaaaf2169e012b6e19627df`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
