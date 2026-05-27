---
id: "S-12"
type: "sprint"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-29"
updated: "2026-04-29"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-005"]
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "SECURITY-MODEL"
  - "COMPLIANCE-MATRIX"
  - "INVARIANT-REGISTRY"
  - "FAILURE-MODES"
  - "OBSERVABILITY-MODEL"
  - "KEY-MANAGEMENT"
tags: ["sprint", "s12", "supply-chain", "slsa", "sbom", "cosign", "sigstore", "rekor", "cargo-audit", "cargo-deny", "dependency-track", "reproducible-builds", "high-risk"]
---

# Sprint S-12 — Supply Chain Hardening (SLSA L3 + SBOM CycloneDX 1.5+ + Cosign keyless + Reproducible Builds + Dependency-Track)

> **doc_status:** DRAFT · **lane:** HIGH_RISK · **Versão:** 1.0.0 · **2026-04-29**
> **Owner:** Gustavo Schneiter · **Aprovador Final:** Gustavo Schneiter
> **Spec contract base:** `_spec_contract.md` v1.1.0 (cycle 9.4 SOTA elevation)

> **Phase boundary:** Fase 2 — Build/Release/Deploy hardening pré-GA.
> **FF-HR-005 specific**: bypass de qualquer controle supply chain = blast radius **todos os customers** (substrate cripto-load-bearing).

---

## 1. Objetivo

Endurecer **toda a cadeia build-attest-sign-deploy** ao nível **SLSA Level 3** (industry-leading vs OSS competitors operando L1/L2) com 7 controles compostos: (1) **SLSA L3 build provenance** via `slsa-github-generator/generator_generic_slsa3.yml@v1.10.0` em hermetic GitHub Actions runner, attestation in-toto v1.0 assinada via sigstore/Fulcio keyless OIDC, inclusion proof publicada em Rekor transparency log; (2) **SBOM CycloneDX 1.5+** gerado via `cargo-cyclonedx` em pipeline, NTIA minimum elements compliant, timestamp RFC 3161 attested via Sigstore TSA, ingerido em **Dependency-Track** (self-hosted Postgres) para continuous CVE matching; (3) **Cosign sign release artifacts** keyless OIDC, signature attached em OCI registry + Rekor, **Cloudflare deploy webhook verifier** rejeita binário não-assinado OU sem Rekor inclusion proof OU com Fulcio chain inválida (binário não-assinado = rollout blocked, hard gate non-bypassable); (4) **`cargo-audit`** PR-gate + daily cron com alert SEV-2 em CRITICAL e SEV-3 em HIGH; (5) **`cargo-deny`** policy enforcement: license allowlist (MIT/Apache-2.0/BSD-2-Clause/BSD-3-Clause/ISC/MPL-2.0/Unicode-DFS-2016), GPL-*/AGPL-*/SSPL-*/Commons-Clause **banned**, zero yanked deps tolerated, sources restricted to crates.io (git deps requerem explicit commit hash); (6) **Dependabot** weekly grouped PRs com auto-merge para minor patches passando CI; (7) **Reproducible builds best-effort**: 2-runner parallel matrix com SHA-256 diff check, target diff ≤ 5% bytes, fontes de non-determinism documentadas em `docs/build/reproducible.md` + ADR-0015 (`SOURCE_DATE_EPOCH`, `--remap-path-prefix`, `rust-toolchain.toml` pin). Mitiga **THR-T-006** (código modificado no pipeline), **FM-156** (dep maintainer malicioso → SolarWinds-style), **FM-157** (typosquatting). Implementa CTRL-SUPPLY-001..008 (codex SEAL cycle 1 alignment: added 006 license allowlist, 007 yanked-dep block, 008 reproducible build verification) + CTRL-CRYPTO-002..005 + ADR-0014.

## 2. Escopo

### 2.1 In-scope

