# Preparação: corelink-py

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `tools/sdks/python/Cargo.toml`. **Baseline:** `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

## Fatos

- Package `corelink-py` em `tools/sdks/python/Cargo.toml`; 2 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 2 arquivos Rust rastreados, 375 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `tools/sdks/python/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `tools/sdks/python/src/lib.rs:34` — `pub struct PyCorelinkClient {`; `tools/sdks/python/src/lib.rs:66` — `pub fn new(pat: String, tenant_id: String, client_verify: bool) -> PyResult<Self> {`; `tools/sdks/python/src/lib.rs:91` — `pub fn _inner_client_verify_enabled(&self) -> bool {`; `tools/sdks/python/src/lib.rs:109` — `pub fn get<'py>(&self, py: Python<'py>, digest: String) -> PyResult<Bound<'py, PyBytes>> {`

## Targets

2 targets enumerados em `../census.json`, registro cujo `manifest` é `tools/sdks/python/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`tools/sdks/python/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/tools/sdks/python/src/lib.rs)

## Relações

4 declarações de dependência e 0 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: nenhum na população Cargo examinada.

## OKF

- Nenhum conceito OKF declara diretamente fonte própria deste package no levantamento de source_files; não equivale a ausência de documentação semântica.
- Roteador existente: `python3 scripts/okf_context.py --file tools/sdks/python/src/lib.rs --full`; ausência de match deve permanecer explícita.

## Riscos

- Investigar ativação e compatibilidade das features declaradas: default, extension-module. Não assumir que --all-features é válido.
- Fronteira a conferir: `tools/sdks/python/build.rs:39` contém `let extension_module = std::env::var_os("CARGO_FEATURE_EXTENSION_MODULE").is_some();`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `tools/sdks/python/build.rs:47` contém `let is_apple = std::env::var("CARGO_CFG_TARGET_VENDOR").as_deref() == Ok("apple");`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `tools/sdks/python/src/lib.rs:18` contém `#![deny(unsafe_code)]`; provar seleção e efeito, não inferir runtime do nome.
- Nenhum consumidor Cargo entre os packages elegíveis nesta seleção declarada. Verificar CLI, FFI, workflows, dados e clientes externos antes de alegar isolamento.

## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file tools/sdks/python/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path tools/sdks/python/Cargo.toml -p corelink-py --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

## Fontes

- [tools/sdks/python/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/tools/sdks/python/Cargo.toml); blob `3b9bde7367978020f29d276f1f72c397c3c13bfe`.
- [tools/sdks/python/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/tools/sdks/python/src/lib.rs); blob `a32a7085c53e080dfb83502be0079ca105560fa1`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
