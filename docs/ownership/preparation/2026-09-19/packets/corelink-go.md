# Preparação: corelink-go

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `tools/sdks/go/Cargo.toml`. **Baseline:** `37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

<a id="fatos"></a>
## Fatos

- Package `corelink-go` em `tools/sdks/go/Cargo.toml`; 1 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 2 arquivos Rust rastreados, 418 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `tools/sdks/go/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `tools/sdks/go/src/lib.rs:21` — `pub use corelink_client_verify::ffi::*;`; `tools/sdks/go/src/lib.rs:23` — `pub mod go_bridge;`; `tools/sdks/go/src/lib.rs:25` — `pub use go_bridge::{`

<a id="targets"></a>
## Targets

1 targets enumerados no [censo completo](../census.json), registro cujo `manifest` é `tools/sdks/go/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`tools/sdks/go/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/tools/sdks/go/src/lib.rs)

<a id="relacoes"></a>
## Relações

2 declarações de dependência e 0 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: nenhum na população Cargo examinada.

<a id="okf"></a>
## OKF

- Nenhum conceito OKF declara diretamente fonte própria deste package no levantamento de source_files; não equivale a ausência de documentação semântica.
- Roteador existente: `python3 scripts/okf_context.py --file tools/sdks/go/src/lib.rs --full`; ausência de match deve permanecer explícita.

<a id="riscos"></a>
## Riscos

- Fronteira a conferir: `tools/sdks/go/src/go_bridge.rs:50` contém `unsafe impl Send for CorelinkGoClient {}`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `tools/sdks/go/src/go_bridge.rs:51` contém `unsafe impl Sync for CorelinkGoClient {}`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `tools/sdks/go/src/go_bridge.rs:70` contém `pub unsafe extern "C" fn corelink_go_client_new(`; provar seleção e efeito, não inferir runtime do nome.
- Nenhum consumidor Cargo entre os packages elegíveis nesta seleção declarada. Verificar CLI, FFI, workflows, dados e clientes externos antes de alegar isolamento.

<a id="comandos"></a>
## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file tools/sdks/go/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path tools/sdks/go/Cargo.toml -p corelink-go --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

<a id="fontes"></a>
## Fontes

- [tools/sdks/go/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/tools/sdks/go/Cargo.toml); blob `9413ad8e7d71ff12dd02964151ca0c5b0a637de0`.
- [tools/sdks/go/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/tools/sdks/go/src/lib.rs); blob `6e7122ff38d159bb096759b6d50c944f47ec6ec3`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
