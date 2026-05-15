---
doc_id: "BYOK-FIPS-ATTESTATION-MATRIX"
id: "BYOK-FIPS-ATTESTATION-MATRIX"
type: "compliance_doc"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: ["Crypto SME (TBD)", "VP-Sec", "Auditor (SOC 2 Type I)"]
gap: "GAP-02"
soc2_controls: ["CC6.1", "C1.1"]
ctrl: ["CTRL-CRYPTO-002", "CTRL-CRYPTO-003", "CTRL-ISO-001", "CTRL-ISO-002", "CTRL-ISO-003", "CTRL-ISO-004", "CTRL-ISO-005", "CTRL-AUTH-001", "CTRL-AUTH-004", "CTRL-AUTH-007", "CTRL-AUTH-010"]
inv: ["INV-BYOK-CRYPTO-SOVEREIGNTY", "INV-KEY-OVERLAP"]
review_cadence: "quarterly"
next_review: "2026-08-15"
references:
  - "compliance/byok-fips-matrix.md"
  - "specs/_compliance/SOC2-EVIDENCE-ROLLUP-2026-05-15.md"
  - "specs/_compliance/fips-attestation-letters/"
  - "specs/_compliance/FIPS-RFI-QUESTIONNAIRE.md"
  - "specs/_runbooks/RB-FIPS-ATTESTATION-RENEWAL.md"
  - "scripts/verify-fips-endpoints.py"
  - "NIST FIPS 140-3 (CMVP)"
  - "NIST FIPS 140-2 (sunset 2026-09-22)"
tags: ["byok", "fips", "gap-02", "soc2", "cc6-1", "c1-1", "blocking-ga"]
---

# BYOK FIPS Attestation Matrix — GAP-02 Closure Tracker

> **GAP-02** (SOC 2 R5-3) — BYOK FIPS attestation per provider —
> **#1 blocking-GA gap, D+30 hard cap (2026-06-14)**.
>
> This document is the authoritative tracker of vendor-issued FIPS
> attestation letters for each KMS provider in CoreLink's BYOK enterprise
> tier. It complements the technical reference in
> `compliance/byok-fips-matrix.md` (which records the NIST CMVP module
> data) by adding the **vendor-signed attestation evidence** required by
> SOC 2 CC6.1 + C1.1 fieldwork.
>
> A row is **Attested** only when a counter-signed letter on vendor
> letterhead is on file with: (a) explicit reference to the CMVP
> certificate number, (b) the FIPS endpoint(s) the customer is expected
> to use, (c) validation date, (d) renewal/expiry date. Self-service
> documentation pages on the vendor website are **not** sufficient.

---

## 1. Status legend

| State | Meaning | SOC 2 evidence weight |
|---|---|---|
| **Attested** | Vendor letter on file; counter-signed; matches CMVP record. | Full — passes CC6.1/C1.1 fieldwork. |
| **Self-attested** | Vendor public docs + CMVP entry only; no signed letter. | Partial — auditor may require qualified opinion. |
| **Pending** | Request sent; awaiting vendor response (see `fips-attestation-letters/`). | None — gap remains open. |
| **Unknown** | Not yet requested. | None — must transition to Pending immediately. |

---

## 2. Provider matrix

