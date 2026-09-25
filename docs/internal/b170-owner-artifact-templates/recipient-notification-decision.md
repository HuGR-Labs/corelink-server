# B-170 Sales/Legal recipient notification decision — unsigned template

Use this form to record Sales/Legal's decision about recipients of superseded
questionnaire copies. Do not put recipient names or email addresses in the
repository. A notification claim requires a redacted source record and receipt
reference supplied by the responsible owner.

```yaml
schema_version: 1
finding: B-170
issue: 1677
artifact_kind: recipient_notification_decision
status: pending
decision_owner: null                 # Sales/Legal owner; name after assignment
decision_timestamp: null             # ISO-8601, after the decision
source_population_reference: null    # CRM/mail export reference, redacted
superseded_copy_versions: []         # verified version identifiers only
recipient_scope: null                # redacted count/segment or external reference
decision: null                       # notify | no_action_required | unresolved
decision_basis_reference: null
notification_channel: null           # required only when notify is selected
notification_sent_at: null           # required only for a sent-notice claim
notification_evidence_reference: null
receipt_evidence_reference: null     # required when delivery/receipt is claimed
signed_or_approved_record: null
```

## Repository facts to reconcile

- The current CAIQ file is a bounded `1.0.0` draft answer bank and SIG-LITE is a
  bounded `1.1.0` draft answer bank. Their repository versions and null
  supersession fields do not establish which copies circulated, whether a copy
  was superseded after delivery, or who received one; the repository has no
  recipient or supersession ledger.
- The owner packet says Sales/Legal must identify any recipient population from
  CRM or mail records and decide whether notice is required.
- No recipient is identified here, and no notification, delivery, or receipt is
  asserted.

## Completion evidence

- [ ] Source population and superseded versions are identified through a redacted
      external reference.
- [ ] Sales/Legal owner, timestamp, decision, and basis are recorded.
- [ ] If `notify`, channel, sent timestamp, and redacted evidence are attached.
- [ ] Delivery or receipt is claimed only with a separate receipt reference.
- [ ] Approved or signed record reference is attached when required by the owner.

This template does not choose a decision, identify recipients, send a notice, or
prove delivery or receipt.
