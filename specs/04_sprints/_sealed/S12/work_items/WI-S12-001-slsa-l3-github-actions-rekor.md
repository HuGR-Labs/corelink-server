---
id: "WI-S12-001"
type: "work_item"
doc_status: "SEALED"
work_status: "DONE"
audit_status: "AUDITED"
version: "1.0.1"
created: "2026-04-29"
updated: "2026-05-13"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-005"]
parent: "S-12"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "SECURITY-MODEL"
  - "KEY-MANAGEMENT"
  - "OBSERVABILITY-MODEL"
  - "COMPLIANCE-MATRIX"
  - "FAILURE-MODES"
tags: ["wi", "s12", "supply-chain", "slsa", "slsa-l3", "github-actions", "sigstore", "fulcio", "rekor", "in-toto", "high-risk"]
---

# WI-S12-001 — SLSA L3 GitHub Actions Workflow + Fulcio Keyless OIDC + Rekor Inclusion Proof + Verify CLI

> **doc_status:** SEALED · **work_status:** DONE · **lane:** HIGH_RISK
> **Parent:** [S-12](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S12-001 |
| Título | SLSA L3 build provenance via `slsa-github-generator/generator_generic_slsa3.yml@v1.10.0` em hermetic GitHub Actions runner; in-toto v1.0 attestation assinada via sigstore/Fulcio keyless OIDC short-lived cert (10 min); inclusion proof publicada em Rekor transparency log; `corelink-supply-verify` Rust CLI para customer-side verification (`rekor-cli search --rekor_server https://rekor.sigstore.dev` + Fulcio chain validate); EVT-010 evidence pack |
| Sprint | S-12 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-005 (controles supply chain — bypass = blast radius global; cripto-load-bearing) |

## 1. Intent

Implementar `.github/workflows/release-slsa3.yml` que produz **SLSA Level 3 build provenance** (industry-leading vs OSS competitors operando L1/L2) via `slsa-github-generator/generator_generic_slsa3.yml@v1.10.0` em hermetic builder isolated VM (no network during compile), gera in-toto v1.0 attestation, assina keyless via Fulcio OIDC short-lived cert (GitHub Actions identity bound to workflow ref), publica inclusion proof em Rekor transparency log público, e disponibiliza `corelink-supply-verify` Rust CLI para customer-side verification. Foundation que WI-S12-003 (Cosign + CF deploy verify) consome.

```rust
// File: crates/corelink-supply-verify/src/lib.rs

#![forbid(unsafe_code)]

use async_trait::async_trait;

#[async_trait]
pub trait SlsaProvenanceVerifier: Send + Sync {
    /// Verify SLSA L3 provenance attestation: Fulcio chain valid, Rekor inclusion proof valid,
    /// builder identity matches expected workflow ref, in-toto v1.0 schema valid.
    async fn verify(
        &self,
        attestation: &SlsaAttestation,           // in-toto v1.0 envelope
        expected_builder: &BuilderIdentity,      // e.g., "https://github.com/HumanGuardrail/corelink-server/.github/workflows/release-slsa3.yml@refs/tags/v0.X.Y"
    ) -> Result<VerifiedProvenance, VerifyError>;
}

#[derive(Debug, Clone)]
pub struct VerifiedProvenance {
    pub builder_id: String,                     // Fulcio cert SAN URI matched
    pub commit_sha: String,                     // GitHub commit SHA pinned
    pub workflow_ref: String,                   // pinned workflow ref
    pub rekor_log_index: u64,                   // public verifiable
    pub rekor_inclusion_proof_url: String,
    pub fulcio_cert_chain_valid: bool,
    pub in_toto_schema_version: String,         // "v1.0"
}

#[derive(Debug, thiserror::Error)]
pub enum VerifyError {
    #[error("Fulcio chain invalid: {0}")]
    FulcioChainInvalid(String),
    #[error("Rekor inclusion proof missing or invalid: {0}")]
    RekorInclusionInvalid(String),
    #[error("builder identity mismatch (got {got}, expected {expected})")]
    BuilderMismatch { got: String, expected: String },
    #[error("in-toto schema invalid: {0}")]
    InTotoSchemaInvalid(String),
    #[error("attestation expired (cert validity window passed)")]
    AttestationExpired,
}
```

Workflow output: `provenance.intoto.jsonl` (attestation envelope) + `provenance.intoto.bundle` (Cosign bundle com Rekor inclusion proof) attached como release asset. Customer-side: `corelink-supply-verify verify --bundle provenance.intoto.bundle --release v0.X.Y` returns `VerifiedProvenance` ou error.

## 2. Narrative (HIGH_RISK ≥ 300 palavras + risk justification)

SLSA L3 attestation é boundary cripto-load-bearing: vulnerabilidades classe SolarWinds (build pipeline tampered), event-stream 2018 (post-build malicious step), XZ Utils 2024 (compiler-stage backdoor) demonstram que **L3 com hermetic builder + transparency log é o mínimo defensável** para substrate cripto. CoreLink é alvo high-value (multi-tenant + CI/CD substrate em 100+ customers post-GA); attack ROI elevado para nation-state actor.

**Bugs catastróficos possíveis** (todos endereçados):

1. **alg=none-equivalent em provenance**: attestation envelope não-assinado aceito como válido; cliente confia falsa provenance. Mitigação: explicit Fulcio chain validate (Fulcio root cert pinned + intermediate chain via TUF rotation); sigstore-cosign verify rejeita unsigned envelopes; CLI `corelink-supply-verify` enforces Fulcio chain mandatory.

2. **Rekor inclusion proof missing**: attestation assinada mas não publicada em Rekor; offline tampering trivial (cliente não pode detect). Mitigação: INV-SUPPLY-PROVENANCE-IN-REKOR (CRITICAL); deploy webhook (WI-S12-003) checks Rekor inclusion antes rollout; CLI default mode = strict (Rekor missing = error).

3. **Builder identity confusion**: attacker stages malicious workflow em fork; gera attestation; cliente não valida `builder_id` matches expected. Mitigação: `expected_builder` arg obrigatório no CLI (não default); workflow ref pinned em release process; SAN URI Fulcio cert match exato vs expected pattern (`https://github.com/HumanGuardrail/corelink-server/.github/workflows/release-slsa3.yml@refs/tags/vX.Y.Z`).

4. **Workflow tampering pré-trigger**: attacker com PR write modifica workflow yaml + triggers release; provenance gerada com workflow malicioso. Mitigação: required code review + CODEOWNERS para `.github/workflows/`; release tag protection; Fulcio cert SAN includes workflow file SHA1 (verifiable post-facto).

5. **Hermetic build bypass**: workflow inadvertently makes network call durante compile (curl, cargo fetch with new dep); SLSA L3 generator fails build OR generates attestation with `builder.buildType` flagged non-hermetic. Mitigação: dependency review action enforced (Dependabot pre-build review); cargo offline mode opcional (lockfile pinned); SLSA generator validates `builder.buildType` == `https://github.com/slsa-framework/slsa-github-generator/generic@v1`.

6. **Fulcio root cert rotation drift**: Fulcio rotates root CA; deploy verifier (WI-S12-003) caches stale root → false rejections sustained. Mitigação: TUF metadata refresh in CLI (sigstore TUF root rotation handled automatically); deploy verifier syncs root cert via TUF every 24h.

7. **In-toto schema drift**: attestation generated em v0.0.1 schema; verifier expects v1.0 → false rejection. Mitigação: schema version pinned em release pipeline; explicit `expected_schema_version: "v1.0"` arg; schema migration ADR mandatory antes upgrade.

8. **Rekor outage**: sigstore Rekor down; new releases blocked. Mitigação: 24h grace period documented em fallback runbook; local cache de Rekor inclusion proofs (3-day retention); incident severity SEV-2 (operational, não breach).

**Atacante adversarial scenarios**:

- **Attestation forge attempt**: attacker stages release tag em fork; generates valid Fulcio cert (OIDC bound to fork's workflow); publishes em Rekor (public log). Mitigação: customer-side verify enforces `builder_id` matches `HumanGuardrail/corelink-server` org + workflow ref pinned; fork attestation = `BuilderMismatch` error.

- **TOCTOU em release pipeline**: attacker amends release commit pós-tag; provenance generated com novo SHA; cliente verifica antiga SHA → false reject. Mitigação: tag protection + signed commits required; release SHA pinned em release notes (immutable Git ref).

- **Rekor entry tampering**: Rekor é append-only Merkle tree; tampering entry detectable via inclusion proof + log consistency proof. Customer-side: CLI valida log consistency proof opcionalmente (paranoid mode).

**Risk justification HIGH_RISK**:

- **FF-HR-005**: provenance bypass = full pipeline trust violation; substrate cripto-load-bearing.
- **Reversibility**: bypass detected pós-customer-deploy = breach com customer notification overhead + reputation damage; mitigação proativa via Rekor mandatory + property tests.

11 sign-offs canonical incl. Architect (Crypto SME specialization mandatory: Fulcio chain validation + Rekor inclusion proof + in-toto v1.0 schema + adversarial attestation forge tests) + AppSec (workflow yaml threat model + GitHub Actions OIDC permissions review).

## 3. Customer Impact & Journey

**Persona 1 — SecOps lead em prospect enterprise (RFP evaluation)**:
- Customer acessa `https://github.com/HumanGuardrail/corelink-server/releases/v0.X.Y` → download `provenance.intoto.bundle`.
- Run `corelink-supply-verify verify --bundle provenance.intoto.bundle --release v0.X.Y --expected-builder "HumanGuardrail/corelink-server"` → output: `VerifiedProvenance { builder_id: "...", rekor_log_index: 12345678, fulcio_cert_chain_valid: true, ... }`.
- Customer-visible diferenciador: 95%+ OSS Rust SaaS opera SLSA L1; CoreLink em L3 = sinal forte para procurement/legal/compliance.

**Persona 2 — Auditor SOC 2 / ISO 27001**:
- Audit query: "Show me 100% releases últimos 30d com Rekor inclusion proof".
- Evidence: Dependency-Track integration (WI-S12-005) extrai Rekor URLs; CLI script `audit-evidence-collect.sh` produz dossier com (release tag, commit SHA, Rekor log index, Fulcio cert SAN, in-toto schema version).
- Compliance Matrix mapping: SOC 2 CC6.7 (change management with signed deploy + provenance).

**Persona 3 — Engineer onboarding em CoreLink**:
- `docs/internal/slsa-l3-pipeline.md` explica build attestation flow.
- ADR (forward) `ADR-XXXX-slsa-l3-rekor-mandatory.md` documenta INV-SUPPLY-PROVENANCE-IN-REKOR rationale.
- `corelink-supply-verify --help` self-documents.

**SLA addendum**:
- SLSA attestation generation latency: ≤ 3 min adicionais por release (parallel to build).
- Rekor inclusion proof publish: ≤ 30s pós-Fulcio cert issuance.
- CLI verify latency: ≤ 5s p99 (Rekor lookup + Fulcio chain validate).

## 4. Capability Mapping

- **CAP-SUPPLY-001** (SLSA L3 build provenance) — IMPLEMENTA primary.
- Trace: `_spec_contract.md §4 + §5.1` + `security_model.md §11.4 (CTRL-SUPPLY-001 + CTRL-CRYPTO-002)` + `compliance_matrix.md §3.6 (SOC 2 CC6.7 + EO 14028 + NIST SSDF)`.

## 5. Tipo

Build pipeline + cripto verifier; HIGH_RISK; FF-HR-005.

## 6. Escopo

### 6.1 In-scope

1. **`.github/workflows/release-slsa3.yml`** workflow novo:
   - Trigger: `release: { types: [published] }` + manual `workflow_dispatch`.
   - Job 1: build Worker WASM bundle + side artifacts (`cargo build --release --target wasm32-unknown-unknown`).
   - Job 2: SLSA generator chamado via reusable workflow `slsa-framework/slsa-github-generator/.github/workflows/generator_generic_slsa3.yml@v1.10.0` com inputs:
     - `base64-subjects: $(echo -n "$ARTIFACT_SHA256:$ARTIFACT_NAME" | base64)`.
     - `provenance-name: "provenance.intoto.jsonl"`.
     - Outputs: `attestation-name`, `provenance-download-name`.
   - Permissions block:
     - `id-token: write` (OIDC para Fulcio).
     - `actions: read` (provenance reading workflow context).
     - `contents: write` (release upload).
   - Hermetic constraints: `cargo build --offline` se lockfile presente; no network calls após dependency fetch.
2. **`crates/corelink-supply-verify/`** crate:
   - Cargo.toml: deps `sigstore = "0.9"` (Rust crate audited), `serde`, `serde_json`, `tokio`, `clap = "4"` (CLI), `thiserror`, `tracing`.
   - Binary `corelink-supply-verify` (CLI):
     - Subcommand `verify --bundle <path> --release <tag> --expected-builder <pattern>`.
     - Subcommand `lookup --rekor-log-index <N>` (paranoid mode; standalone Rekor lookup).
     - Subcommand `extract --bundle <path> --output-format json|yaml` (debug).
   - Library API: `SlsaProvenanceVerifier` trait + `VerifiedProvenance` struct + `VerifyError` enum.
3. **`SlsaProvenanceVerifier` impl**:
   - Parse in-toto envelope (DSSE format).
   - Validate schema version (`predicateType` matches `https://slsa.dev/provenance/v1`).
   - Extract Fulcio cert from envelope; validate chain via TUF-pinned root cert (sigstore-rs handles).
   - Extract `builder.id` from predicate; match against `expected_builder` pattern.
   - Lookup Rekor inclusion proof via log index (sigstore-rs `rekor_client`).
   - Validate inclusion proof Merkle root.
   - Return `VerifiedProvenance` ou error.
4. **Métricas underscored Prometheus** (per observability_model.md §4.1; label `plan` per §3.1):
   - `corelink_supply_slsa_attestations_total{outcome,plan}` (outcome ∈ ok|fulcio_fail|rekor_fail|sign_fail|hermetic_violation).
   - `corelink_supply_slsa_attestation_generation_duration_seconds_bucket` (histogram p50/p95/p99).
   - `corelink_supply_rekor_inclusion_proof_verify_total{outcome}` (outcome ∈ ok|missing|invalid|merkle_mismatch).
   - `corelink_supply_fulcio_chain_validate_total{outcome}` (outcome ∈ ok|root_pin_fail|intermediate_fail|expired).
5. **Observability** — trace span `slsa.attestation.generate` + `slsa.attestation.verify` com attributes:
   - `slsa.builder_id` (string).
   - `slsa.workflow_ref` (string).
   - `slsa.rekor_log_index` (u64).
   - `slsa.commit_sha` (string).
   - `result` (enum).
6. **Property tests** (10k iter PR + 100k iter nightly):
   - `prop_slsa_envelope_signature_invalid_rejected`: 10k random byte mutations em envelope; assert 100% rejection.
   - `prop_slsa_builder_id_mismatch_rejected`: 10k random builder_id strings; assert 100% rejection a less que matches expected pattern.
   - `prop_slsa_rekor_inclusion_proof_invalid_rejected`: 10k mutated Merkle proofs; assert 100% rejection.
   - `prop_slsa_in_toto_schema_drift_rejected`: 10k random schema version strings; assert apenas v1.0 accepted.
7. **Adversarial regression tests** (5 CVE-class scenarios):
   - Attestation forge via fork (different builder_id) → rejected.
   - Rekor inclusion proof tampered (Merkle root mismatch) → rejected.
   - Fulcio cert expired → rejected.
   - In-toto schema v0.0.1 (old format) → rejected.
   - DSSE envelope alg=none → rejected.
8. **Integration test E2E**:
   - Real release workflow trigger em staging fork; verify attestation generated + Rekor entry created + CLI verifies successfully.
9. **`docs/internal/slsa-l3-pipeline.md`** documentation:
   - Architecture diagram (build → SLSA generator → Fulcio → Rekor → release asset).
   - Verification quickstart for customers.
   - Failure mode + escalation matrix.

### 6.2 Out-of-scope (deferred)

- **Cosign sign release artifacts** + **CF deploy webhook verifier**: WI-S12-003 (este WI provê provenance attestation; WI-003 consome para deploy gate).
- **SBOM CycloneDX generation**: WI-S12-002.
- **cargo-audit + cargo-deny + Dependabot**: WI-S12-004.
- **Dependency-Track self-host**: WI-S12-005.
- **Reproducible builds 2-runner diff**: WI-S12-006.
- **SLSA Level 4** (hermetic verifier + two-party review): pós-GA Q3 se justificativa enterprise materializar.
- **Multi-platform attestation aggregation** (e.g., Linux + macOS builders): pós-GA enterprise.
- **Customer-facing verifier as Cloudflare Worker** (web UI): S-16 admin plane.

## 7. Anti-Scope

- ❌ Custom SLSA generator (use `slsa-github-generator` reusable workflow audited).
- ❌ HS256-equivalent signing scheme (only Fulcio keyless OIDC + Cosign sigstore-standard).
- ❌ Long-lived signing keys (no `cosign generate-key-pair` em production; keyless OIDC only).
- ❌ Rekor optional / "best-effort" (mandatory; INV-SUPPLY-PROVENANCE-IN-REKOR enforces).
- ❌ Self-hosted Rekor instance (use sigstore.dev public; pós-GA enterprise tier pode self-host se demand).
- ❌ Skip in-toto schema validation (regression class XZ Utils 2024).
- ❌ Skip Fulcio root cert pinning (TUF rotation handles upgrades; root pinning é defense vs CA compromise).
- ❌ Workflow inline in release branch (CODEOWNERS + tag protection + signed commits required).
- ❌ Provenance attestation only para tagged releases (also nightly canary builds).
- ❌ "Provenance optional via flag" feature (anti-pattern; never).

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: SLSA L3 build provenance + Rekor inclusion + customer-side verify

  Background:
    Given .github/workflows/release-slsa3.yml configured
    And SLSA generator pinned at v1.10.0
    And Fulcio root cert pinned via TUF
    And expected_builder = "https://github.com/HumanGuardrail/corelink-server/.github/workflows/release-slsa3.yml@refs/tags/v0.X.Y"

  Scenario: Release triggers SLSA L3 attestation
    Given a release tag v0.X.Y published
    When release-slsa3.yml workflow runs
    Then SLSA generator produces in-toto v1.0 attestation
    And Fulcio issues short-lived cert (10 min validity) bound to GitHub Actions identity
    And Rekor receives inclusion proof entry
    And provenance.intoto.bundle attached to release as asset
    And metric corelink_supply_slsa_attestations_total{outcome="ok"} incremented

  Scenario: Customer verifies attestation successfully
    Given customer downloads provenance.intoto.bundle
    When customer runs `corelink-supply-verify verify --bundle provenance.intoto.bundle --release v0.X.Y --expected-builder HumanGuardrail/corelink-server`
    Then output: VerifiedProvenance { builder_id, commit_sha, workflow_ref, rekor_log_index, ... }
    And exit code 0
    And metric corelink_supply_rekor_inclusion_proof_verify_total{outcome="ok"} incremented

  Scenario: Forge attempt rejected (builder_id mismatch)
    Given attacker generates attestation em fork "attacker/corelink-server"
    When customer runs verify with --expected-builder HumanGuardrail/corelink-server
    Then VerifyError::BuilderMismatch returned
    And exit code != 0
    And metric corelink_supply_slsa_attestations_total{outcome="builder_mismatch"} incremented

  Scenario: Rekor inclusion proof missing rejected
    Given attestation generated but not published to Rekor (offline tampering)
    When customer runs verify
    Then VerifyError::RekorInclusionInvalid returned
    And exit code != 0

  Scenario: Fulcio chain validation fails (root cert mismatch)
    Given malicious Fulcio cert chain not anchored to sigstore root
    When customer runs verify
    Then VerifyError::FulcioChainInvalid returned
    And exit code != 0

  Scenario: In-toto schema drift rejected
    Given attestation predicateType = "https://slsa.dev/provenance/v0.0.1" (old)
    When customer runs verify
    Then VerifyError::InTotoSchemaInvalid returned

  Scenario: Hermetic build violation flagged
    Given workflow makes network call durante cargo build (e.g., dep fetch from non-crates.io)
    When SLSA generator runs
    Then attestation marks builder.buildType as non-hermetic
    And metric corelink_supply_slsa_attestations_total{outcome="hermetic_violation"} incremented
    And release blocked (release-slsa3.yml job fails)

  Scenario: Rekor outage graceful degradation
    Given Rekor server returns 503 sustained 1h
    When release-slsa3.yml workflow runs
    Then attestation generation completes locally
    And attestation marked rekor_inclusion_required (hard-fail; no pending state per INV-SUPPLY-PROVENANCE-IN-REKOR canonical fail-closed; codex SEAL cycle 1)
    And alert SEV-2 fires
    And release publication BLOCKED until Rekor available (no fallback bypass)

  Scenario: TUF root cert rotation handled
    Given sigstore Fulcio root cert rotates (new root via TUF)
    When customer runs verify after rotation
    Then sigstore-rs auto-fetches new TUF metadata
    And verify succeeds with new root
    And metric corelink_supply_fulcio_chain_validate_total{outcome="ok"} incremented

  Scenario: Property test 100k iter green
    Given prop_slsa_envelope_signature_invalid_rejected 100k iter
    When test runs nightly
    Then 0 false-accepts
    And 0 panics
```

## 9. Design Decisions

### 9.1 Why `slsa-github-generator` (não custom)

`slsa-framework/slsa-github-generator/.github/workflows/generator_generic_slsa3.yml@v1.10.0`:
- Reference implementation maintained by SLSA WG + OpenSSF.
- Audited via OpenSSF Scorecard.
- GitHub-managed isolated VM = L3 builder requirement satisfied.
- Versioned releases (pin at v1.10.0; bump via ADR).

Alternativa rejeitada: hand-roll generator (security boundary; reuse audited é ADR-0014 mandate).

### 9.2 Why Fulcio keyless OIDC (não long-lived keys)

- **Threat class eliminada**: secret exfiltration via compromised CI step (zero long-lived secrets em GitHub Actions).
- **Identity binding**: Fulcio cert SAN = workflow identity (ref + commit SHA); cliente verifica origin cripticamente.
- **10 min cert validity**: replay attack window minimizado; cert expira antes attacker pode abuse.
- Industry trend: Cosign 2.0 default keyless; OSS ecosystem migrating.

Alternativa rejeitada: long-lived RSA key em GitHub Secrets (operationally fragil; rotation overhead; threat class bypass).

### 9.3 Why Rekor mandatory (INV-SUPPLY-PROVENANCE-IN-REKOR)

- **Offline tampering trivial sem transparency log**: attacker substitui attestation file local; cliente sem Rekor não detecta.
- **Public verifiability**: Rekor é Merkle tree append-only; inclusion proof = cripto evidence pública.
- **Auditor signal**: SOC 2 / ISO 27001 auditors querem evidence verifiable independent of vendor (Rekor é third-party trust).
- Cost: ~$0/release (sigstore.dev public; rate limit generous).

Alternativa rejeitada: optional Rekor ("best-effort") — unsafe; INV mandatory + CI gate + deploy verifier hard gate.

### 9.4 Why in-toto v1.0 (não v0.0.1)

- v1.0 stable schema (Mar 2024); ratified em SLSA v1.0 spec.
- v0.0.1 deprecated; tooling migration period over.
- Pin schema version em verifier; explicit `expected_schema_version` arg evita drift attack.

### 9.5 Why customer-side CLI (não server-only verify)

- **Defense em depth**: server verify + customer verify = 2 independent checks; reduz risk of server compromise inadvertently signing falsa provenance.
- **OSS substrate**: customers querem self-verify (não trust vendor cego).
- Industry pattern: Chainguard images, Distroless, Cosign all expect customer-side verify.

### 9.6 Why pin SLSA generator at v1.10.0 (não @latest)

- Reproducible build property: pinned reusable workflow = same generator behavior across builds.
- ADR upgrade cadence: bump via ADR with regression testing.
- Latest tag = supply chain risk (compromised tag could redirect).

### 9.7 Why hermetic build mandatory

- L3 spec requirement: hermetic = no network durante compile.
- Mitigates dependency confusion attack class (malicious dep injected mid-build).
- Trade-off: requires lockfile pinned (`Cargo.lock` committed; covered WI-S12-004).

### 9.8 Why no graceful fallback em Rekor outage (release blocked)

- Provenance sem Rekor = uncryptographically verifiable; effectively SLSA L1.
- 24h grace would create attack window (intentional Rekor disruption + malicious release timing).
- Sigstore SLA historically 99.9%+; outage > 1h rare.
- Trade-off: release publication delayed durante outage; documented em fallback runbook.

### 9.9 Why ADR potencial?

- Sim — **ADR-XXXX**: "SLSA L3 + Rekor mandatory ratification em CoreLink build pipeline". Decisão arquitetural cripto-load-bearing; reuse pattern em S-13 admin plane (admin op signing) + S-14 BYOK (customer key attestation forward).
- Whitelist em `validate_references.py` até materializar.

## 10. Completeness Criteria SOTA

- [ ] **10.s12.001.1** Property test 10k iter (PR) + 100k iter (nightly) sobre fuzz attestation envelope inputs → 0 panics, 0 false-accepts (EVT-002).
- [ ] **10.s12.001.2** Adversarial test: 5 CVE-class regressions (forge fork builder_id, Rekor proof tampered, Fulcio expired, schema drift v0.0.1, DSSE alg=none) — 100% rejected (EVT-040).
- [ ] **10.s12.001.3** E2E test contra GitHub Actions staging fork: release tag → workflow → Fulcio cert + Rekor inclusion → CLI verifies; latência atestação ≤ 3 min adicional, verify ≤ 5s p99 (EVT-018).
- [ ] **10.s12.001.4** SLSA L3 attestation publicada em Rekor para 100% releases pós-S-12 últimos 30d (verifiable via `rekor-cli search`) (EVT-010).
- [ ] **10.s12.001.5** SAST clean (cargo-audit + cargo-deny + clippy `-D warnings`) em `corelink-supply-verify` crate (EVT-002).
- [ ] **10.s12.001.6** Cost regression gate: SLSA generator job ≤ 3 min adicionais por release; build pipeline cost ≤ $0.10 per release adicional (Lote 9.4 §14.10).
- [ ] **10.s12.001.7** OWASP ASVS V14 (configuration) + SSDF PS.1 (cripto integrity) 100% checklist pass (EVT-002).
- [ ] **10.s12.001.8** Hermetic build verified: SLSA generator output `builder.buildType` matches `https://github.com/slsa-framework/slsa-github-generator/generic@v1`; no network call detected durante compile (EVT-027).
- [ ] **10.s12.001.9** Fulcio root cert pinned via TUF; rotation handled automatically (chaos test simulating root rotation) (EVT-023).
- [ ] **10.s12.001.10** INV-SUPPLY-PROVENANCE-IN-REKOR ratificado em registry (já presente; CI gate ativo) (EVT-022).

## 11. DoD

- [ ] `.github/workflows/release-slsa3.yml` committed + verified em staging fork.
- [ ] Crate `corelink-supply-verify` compila (workspace + standalone) + binário `corelink-supply-verify` builds.
- [ ] All Gherkin scenarios green em integration test.
- [ ] Property tests 10k green em PR + 100k green em nightly.
- [ ] E2E test contra GitHub Actions staging green.
- [ ] Métricas emitidas (4 listadas §6.1.4).
- [ ] Trace span `slsa.attestation.generate` + `slsa.attestation.verify` em OTel pipeline.
- [ ] rustdoc + 3 examples (verify basic, paranoid mode com log consistency proof, lookup standalone).
- [ ] `docs/internal/slsa-l3-pipeline.md` published.
- [ ] ADR-XXXX (SLSA L3 + Rekor mandatory) escrito + ratificado.
- [ ] Code review (Crypto SME folded into Architect + AppSec).
- [ ] PRR Architect mini-sign-off (ship gate é WI-S12-007).
- [ ] Cost regression gate green.

## 12. Invariants Validated

### Mantidas

- **INV-SUPPLY-SIGNED-DEPLOY** (HIGH — herdada): este WI provê foundation provenance attestation; WI-S12-003 enforce no deploy webhook.

### Novas (introduzidas por S-12)

- **INV-SUPPLY-PROVENANCE-IN-REKOR** (HIGH — NEW): este WI implementa primary; toda provenance attestation deve estar em Rekor; release sem inclusion proof = blocked. CI gate via release-slsa3.yml + customer-side CLI enforcement.

TLA+ alignment: não-aplicável (build-time controle, não runtime state machine; Cosign/Rekor são externos).

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| SLSA L3 release workflow | `.github/workflows/release-slsa3.yml` | YAML (GitHub Actions) |
| `corelink-supply-verify` crate | `crates/corelink-supply-verify/` | Rust workspace member |
| SlsaProvenanceVerifier impl | `crates/corelink-supply-verify/src/verifier.rs` | Rust |
| VerifiedProvenance + types | `crates/corelink-supply-verify/src/types.rs` | Rust |
| VerifyError enum | `crates/corelink-supply-verify/src/error.rs` | Rust |
| CLI binary | `crates/corelink-supply-verify/src/bin/cli.rs` | Rust |
| Property tests | `crates/corelink-supply-verify/tests/prop_verify.rs` | Rust |
| Adversarial regression tests | `crates/corelink-supply-verify/tests/adversarial.rs` | Rust |
| E2E integration | `tests/e2e_slsa_l3.rs` | Rust |
| ADR-XXXX (SLSA L3 + Rekor) | `specs/03_architecture/adrs/ADR-XXXX-slsa-l3-rekor-mandatory.md` | Markdown |
| Pipeline doc | `docs/internal/slsa-l3-pipeline.md` | Markdown |
| Examples | `crates/corelink-supply-verify/examples/` (verify_basic.rs, paranoid_mode.rs, lookup.rs) | Rust |

## 14. Quality Standards SOTA

- **14.s12.001.1** Zero `unsafe`; zero `unwrap` em src/ (allow em tests).
- **14.s12.001.2** rustdoc 100% public API + 3 examples.
- **14.s12.001.3** Test coverage ≥ 90% (`cargo tarpaulin`).
- **14.s12.001.4** Latência: CLI verify p99 ≤ 5s warm; cold ≤ 10s (TUF metadata first-fetch).
- **14.s12.001.5** SAST: `cargo-audit` + `cargo-deny` + `clippy -D warnings` clean.
- **14.s12.001.6** Métricas RED (rate, errors, duration) + Fulcio/Rekor cache metrics.
- **14.s12.001.7** Runbook RB-FM-156 (dep malicious) — referenciado WI-S12-007.
- **14.s12.001.8** Breaking changes em `VerifiedProvenance` struct = bump major + migration note.
- **14.s12.001.9** Memory: ≤ 64 KiB stack em verify path; TUF metadata cached em ~/.sigstore/.
- **14.s12.001.10** Cost regression gate em CI (SLSA generator ≤ 3 min adicional; baseline benchmark sustained).
- **14.s12.001.11** SLSA generator pinned at exact version (v1.10.0); bump via ADR.
- **14.s12.001.12** Hermetic build verified em SLSA generator output.

## 15. Chaos Experiments

1. **Rekor outage**: simulate 503 from Rekor sustained 30 min; verify release-slsa3.yml fails (no fallback bypass); alert SEV-2 fires; release publication blocked. Hypothesis: graceful degradation = release delayed, não unsafe deploy. Procedure: block Rekor URL via wrangler dev fetch interceptor durante release; observe workflow fail. Abort: outage > 60min em staging cancels (real customer impact).

2. **Fulcio root cert rotation**: simulate sigstore TUF rotation; verify CLI auto-fetches new metadata + verify succeeds. Hypothesis: TUF refresh handles rotation transparently. Procedure: reset local TUF metadata cache; run CLI verify; observe TUF fetch + success. Abort: not applicable (synthetic).

3. **Attestation forge from fork**: red team stages release em `attacker/corelink-server` fork; generates valid Fulcio cert (OIDC bound to fork's workflow); publishes em Rekor (public log); customer-side CLI rejects via builder_id mismatch. Hypothesis: 100% rejection (BuilderMismatch error). Procedure: stage fork release; CLI verify with --expected-builder HumanGuardrail; observe rejection. Abort: not applicable.

4. **In-toto schema drift attack**: attacker generates attestation com `predicateType: "https://slsa.dev/provenance/v0.0.1"` (old format). Hypothesis: rejected via InTotoSchemaInvalid. Procedure: synthesize old-schema envelope; CLI verify; observe rejection.

5. **DSSE alg=none envelope**: attacker generates DSSE envelope com alg=none + no signature. Hypothesis: rejected via FulcioChainInvalid (sigstore-rs enforces). Procedure: synthesize alg=none envelope; CLI verify.

6. **Workflow yaml tampering pre-trigger**: attacker with PR write modifies `.github/workflows/release-slsa3.yml` + triggers release; provenance generated com workflow malicioso; Fulcio cert SAN includes workflow file SHA1 (verifiable post-facto); customer CLI checks SHA1 match expected. Hypothesis: tampering detectable via Fulcio cert SAN + signed commits + CODEOWNERS. Procedure: simulate workflow change PR; observe required reviews + tag protection.

7. **TUF metadata poisoning attempt**: attacker hosts malicious TUF mirror; CLI uses --tuf-mirror flag (não default). Hypothesis: default mirror is sigstore.dev (TUF root pinned); attacker mirror requires explicit flag. Procedure: red team reviews CLI args; verifies no implicit mirror discovery.

## 16. PRR (Production Readiness Review)

PRR HIGH_RISK 11 sign-offs canonical (S-12 ship gate é WI-S12-007; este WI passa por mini-PRR Architect review):

- [ ] All Gherkin scenarios green.
- [ ] Property tests + adversarial tests green.
- [ ] E2E GitHub Actions staging green.
- [ ] Cost regression gate green.
- [ ] Métricas + dashboards configurados em DASH-SUPPLY.
- [ ] ADR-XXXX (SLSA L3 + Rekor mandatory) published.
- [ ] Crypto SME review (Fulcio chain validation + Rekor inclusion proof + in-toto schema).
- [ ] AppSec review (workflow yaml threat model + GitHub Actions OIDC permissions + tag protection).
- [ ] Architect approval (pattern reusable em S-13 admin plane + S-14 BYOK).
- [ ] OWASP ASVS V14 + SSDF PS.1 100% pass.
- [ ] Pentest hooks documented (target em WI-S12-007 Security walkthrough).

## 17. Sub-tasks

| ID | Sub-task | Estimativa |
|---|---|---|
| ST-001 | `.github/workflows/release-slsa3.yml` skeleton + SLSA generator integration | 3h |
| ST-002 | Workflow permissions + OIDC setup + tag protection rules | 1.5h |
| ST-003 | Crate `corelink-supply-verify` scaffold + Cargo.toml deps | 1h |
| ST-004 | `VerifiedProvenance` + types + error enum | 1h |
| ST-005 | `SlsaProvenanceVerifier` impl (Fulcio chain + Rekor lookup + in-toto schema) | 4h |
| ST-006 | CLI binary (3 subcommands: verify, lookup, extract) | 2h |
| ST-007 | Métricas emit (4 metrics) + trace spans | 1.5h |
| ST-008 | Property tests 10k iter (4 props) | 3h |
| ST-009 | Adversarial regression tests (5 CVE classes) | 2h |
| ST-010 | E2E integration test contra staging fork | 3h |
| ST-011 | Chaos test Rekor outage + Fulcio rotation | 2h |
| ST-012 | rustdoc + 3 examples | 1.5h |
| ST-013 | `docs/internal/slsa-l3-pipeline.md` | 1.5h |
| ST-014 | ADR-XXXX redação | 1.5h |
| ST-015 | Crypto SME (folded Architect) + AppSec review feedback iteration | 2h |
| ST-016 | PRR Architect mini-sign-off | 1h |

**Total Optimistic**: ~30h. **PERT** (O=12h, M=18h, P=30h, per spec contract §12): **19h**. Sub-tasks soma é detail-grain; PERT spec contract é consolidated.

## 18. Dependencies

### Hard blockers

- GitHub Actions OIDC enabled em org `HumanGuardrail` (operations team task; not blocker para spec).
- Sigstore Fulcio + Rekor public instances available (sigstore.dev SLA 99.9%+).

### Soft blockers

- Cargo workspace structure baseline (S-01 SEALED).

### Outbound

- WI-S12-002 (SBOM CycloneDX) — paralelo; SBOM também publicada como release asset.
- WI-S12-003 (Cosign + CF deploy verify) — consumes provenance attestation (Rekor inclusion + Fulcio chain).
- WI-S12-005 (Dependency-Track) — SBOM ingestion; orthogonal a este WI.
- WI-S12-007 (PRR ship gate) — gates S-12 close.

## 19. Effort PERT

O: 12h, M: 18h, P: 30h → PERT **19h** (per spec contract §12).

## 20. Time-boxing

**24h hard limit**. If exceeded → escalation: split em sub-WI (workflow + CLI vs adversarial tests).

## 21. Observability

4 métricas listadas §6.1.4. Trace spans `slsa.attestation.generate` + `slsa.attestation.verify` com attributes:
- `slsa.builder_id` (string)
- `slsa.workflow_ref` (string)
- `slsa.rekor_log_index` (u64)
- `slsa.commit_sha` (string)
- `result` (enum)

Logs structured JSON; nivel INFO em success, WARN em retry (TUF refresh), ERROR em verify fail.

Dashboard widget DASH-SUPPLY:
- SLSA attestations published rate (per release).
- Rekor inclusion proof verify success ratio (target ≥ 99.9%).
- p99 latency (CLI verify).
- Fulcio chain validate success ratio.
- Hermetic violation rate (target = 0).

## 22. Cost Analysis

- SLSA generator job: ~3 min × $0.008/min GitHub Actions = $0.024 per release.
- Rekor inclusion: free (sigstore.dev public; rate limit generous).
- Fulcio cert: free (sigstore.dev public).
- TUF metadata fetch: bounded ~10 KiB per CLI run.
- Customer CLI verify: free (CLI distributed via cargo install).
- **Total custo direto S-12 WI-001**: ~$0.024 × 30 releases/mês ≈ $0.72/mês = $9/yr. Negligível vs alternative long-lived key infra ($1k+/yr).

## 23. API Contract

`SlsaProvenanceVerifier` é internal Rust trait + CLI binary. CLI semver stable post v1.0 (workspace constraint); breaking changes em CLI args = bump major + migration note.

CLI output stable JSON (machine-parseable) ou human-readable text (via `--format`). Erro mapping:
- `BuilderMismatch | RekorInclusionInvalid | FulcioChainInvalid | InTotoSchemaInvalid` → exit code 1 (verify failed).
- `AttestationExpired` → exit code 2 (renewal needed).
- IO errors → exit code 3.

## 24. Post-mortem Hooks

- Bypass de provenance verify (forge accepted maliciously) → CRITICAL post-mortem + breach notification consideration.
- Rekor outage > 24h → SEV-2 + ops post-mortem (validate fallback runbook).
- Fulcio rotation breaks verify > 4h sustained → SEV-2 + post-mortem.
- Hermetic build violation undetected em SLSA generator → SEV-1 (CVE territory).
- TUF metadata poisoning detected → CRITICAL + Security incident.

5-Why mandatory em todos os SEV-0/SEV-1.

## 25. Rollback / Recovery

- Workflow rollback: revert PR de `release-slsa3.yml`; re-run anterior workflow.
- CLI rollback: customers usam `cargo install corelink-supply-verify --version <prev>`.
- Cache flush: `~/.sigstore/` directory delete em customer machine.
- RTO: ≤ 30 min (workflow revert + redeploy).
- RPO: 0 (stateless; no data loss possível; old releases retain valid attestation).

Fallback degradation: se Rekor down sustained, release publication blocked (intentional; document em runbook fallback). No graceful "skip Rekor" fallback (security property).

## 26. Security & Privacy

**STRIDE delta**:
- **Spoofing**: Fulcio cert SAN binding + Rekor inclusion proof previne forge.
- **Tampering**: SLSA L3 hermetic builder + reproducible builds 2-runner diff (WI-S12-006) catch tampering.
- **Repudiation**: Rekor transparency log = forensic evidence; cannot deny release origin.
- **Info disclosure**: provenance metadata é public (commit SHA, workflow ref); acceptable for OSS substrate.
- **DoS**: SLSA generator failure = build blocked (graceful; no unsafe state).
- **Elevation of privilege**: keyless OIDC = no long-lived secrets; minimal GitHub Actions permissions; OIDC identity bound to specific workflow.

**LINDDUN delta**:
- **Linkability**: Rekor publicly searchable; release identity (commit SHA + workflow ref) is public knowledge.
- **Identifiability**: builder identity é GitHub Actions service principal (não personal identity).
- **Non-repudiation**: cripto property intentional.
- **Detectability**: dependent CVEs publicly known; SBOM (WI-S12-002) exposes dep list.
- **Disclosure of information**: vendored patches em `[patch.crates-io]` requerem ADR + Security review.
- **Unawareness**: customer-facing supply chain posture documented.
- **Non-compliance**: SOC 2 CC6.7 + EO 14028 + NIST SSDF satisfied.

## 27. Knowledge Transfer

- `crates/corelink-supply-verify/README.md` — overview + integration pattern para customers.
- ADR-XXXX — SLSA L3 + Rekor mandatory ratification.
- Doc `docs/internal/slsa-l3-pipeline.md` — sequence diagram release → SLSA generator → Fulcio → Rekor → release asset.
- Workshop interno (1h) com Crypto SME + AppSec pós-merge.
- Customer-facing blog post (post-GA) anuncia SLSA L3 posture.

## 28. Risk Register (6-col)

| ID | Risco | Prob | Det | Impacto | Exposure | Residual | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | SLSA generator regression em version bump | L | H | CRITICAL | M | LOW | Pin v1.10.0 + ADR antes bump + adversarial test |
| R-002 | Fulcio root cert rotation breaks verify | L | M | HIGH | L | LOW | TUF auto-refresh + chaos test |
| R-003 | Rekor outage > 24h | L | M | HIGH (operational) | L | LOW | Document fallback runbook + 24h grace period |
| R-004 | Workflow yaml tampering pré-trigger | L | M | CRITICAL | M | LOW | CODEOWNERS + tag protection + signed commits + Fulcio SAN includes workflow SHA1 |
| R-005 | Hermetic build violation undetected | L | L | HIGH | L | LOW | SLSA generator validates `builder.buildType`; cargo offline mode |
| R-006 | Customer CLI verify drift (old version) | M | L | MEDIUM | L | LOW | CLI semver stable + deprecation warnings + cargo audit |
| R-007 | TUF metadata poisoning | L | L | CRITICAL | L | LOW | sigstore.dev pinned; `--tuf-mirror` flag explicit (não default) |
| R-008 | Latency regression em CLI verify | L | M | LOW | L | LOW | Cost regression gate + bench |
| R-009 | In-toto schema migration drift | M | L | MEDIUM | L | LOW | `expected_schema_version` arg + ADR antes upgrade |
| R-010 | DSSE alg=none regression em sigstore-rs | L | H | CRITICAL | L | LOW | Property test + cargo-audit + version pin |

## 29. Review Checkpoints

1. **Design (D+0)**: Architect review workflow shape + ADR-XXXX outline.
2. **Crypto (D+1)**: Crypto SME review Fulcio chain + Rekor inclusion proof logic; pair-program adversarial tests.
3. **Code (D+2)**: peer review (folded into Engineer + Architect per ADR-0034).
4. **AppSec (D+2)**: AppSec review workflow yaml + GitHub Actions OIDC permissions + tag protection posture.
5. **Adversarial (pre-merge D+3)**: red team session — forge fork attestation, Rekor tampering, TUF poisoning.
6. **PRR mini (D+3)**: Architect sign-off + readiness review (gates inclusion em WI-S12-007 ship gate).

## 30. Sign-off (HIGH_RISK 11 canonical)

| # | Role | Name | Signed Date | Status |
|---|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | _pending_ | _pending_ |
| 2 | Final Approver | Gustavo Schneiter | _pending_ | _pending_ |
| 3 | Architect | _TBD; emphatic — Crypto SME specialization mandatory (Fulcio chain validation + Rekor inclusion proof + in-toto v1.0 schema + 5 CVE-class adversarial regressions)_ | _pending_ | _pending_ |
| 4 | Security Lead | _TBD; emphatic — provenance attestation boundary review_ | _pending_ | _pending_ |
| 5 | SRE Lead | _staffing-blocked_ | _pending_ | _pending_ |
| 6 | Engineer (S-12 lead) | _TBD_ | _pending_ | _pending_ |
| 7 | QA Lead | _TBD_ | _pending_ | _pending_ |
| 8 | Product | Gustavo Schneiter | _pending_ | _pending_ |
| 9 | Compliance Officer | _TBD; emphatic — SOC 2 CC6.7 + EO 14028 evidence pack_ | _pending_ | _pending_ |
| 10 | Privacy Officer | _TBD_ | _pending_ | _pending_ |
| 11 | AppSec advisor | _TBD; emphatic — workflow yaml threat model + GitHub Actions OIDC permissions + tag protection posture_ | _pending_ | _pending_ |

> Crypto SME (Fulcio chain + Rekor inclusion + in-toto schema + adversarial attestation forge) folds into Architect role specialization mandatory. Peer reviewers contribuem em PR review sem sign-off canonical separado (folded into Engineer + Architect per framework §33.5.4.3 + ADR-0034 solo-tier waiver).

## 31. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-29 | Gustavo (via Claude Opus 4.7) | Criação WI-S12-001 (cycle 11.S12.0). |
| 1.0.1 | 2026-05-13 | Gustavo (via Claude Sonnet 4.6) | Implementation SEALED: release-slsa3.yml + corelink-supply-verify crate + ADR-0045 + docs. |

## 32. Anti-patterns evitados

- ❌ Custom SLSA generator (use `slsa-github-generator` audited).
- ❌ Long-lived signing keys (Cosign keyless OIDC only).
- ❌ Rekor optional / "best-effort" (mandatory; INV-SUPPLY-PROVENANCE-IN-REKOR enforces).
- ❌ Self-hosted Rekor instance pré-GA (use sigstore.dev public).
- ❌ Skip in-toto schema validation (regression class XZ Utils 2024).
- ❌ Skip Fulcio root cert pinning (TUF rotation handles upgrades).
- ❌ Workflow yaml inline em release branch (CODEOWNERS + tag protection).
- ❌ Provenance attestation só para tagged releases (also nightly canary).
- ❌ "Provenance optional via flag" feature (anti-pattern; never).
- ❌ Graceful Rekor outage bypass (no `--skip-rekor` flag; security property).
- ❌ Customer-side verify optional (defense in depth; CLI distributed).

---

**Fim WI-S12-001.** Próximo: WI-S12-002 (SBOM CycloneDX 1.5+ + Dependency-Track ingestion + NTIA minimum elements check).
