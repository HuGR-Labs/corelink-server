---
id: "SPEC-CONTRACT-S12"
type: "spec_contract"
doc_status: "SEALED"
audit_status: "AUDITED"
version: "1.4.0"
created: "2026-04-24"
updated: "2026-05-14"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["spec-contract", "s12", "supply-chain", "slsa", "sbom", "cosign", "sigstore", "cargo-audit", "high-risk", "sota-v1.1"]
---

# Spec Contract — S-12: Supply Chain Hardening (SLSA L3 + SBOM CycloneDX + Cosign + Reproducible Builds)

## 0. Metadata

| Campo | Valor |
|---|---|
| Sprint ID | S-12 |
| Nome | Supply Chain Hardening |
| Lane | HIGH_RISK |
| Lane forcing factors | FF-HR-005 (controles supply chain — bypass = blast radius global) |
| Duração estimada | 2.5 semanas |
| WIs antecipados | 7 |
| SOTA target | SLSA Level 3 verified + SBOM CycloneDX 1.5+ assinado + Cosign+CF deploy verify gate + reproducible build best-effort + dep policy enforcement |

## 1. Objetivo

**Endurecer supply chain** ao nível **SLSA Level 3** (ou superior se factível em GitHub Actions): build provenance assinada via sigstore/Fulcio + transparency log Rekor, SBOM CycloneDX 1.5+ em cada release, Cosign signature + Cloudflare deploy verify gate (binário não-assinado = rollout blocked), `cargo-audit` daily + `cargo-deny` policy + Dependabot, reproducible builds (goal-state com diff 2-runner check). Mitiga **THR-T-006** (código modificado no pipeline), **FM-156** (dep maintainer malicioso → SolarWinds-style), **FM-157** (typosquatting). Implementa CTRL-SUPPLY-001..005 + CTRL-CRYPTO-002..005 + ADR-0014.

**Por que SOTA:** competitors operam SLSA L1 ou L2; supply chain attacks (XZ Utils 2024, event-stream 2018, ua-parser-js 2021) demonstram que **L3 com hermetic build + transparency log é o mínimo defensável**. CoreLink S-12 entrega: provenance attestation publicada em Rekor (publicly verifiable), Cosign verify como hard gate de deploy (não bypassable via Worker config), SBOM ingerida em Dependency-Track para CVE matching contínuo, reproducible build com 2-runner diff check.

## 2. Lane + forcing factors

- **Lane:** HIGH_RISK (10–12 sign-offs).
- **FF-HR-005**: controles supply chain bypass = blast radius **todos os customers** afetados; rigor HIGH_RISK não negociável.
- **Threat model**: nation-state attacks no Rust ecosystem ocorrem em frequência crescente (RustSec advisories triplicaram 2022→2025); CoreLink é alvo high-value (multi-tenant + CI/CD substrate).

## 3. Inherits_from

```yaml
inherits_from:
  - "SECURITY-MODEL"            # CTRL-SUPPLY-001..005, CTRL-CRYPTO-002..005
  - "COMPLIANCE-MATRIX"         # SOC 2 CC6.7 + Executive Order 14028 (US fed contracts)
  - "INVARIANT-REGISTRY"        # INV-SUPPLY-SIGNED-DEPLOY, INV-SUPPLY-SBOM-PRESENT
  - "FAILURE-MODES"             # FM-156 (dep malicious), FM-157 (typosquat)
  - "OBSERVABILITY-MODEL"       # supply chain métricas (cve count, attestation rate)
  - "KEY-MANAGEMENT"            # Cosign keyless via Fulcio OIDC
```

## 4. CAPs entregues

