---
id: "WI-S12-003"
type: "work_item"
doc_status: "SEALED"
work_status: "DONE"
audit_status: "AUDITED"
version: "1.1.0"
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
  - "FAILURE-MODES"
  - "RESILIENCE-PATTERNS"
tags: ["wi", "s12", "supply-chain", "cosign", "sigstore", "fulcio", "rekor", "cloudflare", "deploy-verify", "hard-gate", "high-risk"]
---

# WI-S12-003 — Cosign Sign Release Artifacts (keyless OIDC) + OCI Registry Attach + Cloudflare Deploy Webhook Verifier (`workers-deploy-verifier`) com Hard Gate Non-Bypassable

> **doc_status:** SEALED · **work_status:** DONE · **lane:** HIGH_RISK
> **Parent:** [S-12](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S12-003 |
| Título | Cosign sign release artifacts (Worker WASM bundle + side artifacts) keyless via OIDC (GitHub Actions identity); signature publicada em Rekor + OCI registry attached (`ghcr.io/HumanGuardrail/corelink-worker:vX.Y.Z`); Cloudflare deploy webhook handler `corelink-deploy-verifier` verifica signature + Rekor inclusion proof + Fulcio chain antes de rollout; binário não-assinado OR sem Rekor inclusion OR com Fulcio chain inválida = rollout blocked + alert SEV-2; INV-SUPPLY-SIGNED-DEPLOY enforced; chaos test "deploy unsigned artifact" + "deploy with Rekor missing" verified |
| Sprint | S-12 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-005 (controles supply chain — bypass = blast radius global; cripto-load-bearing) |

## 1. Intent

Implementar `corelink-deploy-verifier` (Cloudflare Worker) que serve como **hard gate non-bypassable** entre release pipeline e Cloudflare Workers deploy: recebe deploy webhook → busca Cosign signature em OCI registry (`ghcr.io/HumanGuardrail/corelink-worker:vX.Y.Z` annotated com Cosign sig) → valida signature via Fulcio chain + Rekor inclusion proof → se válido, propaga deploy via Cloudflare API; se inválido, **rejeita deploy + emite alert SEV-2 + persiste audit event**. Foundation que **enforces INV-SUPPLY-SIGNED-DEPLOY** (CRITICAL). Consume provenance attestation de WI-S12-001 (Rekor lookup) + assina via Cosign keyless OIDC (zero long-lived secrets).

```rust
// File: crates/corelink-deploy-verifier/src/lib.rs

#![forbid(unsafe_code)]

use async_trait::async_trait;

#[async_trait]
pub trait DeployVerifier: Send + Sync {
    /// Verify deploy artifact signature + Rekor inclusion + Fulcio chain.
    /// Hard gate: rejects deploy if any check fails. NO bypass mode.
    async fn verify_and_propagate(
        &self,
        webhook: &CfDeployWebhook,
        cosign_image_ref: &OciImageRef,        // ghcr.io/HumanGuardrail/corelink-worker:vX.Y.Z
        expected_identity: &CosignIdentityPattern, // expected SAN URI regex
    ) -> Result<DeployPropagated, DeployVerifyError>;

    /// Audit event emit (CTRL-AUDIT-002 + INV-AUDIT-APPEND-ONLY); fail-CLOSED.
    async fn emit_audit_event(
        &self,
        event: DeployAuditEvent,
    ) -> Result<(), DeployVerifyError>;
}

#[derive(Debug, Clone)]
pub struct CfDeployWebhook {
    pub release_tag: String,                    // v0.X.Y
    pub commit_sha: String,
    pub workflow_ref: String,                   // refs/tags/v0.X.Y
    pub deploy_target: DeployTarget,            // Workers script + zone + route
    pub triggered_by: GitHubActor,              // OIDC identity (workflow run)
}

#[derive(Debug, Clone)]
pub struct DeployPropagated {
    pub release_tag: String,
    pub cosign_signature_verified: bool,
    pub rekor_log_index: u64,
    pub fulcio_cert_san: String,
    pub propagated_at: SystemTime,
    pub cf_deployment_id: String,
}

#[derive(Debug, thiserror::Error)]
pub enum DeployVerifyError {
    #[error("Cosign signature missing or invalid: {0}")]
    SignatureInvalid(String),
    #[error("Rekor inclusion proof missing for image {image}")]
    RekorMissing { image: String },
    #[error("Fulcio chain invalid: {0}")]
    FulcioChainInvalid(String),
    #[error("identity mismatch (got {got}, expected pattern {expected})")]
    IdentityMismatch { got: String, expected: String },
    #[error("OCI registry fetch failed: {0}")]
    OciFetchFailed(String),
    #[error("Cloudflare API propagation failed: {0}")]
    CfApiFailed(String),
    #[error("audit emit failed (fail-CLOSED): {0}")]
    AuditEmitFailed(String),
}

#[derive(Debug, Clone)]
pub struct DeployAuditEvent {
    pub event_type: String,                     // dev.hugr.corelink.deploy.{verified,blocked}.v1
    pub release_tag: String,
    pub outcome: VerifyOutcome,                 // ok | sig_invalid | rekor_missing | fulcio_invalid | identity_mismatch
    pub timestamp: SystemTime,
    pub trace_id: String,
}
```

Workflow: GitHub Actions release-slsa3.yml (WI-S12-001) → `cosign sign --identity-token=$OIDC_TOKEN ghcr.io/HumanGuardrail/corelink-worker:v0.X.Y` (signs image; publishes em Rekor) → triggers Cloudflare deploy webhook → `corelink-deploy-verifier` Worker validates → if pass, propagate to `wrangler deploy --version-id <X>`; if fail, reject + alert + audit.

## 2. Narrative (HIGH_RISK ≥ 300 palavras + risk justification)

Cloudflare Workers deploy é a **last mile** entre release pipeline e production runtime — é onde supply chain attacks materializam-se em customer impact. Sem hard gate non-bypassable, attestation de WI-S12-001 + SBOM de WI-S12-002 são **defense em depth** sem **enforcement**: defender pode publicar tampered Worker bundle direto via `wrangler deploy` (se attacker compromete deploy credentials). **WI-S12-003 fecha o loop**: deploy webhook **só** propaga rollout se Cosign signature + Rekor inclusion + Fulcio chain válidos.

**Threat class endereçada**: SolarWinds-class supply chain attack onde build pipeline OK mas tampered binário deployed (post-build hijack). Cosign verify gate em deploy webhook = **mandatory checkpoint** entre artifact produção e production rollout.

**Bugs catastróficos possíveis** (todos endereçados):

1. **Cosign signature missing**: attacker bypassa CI gate (ex: directly `wrangler deploy` com credentials roubadas); webhook não detecta. Mitigação: Cloudflare deploy ONLY via `corelink-deploy-verifier` webhook (CF API key restricted to verifier identity); manual `wrangler deploy` blocked via API token IAM scoping.

2. **Rekor inclusion proof missing**: signature presente mas não publicada em Rekor (offline attestation). Mitigação: `RekorMissing` error → reject; INV-SUPPLY-PROVENANCE-IN-REKOR enforced.

3. **Fulcio chain invalid**: signature usando attacker's CA (não sigstore Fulcio). Mitigação: Fulcio root cert pinned via TUF; rotation handled automatically.

4. **Identity mismatch**: Cosign signature OK mas SAN URI = `attacker/corelink-server` fork. Mitigação: `expected_identity` regex `^https://github\.com/HumanGuardrail/corelink-server/\.github/workflows/release-slsa3\.yml@refs/tags/v\d+\.\d+\.\d+$`.

5. **Replay attack**: attacker captures valid signature de v0.X.Y; deploys em v0.X.(Y+1). Mitigação: signature binds image digest (SHA-256 do bundle); novo bundle = novo digest = novo signature required.

6. **Webhook DoS**: attacker floods webhook com bogus deploy events; SaaS overhead. Mitigação: rate limit S-08 inheritance (10 deploys/hora/source); CF Workers built-in DoS protection.

7. **Audit emit failed (fail-OPEN bug)**: `emit_audit_event` falha mas deploy proceeds; tampering invisible. Mitigação: `emit_audit_event` é **fail-CLOSED** (CTRL-AUDIT-002 + INV-AUDIT-APPEND-ONLY); audit emit failure = deploy rejected + alert SEV-1.

8. **OCI registry compromise**: attacker compromises ghcr.io account; substitui image + signature. Mitigação: GitHub Actions OIDC → Fulcio binding workflow ref; Fulcio cert SAN é cripticamente verifiable (não trust em ghcr.io alone); Rekor inclusion proof = transparency log.

9. **CF API token leak**: attacker captures CF deploy token; deploys direct (bypass verifier). Mitigação: CF API token IAM scoped to verifier Worker only (no manual deploy permission); secret rotation quarterly via CTRL-AUTH-014.

10. **Time-of-check to time-of-use (TOCTOU)**: verifier validates v0.X.Y; attacker swaps OCI image entre verify e CF deploy API call. Mitigação: verifier passes image digest (SHA-256) to CF API; CF deploy uses pinned digest, não floating tag.

**Atacante adversarial scenarios** (validated em chaos tests):

- **Unsigned deploy attempt** (WI-007 RB-FM-156 dry-run): attacker triggers deploy via webhook com bogus signature; verifier rejects + alert SEV-2 fires + audit emit succeeds.
- **Rekor missing attempt**: attacker generates signature offline (skip Rekor publish); verifier rejects via `RekorMissing` error.
- **Identity confusion**: attacker stages release em fork; signature valid mas SAN URI mismatch; verifier rejects.
- **Replay attack**: attacker reuses signature de antiga release em novo deploy; image digest mismatch; verifier rejects.

**Risk justification HIGH_RISK**:

- **FF-HR-005**: bypass de deploy gate = full pipeline trust violation; cripto-load-bearing.
- **Reversibility**: tampered Worker em production = customer trust permanently lost; rollback non-trivial (cache invalidation + audit forensics).

11 sign-offs canonical incl. Architect (Crypto SME specialization mandatory: Cosign keyless OIDC + Fulcio chain validation + Rekor inclusion proof + adversarial deploy gate tests) + Security Lead (deploy boundary review) + SRE Lead (operational deploy flow + chaos test) + AppSec (CF API token IAM scoping + ghcr.io threat model).

## 3. Customer Impact & Journey

**Persona 1 — Internal SRE on-call durante deploy**:
- Normal flow: release published → `release-slsa3.yml` runs → SBOM + provenance + Cosign signature → ghcr.io image annotated → CF deploy webhook fires → verifier validates ≤ 5s → CF Workers updated.
- Incident flow: attacker bypasses CI gate → manual deploy attempt → CF API blocks (token IAM scoped) → verifier never invoked → attacker fails.
- Forced incident: webhook receives bogus signed payload → verifier validates fails → alert SEV-2 fires → on-call paged → audit event emitted.

**Persona 2 — Customer-side detection (post-incident forensics)**:
- Customer audits `https://rekor.sigstore.dev/api/v1/log/entries?logIndex=N` → verifies signature legitimacy.
- If forensic detects tampering (image digest mismatch vs Rekor entry), customer escalates to CoreLink Security via incident channel.
- Customer-visible diferenciador: hard gate é industry-leading (zero OSS Rust SaaS opera deploy verify gate em 2026).

**Persona 3 — Auditor SOC 2 Type II**:
- Audit query: "100% deploys últimos 30d com Cosign verify + Rekor inclusion proof".
- Evidence: deploy audit log (DT integration via S-09 audit chain) + alert escalation log + chaos test report.
- Compliance Matrix: SOC 2 CC6.7 (signed deploy mandatory) + CC8.1 (system change management).

**SLA addendum**:
- Deploy verify latency: ≤ 5s p99 (Cosign verify + Rekor lookup + Fulcio chain).
- Deploy block latency (rejection): ≤ 1s p99.
- Alert SEV-2 fire latency (deploy block): ≤ 30s.
- Audit emit latency: ≤ 1s p99 (fail-CLOSED).

## 4. Capability Mapping

- **CAP-SUPPLY-003** (Cosign signature + CF deploy verify) — IMPLEMENTA primary.
- Trace: `_spec_contract.md §4 + §5.3` + `security_model.md §11.4 (CTRL-SUPPLY-002 + CTRL-CRYPTO-002..005)` + `key_management.md §6.2 (Cosign keyless OIDC)` + `compliance_matrix.md §3.6 (SOC 2 CC6.7 + CC8.1)`.

## 5. Tipo

Cripto verifier + deploy gate; HIGH_RISK; FF-HR-005.

## 6. Escopo

### 6.1 In-scope

1. **`.github/workflows/cosign-sign.yml`** workflow novo (extends release-slsa3.yml):
   - Trigger: post-build job no release-slsa3.yml.
   - Step 1: build Docker/OCI image bundle do Worker WASM (`docker build -t ghcr.io/HumanGuardrail/corelink-worker:v0.X.Y .`).
   - Step 2: push image → `docker push ghcr.io/HumanGuardrail/corelink-worker:v0.X.Y`.
   - Step 3: Cosign sign keyless OIDC: `cosign sign --identity-token=$ACTIONS_ID_TOKEN_REQUEST_TOKEN ghcr.io/HumanGuardrail/corelink-worker:v0.X.Y`.
   - Step 4: Cosign attest com SLSA provenance + SBOM: `cosign attest --predicate provenance.intoto.jsonl --type slsaprovenance ghcr.io/...`.
   - Permissions: `id-token: write` (OIDC) + `packages: write` (ghcr.io push).
2. **`crates/corelink-deploy-verifier/`** Cloudflare Worker:
   - Cargo.toml: deps `worker = "0.4"`, `sigstore = "0.9"`, `serde`, `serde_json`, `reqwest`, `thiserror`, `tracing`.
   - Endpoint `POST /webhook/deploy` recebe `CfDeployWebhook`.
   - Authentication: webhook signed via shared HMAC secret (CF Webhooks SLA + secret rotation quarterly).
3. **`DeployVerifier` impl**:
   - Fetch Cosign signature de OCI registry (ghcr.io annotated layer).
   - Fetch Rekor inclusion proof via log index.
   - Validate Fulcio chain (TUF-pinned root).
   - Validate identity SAN URI matches `expected_identity` regex.
   - Validate image digest binding (Cosign signature → image SHA-256 match).
   - If all pass: invoke CF API `wrangler deploy --version-id <digest>` via `cf-api-rs` client.
   - If any fail: reject + audit emit + alert.
4. **CF API IAM scoping**:
   - CF API token role: `Workers Scripts:Edit` only para `corelink-deploy-verifier` Worker identity.
   - Token rotation: quarterly via CTRL-AUTH-014.
   - No manual `wrangler deploy` permission para humans (audit-only).
5. **Audit emit (fail-CLOSED)**:
   - CloudEvents 1.0+: `dev.hugr.corelink.deploy.verified.v1` (success) ou `dev.hugr.corelink.deploy.blocked.v1` (reject).
   - Persist em S-09 audit chain (forward integration; staging stub OK durante S-12).
   - Alert SEV-2 em `deploy.blocked.v1`.
   - Alert SEV-1 em `audit_emit_failed`.
6. **Métricas underscored Prometheus**:
   - `corelink_supply_cosign_verify_total{outcome,plan}` (outcome ∈ ok|sig_invalid|rekor_missing|fulcio_chain_invalid|identity_mismatch|oci_fetch_failed).
   - `corelink_supply_cosign_verify_duration_seconds_bucket` (histogram).
   - `corelink_supply_deploy_blocked_total{reason,plan}` (reason ∈ unsigned|rekor_missing|fulcio_invalid|identity_mismatch|audit_emit_failed).
   - `corelink_supply_deploy_propagated_total{plan}`.
   - `corelink_supply_audit_emit_total{outcome}` (outcome ∈ ok|failed).
7. **Observability** — trace span `deploy.verify` com attributes:
   - `deploy.release_tag` (string).
   - `deploy.cosign_image_ref` (string).
   - `deploy.fulcio_san_uri` (string).
   - `deploy.rekor_log_index` (u64).
   - `deploy.outcome` (enum).
8. **Property tests** (10k iter PR + 100k iter nightly):
   - `prop_cosign_signature_invalid_rejected`: 10k mutated signatures; assert 100% rejected.
   - `prop_rekor_missing_rejected`: 10k payloads sem Rekor proof; assert 100% rejected.
   - `prop_identity_mismatch_rejected`: 10k random SAN URIs; assert apenas regex match accepted.
   - `prop_replay_attack_rejected`: 10k swapped image digests; assert 100% rejected via TOCTOU mitigation.
   - `prop_audit_emit_failure_blocks_deploy`: 10k audit emit failures; assert deploy 100% blocked (fail-CLOSED).
9. **Adversarial regression tests** (5 CVE-class):
   - Unsigned deploy attempt → blocked + alert SEV-2.
   - Rekor missing attempt → blocked.
   - Identity confusion (fork SAN) → blocked.
   - Replay attack (old signature) → blocked.
   - Audit emit failure → deploy fail-CLOSED.
10. **Integration test E2E**:
    - Real release workflow → ghcr.io push + Cosign sign → CF deploy webhook → verifier validates → CF Workers updated.
    - Verify ≤ 5s p99 latency.
11. **Chaos test "deploy unsigned artifact"**:
    - Stage unsigned image em ghcr.io (red team script).
    - Trigger CF deploy webhook.
    - Expect: verifier rejects + alert SEV-2 + audit emit.
    - **Validates INV-SUPPLY-SIGNED-DEPLOY enforcement**.
12. **Chaos test "deploy with Rekor missing"**:
    - Stage signed image mas com signature offline (não Rekor published).
    - Trigger webhook.
    - Expect: verifier rejects via `RekorMissing` + alert SEV-2.
13. **Runbook RB-FM-156** (dep malicioso) — referenced WI-S12-007:
    - Scenario: malicious dep upgrade via Dependabot auto-merge.
    - Expected detection: cargo-audit (WI-S12-004) + DT (WI-S12-005); deploy gate é última defesa.

### 6.2 Out-of-scope (deferred)

- **SLSA L3 provenance**: WI-S12-001 (consumed; este WI verifica via Rekor inclusion).
- **SBOM CycloneDX**: WI-S12-002.
- **cargo-audit + cargo-deny + Dependabot**: WI-S12-004.
- **Dependency-Track self-host**: WI-S12-005.
- **Reproducible builds**: WI-S12-006.
- **PRR ship gate**: WI-S12-007.
- **Multi-region deploy verifier**: pós-GA (single Worker em CF default).
- **Customer-side deploy attestation API**: pós-GA (DT integration suffices).

## 7. Anti-Scope

- ❌ Custom Cosign client (use `sigstore` Rust crate audited).
- ❌ Long-lived signing keys (Cosign keyless OIDC only).
- ❌ Bypass mode em deploy verifier ("emergency override") — anti-pattern; rollback via revert PR + re-deploy with valid signature.
- ❌ Soft-fail mode ("warn but allow") — hard gate mandatory.
- ❌ Skip Rekor inclusion proof check (INV-SUPPLY-PROVENANCE-IN-REKOR enforces).
- ❌ Skip identity SAN match (regression class fork attack).
- ❌ Audit emit best-effort (fail-CLOSED mandatory).
- ❌ Manual `wrangler deploy` allowed (CF API token IAM scoping; humans audit-only).
- ❌ Webhook unauthenticated (HMAC signed required).
- ❌ Cache verify result > 0 seconds (each deploy fresh verify; no reuse).
- ❌ "Deploy without verify if Rekor down" fallback (no graceful bypass; release blocked).

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: Cosign sign + Cloudflare deploy webhook verify hard gate

  Background:
    Given .github/workflows/cosign-sign.yml configured
    And corelink-deploy-verifier Worker deployed em CF
    And expected_identity = "^https://github\\.com/HumanGuardrail/corelink-server/\\.github/workflows/release-slsa3\\.yml@refs/tags/v\\d+\\.\\d+\\.\\d+$"
    And CF API token IAM scoped to verifier only

  Scenario: Successful sign + deploy flow
    Given a release tag v0.X.Y published
    When release-slsa3.yml workflow runs
    Then Worker WASM bundle built + pushed to ghcr.io/HumanGuardrail/corelink-worker:v0.X.Y
    And Cosign sign keyless OIDC produces signature attached to OCI
    And signature published em Rekor (inclusion proof)
    Given CF deploy webhook triggered with v0.X.Y payload
    When corelink-deploy-verifier invokes verify_and_propagate
    Then Cosign signature fetch from OCI succeeds
    And Rekor inclusion proof fetched + Merkle root validated
    And Fulcio chain valid (TUF-pinned root)
    And SAN URI matches expected_identity regex
    And image digest matches signature binding
    And CF API propagation succeeds; deployment_id returned
    And metric corelink_supply_cosign_verify_total{outcome="ok"} incremented
    And audit event dev.hugr.corelink.deploy.verified.v1 emitted

  Scenario: Unsigned deploy blocked (chaos test)
    Given attacker stages unsigned image em ghcr.io/HumanGuardrail/corelink-worker:malicious
    When deploy webhook triggered with malicious payload
    Then verifier returns SignatureInvalid
    And CF API NOT invoked
    And alert SEV-2 'deploy_blocked_unsigned' fires
    And audit event dev.hugr.corelink.deploy.blocked.v1 emitted with reason=unsigned
    And metric corelink_supply_deploy_blocked_total{reason="unsigned"} incremented

  Scenario: Rekor missing blocked
    Given image signed but signature offline (não Rekor published)
    When deploy webhook triggered
    Then verifier returns RekorMissing
    And deploy blocked + alert SEV-2

  Scenario: Identity mismatch blocked (fork attack)
    Given attacker generates signature em fork "attacker/corelink-server"
    When deploy webhook triggered
    Then verifier validates SAN URI = "https://github.com/attacker/corelink-server/..."
    And regex match fails
    And IdentityMismatch error returned
    And deploy blocked + alert SEV-2

  Scenario: Replay attack blocked (TOCTOU)
    Given attacker captures valid signature for v0.X.Y
    When attacker triggers deploy with v0.X.(Y+1) image
    Then verifier validates image digest against signature binding
    And digest mismatch detected
    And deploy blocked + alert SEV-2

  Scenario: Fulcio chain invalid blocked
    Given malicious cert chain not anchored to sigstore Fulcio root
    When deploy webhook triggered
    Then FulcioChainInvalid error returned
    And deploy blocked

  Scenario: Audit emit failure fail-CLOSED
    Given S-09 audit chain endpoint returns 503
    When deploy webhook triggered (otherwise valid)
    Then emit_audit_event fails
    And verify_and_propagate returns AuditEmitFailed (fail-CLOSED)
    And CF API NOT invoked (deploy blocked)
    And alert SEV-1 'audit_emit_failed' fires

  Scenario: Webhook DoS rate limit
    Given attacker floods webhook with 1000 deploys/min
    When rate limit enforced (S-08 stub passthrough em S-12)
    Then > 10 deploys/hora/source rejected with 429
    And alert SEV-3 'webhook_dos' fires

  Scenario: Manual wrangler deploy blocked via IAM
    Given attacker captures CF API token
    When attacker tries `wrangler deploy` directly
    Then CF API returns 403 Forbidden (token scoped to verifier only)

  Scenario: Property test 100k iter green
    Given prop_cosign_signature_invalid_rejected 100k iter
    When test runs nightly
    Then 0 false-accepts
    And 0 panics

  Scenario: SLA latency p99
    Given verify_and_propagate em staging
    When measured over 100 deploys
    Then p99 ≤ 5s (Cosign verify + Rekor + Fulcio + CF propagate)
```

## 9. Design Decisions

### 9.1 Why Cosign keyless OIDC (não long-lived keys)

- Threat class eliminada: secret exfiltration via compromised CI step.
- Industry trend: Cosign 2.0 default keyless; OSS ecosystem migrating.
- Identity binding: Fulcio cert SAN = workflow ref (cripticamente verifiable).

### 9.2 Why hard gate non-bypassable (não soft-fail)

- INV-SUPPLY-SIGNED-DEPLOY (CRITICAL) — soft-fail = invariant violation possible.
- Bypass mode = anti-pattern; emergency rollback = revert PR + new signed release.
- Customer trust dependent on cripto enforcement (não trust em human gate).

### 9.3 Why CF Worker (não GitHub Actions step) para verify

- **Last-mile** enforcement: bypass GitHub Actions com CF deploy via stolen token = mitigated by CF token IAM scoping + verifier Worker.
- Operational: CF Worker é low-latency + CF-internal trust boundary.
- Trade-off: dependency em CF infra; mitigated via redundant alerting + S-09 audit chain.

### 9.4 Why HMAC-signed webhook (não unauthenticated)

- Webhook target é trusted boundary; HMAC secret rotation quarterly via CTRL-AUTH-014.
- Alternativa: mTLS (overhead operacional vs HMAC).

### 9.5 Why ghcr.io OCI registry (não Docker Hub)

- GitHub Actions OIDC bound to ghcr.io account; identity native.
- Free for public packages; private packages bilable.
- Alternativa rejeitada: Docker Hub (rate limits + identity mapping overhead).

### 9.6 Why image digest binding (não floating tag)

- TOCTOU mitigation: signature binds SHA-256 digest; tag swap detected.
- CF deploy uses pinned digest (não floating tag).
- Industry standard: Cosign signs digests by default.

### 9.7 Why audit emit fail-CLOSED

- Audit gap = compliance violation (CTRL-AUDIT-002 + INV-AUDIT-APPEND-ONLY CRITICAL).
- Trade-off: deploy delay durante audit chain outage; documented em runbook.
- Alternativa rejeitada: fail-OPEN (CVE territory; tampering invisible).

### 9.8 Why TUF-pinned Fulcio root cert (não trust-on-first-use)

- TUF metadata refresh handles rotation transparently.
- Pinning prevents CA compromise impact.
- sigstore-rs handles TUF lifecycle.

### 9.9 Why ADR potencial?

- Sim — **ADR-XXXX**: "Deploy gate hard non-bypassable + Cosign keyless OIDC". Ratificação cripto-load-bearing decision.
- Whitelist em `validate_references.py` até materializar.

### 9.10 Why CTRL-AUTH-014 quarterly rotation

- HMAC webhook secret + CF API token + Cosign OCI write token = 3 rotation cycles.
- Quarterly cadence balanced (security vs operational overhead).
- Aligned with key_management.md §3.4 quarterly rotation policy.

## 10. Completeness Criteria SOTA

- [ ] **10.s12.003.1** Property test 10k iter (PR) + 100k iter (nightly) sobre fuzz inputs → 0 panics, 0 false-accepts (EVT-002).
- [ ] **10.s12.003.2** Adversarial test: 5 CVE-class regressions (unsigned, Rekor missing, identity confusion, replay, audit fail-CLOSED) — 100% rejected (EVT-040).
- [ ] **10.s12.003.3** E2E test contra staging fork: release → sign → ghcr push → webhook → verifier → CF deploy; latência verify ≤ 5s p99 (EVT-018).
- [ ] **10.s12.003.4** **Chaos test "deploy unsigned artifact"** → blocked verified + alert SEV-2 + audit emit (EVT-023).
- [ ] **10.s12.003.5** **Chaos test "deploy with Rekor missing"** → blocked verified (EVT-023).
- [ ] **10.s12.003.6** SAST clean (cargo-audit + cargo-deny + clippy `-D warnings`) (EVT-002).
- [ ] **10.s12.003.7** Cost regression gate: verifier Worker p99 ≤ 5s; CF compute ≤ $5/mês adicional (Lote 9.4 §14.10).
- [ ] **10.s12.003.8** OWASP ASVS V14 + V11.1 (deploy boundary) + SSDF PS.1 100% checklist (EVT-002).
- [ ] **10.s12.003.9** INV-SUPPLY-SIGNED-DEPLOY enforced; chaos validated (EVT-022).
- [ ] **10.s12.003.10** CF API token IAM scoped + manual `wrangler deploy` blocked verified.
- [ ] **10.s12.003.11** Audit emit fail-CLOSED enforcement validated em chaos test.
- [ ] **10.s12.003.12** Cripto SME pair-program adversarial review (mandatory pré-merge).

## 11. DoD

- [ ] `.github/workflows/cosign-sign.yml` committed.
- [ ] Crate `corelink-deploy-verifier` compila + Worker deployed em CF staging.
- [ ] All Gherkin scenarios green em integration test.
- [ ] Property tests 10k green em PR + 100k green em nightly.
- [ ] E2E test contra staging green.
- [ ] Chaos tests "deploy unsigned" + "deploy Rekor missing" green.
- [ ] Métricas emitidas (5 listadas §6.1.6).
- [ ] Trace spans `deploy.verify` em OTel pipeline.
- [ ] rustdoc + 3 examples.
- [ ] ADR-XXXX (Deploy gate hard) escrito + ratificado.
- [ ] Code review (Crypto SME folded Architect + Security + AppSec + SRE).
- [ ] PRR Architect mini-sign-off.
- [ ] Cost regression gate green.
- [ ] CF API token IAM scoping verified.

## 12. Invariants Validated

### Mantidas

- **INV-SUPPLY-SIGNED-DEPLOY** (HIGH — herdada): este WI implementa primary; deploy gate enforces.
- **INV-AUDIT-APPEND-ONLY** (CRITICAL — herdada): emit audit event fail-CLOSED em CTRL-AUDIT-002 alignment.

### Novas

- **INV-SUPPLY-PROVENANCE-IN-REKOR** (HIGH — NEW): este WI consume + enforce em deploy gate (Rekor inclusion mandatory).

TLA+ alignment: não-aplicável (build-time + deploy-time controle, não runtime state machine).

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| Cosign sign workflow | `.github/workflows/cosign-sign.yml` | YAML |
| `corelink-deploy-verifier` crate | `crates/corelink-deploy-verifier/` | Rust workspace member |
| DeployVerifier impl | `crates/corelink-deploy-verifier/src/verifier.rs` | Rust |
| CfDeployWebhook + types | `crates/corelink-deploy-verifier/src/types.rs` | Rust |
| DeployVerifyError enum | `crates/corelink-deploy-verifier/src/error.rs` | Rust |
| Audit emitter (fail-CLOSED) | `crates/corelink-deploy-verifier/src/audit.rs` | Rust |
| CF API client | `crates/corelink-deploy-verifier/src/cf_api.rs` | Rust |
| Worker entry point | `crates/corelink-deploy-verifier/src/worker.rs` | Rust |
| Property tests | `crates/corelink-deploy-verifier/tests/prop_verify.rs` | Rust |
| Adversarial tests | `crates/corelink-deploy-verifier/tests/adversarial.rs` | Rust |
| Chaos tests | `crates/corelink-deploy-verifier/tests/chaos.rs` | Rust |
| E2E integration | `tests/e2e_deploy_verify.rs` | Rust |
| ADR-XXXX (Deploy gate hard) | `specs/03_architecture/adrs/ADR-XXXX-deploy-gate-hard-cosign-keyless.md` | Markdown |
| Examples | `crates/corelink-deploy-verifier/examples/` | Rust |

## 14. Quality Standards SOTA

- **14.s12.003.1** Zero `unsafe`; zero `unwrap` em src/.
- **14.s12.003.2** rustdoc 100% public API + 3 examples.
- **14.s12.003.3** Test coverage ≥ 90%.
- **14.s12.003.4** Latência: verify_and_propagate p99 ≤ 5s warm; rejection p99 ≤ 1s.
- **14.s12.003.5** SAST clean.
- **14.s12.003.6** Métricas RED + Cosign/Rekor/Fulcio cache metrics.
- **14.s12.003.7** Runbook RB-FM-156 + RB-FM-157 — referenciado WI-S12-007.
- **14.s12.003.8** Breaking changes em `DeployVerifier` trait = bump major.
- **14.s12.003.9** Memory: CF Worker bound (128 MB stack runtime).
- **14.s12.003.10** Cost regression gate em CI.
- **14.s12.003.11** sigstore-rs pinned (0.9.x); bump via ADR.
- **14.s12.003.12** Audit emit fail-CLOSED tested em chaos.
- **14.s12.003.13** CF API token IAM scoping verified quarterly.

## 15. Chaos Experiments

1. **Deploy unsigned artifact** (canonical test for INV-SUPPLY-SIGNED-DEPLOY): stage unsigned image em ghcr.io; trigger webhook; expect rejection + alert + audit. **Mandatory chaos test for PRR**.

2. **Deploy with Rekor missing**: signature offline (não Rekor published); verifier rejects via RekorMissing.

3. **Identity confusion (fork attack)**: stage release em `attacker/corelink-server` fork; signature valid mas SAN URI mismatch; verifier rejects.

4. **Replay attack (TOCTOU)**: capture valid signature de v0.X.Y; trigger deploy com v0.X.(Y+1) image; image digest mismatch detected; rejected.

5. **Fulcio chain invalid**: malicious cert chain (self-signed CA); verifier rejects via FulcioChainInvalid.

6. **Audit emit failure fail-CLOSED**: simulate S-09 audit chain 503; verify deploy blocked + alert SEV-1.

7. **Webhook DoS**: flood webhook 1000 deploys/min; verify rate limit caps + alert SEV-3.

8. **Manual wrangler deploy bypass attempt**: capture CF API token; try direct `wrangler deploy`; expect 403 (IAM scoped).

9. **OCI registry compromise simulation**: red team stages malicious image em ghcr.io with stolen credentials; signature mismatch detected via Rekor + Fulcio chain.

10. **TUF rotation chaos**: simulate Fulcio root rotation; verify TUF auto-fetch + verify continues working.

## 16. PRR (Production Readiness Review)

PRR HIGH_RISK 11 sign-offs canonical (S-12 ship gate é WI-S12-007; este WI passa por mini-PRR Architect):

- [ ] All Gherkin green.
- [ ] Property + adversarial tests green.
- [ ] E2E staging green.
- [ ] Chaos test "deploy unsigned" green (validates INV-SUPPLY-SIGNED-DEPLOY).
- [ ] Chaos test "deploy Rekor missing" green.
- [ ] Cost regression gate green.
- [ ] Métricas + dashboards configurados em DASH-SUPPLY.
- [ ] ADR-XXXX (Deploy gate hard) published.
- [ ] Crypto SME review (Cosign keyless OIDC + Fulcio chain + Rekor inclusion).
- [ ] AppSec review (CF API IAM scoping + ghcr.io threat model + webhook HMAC).
- [ ] SRE review (deploy operational flow + chaos test).
- [ ] Architect approval (deploy gate pattern reusable em S-13 admin op signing).
- [ ] OWASP ASVS V14 + V11.1 + SSDF PS.1 100% pass.

## 17. Sub-tasks

| ID | Sub-task | Estimativa |
|---|---|---|
| ST-001 | `.github/workflows/cosign-sign.yml` skeleton | 2h |
| ST-002 | OCI image build + ghcr.io push | 1.5h |
| ST-003 | Cosign sign keyless OIDC integration | 2h |
| ST-004 | Cosign attest SLSA + SBOM integration | 1.5h |
| ST-005 | `corelink-deploy-verifier` Worker scaffold | 1.5h |
| ST-006 | `DeployVerifier` impl (Cosign verify + Rekor lookup + Fulcio chain) | 4h |
| ST-007 | CF API client + IAM scoping | 2h |
| ST-008 | Audit emit fail-CLOSED + alerts | 2h |
| ST-009 | Webhook HMAC verification | 1h |
| ST-010 | Métricas emit (5 metrics) + trace spans | 1.5h |
| ST-011 | Property tests 10k iter (5 props) | 3.5h |
| ST-012 | Adversarial regression tests (5 CVE classes) | 2.5h |
| ST-013 | E2E integration test contra staging | 3h |
| ST-014 | Chaos test "deploy unsigned" | 2h |
| ST-015 | Chaos test "deploy Rekor missing" + replay + IAM bypass + Fulcio | 3h |
| ST-016 | rustdoc + 3 examples | 1.5h |
| ST-017 | ADR-XXXX redação | 1.5h |
| ST-018 | Crypto SME (folded Architect) + AppSec + SRE review | 3h |
| ST-019 | PRR mini-sign-off | 1h |

**Total Optimistic**: ~38h. **PERT** (O=14h, M=22h, P=36h per spec contract): **23h**.

## 18. Dependencies

### Hard blockers

- WI-S12-001 (SLSA L3 + Rekor) SEALED — provenance chain consumido aqui.
- GitHub Actions OIDC enabled (ops setup).
- ghcr.io account configurado (ops setup).
- CF API token + IAM role (ops setup).

### Soft blockers

- WI-S12-002 (SBOM) — Cosign attest SBOM ideal mas não bloqueante para verify gate.
- S-09 audit chain (audit emit fail-CLOSED ideal; staging stub OK durante S-12).

### Outbound

- WI-S12-007 (PRR ship gate) — gates S-12 close.
- S-13 admin plane — admin op signing pode reusar pattern.

## 19. Effort PERT

O: 14h, M: 22h, P: 36h → PERT **23h** (per spec contract §12).

## 20. Time-boxing

**28h hard limit**. If exceeded → escalation: split em sub-WI (Cosign sign vs verify gate + chaos).

## 21. Observability

5 métricas + trace span listadas. Logs structured JSON; nivel INFO em success, WARN em retry, ERROR em block + audit emit failed.

Dashboard widget DASH-SUPPLY:
- Cosign verify success ratio (target ≥ 99.9%).
- Deploy block rate (per outcome).
- Verify p99 latency.
- Audit emit success ratio (target = 100% fail-CLOSED).

## 22. Cost Analysis

- ghcr.io storage: free (public packages).
- Cosign sign: free (keyless OIDC).
- CF Worker compute: ~10k requests/mês × $0.50/M = ~$0.005/mês. Bound under $5/yr.
- Trace storage: ~$2/mês.
- **Total custo direto S-12 WI-003**: ~$2/mês = $24/yr.

## 23. API Contract

`DeployVerifier` Rust trait + CF Worker endpoint `POST /webhook/deploy`. Webhook payload schema versioned (`webhook-v1.json` em `specs/_schemas/`).

Erro mapping (HTTP):
- `SignatureInvalid | RekorMissing | FulcioChainInvalid | IdentityMismatch` → HTTP 401 `deploy_verify_failed`.
- `OciFetchFailed` → HTTP 503 `oci_unavailable`.
- `CfApiFailed` → HTTP 502 `cf_api_unavailable`.
- `AuditEmitFailed` → HTTP 500 `audit_emit_failed` (fail-CLOSED).

## 24. Post-mortem Hooks

- Bypass de deploy gate (unsigned deploy succeeded) → CRITICAL post-mortem + breach response.
- Rekor outage breaks deploy > 1h sustained → SEV-2 + ops post-mortem.
- Fulcio rotation breaks verify > 4h → SEV-2.
- Audit emit silent failure (fail-OPEN regression) → SEV-1 + 5-Why.
- CF API token leak detected → CRITICAL + secret rotation.
- TOCTOU exploit (image swap between verify e deploy) → CRITICAL.

## 25. Rollback / Recovery

- Verifier rollback: revert PR + redeploy previous Worker version via `wrangler deploy --version-id <prev>` (OWNER only after manual unblock).
- Workflow rollback: revert PR.
- CF API token rotation: trigger via emergency runbook RB-AUTH-014.
- RTO: ≤ 30 min.
- RPO: 0 (stateless verifier).

Fallback degradation: se Rekor sustained outage > 1h, deploy publication blocked (intentional; document em runbook fallback).

## 26. Security & Privacy

**STRIDE delta**:
- **Spoofing**: Cosign signature + Fulcio cert SAN binding previne forge.
- **Tampering**: image digest binding + Rekor inclusion = tampering detect.
- **Repudiation**: audit chain + Rekor log = forensic-grade evidence.
- **Info disclosure**: deploy events em audit chain (PII-redacted; commit SHA + tag são public).
- **DoS**: webhook rate limit + CF DoS protection.
- **Elevation of privilege**: CF API token IAM scoped (verifier-only); manual deploy blocked.

**LINDDUN delta**:
- **Linkability**: deploy events linkable via release tag (intentional; audit trail).
- **Identifiability**: GitHub Actions service principal (não personal identity).
- **Non-repudiation**: cripto property intentional.
- **Detectability**: deploy attempts em audit log.
- **Disclosure**: audit events em chain (controlled access).
- **Unawareness**: customer-facing supply chain documentation.
- **Non-compliance**: SOC 2 CC6.7 + CC8.1 satisfied.

## 27. Knowledge Transfer

- `crates/corelink-deploy-verifier/README.md`.
- ADR-XXXX (Deploy gate hard).
- Doc `docs/internal/cosign-deploy-flow.md` — sequence diagram.
- Workshop interno (1h) Crypto SME + AppSec + SRE.
- Runbook RB-FM-156 + RB-FM-157 (linked WI-S12-007).

## 28. Risk Register (6-col)

| ID | Risco | Prob | Det | Impacto | Exposure | Residual | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | sigstore-rs regression em version bump | L | H | CRITICAL | M | LOW | Pin 0.9.x + ADR + property test |
| R-002 | Rekor outage > 1h sustained | L | M | HIGH | L | LOW | Document fallback runbook; release blocked (intentional) |
| R-003 | Fulcio root rotation breaks verify | L | M | HIGH | L | LOW | TUF auto-refresh + chaos test |
| R-004 | CF API token leak | L | M | CRITICAL | M | LOW | IAM scoping + rotation quarterly + secret detection scan |
| R-005 | OCI registry compromise (ghcr.io) | L | M | CRITICAL | M | LOW | Cosign signature integrity + Rekor inclusion = trust independente de ghcr.io |
| R-006 | TOCTOU image swap | L | H | CRITICAL | L | LOW | Image digest binding em signature; CF deploy uses pinned digest |
| R-007 | Audit emit fail-OPEN regression | L | L | CRITICAL | L | LOW | Property test prop_audit_emit_failure_blocks_deploy |
| R-008 | Webhook DoS | M | L | LOW | L | LOW | Rate limit + CF DoS |
| R-009 | Manual wrangler deploy bypass | L | L | CRITICAL | L | LOW | CF API token IAM scoping + alert em manual attempt |
| R-010 | Verifier latency > 5s p99 | L | M | LOW | L | LOW | Cost regression gate + bench |
| R-011 | Webhook HMAC secret leak | L | L | HIGH | L | LOW | Rotation quarterly + secret scan |
| R-012 | Replay attack via cached signature | L | M | HIGH | L | LOW | Image digest binding |

## 29. Review Checkpoints

1. **Design (D+0)**: Architect review verifier + ADR-XXXX outline.
2. **Crypto (D+1)**: Crypto SME pair-program Cosign + Fulcio + Rekor logic.
3. **Code (D+3)**: peer review (folded Engineer + Architect).
4. **AppSec (D+3)**: AppSec review CF IAM + ghcr.io + HMAC.
5. **SRE (D+4)**: SRE review deploy flow + chaos test.
6. **Adversarial (pre-merge D+5)**: red team session.
7. **PRR mini (D+5)**: Architect sign-off.

## 30. Sign-off (HIGH_RISK 11 canonical)

| # | Role | Name | Signed Date | Status |
|---|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | _pending_ | _pending_ |
| 2 | Final Approver | Gustavo Schneiter | _pending_ | _pending_ |
| 3 | Architect | _TBD; emphatic — Crypto SME specialization mandatory (Cosign keyless OIDC + Fulcio chain validation + Rekor inclusion proof + audit emit fail-CLOSED + 5 CVE-class adversarial regressions)_ | _pending_ | _pending_ |
| 4 | Security Lead | _TBD; emphatic — deploy boundary review + INV-SUPPLY-SIGNED-DEPLOY enforcement_ | _pending_ | _pending_ |
| 5 | SRE Lead | _TBD; emphatic — deploy operational flow + chaos test review_ | _pending_ | _pending_ |
| 6 | Engineer (S-12 lead) | _TBD_ | _pending_ | _pending_ |
| 7 | QA Lead | _TBD_ | _pending_ | _pending_ |
| 8 | Product | Gustavo Schneiter | _pending_ | _pending_ |
| 9 | Compliance Officer | _TBD; emphatic — SOC 2 CC6.7 + CC8.1 evidence_ | _pending_ | _pending_ |
| 10 | Privacy Officer | _TBD_ | _pending_ | _pending_ |
| 11 | AppSec advisor | _TBD; emphatic — CF API IAM scoping + ghcr.io threat model + webhook HMAC posture_ | _pending_ | _pending_ |

> Crypto SME (Cosign + Fulcio + Rekor + audit emit fail-CLOSED) folds into Architect role specialization mandatory. Peer reviewers contribuem em PR review sem sign-off canonical separado (folded into Engineer + Architect per framework §33.5.4.3 + ADR-0034).

## 31. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-29 | Gustavo (via Claude Opus 4.7) | Criação WI-S12-003 (cycle 11.S12.0). |

## 32. Anti-patterns evitados

- ❌ Custom Cosign client (use sigstore-rs audited).
- ❌ Long-lived signing keys (keyless OIDC only).
- ❌ Bypass mode em deploy verifier ("emergency override").
- ❌ Soft-fail mode ("warn but allow").
- ❌ Skip Rekor inclusion proof check.
- ❌ Skip identity SAN match (regression class fork attack).
- ❌ Audit emit best-effort (fail-CLOSED mandatory).
- ❌ Manual `wrangler deploy` allowed (IAM scoping enforces).
- ❌ Webhook unauthenticated (HMAC required).
- ❌ Cache verify result > 0 seconds (each deploy fresh verify).
- ❌ "Deploy without verify if Rekor down" fallback (no graceful bypass).
- ❌ Floating tag em CF deploy (digest pinned).

---

**Fim WI-S12-003.** Próximo: WI-S12-004 (cargo-audit + cargo-deny + Dependabot).
