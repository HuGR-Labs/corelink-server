# Preparação: corelink-region

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/corelink-region/Cargo.toml`. **Baseline:** `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

## Fatos

- Package `corelink-region` em `crates/corelink-region/Cargo.toml`; 9 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 20 arquivos Rust rastreados, 4375 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-region/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `crates/corelink-region/src/lib.rs:24` — `pub mod audit;`; `crates/corelink-region/src/lib.rs:25` — `pub mod do_sync_age;`; `crates/corelink-region/src/lib.rs:26` — `pub mod error;`; `crates/corelink-region/src/lib.rs:27` — `pub mod event;`

## Targets

9 targets enumerados em `../census.json`, registro cujo `manifest` é `crates/corelink-region/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/corelink-region/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-region/src/lib.rs)

## Relações

8 declarações de dependência e 2 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: `corelink-replication`, `migrate-single-to-multi-region`.

## OKF

- Direto: `docs/knowledge/storage/r2-ac-regional.md` — R2 AC ×5 regional buckets
- Direto: `docs/knowledge/storage/r2-cas-bucket.md` — R2 CAS bucket topology
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-region/src/lib.rs --full`; ausência de match deve permanecer explícita.

## Riscos

- Fronteira a conferir: `crates/corelink-region/src/audit.rs:25` contém `impl RegionAuditSink for InMemoryRegionAuditSink {`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-region/src/audit.rs:44` contém `#[cfg(test)]`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-region/src/do_sync_age.rs:181` contém `impl InMemoryDoSyncAgeProbe {`; provar seleção e efeito, não inferir runtime do nome.
- Coordenar contratos com consumidores declarados corelink-replication, migrate-single-to-multi-region. Aresta Cargo não certifica chamada ou produção.

## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/corelink-region/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/corelink-region/Cargo.toml -p corelink-region --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

## Fontes

- [crates/corelink-region/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-region/Cargo.toml); blob `ed4a187f1e8c83b6a9bd54273e42260f3e56577e`.
- [crates/corelink-region/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-region/src/lib.rs); blob `2070f5d5cb3ebe0526137295c7246614428d3b29`.
- [docs/knowledge/storage/r2-ac-regional.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/storage/r2-ac-regional.md); blob `266b57300702cff4ffef3f312387dbb017b57396`.
- [docs/knowledge/storage/r2-cas-bucket.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/storage/r2-cas-bucket.md); blob `599e53b9eb17fe29d6ef7b3709edbdf678693e27`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
