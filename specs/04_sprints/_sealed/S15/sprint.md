---
id: "S-15"
type: "sprint"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-29"
updated: "2026-04-29"
lane: "STANDARD"
# lane_forcing_factors omitted: STANDARD lane permite empty (REG-LANE-003 só obriga se lane=HIGH_RISK)
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "REMOTE-CACHE-PRODUCT-PROFILE"
  - "AUTH-MODEL"
  - "OBSERVABILITY-MODEL"
  - "FAILURE-MODES"
  - "SECURITY-MODEL"
tags: ["sprint", "s15", "cli", "sdk", "bazel", "buck2", "ffi", "dx", "standard"]
---

# Sprint S-15 — CLI Tool `corelink` (7 Subcommands ls/get/put/stat/bench/doctor/version + JSON Output + Cross-OS Signed: macOS Notarized + Linux GPG + Windows Authenticode) + SDK Integration (Bazel Starter Project + Buck2 Starter Project + FFI Wrappers Python pyO3 + Go cgo + JS/TS WASM Client-Verify Default-On per CTRL-CAS-002) + CI Templates 3 Providers (GitHub Actions + GitLab + CircleCI) + Cargo-Fuzz 1M Iterations + 2 OSS Proof-of-Conversion

> **doc_status:** DRAFT · **lane:** STANDARD · **Versão:** 1.0.0 · **2026-04-29**
> **Owner:** Gustavo Schneiter · **Aprovador Final:** Gustavo Schneiter
> **Spec contract base:** `_spec_contract.md` v1.1.0 (cycle 9 SOTA elevation; STANDARD lane no forcing factors)

> **Phase boundary:** Fase 4 — Developer experience tooling production-ready pré-GA.
> **STANDARD lane rationale (zero forcing factors):** CLI/SDK consume endpoints já validated em S-01..S-04 (CAS write/read + Auth + AC); não introduz novo path de tenant data; client-verify default-on reuses `corelink-client-verify` Rust crate from S-02 (single source of truth, no per-language drift); CTRL-CRED-001 enforcement em CLI output sanitization é well-bounded surface; telemetry opt-in default-off é privacy-first sem novo collection surface. Fuzz test 1M cargo-fuzz + cross-OS signing (Apple notarize + Linux GPG + Windows Authenticode) handle distribution-side risks sem multi-tenant data flow exposure.

---

## 1. Objetivo

Entregar **ferramental cliente production-ready** com **time-to-first-cache-hit ≤ 5 min**: CLI `corelink` em Rust (7 subcommands ls/get/put/stat/bench/doctor/version + `--output=json` flag em todos para scripting + cross-OS signed binaries 3 OSes — macOS arm64+x86_64 notarized via Apple Developer cert, Linux arm64+x86_64 GPG-signed, Windows x86_64 Authenticode-signed); Bazel `examples/bazel-starter` + Buck2 `examples/buck2-starter` real projects com `.bazelrc`/`.buckconfig` references + integration tests reais em GitHub Actions CI (clone → `bazel build //:hello` → confirma cache hit); FFI wrappers (Python pyO3 → PyPI `corelink-py`, Go cgo → pkg.go.dev `corelink-go`, JS/TS WASM → npm `@corelink/client`) com **client verify default-on per CTRL-CAS-002** reusing `corelink-client-verify` Rust crate (single Rust truth — ADR-0016 documents trade-off vs native HTTP per-language duplication = drift risk rejected); CI templates production-ready em 3 providers (`templates/ci/github-actions/corelink-cache.yml` + `templates/ci/gitlab-ci/corelink-cache.yml` + `templates/ci/circleci/corelink-cache.yml`) com `CORELINK_PAT` secret input + cache hit ratio output; telemetry opt-in default-off (`corelink config set telemetry on` discoverable; never `--telemetry=on` cmdline flag bypass; LINDDUN privacy review); cargo-fuzz 1M random inputs zero panics zero secrets leaked CLI fuzz test; Apple notarization + GPG + Authenticode automation completed; 2 OSS projects integrating CoreLink documented em `examples/case-studies.md` (pre-engagement Q3 com Forge + 1 external Bazel-using OSS).

