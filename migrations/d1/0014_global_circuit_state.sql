-- CoreLink D1 (Cloudflare SQLite) — migration 0014 for the global
-- rate-limit circuit-breaker state + per-region trip history (S-08
-- camada-0 of the 4-layer rate-limit bulkhead PAT-RATE-LIMIT-001;
-- WI-S08-005 GlobalCircuitBreaker + InMemoryGlobalCircuitBreaker +
-- RateLimitHeaderBuilder).
--
-- Canonical sources:
--   - specs/04_sprints/S08/work_items/WI-S08-005-rfc9331-headers-global-circuit-breaker.md §6.1.10
--   - specs/04_sprints/S08/_spec_contract.md §4 CAP-RATE-004 + §5 R-S08-4 + §5 R-S08-8 + §5 R-S08-9
--   - specs/03_architecture/invariant_registry.md INV-AVAIL-ISOLATION + INV-AUDIT-APPEND-ONLY + INV-TENANT-ISOLATION
--   - specs/03_architecture/adrs/ADR-0036-d1-schema-migration-governance.md
--
-- Schema rationale:
--   - Per WI §6.1.10, two NEW tables back the camada-0 global
--     circuit-breaker plane. This migration ships both atomically;
--     companion observability widgets (DASH-RATE) ride along WI-S08-006
--     PRR ship gate.
--   - `global_circuit_state` is the durable per-region state mirror of
--     the `GlobalRateLimiter-<region>` DO singleton in-memory state
--     machine; cold-start recovery reads this row and re-arms the alarm
--     loop AT START (Lote 10.4bis lesson).
--   - `global_circuit_trips_history` is the durable per-region append-only
--     trip ring — every Open transition writes a row + every Closed
--     recovery patches `recovered_at`; production wiring at WI-S08-006
--     fans the ring into DASH-RATE for SEV-1 forensics + post-mortem.
--   - 3-state canonical lifecycle per WI §1 invariant 6 + sprint contract
--     §5 R-S08-4: `closed` (normal; all requests pass) → `open`
--     (emergency; all requests 429 GlobalCircuitOpen) → `half_open`
--     (hysteresis; sample 10% requests).
--
-- Invariants enforced at storage layer:
--   - INV-AVAIL-ISOLATION (HIGH; invariant_registry §3.8): the camada-0
--     circuit is the LAST-RESORT defense; per-region scope so a single
--     region's trip cannot cascade to the others (cross-region federation
--     deferred S-14 per sprint contract §10 anti-scope).
--   - INV-AUDIT-APPEND-ONLY (CRITICAL, TLA+): every state transition emits
--     a `corelink.circuit.{tripped,half_open_probe,closed_recovery,
--     request_rejected,manual_override}` audit_outbox row in the same D1
--     batch; fail-closed envelope enforced at the
--     InMemoryGlobalCircuitBreaker layer (audit emit BEFORE state
--     mutation; audit failure aborts the transition; production wiring
--     rolls back the D1 batch on emit failure per Lote 10.6bis pattern +
--     S-07 sprint-close P1-1 fix).
--   - INV-TENANT-ISOLATION (CRITICAL, TLA+): the circuit state itself is
--     tenant-agnostic (system-wide; per-region) but the RFC 9331 headers
--     it composes ARE per-tenant scoped — header emission never carries
--     cross-tenant identifiers (sprint contract §7.10.s08.4 explicit).
--
-- Conventions (mirror migrations/d1/0001..0013):
--   - All timestamps stored as INTEGER Unix epoch milliseconds.
--   - Migration is idempotent via `CREATE TABLE IF NOT EXISTS` /
--     `CREATE INDEX IF NOT EXISTS`.
--   - Migrations are additive-only per scripts/check_migrations_additive.py
--     CI gate.
--   - region stored as canonical CF colocode TEXT (e.g. "iad", "sam",
--     "fra"); see CTRL-RATE-001 for the canonical region list.
--
-- D1 SQL correctness gates (Lote 10.4bis P0 lessons; pre-deploy CI):
--   - CHECK constraints inlined in CREATE TABLE (SQLite/D1 does NOT support
--     `ALTER TABLE … ADD CONSTRAINT chk_*`; only inline at CREATE TABLE per
--     ADR-0036 Rule 1).
--   - BEGIN/COMMIT NOT included (`wrangler d1 migrations apply` uses an
--     implicit transaction).
--
-- Backfill plan: NONE. The tables start empty; rows are inserted by the
-- GlobalCircuitBreaker DO singleton on every state transition (cron-driven
-- for state snapshots + event-driven for trips_history).
--
-- Migration runner: see scripts/migrate_d1.sh.

