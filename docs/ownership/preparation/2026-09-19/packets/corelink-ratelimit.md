# Preparação: corelink-ratelimit

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/corelink-ratelimit/Cargo.toml`. **Baseline:** `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

## Fatos

- Package `corelink-ratelimit` em `crates/corelink-ratelimit/Cargo.toml`; 5 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 16 arquivos Rust rastreados, 5327 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-ratelimit/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `crates/corelink-ratelimit/src/lib.rs:169` — `pub mod audit;`; `crates/corelink-ratelimit/src/lib.rs:170` — `pub mod bucket;`; `crates/corelink-ratelimit/src/lib.rs:171` — `pub mod config;`; `crates/corelink-ratelimit/src/lib.rs:172` — `pub mod error;`

## Targets

5 targets enumerados em `../census.json`, registro cujo `manifest` é `crates/corelink-ratelimit/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/corelink-ratelimit/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-ratelimit/src/lib.rs)

## Relações

4 declarações de dependência e 3 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: `corelink-billing`, `corelink-server`, `e2e-resilience`.

## OKF

- Direto: `docs/knowledge/crates/operations.md` — Operations crate cluster (GC, replication, ratelimit, SRE)
- Direto: `docs/knowledge/tenancy/governance.md` — Tenant governance: rate-limit, customer & user admin
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-ratelimit/src/lib.rs --full`; ausência de match deve permanecer explícita.

## Riscos

- Fronteira a conferir: `crates/corelink-ratelimit/src/audit.rs:151` contém `impl InMemoryRateLimitAuditSink {`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-ratelimit/src/audit.rs:192` contém `impl RateLimitAuditSink for InMemoryRateLimitAuditSink {`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-ratelimit/src/audit.rs:272` contém `#[cfg(test)]`; provar seleção e efeito, não inferir runtime do nome.
- Coordenar contratos com consumidores declarados corelink-billing, corelink-server, e2e-resilience. Aresta Cargo não certifica chamada ou produção.

## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/corelink-ratelimit/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/corelink-ratelimit/Cargo.toml -p corelink-ratelimit --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

## Fontes

- [crates/corelink-ratelimit/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-ratelimit/Cargo.toml); blob `f9f358c65aa557937cd755d0789c08831b1737f8`.
- [crates/corelink-ratelimit/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-ratelimit/src/lib.rs); blob `f6d03b3f5d8eea26914501b0c8f8438a00f17a3f`.
- [docs/knowledge/crates/operations.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/crates/operations.md); blob `f85d562c6e20b4552ed452a569c38e1f66467440`.
- [docs/knowledge/tenancy/governance.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/tenancy/governance.md); blob `197398fb2e465bfe4aef8f3f0a8d7d31dbac93bf`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