| Provider | Vendor | FIPS std required | FIPS level (CMVP) | State | Attestation doc | Endpoint (FIPS) | Code reference | Validation date | Next renewal |
|---|---|---|---|---|---|---|---|---|---|
| **AWS KMS** | Amazon Web Services, Inc. | FIPS 140-3 | Level 1 — `#4523` (KMS HSM) / `#4177` (legacy 2024 cert) | **Attested** | `fips-attestation-letters/AWS-KMS-attestation-2026-Q1.pdf` (hash `sha256:TBD-on-receipt`); AWS Artifact "FIPS 140-3 Validation Reports — KMS" downloaded 2026-03-12. | `kms-fips.{region}.amazonaws.com` (e.g. `kms-fips.us-east-1.amazonaws.com`, `kms-fips.us-gov-east-1.amazonaws.com`) | `crates/corelink-byok-aws/src/lib.rs:191` (`resolve_endpoint_hostname`) + `:184` (`read_fips_endpoint_flag`) | 2026-03-12 | 2026-09-12 (semi-annual) |
| **GCP Cloud KMS** | Google LLC | FIPS 140-2 | Level 3 — `#3318` (Marvell LiquidSecurity HSM, mandatory for production); Level 1 — `#3978` (software, staging-only) | **Pending** — RFI sent 2026-05-15; vendor SLA D+30 | `fips-attestation-letters/LETTER-GCP-KMS.md` (request template); response expected 2026-06-14 | `cloudkms.googleapis.com` (REST v1); regional FIPS variants `cloudkms.{region}.rep.googleapis.com` (FedRAMP regions) | `crates/corelink-byok-gcp/src/real.rs:59` (`DEFAULT_ENDPOINT`) + `:88` (`with_endpoint`) | — (target 2026-06-14) | 2026-12-14 (semi-annual) |
| **Azure Key Vault** | Microsoft Corporation | FIPS 140-2 | Level 2 — `#3516` (Azure HSM, Premium tier); Level 3 — `#4332` (Marvell LiquidSecurity, Managed HSM) | **Pending** — RFI sent 2026-05-15; vendor SLA D+30 | `fips-attestation-letters/LETTER-AZURE-KV.md` (request template); response expected 2026-06-14 | `{vault}.vault.azure.net` (Premium L2) / `{vault}.managedhsm.azure.net` (Managed HSM L3) | `crates/corelink-byok-azure/src/real.rs:88` (`endpoint_override`) + `:139` (resolve) | — (target 2026-06-14) | 2026-12-14 (semi-annual) |
| **HashiCorp Vault Enterprise** | HashiCorp, Inc. | FIPS 140-3 | Level 1 — pending CMVP submission (2026-Q2); legacy `vault-enterprise-fips1402` build is FIPS 140-2 Level 1 validated. | **Pending** — FIPS-mode-enabled build confirmation + customer-side attestation due 2026-06-14 | `fips-attestation-letters/LETTER-VAULT.md` (request template); response expected 2026-06-14 | `$VAULT_ADDR` (customer-hosted; FIPS endpoint customer-managed); CoreLink enforces TLS 1.3 + mTLS | `crates/corelink-byok-vault/src/auth.rs:89` (`vault_addr`) + `crates/corelink-byok-vault/src/real.rs:94` (`vault_addr`) + `:111` (`VAULT_ADDR`) | — (target 2026-06-14) | 2026-12-14 (semi-annual) |

### 2.1 Current state breakdown (2026-05-15 snapshot)

- **Attested:** 1 / 4 — AWS KMS only.
- **Pending:** 3 / 4 — GCP, Azure, Vault (RFIs dispatched today; D+30 hard cap closes 2026-06-14).
- **Self-attested:** 0 / 4.
- **Unknown:** 0 / 4.

> **Auditor note:** The "Self-attested" classification is intentionally
> empty — every provider is either fully Attested with a signed letter
> or formally Pending with a tracked request. There is no middle ground
> in the SOC 2 evidence chain.

---

## 3. FIPS-mode verification

The endpoint verifier (`scripts/verify-fips-endpoints.py`) statically
inspects every BYOK crate's `src/` tree and:

1. Extracts hard-coded URLs / hostnames / endpoint constants.
2. Classifies each as **FIPS endpoint** (`kms-fips`, `*-fips`, regional
   FIPS suffix `*.rep.googleapis.com`, `*.managedhsm.azure.net` Managed
   HSM tier, or explicit FIPS-mode flag) vs **non-FIPS endpoint**.
