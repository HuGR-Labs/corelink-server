### Fixed

- **B-056 CAS reads now have a process-wide byte budget.** Single-object GETs and 8 MiB
  batch reads reserve weighted, FIFO semaphore permits before buffering; RAII release covers
  cancellation and handler errors, while saturation or semaphore failure returns 503 without
  logging tenant or hash identities.
