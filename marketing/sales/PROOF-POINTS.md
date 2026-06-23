---
id: "SALES-PROOF-POINTS"
type: "marketing"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
parent: "R-PREP-SALES-ENABLEMENT"
tags: ["sales", "proof-points", "r-prep", "ga", "numeric-claims", "sources"]
---

# CoreLink Sales Proof Points — Numeric Claims With Sources

> **Rule:** every numeric or absolute claim used in a sales conversation must trace to a doc, a commit, a spec, or a recorded measurement. This file is the canonical mapping.
> **Audience:** Founder, Customer Success, partner SE. If you cite a number not in this table, file it here.
> **Companion docs:** `marketing/sales/FAQ-MASTER.md`, `marketing/sales/OBJECTION-HANDLING.md`, `marketing/sales/COMPETITIVE-MATRIX.md`.

---

## Headline proof points (top 12)

1. **p99 cache-hit latency: target ≤ 300 ms in-region, typical observed 180–220 ms.** Source: SLO catalog `SLO-LAT-CAS-GET`; lighthouse Customer Playbook `marketing/lighthouse-kit/CUSTOMER-PLAYBOOK.md#what-were-measuring-daily-automated`.
2. **Cache availability SLO: ≥ 99.9% per-customer.** Source: SLO catalog `cache-availability`; Lighthouse Customer Playbook table.
3. **Tenant isolation: TLA+ model-checked, gated in CI.** Source: `tenant_isolation.tla` formal spec; `INV-TenantIsolation` CRITICAL invariant; `apps/docs/docs/trust/index.mdx#posture-at-a-glance`.
4. **0 cross-tenant leaks ever.** Source: adversarial E2E test suite; audit-chain reconciliation; external pentest tenant-isolation scenario (clean + clean retest).
5. **BYOK kill-switch round-trip: ≤ 5 minutes hard cap.** Source: DEK cache TTL code path (`crates/corelink-crypto-envelope/src/dek_cache.rs`); `INV-BYOK-CRYPTO-SOVEREIGNTY` CRITICAL invariant; `BLOG-POSTS/02-byok-deep-dive.md#bounded-dek-cache`; lighthouse SLO `byok-kill-switch-rtt`.
6. **Audit chain: RFC 6962 + RFC 8785 JCS, 7-year retention.** Source: `BLOG-POSTS/03-audit-chain-merkle-proofs.md`; `audit_immutability.tla`; `INV-AUDIT-APPEND-ONLY` + `INV-OBS-AUDIT-CHAIN-INTEGRITY` CRITICAL invariants; `apps/docs/docs/trust/data-handling.mdx#retention`. **In-browser proof verifier** at `/customer/audit/visualization` (spec: `specs/_audits/sealed/2026-05-15-audit-viz-spec.md`) — auditors verify inclusion proofs in a tab, no CLI install required.
7. **SOC 2 readiness: 83.7% weighted (113/135 weighted criterion-points green).** Source: `specs/_compliance/SOC2-EVIDENCE-ROLLUP-2026-05-15.md`; Drata dashboard; `apps/docs/docs/trust/compliance.mdx#soc-2`.
8. **ISO 27001:2022 Annex A in-scope crosswalk: 98.9%.** Source: `specs/_compliance/ISO27001-CROSSWALK-2026-05-15.md`; `apps/docs/docs/trust/iso27001.mdx#headline-numbers`.
9. **PCI DSS SAQ-A compliant (self-attested 2026-05-15).** Source: `specs/_compliance/PCI-DSS-SAQ-A-2026-05-15.md`; `apps/docs/docs/trust/pci-dss.mdx`.
10. **Breach notification: 72h to authorities, 24h to affected enterprise tenants.** Source: `specs/_runbooks/RB-BREACH-NOTIF.md`; `apps/docs/docs/trust/incident-response.mdx#breach-notification`.
11. **PAT revocation propagation: < 60s globally (P95).** Source: FM-061 rotation drill; `apps/docs/docs/tutorials/quickstart-faq.mdx#10`.
12. **24/7 on-call across 3 regions, weekly synthetic page sustained 30d pre-GA.** Source: `marketing/launch/STATUS-PAGE-SPEC.md`; `apps/docs/docs/trust/incident-response.mdx#reporting-an-incident-to-us`.

