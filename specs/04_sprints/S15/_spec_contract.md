---
id: "SPEC-CONTRACT-S15"
type: "spec_contract"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-24"
updated: "2026-04-24"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["spec-contract", "s15", "cli", "sdk", "bazel", "buck2", "standard"]
---

# Spec Contract — S-15: CLI Tool + SDK Integration (Bazel/Buck2/Native)

## 0. Metadata

| Sprint ID | S-15 | Lane | STANDARD |
|---|---|---|---|
| Duração | 2.5 semanas | WIs | 5 |

## 1. Objetivo

Entregar ferramental cliente: CLI `corelink` para ops/debugging; SDK config pra Bazel remote_cache + Buck2 remote_cache; documentação de integração CI (GitHub Actions, GitLab, CircleCI). Cliente precisa de 5 min setup pra ver value.

## 2. Lane + forcing factors

- **Lane:** STANDARD.

## 3. Inherits_from

```yaml
inherits_from:
  - "REMOTE-CACHE-PRODUCT-PROFILE"
  - "AUTH-MODEL"
  - "OBSERVABILITY-MODEL"
```

## 4. CAPs entregues

- **CAP-CLI-001**: CLI `corelink` (ls, get, put, stat, bench, doctor).
- **CAP-SDK-001**: Bazel remote_cache configuration examples + docs.
- **CAP-SDK-002**: Buck2 remote_cache configuration.
- **CAP-SDK-003**: Client verify library (reuse S-02 crate) em Python/Go/JS wrappers.
- **CAP-SDK-004**: CI integration templates (GitHub Actions, GitLab, CircleCI).

## 5. Requirements específicos

- **R-S15-1**: Crate `corelink-cli` + release binary 3 OSes (macOS, Linux, Windows).
- **R-S15-2**: Docs `/docs/integrate/bazel` + `/docs/integrate/buck2` com code snippets.
- **R-S15-3**: FFI wrappers client-verify: Python (pyO3), Go (cgo), JS/TS (WASM).
- **R-S15-4**: Starter templates: `.bazelrc` + `.buckconfig` + CI YAML.
- **R-S15-5**: `corelink doctor` command: network + auth + latency sanity check.

## 6. DoD

- [ ] 5 WIs SEALED.
- [ ] Bazel starter project faz cache hit em CoreLink em 5 min de setup.
- [ ] Buck2 starter idem.
- [ ] CLI passa pen-test básico (fuzz on input; no panics).
- [ ] SDKs publicados: crates.io, PyPI, npm, Go module.

## 7. Completeness (delta)

- [ ] **10.s15.1** `corelink doctor` report acionável (próxima ação em cada falha).
- [ ] **10.s15.2** Integration tests: Bazel + Buck2 reais fazendo builds reais em CI.

## 8. Invariants

- Client verify default-on em todos os SDKs (CTRL-CAS-002 enforcement).
- Zero secrets no CLI output (CTRL-CRED-001).

## 9. Quality Standards

- CLI UX: subcommands consistentes; `--json` output pra scripts.
- Docs com ≥ 3 examples por linguagem/tool.

## 10. Anti-scope

- ❌ Execute Action SDK (Fase 2).
- ❌ Proprietary protocols não-REAPI.

## 11. Dependencies

- Blocker: S-01+S-02+S-03+S-04 (CAS + AC + Auth operational).

## 12. WIs antecipados

| ID | Título |
|---|---|
| WI-S15-001 | Crate corelink-cli + release binaries |
| WI-S15-002 | Bazel integration docs + tests |
| WI-S15-003 | Buck2 integration docs + tests |
| WI-S15-004 | SDK wrappers (Python, Go, JS) |
| WI-S15-005 | CI templates (GH Actions, GitLab, CircleCI) |

## 13. Duração

2.5 semanas; buffer 3 dias.

## 14. Critérios de promoção

- DoD + 2 real OSS projects integrating CoreLink.

## 15. Riscos

| Risco | Prob | Impacto |
|---|---|---|
| Bazel/Buck2 REAPI quirks não cobertos | M | MEDIUM |
| FFI bugs em Python/Go/JS wrappers | M | LOW |
| CLI distribution (Windows signing) | L | LOW |

---