| ID | Capability | Detalhe |
|---|---|---|
| **CAP-SUPPLY-001** | SLSA L3 build provenance | GitHub Actions + slsa-github-generator/generic generator → in-toto attestation; sigstore/Fulcio keyless signing; Rekor transparency log. |
| **CAP-SUPPLY-002** | SBOM CycloneDX 1.5+ | `cargo-cyclonedx` em build pipeline; SBOM publicada como release asset + Dependency-Track ingestion; NTIA minimum elements compliant. |
| **CAP-SUPPLY-003** | Cosign signature + CF deploy verify | Cosign sign release artifacts; Cloudflare deploy webhook verify signature pre-activation; binário não-assinado rejeitado. |
| **CAP-SUPPLY-004** | Cargo-audit daily + cargo-deny policy + Dependabot | CI gate: PR + daily cron; license allowlist (MIT/Apache-2.0/BSD/ISC/MPL-2.0); zero HIGH/CRITICAL CVEs in main. |
| **CAP-SUPPLY-005** | Reproducible builds best-effort | 2-runner parallel build + diff hash check; document non-determinism sources. |
| **CAP-SUPPLY-006** | Dependency-Track integration | Self-hosted Dependency-Track (CF Pages + small Postgres) ou managed; CVE alerts continuous. |
| **CAP-SUPPLY-007** | Vendored deps audit + lockfile pinning | `Cargo.lock` committed; vendor `Cargo.toml` patches scrutinized; no git deps non-pinned. |

## 5. Requirements específicos

### 5.1 SLSA L3 (CAP-SUPPLY-001)

- **R-S12-1**: GitHub Actions workflow com SLSA L3 provenance via `slsa-github-generator/generator_generic_slsa3.yml@v1.10.0`:
  - Hermetic build (no network during compile).
  - Builder isolated (separate VM per build).
  - Provenance signed via sigstore/Fulcio keyless OIDC.
  - Rekor transparency log entry mandatory.
  - In-toto attestation v0.0.1 ou superior.
- **R-S12-2**: Provenance verification em Cloudflare deploy webhook (CAP-SUPPLY-003 dependency).

### 5.2 SBOM (CAP-SUPPLY-002)

- **R-S12-3**: `cargo-cyclonedx` em build pipeline; SBOM CycloneDX 1.5+ JSON formato standard.
- **R-S12-4**: SBOM publicada:
  - Como GitHub release asset (`sbom.cdx.json`).
  - Ingerida em Dependency-Track para continuous CVE matching.
  - Timestamp RFC 3161 attested via Sigstore TSA.
- **R-S12-5**: SBOM passa **NTIA minimum elements** check via `cyclonedx-cli validate --minimum-required-fields ntia`.

### 5.3 Cosign (CAP-SUPPLY-003)

- **R-S12-6**: Cosign sign release artifacts (Worker WASM bundle + side artifacts):
  - Keyless via OIDC (GitHub Actions identity).
  - Signature publicada em Rekor + OCI registry attached.
- **R-S12-7**: Cloudflare deploy webhook handler (`workers-deploy-verifier`) verifica signature **antes** de rollout:
  - Cosign verify chain via Fulcio root cert.
  - Rekor lookup (transparency log inclusion proof).
  - Reject deploy se signature missing OR invalid OR not in Rekor.
- **R-S12-8**: Chaos test: tentar deploy artifact não-assinado → expect deploy blocked + alert SEV-2.

### 5.4 Cargo-audit + cargo-deny + Dependabot (CAP-SUPPLY-004)

- **R-S12-9**: `cargo-audit` em CI:
  - PR check: `cargo audit --deny warnings`.
  - Daily cron: scan `Cargo.lock` + alert SEV-3 se HIGH; SEV-2 se CRITICAL.
- **R-S12-10**: `cargo-deny` policy em `deny.toml`:
  - **License allowlist**: MIT, Apache-2.0, BSD-2-Clause, BSD-3-Clause, ISC, MPL-2.0, Unicode-DFS-2016.
  - **Banned**: GPL-* (copyleft incompatível), AGPL-*, SSPL-*, Commons-Clause.
  - **Yanked**: 0 yanked deps allowed em `Cargo.lock`.
  - **Sources**: only crates.io (no git unless explicitly pinned).
  - **Advisories**: deny RUSTSEC-* unless waived ADR.
- **R-S12-11**: Dependabot weekly grouped PRs (separate por security/non-security); auto-merge minor patches que passam CI.