Decomposição em 6 WIs: (1) **WI-S15-001** crate `corelink-cli` em Rust com 7 subcommands + JSON output flag + cross-OS release pipeline (cargo-dist or custom; macOS/Linux/Windows matrix via GitHub Actions) + signing setup placeholders preparados para WI-006 cert acquisition; (2) **WI-S15-002** Bazel integration starter project `examples/bazel-starter/` com `WORKSPACE` + `BUILD.bazel` + `.bazelrc` reference (`--remote_cache=https://corelink.humangr.com/v1/cache --credential_helper=%workspace%/.bazel/corelink-credential-helper.sh` — credential helper protocol Bazel 6+; PAT NUNCA em argv; Lote 10.15 codex P0 CTRL-CRED-001 canonical fix) + README ≤ 5 min setup + GitHub Actions CI test (clone → build → confirma cache hit) + benchmark; (3) **WI-S15-003** Buck2 integration análogo (`examples/buck2-starter/` + `.buckconfig` + CI test); (4) **WI-S15-004** FFI wrappers 3 languages (Python pyO3 + Go cgo + JS/TS WASM) com client-verify default-on per CTRL-CAS-002 reusing `corelink-client-verify` crate + per-language idiomatic API (Python async/await + Go context + JS Promise) + ADR-0016 documenting FFI vs native HTTP trade-off (rejected duplication) + tests valgrind/MSAN + WASM bundle size benchmark; (5) **WI-S15-005** CI templates 3 providers (GitHub Actions + GitLab + CircleCI YAML real working samples) + telemetry opt-in flag (`corelink config set telemetry on`) + privacy review LINDDUN; (6) **WI-S15-006** ship gate: cargo-fuzz 1M iterations + Apple notarization + Authenticode cert acquisition + automation + 2 OSS proof-of-conversion engagement + `examples/case-studies.md` + PRR STANDARD 5-8 sign-offs canonical ship gate. Implementa **CAP-CLI-001..003 + CAP-SDK-001..005** + reforça **CTRL-CAS-002** (client verify default-on FFI 3 languages) + **CTRL-CRED-001** (no secrets em CLI output; fuzz verifies). Não introduz novas INVs (SDK é consumer, não creator) — mantém INV-CAS-INTEGRITY (CRITICAL herdada §3.X) via client verify enforcement em FFI.

**Por que SOTA:** CLI ergonomics + SDK quality é onde dev tools win/lose. Competitors NativeLink/BuildBuddy têm CLI básico + Bazel docs sem starter projects testados em CI; Stripe CLI e Heroku CLI são gold-standard em UX (`doctor` actionable + JSON output + 3-OS signed binaries). CoreLink S-15 entrega: (a) `corelink doctor` 8 checks actionable diagnostic (Network + Auth + Storage write + Storage read + BYOK + Region + Quota + Client verify) com next-action por failure linkado a error_taxonomy COR_* codes (Lote 9.5c canonical 8 checks vs 6-checks inconsistency Codex R3-14 fix); (b) Bazel + Buck2 starter projects testados em CI continuous (zero competitors); (c) FFI wrappers em 3 languages com client-verify default-on (zero competitors); (d) cargo-fuzz 1M iterations zero panics zero secrets leaked (Stripe/Heroku-tier rigor); (e) Apple notarized + GPG + Authenticode 3-OS signed (Stripe/Heroku parity em distribution). Reference: **The 12-Factor CLI** principles, **Heroku CLI Style Guide** (UX heuristics), **Stripe CLI Documentation** (gold-standard `doctor` + JSON output + signing), **REAPI v2 Specification** (Bazel Remote Execution API).

## 2. Escopo

### 2.1 In-scope

