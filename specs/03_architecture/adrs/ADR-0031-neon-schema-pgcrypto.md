---
id: "ADR-0031"
type: "adr"
doc_status: "FROZEN"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-01"
updated: "2026-05-01"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["adr", "neon", "postgres", "pgcrypto", "rls", "schema", "auth", "s03", "dsr"]
---

# ADR-0031 — Auth domain Neon Postgres schema: pgcrypto column encryption + RLS defence-in-depth + DSR cascade

## Status

FROZEN (S-03 WI-S03-005 ratified — SEALED 2026-05-01).

## Context

The auth domain canonical store is Neon Postgres (`data_model.md §4.1`,
`framework.md §17`). The schema must satisfy five distinct
load-bearing invariants from `invariant_registry.md`:

- **INV-AUTH-SCHEMA-RLS-DEFAULT-ON** (CRITICAL) — every auth table has
  RLS enabled; an application bug that forgets `SET LOCAL
  app.current_tenant` silently returns zero rows instead of
  cross-tenant data.
- **INV-AUTH-PII-ENCRYPTED** (CRITICAL) — `email`, `billing_email`,
  WebAuthn `public_key` and `attestation_object` ride as `BYTEA`
  ciphertext; an insider with read-only Neon access OR a leaked backup
  exposes cipher only.
- **INV-AUTH-MIGRATION-ADDITIVE** (HIGH) — every migration is
  additive; no `DROP TABLE` / `ALTER COLUMN TYPE` /
  `ALTER COLUMN DROP NOT NULL` / `RENAME` reaches `main` without an
  explicit ADR + dual-write window.
- **INV-AUTH-CASCADE-DSR-COMPLETE** (HIGH) — `account` hard-DELETE
  cascades through `tenant`, `membership`, `pat`, and
  `webauthn_credentials` so the S-11 erasure pipeline can satisfy
  LGPD Art. 18 / GDPR Art. 17 in a single transaction.
- **INV-AUTH-AUDIT-PSEUDONYMIZATION** (CRITICAL) — the audit chain
  retains pseudonymous identifiers across DSR erasure so the
  S-09 hash-chain integrity property is preserved without re-identifying
  the erased subject.

The canonical questions:

1. **D1 vs Neon for the auth domain** — Cloudflare D1 (regional SQLite)
   is great for CAS metadata but limits the auth domain
   (no logical replication, no `pgcrypto`, no rich `JSONB`,
   single-region). The auth domain is global, relational, and
   PII-bearing. Neon is the canonical fit.
2. **Column-level encryption: pgcrypto vs app-side** — both can satisfy
   the cipher-at-rest invariant. We prefer pgcrypto because the master
   key lives in Neon's secrets manager (separate from the application
   binary); an app compromise cannot exfiltrate ciphertext.
3. **Email lookup: encrypted scan vs deterministic hash** —
   `pgp_sym_encrypt` is randomised (different ciphertext for the same
   plaintext on every INSERT); equality lookups would require a full
   table decrypt scan. We pair the cipher column with a deterministic
   `email_hash BYTEA UNIQUE` derived from a key separate from the
   pgcrypto master.
4. **RLS lifecycle on Cloudflare Workers + sqlx** — `SET LOCAL` is
   transaction-scoped, so the application MUST wrap every
   tenant-scoped query in an explicit transaction. A `before_acquire`
   pool hook resets stale state; an empty / unset
   `app.current_tenant` falls through to a `false` policy predicate
   (zero rows returned, not cross-tenant exposure).
5. **DSR cascade vs audit retention** — `account → tenant → membership /
   pat / webauthn_credentials` cascades on hard DELETE; the
   `revocation_log` table is intentionally **not** a foreign key
   target (it outlives the PAT for cross-region propagation tracking)
   and the audit chain (S-09) stores pseudonymised identifiers so
   erasure does not corrupt the hash chain.

## Decision

### D1 — Neon Postgres is the canonical store for the auth domain

