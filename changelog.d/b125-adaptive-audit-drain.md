### Fixed

- **B-125 — bound the audit-chain drain under bursts.** The real 200-row
  ceiling came from the container's `AUDIT_DRAIN_BATCH_LIMIT` default, because
  the production environment blocks did not set that forwarded tuning
  variable; the hourly signup-worker cron is the sole caller. All five
  production environments now declare a bounded 512-row call budget, while the
  cron retains its 10-call / 10-minute cap and stops when an incomplete response
  reports only lease backpressure or head drift.
- The container seals rows in ordered JSON1 UPDATE chunks of at most 32 rows,
  preserving the exact RFC-8785 bytes, `emitted_at IS NULL` idempotency guard,
  lease fence, and signed head CAS. A crash before the CAS still resumes from
  the durable sealed tail; no chain algorithm or verifier trust rule changed.

### Verification boundary

- This is a code/config repair with focal tests only. B-125 remains **open**:
  no production remeasurement was run or retained here, so no latency or
  throughput acceptance threshold is claimed closed.
