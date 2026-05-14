-- Migration 0037: Signup orchestration + atomic provisioning (WI-S19-001)
--
-- Creates five tables underpinning the atomic D1 signup pipeline. Per
-- Lote 10.19 codex P0 canonical scope clarification on
-- `INV-ONBOARD-ATOMIC-PROVISIONING`: the atomic boundary covers
-- `tenant` + `dpa_acceptance_pending` + `pat` (first PAT) + the
-- `usage_counter` initialiser. The `signup_orchestration` row plus the
-- `signup_attempts` log are bookkeeping companions that the production
-- Cloudflare Worker handler writes alongside the atomic INSERTs but
-- which are NOT load-bearing for the invariant (the orchestrator's
-- audit chain is the canonical source of truth; these rows serve the
-- DASH-ONBOARDING dashboard + RB-FM-SIGNUP-FAILED runbook).
--
-- D1 is the durable mirror for the in-process `corelink-signup`
-- orchestrator. Production wiring runs a daily reconcile against the
-- Stripe customer list to detect drift in the `pending_billing_link`
-- saga compensation lane (procedure deferred to PRR ship gate;
-- equivalent of docs/ops/d1-r2-reconcile.md for the signup ledger).
--
-- Security / privacy (CTRL-PRIV-001 + STRIDE Information Disclosure):
--   - user_email NEVER stored plaintext; only sha256 hex digest in
--     `tenant.email_hash` and `signup_attempts.email_hash`.
--   - PAT raw material NEVER stored; only the hybrid HMAC + Argon2id
--     digest in `pat.pat_hash`.
--   - shown_once_token UUID v7 invalidated after first GET (WI-S19-006
--     owns the reveal endpoint; this migration only persists the token
--     id alongside the PAT row).
--   - correlation_id (PAT-CORRELATION-ID-001) propagates from the
--     Clerk webhook event id through every row in this migration.
--
-- Additive policy: no DROP statements; every CREATE is IF NOT EXISTS;
-- every INSERT is OR IGNORE.  Reversible via 0037_signup_orchestration
-- _down.sql (deferred to PRR ship gate; not required by additive
-- migration policy on initial introduction).