The seven canonical tables (`account`, `tenant`, `user_account`,
`membership`, `pat`, `webauthn_credentials`, `revocation_log`) live
in Neon. D1 retains `blob_meta` / `audit_outbox` / `ac_meta` /
`usage_counter` / `tenant_quota` / `tenant_storage_state` (regional
hot path; matches `data_model.md §4.2`).

### D2 — UUIDv7 minted app-side; no `DEFAULT gen_random_uuid()`

Every PRIMARY KEY UUID is minted via `uuid::Uuid::now_v7()` in
application code. The migration deliberately omits `DEFAULT
gen_random_uuid()` (which would produce v4) so a CI gate scanning the
migration text catches regressions.

### D3 — Encrypted columns ride as `BYTEA`; sibling `<col>_key_id INTEGER` enables multi-key rotation

`account.billing_email`, `user_account.email`,
`webauthn_credentials.public_key`, and
`webauthn_credentials.attestation_object` carry
`pgp_sym_encrypt_bytea(plaintext, master_key)` ciphertext. Each
column has a sibling `<col>_key_id INTEGER NOT NULL DEFAULT 1` that
records which generation of the pgcrypto master key was used so the
quarterly rotation worker can decrypt with v1, re-encrypt with v2,
and bump the `key_id` row by row without downtime.

### D4 — `email_hash` is `HMAC-SHA256(email_hash_key, lower(trim(email)))`; key derived via HKDF-SHA256 with domain-separated info bytes

```
email_hash       = HMAC-SHA256(email_hash_key, lower(trim(email)))
email_hash_key   = HKDF-SHA256(
  ikm  = master_key (Worker secret CORELINK_MASTER_KEY; 32 random bytes),
  salt = b"corelink-email-hash-salt-v1",
  info = b"corelink-v1-email-hash-key",
  L    = 32 bytes,
)
```

The `info` bytes are non-prefix against the four other HKDF info
labels in the system (`b"ac-sig"`, `b"manifest-sig"`,
`b"meta-manifest-sig"`, `b"audit-chain"`) — length-extension safe and
cross-domain collision-resistant.

The migration file refuses to run if `app.email_hash_key` is empty
(deploy guard at `migrations/002_auth_tables.sql §0`); a fail-open
deploy with NULL `email_hash` is not reachable.

### D5 — RLS default-on; `SET LOCAL app.current_tenant` per request transaction

All seven tables have `ENABLE ROW LEVEL SECURITY`. Tenant-scoped
tables (`tenant`, `membership`, `pat`, `revocation_log`) carry a
`tenant_isolation_*` policy that filters on
`tenant_id = current_setting('app.current_tenant', true)::uuid`.
Account-scoped tables (`account`, `user_account`,
`webauthn_credentials`) default to deny-all; admin queries elevate
via `SET LOCAL ROLE auth_admin` and are logged by the S-09 audit
chain.

The application MUST wrap every tenant-scoped query in an explicit
transaction:

```rust
let mut tx = pool.begin().await?;
sqlx::query("SELECT set_config('app.current_tenant', $1, true)")
    .bind(tenant_id.to_string())
    .execute(&mut *tx)
    .await?;
// … queries here run with RLS scoped …
tx.commit().await?;
```

A `before_acquire` pool hook resets `app.current_tenant` /
`app.email_hash_key` so a stale binding cannot leak across pool
checkout boundaries.

### D6 — Migrations are additive-only

`scripts/check_migrations_additive.py` scans every file in
`migrations/` and `migrations/d1/` for `DROP TABLE`, `DROP COLUMN`,
`DROP INDEX`, `DROP TYPE`, `DROP CONSTRAINT`, `DROP TRIGGER`,
`DROP FUNCTION`, `DROP EXTENSION`, `DROP POLICY`, `ALTER COLUMN …
TYPE`, `ALTER COLUMN … DROP NOT NULL`, `TRUNCATE`, `RENAME COLUMN`,
`RENAME TABLE`, `RENAME TO`. A line annotated `--
additive-allowed: ADR-NNNN <reason>` is exempted; the ADR records the
dual-write window. CI gate runs on every PR.

