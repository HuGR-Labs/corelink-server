---
id: "LETTER-AZURE-KV"
type: "vendor_letter_template"
doc_status: "ACTIVE"
version: "1.1.0"
created: "2026-05-15"
updated: "2026-05-27"
owner: "Gustavo Schneiter"
vendor: "Microsoft Corporation"
provider: "Azure Key Vault Premium + Managed HSM"
target_sla_days: 30
gap: "GAP-02"
tags: ["byok", "fips", "letter-template", "azure-kv", "gap-02"]
---

> **Post Wave 35 Phase 2 update 2026-05-27:** `corelink-byok-azure` was absorbed into `corelink-byok` via inline `mod <name>;` per SEAL specs/_audits/sealed/2026-05-26-w35-p2-byok-absorption.md. Canonical consumer path is now `corelink_byok::*`.

# Attestation request — Azure Key Vault Premium + Managed HSM FIPS attestation

> **Template.** Replace `{{...}}` placeholders. Send via the Microsoft
> Service Trust Portal compliance request form + parallel email to
> the Microsoft Account Manager + Cloud Solution Architect. Track in
> `specs/_compliance/BYOK-FIPS-ATTESTATION-MATRIX.md` §2 row
> "Azure Key Vault".

---

**HuGR Labs / CoreLink**
gustavo@humangr.com
{{date_sent: 2026-05-15}}

**To:** Microsoft Azure Compliance — c/o `{{azure_account_manager_email}}`
**Re:** Request for FIPS 140-2 Level 2 / Level 3 attestation letter — Azure Key Vault Premium + Managed HSM — CoreLink BYOK
**SLA:** 30 calendar days (response requested by **{{date_sent + 30d: 2026-06-14}}**)

Dear Microsoft Azure Compliance Team,

HuGR Labs operates **CoreLink** (https://corelink.humangr.com), a
shared content-addressable cache for build artefacts, ML model
weights, container layers, and package registries. CoreLink exposes a
Bring-Your-Own-Key (BYOK) enterprise tier integrated with both
**Azure Key Vault Premium** (HSM-backed keys, FIPS 140-2 Level 2) and
**Azure Managed HSM / Dedicated HSM** (FIPS 140-2 Level 3,
selected by the `*.managedhsm.azure.net` host suffix in
`crates/corelink-byok-azure/src/real.rs`). Both tiers are
supported in production; selection is per-tenant at CMK onboarding.

CoreLink is preparing for **SOC 2 Type I attestation (fieldwork
2026-Q3)** and must produce signed FIPS validation evidence per
sub-processor / KMS provider.

We respectfully request that Microsoft provide, on Microsoft
letterhead and counter-signed by an authorised Azure Compliance
officer, an **attestation letter** covering the following items:

1. Confirmation that **Azure Key Vault Premium tier HSM-backed
   keys** are validated under **FIPS 140-2 Level 2** with NIST CMVP
   certificate **#3516** (Azure Premium HSM) — or its most recent
   successor.
2. Confirmation that **Azure Managed HSM / Azure Dedicated HSM** is
   validated under **FIPS 140-2 Level 3** with NIST CMVP certificate
   **#4332** (Marvell LiquidSecurity, Azure-deployed) — or its most
   recent successor.
3. Module names, validation dates, sunset dates, and the FIPS 140-3
   revalidation roadmap (especially given the 2026-09-22 FIPS 140-2
   sunset).
4. The **endpoint hostnames** customers are expected to use to
   guarantee that `wrapKey` / `unwrapKey` / `encrypt` / `decrypt`
   operations resolve to a FIPS-validated cryptographic module:
   - Premium HSM tier: `{vault}.vault.azure.net` (L2).
   - Managed HSM tier: `{vault}.managedhsm.azure.net` (L3).
5. Confirmation that in the Azure regions we use
   (`{{regions: eastus, westus2, westeurope, brazilsouth}}`) Premium
   HSM and Managed HSM operations are processed exclusively on
   FIPS-validated HSMs.
6. The supported cryptographic algorithms relevant to envelope
   encryption — specifically **RSA-OAEP-256** (used by CoreLink for
   `wrapKey`) and the inner AES-256-GCM layer with AAD binding
   (per ADR-S14-001 in our spec corpus).
7. Audit-log access guarantees: Azure Monitor / Log Analytics
   retention, immutability, and customer access controls for Key
   Vault audit events (required for CoreLink customer-facing forensic
   reports).
8. A statement that Microsoft will notify HuGR Labs in writing within
   **30 days** of any change to the CMVP status of Azure Key Vault or
   Managed HSM that would degrade FIPS validation.
9. The Microsoft-recommended renewal cadence for this attestation
   letter (we propose **semi-annual**; please confirm).

We are happy to receive the letter via the Microsoft Service Trust
Portal, an NDA-protected portal, or as a counter-signed PDF emailed
to `gustavo@humangr.com` and `security@humangr.com`. Please respond
by **{{date_sent + 30d: 2026-06-14}}** so that we can stay on
schedule for SOC 2 fieldwork.

If you require additional context — our Azure tenant ID
(`{{azure_tenant_id}}`), the role assignments used by the CoreLink
orchestrator on the customer-side service principals, our SOC 2
audit firm contact, or a copy of the CoreLink BYOK whitepaper —
please reply to this thread and we will provide them under NDA.

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
| Channel | Microsoft Service Trust Portal request `#{{request_id}}` + email to `{{azure_account_manager_email}}` |
| Owner | Gustavo Schneiter |
| Expected response | `{{date_sent + 30d}}` |
| Escalation if SLA breach | Microsoft CSA → Microsoft Account Executive → Microsoft Azure Compliance Director (per `RB-FIPS-ATTESTATION-RENEWAL.md` §5.2) |
| Storage on receipt | `compliance/attestation-evidence/Azure-KV-FIPS-{{yyyy-mm}}.pdf` (SHA-256 logged in matrix §2) |