- **WI-S12-001**: SLSA L3 GitHub Actions workflow + Fulcio keyless OIDC + Rekor inclusion proof + verify CLI (`rekor-cli search --rekor_server https://rekor.sigstore.dev`).
- **WI-S12-002**: SBOM CycloneDX 1.5+ generation via `cargo-cyclonedx` + Dependency-Track ingestion API + NTIA minimum elements check via `cyclonedx-cli validate --minimum-required-fields ntia` + RFC 3161 timestamp via Sigstore TSA.
- **WI-S12-003**: Cosign sign release artifacts (Worker WASM bundle + side artifacts) keyless via OIDC + OCI registry attach + Cloudflare deploy webhook verifier (`workers-deploy-verifier`) com hard gate + chaos test "deploy unsigned artifact" → blocked verified.
- **WI-S12-004**: `cargo-audit` PR check + daily cron + `cargo-deny` policy em `deny.toml` (license allowlist, banned licenses, yanked, sources, advisories) + Dependabot weekly grouped PRs + auto-merge minor patches.
- **WI-S12-005**: Dependency-Track v4.11+ self-host em CF Pages + Neon Postgres small + SBOM ingestion API + webhook Slack/Email para CVE HIGH/CRITICAL ≤ 15 min + mock CVE injection test.
- **WI-S12-006**: Reproducible builds 2-runner parallel matrix + SHA-256 diff check + ADR-0015 + `docs/build/reproducible.md` documenting non-determinism sources (`SOURCE_DATE_EPOCH`, `--remap-path-prefix`, toolchain pin).
- **WI-S12-007**: RB-FM-156 (dep maintainer malicioso) dry-run + RB-FM-157 (typosquatting) dry-run + Security walkthrough + PRR HIGH_RISK doc 11 sign-offs canonical.

### 2.2 Anti-scope

- ❌ Software Security Questionnaire completo (S-20 GA readiness).
- ❌ SOC 2 Type II audit engagement (pós-GA).
- ❌ FedRAMP Moderate baseline (pós-GA, mercado fed não target inicial).
- ❌ Hardware-backed signing (HSM, YubiKey) — overhead operacional desproporcional vs Cosign keyless OIDC.
- ❌ Air-gapped build environment — incompatível com GitHub Actions; pós-GA se enterprise demand materializar.
- ❌ TUF (The Update Framework) full implementation — sigstore Fulcio+Rekor já cobre threat model; não duplicar.
- ❌ ISO/IEC 27036 (supplier security) full audit — pós-GA enterprise tier.
- ❌ SLSA Level 4 (hermetic verifier + two-party review) — meta pós-GA Q3 se justificativa enterprise/fed materializar; L3 é industry-leading vs OSS competitors.
- ❌ External (third-party) security audit do supply chain pipeline (S-20 com Trail of Bits / Chainguard / NCC Group).

## 3. Customer Impact & Journey

**JTBD:** "Como SecOps lead de prospect enterprise, preciso de evidência verificable de que o build CoreLink **não pode** ser tampered (SolarWinds class) sem detection — provenance pública em Rekor, SBOM por release, signed deploy gate non-bypassable. Como auditor SOC 2, preciso de attestation que dependency CVEs HIGH/CRITICAL são detectadas + escaladas ≤ 15 min."

**CAPs entregues:** CAP-SUPPLY-001..007 (SLSA L3 build provenance + SBOM CycloneDX 1.5+ + Cosign+CF deploy verify + cargo-audit/deny/Dependabot + reproducible builds + Dependency-Track + vendored deps audit).

**Persona 1 — SecOps lead avaliando CoreLink em RFP**:
- Evidence pack inclui Rekor inclusion proof URLs (publicly verifiable), SBOM CycloneDX 1.5+ download, Cosign verify command (`cosign verify --certificate-identity-regexp ... ghcr.io/humangr-labs/corelink-worker:v0.X.Y`).
- Diferenciador competitivo: 95%+ OSS Rust SaaS opera SLSA L1; CoreLink em L3 = sinal forte para procurement/legal.

**Persona 2 — Auditor SOC 2 / ISO 27001**:
- CTRL-SUPPLY-001..005 attestation dossier; PRR doc S-12 11 sign-offs canonical; Dependency-Track CVE alert log; cargo-deny report quarterly.
- Compliance Matrix mapping: CC6.7 (change management with signed deploy), CC7.1 (vulnerability detection), CC8.1 (system change management).

**Persona 3 — Engineer onboarding em CoreLink**:
- `docs/build/reproducible.md` explica build flags + 2-runner check.
- `deny.toml` documenta políticas (qual license adicionar via ADR).
- ADR-0015 documenta non-determinism trade-offs.

