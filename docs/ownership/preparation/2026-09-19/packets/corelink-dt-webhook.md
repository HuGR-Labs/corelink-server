# Preparação: corelink-dt-webhook

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/corelink-dt-webhook/Cargo.toml`. **Baseline:** `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

## Fatos

- Package `corelink-dt-webhook` em `crates/corelink-dt-webhook/Cargo.toml`; 6 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 12 arquivos Rust rastreados, 2259 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-dt-webhook/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `crates/corelink-dt-webhook/src/lib.rs:58` — `pub mod dlq;`; `crates/corelink-dt-webhook/src/lib.rs:59` — `pub mod handler;`; `crates/corelink-dt-webhook/src/lib.rs:60` — `pub mod hmac;`; `crates/corelink-dt-webhook/src/lib.rs:61` — `pub mod metrics;`

## Targets

6 targets enumerados em `../census.json`, registro cujo `manifest` é `crates/corelink-dt-webhook/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/corelink-dt-webhook/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-dt-webhook/src/lib.rs)

## Relações

12 declarações de dependência e 3 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: `corelink-dt-cli`, `corelink-dt-reconcile`, `corelink-ops`.

## OKF

- Direto: `docs/knowledge/ops/sre-operations-hub.md` — SRE operations hub (corelink-ops + satellites)
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-dt-webhook/src/lib.rs --full`; ausência de match deve permanecer explícita.

## Riscos

- Fronteira a conferir: `crates/corelink-dt-webhook/src/dlq.rs:41` contém `impl Default for InMemoryDlq {`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-dt-webhook/src/dlq.rs:47` contém `impl InMemoryDlq {`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-dt-webhook/src/dlq.rs:125` contém `#[cfg(test)]`; provar seleção e efeito, não inferir runtime do nome.
- Coordenar contratos com consumidores declarados corelink-dt-cli, corelink-dt-reconcile, corelink-ops. Aresta Cargo não certifica chamada ou produção.

## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/corelink-dt-webhook/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/corelink-dt-webhook/Cargo.toml -p corelink-dt-webhook --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

## Fontes

- [crates/corelink-dt-webhook/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-dt-webhook/Cargo.toml); blob `5c428bdee832eba0c8edcfc9936bdf385ca4df48`.
- [crates/corelink-dt-webhook/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-dt-webhook/src/lib.rs); blob `031a090ad6cb9168fdc72ed12b6d714e73886c6d`.
- [docs/knowledge/ops/sre-operations-hub.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/ops/sre-operations-hub.md); blob `3b4bc52a8504cbf4daee27b37d5e94306f2c80bc`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
