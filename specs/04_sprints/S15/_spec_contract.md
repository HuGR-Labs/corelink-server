---
id: "SPEC-CONTRACT-S15"
type: "spec_contract"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.2.0"
created: "2026-04-24"
updated: "2026-04-29"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["spec-contract", "s15", "cli", "sdk", "bazel", "buck2", "ffi", "client-verify", "standard", "sota-v1.1"]
---

# Spec Contract — S-15: CLI Tool + SDK Integration (Bazel + Buck2 + Native FFI Wrappers)

## 0. Metadata

| Campo | Valor |
|---|---|
| Sprint ID | S-15 |
| Nome | CLI + SDK Integration |
| Lane | STANDARD |
| Lane forcing factors | n/a (STANDARD) — CLI/SDK não toca multi-tenant data flow direto; usa endpoints já validados S-01..S-04 |
| Duração estimada | 2.5 semanas |
| WIs antecipados | 6 |
| SOTA target | Time-to-first-cache-hit ≤ 5 min — CLI rich UX + Bazel/Buck2 starter projects + Python/Go/JS FFI wrappers + CI templates 3 providers |

## 1. Objetivo

Entregar **ferramental cliente production-ready** com **time-to-first-cache-hit ≤ 5 min**: CLI `corelink` (ls, get, put, stat, bench, doctor) com fuzz-tested binary 3 OSes (macOS/Linux/Windows signed), Bazel + Buck2 starter projects com `.bazelrc`/`.buckconfig` examples + integration tests reais em CI, FFI wrappers (Python pyO3, Go cgo, JS/TS WASM) com client verify default-on, CI templates (GitHub Actions / GitLab / CircleCI). Cliente precisa de **≤ 5 min setup pra ver primeiro cache hit em produção**, ou conversion drops 60% (CoreLink developer experience benchmark vs BuildBuddy onboarding).

**Por que SOTA:** CLI ergonomics + SDK quality é onde dev tools win/lose. Competitors NativeLink/BuildBuddy têm CLI básico + Bazel docs sem starter projects testados. CoreLink S-15 entrega: (a) `corelink doctor` actionable diagnostic; (b) starter projects testados em CI continuous; (c) FFI wrappers com type safety + verify default-on. Reference: **The 12-Factor CLI** principles, **Heroku CLI design heuristics**, **Stripe CLI UX**.

## 2. Lane + forcing factors

- **Lane:** STANDARD (5–8 sign-offs).
- **Não forcing factors HIGH_RISK**: CLI/SDK consome endpoints já verified em S-01..S-04; não introduz novo path de tenant data.
- **Atenção em segurança CLI**: tokens em CLI output → CTRL-CRED-001 enforcement; fuzz-test on input.

## 3. Inherits_from

```yaml
inherits_from:
  - "REMOTE-CACHE-PRODUCT-PROFILE"  # REAPI client semantics
  - "AUTH-MODEL"                     # PAT auth flow
  - "OBSERVABILITY-MODEL"            # CLI emits telemetry opt-in
  - "FAILURE-MODES"                  # FM-150 (transient network), FM-160 (auth invalid)
  - "SECURITY-MODEL"                 # CTRL-CRED-001 (no secrets in output), CTRL-CAS-002 (client verify)
```

## 4. CAPs entregues

| ID | Capability | Detalhe |
|---|---|---|
| **CAP-CLI-001** | CLI `corelink` 7 subcommands | ls, get, put, stat, bench, doctor, version. JSON `--output=json` para scripts. |
| **CAP-CLI-002** | Cross-OS distribution signed | macOS notarized + Linux GPG-signed + Windows Authenticode-signed. |
| **CAP-CLI-003** | `corelink doctor` actionable diagnostic | Checks network, auth, latency, region, BYOK setup; reports next-action per failure. |
| **CAP-SDK-001** | Bazel starter project + docs | `examples/bazel-starter` real project; `.bazelrc` reference; CI test em GitHub Actions. |
| **CAP-SDK-002** | Buck2 starter project + docs | `examples/buck2-starter`; `.buckconfig` reference; CI test. |
| **CAP-SDK-003** | FFI wrappers client-verify | Python (pyO3), Go (cgo), JS/TS (WASM); reuse `corelink-client-verify` crate S-02. |
| **CAP-SDK-004** | CI integration templates | GitHub Actions + GitLab + CircleCI YAML templates publicados. |
| **CAP-SDK-005** | Telemetry opt-in | CLI emite telemetry de usage anonymized **apenas** se `corelink config set telemetry on` (persistent flag em `~/.corelink/config.toml`); default off (privacy-first); **NÃO há `--telemetry=on` cmdline flag** — bypass via cmdline rejected per Lote 10.15 codex P2 alignment. |