**SLA addendum**:
- CVE HIGH/CRITICAL detected → Slack/Email alert ≤ 15 min (Dependency-Track webhook + on-call escalation).
- Yanked dep introduzida via Dependabot auto-merge → CI gate blocks PR ≤ 1 min.
- License audit (quarterly Legal review): cycle ≤ 90 dias.
- Reproducible build divergence > 5% bytes → post-mortem em ≤ 7 dias com ADR update.

## 4. Capability Mapping (trace)

Ver `_spec_contract.md §4`. Foundation: `security_model.md §11.4` (Supply Chain controls) + `compliance_matrix.md §3.6` (SOC 2 CC6.7 + EO 14028) + `key_management.md §6.2` (Cosign keyless OIDC short-lived cert via Fulcio).

## 5. Deliverables

| ID | Entregável | Onde | DoD |
|---|---|---|---|
| S12-D1 | SLSA L3 workflow + Rekor inclusion | `.github/workflows/release-slsa3.yml` + `crates/corelink-supply-verify/` | `slsa-github-generator/generator_generic_slsa3.yml@v1.10.0` integrado; Rekor inclusion proof verificável publicamente; in-toto v1.0 attestation attached |
| S12-D2 | SBOM CycloneDX 1.5+ pipeline | `.github/workflows/sbom.yml` + `tools/sbom-publish/` | `cargo-cyclonedx` em CI; SBOM publicada como release asset + ingerida em Dependency-Track; NTIA minimum elements verde via `cyclonedx-cli validate`; RFC 3161 TSA timestamp attested |
| S12-D3 | Cosign + CF deploy verify gate | `crates/corelink-deploy-verifier/` + `.github/workflows/cosign-sign.yml` | Cosign sign release keyless OIDC; CF deploy webhook verifica signature + Rekor inclusion + Fulcio chain antes de rollout; chaos test "unsigned deploy" → blocked + alert SEV-2 verified |
| S12-D4 | cargo-audit + cargo-deny + Dependabot | `deny.toml` + `.github/workflows/cargo-audit.yml` + `.github/dependabot.yml` | PR check + daily cron; license allowlist + banned + yanked + sources policies em `deny.toml`; Dependabot weekly grouped + auto-merge minor patches |
| S12-D5 | Dependency-Track self-host + CVE alerts | `infra/dependency-track/` (CF Pages + Neon Postgres) + `crates/corelink-dt-webhook/` | DT v4.11+ instance running; webhook → Slack ≤ 15 min para HIGH/CRITICAL; mock CVE injection test verde |
| S12-D6 | Reproducible build 2-runner diff + ADR | `.github/workflows/reproducible-build.yml` + `docs/build/reproducible.md` + `specs/03_architecture/adrs/ADR-0015-reproducible-build-best-effort.md` | 2 parallel runners (matrix `os: [ubuntu-22.04]` × 2 instances) build → SHA-256 diff check; non-determinism sources documentadas; ADR-0015 ratificado |
| S12-D7 | RB dry-runs + Security walkthrough + PRR | `specs/05_runbooks/RB-FM-156.md` + `specs/05_runbooks/RB-FM-157.md` + `specs/04_sprints/_sealed/S12/PRR-S12.md` | RB-FM-156 (dep malicious) + RB-FM-157 (typosquatting) dry-runs executados em staging; reports committed; PRR 11 sign-offs canonical documented |

## 6. Escopo técnico por camada (inherits_from)

### 6.1 Security Model (herda `security_model.md §11.4`)

- CTRL-SUPPLY-001 (provenance attestation publicly verifiable via Rekor).
- CTRL-SUPPLY-002 (signed deploy hard gate; Cosign + Rekor inclusion mandatory).
- CTRL-SUPPLY-003 (SBOM mandatory + Dependency-Track continuous matching).
- CTRL-SUPPLY-004 (license allowlist enforcement via cargo-deny).
- CTRL-SUPPLY-005 (yanked dep zero tolerance).
- CTRL-CRYPTO-002..005 (Cosign keyless OIDC; Fulcio short-lived cert per build; sigstore root cert pinning; transparency log inclusion proof).

### 6.2 Compliance Matrix (herda `compliance_matrix.md §3.6`)

