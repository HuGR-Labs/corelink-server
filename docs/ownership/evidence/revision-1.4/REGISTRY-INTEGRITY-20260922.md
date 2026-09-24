# Registry integrity inventory — 2026-09-22

This is a read-only classification of the 37 registry records whose
cross-artifact metadata is not reconciled. It does not alter artifact bytes or
promote any review.

## Aggregate

| Failure class | Count | Meaning |
|---|---:|---|
| Skill metadata missing | 53 fields | Skill lacks `manifest`, `source-commit` and/or `evidence-set`; the reference remains the metadata authority. |
| Skill metadata differs | 27 fields | Skill source/evidence values differ from its reference. |
| Blast metadata differs | 25 fields | Blast source/evidence values differ from its reference. |
| Maintenance metadata differs | 27 fields | Maintenance source/evidence values differ from its reference. |
| Affected package records | 37 | A package may have more than one class above. |

## Affected packages

The exact package-to-field classification is generated from the current
`registry.json` errors:

```text
corelink-ac: skill/missing
corelink-adapter-host: skill/missing
corelink-adapters-vault: skill/differs, blast_radius/differs, maintenance/differs
corelink-auth: skill/missing, maintenance/differs
corelink-billing-stripe-materializer: maintenance/differs
corelink-byok: skill/differs, blast_radius/differs, maintenance/differs
corelink-cas: skill/missing
corelink-clerk: skill/missing/differs, blast_radius/differs
corelink-clerk-cf: maintenance/differs
corelink-client-verify: skill/missing
corelink-config-do: skill/differs, blast_radius/differs, maintenance/differs
corelink-crypto: skill/differs, blast_radius/differs, maintenance/differs
corelink-dpa-acceptance: skill/differs, blast_radius/differs, maintenance/differs
corelink-dsr: skill/differs, blast_radius/differs
corelink-dt-webhook: skill/missing
corelink-enterprise-inquiry: skill/missing
corelink-erasure-attestation: skill/differs
corelink-handler-ac: skill/missing
corelink-handler-admin: skill/differs, blast_radius/differs, maintenance/differs
corelink-handler-cas: skill/differs, blast_radius/differs, maintenance/differs
corelink-handler-cas-erase: blast_radius/differs
corelink-handler-customer: skill/differs, maintenance/differs
corelink-pat: skill/differs, maintenance/differs
corelink-rate-headers: skill/missing
corelink-region: skill/missing
corelink-runner-overage: blast_radius/differs, maintenance/differs
corelink-slack-real: skill/missing
corelink-slo: skill/missing
corelink-statuspage-real: skill/missing
corelink-stripe-real: skill/missing
corelink-tenant-path: skill/missing
corelink-tier-selection: skill/missing
corelink-turbo-bridge: skill/differs, blast_radius/differs
corelink-wasm: skill/differs
e2e-chaos: skill/missing, blast_radius/differs, maintenance/differs
e2e-dsr: skill/missing
e2e-failover-router: skill/missing
```

## Safe remediation rule

Do not bulk-rewrite these fields without checking the package's immutable
source pin and any prior cold-review hash. Adding or changing metadata changes
artifact bytes and invalidates approval for those bytes. Remediation therefore
requires: choose the reference-authoritative values, patch the affected
artifacts, rerun structural checks, obtain fresh independent cold review, and
regenerate the registry.
