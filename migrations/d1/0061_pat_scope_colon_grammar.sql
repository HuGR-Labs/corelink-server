-- Migration 0061: correct the `pat.scope` CHECK to the colon-grammar
-- vocabulary that the running code and the provisioning path actually use.
--
-- ── The bug (launch landmine) ──────────────────────────────────────────────
-- Migration 0037 created `pat.scope` with:
--     scope TEXT NOT NULL DEFAULT 'read-write'
--         CHECK (scope IN ('read-write', 'read-only', 'admin'))
-- but the auth code speaks a DIFFERENT scope vocabulary:
--   * crates/corelink-container/src/scope.rs  — requires_cache_read/write only
--     recognise `cas:rw` | `cas:r` | `cas:w` | `admin` (anything else is
--     fail-CLOSED → no cache access).
--   * apps/signup-worker/src/webhooks/clerk.ts — the PRODUCTION self-serve
--     provisioning path issues PATs scoped `cas:rw` and persists that exact
--     string: `INSERT INTO pat (..., scope, ...) VALUES (..., 'cas:rw', ...)`.
--
-- Because the live CHECK forbids `cas:rw`, the very first real self-serve
-- signup would hit `CHECK constraint failed` on the PAT INSERT and the tenant
-- would never receive its token. Verified against prod D1 (corelink-prod-d1,
-- 2026-06-08): 25 PAT rows, ALL `scope='admin'` (legacy) — the `cas:rw`
-- provisioning path has never successfully written a row. The constraint is
-- live and enforced.
--
-- ── The fix ────────────────────────────────────────────────────────────────
-- SQLite/D1 cannot ALTER or DROP a CHECK in place, so this is the canonical
-- 12-step table rebuild (https://www.sqlite.org/lang_altertable.html#otheralter):
-- create the table anew with the corrected CHECK + DEFAULT, copy every row,
-- drop the old table, rename, recreate the indexes. No table FK-references
-- `pat` (verified on prod), so the DROP is dependency-safe; `pat` itself keeps
-- its FK to `tenant`. All 25 existing `admin` rows are preserved AS-IS — this
-- migration does NOT demote them (that is the owner-gated least-privilege
-- back-fill in scripts/backfill-admin-scope-prod.sh, which only becomes valid
-- AFTER this migration lands).
--
-- Additive-safety policy (INV-ONBOARD-ATOMIC-PROVISIONING): the data is fully
-- preserved; only the constraint vocabulary changes. Idempotency is provided
-- by the d1_migrations ledger (wrangler applies each file at most once).
--
-- Locally verified (python3 sqlite3, exact prod DDL + 25 seeded admin rows):
--   BEFORE: INSERT scope='cas:rw' → REJECTED by the old CHECK.
--   AFTER : 25 rows + 3 named indexes preserved; cas:rw/cas:r/cas:w/admin
--           accepted; an unknown scope still REJECTED; admin→cas:rw back-fill
--           UPDATE now valid.

CREATE TABLE pat_new (
    pat_id              TEXT    NOT NULL PRIMARY KEY,
    tenant_id           TEXT    NOT NULL,
    pat_hash            TEXT    NOT NULL UNIQUE,
    scope               TEXT    NOT NULL DEFAULT 'cas:rw'
        CHECK (scope IN ('cas:rw', 'cas:r', 'cas:w', 'admin')),
    expires_ms          BIGINT  NOT NULL,
    shown_once_token    TEXT    NOT NULL UNIQUE,
    shown_once_consumed INTEGER NOT NULL DEFAULT 0
        CHECK (shown_once_consumed IN (0, 1)),
    created_ms          BIGINT  NOT NULL,
    token_id            TEXT,
    FOREIGN KEY (tenant_id) REFERENCES tenant(tenant_id)
);

INSERT INTO pat_new (pat_id, tenant_id, pat_hash, scope, expires_ms, shown_once_token, shown_once_consumed, created_ms, token_id)
    SELECT pat_id, tenant_id, pat_hash, scope, expires_ms, shown_once_token, shown_once_consumed, created_ms, token_id
    FROM pat;

DROP TABLE pat;
ALTER TABLE pat_new RENAME TO pat;

-- Recreate the three named indexes exactly as they exist on prod
-- (idx_pat_token_id is UNIQUE — it is the Worker hot-path lookup key).
CREATE INDEX idx_pat_expires ON pat (expires_ms);
CREATE INDEX idx_pat_tenant ON pat (tenant_id);
CREATE UNIQUE INDEX idx_pat_token_id ON pat (token_id);
