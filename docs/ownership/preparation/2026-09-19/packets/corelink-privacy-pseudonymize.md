# Preparação: corelink-privacy-pseudonymize

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/corelink-privacy-pseudonymize/Cargo.toml`. **Baseline:** `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

<a id="fatos"></a>
## Fatos

- Package `corelink-privacy-pseudonymize` em `crates/corelink-privacy-pseudonymize/Cargo.toml`; 2 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 2 arquivos Rust rastreados, 521 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-privacy-pseudonymize/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `crates/corelink-privacy-pseudonymize/src/lib.rs:109` — `pub struct PseudonymizationMarker {`; `crates/corelink-privacy-pseudonymize/src/lib.rs:122` — `pub fn from_hash(hash: PseudonymHash) -> Self {`; `crates/corelink-privacy-pseudonymize/src/lib.rs:134` — `pub struct PseudonymHash([u8; 32]);`; `crates/corelink-privacy-pseudonymize/src/lib.rs:151` — `pub fn to_hex(&self) -> String {`

<a id="targets"></a>
## Targets

2 targets enumerados no [censo completo](../census.json), registro cujo `manifest` é `crates/corelink-privacy-pseudonymize/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/corelink-privacy-pseudonymize/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-privacy-pseudonymize/src/lib.rs)

<a id="relacoes"></a>
## Relações

7 declarações de dependência e 2 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: `corelink-privacy`, `corelink-privacy-erasure-worker`.

<a id="okf"></a>
## OKF

- Nenhum conceito OKF declara diretamente fonte própria deste package no levantamento de source_files; não equivale a ausência de documentação semântica.
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-privacy-pseudonymize/src/lib.rs --full`; ausência de match deve permanecer explícita.

<a id="riscos"></a>
## Riscos

- Fronteira a conferir: `crates/corelink-privacy-pseudonymize/src/lib.rs:75` contém `#![forbid(unsafe_code)]`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-privacy-pseudonymize/src/lib.rs:220` contém `#[cfg(test)]`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-privacy-pseudonymize/tests/pseudonymization_invariants.rs:44` contém `std::env::var("PROPTEST_CASES")`; provar seleção e efeito, não inferir runtime do nome.
- Coordenar contratos com consumidores declarados corelink-privacy, corelink-privacy-erasure-worker. Aresta Cargo não certifica chamada ou produção.

<a id="comandos"></a>
## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/corelink-privacy-pseudonymize/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/corelink-privacy-pseudonymize/Cargo.toml -p corelink-privacy-pseudonymize --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

<a id="fontes"></a>
## Fontes

- [crates/corelink-privacy-pseudonymize/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-privacy-pseudonymize/Cargo.toml); blob `c5b8034c7b23b2be4553eeab0e1371f5efa7f895`.
- [crates/corelink-privacy-pseudonymize/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-privacy-pseudonymize/src/lib.rs); blob `ad8c40cd623c4e145a7d5ad17c439273537fe72d`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
