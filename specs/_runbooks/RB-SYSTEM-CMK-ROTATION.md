---
id: "RB-SYSTEM-CMK-ROTATION"
type: "runbook"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.1.0"
created: "2026-05-14"
updated: "2026-05-27"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["runbook", "s19", "cmk", "rotation", "byok", "envelope-encryption", "enterprise-inquiry", "r2-11"]
---

> **Post Wave 35 Phase 2 update 2026-05-27:** `corelink-byok-aws` and `corelink-byok-gcp` were absorbed into `corelink-byok` via inline `mod aws;` / `mod gcp;` (feature-gated) per SEAL `specs/_audits/sealed/2026-05-26-w35-p2-byok-absorption.md`. Canonical consumer paths are now `corelink_byok::aws::*` and `corelink_byok::gcp::*` (R2-6 / R2-7 KMS providers). Operational references using the absorbed crate paths still work for HISTORICAL log/grep reference but new automation should use the umbrella with the relevant `byok-{aws,gcp}-real` feature flags.

<!-- forensics-backlink -->
> **Forensics:** see `docs/internal/FORENSICS-GUIDE.md` §7.

# RB-SYSTEM-CMK-ROTATION — System CMK rotation & lifecycle

**Scope.** Customer-managed key (CMK) used by CoreLink to envelope-encrypt
pre-tenant **prospect** payloads — specifically the
`enterprise_inquiries.encrypted_payload_b64` column written by
`corelink-enterprise-inquiry::EnterpriseInquiryLedger::submit_inquiry`
when the inquiry submitter has no `tenant_id` yet (R2-11 / S-19
P1-NEW-3).

This CMK is owned by CoreLink (NOT by a customer). It is distinct from
the per-tenant BYOK CMK which customers manage via the
`corelink-byok-aws` / `corelink-byok-gcp` providers and rotate
themselves on whatever cadence their KMS policy mandates.

The AAD `tenant_id` field of every prospect-side seal is the canonical
literal `system::prospect`
(`corelink_enterprise_inquiry::SYSTEM_CMK_TENANT_TAG`).

---

## 1. Bindings

| Env var | Cloud | Form | Notes |
|---|---|---|---|
| `CORELINK_SYSTEM_CMK_ARN` | AWS | `arn:aws:kms:<region>:<acct>:key/<uuid>` | Multi-region key with replicas in `us-east-1` + `eu-west-1`; key policy grants `kms:Encrypt`/`Decrypt`/`DescribeKey` to the CoreLink Worker IAM role only. |
| `CORELINK_SYSTEM_CMK_RESOURCE` | GCP | `projects/<proj>/locations/<region>/keyRings/<ring>/cryptoKeys/<key>` | Auto-rotation = 90d server-side (Cloud KMS managed); manual rotation supplements during incident response. |
| `CORELINK_SYSTEM_CMK_SEARCH_DOMAIN_KEY_B64` | both | base64 32-byte HMAC-SHA256 key | Per-deployment static secret used to derive the AAD `company_hash_hex` HMAC of the company name. Rotated annually OR on incident; rotation re-seals all rows (see §5). |

Both KMS providers MUST be configured with `EncryptionContext` AAD
binding (mandatory per `INV-BYOK-CRYPTO-SOVEREIGNTY`).

---

## 2. Rotation cadence

| Trigger | Action | Owner |
|---|---|---|
| **Scheduled (annual)** | Generate new CMK version; flip `CORELINK_SYSTEM_CMK_*` to point at the new ARN/resource; backfill (§4). | Security on-call (Q1 each year). |
| **Scheduled (90d, GCP only)** | Auto-rotation on the GCP key ring; CoreLink Worker decrypts via the active version automatically (Cloud KMS internal version tagging on the ciphertext). | Cloud KMS managed. |
| **Incident (suspected compromise)** | Immediate rotation via §3. Pages out to Security + Eng on-call. | Security on-call. |
| **Personnel change (Security FTE departure)** | Rotation within 7d of departure if the departing employee held the BREAK_GLASS audit role. | Security lead. |

The `CORELINK_SYSTEM_CMK_SEARCH_DOMAIN_KEY_B64` rotates on the same
annual cadence; rotation requires a re-seal of every row that uses the
HMAC search-domain column for index lookup. The 90d server-side GCP
rotation does NOT trigger a search-domain key rotation.

---

## 3. Incident-driven rotation procedure

