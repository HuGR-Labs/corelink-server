# Preparação: corelink-dsr

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/corelink-dsr/Cargo.toml`. **Baseline:** `37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

<a id="fatos"></a>
## Fatos

- Package `corelink-dsr` em `crates/corelink-dsr/Cargo.toml`; 2 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 10 arquivos Rust rastreados, 4615 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-dsr/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `crates/corelink-dsr/src/lib.rs:228` — `pub mod audit;`; `crates/corelink-dsr/src/lib.rs:229` — `pub mod endpoint;`; `crates/corelink-dsr/src/lib.rs:230` — `pub mod error;`; `crates/corelink-dsr/src/lib.rs:231` — `pub mod event;`

<a id="targets"></a>
## Targets

2 targets enumerados no [censo completo](../census.json), registro cujo `manifest` é `crates/corelink-dsr/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/corelink-dsr/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-dsr/src/lib.rs)

<a id="relacoes"></a>
## Relações

8 declarações de dependência e 5 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: `corelink-privacy`, `corelink-privacy-erasure-worker`, `corelink-server`, `e2e-billing-flow`, `e2e-dsr`.

<a id="okf"></a>
## OKF

- Direto: `docs/knowledge/compliance/dsr-erasure.md` — DSR / right-to-erasure pipeline
- Direto: `docs/knowledge/crates/privacy-compliance.md` — Privacy/compliance crate cluster
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-dsr/src/lib.rs --full`; ausência de match deve permanecer explícita.

<a id="riscos"></a>
## Riscos

- Fronteira a conferir: `crates/corelink-dsr/src/audit.rs:215` contém `impl InMemoryDsrAuditSink {`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-dsr/src/audit.rs:265` contém `impl DsrAuditSink for InMemoryDsrAuditSink {`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-dsr/src/audit.rs:297` contém `#[cfg(test)]`; provar seleção e efeito, não inferir runtime do nome.
- Coordenar contratos com consumidores declarados corelink-privacy, corelink-privacy-erasure-worker, corelink-server, e2e-billing-flow, e2e-dsr. Aresta Cargo não certifica chamada ou produção.

<a id="comandos"></a>
## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/corelink-dsr/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/corelink-dsr/Cargo.toml -p corelink-dsr --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

<a id="fontes"></a>
## Fontes

- [crates/corelink-dsr/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-dsr/Cargo.toml); blob `d7250a943771103c399bd8848c4aa98492190988`.
- [crates/corelink-dsr/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-dsr/src/lib.rs); blob `9499a36d58012395ae7464ec961fa96af28ab64a`.
- [docs/knowledge/compliance/dsr-erasure.md](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/docs/knowledge/compliance/dsr-erasure.md); blob `123fa54ff307472e7c438202640276c27fe958cb`.
- [docs/knowledge/crates/privacy-compliance.md](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/docs/knowledge/crates/privacy-compliance.md); blob `041debef4182856b0ad013d6aa1b9f9087d9ea56`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