---

## 1. Performance claims

| # | Claim | Source |
| --- | --- | --- |
| 1.1 | p99 CAS GET latency target ≤ 300 ms in-region | SLO catalog `SLO-LAT-CAS-GET`; `BLOG-POSTS/05-fast-cache-hit-economics.md` |
| 1.2 | Typical observed p99 CAS GET: 180–220 ms (region-dependent) | Lighthouse 30-day SLA samples; `CUSTOMER-PLAYBOOK.md#daily-metrics-report--sample` |
| 1.3 | p99 audit-append latency ≤ 500 ms | SLO catalog `audit-append-latency-p99`; `CUSTOMER-PLAYBOOK.md` |
| 1.4 | Cache availability ≥ 99.9% per-customer SLO (Lighthouse tier) | SLO catalog `cache-availability` |
| 1.5 | TPS ceiling per tier: Free 100, Team 500, Lighthouse 5000, Enterprise negotiated | `apps/docs/docs/tutorials/quickstart-faq.mdx#8` |
| 1.6 | BYOK unwrap latency p99 ≤ 100 ms (read path; cache hit path does not call KMS) | `BLOG-POSTS/02-byok-deep-dive.md#the-four-providers` |
| 1.7 | Cold-start ramp to steady state: typically D+1 to D+7 | `CUSTOMER-PLAYBOOK.md#what-to-look-for-in-week-1`; `BLOG-POSTS/05-fast-cache-hit-economics.md#honest-caveats` |
| 1.8 | Zero egress fees on cache reads (R2 substrate) | `BLOG-POSTS/05-fast-cache-hit-economics.md#zero-egress-pricing-on-cloudflare-r2` |

## 2. Security claims

| # | Claim | Source |
| --- | --- | --- |
| 2.1 | Tenant isolation modeled in `tenant_isolation.tla`, gated in CI | `INV-TenantIsolation`; `apps/docs/docs/trust/index.mdx` |
| 2.2 | 0 cross-tenant leaks ever | Adversarial E2E + audit-chain reconciliation; pentest tenant-isolation scenario clean + retest clean |
| 2.3 | BYOK across 4 KMS providers (AWS / GCP / Azure / Vault) | `BLOG-POSTS/02-byok-deep-dive.md#the-four-providers` |
| 2.4 | DEK cache TTL hard-capped at 5 minutes (code path, not config) | `crates/corelink-crypto-envelope/src/dek_cache.rs`; ADR-S14-004 |
| 2.5 | AAD binding: `tenant_id \|\| blob_hash \|\| cache_id` enforced on every wrap | `BLOG-POSTS/02-byok-deep-dive.md#aad-binding-cryptographic-locality` |
| 2.6 | AEAD primitives: AES-256-GCM, ChaCha20-Poly1305 per provider matrix | `BLOG-POSTS/02-byok-deep-dive.md#the-four-providers` |
| 2.7 | TLS 1.3 mandatory; HSTS `max-age=63072000; includeSubDomains; preload` | `apps/docs/docs/trust/data-handling.mdx#in-flight` |
| 2.8 | mTLS edge-to-origin (CTRL-NET-002) | `apps/docs/docs/trust/data-handling.mdx#in-flight` |
| 2.9 | Client-side BLAKE3 re-hash default-on (CTRL-CAS-002); mismatch raises `COR_CAS_DIGEST_MISMATCH` | `apps/docs/docs/tutorials/quickstart-faq.mdx#12` |
| 2.10 | `--pat` CLI flag rejected by design (CTRL-CRED-001) | `apps/docs/docs/tutorials/quickstart-faq.mdx#11` |
| 2.11 | Ed25519-signed erasure attestation; NIST SP 800-88 Rev. 1 crypto-erase classification | `BLOG-POSTS/02-byok-deep-dive.md#erasure-attestation`; `INV-ERASURE-ATTESTATION-SIGNED` |
| 2.12 | External pentest (Pentest-1 firm) clean with post-remediation retest pre-GA | `BLOG-POSTS/01-introducing-corelink.md`; spec contract S-20 §6.1 |
| 2.13 | Weekly synthetic BYOK chaos drill on lighthouse tenants | `CUSTOMER-PLAYBOOK.md#what-to-look-for-what-to-escalate` |
| 2.14 | Dual-approval gate on 5 destructive admin ops (`ConfigRollback`, `RetentionPolicyReduce`, `FeatureFlagDisable`, `SecretRotationStart`, `TenantTombstone`) | `CUSTOMER-PLAYBOOK.md#within-48h-of-the-call`; `PAT-DUAL-APPROVAL-001` |
| 2.15 | Signed-deploy pipeline + Rekor transparency-log entries per release | `apps/docs/docs/trust/compliance.mdx#what-you-can-rely-on-today-pre-type-i` |
| 2.16 | 100% test kill rate on BYOK | Mutants baseline doc — `specs/_qa/MUTANTS-BASELINE-BYOK.md`; mutants expansion R-PREP commit `efa56b7` |