## 5. Requirements específicos

### 5.1 CLI (CAP-CLI-001..003)

- **R-S15-1**: Crate `corelink-cli` em Rust com release binary 3 OSes:
  - macOS arm64 + x86_64 (notarized via Apple Developer cert).
  - Linux arm64 + x86_64 (GPG-signed releases).
  - Windows x86_64 (Authenticode-signed).
- **R-S15-2**: 7 subcommands com semantics consistentes:
  - `corelink ls --tenant <id> --prefix <p>` lista CAS/AC entries.
  - `corelink get <digest> [-o file]` baixa blob.
  - `corelink put <file> [--digest=<d>]` upload blob.
  - `corelink stat <digest>` metadata + size + age.
  - `corelink bench [--write|--read|--full]` micro-benchmark vs cluster.
  - `corelink doctor [--json]` diagnostic 8 checks.
  - `corelink version` semver + git rev + SLSA attestation link.
- **R-S15-3**: `--output=json` flag em todos subcommands para scripting.
- **R-S15-4**: Auth via env var `CORELINK_PAT` ou config file `~/.corelink/config.toml`; nunca em CLI args (security).
- **R-S15-5**: `corelink doctor` outputs **8 checks** (Lote 9.5c — alinha 8 checks vs 6 inconsistency Codex R3-14 fix):
  1. **Network**: ping CF endpoint per region; report latency p50/p99.
  2. **Auth**: PAT validity + tenant scope.
  3. **Storage write**: writeable test (1KB blob) com tenant prefix derivation.
  4. **Storage read**: readable test (round-trip integrity verify).
  5. **BYOK**: if configured, KMS access check (S-14 alignment).
  6. **Region**: tenant region matches expected (`<tenant>.<region>.corelink.dev`).
  7. **Quota**: current usage vs plan limit + soft/hard thresholds (S-07/S-08 boundary).
  8. **Client verify**: BLAKE3 verify default-on em SDK (CTRL-CAS-002 reflection).
  - Per-failure: actionable next step (link to docs error_taxonomy `COR_*` codes).

### 5.2 Bazel + Buck2 (CAP-SDK-001 + CAP-SDK-002)

- **R-S15-6**: `examples/bazel-starter/` real project com:
  - `WORKSPACE` + `BUILD.bazel` files.
  - `.bazelrc` reference (Lote 10.15 codex P0 canonical fix CTRL-CRED-001): `--remote_cache=https://corelink.dev/v1/cache --credential_helper=%workspace%/.bazel/corelink-credential-helper.sh` — credential helper protocol Bazel 6+ retorna token via stdout (nunca em argv); shell-expansion `${CORELINK_PAT}` em `--remote_header` colocaria PAT em argv (`ps aux` leak) = CTRL-CRED-001 violation. Helper script reads `CORELINK_PAT` env var + emits Bazel JSON response per <https://bazel.build/docs/credential-helper>.
  - `README.md` step-by-step ≤ 5 min setup.
  - Integration test em GitHub Actions: clone → `bazel build //:hello` → confirma cache hit.
- **R-S15-7**: `examples/buck2-starter/` análogo: `BUCK` files, `.buckconfig` reference, CI test.
- **R-S15-8**: Per BuildBuddy/NativeLink benchmark: starter project deve atingir cache hit ≤ 5 min from clone.

### 5.3 FFI Wrappers (CAP-SDK-003)

- **R-S15-9**: FFI wrappers com client verify default-on (CTRL-CAS-002):
  - **Python** via `pyO3`: `corelink-py` package em PyPI; class `CoreLinkClient(pat, tenant_id)`.
  - **Go** via `cgo`: `corelink-go` module em pkg.go.dev.
  - **JS/TS** via WASM: `@corelink/client` em npm.
- **R-S15-10**: Per-language idiomatic API: Python uses async/await native; Go uses context-based; JS uses Promise-based.
- **R-S15-11**: **Alternative considered**: native HTTP thin client per language (sem FFI) — rejected porque duplica client-verify logic across 3 languages = drift risk; FFI wraps single Rust truth. ADR-0016 documents trade-off.

### 5.4 CI Templates (CAP-SDK-004)

- **R-S15-12**: Templates em `templates/ci/`:
  - `github-actions/corelink-cache.yml`
  - `gitlab-ci/corelink-cache.yml`
  - `circleci/corelink-cache.yml`
- **R-S15-13**: Cada template: input `CORELINK_PAT` secret, output cache hit ratio.

### 5.5 Telemetry (CAP-SDK-005)

