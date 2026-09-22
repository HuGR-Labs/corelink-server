# Preparação: corelink-signup

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/corelink-signup/Cargo.toml`. **Baseline:** `37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

<a id="fatos"></a>
## Fatos

- Package `corelink-signup` em `crates/corelink-signup/Cargo.toml`; 5 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 17 arquivos Rust rastreados, 3844 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-signup/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `crates/corelink-signup/src/lib.rs:150` — `pub mod audit;`; `crates/corelink-signup/src/lib.rs:151` — `pub mod billing;`; `crates/corelink-signup/src/lib.rs:152` — `pub mod correlation;`; `crates/corelink-signup/src/lib.rs:153` — `pub mod error;`

<a id="targets"></a>
## Targets

5 targets enumerados no [censo completo](../census.json), registro cujo `manifest` é `crates/corelink-signup/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/corelink-signup/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-signup/src/lib.rs)

<a id="relacoes"></a>
## Relações

3 declarações de dependência e 2 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: `e2e-billing-flow`, `e2e-signup-flow`.

<a id="okf"></a>
## OKF

- Nenhum conceito OKF declara diretamente fonte própria deste package no levantamento de source_files; não equivale a ausência de documentação semântica.
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-signup/src/lib.rs --full`; ausência de match deve permanecer explícita.

<a id="riscos"></a>
## Riscos

- Fronteira a conferir: `crates/corelink-signup/src/audit.rs:165` contém `impl InMemorySignupAuditSink {`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-signup/src/audit.rs:197` contém `impl SignupAuditSink for InMemorySignupAuditSink {`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-signup/src/audit.rs:221` contém `#[cfg(test)]`; provar seleção e efeito, não inferir runtime do nome.
- Coordenar contratos com consumidores declarados e2e-billing-flow, e2e-signup-flow. Aresta Cargo não certifica chamada ou produção.

<a id="comandos"></a>
## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/corelink-signup/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/corelink-signup/Cargo.toml -p corelink-signup --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

<a id="fontes"></a>
## Fontes

- [crates/corelink-signup/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-signup/Cargo.toml); blob `cc2da2df3e08ccb109fb95ef06c9d256e1762312`.
- [crates/corelink-signup/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-signup/src/lib.rs); blob `44192a950718830490f183adbf77f8e397041d81`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