-- ---------------------------------------------------------------------------
-- global_circuit_state — durable per-region 3-state mirror (closed /
-- open / half_open) of the `GlobalRateLimiter-<region>` DO singleton
-- in-memory state machine. Cold-start recovery reads this row + re-arms
-- alarm AT START.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS global_circuit_state (
  -- Region scope (canonical CF colocode TEXT). Single-row-per-region
  -- mirror; PK is the region itself.
  region                      TEXT     NOT NULL PRIMARY KEY,

  -- Canonical 3-state lifecycle: `closed` (normal; all requests pass) /
  -- `open` (emergency; all 429 GlobalCircuitOpen) / `half_open`
  -- (hysteresis; sample 10% requests).
  state                       TEXT     NOT NULL,

  -- Wall-clock instant of the latest state transition (Unix ms;
  -- canonical no `_ms` suffix per Lote 10.7bis P0-3 column-drift
  -- lesson). Used by hysteresis to enforce the 2min recovery dwell.
  state_since                 INTEGER  NOT NULL,

  -- Latest trip reason, JSON-encoded `TripReason` per WI §6.1.6
  -- (NULL when state is `closed` and no prior trip). The TripReason
  -- enum is `#[non_exhaustive]` so the storage shape is loose-text.
  last_trip_reason            TEXT,

  -- Wall-clock instant of the latest snapshot write (Unix ms; canonical
  -- no `_ms` suffix). Used by the cold-start recovery path to detect
  -- stale snapshots and SEV-3 alert on snapshot drift > 60s.
  snapshotted_at              INTEGER  NOT NULL,

  -- chk_global_circuit_state_canonical: closed canonical 3-literal list
  -- per `CircuitState` `#[non_exhaustive]` enum. Production wiring at
  -- WI-S08-006 may add literals additively (e.g. forensic-only states)
  -- via a follow-on additive migration.
  CHECK (state IN ('closed', 'open', 'half_open')),

  -- chk_global_circuit_state_since_non_negative.
  CHECK (state_since >= 0),

  -- chk_global_circuit_snapshotted_at_non_negative.
  CHECK (snapshotted_at >= 0),

  -- chk_global_circuit_region_non_empty.
  CHECK (length(region) >= 1)
);

-- ---------------------------------------------------------------------------
-- global_circuit_trips_history — durable per-region append-only trip
-- ring; every Open transition writes a row + every Closed recovery
-- patches `recovered_at`. Composite PK (region, tripped_at)
-- region-leftmost so per-region forensic scans are index-only.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS global_circuit_trips_history (
  -- Region scope. Composite PK with `tripped_at`; region-leftmost so
  -- per-region forensic scans (CAP-RATE-004 SEV-1 dashboards) are
  -- index-only.
  region                      TEXT     NOT NULL,

  -- Wall-clock instant the trip fired (Unix ms; canonical no `_ms`
  -- suffix). Composite PK with `region`.
  tripped_at                  INTEGER  NOT NULL,

  -- Wall-clock instant the recovery completed (Unix ms; canonical no
  -- `_ms` suffix). NULL while the circuit remains Open / HalfOpen;
  -- patched in-place by the recovery transition.
  recovered_at                INTEGER,

  -- Canonical TripReason discriminator (`Error5xxRateExceeded` /
  -- `LatencyP99Exceeded` / `DoErrorRateExceeded` / `MultiSignalCombined`
  -- / `ManualOverride`). The TripReason enum is `#[non_exhaustive]`.
  trip_reason                 TEXT     NOT NULL,

  -- Snapshot of the multi-signal readings AT THE INSTANT of the trip
  -- (NULL only for ManualOverride trips where signals are not the
  -- triggering input).
  signal_5xx_rate             REAL,
  signal_p99_us               INTEGER,
  signal_do_error_rate        REAL,

  -- Manual-override attribution (NULL unless trip_reason is
  -- `ManualOverride`). Production wiring populates from `AdminCtx`
  -- extracted via S-13.
  manual_override_admin_id    TEXT,
  manual_override_reason      TEXT,

  PRIMARY KEY (region, tripped_at),

  -- chk_global_trips_recovered_after_tripped: recovery cannot precede
  -- the trip (NULL allowed for ongoing-Open rows).
  CHECK (recovered_at IS NULL OR recovered_at >= tripped_at),

  -- chk_global_trips_5xx_rate_envelope: rate is a probability [0.0, 1.0].
  CHECK (signal_5xx_rate IS NULL OR (signal_5xx_rate >= 0.0 AND signal_5xx_rate <= 1.0)),

  -- chk_global_trips_do_error_rate_envelope.
  CHECK (signal_do_error_rate IS NULL OR (signal_do_error_rate >= 0.0 AND signal_do_error_rate <= 1.0)),

  -- chk_global_trips_p99_non_negative (microseconds; latency).
  CHECK (signal_p99_us IS NULL OR signal_p99_us >= 0),

  -- chk_global_trips_tripped_at_non_negative.
  CHECK (tripped_at >= 0),

  -- chk_global_trips_region_non_empty.
  CHECK (length(region) >= 1),

  -- chk_global_trips_reason_non_empty.
  CHECK (length(trip_reason) >= 1),

  -- chk_global_trips_manual_override_consistent: when reason is
  -- ManualOverride the admin_id MUST be present (LGPD trail compliance).
  CHECK (
    trip_reason != 'ManualOverride'
    OR (manual_override_admin_id IS NOT NULL
        AND length(manual_override_admin_id) >= 1)
  )
);

-- Index: per-region recent-trip scan — admin forensic surface +
-- DASH-RATE widget (WI-S08-006); ordered DESC for the canonical
-- "latest-N-trips" query.
CREATE INDEX IF NOT EXISTS idx_global_trips_recent
  ON global_circuit_trips_history(region, tripped_at DESC);

-- Index: ongoing-Open scan — every dashboard scan filters on
-- `recovered_at IS NULL` for the SEV-1 oncall alarm (CAP-RATE-004
-- alert: `corelink.global_circuit.trips_total > 0`).
CREATE INDEX IF NOT EXISTS idx_global_trips_ongoing
  ON global_circuit_trips_history(region, tripped_at)
  WHERE recovered_at IS NULL;

-- Index: manual-override surface — LGPD audit trail for admin S-13
-- planned drills + emergency overrides; SEV-2 alert source.
CREATE INDEX IF NOT EXISTS idx_global_trips_manual_override
  ON global_circuit_trips_history(region, tripped_at)
  WHERE trip_reason = 'ManualOverride';