- **R-S15-14**: CLI telemetry opt-in **apenas** via `corelink config set telemetry on` (persistent em `~/.corelink/config.toml`); default off. **NÃO há `--telemetry=on` cmdline flag** (Lote 10.15 codex P2 canonical alignment) — bypass via cmdline rejected (privacy-first design).
- **R-S15-15**: Anonymized: CLI version + OS + subcommand + success/fail; nunca tenant_id, blob digests, PAT.

## 6. Definition of Done

- [ ] **WIs SEALED**: 6/6.
- [ ] **Bazel starter project** faz cache hit em CoreLink em ≤ 5 min de setup (CI test verde) (EVT-018).
- [ ] **Buck2 starter project** análogo (EVT-018).
- [ ] **CLI fuzz test** zero panics em 1M random inputs (cargo-fuzz) (EVT-002).
- [ ] **CLI** distribuído + signed em 3 OSes (Apple notarized + GPG + Authenticode) (EVT-024).
- [ ] **SDKs published**: crates.io (`corelink-cli`), PyPI (`corelink-py`), npm (`@corelink/client`), pkg.go.dev (`corelink-go`); zero CVEs HIGH/CRITICAL (EVT-001).
- [ ] **`corelink doctor` actionable** report: 8/8 checks pass com next-action per failure (EVT-018).
- [ ] **CI templates** funcional em 3 providers (GitHub Actions + GitLab + CircleCI) — sample real builds (EVT-018).
- [ ] **2 real OSS projects** integrating CoreLink em CI (proof of conversion) — listed em `examples/case-studies.md` (EVT-018).
- [ ] **PRR STANDARD**: Engineer + QA + Product + DevX advisor + Docs lead.
- [ ] **Runbook**: nenhum novo runbook (CLI is read-mostly + diagnostic; failure paths já cobertos em S-01..S-04 runbooks).

## 7. Completeness Criteria (delta local)

- [ ] **10.s15.1** `corelink doctor` report acionável — próxima ação em cada falha (EVT-018).
- [ ] **10.s15.2** Integration tests: Bazel + Buck2 reais fazendo builds reais em CI sustained 7d (EVT-018).
- [ ] **10.s15.3** **CLI fuzz test** 1M random inputs — 0 panics, 0 secrets leaked em output (EVT-002 + EVT-009 fuzz).
- [ ] **10.s15.4** **FFI wrappers client-verify default-on** — verified em 3 languages via test (EVT-002).
- [ ] **10.s15.5** **Telemetry opt-in privacy** — CLI emit zero data sem flag explicit (EVT-049).
- [ ] **10.s15.6** **Time-to-first-cache-hit ≤ 5 min** — measured via dev workshop com 3 external developers (EVT-018).

## 8. Invariants

### Mantidas (não cria invariants novas — SDK é consumer, não creator)

- **CTRL-CAS-002** (client verify default-on): enforcement em 3 FFI wrappers via test.
- **CTRL-CRED-001** (no secrets in output): CLI output sanitized; `--debug` ainda redacts; fuzz test verifies.
- **INV-CAS-INTEGRITY** (CRITICAL — herdada): client verify default-on protege contra bit rot post-download.

## 9. Quality Standards (delta local)

- **14.s15.1 CLI UX consistency**: subcommands semantics consistentes (POSIX-like flags); `--help` rich; `--version` precise.
- **14.s15.2 Docs com ≥ 3 examples** por linguagem/tool; copy-paste-runnable.
- **14.s15.3 Fuzz test** 1M random inputs em CLI; 0 panics; 0 secrets leaked em error paths.
- **14.s15.4 FFI safety**: pyO3 + cgo + WASM-bindgen tested em CI; memory safety em valgrind/MSAN per language harness.
- **14.s15.5 Conversion benchmark**: time-to-first-cache-hit measured + tracked monthly via dev workshop sample.
- **14.s15.6 Cross-platform CI**: 3 OSes (macOS/Linux/Windows) test em GitHub Actions matrix.
- **14.s15.7 Reproducible CLI builds**: Cargo `--frozen --locked`; deterministic timestamps via `SOURCE_DATE_EPOCH` (S-12 alignment).
- **14.s15.8 SemVer discipline**: CLI breaking changes = major bump; deprecation warnings ≥ 90d antes de remoção.

## 10. Anti-scope

