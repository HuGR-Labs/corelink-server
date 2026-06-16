-- Migration 0070: per-tenant Runners concurrency entitlement (`runners_entitlement`).
--
-- ## Why a dedicated table (NOT the cache-plan ladder)
--
-- The runners TL ratified (memory: runners-cap-option-b) that the runner
-- `max_concurrency` cap is a **SEPARATE entitlement axis** from the cache tier,
-- keyed by `tenant_id`. The `/internal/v1/auth/introspect` endpoint must report
-- the cap from THIS table, NOT derive it from the cache-plan ladder. A tenant's
-- cache `plan` (informational, still returned) says nothing about whether — or
-- how much — Runners concurrency that tenant is entitled to. Overloading the
-- cache tier would couple two independent product axes and silently grant
-- (or deny) Runners concurrency on a cache upgrade.
--
-- ## Fail-CLOSED semantics
--
-- The introspect handler does a single keyed lookup:
--   SELECT max_concurrency FROM runners_entitlement WHERE tenant_id = ?1
-- A row present → `max_concurrency: Some(row.max_concurrency)` on the wire.
-- NO row (the common case today — the table starts empty) → the field is
-- OMITTED (`skip_serializing_if`); the tenant is cache-only and gets no runner
-- cap (the runners fabric treats an absent cap as "no Runners entitlement" and
-- rejects the placement). Empty table = no cap = reject.
--
-- ## Columns
--
--   tenant_id        TEXT    PRIMARY KEY  — matches `tenant.tenant_id` (UUID str);
--                                          exactly one entitlement row per tenant.
--   max_concurrency  INTEGER NOT NULL CHECK(> 0) — the per-tenant runner cap. The
--                                          CHECK forbids a 0/negative cap: a cap
--                                          of "0" is expressed by the ABSENCE of a
--                                          row, never a zero-valued row (so the
--                                          presence of a row always means a real,
--                                          positive entitlement).
--   plan             TEXT                 — informational label for the entitlement
--                                          source/plan (nullable; the runners side
--                                          owns its own custom plan semantics).
--   created_at_ms    INTEGER NOT NULL     — provisioning wall-clock (Unix epoch ms),
--                                          for operator forensics. Matches the wider
--                                          D1 `*_at_ms` convention.
--
-- ## Idempotency / additive-only
--
-- `CREATE TABLE IF NOT EXISTS` lets the migration replay safely. The migration is
-- additive-only (INV-AUTH-MIGRATION-ADDITIVE): it adds a NEW table and never
-- alters/drops/renames an existing object, so it needs no ADR waiver and no
-- `-- additive-allowed:` suppression. No FK to `tenant` is declared (D1 FKs are
-- logical/per-connection inconsistent in CF Workers — see 0002/0003 — and the
-- entitlement table is read-only on the hot path).

CREATE TABLE IF NOT EXISTS runners_entitlement (
    tenant_id       TEXT    PRIMARY KEY,
    max_concurrency INTEGER NOT NULL CHECK (max_concurrency > 0),
    plan            TEXT,
    created_at_ms   INTEGER NOT NULL
);
