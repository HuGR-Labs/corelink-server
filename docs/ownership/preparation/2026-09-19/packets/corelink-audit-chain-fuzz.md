# Preparação: corelink-audit-chain-fuzz

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/corelink-audit-chain/fuzz/Cargo.toml`. **Baseline:** `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

## Fatos

- Package `corelink-audit-chain-fuzz` em `crates/corelink-audit-chain/fuzz/Cargo.toml`; 2 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 2 arquivos Rust rastreados, 188 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-audit-chain/fuzz/fuzz_targets/jcs_canonicalize.rs`, `crates/corelink-audit-chain/fuzz/fuzz_targets/merkle_append.rs`.

## Targets

2 targets enumerados em `../census.json`, registro cujo `manifest` é `crates/corelink-audit-chain/fuzz/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/corelink-audit-chain/fuzz/fuzz_targets/jcs_canonicalize.rs`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-audit-chain/fuzz/fuzz_targets/jcs_canonicalize.rs)
- [`crates/corelink-audit-chain/fuzz/fuzz_targets/merkle_append.rs`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-audit-chain/fuzz/fuzz_targets/merkle_append.rs)

## Relações

6 declarações de dependência e 0 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: nenhum na população Cargo examinada.

## OKF

- Nenhum conceito OKF declara diretamente fonte própria deste package no levantamento de source_files; não equivale a ausência de documentação semântica.
- Contexto herdado de dependência: `docs/knowledge/crates/billing-pipeline.md` — Billing usage→charge pipeline (emit/reconcile/aggregate/materialize + stripe-real egress)
- Contexto herdado de dependência: `docs/knowledge/compliance/audit-chain.md` — RFC-6962 audit / transparency chain
- Contexto herdado de dependência: `docs/knowledge/crates/audit-analytics.md` — Audit/analytics crate cluster
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-audit-chain/fuzz/fuzz_targets/jcs_canonicalize.rs --full`; ausência de match deve permanecer explícita.

## Riscos

- Nenhum consumidor Cargo entre os packages elegíveis nesta seleção declarada. Verificar CLI, FFI, workflows, dados e clientes externos antes de alegar isolamento.
- O harness tem workspace próprio. Testar o alvo original e registrar limites/corpus, sem creditar cobertura da crate-mãe somente pela localização da pasta.

## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1 --manifest-path crates/corelink-audit-chain/fuzz/Cargo.toml`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/corelink-audit-chain/fuzz/fuzz_targets/jcs_canonicalize.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/corelink-audit-chain/fuzz/Cargo.toml -p corelink-audit-chain-fuzz --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

## Fontes

- [crates/corelink-audit-chain/fuzz/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-audit-chain/fuzz/Cargo.toml); blob `f89476fb6d1ed518398f041435f06e7cab05124f`.
- [crates/corelink-audit-chain/fuzz/fuzz_targets/jcs_canonicalize.rs](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-audit-chain/fuzz/fuzz_targets/jcs_canonicalize.rs); blob `df89e3e760caad406392593f2a9db4ce50b8c439`.
- [crates/corelink-audit-chain/fuzz/fuzz_targets/merkle_append.rs](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-audit-chain/fuzz/fuzz_targets/merkle_append.rs); blob `4362a2015da74947d8a7bc5c71204ec96758f51f`.
- [docs/knowledge/compliance/audit-chain.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/compliance/audit-chain.md); blob `e4a587ffccaa3a6e01af52cb039f65d0eb09fc52`.
- [docs/knowledge/crates/audit-analytics.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/crates/audit-analytics.md); blob `864bae2a19336af8486ad5485eaa3764e05db0d9`.
- [docs/knowledge/crates/billing-pipeline.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/crates/billing-pipeline.md); blob `e81604ff79a7b904e705844047b215d9fd7264a4`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
