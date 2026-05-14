---
id: "WI-S12-002"
type: "work_item"
doc_status: "SEALED"
work_status: "DONE"
audit_status: "AUDITED"
version: "1.1.0"
created: "2026-04-29"
updated: "2026-05-14"
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
  - "COMPLIANCE-MATRIX"
  - "OBSERVABILITY-MODEL"
  - "FAILURE-MODES"
tags: ["wi", "s12", "supply-chain", "sbom", "cyclonedx", "ntia", "rfc-3161", "tsa", "dependency-track", "high-risk"]
---

# WI-S12-002 — SBOM CycloneDX 1.5+ Generation via `cargo-cyclonedx` + NTIA Minimum Elements Check + RFC 3161 TSA Timestamp + Dependency-Track Ingestion API

> **doc_status:** SEALED · **work_status:** DONE · **lane:** HIGH_RISK
> **Parent:** [S-12](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S12-002 |
| Título | SBOM CycloneDX 1.5+ generation via `cargo-cyclonedx` em build pipeline; SBOM publicada como GitHub release asset + ingerida em Dependency-Track via API + NTIA minimum elements check via `cyclonedx-cli validate --minimum-required-fields ntia` + RFC 3161 timestamp attested via Sigstore TSA (`tsa.sigstore.dev`); EVT-010 evidence pack para CTRL-SUPPLY-003 |
| Sprint | S-12 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-005 (controles supply chain — bypass = blast radius global) |

## 1. Intent

Implementar `.github/workflows/sbom.yml` + `tools/sbom-publish/` que gera **SBOM CycloneDX 1.5+** JSON formato standard via `cargo-cyclonedx` em cada release, valida NTIA minimum elements compliance via `cyclonedx-cli validate --minimum-required-fields ntia`, attesta RFC 3161 timestamp via Sigstore TSA (`tsa.sigstore.dev`), publica como GitHub release asset (`sbom.cdx.json` + `sbom.cdx.json.tsr`), e ingere em Dependency-Track v4.11+ (provisionado WI-S12-005) para continuous CVE matching. Foundation que WI-S12-005 consome para CVE alerting webhook ≤ 15 min p99.

```rust
// File: tools/sbom-publish/src/lib.rs

#![forbid(unsafe_code)]

use async_trait::async_trait;

#[async_trait]
pub trait SbomPublisher: Send + Sync {
    /// Generate SBOM CycloneDX 1.5+ from Cargo.lock via cargo-cyclonedx.
    /// Validates NTIA minimum elements; attests RFC 3161 timestamp.
    async fn generate(
        &self,
        cargo_lock_path: &Path,
        cargo_toml_path: &Path,
        release_metadata: &ReleaseMetadata,
    ) -> Result<SignedSbom, SbomError>;

    /// Ingest SBOM em Dependency-Track via API v1; returns project_uuid for tracking.
    async fn ingest_dt(
        &self,
        sbom: &SignedSbom,
        dt_endpoint: &Url,
        api_key: &DtApiKey,
    ) -> Result<DtProjectUuid, SbomError>;

    /// Validate NTIA minimum elements per ntia.doc.gov spec.
    async fn validate_ntia(&self, sbom: &Sbom) -> Result<NtiaValidation, SbomError>;
}

#[derive(Debug, Clone)]
pub struct SignedSbom {
    pub sbom: Sbom,                              // CycloneDX 1.5+ struct
    pub format: SbomFormat,                      // CycloneDX_1_5 | CycloneDX_1_6
    pub spec_version: String,                    // "1.5" | "1.6"
    pub serial_number: String,                   // urn:uuid:... (unique per release)
    pub timestamp_rfc3339: String,               // ISO 8601
    pub tsr_token: TsrToken,                     // RFC 3161 TimeStampResp
    pub component_count: u32,
    pub ntia_compliant: bool,
}

#[derive(Debug, Clone)]
pub struct NtiaValidation {
    pub author_present: bool,                    // NTIA: SBOM author
    pub timestamp_present: bool,
    pub component_supplier_present: f32,         // % de components com supplier
    pub component_name_present: f32,
    pub component_version_present: f32,
    pub component_unique_id_present: f32,        // PURL ou CPE
    pub dependency_relationships_present: f32,
    pub overall_compliant: bool,                 // todos os 7 campos NTIA satisfeitos
}

#[derive(Debug, thiserror::Error)]
pub enum SbomError {
    #[error("cargo-cyclonedx execution failed: {0}")]
    GenerationFailed(String),
    #[error("NTIA validation failed: {0:?}")]
    NtiaValidationFailed(NtiaValidation),
    #[error("RFC 3161 TSA timestamp request failed: {0}")]
    TsaRequestFailed(String),
    #[error("Dependency-Track ingestion failed: HTTP {status}: {body}")]
    DtIngestionFailed { status: u16, body: String },
    #[error("CycloneDX schema validation failed: {0}")]
    SchemaInvalid(String),
}
```

Workflow: `cargo cyclonedx` → `cyclonedx-cli validate` (NTIA) → `curl tsa.sigstore.dev` (TSA timestamp) → `dtrack-api ingest` → upload release asset.

## 2. Narrative (HIGH_RISK ≥ 300 palavras + risk justification)

SBOM (Software Bill of Materials) é **regulatory baseline** moderno: Executive Order 14028 (May 2021) torna SBOM mandatory para US federal contracts; NTIA minimum elements (Jul 2021) define schema canônico; SOC 2 CC7.1 + ISO 27001 A.8.30 exigem dependency tracking; LGPD Art. 38 (registro de operações) inclui dependency disclosure. Sem SBOM publicada + ingerida em CVE matching system, CoreLink **não é compliance-grade** para enterprise/fed prospects.

**CycloneDX 1.5+ vs SPDX**: ambos NTIA-compliant; CycloneDX é mais expressivo para dependency relationships + license metadata; SPDX é mais maduro para legal compliance. CoreLink target **CycloneDX 1.5+** alinhado com Chainguard/Distroless industry pattern; Sigstore Cosign attaches CycloneDX bundles; ecosystem Rust (`cargo-cyclonedx`) é first-class. Future: dual-format (CycloneDX + SPDX) se enterprise customer demand materializar (S-20 GA hardening).

**Bugs catastróficos possíveis** (todos endereçados):

1. **SBOM incompleto** (falta deps transitivas): `cargo-cyclonedx` modo `--all` força inclusão de todas as deps (direct + transitive); validation step counts components vs `cargo metadata --format-version 1` output e fails se mismatch > 0%.

2. **NTIA minimum elements miss**: SBOM gerada mas falha NTIA check (e.g., supplier missing em 30% components). Mitigação: `cyclonedx-cli validate --minimum-required-fields ntia` em CI; release gate; PRR check.

3. **Stale SBOM** (gerada mas não regerada após dep update): `Cargo.lock` muda mas SBOM publicada é antiga; CVE matching desatualizado. Mitigação: SBOM regenerada em **toda release** (não só major); SBOM serial_number includes commit SHA + timestamp.

4. **Tampering pós-publicação**: SBOM publicada mas atacante substitui em mirror/CDN; CVE matching usa SBOM tampered. Mitigação: RFC 3161 TSA timestamp via Sigstore TSA = cripto evidence de timestamp (auditor pode verificar); SBOM hash referenciada em SLSA provenance attestation (WI-S12-001) — tampering detectável via attestation chain.

5. **Dependency-Track outage**: SBOM gerada mas DT API down; ingestion fails; CVE matching loses freshness. Mitigação: retry policy 3 tentativas com exponential backoff; fallback queue (S3-equivalent CF KV) para batch ingestion após recovery; alert SEV-3 se DT down > 4h.

6. **PURL malformation**: cargo-cyclonedx emite PURL `pkg:cargo/<name>@<version>`; DT espera `pkg:crates/<name>@<version>` (ecosystem prefix mismatch). Mitigação: post-process step normalizes PURL ecosystem; integration test com DT; ADR documenta canonical PURL format.

7. **Vendored deps em `[patch.crates-io]`**: dep patched localmente; PURL aponta para crates.io original; CVE matching falsamente positive (CVE em upstream vs version local fix). Mitigação: ADR-XXXX requer `[patch.crates-io]` entries + Security review; SBOM custom annotation `cdx:patched_locally=true` per component.

8. **TSA replay attack**: atacante captura TSR token + replays para SBOM antiga. Mitigação: TSR token includes nonce + SBOM hash; verify rejects mismatched hash; INV-SUPPLY-PROVENANCE-IN-REKOR cobre via attestation chain.

**Atacante adversarial scenarios**:

- **Dep injection via SBOM tampering**: attacker substitui SBOM em mirror; injects malicious dep não presente em real Cargo.lock; DT consome → false negative CVE matching para real malicious dep. Mitigação: SBOM hash referenciada em SLSA provenance attestation (WI-S12-001 chain) — tampering detected via chain validate.

- **NTIA bypass**: attacker generates SBOM com NTIA fields presentes mas conteúdo bogus (e.g., supplier="UNKNOWN"). Mitigação: NTIA validation strict mode rejects placeholders; QA review SBOM samples.

- **PURL confusion**: attacker registra crate `corelink-fake` em crates.io; SBOM lists vs real CoreLink workspace member. Mitigação: workspace members emit local PURL `pkg:cargo/corelink-worker@0.X.Y?vcs_url=...` distinguishable.

**Risk justification HIGH_RISK**:

- **FF-HR-005**: SBOM incompleto/tampered = CVE matching cego = THR-T-006 unmitigated.
- **Reversibility**: SBOM stale > 30d sustentado = compliance gap permanente até remediation cycle.

11 sign-offs canonical incl. Compliance Officer (NTIA + EO 14028 evidence) + AppSec (SBOM tampering threat model).

## 3. Customer Impact & Journey

**Persona 1 — Procurement officer em prospect enterprise (RFP)**:
- Customer download `https://github.com/humangr-labs/corelink-server/releases/v0.X.Y/sbom.cdx.json` + `sbom.cdx.json.tsr`.
- Customer runs `cyclonedx-cli validate --input-file sbom.cdx.json --minimum-required-fields ntia` → "✅ NTIA compliant".
- Customer runs `openssl ts -verify -in sbom.cdx.json.tsr -data sbom.cdx.json -CAfile sigstore-tsa-root.pem` → timestamp validated.
- Diferenciador: CycloneDX 1.5+ + RFC 3161 TSA + Dependency-Track integration = enterprise-grade vs OSS competitors com SBOM "best-effort".

**Persona 2 — Auditor SOC 2 / ISO 27001**:
- Audit query: "Show me 100% releases últimos 30d com SBOM publicada + DT ingestion verified".
- Evidence: GitHub releases page + DT project tracking + CVE alert log;
- Compliance Matrix mapping: SOC 2 CC7.1 (vulnerability detection) + EO 14028 (SBOM mandatory).

**Persona 3 — Internal security engineer**:
- Dashboard DASH-SUPPLY: SBOM ingestion success rate, DT CVE alert rate, NTIA validation success ratio.
- Slack channel `#supply-chain-cve-alerts` recebe alerts continuous.

**SLA addendum**:
- SBOM generation latency: ≤ 1 min adicionais por release.
- NTIA validation: ≤ 30s.
- TSA timestamp request: ≤ 10s p99.
- DT ingestion: ≤ 30s p99 (com retry); SLA 99.9% após DT live (WI-S12-005).

## 4. Capability Mapping

- **CAP-SUPPLY-002** (SBOM CycloneDX 1.5+) — IMPLEMENTA primary.
- **CAP-SUPPLY-006** (Dependency-Track integration) — IMPLEMENTA partial (ingestion API; full DT setup em WI-S12-005).
- Trace: `_spec_contract.md §4 + §5.2` + `security_model.md §11.4 (CTRL-SUPPLY-003)` + `compliance_matrix.md §3.6 (SOC 2 CC7.1 + EO 14028 + NIST SSDF)`.

## 5. Tipo

Build pipeline + compliance evidence; HIGH_RISK; FF-HR-005.

## 6. Escopo

### 6.1 In-scope

1. **`.github/workflows/sbom.yml`** workflow novo:
   - Trigger: `release: { types: [published] }` + manual `workflow_dispatch` + cron weekly drift check.
   - Job 1: install `cargo-cyclonedx` + `cyclonedx-cli` + setup Rust toolchain pinned (`rust-toolchain.toml`).
   - Job 2: `cargo cyclonedx --all --format json --spec-version 1.5` → output `sbom.cdx.json`.
   - Job 3: `cyclonedx-cli validate --input-file sbom.cdx.json --minimum-required-fields ntia` (gate).
   - Job 4: TSA timestamp request (`curl -X POST -H "Content-Type: application/timestamp-query" --data-binary @sbom.cdx.json.tsq tsa.sigstore.dev/api/v1/timestamp` → `sbom.cdx.json.tsr`).
   - Job 5: Dependency-Track ingestion via API v1 (`POST /api/v1/bom`).
   - Job 6: upload `sbom.cdx.json` + `sbom.cdx.json.tsr` como release assets.
   - Permissions: `contents: write` (release upload).
2. **`tools/sbom-publish/`** Rust binary:
   - Cargo.toml: deps `serde`, `serde_json`, `reqwest`, `tokio`, `clap = "4"`, `thiserror`, `tracing`, `cyclonedx-bom = "0.6"` (Rust crate audited).
   - Subcommands:
     - `generate --cargo-lock <path> --cargo-toml <path> --output <path>` — invoke cargo-cyclonedx (subprocess) + post-process PURL normalization.
     - `validate-ntia --input <path>` — NTIA minimum elements check (exit 0 = compliant; exit 1 = fail).
     - `attest-tsa --input <path> --tsa-url <url> --output-tsr <path>` — RFC 3161 TimeStampReq + verify response.
     - `ingest-dt --input <path> --dt-url <url> --api-key-env <var> --project-name <name>` — Dependency-Track API ingestion.
3. **NTIA minimum elements implementation** (per ntia.doc.gov spec):
   - 7 minimum fields validated:
     - **Author of SBOM data**: `metadata.authors[*].name` populated.
     - **Timestamp**: `metadata.timestamp` ISO 8601 within ≤ 24h.
     - **Component name**: `components[*].name` 100%.
     - **Component version**: `components[*].version` 100%.
     - **Component supplier**: `components[*].supplier.name` ≥ 95% (allow lib internas).
     - **Unique identifier**: `components[*].purl` OR `components[*].cpe` 100%.
     - **Dependency relationships**: `dependencies[*]` populated.
   - Strict mode (default): 100% threshold; fail-fast.
   - Auditor mode (`--auditor`): emit detailed report + soft warnings.
4. **PURL normalization**:
   - cargo-cyclonedx default: `pkg:cargo/<name>@<version>`.
   - Post-process: emit dual `purl` field — primary `pkg:cargo/<name>@<version>` + alias `pkg:crates/<name>@<version>` (DT compatibility).
   - Workspace members: append `?vcs_url=https://github.com/humangr-labs/corelink-server` discriminator.
5. **RFC 3161 TSA timestamp** via Sigstore TSA (`tsa.sigstore.dev/api/v1/timestamp`):
   - Compute SHA-256 hash de SBOM JSON.
   - Build TimeStampReq (DER-encoded): nonce + hash + algorithm OID.
   - POST to TSA endpoint; receive TimeStampResp.
   - Verify response: TSA cert chain valid, signature over hash valid, timestamp within ±60s of NOW.
   - Persist `.tsr` file alongside SBOM.
6. **Dependency-Track ingestion API** (DT v4.11+ REST):
   - Endpoint: `POST /api/v1/bom` (multipart form: bom file + projectName + projectVersion + autoCreate=true).
   - Authentication: API key via env `DT_API_KEY`.
   - Retry: 3 attempts exponential backoff (1s, 4s, 16s); fallback queue em CF KV se all fail.
   - Returns: project_uuid + processing_token; persist em release artifact pack.
7. **Métricas underscored Prometheus** (per observability_model.md §4.1):
   - `corelink_supply_sbom_generation_duration_seconds_bucket` (histogram p50/p95/p99).
   - `corelink_supply_sbom_components_count_gauge` (snapshot per release).
   - `corelink_supply_sbom_ntia_compliant_total{outcome}` (outcome ∈ ok|fail|warning).
   - `corelink_supply_dt_ingestion_total{outcome}` (outcome ∈ ok|api_error|validation_failed|retry_exhausted).
   - `corelink_supply_tsa_timestamp_total{outcome}` (outcome ∈ ok|tsa_unavailable|verify_failed).
8. **Observability** — trace span `sbom.generate` + `sbom.validate_ntia` + `sbom.attest_tsa` + `sbom.ingest_dt` com attributes:
   - `sbom.format` (CycloneDX_1_5 | CycloneDX_1_6).
   - `sbom.component_count` (u32).
   - `sbom.ntia_compliant` (bool).
   - `result` (enum).
9. **Property tests** (10k iter PR + 100k iter nightly):
   - `prop_sbom_ntia_field_missing_rejected`: 10k random SBOMs com 1+ NTIA fields missing; assert 100% rejected em strict mode.
   - `prop_sbom_purl_malformed_rejected`: 10k mutated PURL strings; assert canonical normalization or rejection.
   - `prop_sbom_component_count_consistent`: 10k SBOMs comparados com `cargo metadata` ground truth; assert match.
   - `prop_sbom_tsa_replay_rejected`: 10k mutated TSR tokens (different SBOM hash); assert rejection.
10. **Adversarial regression tests**:
    - SBOM tampered post-generation (hash mismatch vs SLSA attestation) → flagged.
    - NTIA placeholder values ("UNKNOWN" supplier) → rejected em strict mode.
    - DT ingestion 503 + retry exhausted → fallback queue + alert SEV-3.
    - TSA replay with different SBOM hash → rejected.
    - PURL confusion (`pkg:cargo/corelink-fake@1.0.0`) → workspace member discriminator catches.
11. **Integration test E2E**:
    - Real release workflow trigger em staging fork; verify SBOM generated + NTIA pass + TSA token + DT ingestion.

### 6.2 Out-of-scope (deferred)

- **SLSA L3 provenance attestation**: WI-S12-001 (parallel; SBOM hash referenced em provenance).
- **Cosign sign release artifacts** + **CF deploy verify**: WI-S12-003.
- **cargo-audit + cargo-deny + Dependabot**: WI-S12-004.
- **Dependency-Track self-host setup**: WI-S12-005 (este WI provê ingestion API client; WI-S12-005 provisiona DT).
- **Reproducible builds**: WI-S12-006.
- **SPDX dual-format**: pós-GA enterprise se demand materializar.
- **VEX (Vulnerability Exploitability eXchange)**: pós-GA Q3 (CycloneDX 1.6 supports; CoreLink starts CycloneDX 1.5).
- **License compliance reporting**: WI-S12-004 cargo-deny enforce; este WI emit license metadata em SBOM apenas.

## 7. Anti-Scope

- ❌ Hand-roll SBOM generator (use `cargo-cyclonedx` + `cyclonedx-bom` audited).
- ❌ NTIA validation lax mode em production (strict mode mandatory).
- ❌ Skip RFC 3161 TSA timestamp (SBOM tampering detect dependent).
- ❌ DT ingestion optional (CVE matching dependent).
- ❌ "SBOM optional via flag" feature (anti-pattern; never).
- ❌ Self-hosted TSA pré-GA (use sigstore.dev `tsa.sigstore.dev` public).
- ❌ SBOM published only para tagged releases (also nightly canary builds).
- ❌ PURL ecosystem confusion ignored (normalize required).
- ❌ Vendored patches em SBOM sem custom annotation (false-positive CVE risk).
- ❌ Stale SBOM (regenerada em toda release).

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: SBOM CycloneDX 1.5+ generation + NTIA + TSA + DT ingestion

  Background:
    Given .github/workflows/sbom.yml configured
    And cargo-cyclonedx pinned at 0.5.x
    And cyclonedx-cli pinned at 0.27.x
    And TSA endpoint = "https://tsa.sigstore.dev/api/v1/timestamp"
    And DT endpoint = "https://dt.corelink.dev/api/v1/bom"

  Scenario: Release triggers SBOM generation pipeline
    Given a release tag v0.X.Y published
    When sbom.yml workflow runs
    Then cargo-cyclonedx produces sbom.cdx.json (CycloneDX 1.5+ JSON)
    And cyclonedx-cli validate NTIA returns success
    And TSA timestamp request succeeds; sbom.cdx.json.tsr produced
    And DT ingestion via POST /api/v1/bom succeeds; project_uuid returned
    And sbom.cdx.json + sbom.cdx.json.tsr attached to release as assets
    And metric corelink_supply_dt_ingestion_total{outcome="ok"} incremented

  Scenario: NTIA validation fails — missing supplier in 5% components
    Given SBOM generated with components[*].supplier.name missing in 5% components
    When validate-ntia strict mode runs
    Then NtiaValidation.component_supplier_present = 0.95 < 0.95 threshold (strict)
    And exit code 1
    And release publication BLOCKED
    And metric corelink_supply_sbom_ntia_compliant_total{outcome="fail"} incremented

  Scenario: TSA timestamp replay attempt
    Given attacker captures TSR token for SBOM_v0.X.Y
    When attacker injects TSR for SBOM_v0.X.(Y+1) (different hash)
    Then verify-tsa step detects hash mismatch
    And exit code 1
    And alert SEV-3 fires

  Scenario: Dependency-Track ingestion retry
    Given DT API returns 503 on first attempt
    When sbom-publish ingest-dt runs
    Then 3 retries with exponential backoff (1s, 4s, 16s)
    Then on 3rd retry success, returns project_uuid
    And metric corelink_supply_dt_ingestion_total{outcome="ok"} incremented after retry

  Scenario: DT ingestion exhausted retries
    Given DT API returns 503 on all 3 attempts
    When sbom-publish ingest-dt runs
    Then SBOM saved to fallback queue em CF KV
    And alert SEV-3 fires
    And metric corelink_supply_dt_ingestion_total{outcome="retry_exhausted"} incremented
    And release publication CONTINUES (SBOM published as asset; DT ingestion async retry)

  Scenario: PURL normalization workspace members
    Given workspace member "corelink-worker" version 0.X.Y
    When SBOM generated
    Then component PURL = "pkg:cargo/corelink-worker@0.X.Y?vcs_url=https://github.com/humangr-labs/corelink-server"
    And aliased PURL = "pkg:crates/corelink-worker@0.X.Y" for DT compatibility

  Scenario: Vendored dep with [patch.crates-io] flagged
    Given dep "ring" patched locally via [patch.crates-io]
    When SBOM generated
    Then component "ring" emits cdx:patched_locally = true annotation
    And auditor mode emits warning: "Local patch; CVE matching may yield false positives"

  Scenario: SBOM hash referenced em SLSA provenance
    Given SLSA L3 attestation generated (WI-S12-001) for v0.X.Y
    When attestation predicate inspected
    Then materials section includes SBOM hash + URL (release asset)
    And tampering em SBOM post-publish detectable via attestation chain

  Scenario: Property test 100k iter green
    Given prop_sbom_ntia_field_missing_rejected 100k iter
    When test runs nightly
    Then 0 false-accepts
    And 0 panics

  Scenario: SLA latency
    Given SBOM generation pipeline em staging
    When measured over 30 releases
    Then p99 ≤ 1 min adicional (cargo-cyclonedx ~20s + validate ~10s + TSA ~5s + DT ingest ~10s)
```

## 9. Design Decisions

### 9.1 Why CycloneDX 1.5+ (não SPDX)

- CycloneDX é mais expressivo para dependency relationships + license metadata.
- Sigstore Cosign attaches CycloneDX bundles natively.
- Rust ecosystem `cargo-cyclonedx` é first-class; SPDX support menor.
- Industry pattern: Chainguard/Distroless/Wolfi all CycloneDX.
- Future: dual-format (SPDX) se enterprise demand (S-20 GA hardening).

### 9.2 Why NTIA strict mode (não auditor mode default)

- Strict mode: 100% threshold per field; fail-fast em CI gate.
- Auditor mode: detailed report + soft warnings; useful para debug, não para production gate.
- Compliance evidence (SOC 2/ISO 27001) requer strict mode.

### 9.3 Why RFC 3161 TSA via Sigstore (não custom TSA)

- Sigstore TSA é public, free, audited.
- RFC 3161 standard; openssl `ts -verify` works out-of-box (customer-side).
- Self-hosted TSA = operational overhead + trust anchor management; not justified pré-GA.

### 9.4 Why Dependency-Track (não Snyk/GitHub Advanced Security)

- Self-hostable (data residency control).
- Open-source (não vendor lock-in).
- CycloneDX ingestion native.
- Mature CVE matching engine (NVD + OSV + GHSA).
- Pricing: $0 OSS vs Snyk $$$/dev.

Alternativa rejeitada: GitHub Advanced Security (acceptable but vendor lock-in; defer pós-GA decision).

### 9.5 Why SBOM regenerated per release (não cached)

- Cargo.lock pode mudar entre releases (transitive updates via Dependabot).
- Stale SBOM = CVE matching desatualizado → false negative.
- Generation cost: ~20s; aceitável.

### 9.6 Why workspace member PURL discriminator (`?vcs_url=...`)

- Default `pkg:cargo/<name>@<version>` colide com hypothetical malicious crate `corelink-worker` em crates.io.
- Discriminator unique-identifies workspace members.
- DT compatible (DT parses PURL qualifiers).

### 9.7 Why fallback queue em DT outage (não block release)

- DT outage não-cripto-breaking (CVE matching delayed, não bypass).
- Block release = operational drag.
- Trade-off: alert SEV-3 + async retry + dashboard panel.

### 9.8 Why CycloneDX 1.5+ (não 1.6)

- 1.5 stable + ratified Mar 2024; widespread tooling support.
- 1.6 (Apr 2024) adds VEX + ML transparency; tooling immature.
- CoreLink targets 1.5 baseline; 1.6 upgrade via ADR pós-GA.

### 9.9 Why cargo-cyclonedx pinned at 0.5.x (não @latest)

- Reproducible build property: pinned tooling = same SBOM output.
- ADR upgrade cadence: bump via ADR with regression testing.

### 9.10 ADR potencial?

- Sim — **ADR-XXXX**: "SBOM CycloneDX 1.5+ NTIA strict + RFC 3161 TSA + DT ingestion". Pattern reusable em S-13 admin plane (admin op SBOM) + S-20 GA hardening.
- Whitelist em `validate_references.py` até materializar.

## 10. Completeness Criteria SOTA

- [ ] **10.s12.002.1** Property test 10k iter (PR) + 100k iter (nightly) sobre fuzz SBOM inputs → 0 panics, 0 false-accepts (EVT-002).
- [ ] **10.s12.002.2** Adversarial test: 5 scenarios (tampered, NTIA placeholder, DT exhausted retry, TSA replay, PURL confusion) — 100% mitigated (EVT-040).
- [ ] **10.s12.002.3** E2E test contra staging fork: release → SBOM → NTIA pass → TSA → DT ingest; latência ≤ 1 min adicional p99 (EVT-018).
- [ ] **10.s12.002.4** SBOM publicada para 100% releases pós-S-12 últimos 30d como GitHub release asset (EVT-010).
- [ ] **10.s12.002.5** SAST clean (cargo-audit + cargo-deny + clippy `-D warnings`) em `tools/sbom-publish` crate (EVT-002).
- [ ] **10.s12.002.6** Cost regression gate: SBOM job ≤ 1 min adicional; CI cost ≤ $0.05 per release adicional (Lote 9.4 §14.10).
- [ ] **10.s12.002.7** OWASP ASVS V14 + SSDF PS.1 + EO 14028 SBOM compliance 100% checklist (EVT-002).
- [ ] **10.s12.002.8** Dependency-Track ingestion verified com 1 mock CVE (provided by WI-S12-005) → DT detects + alerts (EVT-021).
- [ ] **10.s12.002.9** SBOM hash referenced em SLSA provenance attestation (chain integrity) (EVT-027).
- [ ] **10.s12.002.10** INV-SUPPLY-SBOM-PRESENT enforced em CI; release sem SBOM blocked (EVT-022).

## 11. DoD

- [ ] `.github/workflows/sbom.yml` committed + verified em staging fork.
- [ ] `tools/sbom-publish/` Rust binary compila + 4 subcommands functional.
- [ ] All Gherkin scenarios green em integration test.
- [ ] Property tests 10k green em PR + 100k green em nightly.
- [ ] E2E test contra staging green.
- [ ] Métricas emitidas (5 listadas §6.1.7).
- [ ] Trace spans 4 listed.
- [ ] rustdoc + 3 examples (generate basic, validate NTIA strict, ingest DT with retry).
- [ ] ADR-XXXX (SBOM CycloneDX + NTIA + TSA + DT) escrito.
- [ ] Code review (Compliance + AppSec).
- [ ] PRR Architect mini-sign-off.
- [ ] Cost regression gate green.

## 12. Invariants Validated

### Mantidas

- **INV-SUPPLY-SBOM-PRESENT** (HIGH — herdada): este WI implementa primary; release sem SBOM blocked via CI gate.

### Novas

- não-aplicável (este WI implementa INVs herdadas; novas INVs em WI-S12-004 license + yanked).

TLA+ alignment: não-aplicável (build-time controle).

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| SBOM workflow | `.github/workflows/sbom.yml` | YAML |
| `tools/sbom-publish` binary | `tools/sbom-publish/` | Rust binary |
| SbomPublisher impl | `tools/sbom-publish/src/publisher.rs` | Rust |
| NTIA validator | `tools/sbom-publish/src/ntia.rs` | Rust |
| RFC 3161 TSA client | `tools/sbom-publish/src/tsa.rs` | Rust |
| DT ingestion client | `tools/sbom-publish/src/dt.rs` | Rust |
| PURL normalizer | `tools/sbom-publish/src/purl.rs` | Rust |
| Property tests | `tools/sbom-publish/tests/prop_sbom.rs` | Rust |
| Adversarial tests | `tools/sbom-publish/tests/adversarial.rs` | Rust |
| E2E integration | `tests/e2e_sbom.rs` | Rust |
| ADR-XXXX (SBOM + NTIA + TSA + DT) | `specs/03_architecture/adrs/ADR-XXXX-sbom-cyclonedx-ntia-tsa-dt.md` | Markdown |
| Examples | `tools/sbom-publish/examples/` (generate.rs, validate_ntia.rs, ingest_dt_retry.rs) | Rust |

## 14. Quality Standards SOTA

- **14.s12.002.1** Zero `unsafe`; zero `unwrap` em src/ (allow em tests).
- **14.s12.002.2** rustdoc 100% public API + 3 examples.
- **14.s12.002.3** Test coverage ≥ 90% (`cargo tarpaulin`).
- **14.s12.002.4** Latência: SBOM generation p99 ≤ 1 min adicional; DT ingest p99 ≤ 30s warm.
- **14.s12.002.5** SAST: `cargo-audit` + `cargo-deny` + `clippy -D warnings` clean.
- **14.s12.002.6** Métricas RED + DT/TSA cache metrics.
- **14.s12.002.7** Runbook RB-FM-156 (dep malicious) — referenciado WI-S12-007.
- **14.s12.002.8** Breaking changes em `SbomPublisher` trait = bump major + migration note.
- **14.s12.002.9** Memory: ≤ 256 MB peak em SBOM generation (large workspaces).
- **14.s12.002.10** Cost regression gate em CI.
- **14.s12.002.11** SBOM tooling pinned (`cargo-cyclonedx` 0.5.x; `cyclonedx-cli` 0.27.x); bump via ADR.
- **14.s12.002.12** NTIA strict mode default; auditor mode opt-in.

## 15. Chaos Experiments

1. **DT outage 4h sustained**: simulate DT API 503; verify retry exhausted + fallback queue + alert SEV-3 + release continues. Hypothesis: graceful degradation; CVE matching delayed mas release published.
2. **TSA outage 1h**: simulate TSA 503; verify SBOM published sem TSR + alert SEV-3 + release continues (TSR retry-able async). Trade-off: SBOM tampering window 1h; documented em runbook.
3. **NTIA placeholder injection**: red team modifies cargo-cyclonedx output to inject `supplier.name = "UNKNOWN"` em 30% components; verify NTIA strict mode rejects.
4. **SBOM tampering pós-publication**: red team substitui SBOM em mirror; verify SLSA provenance chain detect via material hash mismatch.
5. **TSA replay attack**: red team captures TSR token + replays; verify hash mismatch detection.
6. **PURL ecosystem confusion**: synthetic SBOM com `pkg:cargo/corelink-fake@1.0.0`; verify workspace member discriminator catches.
7. **Vendored dep `[patch.crates-io]` regression**: synthetic patched dep sem ADR; verify pre-merge check + Security review trigger.
8. **DT API key rotation**: rotate `DT_API_KEY` env; verify graceful retry + alert SEV-3 if stale key.

## 16. PRR (Production Readiness Review)

PRR HIGH_RISK 11 sign-offs canonical (S-12 ship gate é WI-S12-007; este WI passa por mini-PRR):

- [ ] All Gherkin scenarios green.
- [ ] Property tests + adversarial tests green.
- [ ] E2E staging green.
- [ ] Cost regression gate green.
- [ ] Métricas + dashboards configurados em DASH-SUPPLY.
- [ ] ADR-XXXX (SBOM + NTIA + TSA + DT) published.
- [ ] Compliance review (NTIA + EO 14028 evidence).
- [ ] AppSec review (SBOM tampering threat model + TSA replay).
- [ ] Architect approval (DT integration pattern reusable em S-14 BYOK).
- [ ] OWASP ASVS V14 + SSDF PS.1 100% pass.

## 17. Sub-tasks

| ID | Sub-task | Estimativa |
|---|---|---|
| ST-001 | `.github/workflows/sbom.yml` skeleton + cargo-cyclonedx integration | 2h |
| ST-002 | `tools/sbom-publish` scaffold + Cargo.toml deps | 1h |
| ST-003 | `SbomPublisher` trait + `SignedSbom` types | 1h |
| ST-004 | NTIA validator (7 minimum elements) | 2h |
| ST-005 | PURL normalizer (workspace + ecosystem alias) | 1.5h |
| ST-006 | RFC 3161 TSA client (TimeStampReq + verify) | 2.5h |
| ST-007 | DT ingestion client + retry policy | 2h |
| ST-008 | Métricas emit (5 metrics) + trace spans | 1.5h |
| ST-009 | Property tests 10k iter (4 props) | 2.5h |
| ST-010 | Adversarial regression tests (5 scenarios) | 2h |
| ST-011 | E2E integration test contra staging | 2h |
| ST-012 | Chaos test DT outage + TSA replay | 1.5h |
| ST-013 | rustdoc + 3 examples | 1.5h |
| ST-014 | ADR-XXXX redação | 1h |
| ST-015 | Compliance + AppSec review feedback | 2h |
| ST-016 | PRR mini-sign-off | 0.5h |

**Total Optimistic**: ~26.5h. **PERT** (O=10h, M=16h, P=26h per spec contract): **16.7h**.

## 18. Dependencies

### Hard blockers

- Cargo workspace structure baseline (S-01 SEALED).
- cargo-cyclonedx + cyclonedx-cli installable via crates.io / GitHub releases.

### Soft blockers

- WI-S12-005 (Dependency-Track self-host) — full E2E integration test depends on DT operational; este WI pode usar mock DT em staging durante dev.

### Outbound

- WI-S12-001 (SLSA provenance) — paralelo; SBOM hash referenced em provenance materials.
- WI-S12-003 (Cosign + CF deploy verify) — orthogonal.
- WI-S12-005 (Dependency-Track) — consumes ingestion API client.
- WI-S12-007 (PRR ship gate) — gates S-12 close.

## 19. Effort PERT

O: 10h, M: 16h, P: 26h → PERT **16.7h** (per spec contract §12).

## 20. Time-boxing

**20h hard limit**. If exceeded → escalation: split em sub-WI (generation + NTIA vs TSA + DT).

## 21. Observability

5 métricas + 4 trace spans listadas. Logs structured JSON; nivel INFO em success, WARN em retry, ERROR em ingest fail.

Dashboard widget DASH-SUPPLY:
- SBOM generation duration p99.
- NTIA compliance ratio.
- DT ingestion success ratio.
- TSA timestamp success ratio.
- Components count trend (per release).

## 22. Cost Analysis

- SBOM generation job: ~1 min × $0.008/min = $0.008 per release.
- TSA timestamp: free (sigstore.dev).
- DT ingestion: free (self-hosted; infra cost coberto WI-S12-005).
- DT storage Postgres: ~$5/mês (Neon small) — alocado WI-S12-005.
- **Total custo direto S-12 WI-002**: ~$0.008 × 30 releases/mês ≈ $0.24/mês = $3/yr. Negligível.

## 23. API Contract

`SbomPublisher` é internal Rust trait + binary. CLI semver stable post v1.0.

CLI output JSON (machine-parseable) ou text (`--format`). Erro mapping:
- `GenerationFailed | SchemaInvalid` → exit code 1.
- `NtiaValidationFailed` → exit code 2.
- `TsaRequestFailed` → exit code 3.
- `DtIngestionFailed` → exit code 4 (com retry queue fallback).

## 24. Post-mortem Hooks

- SBOM ingestion failed + fallback queue lost → SEV-2 (CVE matching gap > 24h).
- NTIA validation regression em CI gate → SEV-3.
- TSA replay attack succeeded em production → CRITICAL post-mortem + Security incident.
- SBOM tampering pós-publish detectado → CRITICAL + 5-Why.
- DT API key leaked em logs → SEV-2 + secret rotation.

## 25. Rollback / Recovery

- Workflow rollback: revert PR de `sbom.yml`; previous SBOM retained em release assets.
- Tool rollback: cargo install previous version.
- DT ingestion fallback queue replay manual via `sbom-publish ingest-dt --replay-queue`.
- RTO: ≤ 30 min.
- RPO: 0 (SBOM regenerable from Cargo.lock; deterministic).

## 26. Security & Privacy

**STRIDE delta**:
- **Spoofing**: SBOM hash em SLSA provenance + RFC 3161 TSA = forge resistance.
- **Tampering**: tampering pós-publish detect via attestation chain.
- **Repudiation**: TSA + DT log = forensic evidence.
- **Info disclosure**: SBOM expõe deps (intentional; standard practice; OSS).
- **DoS**: DT outage graceful (fallback queue).
- **Elevation of privilege**: API key principle of least privilege; rotation cadence quarterly.

**LINDDUN delta**:
- **Linkability**: SBOM publicly known deps; OSS substrate.
- **Identifiability**: workspace member PURL discriminator.
- **Non-repudiation**: TSA timestamp = cripto evidence.
- **Detectability**: dep CVEs publicly known.
- **Disclosure**: vendored patches require ADR.
- **Unawareness**: customer-facing supply chain documentation.
- **Non-compliance**: SOC 2 CC7.1 + EO 14028 + NIST SSDF satisfied.

## 27. Knowledge Transfer

- `tools/sbom-publish/README.md` — overview.
- ADR-XXXX — design decisions.
- Doc `docs/internal/sbom-pipeline.md`.
- Workshop interno (1h) Compliance + AppSec.

## 28. Risk Register (6-col)

| ID | Risco | Prob | Det | Impacto | Exposure | Residual | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | cargo-cyclonedx regression em version bump | L | H | HIGH | M | LOW | Pin 0.5.x + ADR antes bump |
| R-002 | NTIA spec drift (new minimum elements added) | L | M | MEDIUM | L | LOW | Quarterly review NTIA spec; adapt validator |
| R-003 | DT outage > 4h | M | L | MEDIUM | M | LOW | Fallback queue + retry async + alert SEV-3 |
| R-004 | TSA outage | L | M | MEDIUM | L | LOW | SBOM published sem TSR; async retry; alert SEV-3 |
| R-005 | PURL ecosystem confusion | M | M | MEDIUM | M | LOW | Workspace discriminator + alias |
| R-006 | Vendored `[patch.crates-io]` false-positive CVE | M | L | LOW | L | LOW | Custom annotation `cdx:patched_locally` + auditor mode warning |
| R-007 | DT API key leak em logs | L | L | HIGH | L | LOW | Secret rotation + log redaction macro |
| R-008 | SBOM size > 10 MB (large workspace) | L | L | LOW | L | LOW | gzip compression em release asset |
| R-009 | TSA replay attack | L | M | HIGH | L | LOW | Hash binding em TSR + verify |
| R-010 | NTIA strict mode breaks legitimate components | L | M | MEDIUM | L | LOW | Auditor mode opt-in com waiver ADR |

## 29. Review Checkpoints

1. **Design (D+0)**: Architect review pipeline + ADR-XXXX outline.
2. **Compliance (D+1)**: Compliance Officer review NTIA + EO 14028 evidence pack.
3. **Code (D+2)**: peer review (folded into Engineer + Architect).
4. **AppSec (D+2)**: AppSec review SBOM tampering + TSA replay threat model.
5. **Adversarial (D+3)**: red team session.
6. **PRR mini (D+3)**: Architect sign-off.

## 30. Sign-off (HIGH_RISK 11 canonical)

| # | Role | Name | Signed Date | Status |
|---|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | _pending_ | _pending_ |
| 2 | Final Approver | Gustavo Schneiter | _pending_ | _pending_ |
| 3 | Architect | _TBD; emphatic — DT integration pattern reusable_ | _pending_ | _pending_ |
| 4 | Security Lead | _TBD; emphatic — SBOM tampering threat model_ | _pending_ | _pending_ |
| 5 | SRE Lead | _staffing-blocked_ | _pending_ | _pending_ |
| 6 | Engineer (S-12 lead) | _TBD_ | _pending_ | _pending_ |
| 7 | QA Lead | _TBD_ | _pending_ | _pending_ |
| 8 | Product | Gustavo Schneiter | _pending_ | _pending_ |
| 9 | Compliance Officer | _TBD; emphatic — NTIA + EO 14028 + SOC 2 CC7.1 evidence_ | _pending_ | _pending_ |
| 10 | Privacy Officer | _TBD_ | _pending_ | _pending_ |
| 11 | AppSec advisor | _TBD; emphatic — TSA replay + DT API key threat model_ | _pending_ | _pending_ |

> Crypto SME (TSA + RFC 3161 timestamp validation) folds into Architect role specialization. Peer reviewers contribuem em PR review sem sign-off canonical separado (folded into Engineer + Architect per framework §33.5.4.3 + ADR-0034).

## 31. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-29 | Gustavo (via Claude Opus 4.7) | Criação WI-S12-002 (cycle 11.S12.0). |
| 1.1.0 | 2026-05-14 | Gustavo (via Claude Sonnet 4.6) | SEALED: `tools/sbom-publish/` Rust crate (lib + bin, 7 src modules), 4 property tests, 6 adversarial scenarios, 3 examples, `.github/workflows/sbom.yml` (5 jobs, SHA-pinned), ADR-S12-001. Build + clippy clean. |

## 32. Anti-patterns evitados

- ❌ Hand-roll SBOM generator (use cargo-cyclonedx audited).
- ❌ NTIA validation lax mode em production.
- ❌ Skip RFC 3161 TSA timestamp.
- ❌ DT ingestion optional.
- ❌ "SBOM optional via flag" feature.
- ❌ Self-hosted TSA pré-GA.
- ❌ Stale SBOM (regenerada em toda release).
- ❌ PURL ecosystem confusion ignored.
- ❌ Vendored patches em SBOM sem custom annotation.
- ❌ Block release em DT outage (graceful fallback queue).
- ❌ DT API key em git/logs (env var only + rotation).

---

**Fim WI-S12-002.** Próximo: WI-S12-003 (Cosign sign release + Cloudflare deploy webhook verify gate).
