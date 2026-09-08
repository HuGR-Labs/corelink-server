# BYOK FIPS evidence

Status: not yet available; BYOK is not enabled in the launched CoreLink data
plane.

This document is the canonical DPA evidence-pack placeholder for the
customer-managed-key (BYOK) control. It deliberately makes no FIPS-validated
module or certification claim. A tenant must not be represented as having
FIPS-backed BYOK based on the presence of this file.

When BYOK is provisioned for a tenant, the compliance owner must replace this
status with the applicable provider's current certificate, module name and
validation number, scope and boundary, evidence date, and expiration/review
date. The evidence must identify the exact production provider and endpoint;
generic references to an SDK or to AES are insufficient.

Until that owner evidence exists, the DPA onboarding language remains the
truthful launch posture: storage uses provider-managed R2 encryption and BYOK
is planned, not an active FIPS control.
