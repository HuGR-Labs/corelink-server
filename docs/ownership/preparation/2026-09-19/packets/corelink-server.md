# Preparação: corelink-server

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/corelink-container/Cargo.toml`. **Baseline:** `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

## Fatos

- Package `corelink-server` em `crates/corelink-container/Cargo.toml`; 17 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 409 arquivos Rust rastreados, 148602 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-container/src/bin/gc_sweep.rs`, `crates/corelink-container/src/lib.rs`, `crates/corelink-container/src/main.rs`.
- Declarações de navegação (amostra, não API completa): `crates/corelink-container/src/lib.rs:70` — `pub mod byok;`; `crates/corelink-container/src/lib.rs:77` — `pub mod adapter_cache;`; `crates/corelink-container/src/lib.rs:80` — `pub mod adapter_kv;`; `crates/corelink-container/src/lib.rs:84` — `pub mod adapter_oci_kv;`

## Targets

17 targets enumerados em `../census.json`, registro cujo `manifest` é `crates/corelink-container/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/corelink-container/src/bin/gc_sweep.rs`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-container/src/bin/gc_sweep.rs)
- [`crates/corelink-container/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-container/src/lib.rs)
- [`crates/corelink-container/src/main.rs`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-container/src/main.rs)

## Relações

75 declarações de dependência e 0 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: nenhum na população Cargo examinada.

## OKF

- Direto: `docs/knowledge/adr/adr-s14-001-multi-region-terraform-module.md` — ADR-S14-001 — Multi-region Terraform module + per-region KV namespace + DO EU jurisdiction
- Direto: `docs/knowledge/adr/adr-s14-002-region-pinning-enforcement.md` — ADR-S14-002 — Tenant region-pinning enforcement (custom domain authoritative)
- Direto: `docs/knowledge/adr/adr-s14-007-erasure-attestation-ed25519-jcs.md` — ADR-S14-007 — Erasure attestation: Ed25519 + RFC 8785 JCS + 30d key overlap
- Direto: `docs/knowledge/auth/argon2id-verify.md` — Argon2id adapter-plane verification + scope
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-container/src/bin/gc_sweep.rs --full`; ausência de match deve permanecer explícita.

## Riscos

- Investigar ativação e compatibilidade das features declaradas: byok-aws-real, byok-azure-real, byok-gcp-real, byok-vault-real, cf-billing-real, cf-r2-real, default. Não assumir que --all-features é válido.
- Fronteira a conferir: `crates/corelink-container/src/adapter_cache.rs:487` contém `#[cfg(test)]`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-container/src/adapter_kv.rs:193` contém `#[cfg(test)]`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-container/src/adapter_oci_kv.rs:158` contém `#[cfg(test)]`; provar seleção e efeito, não inferir runtime do nome.
- Nenhum consumidor Cargo entre os packages elegíveis nesta seleção declarada. Verificar CLI, FFI, workflows, dados e clientes externos antes de alegar isolamento.

## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/corelink-container/src/bin/gc_sweep.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/corelink-container/Cargo.toml -p corelink-server --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

## Fontes

- [crates/corelink-container/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-container/Cargo.toml); blob `e9feace6365776c6a4141ff59efa70fe1b36e158`.
- [crates/corelink-container/src/bin/gc_sweep.rs](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-container/src/bin/gc_sweep.rs); blob `47b9bfb144f3254f3ba246f6989e4317b252877d`.
- [crates/corelink-container/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-container/src/lib.rs); blob `da50cdc85190bb4a0d08a63f0c75ab60b0465f17`.
- [crates/corelink-container/src/main.rs](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-container/src/main.rs); blob `9e8983c85b8ae3f0841a294d3d70f6817f2d412c`.
- [docs/knowledge/adr/adr-s14-001-multi-region-terraform-module.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/adr/adr-s14-001-multi-region-terraform-module.md); blob `7098744947b45ad3481f836947c3466ae3519dac`.
- [docs/knowledge/adr/adr-s14-002-region-pinning-enforcement.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/adr/adr-s14-002-region-pinning-enforcement.md); blob `537ab4403f6858045f502af66ff6a5ce28429d9d`.
- [docs/knowledge/adr/adr-s14-007-erasure-attestation-ed25519-jcs.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/adr/adr-s14-007-erasure-attestation-ed25519-jcs.md); blob `d18d0859868a6f816d835b5a4f93f5612df575bc`.
- [docs/knowledge/auth/argon2id-verify.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/auth/argon2id-verify.md); blob `b4005f6d276c5d1843fee6a324c123f4cec1aa1f`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
