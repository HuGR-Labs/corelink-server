-- Wave-29 stream-1 — pilot signup backend schema (DEBT-027 engineering-CLOSED).
--
-- Backs `POST /v1/signup/pilot/{token}` in
-- `apps/server/src/routes/signup.rs`. The route reserves a pilot slot
-- on a valid HMAC-signed pilot token, persists the reservation in this
-- table, emits `corelink.signup.pilot_reserved.v1`, and returns a
-- placeholder tenant_id + activation URL.
--
-- ## Lifecycle state
--
--   RESERVED   — token redeemed; pilot record persisted; activation URL
--                returned. Operator follow-up via `grant-pilot-tier.sh`.
--   PROVISIONED — operator has stamped `tenant.tier='pilot'` (wave-27
--                 `grant-pilot-tier.sh` outcome); awaiting first blob.
--   ACTIVE     — first blob committed; counts toward the GA-cutover
--                 ≥3-pilot gate (RB-GA-CUTOVER.md).
--   EXPIRED    — token redeemed but pilot never activated within the
--                 14-day window (operator policy; not enforced at the
--                 DB layer).
--   CANCELLED  — pilot withdrew or operator revoked.
--
-- ## Idempotency
--
-- `CREATE TABLE IF NOT EXISTS` lets the migration replay safely. The
-- table is additive-only (INV-AUTH-MIGRATION-ADDITIVE +
-- INV-AUDIT-APPEND-ONLY). The `email` column carries a UNIQUE index so
-- a duplicate signup with the same address is detectable at the
-- D1 layer; the route returns the existing `tenant_id` on duplicate.
--
-- ## PII
--
-- `email` is stored plaintext for the pilot programme (operator runs
-- outreach off this table — `list-pilot-tenants.sh`). The broader
-- production signup flow in `crates/corelink-signup` hashes
-- `user_email` per `UserEmailHash`; this table is operator-internal
-- and predates the email-hash discipline (DEBT-028 captures the lift
-- to email-hash + a separate operator-visible decryption seam if and
-- when GA-cutover graduates the pilot programme into self-serve).

CREATE TABLE IF NOT EXISTS pilot_signups (
    -- Surrogate UUID v7 key (lexicographic-sortable; matches the wider
    -- D1 schema convention).
    id TEXT PRIMARY KEY,
    -- Tenant id stamped at reservation time (the operator's
    -- `grant-pilot-tier.sh` reads this column to enact the tier grant).
    tenant_id TEXT NOT NULL,
    -- Pilot contact email (operator outreach surface).
    email TEXT NOT NULL,
    -- Free-text company name (≤ 256 chars; validated at the route).
    company_name TEXT,
    -- Free-text tier hint (`free` / `pro` / `enterprise` — non-binding
    -- preference captured for operator pre-screening per
    -- `docs/internal/customer-success-playbook.md §1`).
    tier_hint TEXT,
    -- Free-text expected-use-case (operator pre-screening surface).
    expected_use_case TEXT,
    -- Signup wall-clock (Unix epoch ms).
    signed_up_at INTEGER NOT NULL,
    -- Activation wall-clock (Unix epoch ms). NULL until the operator
    -- transitions the row to ACTIVE.
    activated_at INTEGER,
    -- Pilot lifecycle state — see header doc for the canonical 5-set.
    state TEXT NOT NULL CHECK (state IN (
        'RESERVED', 'PROVISIONED', 'ACTIVE', 'EXPIRED', 'CANCELLED'
    )),
    -- Source token id (the 16-hex random body of the pilot token; the
    -- HMAC signature is NOT persisted — only the body is preserved for
    -- replay detection and operator forensics).
    token_id TEXT NOT NULL
);

-- Duplicate-detection on `email` lets the route return the existing
-- `tenant_id` on second signup (idempotency at the email boundary).
CREATE UNIQUE INDEX IF NOT EXISTS idx_pilot_signups_email
    ON pilot_signups (email);

-- Token-id replay detection: once a token has been redeemed, the
-- route looks up here before honouring another redemption attempt
-- (idempotent — returns the original tenant_id).
CREATE UNIQUE INDEX IF NOT EXISTS idx_pilot_signups_token_id
    ON pilot_signups (token_id);

-- Operator analyst slice — "every signup in the last 24h" off the
-- `list-pilot-tenants.sh` panel.
CREATE INDEX IF NOT EXISTS idx_pilot_signups_state_signed_up
    ON pilot_signups (state, signed_up_at);
