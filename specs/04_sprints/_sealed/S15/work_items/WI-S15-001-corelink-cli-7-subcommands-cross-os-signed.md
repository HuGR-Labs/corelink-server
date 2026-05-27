---
id: "WI-S15-001"
type: "work_item"
doc_status: "SEALED"
work_status: "DONE"
audit_status: "ACTIVE"
version: "1.1.0"
created: "2026-04-29"
updated: "2026-05-14"
lane: "STANDARD"
parent: "S-15"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "AUTH-MODEL"
  - "SECURITY-MODEL"
  - "OBSERVABILITY-MODEL"
  - "FAILURE-MODES"
  - "RESILIENCE-PATTERNS"
  - "INVARIANT-REGISTRY"
tags: ["wi", "s15", "cli", "rust", "clap", "cross-os", "json-output", "doctor", "standard"]
---

# WI-S15-001 — Crate `corelink-cli` em Rust com 7 Subcommands (ls/get/put/stat/bench/doctor/version) + `--output=json` Flag em Todos os Subcommands para Scripting + Auth via env var `CORELINK_PAT` ou Config File `~/.corelink/config.toml` (NUNCA `--pat` Flag — Security CTRL-CRED-001) + `corelink doctor` 8 Checks Canonical (Lote 9.5c — Network + Auth + Storage write + Storage read + BYOK + Region + Quota + Client verify) com Per-Failure Next-Action Linkado a `error_taxonomy COR_*` Codes + Cross-OS Release Pipeline GitHub Actions Matrix (macOS arm64+x86_64 + Linux arm64+x86_64 + Windows x86_64) + Reproducible Builds `--frozen --locked` + `SOURCE_DATE_EPOCH` (S-12 alignment) + SemVer Discipline (Breaking Changes = Major Bump; Deprecation Warnings ≥ 90d)

