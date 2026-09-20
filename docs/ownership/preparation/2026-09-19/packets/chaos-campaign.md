# Preparação: chaos-campaign

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `tests/chaos/Cargo.toml`. **Baseline:** `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

## Fatos

- Package `chaos-campaign` em `tests/chaos/Cargo.toml`; 12 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 12 arquivos Rust rastreados, 1700 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `tests/chaos/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `tests/chaos/src/lib.rs:68` — `pub enum CampaignRegion {`; `tests/chaos/src/lib.rs:77` — `pub enum RegionHealth {`; `tests/chaos/src/lib.rs:88` — `pub enum RouteOutcome {`; `tests/chaos/src/lib.rs:99` — `pub struct CampaignFailoverModel {`

## Targets

12 targets enumerados em `../census.json`, registro cujo `manifest` é `tests/chaos/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`tests/chaos/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/tests/chaos/src/lib.rs)

## Relações

0 declarações de dependência e 0 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: nenhum na população Cargo examinada.

## OKF

- Nenhum conceito OKF declara diretamente fonte própria deste package no levantamento de source_files; não equivale a ausência de documentação semântica.
- Roteador existente: `python3 scripts/okf_context.py --file tests/chaos/src/lib.rs --full`; ausência de match deve permanecer explícita.

## Riscos

- Investigar ativação e compatibilidade das features declaradas: chaos, default. Não assumir que --all-features é válido.
- Fronteira a conferir: `tests/chaos/src/lib.rs:55` contém `#![forbid(unsafe_code)]`; provar seleção e efeito, não inferir runtime do nome.
- Nenhum consumidor Cargo entre os packages elegíveis nesta seleção declarada. Verificar CLI, FFI, workflows, dados e clientes externos antes de alegar isolamento.

## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file tests/chaos/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path tests/chaos/Cargo.toml -p chaos-campaign --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

## Fontes

- [tests/chaos/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/tests/chaos/Cargo.toml); blob `1f970e9ba5628054dc98aa0c6a43227087e27bce`.
- [tests/chaos/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/tests/chaos/src/lib.rs); blob `3ed5d277170ae5b80f171d1c440797e18f6a2759`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