- **WI-S15-001**: Crate `corelink-cli` em Rust (workspace member); 7 subcommands com clap derive (`ls --tenant <id> --prefix <p>`, `get <digest> [-o file]`, `put <file> [--digest=<d>]`, `stat <digest>`, `bench [--write|--read|--full]`, `doctor [--json]`, `version`); `--output=json` flag canonical em todos via shared output formatter; auth via env var `CORELINK_PAT` ou config file `~/.corelink/config.toml` (PAT NUNCA em CLI args — security; reject `--pat` flag explicitly com error message redirecionando para env var); `corelink config` subcommand para set/get/list (telemetry opt-in `corelink config set telemetry on` lives here); cross-OS release pipeline GitHub Actions matrix (macOS arm64 + macOS x86_64 + Linux arm64 + Linux x86_64 + Windows x86_64); signing pipeline placeholders preparados (Apple Developer cert + GPG key + Authenticode cert acquisition em WI-006); reproducible builds via `cargo build --frozen --locked` + `SOURCE_DATE_EPOCH` deterministic (S-12 alignment); SemVer discipline (CLI breaking changes = major bump; deprecation warnings ≥ 90d antes de remoção).
- **WI-S15-002**: `examples/bazel-starter/` real project com `WORKSPACE` + `BUILD.bazel` + `.bazelrc` reference (`--remote_cache=https://corelink.humangr.com/v1/cache --credential_helper=%workspace%/.bazel/corelink-credential-helper.sh` — credential helper protocol Bazel 6+; PAT NUNCA em argv; Lote 10.15 codex P0 CTRL-CRED-001 canonical fix) + `README.md` step-by-step ≤ 5 min setup + GitHub Actions CI test integration (clone → `bazel build //:hello` → segunda invocation confirma cache hit via Bazel `--noremote_upload_local_results` log inspection); benchmark sample build com / sem cache + report; fixture project não-trivial (hello-world inline + 1 transitive dep para validar cross-target dedup); `docs/integrations/bazel.md` user guide.
- **WI-S15-003**: `examples/buck2-starter/` análogo: `BUCK` files + `.buckconfig` reference + `README.md` ≤ 5 min setup + GitHub Actions CI test + benchmark + `docs/integrations/buck2.md`; project parity com Bazel sample (mesmo escopo de hello-world + 1 dep) para apples-to-apples DX comparison.
- **WI-S15-004**: FFI wrappers 3 languages reusing `corelink-client-verify` Rust crate (single source of truth):
  - **Python via pyO3**: package `corelink-py` em PyPI; class `CoreLinkClient(pat, tenant_id)` async/await native (asyncio integration); type stubs `.pyi`; tests via pytest + memory safety harness MSAN; sample usage `examples/python/`.
  - **Go via cgo**: module `corelink-go` em pkg.go.dev; context-based API (`func (c *Client) Get(ctx context.Context, digest string) ([]byte, error)`); tests via `go test -race`; sample usage `examples/go/`.
  - **JS/TS via WASM**: package `@corelink/client` em npm; Promise-based API; TypeScript declaration `.d.ts`; bundle size ≤ 1MB benchmark (tree-shake); tests via Jest; sample usage `examples/javascript/`.
  - Client-verify default-on per CTRL-CAS-002 verified em 3 languages via test (BLAKE3 verify post-download; opt-out requires explicit `verify=false` flag com warning logged).
  - **ADR-0016** documents trade-off (FFI = single Rust truth, sem drift; native HTTP per-language = duplica client-verify logic across 3 languages = drift risk; rejected).
- **WI-S15-005**: CI templates real working samples em `templates/ci/`:
  - `github-actions/corelink-cache.yml`: workflow with `CORELINK_PAT` secret + Bazel/Buck2 step + cache hit ratio output em GitHub Actions summary.
  - `gitlab-ci/corelink-cache.yml`: análogo GitLab CI.
  - `circleci/corelink-cache.yml`: análogo CircleCI.
  - Cada template: input `CORELINK_PAT` secret (env var canonical), output cache hit ratio (parse Bazel/Buck2 logs); README com copy-paste integration.
  - Telemetry opt-in default-off implementation: `corelink config set telemetry on` discoverable via `corelink config list`; LINDDUN privacy review (data collected anonymized: CLI version + OS + subcommand + success/fail; nunca tenant_id, blob digests, PAT); property test 0 emissions sem flag.
- **WI-S15-006**: Ship gate cumulative — cargo-fuzz 1M random inputs em CLI subcommands surface (input parsing + config file parsing + JSON deserialization paths; 0 panics; 0 secrets leaked em error paths via `secret_redaction_check` harness); Apple notarization workflow (acquire Apple Developer Program cert ahead of D-day + signtool automation + notarize submit + staple); Linux GPG signing (release binaries signed + GPG public key published); Windows Authenticode signing (acquire EV code-signing cert + signtool automation; fallback unsigned com warning até cert ready se acquisition slips); 2 OSS proof-of-conversion (pre-engagement Q3 com Forge + 1 external Bazel-using OSS project; documented em `examples/case-studies.md` com adoption story + setup time + cache hit ratio); PRR STANDARD doc S-15 com 5-8 sign-offs canonical (Owner + Final Approver + Engineer + QA Lead + Product + DevX advisor + Docs lead = 7 typical); evidence pack: time-to-first-cache-hit ≤ 5 min measured via dev workshop sample (3 external developers); fuzz test 1M green; binaries signed 3 OSes; CI templates 3 providers tested.

### 2.2 Anti-scope

- Execute Action SDK (Fase 2 — Remote Execution).
- Proprietary protocols não-REAPI (REAPI v2 é o padrão; sem fork).
- IDE extensions (VSCode plugin, JetBrains) — pós-GA Q1.
- GUI desktop client (CLI is enough; Web UI is admin S-16).
- Auto-installer brew/apt/yum repos — pós-GA Q1; manual download via `curl | sh` é GA baseline.
- Full SDK feature parity em FFI (e.g., admin ops via Python) — restrict to client-verify + cache ops.
- Telemetry opt-out hidden — must be explicit `corelink config set telemetry off` discoverable.
- Native HTTP client per-language (rejected per ADR-0016 — drift risk).

