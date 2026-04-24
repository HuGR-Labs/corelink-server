---
id: "ADR-0015"
type: "adr"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "0.1.0"
created: "2026-04-24"
updated: "2026-04-24"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["adr", "supply-chain", "reproducible-builds", "stub"]
---

# ADR-0015 — Reproducible Build Best-Effort com 2-Runner Diff Tolerance ≤ 5%

## Context

S-12 Supply chain hardening exige reproducible builds. Rust ecosystem (rustc + LLVM + cargo) tem fontes inerentes de non-determinism: timestamps em debug info, paths absolutos, parallel compilation ordering, etc. Achievement de 100% bit-identical é factível mas custoso (CI overhead 30%+) e fragile (qualquer dep update pode quebrar).

## Decision

Adotamos **reproducible build best-effort** com:

1. **2-runner parallel build**: GitHub Actions matrix com 2 instances; SHA-256 binário comparado.
2. **Tolerance**: diff ≤ 5% bytes (release builds em RUSTFLAGS optimized).
3. **Fontes documented**: `docs/build/reproducible.md` lista non-determinism sources + mitigations.
4. **Mitigations applied**:
   - `SOURCE_DATE_EPOCH` env var (cargo build).
   - `--remap-path-prefix` em RUSTFLAGS.
   - `rust-toolchain.toml` pin compiler version.
   - Cargo `--frozen --locked`.
5. **Goal**: 100% bit-identical em medium-term (12 months); 5% tolerance enough at GA.

## Consequences

**Positive:**
- DoD binary (não "100% OR docs") — explicit threshold ≤ 5% byte diff.
- Mitigation list documented + improvable iterativamente.
- Aligned com Reproducible Builds Project pragmatic approach.

**Negative:**
- Não alcança "true" reproducible builds at GA.
- Documentação de sources adds maintenance burden.

## Alternatives considered

- **100% bit-identical mandatory**: rejected — implementation cost outweighs marginal security gain at GA.
- **No reproducible build effort**: rejected — supply chain attack vector (modified build).

## References

- Reproducible Builds Project <https://reproducible-builds.org/>.
- Cargo deterministic builds documentation.
- `specs/04_sprints/S12/_spec_contract.md`.
