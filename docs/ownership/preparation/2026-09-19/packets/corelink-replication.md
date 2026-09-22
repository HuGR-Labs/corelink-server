# Preparação: corelink-replication

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/corelink-replication/Cargo.toml`. **Baseline:** `37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

<a id="fatos"></a>
## Fatos

- Package `corelink-replication` em `crates/corelink-replication/Cargo.toml`; 6 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 19 arquivos Rust rastreados, 3303 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-replication/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `crates/corelink-replication/src/lib.rs:86` — `pub mod coordinator;`; `crates/corelink-replication/src/lib.rs:87` — `pub mod failover;`; `crates/corelink-replication/src/lib.rs:88` — `pub mod region;`; `crates/corelink-replication/src/lib.rs:89` — `pub mod replica;`

<a id="targets"></a>
## Targets

6 targets enumerados no [censo completo](../census.json), registro cujo `manifest` é `crates/corelink-replication/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/corelink-replication/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-replication/src/lib.rs)

<a id="relacoes"></a>
## Relações

10 declarações de dependência e 2 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: `corelink-adapter-host`, `corelink-reapi`.

<a id="okf"></a>
## OKF

- Direto: `docs/knowledge/planes/replication-failover.md` — Multi-region replication + failover plane
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-replication/src/lib.rs --full`; ausência de match deve permanecer explícita.

<a id="riscos"></a>
## Riscos

- Fronteira a conferir: `crates/corelink-replication/src/coordinator.rs:9` contém `pub use corelink_replication_coordinator::*;`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-replication/src/failover.rs:7` contém `pub use corelink_failover_router::*;`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-replication/src/lib.rs:83` contém `#![forbid(unsafe_code)]`; provar seleção e efeito, não inferir runtime do nome.
- Coordenar contratos com consumidores declarados corelink-adapter-host, corelink-reapi. Aresta Cargo não certifica chamada ou produção.

<a id="comandos"></a>
## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/corelink-replication/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/corelink-replication/Cargo.toml -p corelink-replication --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

<a id="fontes"></a>
## Fontes

- [crates/corelink-replication/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-replication/Cargo.toml); blob `3ed96ed1ee4b862e20c7ca53e15f16883ee02234`.
- [crates/corelink-replication/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-replication/src/lib.rs); blob `6e1f3491cf6d52fc6b6d51324ce5c4c45601cc4d`.
- [docs/knowledge/planes/replication-failover.md](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/docs/knowledge/planes/replication-failover.md); blob `b71c40a5df34dac3205a71eff9af17ce2971368e`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
