# Preparação: corelink-openapi

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `tools/openapi/Cargo.toml`. **Baseline:** `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

## Fatos

- Package `corelink-openapi` em `tools/openapi/Cargo.toml`; 1 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 1 arquivos Rust rastreados, 289 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `tools/openapi/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `tools/openapi/src/lib.rs:68` — `pub mod paths {`; `tools/openapi/src/lib.rs:231` — `pub fn parse_json() -> Result<serde_json::Value, serde_json::Error> {`

## Targets

1 targets enumerados em `../census.json`, registro cujo `manifest` é `tools/openapi/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`tools/openapi/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/tools/openapi/src/lib.rs)

## Relações

3 declarações de dependência e 0 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: nenhum na população Cargo examinada.

## OKF

- Nenhum conceito OKF declara diretamente fonte própria deste package no levantamento de source_files; não equivale a ausência de documentação semântica.
- Roteador existente: `python3 scripts/okf_context.py --file tools/openapi/src/lib.rs --full`; ausência de match deve permanecer explícita.

## Riscos

- Fronteira a conferir: `tools/openapi/src/lib.rs:42` contém `#![forbid(unsafe_code)]`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `tools/openapi/src/lib.rs:63` contém `pub const PACKAGE_VERSION: &str = env!("CARGO_PKG_VERSION");`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `tools/openapi/src/lib.rs:235` contém `#[cfg(test)]`; provar seleção e efeito, não inferir runtime do nome.
- Nenhum consumidor Cargo entre os packages elegíveis nesta seleção declarada. Verificar CLI, FFI, workflows, dados e clientes externos antes de alegar isolamento.

## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file tools/openapi/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path tools/openapi/Cargo.toml -p corelink-openapi --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

## Fontes

- [tools/openapi/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/tools/openapi/Cargo.toml); blob `acac2c04012fd987435ecb8f2c11cd4025d67e41`.
- [tools/openapi/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/tools/openapi/src/lib.rs); blob `69a407d04d75d1b924172cf696b04dffb00bf1fe`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
