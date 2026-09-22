# Preparação: corelink-enterprise-inquiry

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/corelink-enterprise-inquiry/Cargo.toml`. **Baseline:** `37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

<a id="fatos"></a>
## Fatos

- Package `corelink-enterprise-inquiry` em `crates/corelink-enterprise-inquiry/Cargo.toml`; 2 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 15 arquivos Rust rastreados, 5543 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-enterprise-inquiry/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `crates/corelink-enterprise-inquiry/src/lib.rs:82` — `pub mod audit;`; `crates/corelink-enterprise-inquiry/src/lib.rs:83` — `pub mod crm;`; `crates/corelink-enterprise-inquiry/src/lib.rs:84` — `pub mod encryption;`; `crates/corelink-enterprise-inquiry/src/lib.rs:85` — `pub mod error;`

<a id="targets"></a>
## Targets

2 targets enumerados no [censo completo](../census.json), registro cujo `manifest` é `crates/corelink-enterprise-inquiry/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/corelink-enterprise-inquiry/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-enterprise-inquiry/src/lib.rs)

<a id="relacoes"></a>
## Relações

6 declarações de dependência e 2 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: `corelink-ops`, `corelink-slack-real`.

<a id="okf"></a>
## OKF

- Nenhum conceito OKF declara diretamente fonte própria deste package no levantamento de source_files; não equivale a ausência de documentação semântica.
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-enterprise-inquiry/src/lib.rs --full`; ausência de match deve permanecer explícita.

<a id="riscos"></a>
## Riscos

- Fronteira a conferir: `crates/corelink-enterprise-inquiry/src/audit.rs:150` contém `impl InMemoryInquiryAuditSink {`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-enterprise-inquiry/src/audit.rs:182` contém `impl InquiryAuditSink for InMemoryInquiryAuditSink {`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-enterprise-inquiry/src/audit.rs:206` contém `#[cfg(test)]`; provar seleção e efeito, não inferir runtime do nome.
- Coordenar contratos com consumidores declarados corelink-ops, corelink-slack-real. Aresta Cargo não certifica chamada ou produção.

<a id="comandos"></a>
## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/corelink-enterprise-inquiry/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/corelink-enterprise-inquiry/Cargo.toml -p corelink-enterprise-inquiry --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

<a id="fontes"></a>
## Fontes

- [crates/corelink-enterprise-inquiry/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-enterprise-inquiry/Cargo.toml); blob `4d4b70698ffa33ea188c8e0c618362ccfb4cddf8`.
- [crates/corelink-enterprise-inquiry/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-enterprise-inquiry/src/lib.rs); blob `241b0721c64d2420febd1eb6947ea7f75b064649`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