## 3. Compliance claims

| # | Claim | Source |
| --- | --- | --- |
| 3.1 | SOC 2 readiness 83.7% weighted (113/135 weighted criterion-points green; 2 reds, both BYOK-FIPS attestation gaps with D+30 closure cap) | `specs/_compliance/SOC2-EVIDENCE-ROLLUP-2026-05-15.md`; Drata dashboard; `apps/docs/docs/trust/compliance.mdx#soc-2` |
| 3.2 | SOC 2 Type I fieldwork Q4 2026; report Q1 2027 | `apps/docs/docs/trust/compliance.mdx#status` |
| 3.3 | SOC 2 Type II fieldwork Q3 2027; report Q4 2027 | `apps/docs/docs/trust/compliance.mdx#status` |
| 3.4 | SOC 2 auditor: Schellman & Co. LLC (engagement letter executed) | `apps/docs/docs/trust/compliance.mdx#status` |
| 3.5 | Drata-continuous evidence; 90.7% auto-collection rate | `apps/docs/docs/trust/iso27001.mdx#headline-numbers` |
| 3.6 | ISO 27001:2022 Annex A in-scope crosswalk 98.9% (89/90 applicable) | `specs/_compliance/ISO27001-CROSSWALK-2026-05-15.md`; `apps/docs/docs/trust/iso27001.mdx#headline-numbers` |
| 3.7 | ISO 27001 cert target Q1-2027 (Stage 1 Q4-2026 stacked with SOC 2 Type I) | `apps/docs/docs/trust/iso27001.mdx` |
| 3.8 | ISO/SOC overlap 91% via Drata | `apps/docs/docs/trust/iso27001.mdx` |
| 3.9 | First-year ISO cert cost envelope $55–90k | `apps/docs/docs/trust/iso27001.mdx` |
| 3.10 | NIST 800-53 Rev 5 Moderate crosswalk 87% (informational) | `specs/_compliance/NIST-800-53-CROSSWALK.md`; `apps/docs/docs/trust/compliance.mdx#quick-scope-map` |
| 3.11 | LGPD compliant (processor); DPO in place. EU residency **live** (EU tenants' data stays in physically-EU R2). Brazilian-tenant data is US/ENAM-stored under SCCs; `sam` physical residency is **roadmap** — Cloudflare R2 has no South-America region (the prior `sam` Art. 33 §1º attestation is SUPERSEDED) | `apps/docs/docs/trust/compliance.mdx#lgpd-brazil--lei-geral-de-proteção-de-dados`; `apps/docs/docs/trust/data-handling.mdx#residency` |
| 3.12 | LGPD DSR turnaround: 5 business days acknowledge, 15 business days resolution | `apps/docs/docs/trust/compliance.mdx#lgpd` |
| 3.13 | GDPR compliant (processor; joint controller for limited service-telemetry); DPA `legal/dpa/v1.0.0` | `apps/docs/docs/trust/compliance.mdx#gdpr` |
| 3.14 | DPA reviewed in 3 locales by external counsel (EN EU+UK / PT-BR / ES LATAM) | `BLOG-POSTS/04-multi-region-residency.md#three-locale-legal-review` |
| 3.15 | PCI DSS v4.0 SAQ-A compliant (self-attested 2026-05-15); annual recert 2027-05-15 | `specs/_compliance/PCI-DSS-SAQ-A-2026-05-15.md`; `apps/docs/docs/trust/pci-dss.mdx` |
| 3.16 | HIPAA out-of-scope by design; no BAAs signed | `apps/docs/docs/trust/compliance.mdx#hipaa` |
| 3.17 | FedRAMP not in roadmap; NIST 800-53 crosswalk informational | `apps/docs/docs/trust/compliance.mdx#quick-scope-map` |
| 3.18 | Sub-processor change notice: 30 calendar days advance | `apps/docs/docs/trust/subprocessors.mdx#notice-of-changes-30-day-grace` |
| 3.19 | Vendor risk register: 19 vendors; 11 active sub-processors at GA | `specs/_compliance/VENDOR-RISK-REGISTER.md`; `apps/docs/docs/trust/subprocessors.mdx` |

## 4. Operations claims

| # | Claim | Source |
| --- | --- | --- |
| 4.1 | 24/7 on-call across 3 regions per PagerDuty rotation | `apps/docs/docs/trust/incident-response.mdx#reporting-an-incident-to-us` |
| 4.2 | Weekly synthetic page (Monday 14:00 UTC) sustained 30 days pre-GA | `marketing/launch/STATUS-PAGE-SPEC.md`; `apps/docs/docs/trust/incident-response.mdx` |
| 4.3 | PagerDuty page acknowledgement ≤ 10 min 24/7 | `CUSTOMER-PLAYBOOK.md#support-slas` |
| 4.4 | SEV1 status-page post within 5 min; email to affected within 30 min | `apps/docs/docs/trust/incident-response.mdx#communicating-during-a-sev1` |
| 4.5 | SEV1 public post-mortem within 72h; T+14d internal review | `apps/docs/docs/trust/incident-response.mdx#communicating-during-a-sev1` |
| 4.6 | Status page operated by Atlassian Statuspage, isolated from production fabric; 8 components tracked | `apps/docs/docs/trust/incident-response.mdx#status-page` |
| 4.7 | Backup snapshots: 35-day rolling retention | `apps/docs/docs/trust/data-handling.mdx#retention` |
| 4.8 | Audit chain retention: 7 years | `apps/docs/docs/trust/data-handling.mdx#retention` |
| 4.9 | Telemetry aggregates: 13-month retention (no customer PII, aggregates only) | `apps/docs/docs/trust/data-handling.mdx#retention` |
| 4.10 | Slack Connect acknowledgement (Enterprise / Lighthouse): ≤ 1h business, ≤ 4h outside | `CUSTOMER-PLAYBOOK.md#support-slas` |
| 4.11 | Engineering response on classified P1: ≤ 24 hours | `CUSTOMER-PLAYBOOK.md#support-slas` |
| 4.12 | 0 customer-impacting SEV1 incidents to date | `apps/docs/docs/trust/incident-response.mdx#past-incidents` |
| 4.13 | First IR tabletop session TT-01 scheduled 2026-07-22 | `apps/docs/docs/trust/incident-response.mdx#past-incidents` |

## 5. Engineering / GA gate claims

| # | Claim | Source |
| --- | --- | --- |
| 5.1 | 21 engineering sprints sealed at GA | `marketing/launch/BLOG-POSTS/01-introducing-corelink.md`; corelink_spec_baseline memory note |
| 5.2 | 7 engineering work items sealed across S-20 | spec contract S-20 §6.1; `BLOG-POSTS/01` "What 'GA' means" |
| 5.3 | PRR globally approved across 13 canonical sign-offs | spec contract S-20 §6.1 |
| 5.4 | 30 days sustained staging pre-GA | spec contract S-20 §6.1 |
| 5.5 | 3 lighthouse customers with SLA met | `marketing/lighthouse-kit/CUSTOMER-PLAYBOOK.md`; `specs/_lighthouse/lighthouse-customer-program.md` |
| 5.6 | Lighthouse engagement timebox: 60 days; customer engineering ≤ 8 hours total | `CUSTOMER-PLAYBOOK.md#welcome` |
| 5.7 | Lighthouse program cap: never more than 3 concurrent customers | `CUSTOMER-PLAYBOOK.md#welcome` |
| 5.8 | 4 enumerated GA regions (WNAM / ENAM / WEUR / SAM); 3 on request (`oce` / `apc` / `mea`) | `BLOG-POSTS/04-multi-region-residency.md`; `apps/docs/docs/trust/data-handling.mdx#residency` |
| 5.9 | `INV-REGION-NO-CROSS-LEAK` CRITICAL invariant | spec contract registry; `BLOG-POSTS/04` |
| 5.10 | 4 KMS providers at GA (AWS / GCP / Azure / Vault) | `BLOG-POSTS/02-byok-deep-dive.md` |
| 5.11 | 4 canonical SDKs at GA (Rust / Python / Go / JS-TS-Browser) all wrapping `corelink-client-verify` | `apps/docs/docs/tutorials/quickstart-faq.mdx#6` |
| 5.12 | Roadmap-post-GA: Java/Kotlin Q3, Ruby Q4, C#/.NET Q4, Swift community-driven | `apps/docs/docs/tutorials/quickstart-faq.mdx#7` |

## 6. Mutants / test-kill claims (R-PREP)

| # | Claim | Source |
| --- | --- | --- |
| 6.1 | 100% test kill rate on BYOK | `specs/_qa/MUTANTS-BASELINE-BYOK.md`; commit `efa56b7` (mutants expansion R-PREP) |
| 6.2 | Mutants expansion: 5 crates × 75 targeted tests | commit `efa56b7` |
| 6.3 | CI matrix expanded 3 → 8 | commit `efa56b7` |

## 7. Product structure / invariants

| # | Claim | Source |
| --- | --- | --- |
| 7.1 | 4 formal TLA+ specs in CI: `tenant_isolation.tla`, `cas_integrity.tla`, `audit_immutability.tla`, `gc_correctness.tla` | `BLOG-POSTS/01-introducing-corelink.md` |
| 7.2 | Content addressing: BLAKE3 default, SHA-256 for REAPI compatibility | `BLOG-POSTS/01-introducing-corelink.md`; `apps/docs/docs/tutorials/quickstart-faq.mdx#12` |
| 7.3 | Audit-chain leaf canonicalization: RFC 8785 JCS; tree construction: RFC 6962 | `BLOG-POSTS/03-audit-chain-merkle-proofs.md` |
| 7.4 | Daily audit-chain head publication | `BLOG-POSTS/03-audit-chain-merkle-proofs.md` |
| 7.5 | Daily LGPD residency verification (nightly cron) | `scripts/verify-lgpd-residency.py` |
| 7.6 | Cross-tenant ciphertext convergent encryption: deliberately not implemented | `BLOG-POSTS/02-byok-deep-dive.md#trade-offs-we-made` |

---

## How to add a new proof point

1. Confirm the underlying source exists (doc / spec / commit / measurement).
2. Add a row in the relevant section table.
3. If the claim is *new* to sales conversation (not just rephrased), surface it in `FAQ-MASTER.md` and the relevant blog post.
4. If the source moves, update the path here and break the `validate_specs.py` cross-link check intentionally so it's caught.

## Cross-references

- **FAQ:** `marketing/sales/FAQ-MASTER.md`.
- **Objection handling:** `marketing/sales/OBJECTION-HANDLING.md`.
- **Competitive matrix:** `marketing/sales/COMPETITIVE-MATRIX.md`.
- **Trust Center:** `apps/docs/docs/trust/`.
- **Lighthouse playbook:** `marketing/lighthouse-kit/CUSTOMER-PLAYBOOK.md`.
- **Launch checklist V2:** `marketing/launch/LAUNCH-CHECKLIST-V2.md`.

---

**Fim SALES-PROOF-POINTS.**
