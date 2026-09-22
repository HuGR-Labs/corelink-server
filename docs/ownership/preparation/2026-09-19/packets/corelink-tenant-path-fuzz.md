# Preparação: corelink-tenant-path-fuzz

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/tenant-path/fuzz/Cargo.toml`. **Baseline:** `37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

<a id="fatos"></a>
## Fatos

- Package `corelink-tenant-path-fuzz` em `crates/tenant-path/fuzz/Cargo.toml`; 2 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 2 arquivos Rust rastreados, 135 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/tenant-path/fuzz/fuzz_targets/derive_prefix.rs`, `crates/tenant-path/fuzz/fuzz_targets/derive_prefix_extended.rs`.

<a id="targets"></a>
## Targets

2 targets enumerados no [censo completo](../census.json), registro cujo `manifest` é `crates/tenant-path/fuzz/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/tenant-path/fuzz/fuzz_targets/derive_prefix.rs`](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/tenant-path/fuzz/fuzz_targets/derive_prefix.rs)
- [`crates/tenant-path/fuzz/fuzz_targets/derive_prefix_extended.rs`](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/tenant-path/fuzz/fuzz_targets/derive_prefix_extended.rs)

<a id="relacoes"></a>
## Relações

4 declarações de dependência e 0 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: nenhum na população Cargo examinada.

<a id="okf"></a>
## OKF

- Nenhum conceito OKF declara diretamente fonte própria deste package no levantamento de source_files; não equivale a ausência de documentação semântica.
- Contexto herdado de dependência: `docs/knowledge/tenancy/isolation.md` — Tenant isolation via idFromName(tenant_id)
- Roteador existente: `python3 scripts/okf_context.py --file crates/tenant-path/fuzz/fuzz_targets/derive_prefix.rs --full`; ausência de match deve permanecer explícita.

<a id="riscos"></a>
## Riscos

- Nenhum consumidor Cargo entre os packages elegíveis nesta seleção declarada. Verificar CLI, FFI, workflows, dados e clientes externos antes de alegar isolamento.
- O harness tem workspace próprio. Testar o alvo original e registrar limites/corpus, sem creditar cobertura da crate-mãe somente pela localização da pasta.

<a id="comandos"></a>
## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1 --manifest-path crates/tenant-path/fuzz/Cargo.toml`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/tenant-path/fuzz/fuzz_targets/derive_prefix.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/tenant-path/fuzz/Cargo.toml -p corelink-tenant-path-fuzz --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

<a id="fontes"></a>
## Fontes

- [crates/tenant-path/fuzz/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/tenant-path/fuzz/Cargo.toml); blob `aaeeb71ed797939dfa745450b5a4640ef344e579`.
- [crates/tenant-path/fuzz/fuzz_targets/derive_prefix.rs](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/tenant-path/fuzz/fuzz_targets/derive_prefix.rs); blob `465fcb908f422101e87e4e7662e13e0dfaf77d97`.
- [crates/tenant-path/fuzz/fuzz_targets/derive_prefix_extended.rs](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/tenant-path/fuzz/fuzz_targets/derive_prefix_extended.rs); blob `fa2a23560b4af0f5070363de6294e9b751228134`.
- [docs/knowledge/tenancy/isolation.md](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/docs/knowledge/tenancy/isolation.md); blob `1f0976efcf940ea2ecbb9f5b3c8dab8ec7c634ae`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
