# Preparação: corelink-worker-fuzz

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/corelink-worker/fuzz/Cargo.toml`. **Baseline:** `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

<a id="fatos"></a>
## Fatos

- Package `corelink-worker-fuzz` em `crates/corelink-worker/fuzz/Cargo.toml`; 2 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 2 arquivos Rust rastreados, 192 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-worker/fuzz/fuzz_targets/r2_path.rs`, `crates/corelink-worker/fuzz/fuzz_targets/r2_put_get_roundtrip.rs`.

<a id="targets"></a>
## Targets

2 targets enumerados no [censo completo](../census.json), registro cujo `manifest` é `crates/corelink-worker/fuzz/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/corelink-worker/fuzz/fuzz_targets/r2_path.rs`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-worker/fuzz/fuzz_targets/r2_path.rs)
- [`crates/corelink-worker/fuzz/fuzz_targets/r2_put_get_roundtrip.rs`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-worker/fuzz/fuzz_targets/r2_put_get_roundtrip.rs)

<a id="relacoes"></a>
## Relações

8 declarações de dependência e 0 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: nenhum na população Cargo examinada.

<a id="okf"></a>
## OKF

- Nenhum conceito OKF declara diretamente fonte própria deste package no levantamento de source_files; não equivale a ausência de documentação semântica.
- Contexto herdado de dependência: `docs/knowledge/crates/cas-ac-core.md` — CAS/AC core crate cluster
- Contexto herdado de dependência: `docs/knowledge/tenancy/isolation.md` — Tenant isolation via idFromName(tenant_id)
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-worker/fuzz/fuzz_targets/r2_path.rs --full`; ausência de match deve permanecer explícita.

<a id="riscos"></a>
## Riscos

- Nenhum consumidor Cargo entre os packages elegíveis nesta seleção declarada. Verificar CLI, FFI, workflows, dados e clientes externos antes de alegar isolamento.
- O harness tem workspace próprio. Testar o alvo original e registrar limites/corpus, sem creditar cobertura da crate-mãe somente pela localização da pasta.

<a id="comandos"></a>
## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1 --manifest-path crates/corelink-worker/fuzz/Cargo.toml`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/corelink-worker/fuzz/fuzz_targets/r2_path.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/corelink-worker/fuzz/Cargo.toml -p corelink-worker-fuzz --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

<a id="fontes"></a>
## Fontes

- [crates/corelink-worker/fuzz/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-worker/fuzz/Cargo.toml); blob `2d3c6cac3b69e9ecb6d29588bfc25028e2a6fcb2`.
- [crates/corelink-worker/fuzz/fuzz_targets/r2_path.rs](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-worker/fuzz/fuzz_targets/r2_path.rs); blob `4bda17ca5e7c1fd702d6c4f34843105650e408ec`.
- [crates/corelink-worker/fuzz/fuzz_targets/r2_put_get_roundtrip.rs](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-worker/fuzz/fuzz_targets/r2_put_get_roundtrip.rs); blob `2d4dbbc07f9ef2c91082f3693043316bcbead24e`.
- [docs/knowledge/crates/cas-ac-core.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/crates/cas-ac-core.md); blob `01b501172f40115174fe6c88d8942af5f3590b51`.
- [docs/knowledge/tenancy/isolation.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/tenancy/isolation.md); blob `1f0976efcf940ea2ecbb9f5b3c8dab8ec7c634ae`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
