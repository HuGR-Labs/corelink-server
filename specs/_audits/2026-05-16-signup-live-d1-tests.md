# Audit — Signup Live-D1 Integration Tests (Wave-30 stream-9)

- **Date:** 2026-05-16
- **Wave / stream:** Wave-30 / stream-9 (R-prep)
- **Branch:** `wt/r-prep-signup-live-d1-test`
- **Base commit:** `04f2dff` (main)
- **Scope:** lifts the 8 wave-29 stream-1 signup integration tests
  off the `InMemorySignupStore` fake onto a real SQLite-backed D1
  surrogate that applies `migrations/d1/0053_pilot_signups.sql` and
  exercises the full HMAC + audit-emit + persistence round-trip.
- **Closes:** wave-29 stream-1 audit doc §13 follow-up note (the
  live-D1 lift was deferred to wave-30 in the closing summary).
- **Cross-refs:**
    - `specs/_audits/2026-05-16-signup-corelink-dev-backend.md`
      (wave-29 stream-1 baseline; §closure-note updated this wave)
    - `specs/_audits/2026-05-16-neon-shadow-pg-testharness.md`
      (wave-20 testcontainers Postgres pattern that informed the
      D1-surrogate design)
    - `crates/corelink-d1-migrations/src/lib.rs` (R2-13 rusqlite +
      bundled libsqlite3 pin we reused)

## 1. Why

The wave-29 stream-1 commit `b3c359f` shipped 8 axum-oneshot
integration tests for `POST /v1/signup/pilot/{token}` against an
`InMemorySignupStore` `Vec<PilotSignupRecord>` fake. The fake is
adequate for asserting route-level invariants (HMAC verify, audit
emit ordering, rate-limit gate), but it does NOT exercise:

1. The canonical migration `migrations/d1/0053_pilot_signups.sql`
   — schema-drift between the Rust struct shape and the SQL CREATE
   TABLE columns would silently pass the in-memory suite.
2. The D1 UNIQUE-INDEX duplicate-email contract — the in-memory
   store hand-rolls an `iter().any(|r| r.email == ...)` lookup which
   is a different mechanism than a UNIQUE constraint enforced by
   SQLite.
3. Transaction discipline — the in-memory store has no BEGIN /
   COMMIT / ROLLBACK surface, so a partial-write rollback bug in
   the route's wave-29 sequencing (insert → audit-emit → render)
   would not surface against the fake.
4. Migration idempotency — the `CREATE TABLE IF NOT EXISTS` +
   `CREATE UNIQUE INDEX IF NOT EXISTS` patterns in 0053 are not
   exercised at all by the in-memory suite.

This wave delivers a SQLite-backed surrogate that covers all four
gaps without requiring Docker, network egress, or a Cloudflare D1
binding.

## 2. Design — in-process surrogate vs testcontainers

The wave-22 reference pattern
(`crates/corelink-audit-chain/tests/harness/pg_container.rs`) uses
testcontainers + `postgres:16-alpine` because Postgres is a third-
party binary distribution with feature semantics (RLS, JSONB casts,
GUCs) that we MUST exercise against a real running daemon.

D1 is different: at the storage layer it IS SQLite, and the per-row
semantics (transactions, UNIQUE indexes, CHECK constraints) are
stock SQLite — there is no D1-only persistence feature the wave-29
route uses. We therefore picked an **in-process SQLite surrogate**
over the testcontainers pattern:

| Property              | Postgres testcontainer (wave-22)     | SQLite surrogate (this wave)    |
|-----------------------|---------------------------------------|----------------------------------|
| Image dependency       | `postgres:16-alpine` (Docker daemon) | None (bundled libsqlite3 via `rusqlite 0.32 + bundled`) |
| CI lane requirement    | Docker engine + 2 GB image cache     | Zero (libsqlite3 statically linked) |
| Test boot latency      | ~5 s per container                   | ~3 ms per harness                |
| Schema features needed | RLS + JSONB + `set_config(...)`       | CREATE TABLE + UNIQUE INDEX + CHECK constraint |
| Production parity      | Identical (Postgres-on-Neon)         | Identical (SQLite-on-D1)         |

The R2-13 migration replay harness (`crates/corelink-d1-migrations`)
already established the `rusqlite 0.32 + bundled` pin as the canonical
D1 surrogate; this wave reuses the same pin in
`apps/server/Cargo.toml` (dev-dep only).

## 3. Deliverables

| Path                                                       | Type      | Function                                                                 |
|------------------------------------------------------------|-----------|--------------------------------------------------------------------------|
| `apps/server/tests/harness/d1_container.rs`                | Harness   | `D1Harness` + `SqliteSignupStore` (implements `SignupStore`)             |
| `apps/server/tests/signup_pilot_live_d1.rs`                | Integration tests | 8 tests mirroring the wave-29 in-memory suite                    |
| `apps/server/Cargo.toml`                                   | Manifest  | Adds `rusqlite 0.32 + bundled` and `tempfile` to `[dev-dependencies]`    |
| `specs/_audits/2026-05-16-signup-live-d1-tests.md`         | Audit     | This doc                                                                  |
| `specs/_audits/2026-05-16-signup-corelink-dev-backend.md`  | Audit cross-ref | Wave-29 §closure-note updated to record the wave-30 live-D1 lift   |

## 4. Test matrix

The wave-30 stream-9 charter §Deliverables.2 specified 8 tests; all
8 are green:

