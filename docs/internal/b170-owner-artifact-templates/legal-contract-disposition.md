# B-170 Legal contract disposition — unsigned template

Use this form for Legal's disposition of the executed DPA/SLA claims and the
pending residency amendment. Keep executed contract bytes outside this repository;
record a redacted authority reference or digest instead.

```yaml
schema_version: 1
finding: B-170
issue: 1677
artifact_kind: legal_contract_disposition
status: pending
decision_owner: null                 # Legal owner; name only after assignment
decision_timestamp: null             # ISO-8601, after the decision
executed_dpa_reference: null          # redacted authority reference or digest
executed_dpa_version_claimed: null   # external copy identifier, if verified
executed_sla_reference: null          # redacted authority reference or digest
executed_sla_version_claimed: null   # external copy identifier, if verified
residency_template_reference: legal/dpa-residency-amendment.md
residency_template_status_observed: PENDING_LEGAL_REVIEW
disposition: null                    # fill only after Legal review
affected_customer_scope: null        # reference or redacted count; no names
required_follow_up: null
signed_artifact_reference: null      # required before claiming a signed decision
signature_status: pending
```

## Repository facts to reconcile

- The owner packet identifies historical DPA Object Lock and SLA/BYOK language as
  requiring review against the executed instruments.
- The owner packet records the `docs/customer/dpa-onboarding.md` statement that
  CoreLink has executed a DPA with a lighthouse enterprise customer; Legal/owner
  must reconcile that repository claim against an authoritative executed-copy
  reference. The page itself does not establish a customer identity, signature,
  or receipt.
- `legal/dpa/v1.0.0.en-US.md` and `legal/sla/v1.0.0.md` are repository versions;
  their presence does not identify an executed customer copy.
- `legal/dpa-residency-amendment.md` is a template marked pending review, and the
  two dated v1.0.1 files are drafts marked not effective.

## Completion evidence

- [ ] Legal owner and decision timestamp recorded.
- [ ] Each executed instrument reviewed through a redacted authority reference.
- [ ] Disposition and affected scope recorded, or explicitly marked unresolved.
- [ ] Signed artifact reference or digest attached outside this repository when a
      signed disposition is claimed.

This template contains no Legal approval, contract execution, amendment, customer
scope, signature, or effective date.
