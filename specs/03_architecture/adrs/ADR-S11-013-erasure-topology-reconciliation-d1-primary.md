---
id: "ADR-S11-013"
type: "adr"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-06-11"
updated: "2026-06-11"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["adr", "s11", "privacy", "erasure", "dsr", "topology", "regulatory"]
---

# ADR-S11-013 — Erasure Backend Topology Reconciliation (canonical model → shipped D1-primary reality)

## Status

**Accepted — Gustavo Schneiter ratified the D1 erase-vs-retain classification
on 2026-06-11.** Surfaced during DSR Wave 1 cold-verification (2026-06-11); the
GDPR-critical erase-vs-retain policy below is owner-approved and Wave 1
increment 2 may wire the real deletes against it. (Engineering pre-work still
required: the exhaustive per-table `tenant_id`-column verification — that the
WHERE clause is correct for each erase-set table — is an implementation
checklist, not a policy question.)

## Context

The canonical erasure model (`privacy_model.md §6.2`, lines 313–327) and the
12 backend spec stubs in `crates/corelink-privacy-erasure-worker/src/backends/*.rs`
describe a **Neon-Postgres-primary** topology: control-plane, accounts,
consent ledger, DSR tickets, and billing detail all live in Neon; D1 holds only
operational metadata (`blob_meta` + `ac_meta`).

Cold-verification of the **shipped** system (2026-06-11) contradicts this:

| Claim in canonical model | Shipped reality (evidence) |
|---|---|
| Neon holds `dsr_tickets / account / tenant / user_account / consent_ledger / subscription` (`privacy_model.md §6.2.a`; `backends/neon_main.rs:6-9`) | Neon holds **only** `audit_events_shadow` + `audit_shadow_lag` — `migrations/neon/` has exactly 2 migrations (`0001_audit_events_shadow.sql`, `0002_…`). No control-plane tables exist in Neon. |
| Those account/consent tables exist | They are defined **only** in `migrations/002_auth_tables.sql` (`account`, `user_account`, `membership`, `consent_ledger`-class) — the **deferred auth-spine** (owner-authorized deferral; auth ships as **Clerk**). Not deployed in prod. |
| D1 erases only `blob_meta` + `ac_meta` (`privacy_model.md §6.2.e`; `backends/d1.rs:4-6`) | D1 (`corelink-config-prod`) holds **all** subject PII: `tenant` (email_hash, clerk_user_id, stripe_customer_id — migr. 0023/0037/0056), `tier_selections` (0039), `tenant_billing` (0055), `pilot_signups` (**plaintext email** — 0053), `pat` (0037/0054), `usage_counter`, `signup_orchestration`/`signup_attempts` (0037), + ~20 more tenant-keyed tables across migrations 0001–0063. |

**Root cause:** prod consolidated the control-plane + billing into **D1 + Clerk**;
the Neon-primary + auth-spine end-state in `privacy_model.md` was never shipped.

**Consequence if the spec is implemented literally:** `neon_main`/`neon_billing`
adapters target tables that do not exist (no-op), and the `d1` adapter erases 2
of ~25 PII-bearing tables — **leaving `tenant.email_hash`, `pilot_signups.email`
(plaintext), `clerk_user_id`, `pat` hashes, and Stripe linkage undeleted. That is
a GDPR Art. 17 / LGPD Art. 18 VI erasure-incompleteness violation** (and the
`INV-DATA-ERASURE-COMPLETE` CRITICAL invariant, `dsr_erasure_log` migration
0022:14).

## Decision

Reconcile the 12-backend canonical model to the shipped **D1-primary** topology.
The canonical 12-backend contract (`canonical_backend_kinds()`, the
`dsr_erasure_log.backend` CHECK enum) is **preserved unchanged** — the orchestrator
still validates exactly 12 adapters in canonical order. What changes is **what
each adapter's real transport does**, re-mapped to where data actually lives.

### Reconciled per-backend mapping (Wave 1 real transports)

