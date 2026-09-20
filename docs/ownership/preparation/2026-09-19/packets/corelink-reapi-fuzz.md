# Preparação: corelink-reapi-fuzz

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/corelink-reapi/fuzz/Cargo.toml`. **Baseline:** `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

## Fatos

- Package `corelink-reapi-fuzz` em `crates/corelink-reapi/fuzz/Cargo.toml`; 3 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 3 arquivos Rust rastreados, 61 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-reapi/fuzz/fuzz_targets/audit_request_id_total.rs`, `crates/corelink-reapi/fuzz/fuzz_targets/parse_read_resource_name.rs`, `crates/corelink-reapi/fuzz/fuzz_targets/proto_decode_batch_update.rs`.

## Targets

3 targets enumerados em `../census.json`, registro cujo `manifest` é `crates/corelink-reapi/fuzz/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/corelink-reapi/fuzz/fuzz_targets/audit_request_id_total.rs`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-reapi/fuzz/fuzz_targets/audit_request_id_total.rs)
- [`crates/corelink-reapi/fuzz/fuzz_targets/parse_read_resource_name.rs`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-reapi/fuzz/fuzz_targets/parse_read_resource_name.rs)
- [`crates/corelink-reapi/fuzz/fuzz_targets/proto_decode_batch_update.rs`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-reapi/fuzz/fuzz_targets/proto_decode_batch_update.rs)

## Relações

3 declarações de dependência e 0 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: nenhum na população Cargo examinada.

## OKF

- Nenhum conceito OKF declara diretamente fonte própria deste package no levantamento de source_files; não equivale a ausência de documentação semântica.
- Contexto herdado de dependência: `docs/knowledge/crates/cas-ac-core.md` — CAS/AC core crate cluster
- Contexto herdado de dependência: `docs/knowledge/planes/replication-failover.md` — Multi-region replication + failover plane
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-reapi/fuzz/fuzz_targets/audit_request_id_total.rs --full`; ausência de match deve permanecer explícita.

## Riscos

- Nenhum consumidor Cargo entre os packages elegíveis nesta seleção declarada. Verificar CLI, FFI, workflows, dados e clientes externos antes de alegar isolamento.
- O harness tem workspace próprio. Testar o alvo original e registrar limites/corpus, sem creditar cobertura da crate-mãe somente pela localização da pasta.

## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1 --manifest-path crates/corelink-reapi/fuzz/Cargo.toml`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/corelink-reapi/fuzz/fuzz_targets/audit_request_id_total.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/corelink-reapi/fuzz/Cargo.toml -p corelink-reapi-fuzz --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

## Fontes

- [crates/corelink-reapi/fuzz/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-reapi/fuzz/Cargo.toml); blob `0be0b179e2442e2e44522b3f483cb636d44ece7f`.
- [crates/corelink-reapi/fuzz/fuzz_targets/audit_request_id_total.rs](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-reapi/fuzz/fuzz_targets/audit_request_id_total.rs); blob `17ebfd6f62b690c0ceefd86d1f7adf839e3bb0dc`.
- [crates/corelink-reapi/fuzz/fuzz_targets/parse_read_resource_name.rs](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-reapi/fuzz/fuzz_targets/parse_read_resource_name.rs); blob `6b9a36063f5c0ec0532b9e5ee39edb47e3997b49`.
- [crates/corelink-reapi/fuzz/fuzz_targets/proto_decode_batch_update.rs](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-reapi/fuzz/fuzz_targets/proto_decode_batch_update.rs); blob `0fb24df7e09edd76b290ffc9b0ddb0898fd60ab0`.
- [docs/knowledge/crates/cas-ac-core.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/crates/cas-ac-core.md); blob `01b501172f40115174fe6c88d8942af5f3590b51`.
- [docs/knowledge/planes/replication-failover.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/planes/replication-failover.md); blob `b71c40a5df34dac3205a71eff9af17ce2971368e`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