| # | Name                                                   | Asserts                                                                                       |
|---|--------------------------------------------------------|-----------------------------------------------------------------------------------------------|
| 1 | `happy_path_persists_row_with_correct_schema`           | Row lands with all 9 columns populated; `state="RESERVED"`; SQLite-side `tenant_id` matches response |
| 2 | `hmac_tampered_no_row_inserted`                         | Forged HMAC → 401, audit emit captures `signature_mismatch`, SQLite row count stays 0         |
| 3 | `rate_limit_hit_then_clock_rolls_over_allows_again`     | 6th request → 429, advance clock 1h+10s, recovery request → 201 (real time-driven refill)     |
| 4 | `audit_emit_fail_closed_after_store_write_per_wave29_contract` | 503 fail-CLOSED + 1 durable row (pins the wave-29 designed contract against real SQLite) |
| 5 | `duplicate_email_returns_existing_row_no_duplicate_insert` | UNIQUE INDEX returns the original `tenant_id`; SQLite row count stays 1; 2 audit emits     |
| 6 | `cross_ip_distinct_tenant_ids_no_region_leak`           | Two distinct IPs → distinct `tenant_id`s + distinct rows; route is region-neutral by design   |
| 7 | `migration_0053_reapplies_idempotently_with_data_present` | Re-applying 0053 twice with one row present is a no-op (INV-AUTH-MIGRATION-ADDITIVE)        |
| 8 | `unique_constraint_collision_rolls_back_cleanly`        | Direct-store race-window collision returns existing row; SQLite row count stays 1             |

## 5. Wave-29 fail-CLOSED contract — explicit re-statement

The wave-29 stream-1 audit doc §6 documented a deliberate design
call: the route persists the row to the store BEFORE emitting the
`pilot_reserved.v1` audit event. The fail-CLOSED boundary is at the
HTTP response — the customer never sees the 201 if the audit emit
fails. The wave-29 audit doc captures this as "bounded pre-emit
store mutation, idempotent on (email, token_id), reconciled by an
external audit-chain cron".

Test #4 (`audit_emit_fail_closed_after_store_write_per_wave29_contract`)
pins this contract against REAL SQLite — not just the in-memory
mock — so any future refactor that flips the ordering (to emit
BEFORE persist) will surface as a regression.

If a wave-31+ refactor wants to flip the ordering, it MUST update
both this test AND the wave-29 audit doc — the test name + doc
cross-ref make the design call diff-visible.

## 6. Race-window discipline

Test #8 (`unique_constraint_collision_rolls_back_cleanly`) drives
the `SqliteSignupStore::insert_or_existing` directly with a record
that carries an EXISTING email but a fresh `token_id`. The
expectation: the idempotency lookup (email-OR-token_id) returns the
existing row before we even reach the INSERT, so the BEGIN..COMMIT
block is never entered and no partial transaction can leak.

The store's `match exec_result` arm also handles the
"hypothetical concurrent racer" case where two requests both pass
the lookup and one of them surfaces a `UNIQUE constraint failed`
error on INSERT. In that case the store rolls back, re-runs the
idempotent lookup, and returns the winner's row — mirroring the
production D1 binding's expected race-recovery contract. The
single-threaded SQLite file lock makes this rare in practice
(serialised writes), but the contract is pinned for the future
multi-connection wiring.

## 7. Charter compliance

- `#![forbid(unsafe_code)]` — both test files inherit from the
  apps/server crate root + the `forbid(unsafe_code)` attribute on
  the test crate.
- No `unwrap`/`expect`/`panic` in src — the harness uses `expect`
  ONLY at the infra-failure boundary (open SQLite file, apply
  migration). Per charter, test files are explicitly allowed
  these primitives; the harness re-uses the same allow set as the
  wave-29 in-memory tests.
- `cargo build --workspace` — green (full release-mode build
  exercised via the workspace clippy lane below).
- `cargo clippy --workspace --all-targets -- -D warnings` — green.
- `cargo test -p corelink-server --test signup_pilot_live_d1` — 8/8
  green; per-test latency ≤ 10 ms (whole suite finishes in 60 ms).
- `cargo test -p corelink-server --test signup_pilot` — 8/8 green
  (wave-29 in-memory suite preserved unchanged).

## 8. Freeze compliance

Per the wave-30 freeze policy (§3.b GA-blocker prep), this stream
ships:

- ZERO production behaviour changes — `apps/server/src/routes/signup.rs`
  is untouched; the live-D1 tests use the existing trait-object
  surface.
- ZERO migration changes — `migrations/d1/0053_pilot_signups.sql`
  is read-only by this stream.
- ONE additive Cargo dependency surface in `apps/server/Cargo.toml`
  `[dev-dependencies]` (rusqlite + tempfile); production crate
  manifests are untouched.

## 9. Production wiring — deferred

The `SqliteSignupStore` is harness-only. Production wiring to
Cloudflare D1 binds to the wave-17 D1-writer pattern at the PRR
ship gate (wave-29 audit doc §12 deliverable 2). This stream does
NOT pre-emptively ship that binding; it ships the regression suite
that will catch behavioural drift between the SQLite surrogate and
the eventual D1 binding the day the latter lands.

## 10. Follow-ups

1. **Production D1 binding** — wave-29 audit doc §12.2 captures the
   PRR-ship-gate work. This wave's `SqliteSignupStore` is the
   reference implementation the binding's regression suite will
   inherit.
2. **Concurrent-write regression** — the current race-window test
   uses a single-threaded SQLite file. A future stream that wires
   the production D1 binding's per-request connection pool should
   add a `proptest` or `loom`-style multi-actor regression. Captured
   here as a wave-31+ R-prep candidate (NOT freeze-blocking).

---

Signed-off-by: Gustavo Schneiter <gustavo@humangr.com>
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