## 3. Customer Impact & Journey

**JTBD:** "Como Build Engineer em prospect avaliando CoreLink, preciso provar **time-to-first-cache-hit ≤ 5 min** end-to-end: (a) baixar `corelink` CLI signed para meu OS (macOS notarized / Linux GPG / Windows Authenticode); (b) configurar `CORELINK_PAT` via env var; (c) clone `examples/bazel-starter` (ou Buck2) → `bazel build //:hello` → confirma cache hit em CI subsequent invocation; (d) integrar via FFI Python/Go/JS no meu pipeline; (e) usar CI template (GitHub Actions / GitLab / CircleCI) production-ready. Como Security Engineer, preciso evidence que: client-verify é default-on em FFI 3 languages (CTRL-CAS-002 reflection); CLI nunca leak secrets em output (CTRL-CRED-001; fuzz test 1M zero leaks); telemetry é opt-in default-off (privacy-first; nunca data collected sem explicit flag). Como Developer Advocate, preciso `corelink doctor` 8 checks actionable com next-action per failure linkado a error_taxonomy COR_* codes para reduzir setup friction."

**CAPs entregues:** CAP-CLI-001 (7 subcommands) + CAP-CLI-002 (cross-OS distribution signed) + CAP-CLI-003 (`doctor` actionable diagnostic 8 checks) + CAP-SDK-001 (Bazel starter) + CAP-SDK-002 (Buck2 starter) + CAP-SDK-003 (FFI wrappers 3 languages client-verify) + CAP-SDK-004 (CI templates 3 providers) + CAP-SDK-005 (telemetry opt-in default-off).

**Persona 1 — Build Engineer em prospect**:
- Setup time benchmark: clone `examples/bazel-starter` → first cache hit ≤ 5 min (vs NativeLink ~10 min, BuildBuddy ~8 min); measured via dev workshop sample weekly + tracked monthly.
- CLI UX consistency: subcommands semantics POSIX-like flags; `--help` rich; `--version` precise (semver + git rev + SLSA attestation link); `--output=json` em todos para scripting.
- Diferenciador competitivo: 7 subcommands signed 3 OSes + Bazel + Buck2 starter projects testados em CI continuous + FFI 3 languages client-verify default-on.

**Persona 2 — Security Engineer**:
- CTRL-CAS-002 (client verify default-on) enforced em FFI 3 languages via test (single Rust truth `corelink-client-verify` reused; ADR-0016).
- CTRL-CRED-001 (no secrets em CLI output) enforced via cargo-fuzz 1M iterations + `secret_redaction_check` harness 0 leaks em error paths.
- Telemetry opt-in default-off: LINDDUN review; property test 0 emissions sem flag; data anonymized (CLI version + OS + subcommand + success/fail; nunca tenant_id, blob digests, PAT).

**Persona 3 — Developer Advocate / Customer Success**:
- `corelink doctor` 8 checks actionable: Network + Auth + Storage write + Storage read + BYOK + Region + Quota + Client verify; per-failure next-action linkado a `docs/error_taxonomy.md COR_*` code.
- CI templates 3 providers (GitHub Actions + GitLab + CircleCI) production-ready; copy-paste integration; cache hit ratio em workflow summary.
- 2 OSS case-studies (`examples/case-studies.md`) — Forge primary + 1 external Bazel-using OSS — adoption story + setup time + cache hit ratio.

**SLA addendum**:
- Time-to-first-cache-hit: ≤ 5 min (measured via dev workshop 3 external developers; tracked monthly).
- CLI fuzz test: 1M random inputs cargo-fuzz; 0 panics; 0 secrets leaked em error paths.
- FFI wrappers: client-verify default-on em 3 languages via test; memory safety valgrind/MSAN per-language harness.
- CI templates: real builds em 3 providers (GitHub Actions + GitLab + CircleCI); sample workflow files committed.
- 2 OSS proof-of-conversion: documented em `examples/case-studies.md` antes de SEAL.
- Cross-OS signing: macOS notarized + Linux GPG + Windows Authenticode (fallback unsigned com warning se cert acquisition slips; explicit waiver + ADR).

## 4. Capability Mapping (trace)

