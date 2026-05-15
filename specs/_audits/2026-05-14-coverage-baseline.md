---
id: "AUDIT-R7-1-COVERAGE-BASELINE-2026-05-14"
type: "audit"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
sprint: "R7-1"
parent_wi: "R7-1-SUPPLY-QUALITY-ROLLUP"
owner: "Gustavo Schneiter"
tags: ["audit", "coverage", "supply-quality-rollup", "r7-1", "baseline"]
---

# Test-coverage baseline — workspace cargo-llvm-cov (R7-1)

## Escopo

Estabelece o **baseline T0** de cobertura de testes (LLVM source-based) para o
workspace CoreLink, com 70+ crates. A partir desse ponto, qualquer regressão
agregada >2 pp em PR DEVE ser justificada via comentário sticky no PR (gerado
automaticamente por `.github/workflows/coverage.yml`).

## Procedimento canônico

```bash
# Dev local
cargo install cargo-llvm-cov --locked
bash scripts/coverage.sh        # HTML + SUMMARY.txt
open target/coverage/html/index.html

# CI (.github/workflows/coverage.yml)
# - Roda em cada PR que toca crates/**, apps/**, Cargo.{toml,lock}
# - Upload HTML artifact (90d)
# - Sticky PR comment com a SUMMARY.txt
```

A invocação canônica usa `--workspace --no-default-features` para alinhar com
o gate `cas_foundation.yml` (que compila o target wasm32 sem features
host-only). Para o lane host-server o operador pode rodar
`COV_FLAGS="--all-features" bash scripts/coverage.sh`.

## Baseline T0 (a colar após primeira execução em CI)

A tabela abaixo deve ser populada na primeira execução verde do workflow
`coverage.yml` em main. Source-of-truth: artefato `coverage-html-main` da
primeira run + `target/coverage/SUMMARY.txt`.

| Crate                                    | Lines % | Functions % | Regions % | Notas                          |
| ---------------------------------------- | ------- | ----------- | --------- | ------------------------------ |
| _populated by first green CI run_        |         |             |           | T0 baseline (commit `4f115db`) |

**Threshold inicial (não bloqueante na T0)**: 60% line coverage por crate.
Crates abaixo desse threshold ficam em watchlist e devem ser endereçados
durante R7-2/R7-3 (charter: zero loose ends GA).

## Critérios de progressão T1 → T2 → T3

| Marco | Critério                                          | Gate          |
| ----- | ------------------------------------------------- | ------------- |
| T0    | Baseline registrado, workflow verde               | informativo   |
| T1    | Agregado workspace ≥ 70% line coverage            | warn-only     |
| T2    | Agregado ≥ 75% AND nenhum crate < 50%             | required PR   |
| T3 GA | Agregado ≥ 80% AND nenhum crate < 60%             | required main |

## Não-objetivos

- Coverage de fuzz targets (já endereçado por `cargo-fuzz` + `fuzz-all.sh`).
- Coverage de mutation testing (já endereçado por `cargo-mutants` baselines).
- Coverage de `examples/` e `tests/` (excluídos por `--avoid-dev-deps`-style
  flags via filtros do próprio `cargo-llvm-cov` quando relevante).

## Referências

- `scripts/coverage.sh` — wrapper canônico
- `.github/workflows/coverage.yml` — gate CI (SHA-pinned)
- Tool: cargo-llvm-cov 0.6.16 (taiki-e/install-action SHA-pinned)
- Charter: R7-1 supply-quality rollup deliverable (a)
