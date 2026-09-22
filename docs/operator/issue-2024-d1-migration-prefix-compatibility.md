# D1 migration-prefix compatibility decision for issue #2024

**Decision date:** 2026-09-22
**Scope:** the `0118`, `0131`, and `0137` filename collisions on `main`.

Wrangler records a migration by its complete filename in each binding's
`d1_migrations` ledger. A filename already in that ledger must remain in the
repository: renaming it would make the replacement appear pending and could
replay its schema changes.

## Provider evidence

A read-only query against production `CONFIG_DB` (`d64742ea-e102-40b2-a844-ff02e3f94562`) found 129 applied
migrations, ending at `0130_credential_generation_event_receipts.sql`.
The query recorded `0118_byok_transition_fence.sql` as ledger entry 117,
applied at `2026-09-21 16:30:34`. None of the files originally numbered
`0131` through `0139`, nor `0118_gc_accounting_legal_hold.sql`, was present.
The query wrote zero rows.

Repository first-parent history confirms that the retained BYOK migration
entered `main` on 2026-09-12. Every renamed file is absent from the production
ledger, so its filename can change without replaying applied history.

## Resolution

| Previous filename | Ledger status | Resolved filename | Reason |
| --- | --- | --- | --- |
| `0118_byok_transition_fence.sql` | applied (id 117) | unchanged | Preserve recorded Wrangler history. |
| `0118_gc_accounting_legal_hold.sql` | absent | `0142_gc_accounting_legal_hold.sql` | Fresh ordinal after the ordered unapplied sequence. |
| `0131_terraform_drift_summary_artifact.sql` | absent | unchanged | Must precede the Terraform table rebuild that copies this column. |
| `0131_stripe_webhook_inbox.sql` | absent | `0132_stripe_webhook_inbox.sql` | Leaves `0131` to the Terraform dependency and remains before its effect ledger. |
| `0137_dsr_dlq_delivery_receipts.sql` | absent | `0138_dsr_dlq_delivery_receipts.sql` | Preserves the relative order of the unapplied sequence. |
| `0137_runner_entitlement_reconcile_fence.sql` | absent | `0139_runner_entitlement_reconcile_fence.sql` | Must precede the authority migration that alters this table. |

The unaffected but unapplied `0132`–`0136`, `0138`, and `0139` files move one
ordinal later so their schema order remains intact: Stripe effects `0133`,
usage conflicts `0134`, runner aggregate `0135`, storage liability `0136`,
runner checkout `0137`, runner authority `0140`, and Terraform region contract
`0141`. SQL bodies are unchanged apart from ordinal comments and filename
references. No migration was deleted, no applied filename was renamed, and the
unique-prefix and additive-only gates remain mandatory. The D1 validation
workflow runs these gates, schema parsing, and the in-memory SQLite replay on
`ubuntu-24.04` with no Cloudflare credentials.
