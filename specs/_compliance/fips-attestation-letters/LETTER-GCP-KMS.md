---
id: "LETTER-GCP-KMS"
type: "vendor_letter_template"
doc_status: "ACTIVE"
version: "1.1.0"
created: "2026-05-15"
updated: "2026-05-27"
owner: "Gustavo Schneiter"
vendor: "Google LLC"
provider: "GCP Cloud KMS"
target_sla_days: 30
gap: "GAP-02"
tags: ["byok", "fips", "letter-template", "gcp-kms", "gap-02"]
---

> **Post Wave 35 Phase 2 update 2026-05-27:** `corelink-byok-gcp` was absorbed into `corelink-byok` via inline `mod <name>;` per SEAL specs/_audits/sealed/2026-05-26-w35-p2-byok-absorption.md. Canonical consumer path is now `corelink_byok::*`.

# Attestation request — GCP Cloud KMS FIPS 140-2 Level 3 (HSM tier)

> **Template.** Replace `{{...}}` placeholders. Send via the Google
> Cloud Compliance Reports Manager request form + parallel email to
> the GCP Account Manager + Customer Engineer. Track in
> `specs/_compliance/BYOK-FIPS-ATTESTATION-MATRIX.md` §2 row "GCP Cloud KMS".

---

**HuGR Labs / CoreLink**
gustavo@humangr.com
{{date_sent: 2026-05-15}}

**To:** Google Cloud Compliance — c/o `{{gcp_account_manager_email}}`
**Re:** Request for FIPS 140-2 Level 3 (HSM tier) attestation — GCP Cloud KMS — CoreLink BYOK
**SLA:** 30 calendar days (response requested by **{{date_sent + 30d: 2026-06-14}}**)

Dear Google Cloud Compliance Team,

HuGR Labs operates **CoreLink** (https://corelink.humangr.com), a
shared content-addressable cache for build artefacts, ML model
weights, container layers, and package registries. CoreLink exposes a
Bring-Your-Own-Key (BYOK) enterprise tier integrated with
**Google Cloud KMS** (`crates/corelink-byok-gcp/src/real.rs`).
For production tenants we enforce `protectionLevel = HSM` at CMK
onboarding; the software tier (`SOFTWARE`) is permitted only in
staging.

CoreLink is preparing for **SOC 2 Type I attestation (fieldwork
2026-Q3)** and must produce signed FIPS validation evidence per
sub-processor / KMS provider.

We respectfully request that Google provide, on Google Cloud
letterhead and counter-signed by an authorised Google Cloud
Compliance officer, an **attestation letter** covering the following
items:

1. Confirmation that **GCP Cloud KMS HSM tier** is validated under
   **FIPS 140-2 Level 3** with NIST CMVP certificate **#3318**
   (Marvell LiquidSecurity HSM) — or its most recent successor.
2. Confirmation that the GCP Cloud KMS software tier is validated
   under **FIPS 140-2 Level 1** with NIST CMVP certificate **#3978**.
3. Module names, validation dates, sunset dates, and the FIPS 140-3
   revalidation roadmap (especially given the 2026-09-22 FIPS 140-2
   sunset).
4. The **endpoint hostnames** customers are expected to use to
   guarantee that `Encrypt` / `Decrypt` / `GenerateDataKey`
   operations resolve to the HSM-tier FIPS-validated module —
   including the standard `cloudkms.googleapis.com` and the regional
   FIPS variants (`cloudkms.{region}.rep.googleapis.com`).
5. Confirmation that in the GCP regions we use
   (`{{regions: us-east1, us-central1, europe-west1, southamerica-east1}}`)
   keys created with `protectionLevel = HSM` are processed exclusively
   on FIPS 140-2 Level 3 HSMs.
6. The supported cryptographic algorithms relevant to envelope
   encryption (AES-256, AES key-wrap, HMAC-SHA-256, RSA-2048,
   EC P-256, EC P-384) and confirmation each is FIPS-approved in the
   HSM module.
7. Audit-log access guarantees: Cloud Audit Logs retention,
   immutability, and customer access controls for KMS `Decrypt`
   events (required for CoreLink customer-facing forensic reports).
8. A statement that Google will notify HuGR Labs in writing within
   **30 days** of any change to the CMVP status of GCP Cloud KMS that
   would degrade FIPS validation.
9. The Google-recommended renewal cadence for this attestation letter
   (we propose **semi-annual**; please confirm).

We are happy to receive the letter via the Google Cloud Compliance
Reports Manager, an NDA-protected portal, or as a counter-signed PDF
emailed to `gustavo@humangr.com` and `security@humangr.com`. Please
respond by **{{date_sent + 30d: 2026-06-14}}** so that we can stay on
schedule for SOC 2 fieldwork.

If you require additional context — our GCP organisation ID
(`{{gcp_org_id}}`), the IAM bindings used by the CoreLink
orchestrator on the customer-side service accounts, our SOC 2 audit
firm contact, or a copy of the CoreLink BYOK whitepaper — please
reply to this thread and we will provide them under NDA.

Thank you for your support.

Sincerely,

**Gustavo Schneiter**
Founder / Architect, HuGR Labs
gustavo@humangr.com

---

## Internal tracking

| Field | Value |
|---|---|
| Sent date | `{{date_sent}}` |
| Channel | Google Cloud Compliance Reports Manager request `#{{request_id}}` + email to `{{gcp_account_manager_email}}` |
| Owner | Gustavo Schneiter |
| Expected response | `{{date_sent + 30d}}` |
| Escalation if SLA breach | GCP Customer Engineer → GCP Account Executive → GCP Compliance Director (per `RB-FIPS-ATTESTATION-RENEWAL.md` §5.2) |
| Storage on receipt | `compliance/attestation-evidence/GCP-KMS-FIPS-{{yyyy-mm}}.pdf` (SHA-256 logged in matrix §2) |