### 5.5 Reproducible Builds (CAP-SUPPLY-005)

- **R-S12-12**: 2 parallel runners (matrix `os: [ubuntu-22.04]` × 2 instances) build → SHA-256 binário; diff check; report.
- **R-S12-13**: Document non-determinism sources em `docs/build/reproducible.md`:
  - Cargo build embed timestamp → use `SOURCE_DATE_EPOCH`.
  - LLVM debug info paths → use `--remap-path-prefix`.
  - Rustc compiler version pin via `rust-toolchain.toml`.
- **R-S12-14**: Reproducible build target via ADR-0015: 2-runner diff ≤ 5% bytes em release builds; **fontes de non-determinism documentadas em `docs/build/reproducible.md` + ADR-0015 são pré-requisito** (não fallback). Goal medium-term (12 months pós-GA): 100% bit-identical com toolchain mais maduro.

### 5.6 Dependency-Track (CAP-SUPPLY-006)

- **R-S12-15**: Self-hosted Dependency-Track v4.11+ em CF Pages + Neon Postgres (small);
- **R-S12-16**: SBOM ingestion via API após cada release;
- **R-S12-17**: Webhook a Slack/Email se novo CVE HIGH/CRITICAL afeta dep em produção.

### 5.7 Vendored Deps (CAP-SUPPLY-007)

- **R-S12-18**: `Cargo.lock` committed at root; lockfile diff em PR review (mandatory comment se mudou).
- **R-S12-19**: Vendor patches em `[patch.crates-io]` em `Cargo.toml` requerem ADR + Security review.
- **R-S12-20**: Git deps allowed apenas com explicit commit hash (não tag, não branch).

## 6. Definition of Done

- [ ] **WIs SEALED**: 7/7.
- [ ] **SLSA L3 attestation** publicada em Rekor transparency log para todos os releases pós-S-12 (verifiable via `rekor-cli search --rekor_server https://rekor.sigstore.dev`).
- [ ] **SBOM** disponível como GitHub release asset + Dependency-Track ingestion verde + NTIA minimum elements check verde.
- [ ] **Cosign verify é gate** antes de CF deploy (EVT-011); chaos test "deploy unsigned artifact" → blocked verified.
- [ ] **`cargo-audit`** zero findings HIGH/CRITICAL em `Cargo.lock` no momento do release.
- [ ] **`cargo-deny`** policy verde em CI; license allowlist enforced; 0 yanked deps.
- [ ] **Dependabot** ativo + weekly grouped PRs functioning; auto-merge minor patches working.
- [ ] **Reproducible build**: 2 runners produzem binário com diff ≤ 5% bytes (release builds); fontes de non-determinism documentadas em `docs/build/reproducible.md` + ADR-0015 (criar) (EVT-027).
- [ ] **Dependency-Track** integration: SBOM ingerido + 1 CVE simulado triggers Slack alert.
- [ ] **RB-FM-156** (dep malicioso scenario) dry-run executado com Security lead + SRE.
- [ ] **RB-FM-157** (typosquatting) dry-run executado.
- [ ] **PRR HIGH_RISK** (10–12 sign-offs): Security lead + SRE + Engineer + Compliance officer + Product + QA + 2 peers + AppSec advisor + Architect + Crypto SME (Cosign keyless OIDC review).

## 7. Completeness Criteria (delta local)

- [ ] **10.s12.1** SBOM export passa NTIA minimum elements check (cyclonedx-cli).
- [ ] **10.s12.2** Cosign verify failure em deploy bloqueia rollout (chaos test); evidence captured EVT-011.
- [ ] **10.s12.3** SLSA L3 attestation visible em Rekor publicly; auditor pode lookup.
- [ ] **10.s12.4** **Reproducible build** 2-runner diff ≤ 5% bytes (release builds) AND ADR-0015 ratificado AND non-determinism sources documented em `docs/build/reproducible.md` (EVT-027).
- [ ] **10.s12.5** **cargo-audit daily cron** verde sustained 30d staging.
- [ ] **10.s12.6** **Dependency-Track** continuous CVE matching ativo; mock CVE injection → alert delivered ≤ 15 min.
- [ ] **10.s12.7** **Cosign keyless OIDC** working (Fulcio short-lived cert via GitHub Actions identity).

