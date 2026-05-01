# Auth event taxonomy — EVT-047 family (33 canonical types)

> **Source of truth**: [`crates/corelink-audit/src/events.rs`](../../crates/corelink-audit/src/events.rs).
> This doc summarizes the canonical taxonomy enforced by the
> `AuthEventType::canonical()` iterator + `tests/canonical_vectors.rs`
> pinning. Drift between this doc and the Rust enum is a P0 — the
> Rust enum is canonical.

CoreLink audit events are CloudEvents 1.0 envelopes carrying an
`auth.*` type tag plus a CoreLink-extension payload. Every
authenticated S-03 handler emits one or more of these events into
the `audit_outbox` (atomic with the handler transaction); the S-09
chain processor consumes them and seals them into the audit chain.

## CloudEvents 1.0 envelope shape

| CE field | Source | Example |
|---|---|---|
| `specversion` | constant | `"1.0"` |
| `id` | UUIDv7 (per-event) | `019de1cb-57d1-7ef2-a686-75adcd01a16e` |
| `source` | `corelink://<region>/<emitter>` | `corelink://wnam/auth/middleware` |
| `type` | canonical taxonomy string | `auth.token.validated` |
| `time_unix_ms` | producer wall-clock at emit | `1700000000000` |
| `datacontenttype` | constant | `"application/json"` |
| `tenant_id` | UUIDv7 hyphenated lowercase | `01938af0-aaaa-bbbb-cccc-ddddeeeeffff` |
| `principal_id_hash` | 16-hex SHA-256 prefix | `70e76bb54ae2d36d` |
| `region` | canonical short tag | `wnam` |
| `request_id` | correlation id (gRPC `x-request-id`) | `req_abc123` |
| `retention_hint` | per-tenant tier hint | `team90d` |
| `data` | type-tagged payload | `{ "type": "auth.token.validated", "fields": { … } }` |

## Type taxonomy (33 canonical strings)

### Token lifecycle (4)

| Type | Producer | Payload | Notes |
|---|---|---|---|
| `auth.token.issued` | WI-S03-002 (PAT mint) | `token_kind`, `pat_id_hash?`, `scope_bitset` | New PAT minted OR new Clerk session |
| `auth.token.validated` | WI-S03-003 middleware | `token_kind`, `scope_bitset` | Pre-handler success |
| `auth.token.revoked` | WI-S03-004 revocation | `token_kind`, `pat_id_hash?` | Explicit revocation |
| `auth.token.expired` | WI-S03-003 middleware | `token_kind` | Token presented post-`exp` |

### Auth session flow (3)

| Type | Producer | Payload | Notes |
|---|---|---|---|
| `auth.session.created` | WI-S03-001 (Clerk JWKS) | `session_id` | Clerk login OR PAT bound to session |
| `auth.session.revoked` | WI-S03-004 revocation | `session_id` | Explicit revocation |
| `auth.anomaly.token_replay_detected` | WI-S09 chain processor | `token_kind`, `signal` | **SEV-1** — challenge replay or sign_count regression |

### Auth denied — legacy generic (2)

Kept for backwards compat per WI §1 canonical enum; new producers
SHOULD prefer the granular variants below.

| Type | Producer | Payload | Notes |
|---|---|---|---|
| `auth.denied.scope` | WI-S03-003 middleware | `token_kind` | Legacy generic; prefer `scope_insufficient` |
| `auth.denied.invalid` | WI-S03-003 middleware | `token_kind` | Legacy generic; prefer `signature_invalid` / `malformed` / `not_found` |

### Auth denied — granular (7; Lote 10.3bis P0 expansion)

| Type | Producer | Payload | Notes |
|---|---|---|---|
| `auth.denied.signature_invalid` | WI-S03-001/002 verify | `token_kind` | HMAC mismatch / RS256 alg drift |
| `auth.denied.expired` | WI-S03-003 middleware | `token_kind` | Token past `exp` |
| `auth.denied.scope_insufficient` | WI-S03-003 middleware | `token_kind`, `required_scope_bitset`, `actual_scope_bitset` | Scope absent |
| `auth.denied.not_found` | WI-S03-001/002 verify | `token_kind` | Token id absent from store |
| `auth.denied.malformed` | WI-S03-001/002 verify | `token_kind` | Wire-format unparseable |
| `auth.denied.revoked` | WI-S03-003 middleware | `token_kind` | Token explicitly revoked |
| `auth.denied.rate_limit` | WI-S03-003 middleware | `token_kind`, `window_ms` | Per-PAT ceiling hit |

### Tenant + membership lifecycle (7)

| Type | Producer | Payload | Notes |
|---|---|---|---|
| `auth.tenant.provisioned` | S-19 onboarding | `owner_email_hash` | New tenant created |
| `auth.tenant.deleted` | S-11 DSR | `dsr_triggered` | Tenant erasure (LGPD Art. 18) |
| `auth.account.created` | S-19 onboarding | `email_hash` | New user account |
| `auth.account.deleted` | S-11 DSR | `dsr_triggered` | DSR erasure trigger |
| `auth.membership.added` | WI-S03-005 schema | `role` | User joins tenant |
| `auth.membership.removed` | WI-S03-005 schema | `role` | User leaves tenant |
| `auth.membership.role_changed` | WI-S03-005 schema | `from_role`, `to_role` | Member → Admin etc. |

