---
type: "Runbook"
title: "Secrets lifecycle & PAT-scope runbook"
description: "How CoreLink manages secrets and PAT scopes operationally: the write-only Cloudflare secret constraint, the additive PAT-scope launch fix (pat.scope CHECK vs the cas:rw provisioning bug), the printf-not-echo secret-put discipline, and the deferred D1-encrypted-lease secrets broker (ADR-0067)."
source_files:
  - "crates/corelink-container/src/scope.rs"
  - "docs/operator/launch-pat-scope-fix-runbook.md"
  - "specs/03_architecture/adrs/ADR-0067-secrets-broker-d1-encrypted-lease-deferred.md"
checkpoint_sha: "c100df62c1ce7d50185f5102ce1185da0a9fe9f9"
provenance: "AUTHORED"
tags: ["ops", "secrets", "pat-scope", "broker", "runbook"]
timestamp: "2026-06-26T00:00:00Z"
---

# Secrets lifecycle & PAT-scope runbook

CoreLink's secret management lives under one hard platform constraint: **Cloudflare Worker secrets are
write-only** — set via `wrangler secret put`, never read back by code or API — which shapes both the
operator's day-to-day secret discipline and the architecture of the (deferred) secrets broker. This
runbook captures the two operational truths an operator must hold: (1) the additive PAT-scope launch fix,
where the D1 `pat.scope` CHECK constraint and the provisioning path had drifted (`cas:rw` vs the allowed
`read-write`/`read-only`/`admin`), which would have failed the first real self-serve signup; and (2) the
forward design of the eventual secrets broker as a D1-encrypted lease (ADR-0067), the only shape that
works within the write-only-secret wall. The PAT-scope authorization itself is enforced through the
[D1 PAT store](/auth/d1-pat-store.md); the broker decision is recorded in
[ADR-0067](/adr/adr-0067-secrets-broker-d1-encrypted-lease-deferred.md).

# Role
- The operator's secret-handling discipline under the write-only-CF constraint.
- The record of the PAT-scope launch-blocker fix and why it was code-side, not schema-side.
- The forward design for the deferred secrets broker (D1-encrypted lease), recorded so the build is later.

# How it works
1. The D1 `pat.scope` column is CHECK-constrained to `('read-write','read-only','admin')` (migration
   0037), but the provisioning path was writing `scope='cas:rw'` — a CHECK violation that would fail the
   first real signup's PAT INSERT (`docs/operator/launch-pat-scope-fix-runbook.md:8-27`).
2. Because auth migrations are additive-only (CI gate `check_migrations_additive.py`), the fix is
   code-side: provisioning now writes `read-write` and `scope.rs` additively accepts `read-write`/
   `read-only` alongside the legacy `cas:*` forms (`docs/operator/launch-pat-scope-fix-runbook.md:14-27`).
3. Every auth surface routes through `scope.rs`'s `requires_cache_read`/`requires_cache_write`, so the
   single additive change covers the Worker, OCI downscope, and all five adapters — both fns accept the
   `read-write`/`read-only` forms alongside the legacy `cas:*` tokens and fail-CLOSED on empty/unknown
   (`crates/corelink-container/src/scope.rs:73`, `crates/corelink-container/src/scope.rs:93`).
4. The fix ships with no DB migration: deploying the container + signup-worker is sufficient and a real
   signup then persists a CHECK-valid `read-write` (`docs/operator/launch-pat-scope-fix-runbook.md:35-40`).
5. Secrets are set with `printf '%s' "$V" | wrangler secret put` (the runbook's pinning example), not
   `echo` — the cited runbook demonstrates the `printf` discipline
   (`docs/operator/launch-pat-scope-fix-runbook.md:51-56`).
6. The eventual secrets broker is deferred for launch and designed as a D1-encrypted lease: secrets live
   envelope-encrypted in D1 under a CoreLink-owned KMS/root key, leased TTL-bounded to consumers
   (`specs/03_architecture/adrs/ADR-0067-secrets-broker-d1-encrypted-lease-deferred.md:32-40`).

# Invariants
- Auth migrations are additive-only: the `pat` table cannot be destructively rebuilt without an owner ADR,
  which is precisely why the scope fix was code-side (`docs/operator/launch-pat-scope-fix-runbook.md:14-19`).
- Secrets are set with `printf`, not `echo`, per the runbook's pinning discipline — a malformed secret
  surfaces later as a 401/403 that masquerades as a token mismatch (`docs/operator/launch-pat-scope-fix-runbook.md:51-56`).
- A broker over Cloudflare secrets directly is impossible (write-only) — the broker MUST own its own D1
  envelope + root key to read+lease its secrets (`specs/03_architecture/adrs/ADR-0067-secrets-broker-d1-encrypted-lease-deferred.md:24-30`).
- Secrets at rest in the broker design MUST be envelope-encrypted; plaintext-in-D1 was explicitly rejected
  (`specs/03_architecture/adrs/ADR-0067-secrets-broker-d1-encrypted-lease-deferred.md:51-56`).

# Gotchas
- The 25 legacy prod PATs were all `scope='admin'`; the optional least-privilege back-fill to `read-write`
  is owner-gated and safe either way because `admin` already grants cache rw (a superset)
  (`docs/operator/launch-pat-scope-fix-runbook.md:41-50`).
- The Clerk issuer secret is `CLERK_JWT_ISSUER` (read by `corelink-clerk`), NOT `CLERK_ISSUER_URL` as an
  earlier task called it — and unset is valid (it derives from the publishable key)
  (`docs/operator/launch-pat-scope-fix-runbook.md:51-56`).
- The broker is deferred, not designed-away: launch ships on the interim operator-managed flat-file/env
  PAT model, and the lease+TTL design is recorded so the later build is mechanical
  (`specs/03_architecture/adrs/ADR-0067-secrets-broker-d1-encrypted-lease-deferred.md:57-63`).

# Citations
1. `docs/operator/launch-pat-scope-fix-runbook.md:8-27` — the `pat.scope` CHECK vs `cas:rw` launch-blocker + fix.
2. `docs/operator/launch-pat-scope-fix-runbook.md:14-19` — additive-only auth migration constraint (code-side fix).
3. `docs/operator/launch-pat-scope-fix-runbook.md:20-27` — `scope.rs` single chokepoint covers every auth surface (runbook).
3b. `crates/corelink-container/src/scope.rs:73`, `crates/corelink-container/src/scope.rs:93` — `requires_cache_read`/`requires_cache_write` accept `read-write`/`read-only`/`cas:*` and fail-closed (the enforcer).
4. `docs/operator/launch-pat-scope-fix-runbook.md:35-40` — deploy-only, no DB migration.
5. `docs/operator/launch-pat-scope-fix-runbook.md:41-50` — owner-gated legacy-admin least-privilege back-fill.
6. `docs/operator/launch-pat-scope-fix-runbook.md:51-56` — `printf`-not-`echo` secret put + `CLERK_JWT_ISSUER` name.
7. `specs/03_architecture/adrs/ADR-0067-secrets-broker-d1-encrypted-lease-deferred.md:24-30` — write-only CF secrets constraint.
8. `specs/03_architecture/adrs/ADR-0067-secrets-broker-d1-encrypted-lease-deferred.md:32-40` — D1-encrypted-lease broker design.
9. `specs/03_architecture/adrs/ADR-0067-secrets-broker-d1-encrypted-lease-deferred.md:51-56` — plaintext-in-D1 + CF-direct broker rejected.
10. `specs/03_architecture/adrs/ADR-0067-secrets-broker-d1-encrypted-lease-deferred.md:57-63` — deferred to interim flat-file/env model.
