---
id: "AUDIT-R7-1-LICENSE-AUDIT-2026-05-14"
type: "audit"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
sprint: "R7-1"
parent_wi: "R7-1-SUPPLY-QUALITY-ROLLUP"
owner: "Gustavo Schneiter"
tags: ["audit", "license", "supply-chain", "r7-1", "policy"]
---

# Dependency license audit — workspace allow-list policy (R7-1)

## Escopo

Auditoria human-readable de **toda dependência transitiva** do workspace
CoreLink contra a allow-list canônica em `deny.toml` `[licenses].allow`,
complementando o gate `cargo-deny` (`.github/workflows/cargo-deny.yml`) com:

1. tabela publishable (crate × license × allowed?) para o evidence pack;
2. gate CI próprio (`.github/workflows/license-policy.yml`) que falha o PR
   se qualquer dep cair fora da allow-list;
3. weekly cron (Monday 07:00 UTC) detectando re-licenciamento upstream.

## Allow-list canônica

Source-of-truth: `deny.toml` `[licenses].allow`. Espelhada em
`scripts/license-audit.sh` (`ALLOWED=(...)`). Qualquer divergência entre os
dois é violação CTRL-SUPPLY-005.

| SPDX                              | Categoria         | Permitido | Notas                                  |
| --------------------------------- | ----------------- | --------- | -------------------------------------- |
| MIT                               | permissivo        | sim       |                                        |
| Apache-2.0                        | permissivo        | sim       |                                        |
| Apache-2.0 WITH LLVM-exception    | permissivo        | sim       | rustc/LLVM toolchain stack             |
| BSD-2-Clause                      | permissivo        | sim       |                                        |
| BSD-3-Clause                      | permissivo        | sim       |                                        |
| ISC                               | permissivo        | sim       |                                        |
| MPL-2.0                           | weak-copyleft     | sim       | file-level copyleft apenas             |
| Unicode-DFS-2016                  | permissivo (data) | sim       | unicode tables                         |
| Unicode-3.0                       | permissivo (data) | sim       | unicode tables (v3)                    |
| Zlib                              | permissivo        | sim       |                                        |
| CC0-1.0                           | public-domain     | sim       |                                        |
| 0BSD                              | permissivo        | sim       |                                        |
| CDLA-Permissive-2.0               | permissivo (data) | sim       | webpki-roots 1.0+, R1-9 dep update     |

## Block-list (anti-pattern para hosted service)

| SPDX prefix      | Motivo                                                  |
| ---------------- | ------------------------------------------------------- |
| GPL-*            | viral copyleft incompatível com closed-source workers   |
| AGPL-*           | network-copyleft — hosted service ficaria coberto       |
| SSPL-*           | server-side public license (MongoDB family)             |
| CDLA-Sharing-*   | data-sharing copyleft                                   |
| Commons-Clause   | non-OSS restrictive add-on                              |
| BUSL-*           | business-source delayed-OSS (HashiCorp)                 |

**Política implícita**: tudo fora da allow-list é bloqueado por default
(default-deny). O block-list acima é informativo, não exaustivo.

## Procedimento

```bash
cargo install cargo-license --locked
bash scripts/license-audit.sh
# → target/license-audit/licenses.json     (raw cargo-license JSON)
# → target/license-audit/licenses.tsv      (crate \t version \t license)
# → target/license-audit/VIOLATIONS.txt    (empty se clean)
```

Exit codes: `0` clean, `1` tool missing, `2` violações encontradas.

## Tabela populada (a colar após primeira run verde em CI)

A tabela completa crate × license será extraída de
`target/license-audit/licenses.tsv` na primeira execução verde do workflow em
main e colada aqui como referência T0.

| Crate           | Version | License        | Allowed? |
| --------------- | ------- | -------------- | -------- |
| _populated by first green CI run_ |   |                |          |

## Referências

- `deny.toml` `[licenses]` — canonical allow-list
- `scripts/license-audit.sh`
- `.github/workflows/license-policy.yml` — gate CI (SHA-pinned)
- `.github/workflows/cargo-deny.yml` — gate independente (cargo-deny)
- INV-SUPPLY-LICENSE-ALLOWLIST, CTRL-SUPPLY-005