Ver `_spec_contract.md §4`. Foundation: `remote_cache_product_profile.md` (REAPI client semantics) + `auth_model.md` (PAT format hybrid `corelink_<env>_<token_id>.<random_secret>.<hmac_sig>` per S-03 decision (a); PAT NUNCA em CLI args) + `observability_model.md §3.1` (Prometheus snake_case + `plan` label canonical para CLI telemetry opt-in se applicable; INV-OBS-CARDINALITY-BUDGET nunca per-tenant labels) + `failure_modes.md` (FM-150 transient network + FM-160 auth invalid) + `security_model.md` (CTRL-CAS-002 client verify default-on; CTRL-CRED-001 no secrets em output).

## 5. Deliverables

| ID | Entregável | Onde | DoD |
|---|---|---|---|
| S15-D1 | Crate `corelink-cli` 7 subcommands + JSON output + cross-OS release pipeline | `crates/corelink-cli/` + `.github/workflows/release-cli.yml` | Crate em workspace; 7 subcommands com clap derive; `--output=json` em todos; auth via env var ou config file (NUNCA `--pat` flag); release pipeline 3 OSes verde em GH Actions matrix; reproducible builds `--frozen --locked` + `SOURCE_DATE_EPOCH` |
| S15-D2 | Bazel starter project + CI test + docs | `examples/bazel-starter/` + `docs/integrations/bazel.md` | `WORKSPACE` + `BUILD.bazel` + `.bazelrc` reference; README ≤ 5 min setup; GH Actions CI test verde (build → cache hit confirmed); benchmark com/sem cache; integration docs publicados |
| S15-D3 | Buck2 starter project + CI test + docs | `examples/buck2-starter/` + `docs/integrations/buck2.md` | `BUCK` files + `.buckconfig` reference; README ≤ 5 min setup; CI test verde; benchmark; integration docs publicados; parity com Bazel sample |
| S15-D4 | FFI wrappers (Python pyO3 + Go cgo + JS/TS WASM) + ADR-0016 | `crates/corelink-py/` + `crates/corelink-go/` + `crates/corelink-wasm/` + `specs/_decisions/ADR-0016-ffi-vs-native-http.md` | 3 packages publicados (PyPI / pkg.go.dev / npm); client-verify default-on em 3 languages via test; per-language idiomatic API (async/await / context / Promise); valgrind/MSAN clean; ADR-0016 documenta trade-off; samples em `examples/python|go|javascript/` |
| S15-D5 | CI templates 3 providers + telemetry opt-in | `templates/ci/github-actions/` + `templates/ci/gitlab-ci/` + `templates/ci/circleci/` + `crates/corelink-cli/src/config.rs` (telemetry) | 3 YAML real working samples; `CORELINK_PAT` secret input + cache hit ratio output; telemetry opt-in `corelink config set telemetry on` (default off); LINDDUN privacy review; property test 0 emissions sem flag |
| S15-D6 | Ship gate — cargo-fuzz 1M + Apple notarize + Authenticode + 2 OSS proof-of-conversion + PRR STANDARD | `fuzz/fuzz_targets/cli_input.rs` + `.github/workflows/notarize.yml` + `examples/case-studies.md` + `specs/04_sprints/_sealed/S15/PRR-S15.md` | cargo-fuzz 1M iter green (0 panics, 0 secrets leaked); macOS notarized + Linux GPG + Windows Authenticode (or waiver+ADR se cert slip); 2 OSS engaged + case-study docs; PRR STANDARD 5-8 sign-offs canonical |

## 6. Escopo técnico por camada (inherits_from)

### 6.1 Auth Model (herda `auth_model.md`)

