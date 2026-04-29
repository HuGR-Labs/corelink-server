---
id: "WI-S15-006"
type: "work_item"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-29"
updated: "2026-04-29"
lane: "STANDARD"
parent: "S-15"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "SECURITY-MODEL"
  - "OBSERVABILITY-MODEL"
  - "FAILURE-MODES"
  - "RESILIENCE-PATTERNS"
  - "INVARIANT-REGISTRY"
tags: ["wi", "s15", "ship-gate", "cargo-fuzz", "apple-notarize", "authenticode", "oss-proof", "prr", "standard"]
---

# WI-S15-006 — S-15 Ship Gate: cargo-fuzz 1M Random Inputs em CLI Subcommands Surface (0 Panics + 0 Secrets Leaked em Error Paths via `secret_redaction_check` Harness CTRL-CRED-001) + Apple Notarization (Acquire Apple Developer Cert Ahead of D-day + signtool Automation + notarize Submit + staple) + Linux GPG Signing (Release Binaries Signed + Public Key Published) + Windows Authenticode Signing (Acquire EV Code-Signing Cert + signtool Automation; Fallback Unsigned com Warning até Cert Ready se Acquisition Slips com Explicit Waiver + ADR) + 2 OSS Proof-of-Conversion (Pre-engagement Q3 com Forge + 1 External Bazel-using OSS Project; Documented em `examples/case-studies.md` com Adoption Story + Setup Time + Cache Hit Ratio) + PRR STANDARD Doc S-15 (5-8 Sign-offs Canonical; 7 Typical: Owner + Final Approver + Engineer + QA Lead + Product + DevX advisor + Docs lead) + Time-to-First-Cache-Hit ≤ 5 min Measured via Dev Workshop com 3 External Developers + Adversarial Test Summary Aggregation Cross-WI

