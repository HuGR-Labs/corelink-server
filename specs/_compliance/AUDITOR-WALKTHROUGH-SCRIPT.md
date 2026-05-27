---
id: "AUDITOR-WALKTHROUGH-SCRIPT-2026-05-15"
type: "compliance_auditor_walkthrough"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
sprint: "R5-3"
parent_wi: "WI-R5-3-SOC2-ROLLUP"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
inherits_from: ["SOC2-EVIDENCE-ROLLUP-2026-05-15", "SOC2-GAP-ANALYSIS-2026-05-14", "SOC2-ROADMAP-TYPE1-2026-05-14"]
tags: ["soc2", "auditor", "walkthrough", "type-i-prep", "schellman", "a-lign"]
---

# SOC 2 Auditor Walkthrough Script — First Session (90 min)

> **doc_status:** DRAFT · **scope:** First fieldwork session with the selected SOC 2 Type I auditor (Schellman primary / A-LIGN secondary). 90-minute agenda + per-TSC area artifact list + pre-recorded screen-capture index + FAQ.
>
> **When this script applies:** T+4m..T+5m fieldwork window per `specs/_compliance/SOC2-ROADMAP.md` §M4-M5. The Drata Trust Center dashboard is shared read-only at T+1m; this is the first live session after PBC list v2 issuance.
>
> **Companion:** `specs/_compliance/SOC2-EVIDENCE-ROLLUP-2026-05-15.md` (master rollup) + `specs/_compliance/SOC2-GAP-ANALYSIS.md` (33 GAPs).

---

## 0. Pre-session checklist (run T-24h before the meeting)

| # | Item | Owner | Status |
|---|---|---|---|
| 0.1 | Drata dashboard ≥ 95% green (snapshot exported PDF) | Compliance | — |
| 0.2 | Auditor read-only access provisioned (Clerk SSO + GitHub audit-org membership; 30d expiry ticket) | Owner | — |
| 0.3 | PBC list v2 reviewed; mark items "ready" / "in-flight" / "blocked" | Compliance | — |
| 0.4 | Screen-captures uploaded to Drata "Evidence library" (see §3 below) | SRE | — |
| 0.5 | Owner + Compliance Officer + SRE Lead + Architect present | Owner | — |
| 0.6 | Conference room booked + recording consent obtained | Compliance | — |
| 0.7 | Backup speaker for each TSC area assigned (in case primary absent) | Owner | — |

---

## 1. Agenda — 90-minute walkthrough

| Time | TSC area | Lead | Artifacts to show | Live demo / Pre-recorded |
|---|---|---|---|---|
| 00:00-00:05 | Intro + scope reaffirmation | Owner | `SOC2-EVIDENCE-ROLLUP-2026-05-15.md` §1 + scope statement | — |
| 00:05-00:15 | CC1+CC2+CC3 (governance + comms + risk) | Owner + Compliance | `specs/_governance/` 13-role canonical + `specs/03_architecture/failure_modes.md` FM-XXX + `specs/_audits/matrix-stride-ctrl.csv` | Live: `git log --oneline specs/_governance/` |
| 00:15-00:25 | CC4+CC5 (monitoring + control activities) | Compliance + SRE | Drata dashboard 96.4% green + DASH-GA-READINESS Grafana + `compliance_matrix.md` CTRL-XXX index | Live: Grafana DASH-GA-READINESS demo |
| 00:25-00:40 | CC6 (logical + physical access) | Architect + Security + SRE | `auth_model.md` + Clerk SSO console + `compliance/byok-fips-matrix.md` + `legal/sub-processors.md` | Live: Clerk MFA enforcement check + signed-deploy verify (Cosign + Rekor) |
| 00:40-00:55 | CC7+CC8 (operations + change mgmt) | SRE + Engineering | RB-BREACH-NOTIF + WI-S20-006 synthetic page + PagerDuty schedule + GitHub branch protection settings | Pre-recorded: synthetic page firing video (3-min loop) |
| 00:55-01:00 | CC9 (vendor risk) | Compliance + Legal | `legal/sub-processors.md` + Drata vendor module | Live: Drata vendor view |
| 01:00-01:10 | A1 (availability) | SRE | SLO catalog + `region-outage-chaos-s14` summary + RB-DR-DRILL | Pre-recorded: chaos drill timeline (5-min) |
| 01:10-01:20 | C1 + PI1 (confidentiality + processing integrity) | Architect + Engineering | `byok-fips-matrix.md` + Merkle audit-chain explorer + INV-AUDIT-APPEND-ONLY | Live: Merkle audit-chain proof verification |
| 01:20-01:25 | Privacy (P-DSR + P-CONSENT + P-BREACH) | Privacy Officer + Legal | `privacy_model.md §6` + 6-field consent JWT receipt sample + RB-BREACH-NOTIF | Live: DSR export endpoint trigger (test tenant) |
| 01:25-01:30 | Q&A + next steps | Owner | PBC list v2 + remaining GAP ETA | — |

