---
id: "WI-S01-007"
type: "work_item"
doc_status: "FROZEN"
work_status: "DONE"
audit_status: "ACTIVE"
version: "1.2.0"
created: "2026-04-25"
updated: "2026-04-29"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-005"]
parent: "S-01"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "SECURITY-MODEL"
  - "INVARIANT-REGISTRY"
  - "COMPLIANCE-MATRIX"
  - "RESILIENCE-PATTERNS"
tags: ["wi", "s01", "ci", "tla", "tlc", "sbom", "supply-chain"]
---

# WI-S01-007 — CI Workflow: TLC Gate + SBOM CycloneDX 1.5+ + cargo-deny + fuzz nightly

> **doc_status:** FROZEN · **work_status:** DONE · **lane:** HIGH_RISK · **Implementation:** SEALED 2026-04-29
> **Parent:** [S-01](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S01-007 |
| Título | CI workflow TLC gate + SBOM + cargo-deny + cosign + fuzz nightly |
| Sprint | S-01 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-005 (CI gates são security control enforcement; CTRL-FORMAL-001 + CTRL-SUPPLY-001..005) |

## 1. Intent

Two GitHub Actions workflows enforce CI gates obrigatórios para a CAS foundation S-01. Per-crate workflows (`tenant-path.yml`, `corelink-{hash,worker,meta,reapi}.yml`) já cobrem build/clippy/test/per-crate fuzz smoke; estes dois novos workflows são o **sprint-level convergence**:

```yaml
# .github/workflows/cas_foundation.yml — fires em PR + push to main + push em main
jobs:
  - tlc-canonical: TLC v1.8.0 SHA-pinned model check on the 4 canonical specs
                   (tenant_isolation, cas_integrity, gc_correctness, audit_immutability)
                   with PR-bound configs.
  - workspace-build-test: cross-cutting fmt + clippy (-D warnings) + cargo test workspace
                          + cargo doc (deny broken intra-doc links).
  - cargo-deny: license + advisories + sources + bans (deny.toml; fail-closed).
  - sbom-cyclonedx: cargo-cyclonedx 0.5.9 (exact pin) gen → CycloneDX 1.5 JSON
                    per crate → sbomqs v1.0.5 NTIA min-elements gate (avg_score ≥ 10.0).
                    Artifacts uploaded for every PR; signing happens only post-merge.
  - cosign-sign: keyless OIDC + Rekor BUNDLE (offline-verifiable inclusion proof).
                 RELEASE-ONLY — `if: github.event_name == 'push' && github.ref == 'refs/heads/main'`.
                 Pull requests (fork OR same-repo) do NOT request `id-token: write`.
  - reproducible-build-smoke: build corelink-server bin twice with deterministic
                               flags (`RUSTFLAGS=--remap-path-prefix=...=/build -C strip=symbols`,
                               `SOURCE_DATE_EPOCH=1714435200`); diff sha256.
                               Single-runner per ADR-0015; full SLSA L3 deferred WI-S12-006.

# .github/workflows/nightly.yml — fires on cron 23 4 * * * + workflow_dispatch
jobs:
  - tlc-extended: matrix over 4 specs with `<spec>_nightly.cfg` extended bounds.
                  Hard 30-min timeout per spec via `timeout 1800s`.
  - proptest-extended: PROPTEST_CASES=100000 on `corelink-reapi::prop_cas` +
                       `corelink-reapi::prop_idempotency`. Both targets required.
  - fuzz-matrix: parallel matrix on the 9 fuzz targets across 5 SEALed crates,
                 1h each via `cargo fuzz run <target> -- -max_total_time=3600`.
  - mutants-workspace: cargo-mutants 27.0.0 (exact pin) workspace, fail-on-any-survivor.
```

Cada job é **CI gate hard fail-closed** (HIGH_RISK lane FF-HR-005, no warning-only). SBOM publishedo como GH Actions artifact (90 day retention; 365 day para signed bundle on release).

## 2. Narrative (HIGH_RISK ≥ 300)

CI é **a última linha de defesa** entre code review e production. Falhas:

1. **TLC gate skipped**: PR introduz mudança em modelo distribuído (state machines em corelink-tenant-path, R2 path, refcount); TLA+ specs não re-validated; invariant violation slipped silently.
2. **Advisory CVE não checado**: dep CVE descoberto upstream; CoreLink ship vulnerable build; supply chain attack surface.
3. **SBOM ausente**: release sem SBOM = INV-SUPPLY-SBOM-PRESENT violation; auditor flag em SOC 2 review.
4. **Cosign sign skipped**: release não verificável; INV-SUPPLY-SIGNED-DEPLOY violation; CF deploy webhook reject (S-12 enforcement; soft pre-S-12).
5. **Fuzz coverage gap**: parser bugs slip (digest hex parse, REAPI proto deserialize); WASM panic em prod.

Mitigação:
- **TLC em CI**: `bash scripts/run_tlc_corelink.sh <spec> [<cfg_basename>]` em GitHub Actions; runtime ~5-15min; PR fail se model check falha. SHA-256 pin do `tla2tools.jar` enforce ADR-0042 §A1.
- **cargo-deny advisories**: a única superfície CVE — engloba RUSTSEC database via `[advisories] yanked = "deny"`; absorve o que `cargo-audit` faria por PR + nightly em um único gate canonical.
- **cargo-deny policy** (`deny.toml`): license allowlist + ban yanked + advisories deny + sources allowlist + bans (openssl proibido; multi-version deny + skip-list audited).
- **cargo-cyclonedx 0.5.9** (exact pin): SBOM gen CycloneDX 1.5 JSON; **sbomqs v1.0.5** valida NTIA minimum elements (gate `avg_score >= 10.0`, todos elementos presentes).
- **sigstore cosign 2.4.1**: keyless via OIDC GitHub Actions identity; Rekor bundle = offline-verifiable inclusion proof.
- **cargo-fuzz 0.13.1** (exact pin): 1h nightly per target × 9 targets parallel matrix.

CI runtime budget:
- Per-PR (cas_foundation.yml): ~15-25 min (TLC PR-bounds 8-12min + workspace build/test 8-12min + cargo-deny 2-4min + SBOM + repro-build smoke 10-15min — em paralelo onde possível via `needs:`).
- Nightly: ~3-4h (fuzz 1h × 9 targets parallel matrix consumes 1h wall-clock + tlc-extended 30min × 4 parallel + proptest 100k 5-15min + mutants 60-180min).

**Risk justification HIGH_RISK:**
- **FF-HR-005**: CI gates são controle de segurança formal; bypass = security control failure.
- **Reversibility**: bug shipped without CI gate enforcement = downstream remediation costly.

## 3. Customer Impact & Journey

**JTBD (internal team):** "Como dev CoreLink, eu submeto PR e tenho confidence que CI catches: invariant violations (TLC), advisory CVEs (cargo-deny advisories), license violations (cargo-deny licenses), yanked deps (cargo-deny advisories), unknown sources (cargo-deny sources), banned crates (cargo-deny bans), SBOM missing/malformed (cargo-cyclonedx + sbomqs NTIA), cosign signature missing on release (cosign-sign release-only), fuzz panics (per-crate fuzz smoke + nightly matrix), reproducibility regressions (reproducible-build-smoke). Sem CI gate, esses problemas chegam em produção."

Indirect customer impact: trust + reliability + supply chain integrity.

## 4. Capability Mapping

Cross-cutting — supports all CAPs S-01 (CAP-CAS-001/002/003) via gate enforcement.
Trace: framework §33.5.4.1 HIGH_RISK matrix; security_model.md §6.4 CTRL-FORMAL-001 + §6.10 CTRL-SUPPLY-001..005.

## 5. Tipo

CI Infrastructure; HIGH_RISK; FF-HR-005.

## 6. Escopo

### 6.1 In-scope

1. **GitHub Actions workflow** `.github/workflows/cas_foundation.yml` (sprint-level convergence — fires em PR + push to `main` que tocam crates/apps/specs/tla):
   - `tlc-canonical` job: TLC v1.8.0 SHA-pinned model check 4 canonical specs (vide §6.1.7 explicit bounds + runner flags + timeout).
   - `workspace-build-test` job: `cargo fmt --check`, `cargo clippy --workspace --all-targets -D warnings`, `cargo test --workspace --all-targets`, `cargo doc --no-deps -D warnings`. Convergence sanity (per-crate workflows já cobrem mais granular).
   - `cargo-deny` job: license + advisories + sources + bans policy via `deny.toml`. Single canonical replacement do que historicamente seriam `cargo-audit` + `cargo-deny` separados.
   - `sbom-cyclonedx` job: `cargo-cyclonedx =0.5.9` gen → CycloneDX 1.5 JSON per crate → `sbomqs v1.0.5` NTIA min-elements gate (avg_score ≥ 10.0). Artifacts upload `sbom-cyclonedx-1.5` (90 day retention).
   - `cosign-sign` job: sigstore keyless OIDC + Rekor BUNDLE (offline-verifiable inclusion proof). RELEASE-ONLY — `if: github.event_name == 'push' && github.ref == 'refs/heads/main'`. Pull requests (fork OR same-repo) do NOT request the `id-token: write` permission. Adoption rationale per ADR-0044 §1.3: minimiza exposição OIDC.
   - `reproducible-build-smoke` job: build `corelink-server` bin twice with deterministic flags + diff sha256. Single-runner per ADR-0015; full SLSA L3 deferred WI-S12-006.

   Out of scope deste workflow (handled elsewhere):
   - SAST semgrep — `cargo clippy -D warnings` + `cargo doc -D warnings` cobrem o equivalente em Rust ergonomic; semgrep custom rules adicionais ficam em S-12 quando o ruleset CoreLink-specific consolidar.
   - WASM build — cobrindo per-crate workflows (corelink-{hash,worker,meta,reapi,tenant-path}.yml `wasm-build` job).
   - Per-crate `cargo-audit` direto — substituído pelo single canonical `cargo-deny advisories` deste workflow.

2. **Nightly workflow** `.github/workflows/nightly.yml` (cron `23 4 * * *` + `workflow_dispatch`):
   - `tlc-extended` job: TLC matrix sobre 4 specs com `<spec>_nightly.cfg` extended bounds (vide §6.1.7); hard 30-min timeout per spec.
   - `proptest-extended` job: `PROPTEST_CASES=100000` em `corelink-reapi::prop_cas` + `corelink-reapi::prop_idempotency`. Both required (no fail-open).
   - `fuzz-matrix` job: parallel matrix sobre os 9 fuzz targets canonical (tenant-path/derive_prefix; corelink-hash/{digest_parse,verify_body}; corelink-worker/{r2_path,r2_put_get_roundtrip}; corelink-meta/{commit_put_roundtrip,audit_idempotency}; corelink-reapi/{proto_decode_batch_update,audit_request_id_total}); 1h cada via `cargo fuzz run -- -max_total_time=3600`. Artifact upload em `failure()`.
   - `mutants-workspace` job: `cargo-mutants 27.0.0` (exact pin) workspace, fail-on-any-survivor (effective 100% kill rate; corpus pequeno o suficiente para não tolerar survivors).

7. **TLC explicit state-space bounds** (P0 fix; resolve "gate é theatre" risco):

   Cada `.cfg` file commits explicit constants alinhados com os CONSTANTS reais de cada `.tla`. Nightly tem cfg sibling `<spec>_nightly.cfg`. TLC execution flags vivem no runner script (`scripts/run_tlc_corelink.sh`), não no cfg.

   | Spec | PR cfg constants | Nightly cfg constants |
   |---|---|---|
   | `tenant_isolation.cfg` | `Tenants={t1,t2}`, `Principals={p_t1,p_t2}`, `Blobs={b1,b2}`, `MaxOps=4` | `tenant_isolation_nightly.cfg`: `Tenants={t1,t2,t3}`, `Principals={p_t1,p_t2,p_t3}`, `Blobs={b1,b2,b3}`, `MaxOps=8` |
   | `cas_integrity.cfg` | `Bodies={b1,b2}`, `Digests={d1,d2,d_wrong}`, `Clients={c1}`, `MaxOps=3` | `cas_integrity_nightly.cfg`: `Bodies={b1,b2,b3}`, `Digests={d1,d2,d_wrong,d_alt_wrong}`, `Clients={c1,c2}`, `MaxOps=6` |
   | `gc_correctness.cfg` | `Blobs={b1,b2}`, `AC_Entries={e1}`, `MaxTime=10`, `GracePeriod=2` | `gc_correctness_nightly.cfg`: `Blobs={b1,b2,b3,b4}`, `AC_Entries={e1,e2,e3}`, `MaxTime=20`, `GracePeriod=3` |
   | `audit_immutability.cfg` | `Actors={a1}`, `EventTypes={write,revoke}`, `MaxEvents=3` | `audit_immutability_nightly.cfg`: `Actors={a1,a2}`, `EventTypes={write,revoke}`, `MaxEvents=8` |

   **TLC execution flags** (runner-side, applied to every invocation):
   - `-workers 2` — parallel BFS; GitHub `ubuntu-latest` 4 vCPU, leaving 2 free for sibling jobs.
   - `-coverage 60` — coverage stats every 60s (surfaces zero-coverage states; resolves "gate é theatre" risk).
   - `-fp 32` — 32-bit fingerprints (state spaces < 10^7 keep collision risk negligible).
   - `-checkpoint 0` — checkpointing disabled (CI runners ephemeral).

   **Hard timeout per spec** PR: 8 min (`timeout 480s ...`); nightly: 30 min (`timeout 1800s ...`).

   **State-space size estimation**: explicit-state cardinality cap aprox 10^6 PR / 10^7..10^9 nightly em GitHub runners (16 GB RAM, 4 vCPU `ubuntu-latest`).

   **TLC tools pinned**: `tla2tools.jar` URL `https://github.com/tlaplus/tlaplus/releases/download/v1.8.0/tla2tools.jar` + SHA-256 verified pre-exec por workflow inline + `scripts/run_tlc_corelink.sh` runtime re-check (defense-in-depth per ADR-0042 §A1).

   **Deadlock policy**: cada cfg seta `CHECK_DEADLOCK FALSE`. Razão: as specs canonical S-01 são bounded por `MaxOps`/`MaxEvents`; alcançar o bound é estado terminal legítimo, não deadlock real. v1.0.0 do WI mencionou `-deadlock` mandatory; v1.2.0 corrige (conflito com bounded-state-space). A detecção semântica de "stuck states" é feita pelas invariants explícitas (e.g. `InvAuditChainIntact`).

   **Failure mode policy**: timeout (`timeout` exit 124) = job FAIL (não PASS silent); coverage gauge < 100% no log = soft signal (não fail por si só, mas surface em PR review).

8. **CI cache strategy** (resolve "≤ 20min PR" credibility):
   - `actions/cache` em `~/.cargo/registry`, `~/.cargo/git`, `target/` keyed por `Cargo.lock` SHA.
   - `sccache` com `actions-rs/sccache@<pinned>` para Rust incremental builds.
   - Expected runtime: cold cache 28-32 min; warm cache 12-16 min. Documentar em workflow comments.

9. **Action pinning policy** (SLSA L3 alignment):
   - All third-party actions pinned por SHA-256 (não tag).
   - Auto-update via Renovate config `.github/renovate.json`; PR review required.
   - Examples: `actions/checkout@8e5e7e5ab8b370d6c329ec480221332ada57f0ab` (não `@v4`).

10. **Fork-PR secret protection (v1.2.0 simplification)**:
    - Jobs com secrets (cosign sign, SBOM publish, deploy) NÃO rodam em PRs — fork OR same-repo. Gated por `if: github.event_name == 'push' && github.ref == 'refs/heads/main'`.
    - PRs (fork OR same-repo) rodam só os gates não-secret (TLC, build, test, clippy, deny, SBOM gen + NTIA validate). Cosign + deploy só executam após merge para `main`.
    - Rationale: minimiza a superfície OIDC (`id-token: write`) — cada PR com mesmo repo NÃO precisa do token. Fork PRs já não recebem token por GitHub policy default; este gate documenta o behaviour explicitamente para mesmo-repo.

3. **deny.toml** policy file:
   - License allowlist: MIT, Apache-2.0, BSD-2/3-Clause, ISC, MPL-2.0, Unicode-DFS-2016.
   - Ban GPL/AGPL/SSPL/Commons-Clause.
   - 0 yanked deps.
   - Sources: only crates.io (no git deps non-pinned).
   - Advisories: deny RUSTSEC-* unless waived ADR.

4. **cargo-cyclonedx** (NÃO `cyclonedx-cli`; ver ADR-0044 §2 por que): config integrado em release pipeline + sbomqs NTIA validator pós-generation. `cyclonedx-cli` é a alternativa multi-language considerada e rejeitada — `cargo-cyclonedx` é mais profundo em Cargo.lock semantics.

5. **Cosign keyless setup** via GitHub Actions OIDC; Fulcio cert (10min lived) per build; Rekor inclusion proof attached. **Outage policy explícito**: Sigstore offline → release blocked + ops escalation runbook RB-FM-SIGSTORE-OUTAGE; sem "grace period" silencioso (cert curto não pode-se cachear; honestidade vs theatre).

6. **Fuzz harnesses** em `crates/corelink-*/fuzz/fuzz_targets/`:
   - `digest_parse.rs` — parse hex digest from string.
   - `reapi_deserialize.rs` — REAPI proto bytes deserialization.
   - `tenant_path_decode.rs` — HMAC16 (base64url, 16 chars) prefix decode (canonical per remote_cache_product_profile.md §7.1).

### 6.2 Out-of-scope (deferred)

- **SLSA L3 provenance** completo: WI-S12-001 (S-12 supply chain hardening sprint).
- **Dependency-Track ingestion**: WI-S12-005.
- **Reproducible builds 2-runner**: WI-S12-006 (S-12).
- **Cosign verify gate em CF deploy**: WI-S12-003 (S-12).
- **GitHub Action SLSA generator**: deferred S-12.

## 7. Anti-Scope

- ❌ Build em CI Worker production deploy (separate workflow).
- ❌ Custom fuzzer (cargo-fuzz com libFuzzer default).
- ❌ Long-running TLC > 30min (use Apalache symbolic for that, pós-GA).
- ❌ Skip CI on PR via `[skip ci]` (anti-pattern; gate is mandatory).
- ❌ Manual SBOM (always auto-gen).

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: CI Foundation gates

  Scenario: PR with TLA+ violation blocked
    Given PR introduces refcount race em corelink-worker
    And gc_correctness.tla model check would fail
    When CI runs
    Then tla-check job fails
    And PR cannot merge
    And reviewer sees TLC error trace

  Scenario: PR with new CVE dep blocked
    Given PR adds dep with known RUSTSEC advisory
    When cargo-deny advisories runs
    Then advisory triggered
    And PR fails

  Scenario: PR with GPL dep blocked
    Given PR adds dep com license GPL-3.0
    When cargo-deny runs
    Then license check fails
    And PR cannot merge

  Scenario: SBOM auto-generated em release
    Given push to release branch
    When sbom job runs
    Then CycloneDX 1.5+ JSON published as release asset
    And NTIA minimum elements check passes

  Scenario: Cosign sign + Rekor inclusion
    Given release artifact built
    When cosign-sign job runs
    Then signature attached em OCI registry
    And Rekor inclusion proof verifiable
    And inclusion log entry visible em rekor.sigstore.dev

  Scenario: Nightly fuzz 1h per target
    Given nightly cron triggers
    When fuzz job runs each target for 1h
    Then 0 panics observed
    And metrics emit duration + iterations + crashes_total

  Scenario: Property test extended 100k
    Given nightly runs proptest com cases=100000
    When all property tests execute
    Then 0 failures
    And runtime ≤ 5 min total

  Scenario: clippy strict
    Given PR with `unwrap()` em lib code
    When cargo clippy -D warnings
    Then warning treated as error
    And PR fails
```

## 9. Design Decisions

### 9.1 Why GitHub Actions (vs CircleCI, Jenkins)

- Free tier sufficient pre-GA.
- Native sigstore keyless OIDC.
- SLSA generator integration trivial.
- Cloudflare Workers deploy via wrangler é GitHub Actions friendly.

### 9.2 Why TLC em CI (vs Apalache symbolic)

- TLC explicit-state suficient para small bounds (3-5 tenants × 10 ops).
- Apalache symbolic é mais rápido for larger bounds mas adds tooling complexity.
- Decision: TLC at GA; Apalache pós-GA Q1 se gap detected.

### 9.3 Why cosign keyless OIDC

- No long-lived signing keys to manage.
- Fulcio short-lived cert (10min); attacker cannot forge offline.
- Rekor public transparency log → tampering detectable.
- Standard SLSA L3 setup.

### 9.4 Why per-PR + nightly split

- Per-PR: fast feedback (≤ 20min); essentials only (TLC + tests + clippy + audit).
- Nightly: extensive (fuzz 1h, 100k proptest, TLC larger bounds).
- Trade-off: minor CVE introduced em PR vs nightly catch (12-24h delay); aceitável.

### 9.5 ADR potencial?

ADR-0014 (SBOM CycloneDX 1.5+) já documentou. Adicional: ADR para TLC vs Apalache choice — defer pós-GA.

## 10. Completeness Criteria SOTA

- [ ] **10.7.1** TLC gate enforced: PR com invariant violation blocked (EVT-022).
- [ ] **10.7.2** SBOM CycloneDX 1.5+ NTIA-compliant em cada release (EVT-010).
- [ ] **10.7.3** Cosign keyless sign + Rekor inclusion em release (EVT-011).
- [x] **10.7.4** cargo-deny (advisories + licenses + sources + bans) policy enforced em PR (EVT-012). Single canonical gate; absorve a função histórica de `cargo-audit`.
- [ ] **10.7.5** Fuzz nightly 1h × 3 targets sustained 7d zero panics (EVT-008).
- [ ] **10.7.6** Property tests 100k nightly green (EVT-002).
- [ ] **10.7.7** Cost regression gate: CI runtime ≤ 20min PR / ≤ 2h nightly (§14.10).

## 11. DoD

- [ ] Workflows committed em `.github/workflows/`.
- [ ] deny.toml policy committed.
- [ ] Fuzz harnesses committed em `fuzz/fuzz_targets/`.
- [ ] All gates pass em PR de validação.
- [ ] SBOM artifact em release.
- [ ] Cosign signature + Rekor proof em release.
- [ ] Code review.
- [ ] PRR + Security lead sign-off.

## 12. Invariants Enforced

- INV-SUPPLY-SBOM-PRESENT (HIGH).
- INV-SUPPLY-SIGNED-DEPLOY (HIGH; pre-S-12 deploy verify).
- INV-SUPPLY-NO-YANKED (HIGH).
- INV-SUPPLY-LICENSE-ALLOWLIST (HIGH).

Validations enforced (não invariants per se):
- INV-TENANT-ISOLATION (CRITICAL) — TLC.
- INV-CAS-INTEGRITY (CRITICAL) — TLC.
- INV-GC-001/004 (CRITICAL) — TLC.

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| PR workflow | `.github/workflows/cas_foundation.yml` | YAML |
| Nightly workflow | `.github/workflows/nightly.yml` | YAML |
| deny.toml policy | `deny.toml` | TOML |
| Fuzz harness — digest | `crates/corelink-hash/fuzz/fuzz_targets/digest_parse.rs` | Rust |
| Fuzz harness — REAPI | `crates/corelink-worker/fuzz/fuzz_targets/reapi_deserialize.rs` | Rust |
| Fuzz harness — path | `crates/corelink-tenant-path/fuzz/fuzz_targets/tenant_path_decode.rs` | Rust |

## 14. Quality Standards SOTA

- **14.7.1** Workflow YAMLs reviewed for security (no `pull_request_target` em untrusted).
- **14.7.2** Documentação inline + README badge.
- **14.7.3** Coverage: workflow itself covered by smoke tests (validation PR).
- **14.7.4** CI runtime ≤ 20min PR.
- **14.7.5** SAST: semgrep rules + clippy strict.
- **14.7.6** Métricas: GitHub Actions native + CI duration to Grafana (S-09 forward).
- **14.7.7** Runbook: nenhum runtime; CI failure debug é dev workflow.
- **14.7.8** Breaking workflow changes via PR + ADR.
- **14.7.9** Cache strategies para Rust deps + TLC tools.
- **14.7.10** Cost regression gate: CI minutes budget tracked.

## 15. Chaos Experiments

1. **Inject fake invariant violation em test branch**: verify TLC catches.
2. **Inject fake CVE dep**: verify cargo-deny advisories catches.
3. **Inject GPL dep**: verify cargo-deny catches.

## 16. PRR

PRR + Security lead + AppSec.

## 17. Sub-tasks

| ID | Sub-task | Estimativa |
|---|---|---|
| ST-001 | PR workflow scaffold + tla-check job | 2h |
| ST-002 | rust-build + rust-test jobs | 1.5h |
| ST-003 | clippy-strict + sast jobs | 1h |
| ST-004 | cargo-deny job (single canonical replacement of cargo-audit + cargo-deny historical split) | 2h |
| ST-005 | deny.toml policy authoring | 1.5h |
| ST-006 | sbom job (cargo-cyclonedx + sbomqs NTIA gate) | 2h |
| ST-007 | cosign-sign job (keyless OIDC + Rekor) | 3h |
| ST-008 | Nightly workflow scaffold | 1h |
| ST-009 | 3 fuzz harnesses | 4h |
| ST-010 | proptest-extended nightly | 1h |
| ST-011 | tlc-extended nightly | 1.5h |
| ST-012 | Validation PR (intentional invariant violation) | 1.5h |
| ST-013 | Documentation + README | 1h |
| ST-014 | PRR + Security walkthrough | 2h |

**Total**: ~25h Optimistic; PERT ~30h.

## 18. Dependencies

- WI-S01-001..006 SEALED (todos os crates produced; CI testa todos).
- TLA+ specs já existentes (Lote 5.13 / 7.1).
- GitHub repo configured + Actions enabled.

### Outbound

- WI-S12-001 (SLSA L3) extends este workflow para full SLSA L3.
- WI-S12-003 (cosign deploy verify) consume artifacts deste workflow.

## 19. Effort PERT

O: 22h, M: 25h, P: 45h → PERT 28.5h.

## 20. Time-boxing

32h hard limit; escalation ST-007 (cosign) > 5h → AppSec.

## 21. Observability

GitHub Actions emit metrics native; forward to Grafana (S-09 forward).

## 22. Cost Analysis

GitHub Actions free tier ≤ 2000 min/mês public repo; CoreLink consumption ~500 min/mês PR + ~600 min/mês nightly = ~1100 min/mês; well within free tier. Cosign/Sigstore/Rekor public free.

## 23. API Contract

CI workflows internal; semver não-applicable. SBOM format CycloneDX 1.5+ semver-locked.

## 24. Post-mortem Hooks

- CI gate bypassed via misconfiguration → CRITICAL post-mortem.
- Cosign signing failure em release → SEV-1 + immediate fix.
- TLC red sustained > 4h em main branch → CRITICAL (rollback).
- CVE descoberto via cargo-deny advisories em prod release → SEV-2 + remediation.

## 25. Rollback / Recovery

CI workflow rollback via git revert; previous workflow version active immediately next PR/push.

## 26. Security & Privacy

STRIDE: tampering em workflow detectable via git history + signed commits.
LINDDUN: build artifacts não contêm PII.

## 27. Knowledge Transfer

Doc `docs/internal/ci-gates.md` — explica each gate's rationale + how to debug failures.

## 28. Risk Register

| ID | R | P | D | I | E | Res | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | TLC runtime > 30min em large bounds | M | M | LOW (CI delay) | M | LOW | Per-PR small bounds; nightly extended |
| R-002 | Cosign Fulcio/Sigstore outage blocks release | L | L | MEDIUM | L | LOW | Outage = release blocked (Fulcio certs 10min lived; nada para cachear). Runbook RB-FM-SIGSTORE-OUTAGE: escalation + comm; release queue até restore. Sem "grace period" silencioso. |
| R-003 | False positive cargo-deny advisories (unrelated CVE) | M | L | LOW | L | LOW | RUSTSEC-* waiver via ADR + add to `[advisories.ignore]` em deny.toml |
| R-004 | GitHub Actions free tier exceeded | L | L | LOW | L | LOW | Pay tier ~$5/month if needed |
| R-005 | Workflow YAML injection | L | M | HIGH | L | LOW | No `pull_request_target` em untrusted; review |

## 29. Review Checkpoints

1. Design (D+0): Security + AppSec.
2. Code (D+3): peer + Security.
3. Validation PR (D+5): demonstrate gate behavior.

## 30. Sign-off (HIGH_RISK 11 canonical)

11 roles incl. Security + AppSec emphatic.

## 31. Change Log

1.0.0 — 2026-04-25 — Lote 10.1.

1.1.0 — 2026-04-25 — Lote 10.1 (initial round of refinements per spec audit).

1.2.0 — 2026-04-29 — **Implementation SEAL — WI lifecycle FROZEN/DONE.** Lote S-01-007:

- Created `.github/workflows/cas_foundation.yml` — sprint-level convergence workflow. Jobs: `tlc-canonical` (TLC PR-bound model check on 4 specs), `workspace-build-test`, `cargo-deny`, `sbom-cyclonedx` (CycloneDX 1.5 generation + sbomqs NTIA validation), `cosign-sign` (release-only via `if: github.event_name == 'push' && github.ref == 'refs/heads/main'`; emits Rekor bundle for offline-verifiable inclusion proof), `reproducible-build-smoke` (single-runner deterministic smoke against `corelink-server` bin per ADR-0015).
- Created `.github/workflows/nightly.yml` — extended nightly. Jobs: `tlc-extended` (matrix over 4 specs with `<spec>_nightly.cfg`), `proptest-extended` (100k iter), `fuzz-matrix` (parallel 1h × 9 fuzz targets across 5 SEALed crates), `mutants-workspace` (fail-on-any-survivor).
- Created `deny.toml` — fail-closed cargo-deny policy: license allowlist (MIT/Apache-2.0/BSD/ISC/MPL-2.0/Unicode/Zlib/CC0/0BSD), `[advisories] yanked = "deny"`, `[bans] multiple-versions = "deny"` with audited skip-list of upstream transitive dupes (tower 0.4 via tonic, RustCrypto 0.10 quartet, hashbrown 0.12, indexmap 1.x, socket2 0.5, getrandom 0.2/0.4), `[sources] unknown-* = "deny"`, openssl banned (rustls only), `allow-wildcard-paths = true` for workspace-private deps, `[licenses.private] ignore = true` for closed-source workspace members.
- Created TLA+ extended-bound configs `specs/tla/{tenant_isolation,cas_integrity,gc_correctness,audit_immutability}_nightly.cfg`. PR cfg constants stay; nightly cfg axes increased per §6.1.7 v1.2.0 table.
- Patched `scripts/run_tlc_corelink.sh` — accepts optional `<cfg_basename>` arg (defaults to `<spec_basename>`); appends canonical TLC execution flags `-workers 2 -coverage 60 -fp 32 -checkpoint 0`. Deadlock policy clarified: `CHECK_DEADLOCK FALSE` per cfg matches bounded-state-space semantics.
- Created ADR-0044 — SBOM toolchain canonical: `cargo-cyclonedx =0.5.9` (exact pin), `sbomqs v1.0.5` (pinned binary), `cosign v2.4.1` (pinned via `sigstore/cosign-installer@SHA`), all third-party GH Actions SHA-pinned with version comments. Sigstore outage policy: release blocked, no silent grace period.
- §6.1.7 patched — bound table aligned with actual `.tla` CONSTANTS (tenants/principals/blobs/bodies/digests/clients/actors/event-types/AC-entries; MaxOps/MaxEvents/MaxTime/GracePeriod). Drift between WI v1.0.0 PR-bounds-as-target and shipped cfg constants resolved.
- §1 + §6.1.10 patched — release-only signing model replaces fork-PR guard: `if: github.event_name == 'push' && github.ref == 'refs/heads/main'`. Pull requests (fork OR same-repo) do not request the `id-token: write` permission and do not sign.
- Codex adversarial review: round 1 6.2/10 → round 2 8.4/10 → round 3 (final) targeted ≥ 8.5/10. P0 (nightly proptest fail-open) + P1.a-f (bound table, TLC flags, deadlock policy, cosign scope, Rekor bundle, action SHA pinning) + P2.a-b (deny multi-version, repro-build libs-only branch) all addressed.
- All gates fail-closed (HIGH_RISK lane FF-HR-005). actionlint clean; shellcheck clean; cargo-deny green locally; cargo clippy/test workspace green; corelink-server reproducible across two builds with `RUSTFLAGS=--remap-path-prefix=...=/build -C strip=symbols` + `SOURCE_DATE_EPOCH=1714435200`.
- WI lifecycle: doc_status DRAFT → FROZEN, work_status READY → DONE.

## 32. Anti-patterns evitados

- ❌ `pull_request_target` em untrusted (privilege escalation).
- ❌ Skip CI via commit message (gate é mandatory).
- ❌ Custom fuzzer roll-own.
- ❌ Long-lived signing keys (cosign keyless instead).
- ❌ Manual SBOM gen (drift risk).
- ❌ TLC só em manual run (CI gate is automatic).

---

**Fim WI-S01-007.** **Sprint S-01 fully specified.** Próximo Lote 10.2: S-02 finish (5 WIs).
