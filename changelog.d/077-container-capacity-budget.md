### Fixed

- **B-077: cache-plane concurrency now follows the deployed container.** The
  basic 1 GiB / 0.25 vCPU capacity is defined once and shared by CAS,
  Argon2id, Turbo, and the bounded 2048-tenant Bloom cache. CAS reads reserve
  the SDK/handler/BYOK three-copy peak; single and batch writes reserve their
  complete pre-storage peaks before body buffering. Compile-time and startup
  checks reject an over-capacity declaration; reservations and existing tenant
  guards fail closed before buffering, preserving tenant isolation and fairness.
