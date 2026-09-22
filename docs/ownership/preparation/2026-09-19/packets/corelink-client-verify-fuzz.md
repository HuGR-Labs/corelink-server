# Preparação: corelink-client-verify-fuzz

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/corelink-client-verify/fuzz/Cargo.toml`. **Baseline:** `37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

<a id="fatos"></a>
## Fatos

- Package `corelink-client-verify-fuzz` em `crates/corelink-client-verify/fuzz/Cargo.toml`; 2 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 2 arquivos Rust rastreados, 266 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-client-verify/fuzz/fuzz_targets/verify_ffi.rs`, `crates/corelink-client-verify/fuzz/fuzz_targets/verify_sync.rs`.

<a id="targets"></a>
## Targets

2 targets enumerados no [censo completo](../census.json), registro cujo `manifest` é `crates/corelink-client-verify/fuzz/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/corelink-client-verify/fuzz/fuzz_targets/verify_ffi.rs`](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-client-verify/fuzz/fuzz_targets/verify_ffi.rs)
- [`crates/corelink-client-verify/fuzz/fuzz_targets/verify_sync.rs`](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-client-verify/fuzz/fuzz_targets/verify_sync.rs)

<a id="relacoes"></a>
## Relações

3 declarações de dependência e 0 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: nenhum na população Cargo examinada.

<a id="okf"></a>
## OKF

- Nenhum conceito OKF declara diretamente fonte própria deste package no levantamento de source_files; não equivale a ausência de documentação semântica.
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-client-verify/fuzz/fuzz_targets/verify_ffi.rs --full`; ausência de match deve permanecer explícita.

<a id="riscos"></a>
## Riscos

- Nenhum consumidor Cargo entre os packages elegíveis nesta seleção declarada. Verificar CLI, FFI, workflows, dados e clientes externos antes de alegar isolamento.
- O harness tem workspace próprio. Testar o alvo original e registrar limites/corpus, sem creditar cobertura da crate-mãe somente pela localização da pasta.

<a id="comandos"></a>
## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1 --manifest-path crates/corelink-client-verify/fuzz/Cargo.toml`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/corelink-client-verify/fuzz/fuzz_targets/verify_ffi.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/corelink-client-verify/fuzz/Cargo.toml -p corelink-client-verify-fuzz --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

<a id="fontes"></a>
## Fontes

- [crates/corelink-client-verify/fuzz/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-client-verify/fuzz/Cargo.toml); blob `7ee20e2fda1293652cc61956564417fba67b2047`.
- [crates/corelink-client-verify/fuzz/fuzz_targets/verify_ffi.rs](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-client-verify/fuzz/fuzz_targets/verify_ffi.rs); blob `8c92361ea688b2eea7b7ea10d067dd83c3e1501b`.
- [crates/corelink-client-verify/fuzz/fuzz_targets/verify_sync.rs](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-client-verify/fuzz/fuzz_targets/verify_sync.rs); blob `300ebb46f66d8d6b355b77ecbce836f879aa28bf`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
