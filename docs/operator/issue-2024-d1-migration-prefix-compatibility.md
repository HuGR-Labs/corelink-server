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
applied at `2026-09-21 16:30:34`. None of the other five colliding filenames
was present. The query wrote zero rows.

Repository first-parent history confirms that the retained BYOK migration
entered `main` on 2026-09-12. The five moved files entered after it and remain
unapplied according to the production ledger.

## Resolution

| Previous filename | Ledger status | Resolved filename | Reason |
| --- | --- | --- | --- |
| `0118_byok_transition_fence.sql` | applied (id 117) | unchanged | Preserve recorded Wrangler history. |
| `0118_gc_accounting_legal_hold.sql` | absent | `0141_gc_accounting_legal_hold.sql` | Fresh ordinal; dependencies predate `0141`. |
| `0131_stripe_webhook_inbox.sql` | absent | unchanged | Earlier file in the collision and its dependent source retains its canonical filename. |
| `0131_terraform_drift_summary_artifact.sql` | absent | `0140_terraform_drift_summary_artifact.sql` | First fresh ordinal; `terraform_drift_findings` was created by `0025`. |
| `0137_dsr_dlq_delivery_receipts.sql` | absent | unchanged | Earlier file in the collision. |
| `0137_runner_entitlement_reconcile_fence.sql` | absent | `0143_runner_entitlement_reconcile_fence.sql` | Fresh ordinal after the moved dependency-independent migrations. |

The SQL bodies are unchanged apart from ordinal comments. No migration was
deleted, no applied filename was renamed, and the unique-prefix and
additive-only gates remain mandatory. The D1 validation workflow runs these
gates, schema parsing, and the in-memory SQLite replay on `ubuntu-24.04` with
no Cloudflare credentials.
