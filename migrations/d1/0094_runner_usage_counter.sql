-- 0094_runner_usage_counter.sql
--
-- WI-S10-007 (runner compute overage billing) — the DEDICATED runner-compute
-- counter lane: `runner_usage_counter` (per-tenant, per-region, per-billing-period
-- aggregate of the billable `runner_vcpu_seconds` meter) + `runner_hash_chain_head`
-- (per-region tamper-chain resume coordinate). This migration lands that
-- aggregation target — and NOTHING else. It bills NOTHING by itself: it is the
-- storage foundation the counter aggregator (WI-S10-002 crate, production wiring
-- deferred to this WI) UPSERTs into and the Stripe submitter later reads.
--
-- ## Why a DEDICATED runner lane (NOT the cache `usage_counter`)
--
-- The cache counter table `usage_counter` (WI-S10-002) was frozen BEFORE the
-- runner-overage meter existed (the `RunnerVcpuSeconds` kind was added
-- 2026-08-02 — `crates/corelink-billing-emit/src/event.rs`), and its
-- `CHECK (sku IN ('cas_storage_gb_month', 'cas_egress_gb', 'cas_put_op_count',
-- 'cas_get_op_count', 'ac_lookup_op_count'))` enumerates ONLY the 5 CACHE SKUs.
-- A `runner_vcpu_seconds` row cannot satisfy that CHECK, so runner compute has
-- no home in the cache counter. Runner billing is also a DISTINCT product
-- surface: its own Stripe subscription (`runner_billing`, migration 0087, keyed
-- by `runner_subscription_id`), its own entitlement (`runners_entitlement
-- .max_vcpu_h`, migration 0070), its own unit (vCPU-seconds → vCPU-hours), and
-- it is NOT part of the cache reconciliation Layer-1 invariant (Σ R2 cache events
-- = Σ cache counter, replayed from R2 hour buckets). Folding runner usage into
-- the cache counter would conflate two subscriptions in one hash chain and widen
-- the proven cache CHECK/reconcile surface. A separate lane keeps the cache
-- table UNTOUCHED and isolates the runner tamper-chain per product. (Owner
-- decision 2026-08-12, Option A.)
--
-- The raw runner meter already stages cleanly today: `usage_event_staging`
-- (migration 0017) has no `sku` column and is kind-agnostic, and the spawn-Worker
-- already pushes `runner_vcpu_seconds` into it (gated on `BILLING_INGEST_URL`).
-- What is missing is the aggregate the Stripe submitter reads — this table.
--
-- ## Why period-level (NOT hourly like the cache counter)
--
-- The cache `usage_counter` partitions by `hour` because its reconciliation
-- replays R2 hour buckets. Runner overage is a MONTHLY charge measured against
-- the tier's included `max_vcpu_h` allowance, so the natural grain is the
-- `billing_period` (`YYYY-MM`, the same period the aggregator's `CounterGroupKey`
-- already carries). Region is retained as a partition component for per-region
-- forensics and because the aggregator Durable Object runs per-region
-- (`BillingAggregatorCron-<region>`); the collapse to a single per-tenant
-- quantity for Stripe submission happens at the submit step, not here.
--
-- ## Columns — `runner_usage_counter`
--
--   tenant_id       TEXT NOT NULL     — CoreLink tenant UUID that ran the compute.
--   region          TEXT NOT NULL     — 3-char region code (iad/fra/nrt/syd/gru,
--                                       matching the `usage_event_staging` region
--                                       shape, migration 0017 `length(region)=3`);
--                                       PK component so per-region aggregates never
--                                       overwrite each other.
--   billing_period  TEXT NOT NULL     — `YYYY-MM` (7 chars); the monthly bucket.
--   vcpu_seconds    INTEGER NOT NULL  — SUM of the `runner_vcpu_seconds` meter qty
--                                       in this (tenant, region, period). vCPU-hours
--                                       = vcpu_seconds / 3600 at the Stripe surface.
--   event_count     INTEGER NOT NULL  — number of staged records folded in (forensics).
--   aggregated_at   INTEGER NOT NULL  — unix epoch MILLISECONDS the DO wrote the row
--                                       (matches the cache counter's `aggregated_at`
--                                       ms convention).
--   prev_hash       TEXT NOT NULL     — BLAKE3-256 hex (64 chars): the tamper-chain
--                                       link slot (previous head).
--   own_digest      TEXT NOT NULL     — BLAKE3-256 hex (64 chars): this row's digest,
--                                       becomes the next `prev_hash`.
--   schema_version  TEXT NOT NULL     — payload schema version (default '1.0.0').
--
-- ## Columns — `runner_hash_chain_head`
--
--   region                  TEXT NOT NULL — 3-char region code.
--   chain_kind              TEXT NOT NULL — currently only 'runner_vcpu' (column kept
--                                           for parity with the cache
--                                           `hash_chain_head(region, chain_kind)` and
--                                           to leave room for a future late lane
--                                           without a schema change).
--   current_head            TEXT NOT NULL — BLAKE3-256 hex (64): the `prev_hash` slot
--                                           of the next aggregate to append.
--   last_aggregated_period  TEXT          — `YYYY-MM` of the last appended aggregate
--                                           (NULL = genesis, nothing appended yet).
--   updated_at              INTEGER NOT NULL — unix epoch ms of the last head advance.
--
-- ## Idempotency / additive-only / replay posture
--
-- `CREATE TABLE IF NOT EXISTS` + `CREATE INDEX IF NOT EXISTS` let the migration
-- replay safely. It is additive-only (INV-AUTH-MIGRATION-ADDITIVE): it adds NEW
-- tables and NEW indexes and never alters/drops/renames an existing object, so it
-- needs no ADR waiver and no `-- additive-allowed:` suppression. No FK to
-- `tenant`/`runner_billing` is declared (D1 FKs are logical/per-connection
-- inconsistent in CF Workers — see 0002/0003 — matching the sibling billing
-- tables 0070/0087). The `(tenant_id, region, billing_period)` PRIMARY KEY makes
-- a re-aggregation of the same period an idempotent UPSERT target (the cron's
-- watermark-based replay overwrites the row with the recomputed aggregate; the
-- caller deduplicates upstream via the chain-head lookup).