-- ===========================================================
-- tenant — atomic-tx target table (load-bearing for
-- INV-ONBOARD-ATOMIC-PROVISIONING).
-- ===========================================================
CREATE TABLE IF NOT EXISTS tenant (
    -- Tenant id (UUID v7).
    tenant_id           TEXT    NOT NULL PRIMARY KEY,
    -- Signup id (UUID v7); idempotency cache key target.
    signup_id           TEXT    NOT NULL UNIQUE,
    -- sha256 hex digest of normalized email (RFC 5321 + lowercase).
    -- UNIQUE constraint prevents the race-condition exploit where two
    -- concurrent Clerk webhook events for the same user produce two
    -- tenants (Gherkin acceptance: "Race condition 100 concurrent
    -- email_verified same user → 1 tenant created").
    email_hash          TEXT    NOT NULL UNIQUE,
    -- Primary region pinned per Lote 10.16 cookie-canonical map;
    -- immutable post-INSERT per INV-REGION-NO-CROSS-LEAK (S-14
    -- alignment).
    primary_region      TEXT    NOT NULL CHECK (primary_region IN ('enam', 'sam', 'eu')),
    -- Tenant state: `active` = DPA accepted + Stripe linked;
    -- `dpa_pending` = DPA awaiting click-through (WI-S19-002);
    -- `pending_billing_link` = atomic D1 tx committed but Stripe link
    -- pending saga compensation (chaos Stripe outage path; WI §9.2
    -- PAT-DEGRADE-001 degrade read-only). Production wiring transitions
    -- via the Stripe-customer-link saga worker.
    tenant_state        TEXT    NOT NULL DEFAULT 'dpa_pending' CHECK (
        tenant_state IN ('active', 'dpa_pending', 'pending_billing_link', 'degraded_read_only')
    ),
    -- Stripe customer id (NULL until saga compensation completes).
    stripe_customer_id  TEXT,
    -- Created/updated timestamps (ms since epoch).
    created_ms          BIGINT  NOT NULL,
    updated_ms          BIGINT  NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_tenant_email_hash
    ON tenant (email_hash);

CREATE INDEX IF NOT EXISTS idx_tenant_state
    ON tenant (tenant_state);

CREATE INDEX IF NOT EXISTS idx_tenant_primary_region
    ON tenant (primary_region);

-- ===========================================================
-- dpa_acceptance_pending — atomic-tx target (WI-S19-002 owns the
-- rendered click-through + JWT receipt; this WI persists the pending
-- marker atomically with the tenant row per Lote 10.19 P0 canonical
-- scope).
-- ===========================================================
CREATE TABLE IF NOT EXISTS dpa_acceptance_pending (
    tenant_id           TEXT    NOT NULL PRIMARY KEY,
    created_ms          BIGINT  NOT NULL,
    FOREIGN KEY (tenant_id) REFERENCES tenant(tenant_id)
);

-- ===========================================================
-- pat — first-PAT row (atomic-tx target).
-- PAT raw material NEVER stored; only the hybrid HMAC + Argon2id
-- digest appears in `pat_hash`.  The `shown_once_token` is invalidated
-- by the WI-S19-006 reveal endpoint on first GET.
-- ===========================================================
CREATE TABLE IF NOT EXISTS pat (
    pat_id              TEXT    NOT NULL PRIMARY KEY,
    tenant_id           TEXT    NOT NULL,
    -- hybrid HMAC + Argon2id digest (S-03 canonical format; data_model
    -- §4.1).
    pat_hash            TEXT    NOT NULL UNIQUE,
    -- Scope: `read-write` (first PAT canonical per WI §6.1.4).
    scope               TEXT    NOT NULL DEFAULT 'read-write'
        CHECK (scope IN ('read-write', 'read-only', 'admin')),
    -- Expiry (90d canonical per WI §9.4 + AWS access keys rotation
    -- cadence parity).
    expires_ms          BIGINT  NOT NULL,
    -- Shown-once reveal token (UUID v7); invalidated after first GET.
    shown_once_token    TEXT    NOT NULL UNIQUE,
    -- Has the shown-once token been consumed?  (Default 0; the
    -- WI-S19-006 reveal endpoint flips to 1 atomically with the
    -- reveal response.)
    shown_once_consumed INTEGER NOT NULL DEFAULT 0
        CHECK (shown_once_consumed IN (0, 1)),
    created_ms          BIGINT  NOT NULL,
    FOREIGN KEY (tenant_id) REFERENCES tenant(tenant_id)
);

CREATE INDEX IF NOT EXISTS idx_pat_tenant
    ON pat (tenant_id);

CREATE INDEX IF NOT EXISTS idx_pat_expires
    ON pat (expires_ms);

-- ===========================================================
-- usage_counter — atomic-tx target (initialised at 0 per axis;
-- reset_ms = created_ms + 30d at INSERT time).
-- ===========================================================
CREATE TABLE IF NOT EXISTS usage_counter (
    tenant_id           TEXT    NOT NULL PRIMARY KEY,
    cas_bytes_used      BIGINT  NOT NULL DEFAULT 0
        CHECK (cas_bytes_used >= 0),
    ac_calls_used       BIGINT  NOT NULL DEFAULT 0
        CHECK (ac_calls_used >= 0),
    reset_ms            BIGINT  NOT NULL,
    FOREIGN KEY (tenant_id) REFERENCES tenant(tenant_id)
);

-- ===========================================================
-- signup_orchestration — idempotency cache + orchestration audit
-- companion (bookkeeping; not load-bearing for
-- INV-ONBOARD-ATOMIC-PROVISIONING).
-- ===========================================================
CREATE TABLE IF NOT EXISTS signup_orchestration (
    signup_id           TEXT    NOT NULL PRIMARY KEY,
    -- Idempotency-Key header value (R-S19-1).
    idempotency_key     TEXT    NOT NULL UNIQUE,
    -- Tenant id linked to this signup (NULL only if the orchestrator
    -- rejected pre-tx; tx-rollback path also leaves this NULL).
    tenant_id           TEXT,
    -- Clerk webhook event id (PAT-CORRELATION-ID-001 source).
    clerk_event_id      TEXT    NOT NULL,
    -- Correlation id propagated through the pipeline.
    correlation_id      TEXT    NOT NULL,
    -- Terminal outcome label (matches the
    -- `SignupOutcome` discriminant: `provisioned` / `deferred` /
    -- `duplicate` / `rejected`).
    outcome             TEXT    NOT NULL CHECK (
        outcome IN ('provisioned', 'deferred', 'duplicate', 'rejected')
    ),
    -- Billing intent flag (`linked` / `pending_billing_link` /
    -- `deferred` for non-rejected outcomes; NULL otherwise).
    billing_intent      TEXT
        CHECK (billing_intent IS NULL OR billing_intent IN ('linked', 'pending_billing_link', 'deferred')),
    created_ms          BIGINT  NOT NULL,
    updated_ms          BIGINT  NOT NULL,
    FOREIGN KEY (tenant_id) REFERENCES tenant(tenant_id)
);

CREATE INDEX IF NOT EXISTS idx_signup_orchestration_idempotency
    ON signup_orchestration (idempotency_key);

CREATE INDEX IF NOT EXISTS idx_signup_orchestration_outcome
    ON signup_orchestration (outcome, created_ms);

CREATE INDEX IF NOT EXISTS idx_signup_orchestration_correlation
    ON signup_orchestration (correlation_id);

-- ===========================================================
-- signup_attempts — append-only orchestration step log (audit
-- companion; chaos / atomicity rollback debugging).  Per
-- INV-AUDIT-APPEND-ONLY this table is INSERT-only; no UPDATE / DELETE.
-- ===========================================================
CREATE TABLE IF NOT EXISTS signup_attempts (
    attempt_id          TEXT    NOT NULL PRIMARY KEY,
    -- Idempotency-Key header value (joinable with
    -- signup_orchestration).
    idempotency_key     TEXT    NOT NULL,
    -- sha256 hex digest of normalized email (CTRL-PRIV-001 -- never
    -- plaintext).
    email_hash          TEXT    NOT NULL,
    -- Orchestration step at which the attempt was emitted (matches
    -- `OrchestrationStep` discriminant).
    step                TEXT    NOT NULL CHECK (
        step IN (
            'begin',
            'insert_tenant',
            'insert_dpa',
            'insert_first_pat',
            'insert_usage_counter_and_commit'
        )
    ),
    -- Event type (`corelink.signup.{started, completed, failed,
    -- deferred}`).
    event_type          TEXT    NOT NULL CHECK (
        event_type IN (
            'corelink.signup.started',
            'corelink.signup.completed',
            'corelink.signup.failed',
            'corelink.signup.deferred'
        )
    ),
    -- Optional terse reason label (populated for failed / deferred).
    reason              TEXT,
    correlation_id      TEXT    NOT NULL,
    created_ms          BIGINT  NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_signup_attempts_idempotency
    ON signup_attempts (idempotency_key, created_ms);

CREATE INDEX IF NOT EXISTS idx_signup_attempts_event_type
    ON signup_attempts (event_type, created_ms);

CREATE INDEX IF NOT EXISTS idx_signup_attempts_step
    ON signup_attempts (step, created_ms);

CREATE INDEX IF NOT EXISTS idx_signup_attempts_correlation
    ON signup_attempts (correlation_id);
