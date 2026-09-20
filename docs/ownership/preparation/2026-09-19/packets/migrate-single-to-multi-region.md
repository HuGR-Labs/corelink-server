# Preparação: migrate-single-to-multi-region

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `apps/migrate-single-to-multi-region/Cargo.toml`. **Baseline:** `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

## Fatos

- Package `migrate-single-to-multi-region` em `apps/migrate-single-to-multi-region/Cargo.toml`; 1 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 1 arquivos Rust rastreados, 327 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `apps/migrate-single-to-multi-region/src/main.rs`.

## Targets

1 targets enumerados em `../census.json`, registro cujo `manifest` é `apps/migrate-single-to-multi-region/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`apps/migrate-single-to-multi-region/src/main.rs`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/apps/migrate-single-to-multi-region/src/main.rs)

## Relações

8 declarações de dependência e 0 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: nenhum na população Cargo examinada.

## OKF

- Direto: `docs/knowledge/adr/adr-s14-001-multi-region-terraform-module.md` — ADR-S14-001 — Multi-region Terraform module + per-region KV namespace + DO EU jurisdiction
- Roteador existente: `python3 scripts/okf_context.py --file apps/migrate-single-to-multi-region/src/main.rs --full`; ausência de match deve permanecer explícita.

## Riscos

- Fronteira a conferir: `apps/migrate-single-to-multi-region/src/main.rs:17` contém `#![forbid(unsafe_code)]`; provar seleção e efeito, não inferir runtime do nome.
- Nenhum consumidor Cargo entre os packages elegíveis nesta seleção declarada. Verificar CLI, FFI, workflows, dados e clientes externos antes de alegar isolamento.

## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file apps/migrate-single-to-multi-region/src/main.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path apps/migrate-single-to-multi-region/Cargo.toml -p migrate-single-to-multi-region --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

## Fontes

- [apps/migrate-single-to-multi-region/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/apps/migrate-single-to-multi-region/Cargo.toml); blob `4063bebc8ff6c3782512e15315638248d77c927d`.
- [apps/migrate-single-to-multi-region/src/main.rs](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/apps/migrate-single-to-multi-region/src/main.rs); blob `4128e1f04c4e5ed4bec91a22afe7fd2056a4a199`.
- [docs/knowledge/adr/adr-s14-001-multi-region-terraform-module.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/adr/adr-s14-001-multi-region-terraform-module.md); blob `7098744947b45ad3481f836947c3466ae3519dac`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
