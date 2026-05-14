# Admin Plane — Config Singleton Architecture

> Scope: WI-S13-001 (DO config-singleton + CAS + propagation + rollback).
> Updated: 2026-05-14.

---

## Overview

The CoreLink admin plane config-singleton provides a **single source of truth**
for runtime configuration per Cloudflare region:

| Config domain | Type | Consumer |
|---|---|---|
| Feature flags | `Map<feature_id, FeatureFlag>` | All Workers |
| Rate-limit tunables | `Map<(layer, tier), RateLimitTunable>` | Rate-limit middleware |
| Retention policies | `Map<resource, RetentionPolicy>` | GC + DSR workers |

---

## Architecture: DO single-instance per-region

```
Admin CLI/UI
    │
    │ PUT /v1/admin/config  (CAS: expected_version, new_payload)
    ▼
Worker Handler (corelink-config-api)
    │ 1. Admin role check (CTRL-AUTHZ-001)
    │ 2. MFA freshness ≤ 30min (CTRL-AUTH-010)
    │ 3. Schema validation (deny_unknown_fields + validate_payload)
    ▼
DO ConfigSingletonDO  ←── "corelink-config-{region}" (idFromName)
    │ storage.transaction():
    │   if current_version != expected_version → 409 VersionConflict
    │   else: write new_payload, increment version
    │
    │ ──── D1 batch (atomic) ────────────────────────────────────────
    │   INSERT config_change_log (version, payload_hash, payload, …)
    │   INSERT audit_outbox     (admin.config.update event)
    │ ───────────────────────────────────────────────────────────────
    │
    └──→ Emit to Cloudflare Queue: cfg-change-region-{region}
              │
              ▼
         Worker consumer (per region)
              │ try_advance(new_version, new_payload) — version-monotone
              ▼
         In-memory ConfigSnapshot (updated ≤ 5s p99)
              │ safety-net: 60s periodic poll to DO
              ▼
         Rate-limit / GC / feature-flag evaluation uses snapshot
```

---

## Schema versioning policy

Every `ConfigPayload` carries `schema_version: u32`. The current supported
version is `1`.

**Breaking schema changes** (add required field, rename field, remove field):

1. File an ADR (e.g. ADR-S13-002).
2. Bump `SUPPORTED_SCHEMA_VERSION` in `corelink-config-do/src/types.rs`.
3. Provide migration path (default value for new fields in old payloads).
4. Old Workers with `schema_version < new` will reject payloads — deploy
   new Worker before updating config schema.

`serde(deny_unknown_fields)` ensures old Workers reject new schema versions
(risk R-003 — schema drift catastrophe).

---

## CAS retry pattern (client-side)

```
loop (max 3 attempts):
  1. GET /v1/admin/config/current → (version, payload)
  2. Modify payload locally.
  3. PUT /v1/admin/config body={expected_version: version, new_payload: …}
     - 200 → done
     - 409 VersionConflict → exponential backoff (100ms, 200ms, 400ms) → retry
     - 400 SchemaInvalid → fix payload, retry
     - other error → abort
abort with error if budget exhausted (CAS retry storm → metric alert)
```

Metric: `corelink_admin_config_cas_conflict_total{layer}` — alert if > 10/min.

---

## Propagation timing model

| Event | Latency |
|---|---|
| DO write to Queue emit | < 1ms |
| Queue delivery to Worker consumer | < 2s typical |
| Worker snapshot update | < 1ms |
| **Total (Queue path)** | **≤ 5s p99** |
| Safety-net poll | ≤ 60s (Queue outage recovery) |

SLO: `SLO-ADMIN-CONFIG-PROPAGATION` ≤ 5s p99 sustained 30d staging.

Metric: `corelink_admin_config_propagation_seconds_bucket` (histogram p50/p95/p99).

---

## Rollback drill playbook (monthly)

**Recovery target: ≤ 5 min p99 (SLO-ADMIN-CONFIG-ROLLBACK).**

```
# Step 1: Identify the good version.
GET /v1/admin/config/history?limit=20

# Step 2: Verify the target payload (review in admin UI).
# Confirm it does not violate current runtime invariants.

# Step 3: Initiate rollback (dual-approval required).
POST /v1/admin/config/rollback?to_version=<target_version>
Headers: X-Dual-Approver: <approver_user_id>
         Authorization: Bearer <admin_jwt_with_fresh_mfa>

# Step 4: Verify propagation (< 5s p99).
GET /v1/admin/config/current → expect new_version = previous_current + 1
# Check Worker snapshots in all regions.

# Step 5: Confirm audit log.
GET /v1/admin/config/history?limit=5
# Verify new entry with change_type='rollback'.
```

**Common failure scenarios:**

| Scenario | Error | Resolution |
|---|---|---|
| MFA stale (>30 min) | 401 MfaStale | Re-MFA; retry |
| No dual-approver | 403 DualApprovalMissing | Get second admin approval |
| Target > 90d old | 410 VersionExpired | Use R2 long-term archive |
| Target unknown | 404 VersionUnknown | Check history for correct version |
| Target schema drift | 400 SchemaInvalid | Target payload incompatible with current schema; manual restore required |

---

## D1 `config_change_log` schema

```sql
CREATE TABLE config_change_log (
    version          INTEGER PRIMARY KEY,
    payload_hash     BLOB(32)    NOT NULL,  -- SHA-256 JCS canonical JSON
    payload          BLOB        NOT NULL,  -- canonical JCS JSON
    actor_user_id    BLOB(16)    NOT NULL,  -- UUIDv7
    actor_email_hash BLOB(32)    NOT NULL,  -- SHA-256(email)
    mfa_ts_ms        INTEGER     NOT NULL,
    created_at_ms    INTEGER     NOT NULL,
    change_type      TEXT        NOT NULL CHECK (change_type IN ('update', 'rollback')),
    previous_version INTEGER
);
```

**Retention**: 90d hard. Daily cron 02:00 UTC purges rows where
`created_at_ms < now - 90d`. Older versions in R2 7y archive.

---

## Metrics (5 canonical)

| Metric | Type | Labels |
|---|---|---|
| `corelink_admin_config_propagation_seconds_bucket` | Histogram | — |
| `corelink_admin_config_cas_conflict_total` | Counter | `layer`, `plan` |
| `corelink_admin_config_rollback_total` | Counter | `outcome`, `plan` |
| `corelink_admin_config_update_total` | Counter | `outcome`, `plan` |
| `corelink_admin_config_history_size_gauge` | Gauge | — |

Dashboard: `DASH-ADMIN` widget group.

---

## References

- ADR-S13-001 — DO config-singleton single-instance per-region with CAS.
- WI-S13-001 spec — full design decisions, risk register, acceptance criteria.
- WI-S13-002 — admin API + dual-approval middleware (consumer of this foundation).
- `crates/corelink-config-do/src/lib.rs` — trait surface + in-memory impl.
- `crates/corelink-config-api/src/handlers.rs` — HTTP handler logic.
