# Preparação: corelink-cli-fuzz

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `tools/cli/fuzz/Cargo.toml`. **Baseline:** `37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

<a id="fatos"></a>
## Fatos

- Package `corelink-cli-fuzz` em `tools/cli/fuzz/Cargo.toml`; 5 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 5 arquivos Rust rastreados, 204 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `tools/cli/fuzz/fuzz_targets/auth_resolution.rs`, `tools/cli/fuzz/fuzz_targets/cli_input.rs`, `tools/cli/fuzz/fuzz_targets/config_toml.rs`, `tools/cli/fuzz/fuzz_targets/json_deserialize.rs`, `tools/cli/fuzz/fuzz_targets/secret_redaction_check.rs`.

<a id="targets"></a>
## Targets

5 targets enumerados no [censo completo](../census.json), registro cujo `manifest` é `tools/cli/fuzz/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`tools/cli/fuzz/fuzz_targets/auth_resolution.rs`](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/tools/cli/fuzz/fuzz_targets/auth_resolution.rs)
- [`tools/cli/fuzz/fuzz_targets/cli_input.rs`](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/tools/cli/fuzz/fuzz_targets/cli_input.rs)
- [`tools/cli/fuzz/fuzz_targets/config_toml.rs`](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/tools/cli/fuzz/fuzz_targets/config_toml.rs)
- [`tools/cli/fuzz/fuzz_targets/json_deserialize.rs`](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/tools/cli/fuzz/fuzz_targets/json_deserialize.rs)
- [`tools/cli/fuzz/fuzz_targets/secret_redaction_check.rs`](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/tools/cli/fuzz/fuzz_targets/secret_redaction_check.rs)

<a id="relacoes"></a>
## Relações

5 declarações de dependência e 0 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: nenhum na população Cargo examinada.

<a id="okf"></a>
## OKF

- Nenhum conceito OKF declara diretamente fonte própria deste package no levantamento de source_files; não equivale a ausência de documentação semântica.
- Roteador existente: `python3 scripts/okf_context.py --file tools/cli/fuzz/fuzz_targets/auth_resolution.rs --full`; ausência de match deve permanecer explícita.

<a id="riscos"></a>
## Riscos

- Nenhum consumidor Cargo entre os packages elegíveis nesta seleção declarada. Verificar CLI, FFI, workflows, dados e clientes externos antes de alegar isolamento.
- O harness tem workspace próprio. Testar o alvo original e registrar limites/corpus, sem creditar cobertura da crate-mãe somente pela localização da pasta.

<a id="comandos"></a>
## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1 --manifest-path tools/cli/fuzz/Cargo.toml`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file tools/cli/fuzz/fuzz_targets/auth_resolution.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path tools/cli/fuzz/Cargo.toml -p corelink-cli-fuzz --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

<a id="fontes"></a>
## Fontes

- [tools/cli/fuzz/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/tools/cli/fuzz/Cargo.toml); blob `239da43bfd77a6b9a43b4c4074807dfc1d57ef45`.
- [tools/cli/fuzz/fuzz_targets/auth_resolution.rs](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/tools/cli/fuzz/fuzz_targets/auth_resolution.rs); blob `eb6d2e2d80e0a62a81f2be6db4e7d04a77fbda44`.
- [tools/cli/fuzz/fuzz_targets/cli_input.rs](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/tools/cli/fuzz/fuzz_targets/cli_input.rs); blob `86f04eb50d93d4d450a5c120ec45e5999d6829a4`.
- [tools/cli/fuzz/fuzz_targets/config_toml.rs](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/tools/cli/fuzz/fuzz_targets/config_toml.rs); blob `1c4453b2a9fe2e82f5b7a4fa7fef934b62b9d0cd`.
- [tools/cli/fuzz/fuzz_targets/json_deserialize.rs](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/tools/cli/fuzz/fuzz_targets/json_deserialize.rs); blob `dd9b589295bf07261d4d29484aa4dfda6a6b7256`.
- [tools/cli/fuzz/fuzz_targets/secret_redaction_check.rs](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/tools/cli/fuzz/fuzz_targets/secret_redaction_check.rs); blob `bf86d9a6d87a3f36fc4c24a614ddf826f7196f19`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
