# Preparação: corelink-gc

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/corelink-gc/Cargo.toml`. **Baseline:** `37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

<a id="fatos"></a>
## Fatos

- Package `corelink-gc` em `crates/corelink-gc/Cargo.toml`; 16 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 48 arquivos Rust rastreados, 17420 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-gc/src/bin/gc_sweep.rs`, `crates/corelink-gc/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `crates/corelink-gc/src/lib.rs:83` — `pub mod admin;`; `crates/corelink-gc/src/lib.rs:84` — `pub mod audit;`; `crates/corelink-gc/src/lib.rs:85` — `pub mod degrade;`; `crates/corelink-gc/src/lib.rs:86` — `pub mod error;`

<a id="targets"></a>
## Targets

16 targets enumerados no [censo completo](../census.json), registro cujo `manifest` é `crates/corelink-gc/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/corelink-gc/src/bin/gc_sweep.rs`](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-gc/src/bin/gc_sweep.rs)
- [`crates/corelink-gc/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-gc/src/lib.rs)

<a id="relacoes"></a>
## Relações

5 declarações de dependência e 1 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: `corelink-server`.

<a id="okf"></a>
## OKF

- Direto: `docs/knowledge/crates/operations.md` — Operations crate cluster (GC, replication, ratelimit, SRE)
- Direto: `docs/knowledge/ops/gc-eviction.md` — GC / eviction operations
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-gc/src/bin/gc_sweep.rs --full`; ausência de match deve permanecer explícita.

<a id="riscos"></a>
## Riscos

- Fronteira a conferir: `crates/corelink-gc/src/admin.rs:229` contém `#[cfg(test)]`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-gc/src/audit.rs:232` contém `impl InMemoryGcAuditSink {`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-gc/src/audit.rs:273` contém `impl GcAuditSink for InMemoryGcAuditSink {`; provar seleção e efeito, não inferir runtime do nome.
- Coordenar contratos com consumidores declarados corelink-server. Aresta Cargo não certifica chamada ou produção.

<a id="comandos"></a>
## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/corelink-gc/src/bin/gc_sweep.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/corelink-gc/Cargo.toml -p corelink-gc --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

<a id="fontes"></a>
## Fontes

- [crates/corelink-gc/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-gc/Cargo.toml); blob `26c58af62284c2fba62559ae1270d88969856303`.
- [crates/corelink-gc/src/bin/gc_sweep.rs](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-gc/src/bin/gc_sweep.rs); blob `b6fd7bf7f9d62ac2ce1dfdb236286f28177e39ab`.
- [crates/corelink-gc/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-gc/src/lib.rs); blob `a90ca4ba9de9b10d9a17bc14d764a21840765ebf`.
- [docs/knowledge/crates/operations.md](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/docs/knowledge/crates/operations.md); blob `f85d562c6e20b4552ed452a569c38e1f66467440`.
- [docs/knowledge/ops/gc-eviction.md](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/docs/knowledge/ops/gc-eviction.md); blob `811ccc121c895431c5f63d321f7023c3d815d994`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
