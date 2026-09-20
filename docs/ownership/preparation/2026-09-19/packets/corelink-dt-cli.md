# Preparação: corelink-dt-cli

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `tools/dt-cli/Cargo.toml`. **Baseline:** `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

<a id="fatos"></a>
## Fatos

- Package `corelink-dt-cli` em `tools/dt-cli/Cargo.toml`; 1 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 1 arquivos Rust rastreados, 142 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `tools/dt-cli/src/main.rs`.

<a id="targets"></a>
## Targets

1 targets enumerados no [censo completo](../census.json), registro cujo `manifest` é `tools/dt-cli/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`tools/dt-cli/src/main.rs`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/tools/dt-cli/src/main.rs)

<a id="relacoes"></a>
## Relações

7 declarações de dependência e 0 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: nenhum na população Cargo examinada.

<a id="okf"></a>
## OKF

- Nenhum conceito OKF declara diretamente fonte própria deste package no levantamento de source_files; não equivale a ausência de documentação semântica.
- Contexto herdado de dependência: `docs/knowledge/ops/sre-operations-hub.md` — SRE operations hub (corelink-ops + satellites)
- Roteador existente: `python3 scripts/okf_context.py --file tools/dt-cli/src/main.rs --full`; ausência de match deve permanecer explícita.

<a id="riscos"></a>
## Riscos

- Fronteira a conferir: `tools/dt-cli/src/main.rs:56` contém `.or_else(|| std::env::var("DT_PROJECT_UUID").ok())`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `tools/dt-cli/src/main.rs:71` contém `let mock_enabled = std::env::var("DT_MOCK_INJECTION_ENABLED")`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `tools/dt-cli/src/main.rs:75` contém `let secret = std::env::var("DT_WEBHOOK_SECRET")`; provar seleção e efeito, não inferir runtime do nome.
- Nenhum consumidor Cargo entre os packages elegíveis nesta seleção declarada. Verificar CLI, FFI, workflows, dados e clientes externos antes de alegar isolamento.

<a id="comandos"></a>
## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file tools/dt-cli/src/main.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path tools/dt-cli/Cargo.toml -p corelink-dt-cli --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

<a id="fontes"></a>
## Fontes

- [docs/knowledge/ops/sre-operations-hub.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/ops/sre-operations-hub.md); blob `3b4bc52a8504cbf4daee27b37d5e94306f2c80bc`.
- [tools/dt-cli/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/tools/dt-cli/Cargo.toml); blob `e1635058e4eb9c79790cf64f2a2d4bca5ad2ee36`.
- [tools/dt-cli/src/main.rs](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/tools/dt-cli/src/main.rs); blob `40096d928d93c7034134a308841185cef371d6c7`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