- SOC 2 CC6.7 — change management with signed deploy.
- SOC 2 CC7.1 — vulnerability detection (CVE matching + alerting).
- SOC 2 CC8.1 — system change management.
- Executive Order 14028 — SBOM mandatory para US federal contracts (forward-compatible se mercado fed materializar).
- NIST SP 800-218 (SSDF) — secure software development framework alignment.

### 6.3 Failure Modes (herda `failure_modes.md`)

- FM-156 (dep maintainer malicious): RB-FM-156 dry-run em S-12 WI-007.
- FM-157 (typosquatting): RB-FM-157 dry-run em S-12 WI-007.
- THR-T-006 (código modificado no pipeline): mitigado via SLSA L3 hermetic build + Rekor transparency.

### 6.4 Key Management (herda `key_management.md §6.2`)

- Cosign keyless OIDC: short-lived cert (10 min) via Fulcio; GitHub Actions identity = builder identity.
- No long-lived signing keys (eliminação de threat class secret exfiltration).
- Fulcio root cert pinning em deploy verifier; rotation handled via sigstore TUF.

### 6.5 SLOs

- **SLO-SUPPLY-CVE-DETECTION** (novo; adicionar slo_catalog em S-12): CVE HIGH/CRITICAL detected → Slack alert delivery ≤ 15 min p99 sustained 30d staging.
- **SLO-SUPPLY-DEPLOY-VERIFY-LATENCY** (novo): deploy webhook verify latency ≤ 5s p99 (Cosign verify + Rekor lookup).

## 7. Definition of Done (lane HIGH_RISK)

> **Two-phase SEAL** (per Timeline §9): items verificáveis instantaneamente fecham em **Implementation SEAL D+15**; items requerendo "sustained 30d" janela (cargo-audit daily green streak, CVE detection MTTD, deploy-verify SLO p99 sustained, release coverage 30d) fecham em **GA Evidence Gate SEAL D+45**. Ambos SEALs canonicos; sprint considerado concluído apenas após GA Evidence Gate D+45.

- [ ] Todos 7 WIs SEALED (EVT-031).
- [ ] **SLSA L3 attestation** publicada em Rekor transparency log para todos os releases pós-S-12 (verifiable via `rekor-cli search`); 100% releases últimos 30d coverage (EVT-010).
- [ ] **SBOM** disponível como GitHub release asset + Dependency-Track ingestion verde + NTIA minimum elements check verde + RFC 3161 timestamp attested (EVT-010).
- [ ] **Cosign verify é gate** antes de CF deploy (EVT-011); chaos test "deploy unsigned artifact" → blocked verified; chaos test "deploy with Rekor missing" → blocked verified (EVT-023).
- [ ] **`cargo-audit`** zero findings HIGH/CRITICAL em `Cargo.lock` no momento do release; daily cron green sustained 30d staging (EVT-002).
- [ ] **`cargo-deny`** policy verde em CI; license allowlist enforced; 0 yanked deps; 0 banned licenses (EVT-002).
- [ ] **Dependabot** ativo + weekly grouped PRs functioning; auto-merge minor patches working sem regression em CI (EVT-002).
- [ ] **Reproducible build**: 2 runners produzem binário com diff ≤ 5% bytes (release builds); fontes de non-determinism documentadas em `docs/build/reproducible.md` + ADR-0015 ratificado (EVT-027).
- [ ] **Dependency-Track** integration: SBOM ingerido + 1 CVE simulado triggers Slack alert ≤ 15 min p99 (EVT-021).
- [ ] **RB-FM-156** (dep malicioso scenario) dry-run executado com Security lead + SRE + report committed (EVT-017).
- [ ] **RB-FM-157** (typosquatting) dry-run executado + report committed (EVT-017).
- [ ] **PRR HIGH_RISK** 11 sign-offs canonical (per framework §33.5.4.3 + ADR-0034): Owner + Final Approver + Architect (Crypto SME specialization mandatory para Cosign keyless OIDC + Fulcio chain validation) + Security Lead + SRE Lead + Engineer (S-12 lead) + QA Lead + Product + Compliance Officer + Privacy Officer + AppSec advisor (EVT-031).
- [ ] **3 novas INVs** (INV-SUPPLY-PROVENANCE-IN-REKOR, INV-SUPPLY-NO-YANKED, INV-SUPPLY-LICENSE-ALLOWLIST) ratificadas em invariant_registry.md §3.X + CI gates ativos (EVT-022).
- [ ] **Cost regression gate**: CI minutes para SLSA L3 + SBOM + Cosign sign ≤ 8 min adicionais por release; build pipeline cost ≤ $200/mês (EVT-002).
- [ ] **Métricas underscored Prometheus**: 12+ supply chain metrics emitting em staging (`corelink_supply_*`) com label `plan` aplicável (EVT-013).