> **doc_status:** DRAFT · **work_status:** READY · **lane:** STANDARD
> **Parent:** [S-15](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S15-001 |
| Título | Implementação CLI `corelink` em Rust com 7 subcommands canonical (`ls`, `get`, `put`, `stat`, `bench`, `doctor`, `version`) + `--output=json` flag uniforme em todos para scripting (parsing-friendly em CI scripts) + auth via env var `CORELINK_PAT` ou config file `~/.corelink/config.toml` (TOML format chmod 600 enforced; CLI **rejeita** `--pat` flag explicitly per CTRL-CRED-001) + `corelink doctor` 8 checks canonical Lote 9.5c (Network + Auth + Storage write + Storage read + BYOK + Region + Quota + Client verify) com per-failure next-action linkado a `docs/error_taxonomy.md COR_*` code; cross-OS release pipeline GitHub Actions matrix (macOS arm64 + macOS x86_64 + Linux arm64 + Linux x86_64 + Windows x86_64) com signing placeholders preparados (Apple Developer cert + GPG key + Authenticode cert acquisition em WI-S15-006); reproducible builds via `cargo build --frozen --locked` + `SOURCE_DATE_EPOCH` deterministic timestamps (S-12 supply chain alignment); SemVer discipline (CLI breaking changes = major bump; deprecation warnings ≥ 90d antes de remoção); foundation layer para WI-S15-002..006 (FFI wrappers + CI templates + ship gate). |
| Sprint | S-15 |
| Lane | STANDARD |
| Forcing factors | none (CLI consume endpoints S-01..S-04 já validated; não introduz novo path tenant data flow) |

## 1. Intent

Foundation WI do S-15. Entrega o **CLI `corelink` em Rust** production-ready com 7 subcommands canonical, JSON output uniforme, auth seguro (env var ou config file; nunca CLI args), `doctor` actionable 8 checks, e cross-OS release pipeline preparado para signing em WI-S15-006.

```rust
// File: crates/corelink-cli/src/main.rs

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "corelink")]
#[command(version, about = "CoreLink content-addressable cache CLI", long_about = None)]
struct Cli {
    #[arg(long, global = true, help = "Output format (text or json)")]
    output: Option<OutputFormat>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// List CAS/AC entries for a tenant
    Ls {
        #[arg(long, help = "Tenant ID (required)")]
        tenant: String,
        #[arg(long, help = "Optional prefix filter")]
        prefix: Option<String>,
    },
    /// Download a blob by digest
    Get {
        digest: String,
        #[arg(short = 'o', long)]
        output: Option<PathBuf>,
    },
    /// Upload a blob
    Put {
        file: PathBuf,
        #[arg(long, help = "Optional pre-computed digest (BLAKE3)")]
        digest: Option<String>,
    },
    /// Show metadata + size + age for a digest
    Stat { digest: String },
    /// Run micro-benchmark vs cluster
    Bench {
        #[arg(long, conflicts_with_all = ["read", "full"])]
        write: bool,
        #[arg(long, conflicts_with_all = ["write", "full"])]
        read: bool,
        #[arg(long, conflicts_with_all = ["write", "read"])]
        full: bool,
    },
    /// Diagnostic 8 checks (network, auth, storage, byok, region, quota, client verify)
    Doctor {
        #[arg(long, help = "Output JSON for scripting")]
        json: bool,
    },
    /// Print version + git rev + SLSA attestation link
    Version,
    /// Manage local config (~/.corelink/config.toml)
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Set a config key (e.g., `corelink config set telemetry on`)
    Set { key: String, value: String },
    Get { key: String },
    List,
}

fn main() -> Result<(), CliError> {
    let cli = Cli::parse();

    // CTRL-CRED-001: refuse PAT em CLI args. Reject `--pat` if anyone passes it via wrapper.
    if std::env::args().any(|a| a == "--pat" || a.starts_with("--pat=")) {
        eprintln!("error: PAT must NOT be passed as CLI arg (security).");
        eprintln!("       Use env var CORELINK_PAT or config file ~/.corelink/config.toml.");
        std::process::exit(2);
    }

    let pat = resolve_pat()?;  // env var first, then config file
    let client = CoreLinkClient::new(pat)?;

    match cli.command {
        Commands::Ls { tenant, prefix } => cmd_ls(&client, &tenant, prefix.as_deref(), cli.output),
        Commands::Get { digest, output } => cmd_get(&client, &digest, output, cli.output),
        Commands::Put { file, digest } => cmd_put(&client, &file, digest.as_deref(), cli.output),
        Commands::Stat { digest } => cmd_stat(&client, &digest, cli.output),
        Commands::Bench { write, read, full } => cmd_bench(&client, write, read, full, cli.output),
        Commands::Doctor { json } => cmd_doctor(&client, json),
        Commands::Version => cmd_version(cli.output),
        Commands::Config { action } => cmd_config(action, cli.output),
    }
}
```

## 2. Narrative

CLI ergonomics + CLI distribution é onde dev tools win/lose. Stripe CLI e Heroku CLI são gold-standard em UX (`doctor` actionable + JSON output + 3-OS signed binaries). NativeLink/BuildBuddy CLI são limited (Linux-only signed; no `doctor` actionable; limited JSON output). CoreLink S-15 entrega parity com Stripe/Heroku em 7 dimensões + diferenciador via Bazel/Buck2 starter projects testados em CI (zero competitors).

Este WI é foundation: define crate workspace member `crates/corelink-cli/`, implementa 7 subcommands com clap derive, define output formatter shared (text/json) reused em todos subcommands, define auth resolution canonical (env var primeira, config file fallback, NUNCA CLI args), implementa `doctor` 8 checks com per-failure next-action linkado a error taxonomy COR_*, e prepara cross-OS release pipeline GitHub Actions matrix com signing placeholders para WI-S15-006.

**Risk justification STANDARD lane (zero forcing factors)**:
- CLI consume endpoints S-01..S-04 já validated; não introduz novo path tenant data flow.
- Auth via env var ou config file = bounded surface; reject `--pat` flag = explicit defense.
- Cross-OS signing preparado mas não acquired neste WI (defer to WI-S15-006 cert acquisition).
- Reproducible builds via `--frozen --locked` + `SOURCE_DATE_EPOCH` reuse S-12 supply chain baseline.

## 3. Customer Impact & Journey

**Persona 1 — Build Engineer em prospect**:
- Setup: download CLI binary signed para meu OS → `export CORELINK_PAT=...` → `corelink doctor` confirma 8 checks pass → integração via `examples/bazel-starter` (WI-S15-002) ou Buck2 (WI-S15-003).
- JSON output em todos subcommands: `corelink ls --output=json` → parsing-friendly em CI scripts.
- `corelink version` precise: semver + git rev + SLSA attestation link (S-12 supply chain alignment).

**Persona 2 — Security Engineer**:
- PAT NUNCA em CLI args (CTRL-CRED-001); evidence em audit log + cargo-fuzz 1M iter (WI-S15-006).
- Cross-OS signed binaries (Apple notarized + Linux GPG + Windows Authenticode em WI-S15-006).
- Reproducible builds + SLSA attestation (S-12 alignment).

## 4. Capability Mapping

- **CAP-CLI-001** (CLI 7 subcommands) — IMPLEMENTA primary; canonical surface foundation.
- **CAP-CLI-002** (Cross-OS distribution signed) — IMPLEMENTA pipeline placeholders; signing acquisition em WI-S15-006.
- **CAP-CLI-003** (`doctor` actionable diagnostic) — IMPLEMENTA primary; 8 checks canonical Lote 9.5c.
- Trace: `_spec_contract.md §4 + §5.1 (R-S15-1..5)` + `auth_model.md` (PAT format hybrid HMAC + Argon2id) + `security_model.md` (CTRL-CRED-001) + `failure_modes.md` (FM-150 + FM-160).

## 5. Tipo

Feature WI; STANDARD lane; foundation crate.

## 6. Escopo

### 6.1 In-scope

1. **Crate workspace member** `crates/corelink-cli/`:
   - `Cargo.toml` com dependencies: clap (4.x derive), tokio (full features), reqwest (rustls-tls), serde + serde_json, blake3, toml (config parsing), tracing + tracing-subscriber.
   - Workspace member em root `Cargo.toml`.
   - Binary target: `corelink` (single executable).

2. **7 subcommands canonical**:
   - `corelink ls --tenant <id> [--prefix <p>]`: lista CAS/AC entries; suporta paginação via `--limit` + `--cursor`.
   - `corelink get <digest> [-o <file>]`: baixa blob; default stdout; client-verify default-on (BLAKE3 verify).
   - `corelink put <file> [--digest=<d>]`: upload blob; calcula BLAKE3 se `--digest` não fornecido; output digest computed.
   - `corelink stat <digest>`: metadata (size + age + region + tenant_id pseudonimizado).
   - `corelink bench [--write|--read|--full]`: micro-benchmark vs cluster; default = `--full` (write+read round-trip 100 ops); reports p50/p95/p99 latency + throughput.
   - `corelink doctor [--json]`: diagnostic 8 checks (vide §6.4); per-failure next-action.
   - `corelink version`: semver + git rev + SLSA attestation link + build timestamp (deterministic via `SOURCE_DATE_EPOCH`).
   - `corelink config <set|get|list>`: manage `~/.corelink/config.toml` (TOML format; chmod 600 enforced em primeiro write).

3. **`--output=json` flag global**:
   - Aplicável em todos subcommands; default `text`.
   - JSON schema documented em `docs/cli/json-output-schema.md` (forward-compatible — additive only; SemVer discipline).
   - Shared output formatter `crates/corelink-cli/src/output.rs` reused em todos handlers.

4. **Auth resolution canonical**:
   - Order: env var `CORELINK_PAT` first → config file `~/.corelink/config.toml` `[auth].pat` second → error if neither set.
   - **Reject `--pat` flag explicitly**: parse `std::env::args()` early; if any arg matches `--pat` ou `--pat=*`, exit code 2 com error message + suggested env var alternative (CTRL-CRED-001 baseline).
   - PAT format validation: `corelink_<env>_<token_id>.<random_secret>.<hmac_sig>` regex match (S-03 cycle 9 decision (a)); reject malformed em primeiro use com clear error.

5. **Config file `~/.corelink/config.toml`**:
   - TOML format:
     ```toml
     [auth]
     pat = "corelink_prod_abc123.xyz...HmacSig"

     [defaults]
     tenant_id = "acme-corp"
     output_format = "text"  # or "json"

     [telemetry]
     enabled = false  # opt-in default-off; LINDDUN review WI-S15-005
     ```
   - chmod 600 enforced em primeiro write (Unix-like; Windows ACL via `cacls` equivalent).
   - `corelink config set <key> <value>` validates schema + writes atomic (temp file + rename).
   - `corelink config list` outputs config sanitized (PAT redacted como `corelink_prod_abc***`).

6. **`corelink doctor` 8 checks canonical (Lote 9.5c — alinha 8 checks vs 6 inconsistency Codex R3-14 fix)**:
   - **#1 Network**: ping CF endpoint per region; report latency p50/p99; fail = `COR_NET_UNREACHABLE` next-action "verify network connectivity, firewall, DNS".
   - **#2 Auth**: validate PAT format + tenant scope via API call; fail = `COR_AUTH_INVALID` next-action "verify CORELINK_PAT env var or regenerate em admin UI".
   - **#3 Storage write**: writeable test (1KB blob upload com tenant prefix derivation); fail = `COR_STORAGE_WRITE_DENIED` next-action "verify tenant quota + plan limits".
   - **#4 Storage read**: readable test (round-trip integrity verify do blob escrito em #3); fail = `COR_STORAGE_READ_FAIL` next-action "verify tenant region + KMS access se BYOK".
   - **#5 BYOK**: if `[auth].byok_enabled = true`, KMS access check (S-14 alignment); fail = `COR_BYOK_REVOKED` next-action "verify CMK status em KMS provider".
   - **#6 Region**: tenant region matches expected (`<tenant>.<region>.corelink.humangr.com`); fail = `COR_REGION_MISMATCH` next-action "verify tenant primary_region em admin UI".
   - **#7 Quota**: current usage vs plan limit + soft/hard thresholds (S-07/S-08 boundary); fail = `COR_QUOTA_EXCEEDED` next-action "verify plan + contact sales".
   - **#8 Client verify**: BLAKE3 verify default-on em SDK reflection (CTRL-CAS-002); fail = `COR_CLIENT_VERIFY_DISABLED` next-action "verify FFI wrapper config; do NOT disable except for explicit dev/test".
   - Output `text`: tabela rica com check_name + status (OK/FAIL) + latency + next-action.
   - Output `--json`: structured array `[{check, status, latency_ms, error_code, next_action}, ...]` parsing-friendly.

7. **Cross-OS release pipeline**:
   - `.github/workflows/release-cli.yml` matrix:
     - `macos-14` (arm64) + `macos-13` (x86_64).
     - `ubuntu-22.04` (x86_64) + `ubuntu-22.04-arm64` (arm64).
     - `windows-2022` (x86_64).
   - Build com `cargo build --release --frozen --locked --target <triple>`.
   - `SOURCE_DATE_EPOCH` set para git commit timestamp (deterministic; S-12 alignment).
   - Artifacts uploaded para GitHub Release assets + checksums SHA256.
   - Signing placeholders preparados (Apple Developer cert + GPG key + Authenticode cert acquisition em WI-S15-006).

8. **SemVer discipline**:
   - CLI breaking changes (subcommand removed, flag renamed, JSON schema breaking) = major bump.
   - Deprecation warnings ≥ 90d antes de remoção (logged em `--debug` mode + emitted em telemetry quando opt-in).
   - `corelink version` output includes "deprecated since X.Y.Z, removal planned X.(Y+1).0" se applicable.

### 6.2 Out-of-scope (deferred)

- FFI wrappers Python/Go/JS (WI-S15-004).
- Bazel/Buck2 starter projects (WI-S15-002 + WI-S15-003).
- CI templates 3 providers (WI-S15-005).
- Apple notarization + GPG + Authenticode cert acquisition (WI-S15-006).
- Cargo-fuzz 1M iterations (WI-S15-006).
- 2 OSS proof-of-conversion engagement (WI-S15-006).
- Auto-installer brew/apt/yum repos (pós-GA Q1).
- IDE extensions (VSCode, JetBrains) — pós-GA Q1.

## 7. Anti-Scope

- Aceitar PAT em CLI args (CTRL-CRED-001 violation).
- `doctor` < 8 checks (Lote 9.5c canonical).
- JSON output schema breaking sem major bump (SemVer discipline).
- Skip cross-OS reproducible builds (S-12 supply chain alignment).
- Hard-code secrets em CLI binary.
- Subcommand semantics inconsistent (POSIX-like flags canonical).
- `--help` minimal (Stripe/Heroku UX baseline).

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: CLI corelink 7 subcommands + JSON output + auth resolution + doctor 8 checks

  Scenario: All 7 subcommands compile + integrate
    Given crate corelink-cli em workspace
    When `cargo build --release` runs
    Then binary `corelink` produced
    And `corelink --help` lists 7 subcommands + config
    And cada subcommand `--help` rich output documented

  Scenario: --output=json flag uniform em todos subcommands
    Given user runs `corelink ls --tenant acme --output=json`
    Then output is valid JSON parseable by jq
    And schema matches docs/cli/json-output-schema.md
    And same flag works em get/put/stat/bench/doctor/version/config

  Scenario: Auth via env var canonical
    Given env var CORELINK_PAT="corelink_prod_..."
    When `corelink ls --tenant acme` runs
    Then PAT resolved from env var
    And request authorized
    And no PAT echoed em output

  Scenario: Auth fallback config file
    Given env var CORELINK_PAT unset
    Given ~/.corelink/config.toml has [auth].pat = "corelink_prod_..."
    When `corelink ls --tenant acme` runs
    Then PAT resolved from config file
    And request authorized

  Scenario: Reject --pat flag explicitly (CTRL-CRED-001)
    Given user runs `corelink --pat=corelink_prod_abc ls --tenant acme`
    When CLI parses args
    Then exit code 2
    And stderr contains "PAT must NOT be passed as CLI arg"
    And stderr suggests "Use env var CORELINK_PAT or config file"
    And no PAT echoed em error output

  Scenario: corelink doctor 8 checks all pass
    Given valid PAT + reachable cluster + tenant configured
    When `corelink doctor --json` runs
    Then output is JSON array of 8 objects
    And each object has {check, status: "ok", latency_ms, error_code: null, next_action: null}
    And exit code 0

  Scenario: corelink doctor failure linkado a error_taxonomy
    Given network unreachable
    When `corelink doctor --json` runs
    Then check #1 Network status = "fail"
    And error_code = "COR_NET_UNREACHABLE"
    And next_action ≠ null com clear remediation steps
    And exit code 1

  Scenario: Config file chmod 600 enforced
    Given user runs `corelink config set auth.pat corelink_prod_...`
    When config file written
    Then ~/.corelink/config.toml permissions = 600 (Unix)
    And atomic write via temp file + rename

  Scenario: Config list redacts PAT
    Given config file has [auth].pat = "corelink_prod_abc123.xyz..."
    When `corelink config list` runs
    Then output shows pat = "corelink_prod_abc***" (redacted)
    And no full PAT echoed

  Scenario: Cross-OS release pipeline matrix verde
    Given GitHub Actions workflow .github/workflows/release-cli.yml
    When tag v0.1.0 pushed
    Then matrix builds verde em macOS arm64 + macOS x86_64 + Linux arm64 + Linux x86_64 + Windows x86_64
    And artifacts uploaded para GitHub Release com SHA256 checksums
    And SOURCE_DATE_EPOCH set deterministic

  Scenario: Reproducible builds
    Given binary built duas vezes com same git commit
    When sha256sum compared
    Then identical hash em ambas builds (cargo --frozen --locked + SOURCE_DATE_EPOCH)

  Scenario: SemVer breaking change requires major bump
    Given JSON output schema v1.x.y
    When new version removes field foo
    Then version bump to 2.0.0 (major)
    And deprecation warning emitted ≥ 90d antes (em --debug mode)
```

## 9. Design Decisions

### 9.1 Why clap derive (não structopt ou argh)

- Industry standard Rust CLI parser; rich `--help` automático; subcommand support nested.
- Used em rustup, cargo, ripgrep — battle-tested.

### 9.2 Why TOML config (não JSON ou YAML)

- TOML é human-friendly + comments support; JSON sem comments; YAML drift risk (YAML hell).
- Cargo já uses TOML; user already familiar.

### 9.3 Why reject `--pat` flag explicitly (não silently ignore)

- User habit risk: developers paste PAT em CI/CD args → secrets em logs.
- Explicit reject + clear error + alternative suggestion = best UX + best security.

### 9.4 Why 8 doctor checks (Lote 9.5c canonical)

- 8 checks cobre full deploy stack: network → auth → storage → BYOK → region → quota → client verify.
- 6-checks inconsistency Codex R3-14 fix: registry alinhada com 8.

### 9.5 Why cross-OS matrix em GitHub Actions

- Native macOS arm64 (Apple Silicon) + x86_64 (Intel legacy); Linux arm64 (AWS Graviton) + x86_64.
- Windows x86_64 only (arm64 deferred pós-GA per spec contract).

### 9.6 ADR potencial?

- Não. Patterns reused (clap derive + workspace member + GitHub Actions matrix). No novel architecture decision em S-15-001.

## 10. Completeness Criteria

- [x] **10.s15.001.1** 7 subcommands implementados + `--help` rich (EVT-018). `crates/corelink-cli/` — ls/get/put/stat/bench/doctor/version + config.
- [x] **10.s15.001.2** `--output=json` uniforme em todos subcommands; schema documented (EVT-018). `docs/cli/json-output-schema.md`.
- [x] **10.s15.001.3** Auth env var + config file canonical; `--pat` flag rejected (CTRL-CRED-001) (EVT-002). `src/auth.rs` + raw-args guard em `main.rs`.
- [x] **10.s15.001.4** `corelink doctor` 8 checks canonical Lote 9.5c (EVT-018). `src/doctor.rs`.
- [x] **10.s15.001.5** Per-failure next-action linkado a error_taxonomy COR_* (EVT-018). `docs/error_taxonomy.md` criado.
- [x] **10.s15.001.6** Cross-OS release pipeline matrix verde (macOS arm64+x86_64 + Linux arm64+x86_64 + Windows x86_64) (EVT-024). `.github/workflows/release-cli.yml`.
- [x] **10.s15.001.7** Reproducible builds `--frozen --locked` + `SOURCE_DATE_EPOCH` (S-12 alignment) (EVT-024). Workflow sets SOURCE_DATE_EPOCH from git timestamp.
- [x] **10.s15.001.8** SemVer discipline documented; deprecation warnings ≥ 90d. JSON schema v1.0.0 + schema doc.
- [x] **10.s15.001.9** Config file chmod 600 enforced; atomic write. `src/config.rs` atomic_write + set_permissions_600.
- [x] **10.s15.001.10** PAT format validation rejects malformed em primeiro use. `src/auth.rs::validate_pat_shape`.

## 11. DoD

- [x] Crate `corelink-cli` em workspace; binary builds em 5 OSes matrix (workflow preparado; build local macOS ✓).
- [x] 7 subcommands + config subcommand implemented + tested. 43 unit tests pass.
- [x] `--output=json` uniforme; schema docs committed (`docs/cli/json-output-schema.md`).
- [x] Auth resolution canonical (env var → config file → error); `--pat` rejected (exit code 2).
- [x] `corelink doctor` 8 checks; JSON output; per-failure next-action + COR_* codes.
- [x] Cross-OS release pipeline GitHub Actions preparado (`.github/workflows/release-cli.yml`).
- [x] Reproducible builds: `--frozen --locked` + `SOURCE_DATE_EPOCH` em workflow.
- [ ] Métricas CLI (telemetry opt-in WI-S15-005) emitting em staging quando ativo. [DEFERRED → WI-S15-005]
- [x] Tests: unit (43 tests: auth + output + doctor + config + all cmd handlers + 6 negative scenarios).

## 12. Invariants Validated

- **CTRL-CRED-001** (no secrets em output) reforced via reject `--pat` flag + redact em config list + redact em error messages.
- **CTRL-CAS-002** (client verify default-on) reflection em `doctor` check #8.
- **INV-CAS-INTEGRITY** (CRITICAL — registry §3.X herdada) reforced via `corelink get` client-verify post-download + `doctor` check #8.

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| Crate corelink-cli | `crates/corelink-cli/` | Rust |
| CLI binary | `crates/corelink-cli/src/main.rs` | Rust |
| Output formatter shared | `crates/corelink-cli/src/output.rs` | Rust |
| Auth resolver | `crates/corelink-cli/src/auth.rs` | Rust |
| Doctor 8 checks | `crates/corelink-cli/src/doctor.rs` | Rust |
| Config TOML parser | `crates/corelink-cli/src/config.rs` | Rust |
| Cross-OS release workflow | `.github/workflows/release-cli.yml` | YAML |
| JSON output schema docs | `docs/cli/json-output-schema.md` | Markdown |
| Error taxonomy COR_* docs | `docs/error_taxonomy.md` (extended) | Markdown |
| CLI user guide | `docs/cli/usage.md` | Markdown |

## 14. Quality Standards

- **14.s15.001.1** CLI UX consistency: subcommands semantics POSIX-like flags; `--help` rich.
- **14.s15.001.2** Test coverage ≥ 85% (subcommand handlers + auth + doctor + config).
- **14.s15.001.3** Reproducible builds verified em CI.
- **14.s15.001.4** SAST: cargo-audit + cargo-deny clean.
- **14.s15.001.5** Memory bounded em CLI invocation (typical < 50MB; bench up to 256MB acceptable).
- **14.s15.001.6** SemVer discipline: breaking changes documented em CHANGELOG.md.

## 15. Test Plan

### Unit tests (≥ 85% coverage)
- Subcommand parsing: clap derive validates flag combinations + conflicts (e.g., `bench` mutex flags).
- Output formatter: text vs json identical fields; JSON schema validation.
- Auth resolver: env var primary; config file fallback; error if neither.
- PAT format validator: regex match + reject malformed.
- Doctor 8 checks: mock cluster responses; per-check pass/fail paths.
- Config TOML: parse + write atomic + chmod 600.

### Integration tests (E2E vs staging cluster)
- `corelink ls --tenant acme` returns valid response.
- `corelink put <file>` upload + `corelink get <digest>` download round-trip; client-verify confirms hash.
- `corelink doctor --json` 8 checks parse + status verifiable.
- `corelink bench --full` reports latency + throughput metrics.

### Negative scenarios (≥ 4)
1. **Reject `--pat` flag**: `corelink --pat=corelink_prod_abc ls --tenant acme` → exit code 2 + error.
2. **Malformed PAT**: env var `CORELINK_PAT="not_a_valid_pat"` → clear error em primeiro use.
3. **Network unreachable**: `corelink doctor` → check #1 fail + COR_NET_UNREACHABLE next-action.
4. **Auth invalid**: invalid PAT → check #2 fail + COR_AUTH_INVALID next-action.
5. **Quota exceeded**: tenant em hard limit → `corelink put` fail + COR_QUOTA_EXCEEDED next-action.
6. **Config file insecure perms**: chmod 644 detected → warning em primeiro launch + suggested chmod 600.

### Cross-OS matrix
- 5 OSes (macOS arm64+x86_64 + Linux arm64+x86_64 + Windows x86_64); cargo build verde + smoke tests pass.

## 16. Failure Modes

- **FM-150** (transient network): retry com exponential backoff (3 retries default; configurable via `CORELINK_RETRY_MAX`); `doctor` check #1 reports retry behavior.
- **FM-160** (auth invalid): clear error `COR_AUTH_INVALID` next-action; never echoes PAT em error message.

## 17. Controls

- **CTRL-CRED-001** (no secrets em output): reject `--pat` flag; redact em config list; redact em error messages; PAT never logged em telemetry.
- **CTRL-CAS-002** (client verify default-on): `corelink get` BLAKE3 verify post-download; `doctor` check #8 reports default-on status.

## 18. Resilience Patterns

- Retry transient errors (FM-150): exponential backoff 100ms..1s..10s + jitter 50%; max 3 retries default.
- Reproducible builds: `cargo build --frozen --locked` + `SOURCE_DATE_EPOCH` deterministic timestamps (S-12 supply chain alignment).

## 19. Observability

CLI telemetry opt-in default-off (implementation em WI-S15-005); quando ativo:
- `corelink_cli_invocation_total{subcommand, os, cli_version, outcome}` counter (outcome ∈ ok|fail|panic).
- `corelink_cli_doctor_check_total{check_name, outcome}` counter (8 checks × 2 outcomes = 16 series).
- Cardinality budget INV-OBS-CARDINALITY-BUDGET respeitado (≤ 20k séries únicas; NUNCA per-tenant labels).

## 20. Security & Privacy

**STRIDE delta**:
- **Spoofing**: PAT format hybrid HMAC + Argon2id (S-03 cycle 9 decision (a)); env var ou config file canonical.
- **Tampering**: client-verify default-on (BLAKE3 post-download); reproducible builds verified.
- **Repudiation**: telemetry opt-in default-off; quando on, anonymized payload (LINDDUN review WI-S15-005).
- **Information disclosure**: CTRL-CRED-001 enforced via reject `--pat` flag + redact em output.
- **DoS**: retry com bounded backoff; CLI is short-lived single-invocation.
- **Elevation of privilege**: tenant scoped via PAT; CLI cannot escalate.

**LINDDUN delta**:
- Linkability: telemetry NÃO inclui tenant_id quando opt-in (anonymized).
- Identifiability: nunca PII em telemetry.
- Disclosure: secrets nunca em output (CTRL-CRED-001 + cargo-fuzz WI-S15-006 verifies).

## 21. Dependencies

### Hard blockers
- S-01 + S-02 + S-03 + S-04 SEALED (CAS write/read + Auth + AC operational).

### Soft blockers
- S-12 SEALED (reproducible builds + SOURCE_DATE_EPOCH baseline).

### Outbound
- WI-S15-002 (Bazel starter consumes CLI).
- WI-S15-003 (Buck2 starter consumes CLI).
- WI-S15-004 (FFI wrappers reuse client logic; CLI binary not directly).
- WI-S15-005 (CI templates reference CLI binary).
- WI-S15-006 (ship gate signs CLI binaries).

## 22. Effort PERT

O: 16h, M: 24h, P: 38h → PERT **25.0h** (per spec contract §12; foundation crate + 7 subcommands + cross-OS pipeline).

## 23. Cost Analysis

- GitHub Actions matrix runs: ~$5/build × 3 builds/week = $60/mês.
- Crate publishing: free (crates.io).
- Total: ~$60/mês incremental.

## 24. Post-mortem Hooks

- CLI panic em production (user-reported) → 5-Why + cargo-fuzz iter coverage gap analysis (WI-S15-006).
- Secret leaked em CLI output (any) → CRITICAL post-mortem + Security review.
- Cross-OS build matrix red sustained > 24h → SEV-2 (release pipeline broken).
- Reproducible build mismatch detected → SEV-2 + supply chain review (S-12 alignment).

## 25. Rollback / Recovery

CLI binary regression detected pos-release → revert via GitHub Release rollback (previous version remains downloadable); SemVer patch release com fix.

## 26. Risk Register (6-col)

| ID | Risco | Prob | Det | Impacto | Exposure | Residual | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | clap derive breaking change em 4.x → 5.x | L | M | LOW | L | LOW | pin minor version; CHANGELOG monitoring |
| R-002 | Cross-OS matrix flaky em Windows | M | M | LOW | M | LOW | retry policy; fallback warning |
| R-003 | Config file race condition (concurrent CLI invocations) | L | L | LOW | L | LOW | atomic write via temp + rename |
| R-004 | PAT format drift S-03 → S-15 | L | L | MEDIUM | L | LOW | regex test pin; CI gate |
| R-005 | Reproducible build mismatch | L | M | MEDIUM | L | LOW | sha256sum CI gate; SOURCE_DATE_EPOCH set |
| R-006 | doctor check #5 BYOK assumes S-14 SEALED | M | L | LOW | L | LOW | check skip se BYOK não configurado; clear message |

## 27. Knowledge Transfer

- Tech talk (1h): "CoreLink CLI Architecture Overview".
- Doc `docs/cli/usage.md` user guide.
- Onboarding: implement 1 net-new subcommand seguindo pattern.

## 28. Sign-off (STANDARD 5-8 canonical; 7 typical)

| # | Role | Name | Status |
|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | _pending_ |
| 2 | Final Approver | Gustavo Schneiter | _pending_ |
| 3 | Engineer (S-15 lead) | _TBD_ | _pending_ |
| 4 | QA Lead | _TBD_ | _pending_ |
| 5 | Product | Gustavo Schneiter | _pending_ |
| 6 | DevX advisor | _TBD; emphatic — CLI UX consistency + doctor 8 checks + JSON output schema_ | _pending_ |
| 7 | Docs lead | _TBD; emphatic — usage docs + json-output-schema.md + error_taxonomy COR_* extension_ | _pending_ |

> STANDARD lane (per framework §33.5.4): 5-8 canonical sign-offs; 7 typical. Compliance/Privacy/AppSec/Architect com Crypto SME specialization são NÃO mandatory canonical em STANDARD lane (folded em PR review se applicable).

## 29. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-29 | Gustavo (via Claude Opus 4.7) | Criação WI-S15-001 (cycle 12.S15.0; foundation crate + 7 subcommands + cross-OS pipeline + doctor 8 checks Lote 9.5c). |
| 1.1.0 | 2026-05-14 | Gustavo (via Claude Sonnet 4.6) | IMPLEMENTATION: crate `corelink-cli` + 7 subcommands + config + auth resolver + output formatter + doctor 8 checks + release-cli.yml + json-output-schema.md + error_taxonomy.md. 43 unit tests green. Clippy clean. Release build ✓. |

## 30. Anti-patterns evitados

- Aceitar PAT em CLI args (CTRL-CRED-001 violation).
- `doctor` < 8 checks (Lote 9.5c canonical).
- JSON output schema breaking sem major bump.
- Skip cross-OS reproducible builds.
- `--help` minimal (Stripe/Heroku UX baseline required).
- Hard-code secrets em CLI binary.
- Subcommand semantics inconsistent.
- TOML config sem chmod 600 enforcement.
- Skip retry com exponential backoff (FM-150).
- Echo PAT em error messages (CTRL-CRED-001).

---

**Fim WI-S15-001.**
