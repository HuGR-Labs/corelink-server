# Preparação: corelink-eviction

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/corelink-eviction/Cargo.toml`. **Baseline:** `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

<a id="fatos"></a>
## Fatos

- Package `corelink-eviction` em `crates/corelink-eviction/Cargo.toml`; 3 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 15 arquivos Rust rastreados, 5794 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-eviction/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `crates/corelink-eviction/src/lib.rs:171` — `pub mod audit;`; `crates/corelink-eviction/src/lib.rs:172` — `pub mod blob_meta;`; `crates/corelink-eviction/src/lib.rs:173` — `pub mod error;`; `crates/corelink-eviction/src/lib.rs:174` — `pub mod metrics;`

<a id="targets"></a>
## Targets

3 targets enumerados no [censo completo](../census.json), registro cujo `manifest` é `crates/corelink-eviction/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/corelink-eviction/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-eviction/src/lib.rs)

<a id="relacoes"></a>
## Relações

3 declarações de dependência e 4 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: `corelink-analytics`, `corelink-billing`, `corelink-cas`, `corelink-ratelimit`.

<a id="okf"></a>
## OKF

- Direto: `docs/knowledge/crates/operations.md` — Operations crate cluster (GC, replication, ratelimit, SRE)
- Direto: `docs/knowledge/ops/gc-eviction.md` — GC / eviction operations
- Direto: `docs/knowledge/tenancy/request-quota.md` — Request-quota enforcement
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-eviction/src/lib.rs --full`; ausência de match deve permanecer explícita.

<a id="riscos"></a>
## Riscos

- Fronteira a conferir: `crates/corelink-eviction/src/audit.rs:222` contém `impl InMemoryEvictionAuditSink {`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-eviction/src/audit.rs:263` contém `impl EvictionAuditSink for InMemoryEvictionAuditSink {`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-eviction/src/audit.rs:296` contém `#[cfg(test)]`; provar seleção e efeito, não inferir runtime do nome.
- Coordenar contratos com consumidores declarados corelink-analytics, corelink-billing, corelink-cas, corelink-ratelimit. Aresta Cargo não certifica chamada ou produção.

<a id="comandos"></a>
## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/corelink-eviction/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/corelink-eviction/Cargo.toml -p corelink-eviction --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

<a id="fontes"></a>
## Fontes

- [crates/corelink-eviction/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-eviction/Cargo.toml); blob `7625910ebf62c4b712b39904b410adb3cce738d5`.
- [crates/corelink-eviction/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-eviction/src/lib.rs); blob `47ee8e633f4ec00a9a5221390381b79d7e33e042`.
- [docs/knowledge/crates/operations.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/crates/operations.md); blob `f85d562c6e20b4552ed452a569c38e1f66467440`.
- [docs/knowledge/ops/gc-eviction.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/ops/gc-eviction.md); blob `811ccc121c895431c5f63d321f7023c3d815d994`.
- [docs/knowledge/tenancy/request-quota.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/tenancy/request-quota.md); blob `7a00daa36718a1212f7a3cd26e9e23a53d829ee2`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
