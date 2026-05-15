---
id: "LETTER-AWS-KMS"
type: "vendor_letter_template"
doc_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
owner: "Gustavo Schneiter"
vendor: "Amazon Web Services, Inc."
provider: "AWS KMS"
target_sla_days: 30
gap: "GAP-02"
tags: ["byok", "fips", "letter-template", "aws-kms", "gap-02"]
---

# Attestation request — AWS KMS FIPS 140-3 validation

> **Template.** Replace `{{...}}` placeholders. Send on
> HuGR Labs letterhead via the AWS Enterprise Support case channel
> ("Account & Billing" → "Compliance" → "FIPS"), with a parallel email
> to your AWS Account Manager + Solutions Architect. Track in
> `specs/_compliance/BYOK-FIPS-ATTESTATION-MATRIX.md` §2 row "AWS KMS".

---

**HuGR Labs / CoreLink**
gustavo@humangr.com
{{date_sent: 2026-05-15}}

**To:** AWS Compliance Team — c/o `{{aws_account_manager_email}}`
**Re:** Request for FIPS 140-3 attestation letter — AWS KMS — CoreLink BYOK
**SLA:** 30 calendar days (response requested by **{{date_sent + 30d: 2026-06-14}}**)

Dear AWS Compliance Team,

HuGR Labs operates **CoreLink** (https://corelink.humangr.com), a
shared content-addressable cache for build artefacts, ML model
weights, container layers, and package registries. We offer a
Bring-Your-Own-Key (BYOK) enterprise tier in which customer-controlled
AWS KMS Customer Master Keys (CMKs) wrap our per-blob data encryption
keys (DEKs) via the envelope-encryption pattern (AES-256-GCM on the
body; AES-256 key-wrap for the DEK; encryption context `{tenant_id,
blob_hash}` bound as AAD).

CoreLink is preparing for **SOC 2 Type I attestation (fieldwork
2026-Q3)** and must produce signed FIPS validation evidence per
sub-processor / KMS provider. AWS KMS is our default BYOK back-end and
our integration uses the explicit FIPS endpoints
`kms-fips.{region}.amazonaws.com` (commercial) and
`kms-fips.{region}.amazonaws.com` (GovCloud), selected via the
`BYOK_AWS_FIPS_ENDPOINT` flag enforced ON in production
(`crates/corelink-byok-aws/src/lib.rs`).

We respectfully request that AWS provide, on AWS letterhead and
counter-signed by an authorised AWS Compliance officer, an
**attestation letter** covering the following items:

1. Confirmation that **AWS KMS** is validated under
   **FIPS 140-3 Level 1** with NIST CMVP certificate **#4523** (or its
   most recent successor — please cite the exact certificate ID
   active as of the date of this letter).
2. Module name (e.g. "AWS Key Management Service HSM"), validation
   date, sunset date, and revalidation roadmap.
3. The **list of FIPS endpoints** customers are expected to use to
   guarantee that `Encrypt` / `Decrypt` / `GenerateDataKey` operations
   resolve to a FIPS-validated cryptographic module, including the
   commercial pattern `kms-fips.{region}.amazonaws.com` and the
   GovCloud pattern.
4. Confirmation that AWS KMS in the regions we use
   (`{{regions: us-east-1, us-west-2, eu-west-1, sa-east-1}}`)
   processes all CMK operations on FIPS-validated HSMs.
5. The supported cryptographic algorithms relevant to envelope
   encryption (AES-256, AES key-wrap per SP 800-38F, HMAC-SHA-256,
   RSA-2048, EC P-256) and confirmation each is FIPS-approved in the
   module.
6. A statement that AWS will notify HuGR Labs in writing within
   **30 days** of any change to the CMVP status of AWS KMS that
   would degrade FIPS validation (suspension, sunset, revocation).
7. The AWS-recommended renewal cadence for this attestation letter
   (we propose **semi-annual**; please confirm).

We are happy to receive the letter via AWS Artifact, the AWS
Compliance Reports Portal, or as a counter-signed PDF emailed to
`gustavo@humangr.com` and `security@humangr.com`. Please respond by
**{{date_sent + 30d: 2026-06-14}}** so that we can stay on schedule
for SOC 2 fieldwork.

If you require additional context — for example, our AWS account ID
(`{{aws_account_id}}`), the IAM policy used by the CoreLink
orchestrator on the customer-side roles, our SOC 2 audit firm
contact, or a copy of the CoreLink BYOK whitepaper — please reply to
this thread and we will provide them under NDA.

Thank you for your support. We look forward to your reply.

Sincerely,

**Gustavo Schneiter**
Founder / Architect, HuGR Labs
gustavo@humangr.com

---

## Internal tracking

| Field | Value |
|---|---|
| Sent date | `{{date_sent}}` |
| Channel | AWS Enterprise Support case `#{{case_id}}` + email to `{{aws_account_manager_email}}` |
| Owner | Gustavo Schneiter |
| Expected response | `{{date_sent + 30d}}` |
| Escalation if SLA breach | AWS Enterprise Support TAM → AWS Account Executive → AWS Compliance Director (per `RB-FIPS-ATTESTATION-RENEWAL.md` §5.2) |
| Storage on receipt | `compliance/attestation-evidence/AWS-KMS-FIPS-{{yyyy-mm}}.pdf` (SHA-256 logged in matrix §2) |
