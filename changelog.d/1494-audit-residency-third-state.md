### Fixed

- Audit-residency verification now checks the full `audit_outbox` population as
  `satisfied`, `violated`, or `unevaluable`; missing or malformed evidence,
  retained DSR-erasure orphans, and unexplained orphans can no longer be
  reported as a clean inner-join result (B-127).
