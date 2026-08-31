### Fixed

- **B-064 — the audit-chain drain cron read keys that do not exist and never
  looped, turning a per-CALL row budget into a per-HOUR platform ceiling.**
  `apps/signup-worker/src/webhooks/audit_drain_cron.ts` read `j.sealed` and
  `j.partitions` from the drain response; the Rust handler
  (`crates/corelink-container/src/routes/audit_drain.rs`) emits `rows_sealed`,
  `partitions_drained`, `partitions_failed` and `incomplete`. Every hourly run
  therefore logged `sealed=0 partitions=0` regardless of the work done — a
  sweep that sealed 200 rows and one that sealed none were byte-identical in
  the log. The handler's documented contract is that a budget-bounded sweep
  returns `incomplete: true` so the caller re-calls until it is false; the cron
  POSTed exactly once, so `AUDIT_DRAIN_BATCH_LIMIT` (unset in prod ⇒ 200)
  capped the entire platform at 200 sealed rows per hour.

  Measured against the prod `corelink-config-prod` D1 on 2026-08-31: ten
  consecutive hours sealed exactly 200 rows each, while arrivals averaged
  310/h over 30h and peaked at 3,141/h — a 15.7× deficit against peak. The
  unsealed backlog grew from 4,243 to 5,776 rows over the measurement window,
  and mean seal latency reached 1h25 (max 5h13, n=2,932). Seal latency is the
  window in which the tamper-evident chain does not yet cover a row.

  The sweep now reads the handler's real keys and re-calls while `incomplete`
  is true, bounded twice over — `MAX_DRAIN_CALLS = 10` iterations and a
  `DRAIN_WALL_BUDGET_MS` 10-minute wall-clock budget — so an exhausted budget
  leaves the remainder for the next idempotent tick instead of stalling
  forever. A tick that still ends `incomplete` now emits its own WARN rather
  than hiding inside a routine success line. Nine unit tests were added
  (`apps/signup-worker/tests/audit_drain_cron.test.ts`), including regression
  teeth that go red if the old `j.sealed` key names are reintroduced; the file
  previously had no test at all.
