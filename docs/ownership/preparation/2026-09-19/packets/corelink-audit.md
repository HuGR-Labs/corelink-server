# Preparação: corelink-audit

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/corelink-audit/Cargo.toml`. **Baseline:** `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

## Fatos

- Package `corelink-audit` em `crates/corelink-audit/Cargo.toml`; 8 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 21 arquivos Rust rastreados, 4174 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-audit/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `crates/corelink-audit/src/lib.rs:115` — `pub mod emitter;`; `crates/corelink-audit/src/lib.rs:116` — `pub mod error;`; `crates/corelink-audit/src/lib.rs:117` — `pub mod events;`; `crates/corelink-audit/src/lib.rs:118` — `pub mod link_hash;`

## Targets

8 targets enumerados em `../census.json`, registro cujo `manifest` é `crates/corelink-audit/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/corelink-audit/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-audit/src/lib.rs)

## Relações

13 declarações de dependência e 9 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: `corelink-ac`, `corelink-adapter-host`, `corelink-auth`, `corelink-cas`, `corelink-ops`, `corelink-server`, `corelink-worker`, `e2e-tenant-isolation`.

## OKF

- Direto: `docs/knowledge/crates/audit-analytics.md` — Audit/analytics crate cluster
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-audit/src/lib.rs --full`; ausência de match deve permanecer explícita.

## Riscos

- Fronteira a conferir: `crates/corelink-audit/src/analytics.rs:10` contém `pub use corelink_analytics::*;`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-audit/src/chain.rs:17` contém `pub use corelink_audit_chain::*;`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-audit/src/emitter.rs:81` contém `impl InMemoryEmitter {`; provar seleção e efeito, não inferir runtime do nome.
- Coordenar contratos com consumidores declarados corelink-ac, corelink-adapter-host, corelink-auth, corelink-cas, corelink-ops, corelink-server. Aresta Cargo não certifica chamada ou produção.

## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/corelink-audit/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/corelink-audit/Cargo.toml -p corelink-audit --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

## Fontes

- [crates/corelink-audit/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-audit/Cargo.toml); blob `c789259deb2d1987b0a0fb83f3c21e9787ead30f`.
- [crates/corelink-audit/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-audit/src/lib.rs); blob `93aa6c1f953afda5911004918a9b4cff2b686611`.
- [docs/knowledge/crates/audit-analytics.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/crates/audit-analytics.md); blob `864bae2a19336af8486ad5485eaa3764e05db0d9`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