- PAT format hybrid `corelink_<env>_<token_id>.<random_secret>.<hmac_sig>` (S-03 cycle 9 SEAL decision (a) HMAC + Argon2id verifier server-side; client transmits literal token).
- CLI auth via env var `CORELINK_PAT` ou config file `~/.corelink/config.toml` (TOML format; chmod 600 enforced em primeiro launch); **NUNCA** via CLI args (`--pat` flag rejected explicitly com error message + suggested env var alternative).
- `corelink doctor` Auth check (#2 of 8): valida PAT format + tenant scope + expiry; failure linkado a `COR_AUTH_*` error code com next-action.
- FFI wrappers receive PAT via constructor argument (`CoreLinkClient(pat, tenant_id)`); never logged; never em telemetry payload.

### 6.2 Security Model (herda `security_model.md`)

- **CTRL-CAS-002** (client verify default-on) — IMPLEMENTA primary em FFI 3 languages via reuse `corelink-client-verify` Rust crate (S-02); test verifies default-on em Python pyO3 + Go cgo + JS/TS WASM; opt-out requires explicit `verify=false` flag com warning logged.
- **CTRL-CRED-001** (no secrets em CLI output) — IMPLEMENTA via output sanitization (PAT redacted em `--debug` mode + error paths); fuzz test 1M iter cargo-fuzz `secret_redaction_check` harness 0 leaks; cargo-deny + cargo-audit clean.
- **CTRL-AUDIT-002** (rich audit events) herdada — CLI emits opcional telemetry (opt-in default-off) consumido via observability stack; nunca PII (no tenant_id, no blob digests, no PAT).
- INV-CAS-INTEGRITY (CRITICAL — registry §3.X herdada) reforced via FFI client-verify default-on protege contra bit rot post-download.

### 6.3 Observability Model (herda `observability_model.md §3.1`)

CLI telemetry opt-in default-off; Prometheus snake_case com `plan` label canonical (NUNCA per-tenant labels per INV-OBS-CARDINALITY-BUDGET):

- `corelink_cli_invocation_total{subcommand, os, cli_version, outcome}` (counter; outcome ∈ ok|fail|panic; emitido apenas se `corelink config get telemetry == on`).
- `corelink_cli_doctor_check_total{check_name, outcome}` (check_name ∈ network|auth|storage_write|storage_read|byok|region|quota|client_verify; outcome ∈ ok|fail).
- `corelink_cli_fuzz_iterations_total` (counter CI; reports 1M target).
- `corelink_sdk_ffi_client_verify_total{language, outcome}` (language ∈ python|go|js; outcome ∈ ok|hash_mismatch).
- `corelink_sdk_ffi_invocation_total{language, op}` (op ∈ get|put|stat).
- `corelink_ci_template_cache_hit_ratio{provider}` (gauge; provider ∈ github_actions|gitlab|circleci; emitido quando customer adota template + telemetry opt-in).

Cardinality budget INV-OBS-CARDINALITY-BUDGET respeitado (≤ 20k séries únicas per métrica; ≤ 100k total; **NUNCA per-tenant labels**).

### 6.4 Failure Modes (herda `failure_modes.md`)

- **FM-150** (transient network) — CLI implements retry com exponential backoff (3 retries default; configurable via `CORELINK_RETRY_MAX`); FFI wrappers idem; `corelink doctor` Network check (#1) reports retry behavior.
- **FM-160** (auth invalid) — CLI returns clear error `COR_AUTH_INVALID` com next-action (verify env var, regenerate PAT em admin UI); never echoes PAT in error message.

### 6.5 Resilience Patterns (herda `resilience_patterns.md`)

- Retry transient errors (FM-150) — exponential backoff 100ms..1s..10s; jitter 50%; max 3 retries default.
- Circuit-breaker not applicable (CLI is short-lived; reuses HTTP client per-invocation).
- Reproducible builds — `cargo build --frozen --locked` + `SOURCE_DATE_EPOCH` deterministic timestamps (S-12 alignment); supply chain integrity baseline.

## 7. Definition of Done (lane STANDARD)

> **Single-phase SEAL D+15** (STANDARD lane; DoD §6 não requer "30d sustained" criteria — only weekly conversion benchmark + fuzz test green em PR + cross-OS signed binaries). Sem two-phase observation window (vs S-13/S-14 HIGH_RISK 30d sustained gates).

- [ ] **WIs SEALED**: 6/6 (EVT-031).
- [ ] **Bazel starter project** faz cache hit em CoreLink em ≤ 5 min de setup (CI test verde) (EVT-018).
- [ ] **Buck2 starter project** análogo (EVT-018).
- [ ] **CLI fuzz test** zero panics em 1M random inputs (cargo-fuzz) (EVT-002).
- [ ] **CLI** distribuído + signed em 3 OSes (Apple notarized + GPG + Authenticode) (EVT-024).
- [ ] **SDKs published**: crates.io (`corelink-cli`), PyPI (`corelink-py`), npm (`@corelink/client`), pkg.go.dev (`corelink-go`); zero CVEs HIGH/CRITICAL (EVT-001).
- [ ] **`corelink doctor` actionable** report: 8/8 checks pass com next-action per failure (EVT-018).
- [ ] **CI templates** funcional em 3 providers (GitHub Actions + GitLab + CircleCI) — sample real builds (EVT-018).
- [ ] **2 real OSS projects** integrating CoreLink em CI (proof of conversion) — listed em `examples/case-studies.md` (EVT-018).
- [ ] **PRR STANDARD**: Owner + Final Approver + Engineer + QA Lead + Product + DevX advisor + Docs lead (5-8 sign-offs canonical; 7 typical).
- [ ] **Runbook**: nenhum novo runbook (CLI is read-mostly + diagnostic; failure paths já cobertos em S-01..S-04 runbooks).
- [ ] **10.s15.1** `corelink doctor` report acionável — próxima ação em cada falha (EVT-018).
- [ ] **10.s15.2** Integration tests: Bazel + Buck2 reais fazendo builds reais em CI sustained 7d (EVT-018).
- [ ] **10.s15.3** **CLI fuzz test** 1M random inputs — 0 panics, 0 secrets leaked em output (EVT-002 + EVT-009 fuzz).
- [ ] **10.s15.4** **FFI wrappers client-verify default-on** — verified em 3 languages via test (EVT-002).
- [ ] **10.s15.5** **Telemetry opt-in privacy** — CLI emit zero data sem flag explicit (EVT-049).
- [ ] **10.s15.6** **Time-to-first-cache-hit ≤ 5 min** — measured via dev workshop com 3 external developers (EVT-018).
- [ ] **CTRL-CAS-002** (client verify default-on) enforced em FFI 3 languages.
- [ ] **CTRL-CRED-001** (no secrets em CLI output) enforced via fuzz test + redaction harness.
- [ ] **INV-CAS-INTEGRITY** (CRITICAL — registry §3.X herdada) reforced via FFI client-verify default-on; nenhuma INV nova introduzida.
- [ ] **ADR-0016** (FFI vs native HTTP) committed; trade-off documented.
- [ ] **Cost regression gate**: CLI/SDK distribution infra ≤ $200/mês (GitHub Actions matrix + signing certs amortized + npm/PyPI/crates.io free tier).
- [ ] **Métricas underscored Prometheus**: 6+ CLI/SDK metrics emitting em staging com label `plan` aplicável (NUNCA per-tenant labels per INV-OBS-CARDINALITY-BUDGET) (EVT-013).

## 8. Dependencies

### Hard blockers

- **S-01 + S-02 + S-03 + S-04 SEALED** (CAS write + read + Auth + AC operational).

### Soft blockers

- **S-09 SEALED** (CLI emite telemetry consumindo observability stack quando opt-in).

### Outbound

- S-16 (admin UI may consume CLI as backend logic).
- S-18 (public docs reference CLI examples).
- S-19 (customer onboarding uses starter projects).
- S-20 (GA exige 2 OSS projects integrating + CLI signed 3 OSes + FFI wrappers published).

## 9. Timeline

- **Sprint kick-off**: D+0 (após S-04 + S-09 SEALED; STANDARD lane no two-phase SEAL).
- **D+5**: WI-S15-001 SEALED (CLI 7 subcommands + cross-OS release pipeline).
- **D+8**: WI-S15-002 + WI-S15-003 SEALED (Bazel + Buck2 starter projects).
- **D+12**: WI-S15-004 SEALED (FFI wrappers 3 languages + ADR-0016).
- **D+13**: WI-S15-005 SEALED (CI templates 3 providers + telemetry opt-in).
- **D+15**: WI-S15-006 SEALED (cargo-fuzz 1M + Apple notarize + Authenticode + 2 OSS proof-of-conversion).
- **D+15**: **Single-phase SEAL ceremony** (STANDARD lane; DoD §6 não requer 30d observation window — only weekly conversion benchmark + fuzz test verde em PR + cross-OS signed binaries; PRR coletados; sprint review).
- **Total**: 2.5 semanas (12 dias úteis) + buffer 3 dias.

## 10. Risk Register

Ver `_spec_contract.md §15` (10 riscos: Bazel/Buck2 REAPI quirks não cobertos, FFI bugs em Python/Go/JS wrappers, CLI distribution Windows signing Authenticode cert process, Apple notarization rejected binary heuristic mismatch, Time-to-first-cache-hit > 5 min UX miss, Telemetry leak PII collected sem opt-in, PAT em CLI args bypass user habit, cargo-fuzz misses panic path, OSS adoption slow 2 projects pre-GA, JS WASM bundle size > 1MB).

## 11. Observability Plan

DASH-CLI-SDK (novo dashboard, opt-in only via customer telemetry):
- CLI invocation rate per subcommand (`corelink_cli_invocation_total{subcommand}`).
- `corelink doctor` check pass rate per-check.
- FFI wrapper invocation rate per language.
- Client-verify hash mismatch counter per language (alert > 0).
- CI template adoption rate per provider.
- Cargo-fuzz CI iterations counter (1M target reached).

Métricas listadas em §6.3 (6+); todas com label `plan` aplicável quando customer-emitted; cardinality budget INV-OBS-CARDINALITY-BUDGET respeitado (NUNCA per-tenant labels).

## 12. Security & Privacy

**STRIDE delta** (vs S-04 baseline):
- **Spoofing**: PAT format hybrid HMAC + Argon2id (S-03 cycle 9 decision (a)); CLI never accepts PAT em args (env var ou config file canonical); FFI wrappers receive via constructor argument.
- **Tampering**: client-verify default-on em FFI 3 languages (CTRL-CAS-002 reflection); BLAKE3 verify post-download detects tampered blob bytes; opt-out explicit warning logged.
- **Repudiation**: CLI telemetry opt-in default-off; quando on, anonymized payload (CLI version + OS + subcommand + outcome; nunca tenant_id, blob digests, PAT); LINDDUN review.
- **Information disclosure**: CTRL-CRED-001 enforced via cargo-fuzz 1M iter `secret_redaction_check` harness 0 leaks em error paths; PAT redacted em `--debug` mode; never echoed em error messages.
- **DoS**: CLI is short-lived single-invocation; FFI wrappers reuse HTTP client per-invocation; retry com exponential backoff bounds (max 3 default).
- **Elevation of privilege**: CLI is client-side; tenant_id scoped via PAT (server enforces auth_model); FFI wrappers cannot escalate beyond PAT scope.

**LINDDUN delta**:
- **Linkability**: CLI telemetry opt-in default-off; quando on, NÃO inclui tenant_id (anonymized version + OS + subcommand + outcome); cardinality budget respeitado.
- **Identifiability**: telemetry payload nunca inclui PII (no email, no tenant_id, no blob digests).
- **Non-repudiation**: opt-in explicit; LINDDUN review confirms anonymization.
- **Detectability**: client-verify hash mismatch counter alert > 0 (`corelink_sdk_ffi_client_verify_total{outcome=hash_mismatch}` SEV-2).
- **Disclosure**: secrets nunca em CLI output (CTRL-CRED-001); fuzz verifies.
- **Unawareness**: telemetry opt-in is discoverable via `corelink config list`; documented em `docs/cli/telemetry.md`.
- **Non-compliance**: GDPR Art. 25 (data protection by design — opt-in default-off); LGPD Art. 6º X (transparency); LINDDUN review committed em `specs/_audits/2026-XX-XX-linddun-cli-telemetry.md`.

## 13. Post-mortem hooks

Triggers que **automaticamente abrem post-mortem doc** (per `_spec_contract.md §18`):

- CLI fuzz panic em produção (user reported) → 5-Why mandatório + cargo-fuzz iter delta.
- Secret leak em CLI output (any) → CRITICAL post-mortem + Security review.
- Time-to-first-cache-hit > 10 min sustained — post-mortem (DX regression).
- FFI wrapper memory unsafety detected (valgrind/MSAN positive) → CRITICAL post-mortem.
- Telemetry collected sem opt-in → CRITICAL post-mortem + Privacy + Legal.
- Apple notarization rejected pos-D-day → notarize early staging + iterate; documented em build pipeline.
- Authenticode cert process slip > 1 release → fallback unsigned com warning; expedite cert acquisition.

## 14. Sign-off (STANDARD 5-8 canonical; 7 typical)

7 roles canonical para STANDARD lane (per framework §33.5.4): Owner + Final Approver + Engineer (S-15 lead) + QA Lead + Product + DevX advisor + Docs lead.

**Lane rationale (STANDARD vs HIGH_RISK 11)**:
- CLI/SDK consume endpoints já validated em S-01..S-04; não introduz novo path de tenant data flow direto.
- Client-verify default-on reuses single Rust truth (`corelink-client-verify` crate S-02); zero per-language drift surface.
- CTRL-CRED-001 enforcement em CLI output é well-bounded (cargo-fuzz 1M iter + redaction harness).
- Telemetry opt-in default-off é privacy-first sem novo data collection surface.
- Não há cripto-load-bearing controles novos (BYOK + envelope encryption + Ed25519 = S-14 HIGH_RISK; S-15 reuses).
- Não há regulatory residency exposure (S-14 cobre WNAM/ENAM/WEUR/SAM; S-15 é cliente-side tooling).

Compliance/Privacy/AppSec/Architect com Crypto SME specialization são **NÃO mandatory canonical** em STANDARD lane (folded em PR review se applicable; LINDDUN review em WI-S15-005 contributes Privacy lens within Product + DevX advisor).

## 15. Change log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-29 | Gustavo (via Claude Opus 4.7) | Criação sprint.md S-15 (cycle 12.S15.0; spec contract v1.1.0 STANDARD lane base). |

---

**Fim de S-15 sprint contract.**
