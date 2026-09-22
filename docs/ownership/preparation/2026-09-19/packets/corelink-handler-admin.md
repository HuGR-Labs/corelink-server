# Preparação: corelink-handler-admin

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/corelink-handler-admin/Cargo.toml`. **Baseline:** `37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

<a id="fatos"></a>
## Fatos

- Package `corelink-handler-admin` em `crates/corelink-handler-admin/Cargo.toml`; 2 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 7 arquivos Rust rastreados, 1598 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-handler-admin/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `crates/corelink-handler-admin/src/lib.rs:25` — `pub mod audit;`; `crates/corelink-handler-admin/src/lib.rs:26` — `pub mod error;`; `crates/corelink-handler-admin/src/lib.rs:27` — `pub mod handler;`; `crates/corelink-handler-admin/src/lib.rs:28` — `pub mod ledger;`

<a id="targets"></a>
## Targets

2 targets enumerados no [censo completo](../census.json), registro cujo `manifest` é `crates/corelink-handler-admin/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/corelink-handler-admin/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-handler-admin/src/lib.rs)

<a id="relacoes"></a>
## Relações

3 declarações de dependência e 2 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: `corelink-ops`, `corelink-server`.

<a id="okf"></a>
## OKF

- Direto: `docs/knowledge/crates/handler-trait-seam.md` — Handler-trait seam (CAS/AC/customer/erase/admin)
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-handler-admin/src/lib.rs --full`; ausência de match deve permanecer explícita.

<a id="riscos"></a>
## Riscos

- Fronteira a conferir: `crates/corelink-handler-admin/src/audit.rs:77` contém `impl InMemoryAuditSink {`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-handler-admin/src/audit.rs:111` contém `impl AuditSink for InMemoryAuditSink {`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-handler-admin/src/handler.rs:245` contém `impl core::fmt::Debug for InMemoryAdminHandler {`; provar seleção e efeito, não inferir runtime do nome.
- Coordenar contratos com consumidores declarados corelink-ops, corelink-server. Aresta Cargo não certifica chamada ou produção.

<a id="comandos"></a>
## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/corelink-handler-admin/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/corelink-handler-admin/Cargo.toml -p corelink-handler-admin --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

<a id="fontes"></a>
## Fontes

- [crates/corelink-handler-admin/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-handler-admin/Cargo.toml); blob `8e19466d7eb1a94872b313cbfd89269507e902f2`.
- [crates/corelink-handler-admin/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-handler-admin/src/lib.rs); blob `07704dcf01e28217bbdaf7422a2a6c1db956238e`.
- [docs/knowledge/crates/handler-trait-seam.md](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/docs/knowledge/crates/handler-trait-seam.md); blob `fd51c52a5b57a62c0e9594832e1df4ba052e91a8`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
