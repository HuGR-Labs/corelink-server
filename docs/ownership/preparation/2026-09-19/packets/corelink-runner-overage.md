# Preparação: corelink-runner-overage

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/corelink-runner-overage/Cargo.toml`. **Baseline:** `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

## Fatos

- Package `corelink-runner-overage` em `crates/corelink-runner-overage/Cargo.toml`; 1 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 1 arquivos Rust rastreados, 263 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-runner-overage/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `crates/corelink-runner-overage/src/lib.rs:56` — `pub enum RunnerTier {`; `crates/corelink-runner-overage/src/lib.rs:107` — `pub fn from_sku(raw: &str) -> Option<Self> {`; `crates/corelink-runner-overage/src/lib.rs:123` — `pub fn overage_vcpu_seconds(total_vcpu_seconds: u128, tier: RunnerTier) -> u128 {`; `crates/corelink-runner-overage/src/lib.rs:135` — `pub fn overage_vcpu_hours_decimal(overage_vcpu_seconds: u128) -> String {`

## Targets

1 targets enumerados em `../census.json`, registro cujo `manifest` é `crates/corelink-runner-overage/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/corelink-runner-overage/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-runner-overage/src/lib.rs)

## Relações

0 declarações de dependência e 1 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: `corelink-runner-aggregate`.

## OKF

- Nenhum conceito OKF declara diretamente fonte própria deste package no levantamento de source_files; não equivale a ausência de documentação semântica.
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-runner-overage/src/lib.rs --full`; ausência de match deve permanecer explícita.

## Riscos

- Fronteira a conferir: `crates/corelink-runner-overage/src/lib.rs:37` contém `#![forbid(unsafe_code)]`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-runner-overage/src/lib.rs:169` contém `#[cfg(test)]`; provar seleção e efeito, não inferir runtime do nome.
- Coordenar contratos com consumidores declarados corelink-runner-aggregate. Aresta Cargo não certifica chamada ou produção.

## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/corelink-runner-overage/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/corelink-runner-overage/Cargo.toml -p corelink-runner-overage --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

## Fontes

- [crates/corelink-runner-overage/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-runner-overage/Cargo.toml); blob `5d8dcd5fd8e54a013f50346971af91abfcc2c7f5`.
- [crates/corelink-runner-overage/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-runner-overage/src/lib.rs); blob `277409f6f8df452e070e5633962c95bee1722e60`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
