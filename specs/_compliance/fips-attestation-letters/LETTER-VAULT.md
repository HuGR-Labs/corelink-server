---
id: "LETTER-VAULT"
type: "vendor_letter_template"
doc_status: "ACTIVE"
version: "1.1.0"
created: "2026-05-15"
updated: "2026-05-27"
owner: "Gustavo Schneiter"
vendor: "HashiCorp, Inc. (an IBM company)"
provider: "HashiCorp Vault Enterprise (FIPS build)"
target_sla_days: 30
gap: "GAP-02"
tags: ["byok", "fips", "letter-template", "vault", "gap-02"]
---

> **Post Wave 35 Phase 2 update 2026-05-27:** `corelink-byok-vault` was absorbed into `corelink-byok` via inline `mod <name>;` per SEAL specs/_audits/sealed/2026-05-26-w35-p2-byok-absorption.md. Canonical consumer path is now `corelink_byok::*`.

# Attestation request — HashiCorp Vault Enterprise FIPS build + Transit secrets engine

> **Template.** Replace `{{...}}` placeholders. Send via the
> HashiCorp Support portal (Enterprise customers) + parallel email
> to the HashiCorp Account Manager + Solutions Engineer. Track in
> `specs/_compliance/BYOK-FIPS-ATTESTATION-MATRIX.md` §2 row
> "HashiCorp Vault Enterprise".

---

**HuGR Labs / CoreLink**
gustavo@humangr.com
{{date_sent: 2026-05-15}}

**To:** HashiCorp Compliance — c/o `{{hashicorp_account_manager_email}}`
**Re:** Request for FIPS 140-3 attestation + FIPS-mode confirmation — Vault Enterprise FIPS build + Transit secrets engine — CoreLink BYOK
**SLA:** 30 calendar days (response requested by **{{date_sent + 30d: 2026-06-14}}**)

Dear HashiCorp Compliance Team,

HuGR Labs operates **CoreLink** (https://corelink.humangr.com), a
shared content-addressable cache for build artefacts, ML model
weights, container layers, and package registries. CoreLink exposes a
Bring-Your-Own-Key (BYOK) enterprise tier integrated with
**HashiCorp Vault Enterprise** via the Transit secrets engine
(DEK wrap/unwrap operations; mTLS authentication; auth methods
AppRole / Kubernetes / AWS IAM). Customers self-host Vault; CoreLink
connects to the customer-supplied `VAULT_ADDR`
(`crates/corelink-byok-vault/src/auth.rs:89` / `real.rs:111`).

CoreLink is preparing for **SOC 2 Type I attestation (fieldwork
2026-Q3)** and must produce signed FIPS validation evidence per
sub-processor / KMS provider. Because Vault is customer-hosted, our
controls must confirm both (a) the **FIPS validation of the Vault
build** itself and (b) the customer's deployment of the FIPS build
with FIPS mode enabled at the OS / boringcrypto layer.

We respectfully request that HashiCorp provide, on HashiCorp
letterhead and counter-signed by an authorised HashiCorp Compliance
officer, an **attestation letter** covering the following items:

1. Confirmation of the **current FIPS validation status** of
   Vault Enterprise FIPS-mode builds, naming the exact build artefact
   (e.g. `vault-enterprise-fips1402`, FIPS 140-2 Level 1 today;
   anticipated FIPS 140-3 Level 1 successor build) and the
   underlying CMVP certificate ID (NIST CMVP for the BoringCrypto /
   GoFIPS-validated module embedded in the build).
2. The **expected timeline** for the FIPS 140-3 certified Vault
   Enterprise build (we understand CMVP submission was targeted for
   2026-Q2 — please confirm or update).
3. The **operational guidance** customers must follow to deploy
   Vault Enterprise in a FIPS-validated configuration:
   - Required OS-level FIPS mode (RHEL / Ubuntu Pro FIPS / Amazon
     Linux FIPS).
   - Required Vault configuration flags (e.g. `disable_mlock`,
     storage backend constraints).
   - Required auto-unseal configuration (FIPS-validated KMS only).
   - How to verify FIPS mode is active at runtime (we currently
     query `sys/seal-status` and expect a documented FIPS-mode
     indicator).
4. The supported cryptographic algorithms in the Transit secrets
   engine relevant to envelope encryption (AES-256-GCM, AES-256-KW,
   HMAC-SHA-256, RSA-2048, EC P-256, EC P-384) and confirmation each
   is FIPS-approved in the build.
5. Audit-log access guarantees: Vault audit-device retention,
   immutability options (file / syslog / socket), and customer
   guidance for tamper-evident audit-log shipping.
6. A statement that HashiCorp will notify HuGR Labs in writing
   within **30 days** of any change to the CMVP status of the Vault
   Enterprise FIPS build that would degrade FIPS validation.
7. The HashiCorp-recommended renewal cadence for this attestation
   letter (we propose **semi-annual**; please confirm).
8. A **sub-processor list** for the FIPS build (any third-party
   cryptographic library suppliers — e.g. Go BoringCrypto module
   provider).

We are happy to receive the letter via the HashiCorp Support
portal, an NDA-protected channel, or as a counter-signed PDF
emailed to `gustavo@humangr.com` and `security@humangr.com`. Please
respond by **{{date_sent + 30d: 2026-06-14}}** so that we can stay on
schedule for SOC 2 fieldwork.

If you require additional context — our HashiCorp Enterprise
license ID (`{{hashicorp_license_id}}`), the auth method
configuration used by the CoreLink orchestrator on the customer-side
Vault, our SOC 2 audit firm contact, or a copy of the CoreLink BYOK
whitepaper — please reply to this thread and we will provide them
under NDA.

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
| Channel | HashiCorp Support portal ticket `#{{ticket_id}}` + email to `{{hashicorp_account_manager_email}}` |
| Owner | Gustavo Schneiter |
| Expected response | `{{date_sent + 30d}}` |
| Escalation if SLA breach | HashiCorp SE → HashiCorp Account Executive → HashiCorp Compliance Director (per `RB-FIPS-ATTESTATION-RENEWAL.md` §5.2) |
| Storage on receipt | `compliance/attestation-evidence/Vault-FIPS-{{yyyy-mm}}.pdf` (SHA-256 logged in matrix §2) |