CREATE TABLE IF NOT EXISTS runner_usage_counter (
    tenant_id       TEXT    NOT NULL,
    region          TEXT    NOT NULL,   -- 3-char code (iad/fra/nrt/syd/gru)
    billing_period  TEXT    NOT NULL,   -- 'YYYY-MM'
    vcpu_seconds    INTEGER NOT NULL DEFAULT 0,
    event_count     INTEGER NOT NULL DEFAULT 0,
    aggregated_at   INTEGER NOT NULL,   -- unix epoch MILLISECONDS
    prev_hash       TEXT    NOT NULL,   -- BLAKE3-256 hex (64 chars)
    own_digest      TEXT    NOT NULL,   -- BLAKE3-256 hex (64 chars)
    schema_version  TEXT    NOT NULL DEFAULT '1.0.0',
    PRIMARY KEY (tenant_id, region, billing_period),
    CHECK (length(region) = 3),
    CHECK (length(billing_period) = 7),
    CHECK (vcpu_seconds >= 0),
    CHECK (event_count >= 0),
    CHECK (length(prev_hash) = 64),
    CHECK (length(own_digest) = 64)
);

CREATE INDEX IF NOT EXISTS idx_runner_usage_counter_tenant_period
    ON runner_usage_counter(tenant_id, billing_period);
CREATE INDEX IF NOT EXISTS idx_runner_usage_counter_period
    ON runner_usage_counter(billing_period);

CREATE TABLE IF NOT EXISTS runner_hash_chain_head (
    region                  TEXT    NOT NULL,
    chain_kind              TEXT    NOT NULL DEFAULT 'runner_vcpu',
    current_head            TEXT    NOT NULL,   -- BLAKE3-256 hex (64 chars)
    last_aggregated_period  TEXT,               -- 'YYYY-MM'; NULL = genesis
    updated_at              INTEGER NOT NULL,   -- unix epoch MILLISECONDS
    PRIMARY KEY (region, chain_kind),
    CHECK (length(region) = 3),
    CHECK (chain_kind IN ('runner_vcpu')),
    CHECK (length(current_head) = 64),
    CHECK (last_aggregated_period IS NULL OR length(last_aggregated_period) = 7)
);