### D7 — DSR cascade chain hardcoded in FK policy

The cascade is `account → tenant → {membership, pat,
webauthn_credentials}` and `user_account → {membership,
webauthn_credentials}`. `pat.issued_to_user` is **nullable** with
`REFERENCES user_account(user_id)` (no cascade) so service / CI tokens
survive user erasure without orphaning. `revocation_log.pat_id` is a
plain UUID column (not a FK) so revocation-propagation rows survive
PAT erasure.

The simulator
(`crates/corelink-auth-schema/src/sim.rs::AuthSchema::dsr_hard_delete_account`)
exercises the cascade in property tests at 2 048 iterations × 1–4
tenants per account; production validation against staging Neon is
gated on the WI-S03-005 §10.5.7 chaos drill (deferred until staging
credentials land).

## Consequences

**Positive**:

- Insider DB read sees ciphertext only; backup leaks expose ciphertext
  with the master key in a separate KMS.
- App-layer bugs that forget the tenant filter return zero rows
  instead of cross-tenant data.
- Rotation is gradual + zero-downtime via the `key_id` column convention.
- LGPD Art. 18 / GDPR Art. 17 erasure is a single SQL transaction.
- Migration drift is caught by CI before merge.

**Negative**:

- ~5–10 % query overhead from RLS predicate evaluation on the hot path.
- Master-key rotation requires a background worker (S-13 forward).
- Email lookup depends on `email_hash_key` secrecy — compromise enables
  a rainbow attack across the user corpus. Mitigation: the key is
  separate from the pgcrypto master and rotated on its own quarterly
  cadence (`RB-EMAIL-HASH-KEY-ROTATION`, forward).

**Reversibility**:

- Switching back to D1-only is not feasible without rewriting the auth
  surface. Switching from `pgcrypto` to app-side encryption is feasible
  (write-side is migrated, then read-side adds an app-decrypt fallback
  + drop the pgcrypto column once the dual-write window closes); ADR
  reversal would land alongside the dual-write ADR.

## Alternatives considered

- **Per-tenant database (one Postgres DB per tenant)**: rejected.
  100 000 tenants × per-DB-creation cost is unmanageable; cross-tenant
  admin queries become impossible.
- **App-side AES-GCM**: rejected as the *only* layer (defence in depth
  is fine — S-14 BYOK adds it as Layer 2). App-side keys live in the
  Worker binary's runtime memory; an app compromise compromises the
  ciphertext.
- **Pure SHA-256 email hash (no HMAC key)**: rejected — global rainbow
  table attack viable across the user corpus.
- **Skip RLS, rely solely on app-layer `WHERE tenant_id = $1`**:
  rejected. Defence in depth requires the DB layer to enforce the
  filter even if the app forgets it; the cost is bounded.

## Cross-references

- `specs/04_sprints/S03/work_items/WI-S03-005-neon-schema-auth-tables.md`
- `specs/03_architecture/data_model.md §4.1`
- `specs/03_architecture/auth_model.md §2 / §5`
- `specs/03_architecture/key_management.md §3.2 / §3.13`
- `specs/03_architecture/privacy_model.md §3 / §7.1`
- `specs/03_architecture/invariant_registry.md` (5 INVs above)
- `migrations/002_auth_tables.sql`
- `crates/corelink-auth-schema/src/{sim.rs, email_hash.rs, pseudonymize.rs, rls.rs}`
- `scripts/check_migrations_additive.py`
- `RB-EMAIL-HASH-KEY-ROTATION` (forward; ST-019 Lote 10.3-tris)

## Change log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-05-01 | Gustavo (via Claude Opus 4.7) | ADR criado SEAL alinhado com WI-S03-005 SEAL: 7 tables + 6 enums + 11 indexes + RLS default-on + pgcrypto column encryption + HKDF-derived email_hash key + additive-only migration governance + DSR cascade chain. Whitelist em validate_references.py. |