## 8. Invariants

### Mantidas

- **INV-SUPPLY-SIGNED-DEPLOY** (CRITICAL — herdada): binário não-assinado rejeitado em deploy.
- **INV-SUPPLY-SBOM-PRESENT** (HIGH — herdada): release sem SBOM blocked.

### Novas (introduzidas por S-12 — adicionar a invariant_registry.md)

- **INV-SUPPLY-PROVENANCE-IN-REKOR** (HIGH — novo): toda provenance attestation deve estar em Rekor transparency log; release sem Rekor inclusion proof = blocked. **Why:** offline tampering de attestation file local é trivial; Rekor torna tampering publicamente detectável. **How to apply:** deploy webhook checks Rekor inclusion antes de rollout.
- **INV-SUPPLY-NO-YANKED** (HIGH — novo): zero yanked deps em `Cargo.lock` em main branch. **Why:** yanked = author retired (segurança ou bug); usar = risco. **How to apply:** cargo-deny policy + CI gate.
- **INV-SUPPLY-LICENSE-ALLOWLIST** (HIGH — novo): zero deps fora da license allowlist (MIT/Apache-2.0/BSD/ISC/MPL-2.0). **Why:** GPL/AGPL = copyleft viral; uso inadvertent = legal exposure. **How to apply:** cargo-deny.

## 9. Quality Standards (delta local)

- **14.s12.1 Zero dependências yanked** no `Cargo.lock` em main; CI gate.
- **14.s12.2 Zero licenses GPL/AGPL/SSPL** em transitivos; cargo-deny enforces.
- **14.s12.3 SBOM timestamp RFC 3161 attested** via Sigstore TSA.
- **14.s12.4 Cosign keyless OIDC** (não usar long-lived keys; OIDC Fulcio short-lived cert per build).
- **14.s12.5 RUSTSEC advisories**: triage ≤ 24h; fix ≤ 7d para HIGH; ≤ 30d para MEDIUM.
- **14.s12.6 Vendor patch transparency**: cada `[patch.crates-io]` entry requer ADR + Security review.
- **14.s12.7 Build hermeticity**: builder runs em isolated VM; no network during cargo build; verified via SLSA L3 generator.
- **14.s12.8 Provenance retention**: attestations + SBOMs retained 7y em CDN (parallel ao audit log).

## 10. Anti-scope

- ❌ Software Security Questionnaire completo (S-20 GA readiness).
- ❌ SOC 2 Type II audit engagement (pós-GA).
- ❌ FedRAMP Moderate baseline (pós-GA, mercado fed não target inicial).
- ❌ Hardware-backed signing (HSM, YubiKey) — overhead operacional desproporcional vs Cosign keyless OIDC.
- ❌ Air-gapped build environment — incompatível com GitHub Actions; pós-GA se enterprise demand.
- ❌ TUF (The Update Framework) full implementation — sigstore Fulcio+Rekor já cobre threat model; não duplicar.
- ❌ ISO/IEC 27036 (supplier security) full audit — pós-GA enterprise tier.

## 11. Dependencies

### Hard blockers

- nenhum (sprint independente — pode rodar paralelo a outros).

### Soft blockers

- **S-03 SEALED** recomendado (deploy real flow para CF).

### Outbound

- S-13, S-14, S-17, S-20 — todos consomem hardened deploy pipeline.
- Crítico para **GA**: PRR exige SLSA L3 attestation publicada para 100% releases últimos 30d.

## 12. WIs antecipados (PERT)

