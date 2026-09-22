# Preparação: corelink-failover-router

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/corelink-failover-router/Cargo.toml`. **Baseline:** `37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

<a id="fatos"></a>
## Fatos

- Package `corelink-failover-router` em `crates/corelink-failover-router/Cargo.toml`; 3 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 9 arquivos Rust rastreados, 1857 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-failover-router/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `crates/corelink-failover-router/src/lib.rs:62` — `pub mod audit;`; `crates/corelink-failover-router/src/lib.rs:63` — `pub mod error;`; `crates/corelink-failover-router/src/lib.rs:64` — `pub mod failback;`; `crates/corelink-failover-router/src/lib.rs:65` — `pub mod health;`

<a id="targets"></a>
## Targets

3 targets enumerados no [censo completo](../census.json), registro cujo `manifest` é `crates/corelink-failover-router/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/corelink-failover-router/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-failover-router/src/lib.rs)

<a id="relacoes"></a>
## Relações

4 declarações de dependência e 5 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: `corelink-ops`, `corelink-replication`, `corelink-replication-coordinator`, `corelink-server`, `e2e-failover-router`.

<a id="okf"></a>
## OKF

- Direto: `docs/knowledge/planes/replication-failover.md` — Multi-region replication + failover plane
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-failover-router/src/lib.rs --full`; ausência de match deve permanecer explícita.

<a id="riscos"></a>
## Riscos

- Fronteira a conferir: `crates/corelink-failover-router/src/audit.rs:9` contém `pub use corelink_replica_worker::{`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-failover-router/src/failback.rs:116` contém `impl InMemoryAuditOutbox {`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-failover-router/src/failback.rs:138` contém `impl AuditOutboxRepository for InMemoryAuditOutbox {`; provar seleção e efeito, não inferir runtime do nome.
- Coordenar contratos com consumidores declarados corelink-ops, corelink-replication, corelink-replication-coordinator, corelink-server, e2e-failover-router. Aresta Cargo não certifica chamada ou produção.

<a id="comandos"></a>
## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/corelink-failover-router/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/corelink-failover-router/Cargo.toml -p corelink-failover-router --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

<a id="fontes"></a>
## Fontes

- [crates/corelink-failover-router/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-failover-router/Cargo.toml); blob `48cfde0dc23261594577fef5e3b0dc3a2eb93b74`.
- [crates/corelink-failover-router/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-failover-router/src/lib.rs); blob `c3e6a6dda601d18d33b18ad6e1fd38beeb46747a`.
- [docs/knowledge/planes/replication-failover.md](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/docs/knowledge/planes/replication-failover.md); blob `b71c40a5df34dac3205a71eff9af17ce2971368e`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
