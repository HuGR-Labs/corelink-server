# Preparação: corelink-handler-customer

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/corelink-handler-customer/Cargo.toml`. **Baseline:** `37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

<a id="fatos"></a>
## Fatos

- Package `corelink-handler-customer` em `crates/corelink-handler-customer/Cargo.toml`; 3 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 14 arquivos Rust rastreados, 3568 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-handler-customer/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `crates/corelink-handler-customer/src/lib.rs:50` — `pub mod audit;`; `crates/corelink-handler-customer/src/lib.rs:51` — `pub mod error;`; `crates/corelink-handler-customer/src/lib.rs:52` — `pub mod handler;`; `crates/corelink-handler-customer/src/lib.rs:53` — `pub mod observer;`

<a id="targets"></a>
## Targets

3 targets enumerados no [censo completo](../census.json), registro cujo `manifest` é `crates/corelink-handler-customer/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/corelink-handler-customer/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-handler-customer/src/lib.rs)

<a id="relacoes"></a>
## Relações

4 declarações de dependência e 1 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: `corelink-server`.

<a id="okf"></a>
## OKF

- Direto: `docs/knowledge/crates/handler-trait-seam.md` — Handler-trait seam (CAS/AC/customer/erase/admin)
- Direto: `docs/knowledge/flows/signup-auto-provision.md` — Signup auto-provision + account-deletion erasure (Clerk webhook)
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-handler-customer/src/lib.rs --full`; ausência de match deve permanecer explícita.

<a id="riscos"></a>
## Riscos

- Fronteira a conferir: `crates/corelink-handler-customer/src/audit.rs:199` contém `impl InMemoryAuditSink {`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-handler-customer/src/audit.rs:252` contém `impl AuditSink for InMemoryAuditSink {`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-handler-customer/src/handler.rs:223` contém `impl core::fmt::Debug for InMemoryCustomerHandler {`; provar seleção e efeito, não inferir runtime do nome.
- Coordenar contratos com consumidores declarados corelink-server. Aresta Cargo não certifica chamada ou produção.

<a id="comandos"></a>
## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/corelink-handler-customer/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/corelink-handler-customer/Cargo.toml -p corelink-handler-customer --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

<a id="fontes"></a>
## Fontes

- [crates/corelink-handler-customer/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-handler-customer/Cargo.toml); blob `4aafb7ca54f8bc586a7a846c28a1ba35d4cf3097`.
- [crates/corelink-handler-customer/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-handler-customer/src/lib.rs); blob `e0061b84d5ace030090d6ef6eeeb516fd84a382b`.
- [docs/knowledge/crates/handler-trait-seam.md](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/docs/knowledge/crates/handler-trait-seam.md); blob `fd51c52a5b57a62c0e9594832e1df4ba052e91a8`.
- [docs/knowledge/flows/signup-auto-provision.md](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/docs/knowledge/flows/signup-auto-provision.md); blob `168ae657435f7aa5b73db81470eda9755ed3caef`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