---

## 2. Per-TSC area artifact + dashboard + control-test bundles

### 2.1 CC1+CC2+CC3 — Governance, communications, risk

| Show | What auditor sees | Where |
|---|---|---|
| Doc | 13-role canonical sign-off + WI frontmatter validator | `specs/_governance/` + `scripts/validate_specs.py` output |
| Dashboard | DASH-COMPLIANCE-S20 (Drata + internal scorecard side-by-side) | Grafana — link in Drata Trust Center |
| Control test | Pick random WI from S-17..S-20 and trace 13 sign-offs in frontmatter; show `validate_specs.py` rejects malformed | Live: `python3 scripts/validate_specs.py specs/04_sprints/S20/WI-S20-006*.md` |

### 2.2 CC4+CC5 — Monitoring + control activities

| Show | What auditor sees | Where |
|---|---|---|
| Doc | `compliance_matrix.md` CTRL-XXX cumulative + PAT-XXX patterns | `specs/03_architecture/compliance_matrix.md` |
| Dashboard | DASH-GA-READINESS Grafana (SLO + error budget + Drata feed) | Grafana |
| Control test | Drata daily-tick log for `audit_logs` stream (yesterday's batch) — show `corelink.compliance.drata_evidence_sent` envelope with `receipt_id` | Live: `wrangler tail corelink-drata-sync --format pretty` |

### 2.3 CC6 — Logical & physical access

| Show | What auditor sees | Where |
|---|---|---|
| Doc | `auth_model.md` + Clerk RBAC + `byok-fips-matrix.md` + sub-processor list | `specs/03_architecture/auth_model.md` + `compliance/byok-fips-matrix.md` |
| Dashboard | Clerk admin (MFA enforcement %); BYOK key-rotation schedule | Clerk console + Drata custom view |
| Control test | Pick a random PAT issuance from past 30d → trace dual-approval + Rekor receipt + audit envelope | Live: `gh api ...` PR walkthrough + `rekor-cli get --uuid <hash>` |

### 2.4 CC7+CC8 — Operations + change management

| Show | What auditor sees | Where |
|---|---|---|
| Doc | RB-BREACH-NOTIF + RB-DR-DRILL + PagerDuty schedule + GitHub branch protection | `specs/05_quality/runbooks/RB-*.md` + repo settings page |
| Dashboard | PagerDuty incident timeline (past 30d); synthetic page weekly history | PagerDuty + Drata `incident_response` feed |
| Control test | Trigger synthetic page test in front of auditor; show end-to-end paging + acknowledgement + resolution log | Live: `./scripts/synthetic_page_drill.sh` — full loop ~4 min |

### 2.5 CC9 — Vendor risk

| Show | What auditor sees | Where |
|---|---|---|
| Doc | `legal/sub-processors.md` (14 entries; 10 documented + 4 in-flight per GAP-14) | repo |
| Dashboard | Drata vendor module (14 vendors × SOC 2 status × DPA on file × residency) | Drata |
| Control test | Pick one sub-processor (e.g., Cloudflare) → show: SOC 2 Type II report on file + DPA signed + residency clause | Live: Drata vendor click-through |

### 2.6 A1 — Availability

| Show | What auditor sees | Where |
|---|---|---|
| Doc | SLO catalog + RB-DR-DRILL + region-outage chaos summary | `specs/03_architecture/slo_catalog.md` + `specs/_audits/sealed/2026-05-14-region-outage-chaos-s14.md` |
| Dashboard | Grafana SLO panel (past 90d) + multi-region replication health | Grafana |
| Control test | Show region-outage chaos drill recording; if T+2m+ also show cold-restore drill (closes GAP-15) | Pre-recorded chaos drill |

### 2.7 C1 + PI1 — Confidentiality + processing integrity

| Show | What auditor sees | Where |
|---|---|---|
| Doc | `byok-fips-matrix.md` + Merkle audit chain spec + INV-AUDIT-APPEND-ONLY + reconcile job spec | `compliance/byok-fips-matrix.md` + `specs/03_architecture/observability_model.md §7` |
| Dashboard | Audit-chain integrity check (daily verification job) + BYOK envelope-encryption metrics | Grafana DASH-COMPLIANCE-S20 |
| Control test | Pull random audit-event from past 7d → verify Merkle proof on the spot | Live: `./scripts/audit_chain_verify.sh <event_id>` |

### 2.8 Privacy — DSR + consent + breach

| Show | What auditor sees | Where |
|---|---|---|
| Doc | `privacy_model.md §6` DSR + `legal/dpa/` + LIA template + RB-BREACH-NOTIF | repo |
| Dashboard | DSR ticket queue (past 90d) + consent-receipt issuance rate | Drata `access_reviews` + `audit_logs` feeds |
| Control test | Trigger DSR-EXPORT on test tenant → show 6-field consent receipt JWT + portability JSON download | Live: portal walkthrough |

---

## 3. Pre-recorded screen-capture list

Captured + uploaded to Drata Evidence Library; auditor can replay async without owner present.

| # | Capture | Duration | What it shows | Drata path |
|---|---|---|---|---|
| 1 | Synthetic page firing end-to-end | 4 min | Workflow trigger → PagerDuty page → on-call ack → resolution → audit envelope | `evidence/incident_response/2026-05-15-synthetic-page.mp4` |
| 2 | Region-outage chaos drill timeline | 5 min | Region failover → traffic redirect → recovery within SLO | `evidence/availability/2026-05-14-region-outage-chaos-s14.mp4` |
| 3 | BYOK kill-switch drill | 3 min | Tenant revokes BYOK key → encrypt-fail surfaces; decrypt of historical fails per design | `evidence/confidentiality/2026-05-14-byok-kill-switch.mp4` |
| 4 | Dual-approval admin operation | 2 min | Owner initiates → second approver required → audit envelope + Rekor receipt | `evidence/access_control/2026-05-15-dual-approval.mp4` |
| 5 | Signed-deploy + Cosign verify at deploy time | 2 min | CI builds → Cosign signs → Rekor logs → deploy gate verifies | `evidence/change_management/2026-05-15-signed-deploy.mp4` |
| 6 | DSR export end-to-end | 3 min | Tenant submits DSR → portal validates → export JSON delivered + audit envelope | `evidence/privacy/2026-05-15-dsr-export.mp4` |
| 7 | Drata daily tick + ledger | 2 min | `corelink-drata-sync` cron tick → 6 streams POSTed → D1 ledger row inserted | `evidence/monitoring/2026-05-15-drata-tick.mp4` |
| 8 | Audit-chain Merkle proof verify | 2 min | Pull random event → re-compute hash chain → verify root signature | `evidence/processing_integrity/2026-05-15-merkle-verify.mp4` |
| 9 | Onboarding DPA-first gate | 2 min | New tenant signup → DPA acceptance forced before any data write | `evidence/privacy/2026-05-15-dpa-first.mp4` |
| 10 | Quarterly access review (when first cycle done) | 4 min | Drata-driven review → owner attests → audit envelope | TBD T+1m (GAP-01) |

---

## 4. FAQ — 10 expected auditor questions + canonical answers

### Q1. "How do you guarantee the audit-log chain hasn't been tampered with?"

**A.** Audit events are written append-only to `audit_outbox` (D1) and replicated to R2 with Object Lock Governance Mode (7y retention). Each event is hashed (BLAKE3) and chained to the prior event's hash. A daily verification job (PAT-AUDIT-VERIFY-001) re-walks the chain, re-computes the Merkle root, and compares against the signed root from `T-1d`. Root is signed Ed25519 with a key held in HashiCorp Vault (rotated quarterly). Invariants: INV-AUDIT-APPEND-ONLY + INV-OBS-AUDIT-CHAIN-INTEGRITY. **Demo:** `./scripts/audit_chain_verify.sh <event_id>` returns proof in <2s.

### Q2. "What is your BYOK posture for tenants with regulated data?"

**A.** BYOK envelopes are documented in `compliance/byok-fips-matrix.md`. Tenants bring their own KMS key (AWS KMS / GCP Cloud KMS / Azure Key Vault / HashiCorp Vault). CoreLink envelope-wraps a tenant-scoped data-encryption key (DEK) with the customer's key-encryption key (KEK). FIPS attestation status: **AWS KMS FIPS 140-2 L3 attested**; GCP FIPS 140-3 L1 vendor letter pending (GAP-02 closing D+30); Azure Key Vault attestation pending. Tenant can revoke the KEK at any time (BYOK kill-switch drill: `2026-05-14-byok-kill-switch-drill-aws.md`). Invariant: INV-BYOK-CRYPTO-SOVEREIGNTY.

### Q3. "How do you handle quarterly access reviews?"

**A.** Currently transitioning from ad-hoc to automated. GAP-01 + GAP-26 track the automation: Drata `access_reviews` stream pulls Clerk role assignments + tenant ACL changes nightly. Owner attests quarterly via signed receipt → emitted as `corelink.compliance.access_review_completed` audit envelope. First fully-automated cycle T+1m. Compensating control until automation lands: manual export from Drata + owner-signed attestation in `specs/_governance/access-review-quarterly.md`.

### Q4. "What is your incident response cadence and how is it tested?"

**A.** PagerDuty 24/7 rotation across 3 regions (per WI-S20-006). Synthetic page fires weekly via cron worker → triggers a non-prod PagerDuty incident → tests end-to-end paging + ack + comms. Tabletop drills tracked in `specs/_compliance/BCP-DR-DRILL-CADENCE.md` (14 drills covering P1/P2/P3 severities). GAP-03 tracks full IR tabletop with lighthouse-customer participation (ETA D+60). Recent drills: RB-FM-105 dry-run, region-outage chaos S-14, byok-kill-switch S-17 tabletop.

### Q5. "How do sub-processor changes get communicated to customers?"

**A.** `legal/sub-processors.md` is the source-of-truth (also published at `/privacy/sub-processors`). DPA §X.Y commits us to 30-day advance notice on material sub-processor changes; customer email triggered from this list via Drata `change_management` event when the file is modified in a merged PR. GAP-21 tracks automation polish (current cadence is manual email); GAP-14 tracks completing the register for the 4 pending sub-processors (Sentry / Stripe Atlas counsel / PostHog / LogRocket).

### Q6. "How do you ensure data residency commitments are honored?"

**A.** Tenants select region at onboarding (sam / weur / use / usw). All storage (R2 + D1 + DO + KV) is region-pinned per tenant; Cloudflare Workers respect tenant region via DO namespace routing. INV-DATA-RESIDENCY enforces this at runtime. Cross-region replication is **disabled by default**; opt-in only with explicit DPA addendum. GAP-22 tracks LGPD Art. 33 §1º per-region attestation refresh (D+60).

### Q7. "What happens when a tenant requests erasure (right to be forgotten)?"

**A.** RB-DSR-INTAKE-FAILURE + RB-DSR-ERASURE-INCOMPLETE + RB-GDPR-ERASURE-HOLD govern the path. Tenant submits DSR → DSR portal validates → erasure job runs across R2 + D1 + DO + KV + audit-outbox (excluding immutable audit envelopes which are pseudonymized via salt rotation per ADR-S11-003). Completion attestation signed Ed25519 (INV-ERASURE-ATTESTATION-SIGNED). Audit envelope `corelink.privacy.dsr_completed` emitted. Demo via DSR export pre-recorded capture #6 above. Open gap: GAP-17 (portability format GDPR Art. 20 validation).

### Q8. "How do you prevent unauthorized software from running in production?"

**A.** Cloudflare Workers + Pages are immutable post-deploy (no exec path in runtime). All deploys go through: PR merge → CI builds binary → Cosign signs → Rekor logs (INV-SUPPLY-PROVENANCE-IN-REKOR) → deploy gate verifies Cosign signature against expected identity (INV-SUPPLY-SIGNED-DEPLOY). Dependency tracked via SBOM signed (INV-SUPPLY-SBOM-PRESENT) + license allowlist (INV-SUPPLY-LICENSE-ALLOWLIST) + cargo-deny + cargo-fuzz. GAP-11 tracks runtime drift detection (deferred to T+6m; compensating control = immutable-image attestation).

### Q9. "What's the readiness for SOC 2 Type II observation?"

**A.** Type II observation window opens T+6m (immediately after Type I report delivery). Drata is already collecting evidence continuously since T-30d (this snapshot). The 6 EvidenceStream variants (`audit_logs`, `access_reviews`, `credential_management`, `change_management`, `incident_response`, `vulnerability_management`) provide 90.7% auto-coverage (95.3% effective). All major gaps close by T+3m before Type I fieldwork; minor gaps close T+6m..T+12m. Type II target fieldwork T+15m..T+18m; budget $50-80k engagement.

### Q10. "If we find a material weakness, what's your remediation cadence?"

**A.** Weekly compliance review meeting (Owner + Compliance Officer; GAP-08 sets cadence by D+30) catches drift early. Drata drift alerts fire on >1% week-over-week regression → triggers SRE investigation. Severity-tiered SLA: Critical = 24h triage + 7d remediation; High = 7d triage + 30d remediation; Medium = 30d triage + 90d remediation. GAP-29 formalizes the SLA per severity (ETA D+30). For audit-discovered material weaknesses: management response within 30d documenting remediation plan + compensating controls; standalone ADR per finding.

---

## 5. Auditor access provisioning (T-7d before walkthrough)

| Resource | Access type | Expiry | Ticket |
|---|---|---|---|
| Drata Trust Center | Read-only via auditor email | 60d post-fieldwork | DRATA-AUDIT-{date} |
| GitHub `humangr-labs/corelink-server` | Audit-org read membership | 60d post-fieldwork | GH-AUDIT-{date} |
| Grafana Cloud (read-only dashboards) | SSO with auditor email | 30d | GRAF-AUDIT-{date} |
| Cloudflare logs export (audit_logs stream) | Time-bounded R2 presigned URL | 7d rolling | CF-AUDIT-{date} |
| PagerDuty incident export | Read-only via auditor email | 30d | PD-AUDIT-{date} |

All auditor queries logged in meta-audit (`corelink.compliance.auditor_query`) for chain-of-custody.

---

## 6. Post-walkthrough deliverables

| # | Item | Owner | Due |
|---|---|---|---|
| 6.1 | Walkthrough notes (auditor-owned) | Auditor | T+1d |
| 6.2 | PBC list v2 final (refined per walkthrough) | Auditor + Compliance | T+3d |
| 6.3 | Sample population agreement (typically 25 access events + 25 change events + 25 incidents) | Compliance | T+5d |
| 6.4 | Control descriptions reviewed + signed | Owner | T+7d |
| 6.5 | Schedule for follow-up walkthroughs (per CC6 deep-dive, etc.) | Compliance | T+7d |

---

## 7. Acceptance per R5-3 contract

- 90-minute agenda mapping each TSC area to lead + artifacts + live/recorded demo.
- Per-TSC artifact + dashboard + control-test bundle (8 areas).
- 10 pre-recorded screen-captures indexed with Drata path.
- 10 FAQ Q&A covering most likely auditor probes.
- Auditor access-provisioning matrix with expiry + ticket convention.

---

## 8. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-05-15 | Gustavo (via Claude Sonnet 4.7 R5-3 builder) | Initial walkthrough script; 90-min agenda + 8-area bundle + 10 captures + 10 FAQ; aligned with SOC2-ROADMAP M3-M4 phase. |

---

**Fim AUDITOR-WALKTHROUGH-SCRIPT.**
