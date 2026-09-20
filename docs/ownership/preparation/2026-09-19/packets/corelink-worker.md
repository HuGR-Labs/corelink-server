# Preparação: corelink-worker

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/corelink-worker/Cargo.toml`. **Baseline:** `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

<a id="fatos"></a>
## Fatos

- Package `corelink-worker` em `crates/corelink-worker/Cargo.toml`; 21 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 97 arquivos Rust rastreados, 30052 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-worker/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `crates/corelink-worker/src/lib.rs:89` — `pub mod auth;`; `crates/corelink-worker/src/lib.rs:90` — `pub mod cache;`; `crates/corelink-worker/src/lib.rs:92` — `pub mod middleware;`; `crates/corelink-worker/src/lib.rs:94` — `pub mod reapi;`

<a id="targets"></a>
## Targets

21 targets enumerados no [censo completo](../census.json), registro cujo `manifest` é `crates/corelink-worker/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/corelink-worker/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-worker/src/lib.rs)

<a id="relacoes"></a>
## Relações

50 declarações de dependência e 8 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: `corelink-adapter-host`, `corelink-auth`, `corelink-cas`, `corelink-cf-bindings`, `corelink-reapi`, `corelink-replication`, `corelink-worker-fuzz`.

<a id="okf"></a>
## OKF

- Direto: `docs/knowledge/tenancy/isolation.md` — Tenant isolation via idFromName(tenant_id)
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-worker/src/lib.rs --full`; ausência de match deve permanecer explícita.

<a id="riscos"></a>
## Riscos

- Investigar ativação e compatibilidade das features declaradas: default, tower-middleware. Não assumir que --all-features é válido.
- Fronteira a conferir: `crates/corelink-worker/src/auth/revocation.rs:111` contém `#[cfg(test)]`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-worker/src/auth/revocation.rs:119` contém `pub use in_memory_broadcast::InMemoryBroadcast;`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-worker/src/auth/revocation.rs:120` contém `pub use in_memory_meta::{InMemoryMetaRevocationSink, MonotonicTestClock, TestAuditRow, TestClock};`; provar seleção e efeito, não inferir runtime do nome.
- Coordenar contratos com consumidores declarados corelink-adapter-host, corelink-auth, corelink-cas, corelink-cf-bindings, corelink-reapi, corelink-replication. Aresta Cargo não certifica chamada ou produção.

<a id="comandos"></a>
## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/corelink-worker/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/corelink-worker/Cargo.toml -p corelink-worker --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

<a id="fontes"></a>
## Fontes

- [crates/corelink-worker/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-worker/Cargo.toml); blob `18e9450da475cd29e2a168ee670398542189b95c`.
- [crates/corelink-worker/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-worker/src/lib.rs); blob `a9e915b9f19cc5744371c25eab2e60501a189ccf`.
- [docs/knowledge/tenancy/isolation.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/tenancy/isolation.md); blob `1f0976efcf940ea2ecbb9f5b3c8dab8ec7c634ae`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