## 8. Dependencies

### Hard blockers

- nenhum (sprint independente — pode rodar paralelo a outros sprints).

### Soft blockers

- **S-03 SEALED** recomendado (deploy real flow para CF; este sprint endurece o pipeline já operacional).
- S-09 audit chain (DT webhook events idealmente emitem para audit chain; staging stub OK durante S-12).

### Outbound

- S-13, S-14, S-17, S-20 — todos consomem hardened deploy pipeline.
- Crítico para **GA**: PRR exige SLSA L3 attestation publicada para 100% releases últimos 30d.
- S-20 GA gate: external supply chain audit (Trail of Bits / Chainguard) referencia S-12 baseline.

## 9. Timeline

- **Sprint kick-off**: D+0 (após qualquer sprint atual fecha; preferentemente após S-03).
- **D+3**: WI-001 SEALED (SLSA L3 attestation visible em Rekor para 1 release).
- **D+5**: WI-002 SEALED (SBOM ingerido em Dependency-Track).
- **D+8**: WI-003 SEALED (Cosign verify gate ativo + chaos test passed).
- **D+10**: WI-004 + WI-005 SEALED (cargo-deny + DT alerts).
- **D+12**: WI-006 SEALED (reproducible build best-effort done).
- **D+13**: WI-007 SEALED (runbooks dry-run + PRR).
- **D+15**: Sprint review + sign-offs + **Implementation SEAL ceremony** (todos WIs entregues + tooling em produção + zero P0/P1 abertos).
- **D+15..D+45**: **Observation window (30d sustained evidence)** — release coverage 100% SLSA attestation + daily audit verde + CVE detection ≤ 24h sustained + deploy-verify SLO p99 ≤ 100ms sustained. Métricas coletadas continuously; nenhum WI re-aberto exceto fix-critical.
- **D+45**: **GA Evidence Gate SEAL** (sprint sign-off final) — DoD ship-gate criteria validated com janela 30d real (não-simulada); release coverage, audit green streak, CVE alerting MTTD, deploy-verify p99 todos comprovados via DASH-SUPPLY + Dependency-Track. Implementation já SEALED em D+15; este gate libera S-20 GA dependency.
- **Total:** 2.5 semanas implementação (12 dias úteis) + 30d observation window + GA Evidence Gate D+45.

## 10. Risk Register

Ver `_spec_contract.md §15` (11 risks 6-col com Owner per item).

## 11. Observability Plan

DASH-SUPPLY (novo dashboard):
- SLSA attestations published rate (per release).
- Rekor inclusion proof verify success ratio.
- SBOM generation duration p99.
- Dependency-Track CVE matching latency.
- cargo-audit findings count (HIGH/CRITICAL trend 30d).
- cargo-deny violations count (license/yanked/sources).
- Cosign verify latency p99 (deploy webhook).
- Reproducible build diff bytes (2-runner check).
- Dependabot PR merge ratio (auto-merge success).

Métricas underscored Prometheus (per observability_model.md §4.1):
- `corelink_supply_slsa_attestations_total{outcome,plan}` (outcome ∈ ok|fulcio_fail|rekor_fail|sign_fail).
- `corelink_supply_sbom_generation_duration_seconds_bucket`.
- `corelink_supply_dt_cve_alerts_total{severity}` (severity ∈ critical|high|medium|low).
- `corelink_supply_cargo_audit_findings_total{severity}`.
- `corelink_supply_cargo_deny_violations_total{rule}` (rule ∈ license|yanked|sources|advisory).
- `corelink_supply_cosign_verify_duration_seconds_bucket`.
- `corelink_supply_cosign_verify_total{outcome,plan}` (outcome ∈ ok|sig_invalid|rekor_missing|fulcio_chain_invalid).
- `corelink_supply_reproducible_diff_bytes_gauge`.
- `corelink_supply_dependabot_prs_total{outcome}` (outcome ∈ auto_merged|manual|failed).
- `corelink_supply_rekor_inclusion_proof_verify_total{outcome}`.
- `corelink_supply_deploy_blocked_total{reason,plan}` (reason ∈ unsigned|rekor_missing|fulcio_invalid|sbom_missing).
- `corelink_supply_dt_ingestion_total{outcome}` (outcome ∈ ok|api_error|validation_failed).

