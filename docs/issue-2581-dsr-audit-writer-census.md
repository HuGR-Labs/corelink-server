# Issue 2581 DSR and audit writer census

This census covers persistent mutations in the exclusive DSR/audit writer
family. “Admission applicable” means the mutation can originate in a request
that has consumed the immutable DSR admission context from #2576. Ordinary
traffic and background jobs receive no synthetic ownership context.

| Writer boundary | Persistent mutation | Ownership class | Disposition | Admission applicability |
| --- | --- | --- | --- | --- |
| `routes/dsr/audit.rs::D1ErasureAuditSink::emit_attributed` | `audit_outbox` event | `audit_evidence` | retained | Erasure pipeline; registration shares the D1 insert batch. |
| `routes/dsr/ledger.rs::D1ErasureIdempotencyLedger::upsert_with_context` | `dsr_erasure_log` backend obligation | `dsr_obligation` | retained | Erasure pipeline; insertion/replay registration is derived from the verified context. |
| `routes/dsr/ledger.rs::set_outcome_snapshot_with_context` | `dsr_erasure_log.outcome_json` | `dsr_obligation` | retained | Erasure pipeline; outcome update and registrations share a D1 batch. |
| `routes/dsr/access.rs::audit_dsr_event` | `audit_outbox` access, portability, and rectification event | `audit_evidence` | retained | Internal DSR request; registration shares the D1 insert batch. |
| `routes/dsr/access.rs::persist_export` | `dsr_exports/{dsr_id}.json` and `.sig.json` R2 objects | `dsr_artifact` | disposable | Internal portability request; each object uses a D1 prepare → R2 PUT → D1 resource/commit transaction. |
| `routes/dsr/access.rs::run_rectification` | allowlisted `tenant.email_hash` update | `dsr_artifact` | disposable | Internal rectification request; update, changes result, and registration share one D1 batch. |
| `routes/dsr/attestation.rs::sign_and_persist` | signed attestation R2 object | `audit_evidence` | retained | Internal verify request; durable R2 intent/reconciliation precedes success. |
| `routes/dsr/attestation.rs::sign_and_persist` | public-key and attestation index rows | `audit_evidence` | retained | Internal verify request; each D1 mutation shares a batch with registration. |
| `routes/dsr/adapter_d1.rs` and `adapter_r2_{cas,ac}.rs` | tenant-scoped D1/R2 deletes | none | destructive erasure effects | These remove pre-existing subject data; they create no owned artifact. Per-backend results are retained as `dsr_obligation` rows and `audit_evidence`, the operation-level proof boundaries above. |
| `routes/dsr/adapter_r2_cas_legalhold.rs` | `cas_retention` retention record | none | lawful retention metadata | This preserves pre-existing customer content under an active legal hold; it is not a synthetic DSR artifact. |
| `routes/dsr/portal/part-00.rs` and `part-01.rs` | customer portal ticket/status transitions | none | ordinary durable workflow | Customer portal has no staging-admission gate and these writes are not initiated by an admitted synthetic request. |
| `apps/signup-worker/src/webhooks/dsr_verify_cron.ts` | background `dsr_requested` status update | none | ordinary background workflow | Cron has no request-scoped admission claim. |
| `apps/signup-worker/src/webhooks/dsr_consumer.ts` | webhook queue, redrive, receipt, claim, and retention transitions | none | ordinary webhook workflow | Queue/webhook operations have no DSR HTTP admission context; they remain outside this synthetic request path. |
| `storage/d1_audit_sink/**` | no production writer; test fixtures only | none | tests | The DSR production audit boundary is `routes/dsr/audit.rs`; generic/signup sinks are other writer families. |

The ownership rows are generated only from the immutable admitted context;
handles contain bounded DSR identifiers, backend/class labels, and object
keys. Receipt references are produced by the frozen #2623 contract and contain
only domain-separated SHA-256 digests. No request body, credential, nonce, or
personal data is copied into an ownership record.