| Canonical kind | Spec intent (Neon-primary) | **Reconciled real transport (D1-primary)** |
|---|---|---|
| `NeonMain` | hard-delete Neon account/tenant/consent rows | **NotApplicable** — auth-spine not shipped; control-plane identity lives in D1 (handled by `D1`). Returns `NotApplicable("control-plane consolidated into D1; see ADR-S11-013")`. Future-proof: becomes real if the auth-spine lands. |
| `NeonBilling` | retain Neon `invoice`/`usage_event` under fiscal hold | **NotApplicable** — billing mirror is in D1 (`stripe_*`, retained); see `Stripe` + the D1 retain-set. |
| `R2Cas` | refcount-aware blob delete | **REAL** — R2 `corelink-cas-prod` DeleteObject **only when `blob_meta.refcount → 0`**; decrement-only otherwise (cross-tenant break = CRITICAL, WI-S11-002 §28 R-003). Coordinates D1 `blob_meta`. |
| `R2Ac` | action-cache delete | **REAL** — R2 `corelink-ac-{5 regions}` per tenant prefix; D1 `ac_meta` rows. |
| `D1` | `blob_meta` + `ac_meta` only | **REAL + EXPANDED** — hard-delete the full **erase-set** below (the bulk of subject PII). This is the load-bearing change. |
| `Kv` | KV key-prefix delete | **NotApplicable / cache-evict** — KV namespaces (`METADATA_KV`, `CLERK_JWKS_KV`, `NEGATIVE_CACHE_KV`) hold **no durable PII** (auth-token + blob-digest caches, TTL'd). Best-effort evict tenant-prefixed keys; `NotApplicable` for PII purposes. |
| `Stripe` | `Customer.update` PII-nullify | **REAL** — `Customer.update` (email→`pseudo@redacted`, name→`erased_<hash>`, address→null, metadata pii_redacted). **NEVER `Customer.delete`** (GAAP/LGPD Art.16 fiscal). Requires adding `update_customer` to `StripeRealClient` (gap — only `create/get` exist today). |
| `Loki` | `/loki/api/v1/delete` per subject | **REAL-or-NotApplicable** — pending confirmation a live Loki sink ingests tenant-tagged logs (sub-processor list names Grafana Labs). If logging is stdout/Workers-only → `NotApplicable`. **Owner to confirm.** |
| `R2AuditPseudo` | audit Object-Lock 7y; pseudonymize subject | **REAL pseudonymize** — never hard-delete (WORM); HKDF `corelink/v1/audit-pseudonym`. |
| `NeonPitrPseudo` | Neon PITR 30d natural rotation | **NotApplicable / tombstone-replay** — Neon holds only audit shadow; PITR rotation applies to the shadow. D1 has its own backup lifecycle. |
| `R2CasLegalHoldPseudo` | retain under hold; pseudonymize index | **REAL pseudonymize** — governance-mode hold partition. |
| `R2EvidencePseudo` | retain `evidence-*` 7y; pseudonymize | **REAL pseudonymize** — compliance evidence store. |

### D1 erase-vs-retain classification (GDPR-CRITICAL — OWNER-RATIFIED 2026-06-11)

The `D1` adapter hard-deletes the **erase-set** and never touches the **retain-set**.
Misclassifying in either direction is a defect: under-delete = Art. 17 violation;
over-delete = breaks fiscal (LGPD Art. 16 I, 5y) / audit-immutability (7y WORM).

**ERASE-SET** (hard `DELETE … WHERE tenant_id = ?` — subject PII, no retention basis):
`tenant`, `tier_selections`, `tenant_billing`, `pilot_signups`, `pat`,
`usage_counter`, `signup_orchestration`, `signup_attempts` (keyed by `email_hash`
via the orchestration link), `tenant_storage_state`, `tenant_offboarding_state`,
`usage_event_staging`, `dpa_acceptance_pending`, `tenant_primary_region`,
`tenant_config_region`, `hot_blobs`, `quota_reservations`, `quota_cas_attempts`,
`quota_fsm_state`, `ratelimit_buckets`, `byok_envelope`, `byok_tenant_status`,
`adapter_cache_map`, `adapter_npm_meta`, `adapter_pip_index`, `adapter_oci_kv`.
(`blob_meta`/`ac_meta` are owned by `R2Cas`/`R2Ac`, refcount-aware.)

**RETAIN-SET** (legal/fiscal/audit basis — NEVER hard-delete; pseudonymize PII fields where present):
`dsr_erasure_log` (the erasure record itself; forensic 7y — migr. 0022:10),
`erasure_attestation`, `dpa_acceptances` (proof-of-contract), `export_audit_log`,
`stripe_customers`/`stripe_subscriptions`/`stripe_invoices`/`stripe_disputes`/`stripe_refunds`
(fiscal 5y; pseudonymize email in `payload_json`), `billing_replay_audit`,
`stripe_webhook_events_processed`, `stripe_webhook_dlq`, `stripe_idem_keys`,
`billing_reconciliation_drift`, `config_change_log`, `audit_outbox`.

**FLAGGED for explicit owner call** (defensible either way):
`abuse_scores` (security-retention vs subject behavioral PII),
the `stripe_*` mirror pseudonymization depth (which `payload_json` fields).

> ✅ **OWNER-RATIFIED 2026-06-11.** The erase-vs-retain *policy* is approved. The
> exhaustive table-by-table `tenant_id`-column verification across the 63 D1
> migrations remains an engineering checklist for increment 2 (confirm each
> erase-set table's key column + that no retain-set row is touched), but is no
> longer a policy/approval gate.

### Contract gaps discovered (must close in Wave 1)

1. **`subject_id_hash` threading.** `dsr_erasure_log.subject_id_hash` is `NOT NULL`
   (canonical `sha256(subject_id ‖ tenant_salt)`, court-order re-correlation), but
   `BackendCompletion` (the ledger payload, `event.rs:473`) carries no subject id.
   The real D1 ledger cannot populate the column from the trait surface today.
   **Resolution:** additively extend `BackendCompletion` with `subject_id_hash:
   [u8;32]`, computed in the orchestrator from the request via the canonical
   `pseudonymize` surface. **High blast-radius** (it is `Serialize`+`PartialEq`,
   drives replay/divergent semantics, touches orchestrator/report/verification_job
   + the canonical-consistency gate) → execute as its own verified increment.
2. **`StripeRealClient` lacks `update_customer`** — only `create_customer`/`get_customer`
   exist (`client.rs:547`). Add `update_customer` via the existing private `post_form`
   (`client.rs:452`) → `POST /v1/customers/{id}`.
3. **Async-over-sync bridge.** `BackendErasureAdapter::erase` is sync (`backends.rs:114`);
   `D1HttpClient::query` / `R2S3Client` are async. Use the in-repo battle-tested
   pattern `tokio::task::block_in_place(|| handle.block_on(fut))` (`storage/r2_s3.rs:358`),
   with the route handler running `process_erasure` under `spawn_blocking`.
4. **`Loki` existence** — confirm a live tenant-tagged Loki sink (else `NotApplicable`).

## Consequences

- **Erasure is correct for the shipped system** rather than literal-to-an-unshipped
  spec. No silent GDPR gap.
- The canonical 12-backend contract and `dsr_erasure_log` schema are **unchanged**;
  only transport semantics + one additive `BackendCompletion` field change.
- `privacy_model.md §6.2` should be amended (follow-up) to mark the Neon-primary
  control-plane as the deferred end-state and document the D1-primary interim.
- **Safety:** nothing in the pipeline deletes in prod until owner provisioning
  (task #46: create `corelink-dsr-erasure{,-dlq}` queues, bind `ERASURE_SALT_KEY`
  + `CORELINK_INTERNAL_AUTH_KEY`). The route only mounts when the internal-auth key
  is set. Wave 1 builds the inert, verified pipeline; the owner flips it live.

### Wave 1 increment plan (each compiled + tested before the next)

0. Additive `BackendCompletion.subject_id_hash` + orchestrator derivation (crate; gap #1).
1. Real D1-backed `ErasureIdempotencyLedger` + `ErasureAuditSink` over `dsr_erasure_log` (zero delete risk; validates the bridge).
2. **D1 effective erase adapter** against the ratified erase-set (IRREVERSIBLE — gated on owner sign-off of the classification table above).
3. `R2Cas` refcount-safe + `R2Ac` (5 regions).
4. `Stripe` pseudonymize (+ `update_customer`) + the 4 pseudonymized WORM backends + `Kv`/`Loki` reconciled.
5. Wire `build_worker()` (replace `build_placeholder_worker`), 24h verification cron + BLAKE3-keyed report signer; full test pass; PR.

## R2-erasure open questions (increment 3 — cold-verified 2026-06-11, BLOCKING)

Increment 3 (`R2Cas` + `R2Ac`) was cold-verified against the shipped storage code
before writing any adapter. The verification surfaced that the canonical
`r2_cas.rs` stub ("refcount-aware soft-delete; blob shared with another tenant
via S-07 dedup") **does not match prod**, and that a naive D1-driven erase would
be **GDPR-incomplete**. Confirmed facts + the blocking open questions:

**Confirmed (safe to rely on):**
- CAS is **one bucket** (`corelink-cas-prod`), region carried in the key prefix.
  AC is **five buckets** (`corelink-ac-{sam,iad,lhr,nrt,syd}`), region in the
  bucket name. (`wrangler.toml`, all prod envs.)
- The `chunks` dedup refcount is **intra-tenant only** (PK `(tenant_id,
  chunk_digest)`; "cross-tenant chunk leak FM-303 impossible at storage layer").
  So the canonical `subject_unaffiliated` (blob shared with *another* tenant)
  arm **cannot occur** — a full per-tenant hard-delete is cross-tenant-safe by
  construction (the tenant_prefix is `HMAC(TDK, tenant_id)[:16]`).
- `R2S3Client` wraps `aws-sdk-s3`; `delete()` (idempotent `DeleteObject`) landed
  (`400dc6cd`). `ListObjectsV2` is **not yet implemented**.
- `ac_meta` (`0002`) indexes every AC entry per tenant (`(tenant_id,
  action_digest)` + `region` + materialised `tenant_prefix` BLOB) → AC erase can
  be D1-driven (no LIST needed): for each row, delete
  `<region>/<tenant_prefix_hex>/<action_digest>` from the `corelink-ac-<region>`
  bucket, then delete the `ac_meta` rows.

**BLOCKING open questions (must resolve before the CAS adapter is safe):**
1. **Native whole-blob CAS path.** `R2CasHandler` (CasWriteHandler) puts **whole
   blobs** at `<cas_region>/<tenant_prefix>/<digest>` — a path **separate** from
   the multipart `chunks` table. A `chunks.r2_object_key`-driven erase would
   **orphan every native whole-blob object** = incomplete erasure. The safe
   design is **`ListObjectsV2` by prefix** `<region>/<tenant_prefix>/` (+
   `chunk-<region>/<tenant_prefix>/`) then delete-all — complete by construction.
2. **`blob_meta` / `manifest_chunks` have NO INSERT site in the container Rust**
   (grep clean). Either a TS Worker writes them or the live model differs from
   the schema. Until the authoritative CAS index/write path is known, a
   D1-driven CAS erase cannot be proven complete.
3. **Multi-region residency.** Whether a single tenant's CAS bytes can span
   multiple storage regions (so erase must LIST all 5 region prefixes) or are
   pinned to one `primary_region`. Determines the LIST fan-out.

**Decision:** do NOT ship a chunks-only CAS erase. Increment 3 is gated on (a)
adding `R2S3Client::list_objects_v2` (paginated) and (b) a focused cold-map of
the **complete** live CAS write/index/residency model (container Rust + the TS
Workers). `R2Ac` is unblocked and may land first. Until then the `R2Cas`/`R2Ac`
backends remain the inert `InMemory` placeholders (pipeline is prod-inert until
task #46 regardless).

## References

- `privacy_model.md §6.2` (canonical erasure pipeline — the spec being reconciled).
- `crates/corelink-privacy-erasure-worker/src/backends/*.rs` (the 12 spec stubs).
- `migrations/d1/0022_dsr_erasure_log.sql` (ledger schema + 7y retention + INV-DATA-ERASURE-COMPLETE).
- `migrations/neon/` (proves Neon = audit shadow only) · `migrations/002_auth_tables.sql` (deferred auth-spine).
- [[pr4-spine-design]] (auth-spine deferral) · [[dsr-wi-s11-008]] (the work item).