## 12. Security & Privacy

**STRIDE** (delta vs S-09 audit):
- **Spoofing**: Cosign signature + Fulcio chain previne forge de release; Rekor inclusion previne offline tampering.
- **Tampering**: SLSA L3 hermetic build + reproducible builds 2-runner diff catch tampering pré-deploy.
- **Repudiation**: Rekor transparency log + SBOM TSA timestamp = forensic-grade evidence; cannot deny release origin.
- **Information disclosure**: SBOM exposes dep list (acceptable; industry standard); secrets em env nunca em SBOM (sanitize step).
- **DoS**: SLSA generator failure = build blocked (graceful degradation com manual override via Security + Architect dual-approval ADR).
- **Elevation of privilege**: Cosign keyless OIDC = no long-lived secrets; minimal GitHub Actions permissions; OIDC identity bound to specific workflow + ref.

**LINDDUN** (delta):
- **Linkability**: Rekor é publicly searchable; release identity (commit SHA + workflow ref) é public knowledge (acceptable for OSS substrate).
- **Identifiability**: builder identity em provenance é GitHub Actions service principal (não personal identity).
- **Non-repudiation**: Rekor inclusion proof + Fulcio cert chain = cryptographic proof of origin (intentional; security property).
- **Detectability**: dependent CVEs publicly known (CVE database); SBOM exposure não eleva risco.
- **Disclosure of information**: vendored patches em `[patch.crates-io]` requerem ADR + Security review (avoid leak proprietary fixes).
- **Unawareness**: customer-facing supply chain posture documented em landing + DPA.
- **Non-compliance**: SOC 2 CC6.7/CC7.1/CC8.1 + EO 14028 SBOM mandatory + NIST SP 800-218 SSDF satisfied.

## 13. Post-mortem hooks

Triggers que **automaticamente abrem post-mortem doc**:

- Deploy attempt with unsigned artifact → INFO (validates gate working) + log; se inadvertent (interno) → 5-Why obrigatório.
- CVE HIGH/CRITICAL detectado em Cargo.lock pós-deploy → SEV-2 + post-mortem (root cause: por que cargo-audit miss?).
- Yanked dep introduced via Dependabot auto-merge → SEV-3 + post-mortem + cargo-deny rule strengthen.
- SLSA attestation forge attempt detected → CRITICAL post-mortem + Security incident response.
- Reproducible build divergence > 5% bytes → SEV-3 + post-mortem (debug non-determinism source).
- Vendor patch sem ADR mergeado em main → SEV-3 + post-mortem + retroactive ADR + Security review.
- License audit miss (GPL leak) → CRITICAL post-mortem + Legal + remediation timeline ≤ 30 dias.
- Rekor outage (fail-closed canonical; no operator override per INV-SUPPLY-PROVENANCE-IN-REKOR; codex SEAL cycle 1) > 24h → SEV-2 + ops post-mortem (validate fallback grace period suficiente).
- Dependency-Track outage > 4h → SEV-3 + ops post-mortem (validate redundant CVE matching via OSS Index API fallback).

## 14. Sign-off (HIGH_RISK 11 canonical)

11 roles: Owner + Final Approver + Architect (com Crypto SME specialization mandatory para Cosign keyless OIDC + Fulcio chain validation + Rekor inclusion proof verify) + Security Lead + SRE Lead + Engineer (S-12 lead) + QA Lead + Product + Compliance Officer + Privacy Officer + AppSec advisor. Crypto SME folds into Architect role per framework §33.5.4.3 + ADR-0034 solo-tier waiver. Peer reviewers contribuem em PR review sem sign-off canonical separado (folded into Engineer + Architect).

## 15. Change log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-29 | Gustavo (via Claude Opus 4.7) | Criação sprint.md S-12 (cycle 11.S12.0). |

---

**Fim de S-12 sprint contract.**
