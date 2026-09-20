# Preparação: corelink-adapters-cloud

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/corelink-adapters-cloud/Cargo.toml`. **Baseline:** `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

<a id="fatos"></a>
## Fatos

- Package `corelink-adapters-cloud` em `crates/corelink-adapters-cloud/Cargo.toml`; 1 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 6 arquivos Rust rastreados, 199 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-adapters-cloud/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `crates/corelink-adapters-cloud/src/lib.rs:84` — `pub mod cf;`; `crates/corelink-adapters-cloud/src/lib.rs:85` — `pub mod clerk;`; `crates/corelink-adapters-cloud/src/lib.rs:86` — `pub mod slack;`; `crates/corelink-adapters-cloud/src/lib.rs:87` — `pub mod statuspage;`

<a id="targets"></a>
## Targets

1 targets enumerados no [censo completo](../census.json), registro cujo `manifest` é `crates/corelink-adapters-cloud/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/corelink-adapters-cloud/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-adapters-cloud/src/lib.rs)

<a id="relacoes"></a>
## Relações

5 declarações de dependência e 0 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: nenhum na população Cargo examinada.

<a id="okf"></a>
## OKF

- Nenhum conceito OKF declara diretamente fonte própria deste package no levantamento de source_files; não equivale a ausência de documentação semântica.
- Contexto herdado de dependência: `docs/knowledge/crates/container-platform.md` — Container/platform crate cluster
- Contexto herdado de dependência: `docs/knowledge/ops/sre-operations-hub.md` — SRE operations hub (corelink-ops + satellites)
- Contexto herdado de dependência: `docs/knowledge/crates/billing-pipeline.md` — Billing usage→charge pipeline (emit/reconcile/aggregate/materialize + stripe-real egress)
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-adapters-cloud/src/lib.rs --full`; ausência de match deve permanecer explícita.

<a id="riscos"></a>
## Riscos

- Fronteira a conferir: `crates/corelink-adapters-cloud/src/cf.rs:13` contém `pub use corelink_cf_bindings::*;`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-adapters-cloud/src/clerk.rs:12` contém `pub use corelink_clerk_cf::*;`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-adapters-cloud/src/lib.rs:81` contém `#![forbid(unsafe_code)]`; provar seleção e efeito, não inferir runtime do nome.
- Nenhum consumidor Cargo entre os packages elegíveis nesta seleção declarada. Verificar CLI, FFI, workflows, dados e clientes externos antes de alegar isolamento.

<a id="comandos"></a>
## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/corelink-adapters-cloud/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/corelink-adapters-cloud/Cargo.toml -p corelink-adapters-cloud --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

<a id="fontes"></a>
## Fontes

- [crates/corelink-adapters-cloud/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-adapters-cloud/Cargo.toml); blob `4acb0e994d99ecc1432a9cdd65b81a3dc85d75ef`.
- [crates/corelink-adapters-cloud/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-adapters-cloud/src/lib.rs); blob `366d989daf3aacc4c6a7470d9769131f5607d1da`.
- [docs/knowledge/crates/billing-pipeline.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/crates/billing-pipeline.md); blob `e81604ff79a7b904e705844047b215d9fd7264a4`.
- [docs/knowledge/crates/container-platform.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/crates/container-platform.md); blob `e73be0b67d80e00b8d4b1140cec57da6d93dd873`.
- [docs/knowledge/ops/sre-operations-hub.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/ops/sre-operations-hub.md); blob `3b4bc52a8504cbf4daee27b37d5e94306f2c80bc`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