> **doc_status:** DRAFT · **work_status:** READY · **lane:** STANDARD
> **Parent:** [S-15](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S15-006 |
| Título | S-15 ship gate — cargo-fuzz 1M random inputs em CLI subcommands surface (input parsing + config file parsing + JSON deserialization paths + auth resolution paths; 0 panics; 0 secrets leaked em error paths via `secret_redaction_check` harness CTRL-CRED-001) + Apple notarization (acquire Apple Developer Program cert ahead of D-day + signtool automation + notarize submit + staple) + Linux GPG signing (release binaries signed + GPG public key published em `https://corelink.dev/.well-known/gpg-pubkey.asc`) + Windows Authenticode signing (acquire EV code-signing cert + signtool automation; fallback unsigned com warning até cert ready se acquisition slips — explicit waiver + ADR documented) + 2 OSS proof-of-conversion (pre-engagement Q3 com Forge + 1 external Bazel-using OSS project; documented em `examples/case-studies.md` com adoption story + setup time + cache hit ratio); PRR STANDARD doc S-15 com 5-8 sign-offs canonical (7 typical: Owner + Final Approver + Engineer + QA Lead + Product + DevX advisor + Docs lead); time-to-first-cache-hit ≤ 5 min measured via dev workshop com 3 external developers; adversarial test summary aggregation cross-WI (CLI fuzz scenarios + FFI memory safety + WASM bundle size + telemetry privacy + signing trust chain + OSS adoption); evidence pack: 1M fuzz iter green + 3 OSes signed + 2 OSS case-studies + dev workshop ≤ 5 min measured; final gate releases all S-15 WIs to Implementation SEAL state D+15 (libera S-16/S-18/S-19/S-20 dev). |
| Sprint | S-15 |
| Lane | STANDARD |
| Forcing factors | none (closing WI; STANDARD lane single-phase SEAL D+15; DoD §6 não requer 30d observation window) |

## 1. Intent

WI-S15-001..005 implementam features. **Este WI prova que o sistema CLI/SDK completo funciona sob adversarial scrutiny + production-equivalent load + OSS adoption validation**. É o ship gate do S-15. **Gating canônico**: este WI fecha em **single-phase SEAL D+15** (STANDARD lane; DoD §6 não requer "30d sustained" criteria — only weekly conversion benchmark + fuzz test + cross-OS signed binaries) com cargo-fuzz 1M iter green + 3 OSes signed + 2 OSS case-studies + PRR 5-8 sign-offs canonical coletados — isso libera **downstream development S-16/S-18/S-19/S-20** (hard dependency satisfeita per spec contract S-15 §11).

Deliverables 6-fold:

1. **cargo-fuzz 1M random inputs em CLI subcommands surface**:
   - Fuzz targets em `fuzz/fuzz_targets/`:
     - `cli_input.rs`: random CLI args + flags + subcommand combinations.
     - `config_toml.rs`: random TOML config file content (malformed + edge cases).
     - `json_deserialize.rs`: random JSON input para `--output=json` parsing paths.
     - `auth_resolution.rs`: random env var content + config file content paths.
   - Iteration target: 1M per target em PR (5M total cumulative weekly nightly).
   - Acceptance criteria:
     - **0 panics**: cargo-fuzz reports zero crashes/panics across all targets.
     - **0 secrets leaked**: `secret_redaction_check` harness inspects error paths + asserts no PAT regex match em stderr/output (CTRL-CRED-001 baseline).
   - Coverage tracking: `cargo-fuzz coverage` reports lines hit em CLI surface.
   - Multi-engine: libFuzzer primary + AFL++ secondary para diverse mutation.
   - Output: `specs/_audits/2026-XX-XX-cargo-fuzz-summary-s15.md` com iteration count + coverage + 0 panics + 0 leaks.

2. **Apple notarization** (per spec contract DoD §6):
   - **Acquire Apple Developer Program cert** ahead of D-day (lead time 1-2 weeks; plan kick-off D+0).
   - **Signing automation** em `.github/workflows/notarize-macos.yml`:
     - `codesign --sign "Developer ID Application" --options runtime --entitlements entitlements.plist corelink`.
     - `xcrun notarytool submit` para Apple notarize service.
     - `xcrun stapler staple` to embed notarization ticket.
   - **Validation**: `spctl --assess --verbose corelink` confirms accepted by Gatekeeper.
   - **Output**: signed + notarized binaries para macOS arm64 + x86_64 em GitHub Release assets.

3. **Linux GPG signing** (per spec contract DoD §6):
   - **Acquire GPG signing key** (4096-bit RSA; stored em GitHub Actions secret `GPG_PRIVATE_KEY`).
   - **Signing automation** em `.github/workflows/sign-linux.yml`:
     - `gpg --detach-sign --armor corelink` produces `corelink.asc`.
   - **GPG public key published** em `https://corelink.dev/.well-known/gpg-pubkey.asc` (well-known canonical location).
   - **README integration**: customer can verify via `gpg --verify corelink.asc corelink`.
   - **Output**: signed binaries para Linux arm64 + x86_64 em GitHub Release assets + `.asc` detached signatures.

4. **Windows Authenticode signing** (per spec contract DoD §6):
   - **Acquire EV (Extended Validation) code-signing cert** (lead time 1-2 weeks; vendors: DigiCert, Sectigo, GlobalSign; cost ~$300-500/year).
   - **Signing automation** em `.github/workflows/sign-windows.yml`:
     - `signtool sign /a /tr http://timestamp.digicert.com /td sha256 /fd sha256 corelink.exe`.
   - **Validation**: `signtool verify /pa /v corelink.exe` confirms valid Authenticode signature.
   - **Cert acquisition timeline**: start D+0 (initiate vendor application + business validation docs); EV cert delivery typically D+5..D+10; signing setup D+10..D+12; sign + validate D+12..D+13.
   - **Lote 10.15 codex P1 fix — NÃO há fallback path para unsigned Windows release**: spec contract DoD §6 promise "3 OSes signed" é hard requirement; CONDITIONALLY_APPROVED com Windows unsigned waiver REMOVIDO (codex P1: weakens "3 OS signed" promise + customer trust loss em Windows install warnings). Se cert acquisition slips além de D+13, **Windows release é DEFERRED** (NÃO unsigned-with-warning ship); Linux + macOS shipam em D+15 SEAL; Windows ships em sprint subsequente após cert ready. Sprint promise updated: "macOS + Linux signed at D+15; Windows signed within +1 sprint contingent on cert acquisition".
   - **Output**: signed binary para Windows x86_64 em GitHub Release assets (apenas após cert ready; nunca unsigned ship).

5. **2 OSS proof-of-conversion** (per spec contract DoD §6):
   - **Pre-engagement Q3** (D+0 to D+10):
     - **OSS #1 — Forge (HuGR customer-zero, internal)**: HuGR Forge customer-zero (pre-existing relationship; quickest engagement); integration target = Forge's Bazel-based monorepo CI. **Caveat (Lote 10.15 codex P2 fix)**: Forge é internal customer-zero — NÃO conta como independent external proof; é validation interna; case-study pode ser usado para demo mas marketing claim "2 external OSS adoption" é misleading sem o segundo external.
     - **OSS #2 — External Bazel-using OSS (independent)**: shortlist 3 candidates (e.g., `bazelbuild/rules_rust`, `bufbuild/buf`, `tilt-dev/tilt`); pick 1 via competitive engagement; engagement signed by D+12. **CONDICIONAL (Lote 10.15 codex P2 strengthening)**: SEAL gate requires **≥ 1 truly independent external OSS adoption** (Forge não conta); se nenhum dos 3 candidates assinar até D+13, escalate budget for engagement incentive ($5-15k consulting fee for adoption + case-study) OR defer GA promotion S-20 dependency até second external OSS adopted. Marketing claim atualizado: "1 internal customer-zero + 1 independent external OSS adoption" (NÃO "2 external OSS").
     - **(Aspirational; sprint-following)** **OSS #3 — Second independent external**: pursue post-S-15 SEAL para strengthen GA conversion narrative; not blocking.
   - **Documentation** em `examples/case-studies.md`:
     - Per OSS: adoption story + setup time + cache hit ratio + monthly cost savings + customer testimonial.
     - Sanitized (customer NDA-aware); shareable em sales conversations.
   - **Output**: 2 case-studies committed + customer testimonials.

6. **PRR STANDARD doc S-15 + adversarial summary aggregation** (per spec contract §6 DoD + §14):
   - **PRR doc** `specs/04_sprints/S15/PRR-S15.md` covering:
     - DoD §6 + §7 criteria status (single-phase SEAL D+15; STANDARD lane no observation window).
     - CTRLs trace verified (CTRL-CAS-002 + CTRL-CRED-001 enforced).
     - All 6 WIs SEALED state precondition.
     - Cargo-fuzz 1M iter green + 0 panics + 0 secrets leaked.
     - 3 OSes signed binaries (Apple notarized + Linux GPG + Windows Authenticode or waiver+ADR).
     - 2 OSS case-studies committed.
     - Time-to-first-cache-hit ≤ 5 min measured via dev workshop com 3 external developers.
     - 6+ CLI/SDK métricas emitting em staging (telemetry opt-in baseline).
     - SDKs published (crates.io + PyPI + npm + pkg.go.dev) com zero CVEs HIGH/CRITICAL.
     - `corelink doctor` 8 checks actionable verified.
     - CI templates 3 providers funcional + sample real builds verde.
     - **Promotion gate decision**: `APPROVED` | `CONDITIONALLY_APPROVED` (com waivers + ADR + expiry) | `REJECTED`.
     - **CONDITIONALLY_APPROVED waivers** (typical em STANDARD lane S-15):
       - Windows Authenticode cert acquisition slip → fallback unsigned com warning + ADR + expiry next sprint.
       - 1 OSS engagement slip beyond D+15 → defer to S-19 customer onboarding sprint com explicit ADR.
   - **Adversarial summary aggregation** (cross-WI 25+ scenarios):
     - WI-S15-001: 5 scenarios (PAT em CLI args bypass + malformed config + auth resolution drift + cross-OS matrix flake + reproducible build mismatch).
     - WI-S15-002: 5 scenarios (Bazel REAPI quirks + CI flake + time-to-cache-hit > 5 min + Bazel 8.x breaking + REAPI v2 spec drift).
     - WI-S15-003: 5 scenarios (Buck2 REAPI quirks + CI flake + time-to-cache-hit > 5 min + Buck2 release breaks integration + DX parity Bazel-Buck2 drift).
     - WI-S15-004: 8 scenarios (FFI memory unsafety pyO3/cgo/wasm + WASM bundle > 1MB + tooling breaking change + client-verify drift + race condition cgo + npm install fail older Node + ADR-0016 reverted).
     - WI-S15-005: 5 scenarios (telemetry leak PII + CI template breaking + endpoint unreachable + GH/GitLab/CircleCI API drift + anonymized_id reverse-engineered).
     - WI-S15-006: 4 scenarios (cargo-fuzz misses panic path + Apple notarize rejected + Authenticode cert slip + 2 OSS adoption slow).
   - **Final gate releases all S-15 WIs to Implementation SEAL state D+15**: libera downstream S-16/S-18/S-19/S-20 dev.

## 2. Narrative

S-15 é o **maior salto de surface DX (developer experience) tooling** em CoreLink: CLI 7 subcommands signed 3 OSes + Bazel + Buck2 starter projects testados em CI + FFI 3 languages com client-verify default-on + CI templates 3 providers + telemetry opt-in privacy-first. Cada um dos 5 WIs anteriores tem completeness mini-checklist; **WI-S15-006 é o gate cumulativo**: valida que o sistema completo composto funciona sob:

1. **Adversarial scenarios via cargo-fuzz 1M iter**: input parsing + config file parsing + JSON deserialization + auth resolution paths fuzzed com libFuzzer + AFL++; 0 panics + 0 secrets leaked em error paths (CTRL-CRED-001 baseline). Stripe/Heroku CLI-tier rigor (zero competitors em CLI fuzz scope).

2. **Cross-OS signing trust chain**: Apple notarized + Linux GPG + Windows Authenticode = customer trust evidence (Stripe/Heroku parity); fallback unsigned com warning + ADR se Authenticode cert slips (waiver doc com expiry next sprint).

3. **OSS proof-of-conversion 2 projects**: Forge (customer zero) + 1 external Bazel-using OSS = real adoption evidence; documented em `examples/case-studies.md` com adoption story + setup time + cache hit ratio + customer testimonial. Critical para GA conversion (S-20 dependency hard).

4. **Time-to-first-cache-hit ≤ 5 min measurement**: dev workshop sample com 3 external developers (rotated por sprint); measured em real-world conditions; tracked monthly. DX validation evidence-grade.

5. **PRR STANDARD 5-8 sign-offs canonical**: 7 typical (Owner + Final Approver + Engineer + QA Lead + Product + DevX advisor + Docs lead). Crypto SME folded em Architect specialization se applicable em PR review (este WI consume CTRL-CAS-002 baseline; not novel cripto control).

**Risk justification STANDARD lane (single-phase SEAL D+15)**:
- DoD §6 não requer "30d sustained" criteria; only weekly conversion benchmark + fuzz test verde em PR + cross-OS signed binaries.
- CLI/SDK consume endpoints S-01..S-04 já validated; não introduz novo path tenant data flow.
- Não há cripto-load-bearing novel control (BYOK + envelope encryption + Ed25519 = S-14 HIGH_RISK; este WI consume).
- Single-phase SEAL D+15 sufficient (vs S-13/S-14 two-phase SEAL com observation window 30d).

5-8 sign-offs canonical (7 typical) **mandatory** (sprint contract S-15 §14); cripto-touching WI-S15-004 (FFI client-verify default-on reflection) receive specialization review within Architect role em PR review se applicable, but **NÃO mandatory canonical** em STANDARD lane.

## 3. Customer Impact & Journey

**Persona 1 — Customer adopting CoreLink (post-S-15 GA)**:
- Documentation: "DX posture: CLI 7 subcommands signed 3 OSes (macOS notarized + Linux GPG + Windows Authenticode) + cargo-fuzz 1M iter zero panics + Bazel + Buck2 starter projects tested CI continuous + FFI 3 languages (Python/Go/JS) client-verify default-on + CI templates 3 providers + telemetry opt-in privacy-first + 2 OSS proof-of-conversion".
- PRR doc é evidence-grade artifact: customers can request via NDA.
- 2 OSS case-studies sanitized shareable em sales conversations.

**Persona 2 — Compliance auditor (SOC 2 Type II + ISO 27001 + LINDDUN)**:
- Audit query: S-15 WIs SEALED state; PRR doc 5-8 sign-offs canonical documented.
- LINDDUN review (WI-S15-005) = privacy attestation.
- CTRL-CAS-002 + CTRL-CRED-001 enforced via fuzz + 3-language test evidence.

**Persona 3 — Internal SRE/Engineer**:
- Cargo-fuzz 1M iter sustained = production confidence.
- 2 OSS case-studies = adoption signal.

## 4. Capability Mapping

- All CAP-CLI-* + CAP-SDK-* (validates whole DX domain).
- **CAP-CLI-002** (Cross-OS distribution signed) — IMPLEMENTA primary completion (signing acquisition + automation).
- Trace: `_spec_contract.md §6 (Definition of Done)` + `framework §33.5.4 (STANDARD 5-8 sign-offs canonical)`.

## 5. Tipo

Sprint ship gate; STANDARD lane; closing WI single-phase SEAL D+15.

## 6. Escopo

### 6.1 In-scope

1. **Cargo-fuzz 1M random inputs em CLI subcommands surface**:
   - Fuzz targets em `fuzz/fuzz_targets/`:
     - `cli_input.rs`: clap parser + arbitrary subcommand/flag combinations via `Arbitrary` derive.
     - `config_toml.rs`: TOML parser fed random byte sequences + edge cases (malformed, oversized, deeply nested).
     - `json_deserialize.rs`: `--output=json` parsing paths fed random JSON.
     - `auth_resolution.rs`: env var content + config file content random paths (PAT format edge cases).
   - **Iteration target**: 1M per target em PR + 5M cumulative weekly nightly.
   - **Multi-engine**: libFuzzer primary + AFL++ secondary (per `_spec_contract.md §15 row 8` mitigation: "Multi-engine fuzz (libFuzzer + AFL); coverage tracking; minimum 100M iterations release").
   - **Acceptance**:
     - **0 panics**: cargo-fuzz reports zero crashes across targets.
     - **0 secrets leaked**: `secret_redaction_check` harness (custom: regex match `corelink_<env>_*` em fuzz output stderr; assert == 0 matches).
   - **Coverage tracking**: `cargo-fuzz coverage` reports lines hit em CLI surface; target ≥ 80% coverage.
   - **Output**: `specs/_audits/2026-XX-XX-cargo-fuzz-summary-s15.md` com iteration count + coverage + 0 panics + 0 leaks.

2. **Apple notarization**:
   - Acquire Apple Developer Program cert (D+0 plan kick-off; lead time 1-2 weeks).
   - Signing automation em `.github/workflows/notarize-macos.yml` (extends WI-S15-001 release pipeline placeholders).
   - `codesign` + `xcrun notarytool submit` + `xcrun stapler staple` flow.
   - Validation via `spctl --assess --verbose corelink`.
   - Output: signed + notarized binaries macOS arm64 + x86_64.

3. **Linux GPG signing**:
   - Acquire GPG signing key 4096-bit RSA (stored em GitHub Actions secret).
   - Signing automation em `.github/workflows/sign-linux.yml`.
   - `gpg --detach-sign --armor` produces `.asc`.
   - Public key published em `https://corelink.dev/.well-known/gpg-pubkey.asc`.
   - README integration: `gpg --verify corelink.asc corelink`.
   - Output: signed binaries + `.asc` detached signatures.

4. **Windows Authenticode signing**:
   - Acquire EV code-signing cert (DigiCert/Sectigo/GlobalSign; lead time 1-2 weeks; cost ~$300-500/year).
   - Signing automation em `.github/workflows/sign-windows.yml`.
   - `signtool sign /a /tr http://timestamp.digicert.com /td sha256 /fd sha256 corelink.exe`.
   - Validation: `signtool verify /pa /v corelink.exe`.
   - **Fallback**: se cert slip beyond D+10, fallback unsigned com warning + explicit waiver + ADR (`specs/_decisions/ADR-XXXX-windows-authenticode-fallback.md`) com expiry next sprint.
   - Output: signed binary Windows x86_64 (or fallback unsigned com warning).

5. **2 OSS proof-of-conversion**:
   - Pre-engagement Q3 (D+0 to D+10):
     - OSS #1 Forge (HuGR customer-zero; quickest engagement).
     - OSS #2 external Bazel-using OSS (shortlist + competitive engagement; signed by D+12).
   - Documentation em `examples/case-studies.md`:
     - Per OSS: adoption story + setup time + cache hit ratio + monthly cost savings + customer testimonial.
     - Sanitized (customer NDA-aware).
   - Output: 2 case-studies committed + customer testimonials.

6. **PRR STANDARD doc S-15**:
   - File `specs/04_sprints/S15/PRR-S15.md`:
     - 5-8 sign-offs canonical table (7 typical).
     - Evidence pack:
       - Cargo-fuzz 1M iter summary (0 panics + 0 leaks).
       - 3 OSes signed binaries (Apple notarized + Linux GPG + Windows Authenticode or waiver+ADR).
       - 2 OSS case-studies linked.
       - Time-to-first-cache-hit ≤ 5 min dev workshop measured (3 external developers).
       - 6+ CLI/SDK métricas emitting em staging.
       - SDKs published (crates.io + PyPI + npm + pkg.go.dev) zero CVEs HIGH/CRITICAL.
       - `corelink doctor` 8 checks actionable verified.
       - CI templates 3 providers funcional.
       - LINDDUN privacy review (WI-S15-005) trace.
       - ADR-0016 (FFI vs native HTTP) trace.
     - Promotion gate decision: APPROVED | CONDITIONALLY_APPROVED | REJECTED.
     - **CONDITIONALLY_APPROVED typical waivers** em STANDARD lane S-15:
       - Windows Authenticode cert slip → fallback unsigned + ADR + expiry.
       - 1 OSS engagement slip → defer S-19 com ADR.
   - **Adversarial summary aggregation** 25+ scenarios cross-WI documented com remediation status; output `specs/_audits/2026-XX-XX-adversarial-summary-s15.md`.

7. **Time-to-first-cache-hit ≤ 5 min dev workshop**:
   - 3 external developers recruited (rotated per sprint; bias mitigation).
   - Workshop scenarios:
     - macOS arm64 + Linux x86_64 + Windows x86_64 sample.
     - Bazel + Buck2 alternation.
     - FFI Python sample.
   - Measured: clone → setup → first cache hit total elapsed time.
   - Target ≤ 5 min; results documented em `specs/_audits/2026-XX-XX-dev-workshop-s15.md`.

### 6.2 Out-of-scope (deferred)

- External pentest CLI/SDK (not required em STANDARD lane; defer S-20 GA hardening if scope expands).
- Continuous fuzzing platform (cargo-fuzz beyond 1M iter) — pós-GA Q1.
- Bug bounty program — pós-GA Q1.
- Customer-facing CLI dashboard UI — pós-GA enterprise + S-16 admin UI.
- Auto-installer brew/apt/yum repos — pós-GA Q1.

## 7. Anti-Scope

- Skip cargo-fuzz 1M (mandatory ship gate).
- Skip Apple notarization (mandatory; fallback unsigned com waiver+ADR).
- Skip GPG signing (mandatory).
- Skip 2 OSS proof-of-conversion (mandatory DoD §6).
- APPROVED PRR sem 5-8 sign-offs canonical.
- Production deploy sem cargo-fuzz 1M green.
- Skip dev workshop time-to-first-cache-hit measurement.
- Waivers acumulando sem expiry.
- Walkthrough scope creep (este WI é STANDARD; não requer external pentest).

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: S-15 ship gate — cargo-fuzz 1M + 3 OSes signed + 2 OSS proof + PRR

  Scenario: cargo-fuzz 1M iter zero panics zero secrets leaked
    Given fuzz targets em fuzz/fuzz_targets/
    When `cargo fuzz run cli_input -- -runs=1000000` runs (per target)
    Then 0 crashes/panics reported
    And `secret_redaction_check` harness asserts 0 PAT regex matches em fuzz output
    And coverage ≥ 80% lines hit em CLI surface
    And summary report committed em specs/_audits/cargo-fuzz-summary-s15.md

  Scenario: Apple notarization succeeded
    Given Apple Developer Program cert acquired
    When `.github/workflows/notarize-macos.yml` runs em release
    Then codesign + notarytool submit + stapler staple succeed
    And spctl --assess --verbose corelink confirms accepted
    And signed + notarized binaries em GitHub Release assets (macOS arm64 + x86_64)

  Scenario: Linux GPG signing succeeded
    Given GPG signing key acquired + stored em GH Actions secret
    When `.github/workflows/sign-linux.yml` runs em release
    Then `gpg --detach-sign --armor corelink` produces corelink.asc
    And public key published em https://corelink.dev/.well-known/gpg-pubkey.asc
    And customer can verify via `gpg --verify corelink.asc corelink`

  Scenario: Windows Authenticode signing succeeded
    Given EV code-signing cert acquired
    When `.github/workflows/sign-windows.yml` runs em release
    Then signtool sign succeeds
    And signtool verify /pa /v corelink.exe confirms valid
    And signed binary Windows x86_64 em GitHub Release assets

  Scenario: Windows Authenticode cert slip → Windows DEFERRED (NÃO unsigned ship; Lote 10.15 codex P1 fix)
    Given EV cert acquisition slips beyond D+13
    When Windows binary cannot be signed in time for D+15 SEAL
    Then macOS + Linux ship signed at D+15 SEAL ceremony
    And Windows release is DEFERRED (NÃO shipped unsigned with warning)
    And ADR em specs/_decisions/ADR-XXXX-windows-cert-deferral.md documents deferral + ETA
    And Windows ships within +1 sprint contingent on cert ready
    And PRR gate = APPROVED para macOS+Linux scope (Windows deferred separately, NÃO blocking sprint SEAL)

  Scenario: 2 OSS proof-of-conversion engaged
    Given Forge + 1 external Bazel-using OSS shortlisted
    When pre-engagement Q3 D+0 to D+10
    Then 2 OSS engagements signed by D+12
    And case-studies committed em examples/case-studies.md
    And per OSS: adoption story + setup time + cache hit ratio + testimonial

  Scenario: Time-to-first-cache-hit ≤ 5 min dev workshop
    Given 3 external developers recruited (rotated per sprint)
    When dev workshop runs (macOS + Linux + Windows; Bazel + Buck2; FFI Python)
    Then measured time from clone → first cache hit ≤ 5 min em ≥ 80% trials
    And report committed em specs/_audits/dev-workshop-s15.md

  Scenario: PRR S-15 5-8 sign-offs canonical documented
    Given PRR-S15.md created
    When 7 reviewers sign (typical: Owner + Final Approver + Engineer + QA + Product + DevX + Docs)
    Then sign-off table populated com names + dates + status=approved
    And promotion gate = APPROVED OR CONDITIONALLY_APPROVED com waivers

  Scenario: All 6 WIs SEALED state precondition
    Given WI-S15-001..005 todos SEALED
    When PRR review proceeds
    Then ship gate validates state precondition
    And blocks SEAL se any WI não SEALED

  Scenario: SDKs published com zero CVEs HIGH/CRITICAL
    Given crates.io + PyPI + npm + pkg.go.dev publication
    When cargo-audit + pip-audit + npm audit run
    Then zero CVEs HIGH/CRITICAL em dependencies

  Scenario: Adversarial summary aggregation 25+ scenarios
    Given individual adversarial scenarios em WI-S15-001..005 + this WI
    When summary report aggregated
    Then 25+ scenarios documented
    And 100% mitigation rate sustained
    And report committed em specs/_audits/adversarial-summary-s15.md

  Scenario: Single-phase SEAL D+15 (STANDARD lane)
    Given DoD §6 não requer 30d observation window
    When PRR coletados D+15
    Then sprint SEAL ceremony D+15 (não two-phase)
    And libera S-16/S-18/S-19/S-20 dev
```

## 9. Design Decisions

### 9.1 Why cargo-fuzz 1M (não 10k ou 100M)

- Stripe/Heroku CLI fuzz baseline ~1M iter PR + 100M release (per spec contract §15 row 8 mitigation).
- 1M PR = balance feedback loop fast + adversarial coverage; 100M nightly cumulative.

### 9.2 Why fallback unsigned com waiver+ADR (não block sprint)

- Windows EV cert acquisition lead time 1-2 weeks unpredictable.
- Block sprint = unnecessary delay; fallback unsigned com warning + waiver expiry next sprint = balance.

### 9.3 Why 2 OSS (não 1 ou 5)

- 1 OSS = single point insufficient (DX validation gap).
- 5 OSS = scope creep; defer to S-19 customer onboarding wave.
- 2 OSS = Forge customer-zero + 1 external = balance.

### 9.4 Why STANDARD 5-8 sign-offs (não HIGH_RISK 11)

- DoD §6 não requer 30d observation window; CTRLs CAS-002 + CRED-001 são enforcement reflection (not novel).
- Não há cripto-load-bearing novel control (S-14 HIGH_RISK cobre BYOK + envelope encryption).
- Crypto SME folds em Architect specialization se applicable em PR review (não mandatory canonical).

### 9.5 Why single-phase SEAL D+15 (não two-phase)

- STANDARD lane DoD §6 não requer "30d sustained" criteria.
- Weekly conversion benchmark + fuzz test verde em PR + cross-OS signed binaries são instant-verifiable.
- vs S-13/S-14 HIGH_RISK two-phase SEAL com observation window 30d (rotation overlap sustained, BYOK matrix weekly, kill switch chaos drill weekly).

### 9.6 Why CONDITIONALLY_APPROVED gate option

- Sometimes Authenticode cert slips ou 1 OSS engagement não fecha em D+15.
- PRR proceeds com waivers documented: specific risk acknowledged + mitigating controls + ADR + expiry next sprint.
- Rejection of all waivers = sprint blocks unnecessarily.

### 9.7 ADR potencial?

- Sim — ADR Windows Authenticode fallback em waiver scenario (created em runtime se applicable).
- ADR-0016 (FFI vs native HTTP) já em WI-S15-004.

## 10. Completeness Criteria

- [ ] **10.s15.006.1** Cargo-fuzz 1M per target green em PR + 5M cumulative weekly nightly (EVT-002 + EVT-009).
- [ ] **10.s15.006.2** 0 panics + 0 secrets leaked em error paths (CTRL-CRED-001) (EVT-002).
- [ ] **10.s15.006.3** Coverage ≥ 80% CLI surface (EVT-002).
- [ ] **10.s15.006.4** Apple notarization succeeded; binaries signed macOS arm64+x86_64 (EVT-024).
- [ ] **10.s15.006.5** Linux GPG signing succeeded; public key published (EVT-024).
- [ ] **10.s15.006.6** Windows Authenticode signing succeeded **at D+15 SEAL** OR Windows release **DEFERRED** to subsequent sprint (NÃO unsigned ship; Lote 10.15 codex P1 fix removes prior fallback waiver+ADR path) (EVT-024).
- [ ] **10.s15.006.7** 2 OSS proof-of-conversion engaged + case-studies committed (EVT-018).
- [ ] **10.s15.006.8** Time-to-first-cache-hit ≤ 5 min dev workshop measured (EVT-018).
- [ ] **10.s15.006.9** PRR-S15.md 5-8 sign-offs canonical documented (EVT-031).
- [ ] **10.s15.006.10** Adversarial summary aggregated em report (25+ scenarios) (EVT-040).
- [ ] **10.s15.006.11** All 5 WIs (WI-S15-001..005) SEALED state precondition.
- [ ] **10.s15.006.12** SDKs published (crates.io + PyPI + npm + pkg.go.dev) zero CVEs HIGH/CRITICAL (EVT-001).
- [ ] **10.s15.006.13** Cost regression gate: full S-15 distribution infra ≤ $200/mês.

## 11. DoD

- [ ] Cargo-fuzz 1M green committed (summary report + automation).
- [ ] Apple notarization workflow + signed binaries.
- [ ] Linux GPG signing workflow + signed binaries + public key published.
- [ ] Windows Authenticode signing workflow + signed binary OR waiver+ADR.
- [ ] 2 OSS case-studies committed em `examples/case-studies.md`.
- [ ] Dev workshop ≤ 5 min measured + report committed.
- [ ] PRR-S15.md committed com 5-8 sign-offs canonical.
- [ ] Adversarial test summary report committed.
- [ ] Métricas emitting em staging (telemetry opt-in baseline).
- [ ] WIs S-15-001..005 SEALED state.
- [ ] Sprint S-15 closed; release notes committed.

## 12. Invariants Validated

- **CTRL-CAS-002** (client verify default-on) IMPLEMENTA reflection cumulative em FFI 3 languages (WI-S15-004).
- **CTRL-CRED-001** (no secrets em CLI output) IMPLEMENTA cumulative via cargo-fuzz 1M iter `secret_redaction_check` harness 0 leaks.
- **INV-CAS-INTEGRITY** (CRITICAL — registry §3.X herdada) reforced via FFI client-verify default-on.
- All S-15 controls cumulatively validated.
- **Não introduz INVs novas** (SDK é consumer, não creator; per spec contract §8 mantidas only).

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| Cargo-fuzz targets | `fuzz/fuzz_targets/` | Rust |
| Cargo-fuzz summary | `specs/_audits/2026-XX-XX-cargo-fuzz-summary-s15.md` | Markdown |
| Apple notarize workflow | `.github/workflows/notarize-macos.yml` | YAML |
| Linux GPG sign workflow | `.github/workflows/sign-linux.yml` | YAML |
| Windows Authenticode workflow | `.github/workflows/sign-windows.yml` | YAML |
| Windows fallback ADR (if applicable) | `specs/_decisions/ADR-XXXX-windows-authenticode-fallback.md` | Markdown |
| GPG public key | `https://corelink.dev/.well-known/gpg-pubkey.asc` (CDN endpoint) | ASCII-armor |
| 2 OSS case-studies | `examples/case-studies.md` | Markdown |
| Dev workshop report | `specs/_audits/2026-XX-XX-dev-workshop-s15.md` | Markdown |
| PRR doc S-15 | `specs/04_sprints/S15/PRR-S15.md` | Markdown |
| Adversarial test summary | `specs/_audits/2026-XX-XX-adversarial-summary-s15.md` | Markdown |
| Release notes S-15 | `specs/04_sprints/S15/RELEASE_NOTES.md` | Markdown |
| `secret_redaction_check` harness | `fuzz/secret_redaction_check.rs` | Rust |

## 14. Quality Standards

- **14.s15.006.1** Cargo-fuzz 1M iter PR + 5M nightly cumulative; multi-engine (libFuzzer + AFL++).
- **14.s15.006.2** 0 panics + 0 secrets leaked em error paths.
- **14.s15.006.3** Coverage ≥ 80% CLI surface.
- **14.s15.006.4** 3 OSes signed (Apple notarized + Linux GPG + Windows Authenticode or waiver+ADR).
- **14.s15.006.5** 2 OSS proof-of-conversion documented.
- **14.s15.006.6** Time-to-first-cache-hit ≤ 5 min dev workshop measured monthly tracked.
- **14.s15.006.7** PRR doc 5-8 sign-offs canonical documented; non-fictional gates.
- **14.s15.006.8** SAST: cargo-audit + cargo-deny clean; pip-audit + npm audit clean.
- **14.s15.006.9** Cost regression gate: full S-15 distribution ≤ $200/mês.
- **14.s15.006.10** Adversarial summary 25+ scenarios.

## 15. Test Plan

### Cargo-fuzz harness
- 4 fuzz targets (cli_input + config_toml + json_deserialize + auth_resolution).
- 1M iter per target em PR; 5M cumulative nightly.
- Multi-engine libFuzzer + AFL++.
- Coverage tracking ≥ 80% CLI surface.
- `secret_redaction_check` harness: regex match `corelink_<env>_*` em stderr/output; assert 0.

### Cross-OS validation
- macOS notarized: `spctl --assess --verbose corelink` confirms accepted.
- Linux GPG: `gpg --verify corelink.asc corelink` confirms.
- Windows Authenticode: `signtool verify /pa /v corelink.exe` confirms (or fallback unsigned warning verified).

### OSS adoption validation
- 2 case-studies committed: adoption story + setup time + cache hit ratio + testimonial.
- Customer feedback collected via NDA-aware channel.

### Dev workshop
- 3 external developers; rotated per sprint (bias mitigation).
- Macros + Linux + Windows scenarios; Bazel + Buck2 alternation; FFI Python.
- Measured time clone → first cache hit; ≥ 80% trials ≤ 5 min.

### Adversarial summary aggregation
- 25+ scenarios cross-WI; 100% mitigation rate sustained.

## 16. Failure Modes

- **FM-150** (transient network): handled via retry per WI-S15-001.
- **FM-160** (auth invalid): handled via clear error per WI-S15-001.

## 17. Controls

- **CTRL-CRED-001** enforced cumulative via cargo-fuzz `secret_redaction_check` harness.
- **CTRL-CAS-002** enforced cumulative via FFI client-verify default-on (WI-S15-004 reflection).
- **CTRL-AUDIT-002** consumed via telemetry opt-in (WI-S15-005).

## 18. Resilience Patterns

- Multi-engine fuzz (libFuzzer + AFL++) coverage tracking; minimum 100M iter cumulative release.
- Fallback unsigned com waiver+ADR se Authenticode cert slip.
- Reproducible builds (S-12 alignment) sustained via cargo `--frozen --locked`.

## 19. Observability

PRR dashboard:
- Cargo-fuzz iter count + coverage + 0 panics status.
- 3 OSes signed binaries status (release).
- 2 OSS adoption status.
- Dev workshop ≤ 5 min trials sustained ratio.
- PRR 5-8 sign-offs canonical status.
- All métricas validation status (telemetry opt-in baseline).

## 20. Security & Privacy

**STRIDE delta**:
- **Spoofing**: 3 OSes signed binaries (Apple notarized + GPG + Authenticode); customer can verify trust chain.
- **Tampering**: cargo-fuzz 1M iter detects parser/serializer bugs.
- **Repudiation**: PRR sign-off table + workshop report + fuzz summary = forensic-grade trail.
- **Information disclosure**: `secret_redaction_check` harness verifies 0 leaks em error paths.
- **DoS**: telemetry opt-in non-blocking (WI-S15-005).
- **Elevation of privilege**: not applicable (CLI/SDK is client-side; tenant scoped via PAT).

**LINDDUN delta**:
- Linkability: 2 OSS case-studies anonymized se customer requests.
- Identifiability: PRR doc internal; sanitized version shareable.
- Non-compliance: GDPR Art. 25 (data protection by design) + LGPD Art. 6º X (transparency) satisfied via WI-S15-005 LINDDUN review.

## 21. Dependencies

### Hard blockers
- WI-S15-001..005 all SEALED.
- Apple Developer Program cert acquired (D+0 plan kick-off).
- GPG signing key acquired.
- Windows EV code-signing cert acquired (or fallback waiver+ADR ready).
- 2 OSS engagements pre-engagement Q3.
- 3 external developers recruited para dev workshop.
- PRR reviewers available (5-8 roles canonical).

### Soft blockers
- S-09 SEALED (telemetry consumed via observability stack server-side).
- S-12 SEALED (reproducible builds + signing trust chain baseline).

### Outbound
- S-16 (admin UI may consume CLI as backend logic).
- S-18 (public docs reference CLI examples + 2 case-studies).
- S-19 (customer onboarding uses starter projects + FFI samples).
- S-20 (GA exige 2 OSS projects integrating + CLI signed 3 OSes + cargo-fuzz 1M green).

## 22. Effort PERT

O: 12h, M: 18h, P: 28h → PERT **18.7h** (per spec contract §12; closing WI; concentrated cargo-fuzz + 3 signing flows + 2 OSS engagement + PRR coordination).

## 23. Cost Analysis

**Direct cost**:
- Cargo-fuzz CI compute: ~$50/mês.
- Apple Developer Program cert: $99/year amortized = $8/mês.
- GPG signing key: free.
- Windows EV cert: ~$300-500/year amortized = $30/mês.
- Dev workshop external developers: ~$1000/sprint × 6 sprints/yr = $6k/yr.
- 2 OSS engagement: internal time + minor incentive program (~$2k/yr).

**Total**: ~$8k/yr + ~$90/mês CI.

**Indirect cost**: 0 production CLI panics + secret leaks prevented = priceless.

## 24. Post-mortem Hooks

- CLI fuzz panic em produção (user reported) → 5-Why mandatory + cargo-fuzz iter delta analysis.
- Secret leak em CLI output (any) → CRITICAL post-mortem + Security review.
- Time-to-first-cache-hit > 10 min sustained → post-mortem (DX regression).
- Apple notarization rejected pos-D-day → notarize early staging + iterate.
- Authenticode cert process slip > 1 release → expedite acquisition + waiver review.
- 2 OSS adoption slow (não conseguir 2 em pre-engagement Q3) → post-mortem + S-19 fallback engagement.

## 25. Rollback / Recovery

PRR REJECTED → sprint reverts to DRAFT; remediation cycle. RTO ≤ 1 sprint.

## 26. Risk Register (6-col)

| ID | Risco | Prob | Det | Impacto | Exposure | Residual | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | Cargo-fuzz misses panic path | M | L | LOW | L | LOW | Multi-engine (libFuzzer + AFL++); coverage tracking; 100M iter release |
| R-002 | Apple notarization rejected (binary heuristic mismatch) | L | M | LOW | L | LOW | Notarize early staging + iterate; documented em build pipeline |
| R-003 | Windows Authenticode cert process slip | L | L | LOW | L | LOW | Acquire cert ahead D-day + signtool automation; fallback unsigned com waiver+ADR |
| R-004 | OSS adoption slow (< 2 pre-engagement Q3) | M | M | MEDIUM | M | LOW | Pre-engagement Forge + 1 external Bazel-using OSS; incentive program |
| R-005 | PRR sign-off staffing gap | M | M | HIGH | M | LOW | 2-week notice; alternate reviewers documented |
| R-006 | Dev workshop bias (3 developers same persona) | M | M | LOW | M | LOW | Rotation per sprint; diverse persona recruitment |
| R-007 | Time-to-first-cache-hit > 5 min UX miss | M | M | MEDIUM | M | LOW | Dev workshop weekly + iterate starter projects + onboarding video |
| R-008 | CONDITIONALLY_APPROVED waivers acumulam | L | M | MEDIUM | L | LOW | Waiver expiry mandatory next sprint; quarterly review |

## 27. Knowledge Transfer

- Tech talk (1.5h): "S-15 DX Tooling Whole-Stack Review + 2 OSS Case-Studies".
- Doc `docs/internal/s15-fuzz-summary.md` — sanitized findings.
- Doc `docs/internal/s15-signing-trust-chain.md` — Apple notarized + GPG + Authenticode walkthrough reusable.
- PRR-S15 release party post-SEAL com Engineer + DevX + Docs.
- Onboarding test (5 questions): cargo-fuzz target design + signing trust chain + PAT redaction + telemetry opt-in + ADR-0016 trade-off.

## 28. Sign-off (STANDARD 5-8 canonical; 7 typical — sprint ship gate)

Este WI emite o PRR; sign-off do PRR-S15.md doc é o sign-off final S-15 sprint.

**Staffing reality (per ADR-0034 solo-tier)**:

Pré-PRR mandatory check: confirmed canonical reviewers vs pending. Sprint S-15 pode-se SEAL com **5-8 sign-offs canonical** (7 typical) completos. Tier-1 staffing gap = sprint cannot SEAL until staffed OR explicit waiver com expiry + ADR.

| Status atual (2026-04-29) | Roles |
|---|---|
| **Confirmed (3)** | Owner (Gustavo Schneiter); Final Approver (Gustavo Schneiter); Product (Gustavo Schneiter — solo founder dual-hat) |
| **Pending Tier-1 hire/contract (4 specialized canonical roles em STANDARD)** | Engineer (S-15 lead), QA Lead, DevX advisor, Docs lead |
| **Total pending** | 4 of 7 typical canonical |

**Escalation plan se PRR sem todos canonical staffed**:
1. **Option A — solo-tier waiver**: Owner + Final Approver assume múltiplos dual-hats com explicit ADR (`ADR-0034-solo-tier-prr-waiver.md`). Documenta accepted residual risk + post-staffing review cadence.
2. **Option B — defer SEAL**: spec final permanece DRAFT até staffing closes.
3. **Option C — external advisor pool**: contract per-engagement DevX + Docs reviewers (lower lead time vs Tier-1 cripto/security/compliance roles em S-13/S-14 HIGH_RISK).

**Recommended path (current state; Lote 10.15 codex P1 fix — PRR independence baseline)**: Option C **mandatory minimum 2 of 4 pending roles** preenchidos via external advisor antes de SEAL (canonical: DevX advisor + Docs lead — typical 2-week lead time vs cripto/security em S-13/S-14 HIGH_RISK; cost ~$5-15k engagement). Option A solo-tier dual-hat **NÃO é PRR-independence baseline** — codex codex P1: self-waiver/dual-hat enfraquece PRR review independence. Sprint SEAL gate requires ≥ 5/7 canonical sign-offs com ≥ 2 external (não solo-tier dual-hat). Option B defer SEAL é alternativa válida se Option C cost prohibitive em short sprint (defer 1 sprint until staffed).

| # | Role | Name | Signed Date | Status |
|---|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | _pending_ | _pending_ |
| 2 | Final Approver | Gustavo Schneiter | _pending_ | _pending_ |
| 3 | Engineer (S-15 lead) | _TBD; emphatic — CLI implementation + cross-OS pipeline + cargo-fuzz harness_ | _pending_ | _pending_ |
| 4 | QA Lead | _TBD; emphatic — cargo-fuzz 1M iter + memory safety FFI + cross-runtime CI matrix_ | _pending_ | _pending_ |
| 5 | Product | Gustavo Schneiter | _pending_ | _pending_ |
| 6 | DevX advisor | _TBD; emphatic — time-to-first-cache-hit ≤ 5 min + 2 OSS proof-of-conversion + dev workshop_ | _pending_ | _pending_ |
| 7 | Docs lead | _TBD; emphatic — PRR doc + 2 case-studies + adversarial summary + ADR-0016 + privacy policy_ | _pending_ | _pending_ |

> Crypto SME folds em Architect role specialization se applicable em PR review (este WI consume CTRL-CAS-002 baseline em FFI; not novel cripto control). Compliance/Privacy/AppSec NÃO mandatory canonical em STANDARD lane (folded em Product + DevX advisor; LINDDUN review WI-S15-005 contributes Privacy lens).

## 29. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-29 | Gustavo (via Claude Opus 4.7) | Criação WI-S15-006 (cycle 12.S15.0; STANDARD lane single-phase SEAL D+15; cargo-fuzz 1M + Apple notarize + GPG + Authenticode + 2 OSS proof-of-conversion + PRR 5-8 canonical). |

## 30. Anti-patterns evitados

- Skip cargo-fuzz 1M (mandatory ship gate).
- Skip Apple notarization (mandatory; fallback unsigned com waiver+ADR).
- Skip GPG signing (mandatory).
- Skip 2 OSS proof-of-conversion (mandatory DoD §6).
- Approve PRR sem 5-8 sign-offs canonical.
- Production deploy sem cargo-fuzz 1M green.
- Skip dev workshop time-to-first-cache-hit measurement.
- Waivers acumulando sem expiry.
- External pentest scope creep (este WI é STANDARD; defer S-20 GA).
- Two-phase SEAL com 30d observation window (STANDARD lane single-phase D+15 sufficient).

---

**Fim WI-S15-006.** **S-15 sprint full WI spec completo (6/6 WIs SOTA STANDARD lane).** Próximo lote: 12.S16.0 (S-16 — Admin UI Web).