- ❌ Execute Action SDK (Fase 2 — Remote Execution).
- ❌ Proprietary protocols não-REAPI (REAPI v2 é o padrão; sem fork).
- ❌ IDE extensions (VSCode plugin, JetBrains) — pós-GA Q1.
- ❌ GUI desktop client — anti-scope; CLI is enough; Web UI is admin (S-16).
- ❌ Auto-installer brew/apt/yum repos — pós-GA Q1; manual download via `curl | sh` é GA baseline.
- ❌ Full SDK feature parity em FFI (e.g., admin ops via Python) — restrict to client-verify + cache ops.
- ❌ Telemetry opt-out hidden — must be explicit `corelink config set telemetry off` discoverable.

## 11. Dependencies

### Hard blockers

- **S-01 + S-02 + S-03 + S-04 SEALED** (CAS write + read + Auth + AC operational).

### Soft blockers

- **S-09 SEALED** (CLI emite telemetry consumindo observability stack).

### Outbound

- S-16 (admin UI may consume CLI as backend logic).
- S-18 (public docs reference CLI examples).
- S-19 (customer onboarding uses starter projects).
- S-20 (GA exige 2 OSS projects integrating).

## 12. WIs antecipados (PERT)

| ID | Título | Sub-tasks | O | M | P | PERT |
|---|---|---|---|---|---|---|
| **WI-S15-001** | Crate corelink-cli + 7 subcommands + JSON output + cross-OS release | crate + commands + flags; JSON output; release pipeline 3 OSes; signing setup | 16h | 24h | 38h | **25.0h** |
| **WI-S15-002** | Bazel integration docs + starter project + CI test | examples/bazel-starter; .bazelrc; README; GH Actions CI test; benchmark | 10h | 14h | 22h | **14.7h** |
| **WI-S15-003** | Buck2 integration docs + starter project + CI test | examples/buck2-starter; .buckconfig; README; CI test; benchmark | 10h | 14h | 22h | **14.7h** |
| **WI-S15-004** | SDK FFI wrappers (Python pyO3 + Go cgo + JS/TS WASM) + ADR-0016 | 3 wrappers; client-verify default-on; idiomatic API per lang; tests; ADR | 18h | 28h | 44h | **28.7h** |
| **WI-S15-005** | CI templates (GH Actions + GitLab + CircleCI) + telemetry opt-in | 3 templates; sample real builds; telemetry opt-in flag; privacy review | 8h | 12h | 18h | **12.3h** |
| **WI-S15-006** | Fuzz test CLI + Apple notarization + Authenticode + 2 OSS proof-of-conversion | cargo-fuzz 1M; Apple cert + notarize; Windows Authenticode; OSS engagement | 12h | 18h | 28h | **18.7h** |

**Total PERT:** ~114h ≈ 14 dias work × 1 eng. Buffer 3 dias confere com 2.5 semanas.

## 13. Duração + Timeline

- **Duração:** 2.5 semanas (12 dias úteis) + buffer 3 dias.
- **Marcos:**
  - **D+5:** WI-001 SEALED (CLI 7 subcommands).
  - **D+8:** WI-002 + WI-003 SEALED (Bazel + Buck2 starter).
  - **D+12:** WI-004 SEALED (FFI wrappers).
  - **D+13:** WI-005 SEALED (CI templates).
  - **D+15:** WI-006 SEALED (fuzz + signing + OSS).
  - **D+15:** Sprint review.

## 14. Critérios de promoção

- DoD complete + 2 real OSS projects integrating CoreLink (proof of conversion).
- Time-to-first-cache-hit ≤ 5 min measured.
- Fuzz test 1M zero panics zero secrets leaked.
- PRR STANDARD aprovado.

## 15. Riscos (registry expandido)

| Risco | Prob | Det | Impacto | Exposure | Residual após mitigação | Mitigação |
|---|---|---|---|---|---|---|
| **Bazel/Buck2 REAPI quirks não cobertos** | M | M | MEDIUM | M | LOW | Real CI test em starter projects + community feedback channels + REAPI v2 spec adherence. |
| **FFI bugs em Python/Go/JS wrappers** | M | M | LOW | L | LOW | Per-language test harness (valgrind/MSAN) + CI matrix; ADR-0016 trade-off documented. |
| **CLI distribution Windows signing** (Authenticode cert process) | L | L | LOW | L | LOW | Acquire cert ahead of D-day + signtool automation; fallback unsigned with warning até cert ready. |
| **Apple notarization rejected** (binary heuristic mismatch) | L | M | LOW | L | LOW | Notarize early staging + iterate; documented in build pipeline. |
| **Time-to-first-cache-hit > 5 min** (UX miss) | M | M | MEDIUM (conversion impact) | M | LOW | Dev workshop sample weekly + iterate starter projects + onboarding video. |
| **Telemetry leak** (PII collected sem opt-in) | L | M | HIGH (privacy) | L | LOW | Privacy review + audit trail + property test 0 leaks. |
| **PAT em CLI args bypass** (user habit) | M | L | HIGH (credential exposure) | M | LOW | Reject `--pat` flag; refuse + suggest env var; doc warning. |
| **cargo-fuzz misses panic path** | M | L | LOW | L | LOW | Multi-engine fuzz (libFuzzer + AFL); coverage tracking; minimum 100M iterations release. |
| **OSS adoption slow** (não conseguir 2 projects pre-GA) | M | M | MEDIUM (DX validation gap) | M | LOW | Pre-engagement Q3 com Forge + 1 external Bazel-using OSS project; incentive program. |
| **JS WASM bundle size** > 1MB | M | L | LOW | L | LOW | Tree-shake dependency; benchmark; documented. |