### WebAuthn (6)

| Type | Producer | Payload | Notes |
|---|---|---|---|
| `auth.webauthn.registered` | WI-S03-006 engine | `credential_id_hash`, `aaguid_hex` | New credential registered |
| `auth.webauthn.authenticated` | WI-S03-006 engine | `credential_id_hash`, `sign_count` | Step-up succeeded |
| `auth.webauthn.deleted` | WI-S03-006 engine | `credential_id_hash` | User-initiated delete |
| `auth.webauthn.sign_count_regression` | WI-S03-006 engine | `credential_id_hash`, `previous_sign_count`, `observed_sign_count` | **SEV-1** — cloned authenticator |
| `auth.webauthn.origin_attack_attempt` | WI-S03-006 engine | `submitted_origin` | **SEV-1** — origin allowlist mismatch |
| `auth.webauthn.new_device_used` | WI-S09 anomaly | `credential_id_hash` | Passkey synced to new device (anomaly hint) |

### PAT lifecycle (1)

| Type | Producer | Payload | Notes |
|---|---|---|---|
| `auth.pat.scope_escalated` | WI-S03-004 revocation | `pat_id_hash`, `previous_scope_bitset`, `new_scope_bitset` | **SEV-1** — privileged op or insider abuse |

### Admin op (2)

| Type | Producer | Payload | Notes |
|---|---|---|---|
| `auth.admin_op.webauthn_authenticated` | WI-S03-006 engine | `op_class` | Step-up token minted |
| `auth.admin_op.mass_revoke` | WI-S03-004 admin | `revoked_count` | **SEV-1** — bulk revocation |

### Anomaly (1)

| Type | Producer | Payload | Notes |
|---|---|---|---|
| `auth.anomaly.cross_region_burst` | WI-S09 anomaly | `distinct_regions` | **SEV-1** — same principal in > 3 regions in 5-min window |

## SEV-1 fan-out set (exactly 6)

The `MultiplexEmitter` (S-09 wiring) writes these events to the
outbox AND fans out to a direct SIEM webhook. The 60-s outbox-drain
lag is unacceptable for incident response.

```
auth.anomaly.token_replay_detected
auth.webauthn.sign_count_regression
auth.webauthn.origin_attack_attempt
auth.pat.scope_escalated
auth.admin_op.mass_revoke
auth.anomaly.cross_region_burst
```

`AuthEventType::is_sev1()` is the canonical predicate; the set is
asserted by `tests/canonical_vectors.rs::sev1_fanout_set_canonical`.

## PII redaction surface

Every PII-bearing field uses a hash newtype derived via SHA-256
prefix-16-hex:

| Newtype | Canonical input | Output | Notes |
|---|---|---|---|
| `PrincipalIdHash` | Clerk `sub` claim or PAT principal | 16 hex chars | Audit-chain pseudonym |
| `PatIdHash` | PAT id (UUIDv7 string) | 16 hex chars | — |
| `EmailHash` | raw email | 16 hex chars | Distinct from `corelink-auth-schema::EmailHashKey` (HKDF + per-tenant salt) — the audit pseudonym is intentionally un-salted for cross-tenant forensic correlation |
| `WebAuthnCredentialIdHash` | `CredentialId` bytes | 16 hex chars | — |

There is **no public `From<String>` / `new(&str)`** for any of them
— the only constructor is the SHA-256 derivation.

## Per-tenant retention hint

| Tier | Hint | Days |
|---|---|---|
| Solo | `solo30d` | 30 |
| Team | `team90d` | 90 |
| Business | `business1y` | 365 |
| Enterprise | `enterprise7y` | 2555 |

`RetentionHint::for_tier(TenantTier)` is the single canonical mapping
(asserted by 10 000-iter property test).

## Producer cookbook

```rust
use corelink_audit::{
    AuthEvent, AuthEventData, AuthEventType, Emitter, PrincipalIdHash,
    RegionTag, RequestId, RetentionHint, TenantId, TenantTier, TokenKind,
};

let event = AuthEvent::new(
    AuthEventType::TokenValidated,
    "corelink://wnam/auth/middleware",
    tenant_id,
    PrincipalIdHash::derive(clerk_sub)?,
    RegionTag::Wnam,
    RequestId::new(grpc_x_request_id),
    RetentionHint::for_tier(TenantTier::Team),
    SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as i64,
    AuthEventData::TokenValidated {
        token_kind: TokenKind::Pat,
        scope_bitset: 0b0000_0001,
    },
);

emitter.emit(event)?;
```

## See also

- ADR-0033 — design rationale + JCS / outbox / hash-newtype trade-offs.
- WI-S03-007 — implementation work item (HIGH_RISK lane).
- `invariant_registry.md` — INV-AUDIT-NO-RAW-PII / EMIT-ATOMIC-WITH-HANDLER /
  CHAIN-HASH-DETERMINISTIC / EVENT-TYPE-EXHAUSTIVE / RETENTION-HINT-ACCURATE.
- `crates/corelink-audit/` — canonical Rust source.