| ID | Título | Sub-tasks | O | M | P | PERT |
|---|---|---|---|---|---|---|
| **WI-S12-001** | SLSA L3 GitHub Actions workflow + Rekor inclusion | slsa-github-generator config; workflow; Fulcio OIDC; Rekor verify CLI; runbook | 12h | 18h | 30h | **19.0h** |
| **WI-S12-002** | SBOM CycloneDX generation + Dependency-Track ingestion + NTIA check | cargo-cyclonedx integration; SBOM publish workflow; Dependency-Track API; NTIA validator | 10h | 16h | 26h | **16.7h** |
| **WI-S12-003** | Cosign sign release + CF deploy webhook verify gate | Cosign sign step; OCI attach; deploy webhook verify; chaos test unsigned deploy | 14h | 22h | 36h | **23.0h** |
| **WI-S12-004** | cargo-audit + cargo-deny policies + Dependabot | deny.toml; cargo-audit CI gate; daily cron; Dependabot config; auto-merge minor | 8h | 12h | 20h | **12.7h** |
| **WI-S12-005** | Dependency-Track self-host + CVE alerts + webhook | DT instance setup; webhook config; mock CVE test; runbook | 10h | 16h | 24h | **16.3h** |
| **WI-S12-006** | Reproducible build verification + 2-runner diff + ADR | SOURCE_DATE_EPOCH; remap-path-prefix; 2-runner workflow; diff check; document sources | 12h | 18h | 30h | **19.0h** |
| **WI-S12-007** | RB-FM-156 + RB-FM-157 dry-runs + PRR + Security walkthrough | dep malicious scenario; typosquat scenario; Security walkthrough; PRR doc | 8h | 12h | 18h | **12.3h** |

**Total PERT:** ~119h ≈ 15 dias work × 1 eng. Buffer 3 dias confere com 2.5 semanas.

## 13. Duração + Timeline

- **Duração:** 2.5 semanas (12 dias úteis) + buffer 3 dias.
- **Marcos:**
  - **D+3:** WI-001 SEALED (SLSA L3 attestation visible em Rekor para 1 release).
  - **D+5:** WI-002 SEALED (SBOM ingerido em Dependency-Track).
  - **D+8:** WI-003 SEALED (Cosign verify gate ativo + chaos test passed).
  - **D+10:** WI-004 + WI-005 SEALED (cargo-deny + DT alerts).
  - **D+12:** WI-006 SEALED (reproducible build best-effort done).
  - **D+13:** WI-007 SEALED (runbooks dry-run + PRR).
  - **D+15:** Sprint review + sign-offs.

## 14. Critérios de promoção

- DoD complete + SLSA L3 verified + zero CVEs HIGH/CRITICAL last 30d.
- RB-FM-156 + RB-FM-157 dry-run clean.
- Cosign verify chaos test passed.
- 2-runner reproducible build documented.
- PRR HIGH_RISK aprovado.

## 15. Riscos (registry expandido)

| Risco | Prob | Det | Impacto | Exposure | Residual após mitigação | Mitigação |
|---|---|---|---|---|---|---|
| **SLSA L3 runner self-hosted forced** (não GitHub managed) | L | M | MEDIUM (operational complexity) | L | LOW | Use slsa-github-generator (GitHub-managed L3 supported); fallback documentation. |
| **Reproducible build infeasible** com deps Rust + LLVM | H | L | LOW (goal, não blocker) | M | LOW | Document sources; aim 80%+ bit-identical; ADR para non-deterministic exceptions. |
| **Cosign learning curve** + Fulcio OIDC setup | M | L | LOW | L | LOW | Hire AppSec consultant para 1-week ramp se needed; Fulcio docs maduros. |
| **Dep supply chain attack** durante sprint (FM-156) | L | H | CRITICAL | M | LOW | cargo-deny + advisory monitoring + Slack alert + RB-FM-156. |
| **Typosquatting** (FM-157) — typo crate publish | L | H | HIGH | M | LOW | Cargo crates official + lockfile review hook; cargo-deny sources allowlist. |
| **Rekor outage** (sigstore down) blocks deploy | L | M | HIGH (operational) | L | **NEGLIGIBLE — fail-closed canonical** | **NO grace period; NO operator override**: deploy hard-blocks até Rekor recovers. INV-SUPPLY-PROVENANCE-IN-REKOR é CRITICAL gate por design (offline tampering trivial sem Rekor). Local cache permitido APENAS para performance (lookup speed); jamais substitui inclusion proof requirement em deploy verify. Codex SEAL P0 alignment cycle 1: replaces ambiguous '24h grace' policy. |
| **License audit miss** (transitive GPL leak) | M | M | HIGH (legal) | M | LOW | cargo-deny exhaustive license allowlist; quarterly Legal review. |
| **SBOM ingestion fails** (Dependency-Track outage) | M | L | LOW (alerting delay) | L | LOW | Self-hosted DT + redundant CVE matching via OSS Index API fallback. |
| **GitHub Actions secrets exfiltration** (compromised dep step) | L | L | CRITICAL (sign keys) | M | LOW | Cosign keyless OIDC = no long-lived secrets; minimal permissions; audit. |
| **Yanked dep introduced via transitive update** | M | L | MEDIUM | L | LOW | INV-SUPPLY-NO-YANKED + cargo-deny daily; auto-block PR. |
| **Provenance attestation forge attempt** | L | H | CRITICAL | M | LOW | Rekor inclusion proof mandatory + Fulcio root cert pinning; INV-SUPPLY-PROVENANCE-IN-REKOR. |