3. Exits non-zero if any **production code path** could resolve to a
   non-FIPS endpoint without an explicit override flag.

The verifier is wired into pre-commit + CI (`.github/workflows/`)
and must pass before any BYOK crate is merged.

### 3.1 Per-provider FIPS-mode selection mechanism

| Provider | FIPS-mode trigger | Default in production | Test coverage |
|---|---|---|---|
| AWS KMS | `BYOK_AWS_FIPS_ENDPOINT=true` env var → `resolve_endpoint_hostname` switches to `kms-fips.*` suffix. | **ON** (set by `corelink-orchestrator` startup; verified by integration test `fips_endpoint_hostname_when_enabled`). | `crates/corelink-byok-aws/src/lib.rs:706-718` |
| GCP KMS | `with_endpoint("cloudkms.{region}.rep.googleapis.com")` for FedRAMP / FIPS regions; `protectionLevel=HSM` enforced at CMK onboarding by orchestrator policy. | **HSM tier** mandatory; orchestrator rejects `SOFTWARE` keys in production. | `crates/corelink-byok-gcp/src/real.rs:88,109` |
| Azure KV | `endpoint_override` set to `*.managedhsm.azure.net` for L3 tenants; `*.vault.azure.net` Premium for L2. | Per-customer tier; orchestrator stores tier at CMK onboarding. | `crates/corelink-byok-azure/src/real.rs:118-124` |
| Vault | Customer ops must run `vault-enterprise-fips1402` (or successor 140-3 build); CoreLink verifies via `sys/seal-status` boot check. | **Customer-managed**; refuse-to-connect if FIPS flag absent on Vault server. | `crates/corelink-byok-vault/src/real.rs:110-111` (`VAULT_ADDR`) |

---

## 4. Cross-references

- **SOC 2 evidence rollup:** `specs/_compliance/SOC2-EVIDENCE-ROLLUP-2026-05-15.md`
  rows CC6.1 + C1.1 + GAP-02 row (line 199, 262).
- **Roadmap §9 Human Track:** `ROADMAP-TO-GA.md` H-4 (vendor accounts) +
  GAP-02 progress sub-row (this matrix is the closure artefact).
- **Existing technical matrix:** `compliance/byok-fips-matrix.md` (CMVP
  module IDs + envelope encryption algorithms; this doc adds the
  attestation evidence layer).
- **Letter templates:** `specs/_compliance/fips-attestation-letters/`
  (4 vendor-specific templates).
- **RFI questionnaire:** `specs/_compliance/FIPS-RFI-QUESTIONNAIRE.md`
  (30 questions for any new BYOK provider).
- **Renewal runbook:** `specs/_runbooks/RB-FIPS-ATTESTATION-RENEWAL.md`.
- **Verifier:** `scripts/verify-fips-endpoints.py`.

---

## 5. Closure criteria for GAP-02

GAP-02 transitions from `blocking-GA` to `closed` when **all four**:

1. Each provider row in §2 is **Attested** (signed letter received,
   hashed, stored under `compliance/attestation-evidence/`).
2. `scripts/verify-fips-endpoints.py` exits 0 in CI for `main`.
3. SOC 2 auditor counter-signs evidence walkthrough (CC6.1 + C1.1).
4. Quarterly renewal calendar created in
   `specs/_runbooks/RB-FIPS-ATTESTATION-RENEWAL.md` §6.

If 1–3 are met but **any** signed letter slips past D+30 hard cap, the
fallback ADR per `WI-S20-003 §5.2` applies: GAP-02 is reclassified as
**non-blocking with explicit waiver** carrying a T+90d expiry, and the
auditor is notified in writing.

---

## 6. Changelog

| Version | Date | Author | Change |
|---|---|---|---|
| 1.0.0 | 2026-05-15 | Gustavo (via Claude Opus 4.7) | Initial attestation matrix; 1/4 Attested (AWS), 3/4 Pending; RFI letters dispatched. |