## 16. Benchmarks SOTA externos

| Critério | NativeLink CLI | BuildBuddy CLI | Stripe CLI | Heroku CLI | **CoreLink target S-15** |
|---|---|---|---|---|---|
| 7+ subcommands | Yes | Yes | Yes | Yes | **Yes — 7 subcommands** |
| Cross-OS signed binaries | Linux only | Linux only | Yes (3 OSes) | Yes (3 OSes) | **Yes — macOS notarized + Linux GPG + Windows Authenticode** |
| `doctor` actionable diagnostic | Limited | No | Yes | Yes | **Yes — 8 checks + next-action per fail** |
| FFI wrappers (Python/Go/JS) | No | Limited | No (REST SDKs) | No | **Yes — 3 languages client-verify default-on** |
| Starter projects tested CI | Manual | Limited | Yes | Yes | **Yes — Bazel + Buck2 real CI** |
| Time-to-first-cache-hit ≤ 5 min | ~10 min | ~8 min | N/A | N/A | **≤ 5 min** |
| Fuzz-tested CLI | No | No | Yes | Yes | **Yes — 1M cargo-fuzz iterations** |
| JSON `--output` for scripts | Limited | Yes | Yes | Yes | **Yes** |
| Telemetry opt-in (privacy-first) | Always-on | Always-on | Yes | Opt-in | **Yes — opt-in default off** |

**Veredito SOTA:** S-15 v1.1 atinge feature parity com Stripe/Heroku CLI em 9/9 dimensões; vantagem em FFI wrappers + Bazel/Buck2 starter tested CI.

## 17. References (RFCs, papers, standards)

- **REAPI v2 Specification** (Bazel Remote Execution API) <https://github.com/bazelbuild/remote-apis>.
- **The 12-Factor CLI** <https://12factor.net/>.
- **Heroku CLI Style Guide** (UX heuristics).
- **Stripe CLI Documentation** <https://stripe.com/docs/stripe-cli>.
- **Apple Notarization** <https://developer.apple.com/documentation/security/notarizing_macos_software_before_distribution>.
- **Microsoft Authenticode Signing** <https://learn.microsoft.com/en-us/windows-hardware/drivers/install/authenticode>.
- **GPG Signing Best Practices** (Debian/Fedora).
- **pyO3 Documentation** <https://pyo3.rs/>.
- **WASM-bindgen** <https://rustwasm.github.io/wasm-bindgen/>.
- **cargo-fuzz** <https://rust-fuzz.github.io/book/cargo-fuzz.html>.

## 18. Post-mortem hooks

Triggers que **automaticamente abrem post-mortem doc**:

- CLI fuzz panic em produção (user reported) → 5-Why mandatório.
- Secret leak em CLI output (any) → CRITICAL post-mortem + Security review.
- Time-to-first-cache-hit > 10 min sustained — post-mortem (DX regression).
- FFI wrapper memory unsafety detected (valgrind/MSAN positive) → CRITICAL post-mortem.
- Telemetry collected sem opt-in → CRITICAL post-mortem + Privacy + Legal.

## 19. Waiver policy

S-15 **NÃO PODE** promover via waiver dos seguintes itens:

- ❌ Client verify default-on em FFI wrappers (CTRL-CAS-002 baseline).
- ❌ No secrets em CLI output (CTRL-CRED-001 baseline).
- ❌ Fuzz test 1M iterations 0 panics — CLI estabilidade baseline.

Itens waivable com Engineer lead + DevX advisor + ADR:

- ⚠️ Time-to-first-cache-hit ≤ 5 min → ≤ 8 min com plan to reduce in next sprint.
- ⚠️ FFI 3 languages → 2 languages GA (defer JS to post-GA).
- ⚠️ Apple notarization deferred → unsigned binary com warning para 1 release; complete next.

---

**Fim spec contract S-15 v1.1.0 SOTA.**