## 16. Benchmarks SOTA externos (target qualitativo + quantitativo)

| Critério | Google internal | Chainguard | GitHub OSS | NPM/PyPI | **CoreLink target S-12** |
|---|---|---|---|---|---|
| SLSA Level | L4 | L3+ | L1-L3 | L1 | **L3 (target L4 pós-GA se factível)** |
| SBOM format | SPDX 2.3 | CycloneDX 1.5 | SPDX | None mandatory | **CycloneDX 1.5+ NTIA compliant** |
| Provenance signing | Internal | Cosign keyless | Sigstore | None default | **Cosign keyless via Fulcio OIDC** |
| Transparency log | Internal | Rekor | Rekor | None | **Rekor mandatory** |
| Reproducible builds | 100% | Best-effort | N/A | N/A | **Best-effort + 2-runner diff + ADR** |
| Dep CVE continuous monitoring | Internal | DT-equiv | Dependabot | Snyk | **Dependency-Track + Dependabot** |
| License allowlist enforcement | Internal | OPA | None default | None default | **cargo-deny + 7 licenses** |
| Yanked dep block | N/A | Yes | Manual | N/A | **cargo-deny enforced** |
| Hardened build hermeticity | Yes (full) | Yes | Limited | No | **GitHub Actions isolated VM** |

**Veredito SOTA:** S-12 v1.1 atinge **L3 (industry-leading vs OSS competitors)**; L4 é meta pós-GA se justificativa enterprise/fed materializar. Vantagem em INV-SUPPLY-PROVENANCE-IN-REKOR mandatory + cargo-deny exhaustive policy.

## 17. References (RFCs, papers, standards)

- **SLSA Specification v1.0** <https://slsa.dev/spec/v1.0/>.
- **in-toto Attestation Framework** <https://in-toto.io/>.
- **Sigstore (Fulcio + Rekor + Cosign)** <https://www.sigstore.dev/>.
- **CycloneDX SBOM Specification 1.5** <https://cyclonedx.org/>.
- **NTIA Minimum Elements for SBOM** <https://www.ntia.doc.gov/files/ntia/publications/sbom_minimum_elements_report.pdf>.
- **Executive Order 14028** — Improving the Nation's Cybersecurity (US fed contracts SBOM mandatory).
- **NIST SP 800-218** — Secure Software Development Framework (SSDF).
- **SOC 2 CC6.7** — change management (signed deploy).
- **OpenSSF Scorecard** — automated security best-practices score.
- **Reproducible Builds Project** <https://reproducible-builds.org/>.
- **Cargo Book — `[patch.crates-io]`** <https://doc.rust-lang.org/cargo/reference/overriding-dependencies.html>.
- **RustSec Advisory Database** <https://rustsec.org/>.

## 18. Post-mortem hooks

Triggers que **automaticamente abrem post-mortem doc**:

