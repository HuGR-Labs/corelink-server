# Preparação: corelink-terraform-drift-consumer

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/corelink-terraform-drift-consumer/Cargo.toml`. **Baseline:** `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

## Fatos

- Package `corelink-terraform-drift-consumer` em `crates/corelink-terraform-drift-consumer/Cargo.toml`; 2 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 9 arquivos Rust rastreados, 1473 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-terraform-drift-consumer/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `crates/corelink-terraform-drift-consumer/src/lib.rs:56` — `pub mod audit;`; `crates/corelink-terraform-drift-consumer/src/lib.rs:57` — `pub mod classifier;`; `crates/corelink-terraform-drift-consumer/src/lib.rs:58` — `pub mod consumer;`; `crates/corelink-terraform-drift-consumer/src/lib.rs:59` — `pub mod error;`

## Targets

2 targets enumerados em `../census.json`, registro cujo `manifest` é `crates/corelink-terraform-drift-consumer/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/corelink-terraform-drift-consumer/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-terraform-drift-consumer/src/lib.rs)

## Relações

5 declarações de dependência e 1 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: `corelink-ops`.

## OKF

- Direto: `docs/knowledge/ops/sre-operations-hub.md` — SRE operations hub (corelink-ops + satellites)
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-terraform-drift-consumer/src/lib.rs --full`; ausência de match deve permanecer explícita.

## Riscos

- Fronteira a conferir: `crates/corelink-terraform-drift-consumer/src/audit.rs:117` contém `impl DriftAuditSink for InMemoryDriftAuditSink {`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-terraform-drift-consumer/src/classifier.rs:54` contém `#[cfg(test)]`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-terraform-drift-consumer/src/consumer.rs:131` contém `#[cfg(test)]`; provar seleção e efeito, não inferir runtime do nome.
- Coordenar contratos com consumidores declarados corelink-ops. Aresta Cargo não certifica chamada ou produção.

## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/corelink-terraform-drift-consumer/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/corelink-terraform-drift-consumer/Cargo.toml -p corelink-terraform-drift-consumer --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

## Fontes

- [crates/corelink-terraform-drift-consumer/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-terraform-drift-consumer/Cargo.toml); blob `dceb8cfb3e533f07600d15a09e4dabc1e9229f5b`.
- [crates/corelink-terraform-drift-consumer/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-terraform-drift-consumer/src/lib.rs); blob `98d5c60724334ad16602389fdb24f80a2a52e832`.
- [docs/knowledge/ops/sre-operations-hub.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/ops/sre-operations-hub.md); blob `3b4bc52a8504cbf4daee27b37d5e94306f2c80bc`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