1. **Verify the scope.** Confirm the CMK is the *system* CMK (not a
   customer's BYOK CMK). For BYOK-CMK incidents see
   `RB-BYOK-KILL-SWITCH.md`.
2. **Mint the replacement.**
    - AWS: `aws kms create-key --multi-region --description "corelink-system-cmk-v<N+1>"` + replicate to `eu-west-1` + apply the key policy from `infra/kms/system-cmk-policy.json`.
    - GCP: `gcloud kms keys create system-cmk-v<N+1> --keyring=corelink-system --location=global --purpose=encryption --rotation-period=90d --next-rotation-time=...`.
3. **Stage the env flip.** Update `wrangler.toml` + Cloudflare Worker secret
   bindings to set `CORELINK_SYSTEM_CMK_ARN_NEXT` /
   `CORELINK_SYSTEM_CMK_RESOURCE_NEXT` to the new id. Deploy.
4. **Dual-write window (24h).** The Worker is bound to BOTH CMKs;
   every new `submit_inquiry` call seals under the NEW CMK; reads
   transparently fall through to the OLD CMK on cache miss
   (`corelink-byok::DekCache` honours both wrapped DEKs because the
   `WrappedDek.key_id` is per-row).
5. **Backfill (§4).** Re-seal every row whose `wrapped_dek.key_id`
   still points at the OLD CMK.
6. **Cutover.** Once §4 reports zero rows under the OLD CMK, flip
   `CORELINK_SYSTEM_CMK_ARN` to the new id, drop `_NEXT`, deploy, and
   schedule the OLD CMK for deletion (AWS: 30d pending window; GCP:
   `DESTROY_SCHEDULED` + 24h grace).
7. **Audit.** Emit `corelink.security.system_cmk_rotation_completed`
   with `{from_key_id, to_key_id, rows_resealed, started_ms, completed_ms}`
   to the audit sink; attach the run summary to the security incident
   ticket.

---

## 4. Re-seal (backfill) procedure

The re-seal worker reads rows from `enterprise_inquiries` where the
sealed payload's `wrapped_dek.key_id` matches the OLD CMK id, unwraps
under the OLD CMK, re-seals under the NEW CMK, and PATCHes the row.

Constraints:

- **AAD preserved.** The AAD context is unchanged across the re-seal —
  same `tenant_id` (always `system::prospect`), same
  `company_hash_hex`, same `payload_hash_blake3_hex`. The DEK changes
  (fresh CSPRNG per re-seal call); the body ciphertext changes
  accordingly.
- **DEK cache TTL.** Per `INV-BYOK-CRYPTO-SOVEREIGNTY` the DEK cache
  TTL is ≤ 300 s hard. The re-seal worker MUST drain the cache between
  the OLD-unwrap and the NEW-wrap so the OLD DEK never lingers beyond
  the per-row scope.
- **Idempotency.** The re-seal worker dedup-key is
  `("system-cmk-rotation", from_version, to_version, inquiry_id)`.
  Re-running the worker is a no-op on rows already migrated.
- **Throughput.** Rate-limit to 100 rows/min to stay within KMS
  per-region quotas (AWS: 30k req/s aggregate, but bursty re-seal
  campaigns trigger CloudTrail anomaly alerts).
- **Audit.** Each row emits
  `corelink.security.system_cmk_row_resealed { inquiry_id, from_key_id, to_key_id }`.

If a row fails to unwrap (KMS reports `AccessDenied` /
`KMSInvalidStateException`), flag it for manual review — the row is
quarantined and the worker continues; resolution out of band per the
incident commander.

---

## 5. Search-domain key rotation procedure

Rotating `CORELINK_SYSTEM_CMK_SEARCH_DOMAIN_KEY_B64` invalidates every
row's `company_hash_hex` AAD binding. Steps:

1. Generate new 32-byte HMAC key via `openssl rand -base64 32`.
2. Stage as `CORELINK_SYSTEM_CMK_SEARCH_DOMAIN_KEY_B64_NEXT`. Deploy.
3. Backfill: for each row, unwrap under the current CMK (§4 above is a
   no-op CMK-wise here), re-compute the AAD `company_hash_hex` under
   the NEW key, re-seal (DEK rotates as a side-effect; that is
   intentional defence-in-depth).
4. Cutover the env var.

This is rarely needed; the search-domain key is NOT a key-of-keys —
its compromise leaks only the *deterministic mapping* `company_name →
company_hash_hex`, which an attacker can already brute-force from a
small dictionary of company names. The 32-byte length blocks
rainbow-table lookups but the key has lower sensitivity than the CMK.

---

## 6. Verification gate (post-rotation)

Run these checks within 30 min of cutover:

- [ ] `SELECT COUNT(*) FROM enterprise_inquiries WHERE encrypted_payload_b64 LIKE '%<old-cmk-tag>%'` returns 0.
- [ ] `aws kms describe-key --key-id <old-arn>` returns `KeyState=PendingDeletion`.
- [ ] Synthetic prospect inquiry submission via `corelink-cli inquiry submit --dry-run` succeeds end-to-end (seal + unseal at HubSpot adapter boundary).
- [ ] Audit chain contains the `system_cmk_rotation_completed` event with `rows_resealed > 0`.
- [ ] `corelink_byok_envelope_encrypt_total{provider="aws",key_version="<NEW>"}` is climbing in Grafana; old-version counter is flat.

---

## 7. References

- `crates/corelink-enterprise-inquiry/src/encryption.rs` — seal /
  unseal pipeline; AAD context derivation;
  `SYSTEM_CMK_TENANT_TAG` constant.
- `crates/corelink-byok/src/envelope.rs` — envelope encryption write /
  read paths; DEK cache TTL hard limit.
- `crates/corelink-byok-aws/src/lib.rs` — AWS KMS provider (R2-6).
- `crates/corelink-byok-gcp/src/lib.rs` — GCP KMS provider (R2-7).
- `migrations/d1/0040_enterprise_inquiries.sql` — D1 schema for
  `encrypted_payload_b64`.
- `specs/_audits/sealed/2026-05-14-s19-sprint-close-review-round1.md` §P1-NEW-3 —
  the audit finding this runbook + the R2-11 implementation closed.