- Deploy attempt with unsigned artifact → INFO (validates gate working) + log; se inadvertent → 5-Why obrigatório.
- CVE HIGH/CRITICAL detectado em Cargo.lock pós-deploy → SEV-2 + post-mortem (root cause: por que cargo-audit miss?).
- Yanked dep introduced via Dependabot auto-merge → post-mortem + cargo-deny rule strengthen.
- SLSA attestation forge attempt detected → CRITICAL post-mortem + Security incident.
- Reproducible build divergence > 5% bit difference → post-mortem (debug non-determinism source).
- Vendor patch sem ADR mergeado em main → post-mortem + retroactive ADR + Security review.
- License audit miss (GPL leak) → CRITICAL post-mortem + Legal + remediation timeline.

## 19. Waiver policy

S-12 **NÃO PODE** promover via waiver dos seguintes itens:

- ❌ INV-SUPPLY-SIGNED-DEPLOY — no signed = no deploy.
- ❌ INV-SUPPLY-SBOM-PRESENT — no SBOM = no release.
- ❌ INV-SUPPLY-PROVENANCE-IN-REKOR — Rekor inclusion mandatory.
- ❌ cargo-audit zero HIGH/CRITICAL no release moment — security baseline.
- ❌ License allowlist (no GPL/AGPL/SSPL) — legal baseline.

Itens waivable com Security lead + Legal + ADR:

- ⚠️ Reproducible build < 100% bit-identical aceitável com documented sources.
- ⚠️ Yanked dep allowed se replacement não disponível AND vulnerability não applicable AND fix em < 7d.
- ⚠️ Vendor patch via `[patch.crates-io]` permitido com ADR + Security review + 90d sunset clock.

---

## 20. Changelog

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-24 | Gustavo (via Claude Opus 4.7) | Criação spec contract S-12. |
| 1.1.0 | 2026-04-29 | Gustavo (via Claude Opus 4.7) | SOTA v1.1 — INV-SUPPLY-PROVENANCE-IN-REKOR fail-closed hardening (no grace period). |
| 1.2.0 | 2026-05-13 | Gustavo (via Claude Sonnet 4.6) | WI-S12-001 SEALED: release-slsa3.yml + corelink-supply-verify + ADR-0045 + slsa-l3-pipeline.md. |
| 1.2.1 | 2026-05-14 | Gustavo (via Claude Sonnet 4.6) | WI-S12-002 SEALED: sbom-publish crate + sbom.yml + ADR-S12-001. |
| 1.2.2 | 2026-05-14 | Gustavo (via Claude Sonnet 4.6) | WI-S12-003 SEALED: corelink-deploy-verifier + cosign-sign.yml + ADR-0044. |
| 1.3.0 | 2026-05-14 | Gustavo (via Claude Sonnet 4.6) | WI-S12-004 SEALED: cargo-audit/deny/Dependabot workflows + deny.toml + corelink-supply-chain-policy + dep-policy.md + ADR-S12-045/046/047. |
| 1.3.1 | 2026-05-14 | Gustavo (via Claude Sonnet 4.6) | WI-S12-005 SEALED: corelink-dt-webhook + CLI + reconcile + DT infra + ADR-0037 + dt-dr-runbook.md. |
| 1.3.2 | 2026-05-14 | Gustavo (via Claude Sonnet 4.6) | WI-S12-006 SEALED: reproducible builds 2-runner diff + SOURCE_DATE_EPOCH + rust-toolchain.toml + ADR-0015 ratified. |
| 1.4.0 | 2026-05-14 | Gustavo (via Claude Sonnet 4.6) | WI-S12-007 SEALED: RB-FM-157 FROZEN + RB-FM-156/157 dry-run reports + PRR-S12 11 sign-offs CONDITIONALLY_APPROVED + security walkthrough P0=0 + adversarial summary 35 scenarios 100% + OWASP ASVS V14+V11.1+SSDF+EO14028 40/40. S-12 sprint SEALED. |

**Fim spec contract S-12 v1.4.0 SOTA.**
