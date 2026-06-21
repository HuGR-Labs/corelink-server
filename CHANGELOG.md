# Changelog

All notable changes to CoreLink will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Sprint-tagged sections below mirror the 21-sprint spec-corpus → impl-sealed
trajectory (S-00 through S-20 + the `ga-engineering-gate-complete` cutover
on 2026-05-14). Each post-S-13 sprint receives its own dated section; the
S-00 → S-13 spec-corpus phase is collapsed under `[0.x]`.

Each entry cross-references:

- **WI-S__-___** — sprint work items (see `specs/04_sprints/S__/`)
- **CAP-_____** — capabilities (declared in `_spec_contract.md` §4)
- **R1-9!** / **R2-__** — security findings closed (see `ROADMAP-TO-GA.md`)
- **P0/P1** audit-doc IDs — sprint-close adversarial review findings
  (see `specs/04_sprints/S__/_audits/sprint-close-round-*.md`)

---

## [Unreleased]

### Fixed
- **cargo/sccache surface 502'd on every PUT in prod** — the cargo adapter passed the
  sccache key (`blake3(rustc-cmdline + input fingerprints)` — a hash of the compile
  INPUTS, not of the cached OUTPUT) straight through as the CAS `digest_hex`, but the
  CAS write verifies `claimed == blake3(content)` (`handler.rs::HashMismatch`), so every
  sccache store failed integrity → HTTP 502 (the surface never worked against the
  verifying CAS). Now routes cargo through the same 2-level `MoatCache` brew/npm/pip use
  (`put` content-addresses the bytes + records `(namespace,key)→content_hash`; `get`
  resolves the map then fetches), namespaced PER-TENANT (private; never `_public`). Gains
  content dedup for free. Found in the e2e gated-surface sweep (bazel REAPI v2 CAS+AC and
  the OCI registry validated working in the same sweep). Needs a container redeploy.
- **Homebrew bottle proxy was 502ing in prod** — the brew upstream fetcher did a
  plain unauthenticated GET to ghcr.io, which 401s ALL pulls (even public
  homebrew/core bottles) with a Bearer challenge → `Upstream` → HTTP 502, so
  Homebrew on CoreLink was non-functional. Now performs the anonymous OCI-token
  dance (parse `WWW-Authenticate`, fetch the realm token SSRF-guarded to the
  upstream host, retry once) AND sends the OCI/Docker manifest `Accept` header —
  ghcr.io returns **404** for a manifest GET that omits it, even with a valid
  token (verified live); AND strips the `/brew/<tenant>/` route prefix the Worker
  forwards verbatim (nest_service preserves the full path) — it was sent to ghcr as
  `ghcr.io/brew/<tenant>/v2/…` → 404. Those three fixed the upstream FETCH; the public
  bottle STORE then surfaced two more: (4) the shared `_public` dedup namespace had no
  `tenant_storage_state` row, so byte-accounting failed CLOSED (`storage cap
  indeterminate`) — seeded uncapped per region (migration 0073); (5) `_public` is not a
  UUID, so the R2 key prefix derivation fail-CLOSED'd it (INV-TENANT-ISOLATION) — now
  derived from a reserved sentinel UUID (`storage/r2_s3.rs`: stable, secret-keyed, never
  collides with a real tenant, consistent so the cross-tenant dedup actually dedups).
  Verified end-to-end LIVE (brew manifest → 200). Needs a container redeploy to take
  effect. Caught by
  the e2e user-journey suite smoke.
- **Admin-route audit hardening (REV-S1)** — two container admin-plane defects
  closed: (1) `admin_pilot::handle_create` and `handle_grant_tier` now take the
  request body as raw `Bytes` and JSON-parse it ONLY after the internal-auth gate
  passes (M3 pattern, matching `admin::handle_mutate`), so an unauthenticated
  caller can no longer force the container to deserialize an arbitrarily-large
  body pre-auth; (2) `admin::handle_read` and `handle_mutate` now stamp audit
  records with the real `SystemWallClock.now_ms()` instead of a hardcoded
  epoch-zero timestamp, restoring orderability of the admin forensic log
  (matches the CAS/AC/Bazel/Turbo audit path).
- **Turborepo remote cache was non-functional in prod** — the `corelink-turbo-prod`
  R2 bucket (the default `R2_TURBO_BUCKET`, backing every env) was never provisioned,
  so `GET/PUT /v8/artifacts/*` returned a generic 500 ("internal", a missing-bucket S3
  error) while `/status` still 200'd. Created the bucket (live fix, no redeploy — the
  turbo store uses the R2 S3 API, not a binding) and added it to
  `scripts/provision-cf-corelink-prod.sh` so it is provisioned reproducibly. Discovered
  by the new e2e user-journey suite (`tests/e2e-user-journeys`).

### Security
- **`owner_tenant` is now MANDATORY on `/internal/v1/runner/revoke` (REV-S2 closed)** — the
  revoke UPDATE always carries `AND tenant_id = owner_tenant`, and an absent/empty
  `owner_tenant` is a hard 400 (no UPDATE runs). This closes the backward-compat un-scoped
  window: a compromised `runner_mint` key can no longer revoke ANOTHER tenant's PAT by
  guessing a `pat_id`. Safe to flip now — the runners dispatcher's PR-B is deployed + proven
  to send `owner_tenant` on every revoke (green-lit by the Runners TL 2026-06-21, no lockstep).
- **Runner-mint internal-auth scoped to its own consumer key** — the runner
  dispatcher's `/internal/v1/runner/{mint,revoke}` now authenticate with a NEW
  `runner_mint` consumer (`CORELINK_RUNNER_MINT_AUTH_KEY`), distinct from signup's
  `pat_mint`. The untrusted runner-spawn-Worker thus gets a key that can ONLY
  mint/revoke per-job runner PATs — never the signup PAT-mint, erase, or admin
  surfaces (A6 least-privilege). ADDITIVE: falls back to the shared
  `CORELINK_INTERNAL_AUTH_KEY` until set, so no break before provisioning; the
  Worker→container mint authority still uses the shared key. Checklist row #161.
- **Internal PAT revoke/rotate are now tenant-scoped (REV-S2, blast-radius
  bound).** `POST /internal/v1/runner/revoke` and `POST /internal/v1/auth/rotate`
  previously keyed only on `pat_id`: a holder of the `pat_mint` internal-auth key
  could revoke ANY tenant's PAT (targeted DoS on a customer's primary API key) or
  rotate ANY tenant's PAT into a fresh working credential (cross-tenant privilege
  escalation). Both now REQUIRE an `owner_tenant` in the body. Revoke adds a
  `tenant_id = owner_tenant` predicate to the soft-revoke UPDATE; rotate validates
  `owner_tenant === oldRow.tenant_id` (403 on mismatch) before minting — bounding a
  compromised mint key to only the tenant it names. Files:
  `worker/src/lib/runner_mint.ts`, `worker/src/lib/auth_rotate.ts`.
- **DO container-start race (REV-S2).** Concurrent requests to a stopped Durable
  Object could each pass the `containerStatus === "stopped"` check and double-call
  `container.start()` (double cold-start telemetry + competing health polls).
  `startContainer` now flips the in-memory status to `"starting"` SYNCHRONOUSLY
  before its first `await`, so any concurrent re-entry falls into the
  `waitForContainerReady` branch. File: `worker/src/durable_object.ts`.
- **CAS read tombstone (GDPR-erasure) gate now fails CLOSED on D1 error**
  (`PEN-2`/`REV-S1`). On a CAS read, a transient D1 transport fault in the
  `is_tombstoned` lookup previously fell through to the normal R2 read path,
  which could resurrect (serve 200) a legally-erased artifact during a D1 blip.
  The gate now returns `503 Service Unavailable` on a lookup error instead of
  serving the bytes — confidentiality of GDPR/DSR-erased data outweighs
  availability of a live blob during an outage. File:
  `crates/corelink-container/src/routes/cas.rs` (single-read gate only).
- **Package-proxy upstream clients: SSRF redirect hardening + brew request
  timeout** (REV-O2, REV-S3). The npm/pip/brew adapter `reqwest` clients no
  longer follow redirects under reqwest's permissive default policy (up to 10
  hops to ANY host). Each client now installs a bounded (≤5 hop) custom redirect
  policy that REFUSES any hop whose `Location` resolves to an internal IP literal
  — loopback, RFC-1918/RFC-4193 private space, link-local (incl. the
  `169.254.169.254` cloud-metadata endpoint), CGNAT `100.64/10`, broadcast,
  documentation, and the unspecified address — closing the SSRF hole where a
  first-hop upstream could bounce the request at an internal/metadata target.
  Legitimate public-CDN redirects (ghcr→download CDN, pypi.org→
  files.pythonhosted.org) still follow. Separately, the brew upstream client
  gained the 30s total + 10s connect timeouts the npm/pip clients already had,
  so a slow/adversarial ghcr.io can no longer pin a Tokio task indefinitely.
  Files: `crates/corelink-adapter-host/src/{brew,npm,pip}/upstream.rs`.
- **cargo plane: quota gate no longer fails OPEN on a missing tenant label, and
  PUT no longer runs Argon2id twice** (`REV-S3`). (1) The `/cargo/*` per-tenant
  `$`-ceiling gate (ADR-0068) now attributes cost to the PAT-resolved tenant id
  for writes (authoritative, un-spoofable) and falls back to the Worker-set
  `x-corelink-tenant-id` for reads; if NO tenant id is available it now fails
  CLOSED (503) instead of silently skipping the charge — a missing label is a
  Worker header-injection regression and must surface, not let a tenant exceed
  its ceiling unmetered. (2) The container write-gate threads the tenant id from
  its single F27 PAT verification into a typed, server-internal request
  extension (`GateResolvedTenant`); the adapter's `handle_put` reuses it instead
  of re-running the HMAC + Argon2id verify a second time — eliminating ~40-200ms
  of redundant CAS-hot-path latency and making the surface's "exactly ONE PAT
  verification per request" doc claim accurate. Files:
  `crates/corelink-container/src/routes/cargo.rs`,
  `crates/corelink-adapter-host/src/cargo/server.rs`.

- **Per-consumer introspect keys (blast-radius isolation)** — the
  `/internal/v1/auth/introspect` gate now accepts a SET of dedicated service
  secrets, one per distinct consumer, so a compromised consumer can never present
  (nor leak the blast radius of) another's credential. Primary
  `FABRIC_INTROSPECT_AUTH_KEY` = the corelink-runners fabric; new optional
  `FABRIC_INTROSPECT_AUTH_KEY_HUGR` (≥32 chars, else ignored with a warn) = the
  HuGR toolkits fleet (37 MCP Workers) consuming CoreLink auth via Mode B
  introspection. The gate checks the header against every configured key WITHOUT
  short-circuiting (no consumer-identity timing oracle) and stays fail-CLOSED.
  File: `crates/corelink-container/src/routes/auth_introspect.rs`.
- **admin-ui `/welcome` no longer reads the one-time PAT plaintext from the Clerk
  session JWT** (`CRED-pat-plaintext`, HIGH). The welcome page now fetches
  `pat_plaintext` SERVER-SIDE from the user's Clerk `private_metadata` via the
  Backend API (`clerkClient().users.getUser()`) instead of `sessionClaims`, and the
  "I've saved my token" clear targets `private_metadata.pat_plaintext` (via
  `clerkClient().users.updateUser()`) rather than `public_metadata`. This matches the
  signup-worker moving the secret off all client-readable/JWT-broadcast surfaces into
  backend-only `private_metadata` (`public_metadata` retains only `{tenant_id, region}`).
  Files: `apps/admin-ui/src/app/[locale]/(authenticated)/welcome/{page.tsx,actions.ts}`.
- **npm + pip planes: quota gate no longer fails OPEN on a missing tenant label**
  (`REV-S3`, mirrors the cargo plane fix). The `/npm/*` and `/pip/*` per-tenant
  `$`-ceiling gates (ADR-0068) previously attributed cost ONLY to the Worker-set
  `x-corelink-tenant-id` header and SILENTLY SKIPPED the charge when it was
  missing/empty — an unmetered quota bypass on any billable op that reached the
  gate without the header. Both gates now attribute cost to the PAT-resolved
  tenant id for writes (authoritative, un-spoofable via the F27 verify), fall
  back to the header for reads, and FAIL CLOSED (`503`) when neither is
  available, so a missing label surfaces as a Worker header-injection regression
  instead of letting a tenant exceed its ceiling unmetered. Regression tests
  assert a billable op with no tenant header now `503`s rather than skipping.
  Files: `crates/corelink-container/src/routes/{npm,pip}.rs`.
- **npm tarball integrity now verifies SHA512, not just broken SHA1** (`REV-S3`).
  The npm cache-fill previously verified downloaded tarball integrity ONLY
  against the legacy SHA1 `dist.shasum` (cryptographically broken — a forged
  tarball with a SHA1 collision would have been stored + served as authentic).
  The adapter now prefers the SHA512 `dist.integrity` SRI (`sha512-<base64>`)
  that the npm registry publishes for modern packages — the strong hash is
  load-bearing and a tarball that fails SHA512 is rejected (fail-CLOSED + audit
  emit) even if its SHA1 matches; SHA1 remains only as a fallback for legacy
  packages with no SHA512 SRI. Files:
  `crates/corelink-adapter-host/src/npm/tarball.rs` (new `verify_sha512` /
  `parse_sha512_sri` / `verify_tarball_integrity`),
  `crates/corelink-adapter-host/src/npm/server.rs` (threads `dist.integrity`).
### Performance
- **Budget-LEASING for the per-tenant `$`-ceiling quota gate** (WP-2a;
  `crates/corelink-container/src/tenant_quota.rs`): a new
  `LeasedQuotaStore` wraps an inner `Arc<dyn QuotaStore>` and itself
  implements `QuotaStore` (drop-in — the trait, `QuotaGuard`, and the
  routes are unchanged; the lead wires it at construction). Today the CAS
  hot path calls the gate on EVERY billable op, which against the
  production `D1QuotaStore` is one synchronous D1-over-HTTP round-trip
  (~0.3–0.7 s) per op — a measured root cause of CAS latency. The wrapper
  debits a small CHUNK of budget (`DEFAULT_LEASE_OPS = 16` ops) from the
  durable inner store atomically UP FRONT, then serves subsequent ops from
  the in-memory lease without touching D1 until it drains, amortising the
  round-trip 16:1. The fail-CLOSED `$`-ceiling (#318 paid-without-payment
  gate + all CAA-360 invariants) is **preserved exactly**: budget is always
  debited in durable D1 before any op is served (charge-never-lost; a crash
  under-charges, never over-serves); a drained lease with an over-ceiling /
  unreachable inner store still rejects (`402`/`503`, never fail-open); a
  near-ceiling refill falls back to a partial lease so the durable side
  NEVER exceeds the ceiling; the lease is keyed by cycle anchor so a rolled
  cycle discards the stale lease; and all lease accounting is `Mutex`-guarded
  (no double-spend). Worst-case *overshoot* is bounded at
  `LEASE_OPS × cost_per_op` = `$0.016` on the `$5`/mo tripwire (0.32 %), the
  crash-loss tail of a single in-flight lease. Six new tests cover
  amortisation, over-ceiling 402, drained-then-store-error 503,
  charge-never-lost, cycle-roll invalidation, and concurrent no-double-spend.

### Added
- **WP-2b — Bloom-fronted CAS tombstone gate (`crates/corelink-container/src/routes/cas_erase.rs`).**
  New `BloomTombstoneStore` wraps an inner `Arc<dyn TombstoneStore>` and itself implements the
  (unchanged) `TombstoneStore` trait — a drop-in the lead wires at construction. The CAS read hot
  path calls `is_tombstoned(tenant, hash)` on EVERY GET, which today is a synchronous
  D1-over-HTTP round-trip (~0.3–0.7s) — a measured CAS-latency root cause. Tombstones (erased
  objects) are rare, so the wrapper fronts D1 with an in-memory per-tenant Bloom filter: a
  `definitely-absent` digest returns `Ok(false)` with **zero** D1 calls (the 99.99 % common
  case); a `maybe-present` digest falls through to the authoritative inner store. **GDPR Art.17
  safety:** a Bloom has false-positives (safe → extra D1 check) but, by construction, no
  false-negatives; cross-instance freshness is bounded by a per-tenant **staleness window**
  (default 30 s) — a tenant's bloom is loaded on first touch and re-stamped when older than the
  window, and the reloading lookup falls through to D1 authoritatively, so a tombstone written by
  another container instance is visible within ≤ one window. This is ≤ the existing posture (the
  gate already fails OPEN on a transient D1 blip; the erase write-side deletes the R2 bytes before
  writing the tombstone, so a within-window slip-through 404s rather than serving erased content).
  Inner errors propagate UNCHANGED on the maybe path (preserves fail-OPEN). Hand-rolled fixed-size
  bloom over std `DefaultHasher` (SipHash-1-3) double-hashing on a `Vec<AtomicU64>` — bounded
  memory (128 KiB/tenant, never grows), lock-free reads/writes — **no new external crate**. Adds
  unit + async tests proving each of the five invariants (fast-path skips D1, through-write stays
  true, no-false-negative after refresh, false-positive defers to inner, inner-error propagates,
  concurrent read/write). `cas.rs` is untouched.
- **Native-REST bulk CAS endpoints** (`crates/corelink-container/src/routes/cas.rs`):
  `POST /v1/cas/:tenant/batch` (length-framed bulk write), `POST /v1/cas/:tenant/batch-read`
  (length-framed bulk read), and `POST /v1/cas/:tenant/batch-exists` (bulk HEAD-class
  existence probe). The single-object `/v1/cas/:tenant/:hash` path costs one D1 round-trip
  per object, which dominates wall-clock on bulk git ingest; these collapse N objects into
  ONE request — one auth + one scope check + one PAT-gate + a **single** `QuotaGate::check_batch(n)`
  charge (never per-object, preserving the #318 $-ceiling discipline) + batched storage. FROZEN
  contract: upload content-type `application/x-hugit-cas-batch` (415 otherwise; read-side routes
  also accept `application/x-ndjson`), caps of ≤2000 objects AND ≤8 MiB per batch (413
  `batch_too_large`, sits under the global 10 MiB body limit), framing errors ⇒ 400, cross-tenant
  ⇒ 403. The upload commits each object independently (per-object `created`/`exists`/`error`)
  so one bad object never aborts the batch; content-verify and storage are delegated to the SAME
  `state.write`/`state.read` chokepoints the single-object path uses. Full route + framing-helper
  + 16 route tests (happy/idempotent/per-object-error/over-cap/415/400/round-trip/tombstone-gone/
  exists/cross-tenant/quota-charged-once).
- **`specs/_runbooks/RB-INCIDENT-RESPONSE.md` (DRAFT)** — master incident-response
  runbook, authored to close a compliance-doc gap: the DPA §9 (all 3 locales), the
  PCI-DSS SAQ-A Q19, and a sealed S20 adversarial-summary finding all cite this path,
  but the file did not exist. Grounds the IR flow (detection → triage/severity →
  containment → breach-notification decision-tree handoff → eradication/recovery →
  postmortem handoff → the DPA's 72h regulatory/customer-notification timeline) in the
  existing artifacts (`RB-ONCALL-ESCALATION-MATRIX.md`, `RB-POSTMORTEM-PROCESS.md`,
  `RB-SECURITY-VULNERABILITY-INTAKE.md`, `legal/breach-notification/rb-breach-notif-decision-tree.yaml`
  + templates/dry-run scenarios, the `emitLifecycleEvent` PagerDuty wiring, and the
  `corelink-slo`/`corelink-telemetry` crates). Carries a prominent DRAFT/pending-owner+legal
  header and marks owner-gated / not-yet-built capabilities explicitly. `validate_specs.py`
  green (0 failures).

### Changed
- **`customer_d1.rs::map_billing_status` — REV-S5 known-limitation made explicit (no
  behavior change).** Audit REV-S5 flags that the `incomplete` (pending FIRST payment)
  `tenant_billing.status` is collapsed onto the dashboard `past_due`, so a pending first
  payment reads as a renewal failure. The recommended fix (a distinct `pending` status)
  cannot land in `customer_d1.rs` alone: the dashboard status set is a FROZEN cross-team
  contract (the `apps/admin-ui` `customer-types.ts` union + the `tenant_billing.status`
  CHECK in `specs/03_architecture/data_model.md` have no `pending` value). The `incomplete`
  arm is split out (still → `past_due`, byte-identical output) and a guard test
  (`billing_status_map_stays_in_frozen_dashboard_union`) now trips if the map ever emits a
  value outside the frozen union — forcing a future fix to widen the contract in the same
  change. Emitted statuses are unchanged.
- **Consolidated SAFE minor/patch JS/Node dependency bumps** across the workspace
  (dependabot groups `root-tooling-minor-patch` #339, `apps/admin-ui` npm-minor-patch
  #289, `apps/docs` docs-minor-patch #314). Touches root + every `apps/*` +
  `worker` `package.json` and regenerates `pnpm-lock.yaml`: `wrangler` →4.101.0,
  `@cloudflare/workers-types` →4.20260617.1, `@cloudflare/vitest-pool-workers` →0.16.16,
  `miniflare` →4.20260616.0, `@vitest/coverage-istanbul` →4.1.9; admin-ui `react`/
  `react-dom`/`@types/react*` →19.2.x, `@radix-ui/*`, `@tailwindcss/postcss`/`tailwindcss`
  →4.3.1, `@playwright/test` →1.61.0, `happy-dom` →20.10.6; docs `@sentry/browser`
  →10.58.0, `@typescript-eslint/*` →8.61.1, `protobufjs` →8.6.4. **No framework majors:**
  the two MAJOR dependabot groups (`#290` admin-ui-major — Next 16 / Clerk / Stripe /
  Sentry, and `#315` docs-major — eslint 10) are intentionally **DEFERRED to a QA'd
  post-launch upgrade** because they break the CI `lint` gates and are runtime-risky on
  the launch checkout UI. Verified: admin-ui + docs build/typecheck/**lint** green;
  worker/analytics/get-corelink/signup typecheck green; `next@15.5.18`, `eslint@9`,
  `@clerk/nextjs@6`, `@stripe/stripe-js@4`, `@sentry/nextjs@8` unchanged.

### Added
- **Worker-plane observability: native retained Workers Logs + an inert Sentry hook.** An operator was
  blind on the Worker plane at launch (no `[observability]` anywhere, no Sentry on the main Worker). Now:
  (1) every Worker `wrangler.toml` carries an `[observability]` block (`enabled = true`,
  `head_sampling_rate = 1`) — the zero-cost native retained/queryable Workers Logs feature — added
  top-level AND per deployed env (the block is NOT inherited by named `[env.*]`): main Worker
  `prod` + `staging` + the four regional `prod-{sam,lhr,nrt,syd}`, plus `signup-worker`,
  `analytics-worker` (`prod`/`staging`), `get-corelink-worker` (`prod`), and `corelink-clerk-cf` (`prod`).
  (2) The MAIN Worker (`worker/src/index.ts`) now wires `@sentry/cloudflare` error tracking via
  `Sentry.withSentry`, mirroring `apps/analytics-worker` EXACTLY: init is gated on `env.SENTRY_DSN`
  (empty DSN ⇒ a COMPLETE no-op, so it stays inert until the operator sets the secret), `sendDefaultPii=false`,
  and a `beforeSend` scrub of Authorization/Cookie/API-key/`x-corelink-internal-auth` headers
  (INV-NO-PII-IN-LOGS). `SENTRY_DSN?`/`SENTRY_RELEASE?` added to the Worker `Env` interface; the dep is
  `@sentry/cloudflare@^8.45.0` (same version as analytics-worker). This is NOT Logpush to an external sink
  (that needs an owner-provided destination and is deliberately left off). **Operator activation** (all
  manual, post-merge): (a) deploy the Workers so the `[observability]` blocks take effect; (b)
  `wrangler secret put SENTRY_DSN --env prod` (and per regional env) to arm error tracking — until set the
  hook sends nothing; (c) OPTIONALLY configure a Logpush destination if logs must leave Cloudflare.
- **githugr Clerk sessions on the exchange seams (multi-issuer, Option B).** `/v1/session/exchange` and
  `/internal/v1/auth/token-exchange` now accept sessions from the SEPARATE githugr Clerk instance
  (`clerk.githugr.com`) IN ADDITION to CoreLink's — opt-in per call-site (`allowGithugrIssuer`), routed
  by the (routing-only) unverified `iss`, verified networklessly against githugr's PUBLIC `jwtKey` (no
  githugr secret crosses into CoreLink), and resolved to a single fixed `GITHUGR_TENANT_ID` (githugr is
  ONE non-billing CoreLink tenant; the forge does its own per-user isolation via the passed-through
  `principal`). `CLERK_ISSUER_URL` is left untouched, so CoreLink's own dashboard + onboarding are
  unaffected. Armed only when all three settings are present (`GITHUGR_CLERK_ISSUER_URL`,
  `GITHUGR_CLERK_JWT_KEY`, `GITHUGR_TENANT_ID`); absent ⇒ dormant (githugr sessions 401, no behavior
  change). A separate `GITHUGR_AZP_ALLOWLIST` const keeps githugr's azp from widening CoreLink's.
- **`max_vcpu_h` on the runners introspect entitlement (runner↔server contract).** The
  `/internal/v1/auth/introspect` response now carries an optional `max_vcpu_h` (u32 vCPU-hours,
  `skip_serializing_if`) alongside `max_concurrency`, read from a new nullable `max_vcpu_h` column on
  `runners_entitlement` (migration `0072`). Per-tenant monthly compute ceiling (100/240/600/1200/2400 by
  tier; Enterprise bespoke). Intentional fail-closed ASYMMETRY vs `max_concurrency`: absent
  `max_concurrency` ⇒ "no Runners entitlement → reject"; absent `max_vcpu_h` ⇒ "entitled, compute-wall
  OFF" (byte-compatible; arms when populated). `conformance/corelink-introspect.json` updated.
- **Launch-hardening scanners: gitleaks (secret-leak) + trivy (JS-dep CVE + IaC misconfig) CI gates.**
  Two pinned-binary, no-sudo gates on the self-hosted macOS fleet (the Linux fleet is down — see Fixed).
  `gitleaks` (`.gitleaks.toml`) extends
  the default ruleset with a custom Stripe-`whsec_` rule (the default set misses webhook secrets — the
  baseline caught a stale committed one) and a `CORELINK_*_AUTH_KEY`/`PAT_SIGNING_KEY` 64-hex rule; PR
  runs scan the `base..head` range, dispatch runs scan full history. `trivy` (`.trivyignore.yaml`,
  path-scoped) scans the JS/npm graph (`--scanners vuln`, `Cargo.lock` skipped → RustSec stays
  cargo-deny's lane) and IaC/Dockerfile misconfig. Baseline triage:
  `docs/security/2026-06-18-gitleaks-baseline-triage.md`.

### Fixed
- **Stripe/Clerk webhook audit fixes (billing dunning-recovery, terminal payment detection, DSR legal-hold).**
  Three findings in `apps/signup-worker/src/webhooks/`: (1) REV-S5 (medium) —
  `customer.subscription.updated` returning to active/trialing after a dunning lapse (payment-method fix +
  auto-retry, an operator marking an invoice paid, or an incomplete→trialing resolution) never re-activated
  `tier_selections`, stranding a PAYING tenant at `subscription_state='inactive'` (quota gate denies access).
  Added a re-activation path (`reactivateTierSelectionBySubscription`) gated on `tenant_billing.status != 'canceled'`
  so a late out-of-order `updated(active)` after a cancel can NOT resurrect a terminated subscription, and which
  NEVER inserts (checkout stays the single activation writer). (2) REV-S5 (low) — `invoice.payment_failed` terminal
  detection relied solely on `'next_payment_attempt' in obj && === null`, missing off-cycle/manual/credit-note
  invoices Stripe marks `status: 'uncollectible'` without that key; added `status === 'uncollectible'` as an
  additional terminal condition. (3) REV-O1 (low) — the Clerk `user.deleted` erasure trigger hardcoded
  `legal_hold: false`, making the CTRL-PRIV-033 preservation branch unreachable; `buildErasureQueueMessage` now
  takes an explicit `legalHold` resolved from a `tenant_legal_hold` source-of-truth (`tenantUnderLegalHold`), so a
  held tenant's deletion preserves rather than erases. Files: `webhooks/stripe.ts`, `webhooks/clerk.ts` (+ tests).
- **`build-container-prod.sh` smoke probe could hang the build indefinitely.** Step 6 ran an
  unbounded foreground `docker run --rm … --version`; the CoreLink server binary ignores that flag
  and BOOTS instead of exiting, so the probe blocked forever (observed: a 30+ minute hang during the
  prod redeploy, with a stray booted container left running). Added a portable `bounded_run` helper
  (prefers `timeout`/`gtimeout`, pure-bash watchdog fallback for the macOS build host) wrapping both
  flag-probes, and named the probe containers so a bound-killed probe is force-removed instead of
  lingering. A bound-kill now cleanly falls through to the authoritative detached gRPC start-probe.
- **Main worker was not deployable — added the missing `nodejs_compat` flag.** `@sentry/cloudflare`
  (added to `worker/src/index.ts` for observability) imports `node:async_hooks`, which the CF API
  rejected at deploy with `No such module "node:async_hooks"` [10021] because the root `wrangler.toml`
  set no `compatibility_flags`. Added `compatibility_flags = ["nodejs_compat"]` top-level (covers
  prod + the 4 regional envs + staging; `compatibility_date` 2026-04-01 is past the flag's floor) —
  matching `apps/signup-worker` / `apps/get-corelink-worker`. Verified `wrangler deploy --env prod
  --dry-run` builds clean. (This is why prod was stale: `main` failed a clean deploy. Follow-up: a
  `wrangler deploy --dry-run` CI gate so "merged" implies "deployable.")
- **Vendor-review-evidence gap — `legal_review_evidence:` paths now exist + are existence-checked.**
  `legal/sub-processors.md` (and `legal/dpa/SUB-PROCESSOR-COMMITMENTS.md`) declare
  `legal_review_evidence:` paths under `docs/compliance/vendor-reviews/`, but that
  directory did not exist and `scripts/validate_sub_processors.py` only checked the
  path *format*, not *existence* — so the gap passed silently. Created
  `docs/compliance/vendor-reviews/` with a `README.md` (evidence-store doc), a
  `_TEMPLATE.md`, and a per-sub-processor evidence stub at the exact path each
  disclosure references (all clearly marked `STATUS: TEMPLATE — pending the actual
  legal review record`; no review outcomes fabricated — owner/counsel to complete).
  Hardened the validator with a path-EXISTENCE check (d.2) alongside the existing
  format check (d.1), anchored to the repo root, so a well-formed-but-missing
  evidence path can no longer pass. Sub-processor list contents and the DPO email
  are unchanged.
- **Legal-doc hygiene — sub-processor source-of-truth + broken internal path refs.**
- **Bounded the adapter PAT verifier's per-tenant Argon2id semaphore map (memory creep).**
  `PatVerifier.per_tenant_permits` (`crates/corelink-container/src/adapter_pat.rs`) lazily
  created one `Arc<Semaphore>` per distinct real `tenant_id` and never evicted — a slow
  unbounded-memory creep over a long-running container (the #1/#12 follow-up flagged at the
  merge of #354). It is now an LRU bounded at 10k entries: when the map is full and a NEW
  tenant must be inserted, the verifier evicts the least-recently-used entry that is FULLY
  IDLE (`available_permits == per_tenant_cap` ⇒ no in-flight verify for that tenant), so
  eviction can never disrupt an active or contended tenant; a re-inserted evicted tenant
  lazily recreates its (idle) semaphore — semantically identical. The single shared
  `UNKNOWN_TOKEN_BUCKET` (dummy-burn) is never evicted. The two-tier global→per-tenant
  acquire order, the no-lock-across-await discipline, and the poisoned-lock fail-safe
  (fall back to global-only bounding) are all unchanged.
  Reconciled the sub-processor source of truth so the DPA, the legal disclosure
  (`legal/sub-processors.md`), and the auto-generated public page can no longer
  drift: `legal/sub-processors.md` now documents the single chain explicitly —
  the engineering source of truth is `specs/_compliance/VENDOR-RISK-REGISTER.md`
  (from which `scripts/gen-public-subprocessors.py` generates the public
  `/trust/subprocessors` page under the `.github/workflows/subprocessors-sync.yml`
  drift gate), while `legal/sub-processors.md` is the authoritative *contractual*
  disclosure that must be updated in the same PR; corrected the legacy "Change
  Process" wording that wrongly implied the page was generated from the legal
  file itself, and aligned the generator docstring. Also fixed broken internal
  path references in the DPA suite: `specs/03_architecture/canonical/security_model.md`
  → `specs/03_architecture/security_model.md` (DPA en-US/es-419/pt-BR + EU SCC
  Annex II), `specs/03_architecture/canonical/resilience_patterns.md`
  → `specs/03_architecture/resilience_patterns.md` (EU SCC), and
  `legal/lia/tia-template.md` → `legal/tia-template.md` (DPA all 3 locales).
  Sub-processor list contents and the privacy/DPO contact email were left
  untouched (legal-fact decisions). Doc/config only.
- **OCI registry storage-cap on downgrade (WP #10).** An OCI blob push reached the byte-accounting
  moat (`MoatCache::put`) with the per-tier storage cap hard-coded to `None`, and the Worker forwards
  the OCI `/v2/*` + `/token` surface RAW (it cannot resolve the cap for the two-leg flow, so it never
  sets the native `x-corelink-storage-quota-bytes` header). A **DOWNGRADED** tenant pushing exclusively
  over OCI therefore over-stored up to its stale cap until a native CAS/AC write reseeded the row. The
  cap is now resolved at the `/token` mint — the one seam where OCI knows the tenant (full Option-B PAT
  re-verify) — via a container-side `tier → cap` resolver (`oci_cap.rs`) that ports the Worker's
  `QUOTAS[tier].storageBytesMax` derivation (`tier_selections(active) → tenant.tier → free`,
  fail-CLOSED on a D1 error). The resolved cap is embedded in the **signed** HMAC bearer (token wire
  format extended to `corelink-oci.<tenant>.<scope>.<cap>.<expiry>.<hmac>`; re-signed over the new
  preimage so a tenant cannot forge a larger cap — OCI has no live customer bearers pre-launch, so no
  back-compat shim) and threaded at finalize (`finalize_upload → MoatCache::put →
  CasWriteRequest::with_storage_quota_bytes`). An OCI write now reserves against the resolved cap and is
  rejected (402-equivalent) when over it; an unresolvable cap fails CLOSED on an unseeded tenant,
  mirroring the native plane. Regression tests prove over-cap reject / under-cap accrue / fresh-tenant
  fail-closed; the digest-verify-before-commit invariant (#2) stays green.
- **D1 migration apply tooling — DR-hardening against the ledger-desync / re-provision landmine
  (#19).** The two stale prod-apply scripts (`scripts/apply-d1-migrations-prod.sh`,
  `scripts/d-day-migrations-apply-prod.sh`) hard-paused on a brittle hardcoded
  `EXPECTED_FILE_COUNT` (62 / 52) — with 71 migration files on disk both would HARD-PAUSE on any
  legitimate re-provision. The count is now computed **dynamically** from the actual
  `migrations/d1/*.sql` files at runtime (the truncation guard — count==0 hard-pauses — is kept;
  true ledger-vs-files drift is owned by wrangler's idempotent `d1_migrations` ledger). Added a
  **unique-4-digit-prefix lint** (`scripts/check_migration_prefixes.py`) wired into the fast
  `d1-migration-validate` CI gate that FAILS if two migrations share a 4-digit prefix (prevents the
  duplicate-`0044` class going forward; the existing already-applied `0044` pair is grandfathered as
  an exact-set exception). Fixed the replay test
  (`crates/corelink-ops/tests/migrations_d1_migration_integration.rs`): removed the now-stale
  `PRE_EXISTING_FAILURES` pins (0027 / 0036 / 0037 — verified all three now replay cleanly against
  in-memory SQLite, the underlying migrations were corrected in earlier waves) and added a
  stale-entry guard mirroring the `KNOWN_HAZARDS` pattern so dead failure-pins can't silently
  accumulate.
- **Turbo `/events` fairness — residual slowloris + per-tenant monopolisation gaps (rt-nuclear
  cycle-2 #8 + #9).** A prior fix gave `POST /v8/artifacts/events` its own
  `GLOBAL_TURBO_EVENTS_BUDGET` (decoupled from the PUT write budget) but left two residuals on the
  events pool. **#8 (slow-body):** the handler buffered the body via the unbounded `Bytes` extractor,
  so a slowloris dribbling its (≤ 64 KiB) body could pin an events permit + slot indefinitely — the
  handler now reads the raw body under an `EVENTS_BODY_READ_TIMEOUT` (5s) deadline (and the existing
  64 KiB `EVENTS_BODY_LIMIT_BYTES` cap → 413), aborting a stalled body with 408 so the held permit +
  slot RAII-release. **#9 (per-tenant cap):** `/events` had only the process-wide budget, so one
  tenant could take every permit and starve other tenants — added a per-tenant `EventsConcurrencyGuard`
  (`FromRequestParts`, runs before the body) capping each tenant at `EVENTS_CONCURRENCY_LIMIT` (4)
  in-flight (429 over it), mirroring the PUT plane's `PutConcurrencyGuard`/`PutSlot`. Additive
  hardening — the existing events-budget behaviour is unchanged. Regression tests cover the 408
  slow-body release and the per-tenant cap (hog 429'd, other tenant still served).
  (`crates/corelink-container/src/routes/turbo_v8.rs`).
- **Per-tenant fairness for the Argon2id PAT-verify pool (red-team #1/#12, HIGH — auth hot path).**
  The container-side `PatVerifier` (`crates/corelink-container/src/adapter_pat.rs`) bounded total
  Argon2id concurrency with ONE process-wide semaphore (`ARGON2_VERIFY_PERMITS = 16`) but was not
  per-tenant-fair: a single tenant flooding distinct PATs (each forcing a fresh Argon2id verify)
  could take all 16 permits and 503 the native plane for every OTHER tenant (#1). Added a two-tier
  gate — the global bound is now paired with a lazily-created per-tenant `Semaphore`
  (`ARGON2_PER_TENANT_PERMITS = max(2, ARGON2_VERIFY_PERMITS/4) = 4`), acquired in a CONSISTENT
  order (global → per-tenant, deadlock-free) and RAII-released on every path (success/error/timeout).
  One tenant can now hold at most 1/4 of the pool, so ≥3/4 stays reachable by others under a flood;
  exceeding the sub-cap fails CLOSED with the same `Backend("overloaded")` signal as a global timeout.
  FAIL-SAFE: a poisoned per-tenant map falls back to global-only bounding (a bookkeeping fault never
  blocks a legitimate auth). For #12, the cheap D1 row lookup already runs BEFORE the expensive
  Argon2id verify (a valid-HMAC token for a nonexistent/expired/revoked `token_id` is decided at the
  D1 stage, not via a full Argon2id) — confirmed, no reorder needed; additionally the None-row
  timing-parity dummy burn is now routed through one shared synthetic per-tenant bucket so a
  leaked-key flood across bogus `token_id`s cannot drain the global pool through that path either.
  Adds 4 deterministic concurrency tests proving the fairness invariant (tenant A saturated ⇒ tenant B
  not starved), the global bound still holding, the fail-safe fall-through, and the same-tenant /
  distinct-PAT containment. (Shuttle — the deterministic async-interleaving tester — is NOT wired in
  this workspace; retrofitting it cannot model the `spawn_blocking` + real Argon2id path, so it is
  flagged as the owner-aware follow-up rather than half-wired.)
- **OCI signing-key legacy alias was a silent no-op + the env-contract gate was off PRs (A4
  secrets/config hygiene #6/#17/#18/#24).** Prod's Worker holds the OCI session HMAC key under the
  legacy name `HUGR_OCI_TOKEN_KEY` (CAA-360 #8 name drift); the container reads
  `CORELINK_OCI_TOKEN_KEY` first and falls back to `HUGR_OCI_TOKEN_KEY` via `.or_else(...)`
  (`crates/corelink-container/src/routes.rs:646`, kept intact), so OCI works in prod only through
  that fallback — yet the Worker DO never forwarded `HUGR_OCI_TOKEN_KEY` to the container, making the
  fallback the ERASURE_SALT_KEY/F8 class of silent no-op. Forwarded `HUGR_OCI_TOKEN_KEY` alongside
  `CORELINK_OCI_TOKEN_KEY` in `worker/src/durable_object.ts` container.start (and declared it on the
  `Env` interface), documented it as a legacy-alias row in `docs/internal/secrets-checklist.md`, and
  wired `scripts/check-env-contract.py` (pure-grep container env-contract gate) into the
  `secrets-drift` PR workflow so any future read-but-not-forwarded var fails the PR. No key material
  changes; retire by renaming the prod secret to the canonical name.
- **Worker vitest test-debt cleared — the suite is now honest and fully green (no unexplained reds).**
  After the honest-PAT harness (#345) landed, 10 worker vitest tests were red because their expectations
  predated either that harness or two product hardenings. All were stale TEST bugs (NO product code
  changed): (1) three `/health` env-field assertions (`index.test.ts`, `integration.test.ts`,
  `do_miniflare_integration.miniflare.test.ts`) still expected a `body.env` field that F19 intentionally
  OMITS on unauthenticated endpoints — now assert `env` is absent; (2) the region-fanout scope test
  (`index.test.ts` H1) seeded `primary_region: "lhr"` (a colo string) where the worker now expects a MACRO
  code (`coloForMacro`: `weur → lhr`) — fixed to seed `weur`; (3) the two `customer_clerk_bridge` PAT-surface
  regressions and (4) the five `do_miniflare_integration` auth/isolation tests used the pre-harness unsigned
  all-"A" PAT and unset `PAT_SIGNING_KEY`, so `extractAuth` fail-closed to 503 instead of exercising the real
  401/DO path — now mint HMAC-valid PATs via `mintTestPat` and bind `TEST_PAT_SIGNING_KEY` (the miniflare
  harness also now seeds a `tenant.primary_region` row so authenticated requests fall through to the DO's
  `CONTAINER_UNAVAILABLE`). The previously `.skip`-ed quota-429 integration test in `quota.test.ts` — skipped
  pending exactly this harness — was un-skipped and now passes. Final: 369 passed, 0 failed, 0 skipped
  (main config) + 22 passed (miniflare config). No tests left skipped-with-reason; the pool is usable via the
  programmatic miniflare v4 API.
- **Docs site pointed customers at dead hosts on the first-5-minutes path.** The auto-generated REST API
  reference (50 endpoint pages × 5 language samples) and the static OpenAPI download both targeted the
  non-resolving `api.corelink.humangr.com`; the quickstart/installation/first-PAT pages sent sign-up and
  welcome links at the dead apex/`app.corelink.humangr.com` hosts; and several pages named the dead
  `docs.corelink.humangr.com`. Fixed the generator (`scripts/gen-api-reference.py`) to resolve the
  example base URL from the OpenAPI `servers[]` (canonical `https://corelink-api.humangr.com`) instead of
  a hardcoded literal, regenerated all 50 pages, and corrected the remaining hand-written + i18n pages and
  the static `openapi-corelink-v1.yaml` `servers` block to the canonical flat hosts (`corelink-api` /
  `corelink-app` / `corelink-docs`.humangr.com). Also added the missing `static/img/og-image.png` (the
  Docusaurus `og:image`/`twitter:image` config 404'd — only the `.svg` existed) and replaced the
  `:::note Placeholder` admonition on the primary-nav **Explanation** landing page with real content.
- **Container Stripe-webhook materializer could (re-)grant a paid entitlement on a non-granting
  subscription status (money-path defense-in-depth).** On `customer.subscription.updated`,
  `reconcile_tier` → `persist_tier_change` → `upsert_tier` (`SQL_UPSERT_TIER`) UNCONDITIONALLY wrote
  `tier_selections.subscription_state='active'` for ANY status, so a subscription that had dropped to
  `past_due`/`unpaid`/`incomplete`/`incomplete_expired`/`paused`/`canceled` (or an unknown status) on a
  recognized plan would re-grant the canonical access gate — the materializer is a SECOND writer of that
  gate and, unlike the authoritative signup-worker, had no payment-status check (the only prior guard was
  an accidental plan/price mismatch). Added a status gate mirroring the signup-worker's
  `subscriptionStatusGrantsAccess` (granting set = `active`, `trialing` ONLY; everything else fail-safe):
  a non-granting status now SKIPS the `'active'` entitlement upsert (and its `tier_changed` audit) while
  still recording the `subscription.materialized` audit + the `stripe_subscriptions` row with the real
  status. `active`/`trialing` behavior is unchanged. Regression test added.

### Security
- **PAT plaintext no longer rides the session JWT / client surface (CRED, HIGH — CTRL-CRED-001).**
  The signup-worker wrote the freshly-minted PAT plaintext into Clerk **`public_metadata`**, which is
  client-readable (`useUser()`) AND embedded in the session JWT — so the secret was broadcast to every
  service that validated the session (incl. githugr) and, because the only clear was the client-driven
  `/welcome` reveal, it persisted FOREVER for any user who never opened `/welcome`. Fixed: the signup
  flow now writes `pat_plaintext` (+ a `pat_revealed_at` clock) to Clerk **`private_metadata`**
  (backend-only — never in the JWT, never client-readable); `public_metadata` keeps ONLY the legit
  session claims `{ tenant_id, region }`. Added a **guaranteed hourly scrub cron**
  (`apps/signup-worker/src/webhooks/pat_scrub_cron.ts`, wired into the worker's `scheduled()` on the
  existing `0 * * * *` tick) that clears `private_metadata.pat_plaintext` for any user whose reveal is
  older than a 1h TTL (fail-closed: a secret with no usable reveal clock is also scrubbed), so an
  un-visited `/welcome` cannot leave the secret resident. The e2e check (`scripts/e2e-clerk-signup.sh`)
  now asserts the PAT is ABSENT from `public_metadata` and present in `private_metadata`. Unit tests
  cover the metadata split and the scrub cron (stale→PATCH null, fresh→skip). The plaintext is never
  logged on any path. (signup-worker; `apps/admin-ui` welcome-read side tracked separately as WP-B.)
- **OCI blob upload could persist a digest-lie (cache poisoning — rt-nuclear cycle-2 #2).**
  `OciMoatStore::finalize_upload` ASSEMBLED and PERSISTED the uploaded bytes before the push handler's
  `verify_against_bytes` ran, and a mismatch left the bytes persisted under the (lying) `?digest=` key
  with no rollback — so a tenant could store content M under a `sha256:X` it does not hash to, breaking
  content-addressing within its registry. The store now verifies the declared digest against the
  assembled bytes (reusing `OciDigest::verify_against_bytes`, honoring the declared algorithm) BEFORE
  `moat.put` and rejects a mismatch fail-closed — a lying digest never reaches the persistent slot (no
  rollback needed). Regression test added (digest-lie finalize → error + nothing persisted).
- **Turbo `/v8/artifacts/events` could starve real cache writes (rt-nuclear cycle-2 #4/#8).** The
  accept-and-drop telemetry route shared the process-wide PUT write budget (`GlobalPutBudgetGuard`), so
  a telemetry flood / slow-body `/events` POST held PUT permits and 503'd legitimate Turbo cache writes
  fleet-wide (cross-plane DoS at ~$0 attacker cost). `/events` now holds a permit from its OWN dedicated
  budget (`EventsBudgetGuard` / `GLOBAL_TURBO_EVENTS_PERMITS`), isolating telemetry from writes while
  preserving the original OOM bound (≤ permits × 64 KiB) on the events pool.
- **Read-only PAT could revoke/enumerate ANY credential in its tenant (rt-nuclear cycle-2 #7).**
  `handle_keys_revoke` and `handle_keys_list` (customer plane) gated only on tenant + PAT possession —
  not scope — so a `cas:r` cache-pull token could revoke the owner's PAT (intra-tenant credential-DoS /
  org lockout) or enumerate every credential (the attack's recon step). Both now require cache-write
  capability (`requires_cache_write`), matching `handle_keys_create`; dashboard (`read-write`) + `cas:rw`
  callers are unaffected. Regression tests added (403 on `cas:r` revoke + list).
- **OCI registry reads bypassed the monthly $-ceiling (rt-nuclear cycle-2 #3).** The OCI per-tenant
  $-ceiling gate was wrapped in `if is_write`, so an authenticated tenant could pull unlimited blobs/
  manifests without ever hitting their ceiling — unmetered R2-GET cost-amplification and a read-path
  carve-out the native CAS/AC plane (which charges reads identically) does not have. The gate now runs
  on every OCI method with a resolvable bearer tenant; unauthenticated reads stay unmetered as before.
- **Closed GitHub Actions template-injection (shell-injection) in three PR-triggered workflows**
  (`dependabot-policy.yml`, `mutation-pr.yml`, `openapi-validate.yml`). Attacker-controllable context
  values — the PR base branch ref (`github.event.pull_request.base.ref` / `github.base_ref`) and the
  PR labels JSON — were interpolated by `${{ … }}` directly into `run:` shell, where a branch name or
  label carrying shell metacharacters could execute (the tj-actions/breach class; `dependabot-policy`
  is `pull_request_target`, the high-privilege trigger). Each is now passed via a job-scoped `env:` and
  referenced as a shell variable (data, not code-substitution) — behavior-identical. Found by `zizmor`;
  fixes verified locally with `zizmor` + `actionlint`. (The remaining lower-severity zizmor findings —
  safe `number`/`sha` values, `excessive-permissions`, `artipacked` — are a separate owner-aware phased
  sweep per `docs/launch/2026-06-18-sota-tooling-roadmap.md`.)
- **gitleaks + trivy scanners moved off the down Linux fleet onto the macOS fleet.** The self-hosted
  `[self-hosted, Linux, X64]` fleet has no runner registered (2026-06-18), so the two launch-hardening
  scanner gates were stranded queued. Both now `runs-on: [self-hosted, mac, corelink-builder]` (the same
  move already made for `semgrep`/`cargo-deny`/`cargo-audit`) with the macOS-x64 release binary +
  checksum (`gitleaks_8.30.1_darwin_x64`, `trivy_0.71.1_macOS-64bit`, authoritative `…_checksums.txt`)
  and `shasum -a 256 -c -` (macOS has no `sha256sum`). Light scans (no Rust compile) so they do not
  contend for disk with the build jobs. Install path verified locally on the Intel fleet arch; the heavy
  Linux-determinism jobs (`reproducible-build`, `ffi-matrix`) remain Linux-pinned pending a Linux runner.
- **Redacted a committed Stripe webhook signing secret** (`whsec_…`, stale/dead — its endpoint was
  already deleted) from `docs/operator/stripe-checkout-e2e-2026-05-29.md`. ⚠️ Operator action: confirm
  the secret is rotated/revoked in Stripe (it remains in git history).
- **GCP Workload Identity Federation provider now fails closed (trivy GCP-0068).** The BYOK GCP-KMS WIF
  provider (`infra/terraform/modules/byok-providers/gcp-kms`) had no `attribute_condition`, so it would
  federate any `sub` from the issuer (impersonation was still gated one layer later by the subject-pinned
  SA binding). Added `attribute_condition = assertion.sub == <corelink_runtime_subject>` so an unexpected
  subject is rejected at federation time. No behavior change for the one intended principal.
- **Bumped `ws` 7.5.10 → 7.5.11 (CVE-2026-48779)** via a pnpm override. Transitive dev-tooling dep
  (Lighthouse/puppeteer CI perf-audit graph), not the production runtime; patch bump within v7.
- **R2 op-class COGS instrumentation on the k6 cache load tests.** New
  `tests/load/k6/lib/cogs.js` attributes synthetic cache traffic to R2 op-classes
  (Class-A writes $4.50/1M, Class-B reads $0.36/1M — egress is $0, op-count is the COGS knob)
  and prints an estimated $ cost + `$/1M cache ops` in the summary. Wired into `cas-write-read.js`
  (Class-A per successful CAS write; Class-B per cache MISS — a HIT serves from edge ≈ 0 R2 GET).
  First-order model (documented caveats); turns "margin asserted" into "margin measured".

### Fixed
- **Worker vitest harness was dishonest — PAT-gated tests passed on a 503 misconfig, not on auth logic.**
  The unit-test env left `PAT_SIGNING_KEY` UNSET, so the native-plane possession gate (`extractAuth`)
  failed CLOSED (503) BEFORE reaching any HMAC/D1 auth or route logic; a 503 (misconfig) was
  indistinguishable from a real 401 (bad/forged/expired PAT), so "green" PAT tests were passing on the
  misconfig (test theater, adversarial-audit finding). Fixed the harness in `worker/tests/setup.ts` by
  exporting a fixed valid test signing key (`TEST_PAT_SIGNING_KEY`, 64 hex = 32 bytes) plus a `mintTestPat()`
  helper that mints canonical PATs whose 128-bit truncated HMAC-SHA256 sig verifies under it; `index.test.ts`
  now binds that key in `makeEnv()` and presents a validly-signed `TEST_PAT_TOKEN`, so PAT-gated tests
  exercise the real auth path. The fail-closed path is now covered on PURPOSE by an explicit negative test
  (`no PAT_SIGNING_KEY in env => 503`, plus a too-short-key variant) instead of being the silent default.
  This honesty turned 54 previously-503-red tests green. Follow-up (same branch): the 2 surfaced
  `parsePat` ci/ro tests now mint a VALIDLY-SIGNED 95-char ci/ro PAT (via `mintTestPat()` with only the
  env prefix rewritten — the HMAC preimage `<token_id>.<random_secret>` excludes the env segment) so they
  assert the REAL accepted-env behavior (clears parse + HMAC + D1 → reaches the DO stub, 503), not the old
  no-key 503 theater. `integration.test.ts`'s OWN `makeEnv` now also binds `TEST_PAT_SIGNING_KEY` and its
  `VALID_TOKEN` is minted with a real signature, un-503-ing its ~7 "reaches DO" pipeline tests. The
  remaining pre-existing worker-vitest reds (`customer_clerk_bridge.test.ts` + the `*.miniflare.test.ts`
  suites — the miniflare pool is unusable in this env) are genuinely separate and out of scope here.
- **Multi-region request-quota OVER-count: a fan-out sub-request was metered a second time (#11).**
  The Worker's monthly request-quota block ran its counter UPSERT on EVERY invocation, including the
  internal region fan-out sub-request that the primary Worker issues to a regional Worker. A single
  logical request from a multi-region tenant was therefore counted twice (customer-unfavourable
  double-charge), not a security bypass. Fixed by gating ONLY the metering (the
  `incrementMonthlyRequestCount` UPSERT + the request-cap comparison) on `isFanout`. The fan-out
  marker is **forgery-safe**: because the public edge does NOT ingress-strip `x-corelink-fanout-from`
  before the quota gate, a mere presence check would let any client forge the header to skip metering
  (a request-quota BYPASS, fail-open). Instead the primary Worker sets the header to the shared
  server-to-server secret `CORELINK_INTERNAL_AUTH_KEY` (bound on `[env.prod]` and every regional
  worker env per ADR-MULTI-REGION-V1) on the fan-out forward — over the service binding only, after
  `stripClientTrustHeaders` — and the regional Worker treats the request as a fan-out only on a
  CONSTANT-TIME match (`constantTimeSecretEqual`) against that secret. A forged value never matches,
  so it still meters; if the secret is unbound the match can never succeed, so every request meters
  (fail-SAFE). Tier resolution (`getTierForTenant`) and the server-trusted `STORAGE_QUOTA_HEADER`
  forwarding stay UNCONDITIONAL so a fan-out sub-request still forwards the resolved storage cap to
  its regional container.
- **k6 load tests defaulted their target host to the third-party `staging.corelink.dev` domain.**
  `corelink.dev` is an unrelated company (CoreLink Development); a local run without
  `K6_TARGET_HOST` set would have aimed load traffic at someone else's domain. Retargeted the
  default to our `staging.corelink.humangr.com` (matching the endurance workflow) across the suite;
  CI is unaffected (it always sets `K6_TARGET_HOST` from a validated staging secret).
- **CAS *write*-vs-*delete* byte-accounting race on the AC plane (rt-nuclear verify, C2 sibling).** The
  `AccountingAcHandler` `update()`/`delete()` decorators shared no per-key lock (only the CAS handler did),
  so a concurrent AC `update` + `delete` of the same `action_digest` released a stale `reclaimed_bytes` →
  `bytes_used` under-count → storage-quota evasion. Fixed by lifting the same per-`(tenant, action_digest)`
  sharded lock onto `AccountingAcHandler`, held across both `update()` and `delete()`.
- **Flaky Turbo concurrent-shrink regression test (C1).** `concurrent_same_key_shrink_puts_net_true_delta`
  fired N=8 concurrent same-tenant PUTs, but `PutConcurrencyGuard` caps at 4/tenant, so excess overlap
  intermittently 429'd and tripped the "each PUT 200s" assert (it passed in #326 by a scheduling fluke).
  Reduced N to 4 (= the admitted cap); the 4-way same-key shrink still exercises the double-release race
  deterministically (the per-key lock itself was proven correct — never the source of the flake).
- **Turbo write byte-accounting TOCTOU: concurrent same-key PUTs double-released `prior_len`
  (rt-nuclear verify C1).** The #324 byte-delta fix read `prior_len` via a non-serialized presence
  probe, so 2-4 concurrent PUTs to the same key (within the per-tenant cap) all observed the same prior
  size and all released it → `bytes_used` underflowed ~prior/round → re-grow + repeat → unbounded free
  storage. Fixed by serializing the probe→put→release per `(tenant, key)` with a memory-bounded sharded
  async lock (1024 shards) in `turbo_v8::handle_put`; distinct keys stay concurrent.
- **Turbo `/v8/artifacts/events` OOM + no global in-flight budget (rt-nuclear verify C4/C5).** The
  telemetry events route inherited the 100 MiB artifact body limit with no concurrency guard, and the
  PUT concurrency cap was per-tenant only (4×100 MiB) with no process-wide ceiling — so concurrent
  100 MiB POSTs (one tenant via events, or N tenants via PUT) could OOM the shared container. The events
  route now has a 64 KiB body limit; a process-wide `Semaphore` (16 permits) is reserved in a
  `FromRequestParts` extractor BEFORE the body is buffered (503 on global saturation).
- **CAS write-vs-delete byte-accounting race (rt-nuclear verify C2).** `AccountingCasHandler::write`
  (reserve→PUT→release) and `::delete` (release HEAD-measured `reclaimed_bytes`) shared no per-key lock
  (the existing per-key lock covered delete-vs-delete only), so racing a delete against an overwrite of
  the same CAS hash released a stale size → `bytes_used` under-count. Fixed by lifting a per-`(tenant,
  hash)` sharded lock (256 shards) into the decorator, held by BOTH `write()` and `delete()`.
- **Native plane & OCI bearer honored a revoked PAT for the full cache/token TTL (rt-nuclear verify
  C3/C6).** A `NativePatGate` cache hit returned `Ok` without re-checking D1 revocation (revoked PAT
  valid up to 60s on the native plane), and the stateless OCI realm bearer (1h) was never re-checked
  against D1 (revoked PAT kept registry r/w up to 60 min). Both windows are now bounded to the
  industry-standard control for cached/stateless credentials: native `VERIFY_CACHE_TTL` 60s→5s, OCI
  `TOKEN_TTL_SECS` 3600s→300s (clients re-auth on 401 transparently; no added hot-path D1 latency).
- **OCI registry reads bypassed ALL quota/billing brakes (rt-nuclear r34 #1/#11).** `oci_quota_gate`
  metered only write methods (`is_write`), so `docker pull` / blob+manifest `GET`/`HEAD` against the
  shared `_oci` container were unmetered free egress AND evaded the monthly request-count cap. Reads
  carrying a verified HMAC bearer now increment the monthly request-count gate (fail-OPEN, so no false
  402s on availability); the per-request `$`-ceiling stays write-only, consistent with the native read plane.
- **OCI `/token` Argon2id verify had no concurrency bound → OOM griefing of the shared container
  (rt-nuclear r34 #2).** A flood of concurrent `GET /token` with a valid PAT fanned out unbounded 64-MiB
  Argon2id allocations on the single shared `_oci` container (registry outage for all tenants).
  `PatVerifier::verify_capability` now acquires a process-wide bounded `Semaphore` permit (sized to the
  container RAM / `m_cost` budget) before BOTH blocking Argon2id paths (the hot verify and the
  constant-time dummy-burn); a forged token is shed by the cheap HMAC fast-reject BEFORE any permit is
  taken, and overload fails CLOSED (denial, never a bypass).
- **A present-but-malformed `PAT_SIGNING_KEY` rotation sibling silently disabled the native Argon2id
  backstop fleet-wide (rt-nuclear r34 #7).** A malformed `PAT_SIGNING_KEY_PREV`/`_NEW` made
  `PatVerifier::from_env()` return `None`, which mounted the native CAS/AC/Bazel/Turbo planes WITHOUT the
  only container-side possession check. The container now fails CLOSED at startup: in prod (D1 +
  `PAT_SIGNING_KEY` present) a `None` gate is fatal (`process::exit(1)`), and the Worker's verify-key
  assembly raises a loud config error on a present-but-malformed sibling instead of silently dropping it
  (Worker/container symmetry).
- **Turbo storage byte-accounting bypass via opaque-key overwrite (rt-nuclear r34 #3/#4/#5/#6).** Turbo
  artifact keys are opaque and never content-verified, but the `#25` idempotent-rollback keyed "durable"
  on KEY EXISTENCE, so re-PUTting an existing key with a larger body rolled back the FULL byte reservation
  → unbounded R2 storage at `bytes_used ≈ 0` (the per-tenant storage cap became inert). `CasWriteStore::write`
  now returns the prior object's size (`Option<u64>`) and the route reconciles the true on-disk DELTA
  (release the prior size, not the new size) — correct on grow / shrink / same-size / fresh-insert; a
  presence-probe error fails CLOSED (charges the full new bytes).
- **Per-blob CAS-erase had NO legitimacy gate → leaked-key cross-tenant deletion + permanent 410 poison
  (rt-nuclear r34 #8/#9).** `POST /_internal/cas/:tenant/:hash/erase` gated only on the internal-auth key,
  so a leaked key could irreversibly erase + permanently 410-tombstone ANY tenant's blobs (the #18/#19 fix
  hardened only the DSR mass-erase leg). The route now requires a `dsr_id` and runs the SAME D1
  `dsr_requested` legitimacy pre-check as the mass-erase leg — both legs share ONE `D1DsrLegitimacyStore`
  (single source of erasure-authz truth, no drift) — fail-CLOSED (403 when no live row, 503 on D1 fault);
  the route refuses to mount without the legitimacy store. **Breaking:** callers of the per-blob erase route
  (e.g. `clw` D-1) must now send a `dsr_id` backed by a live `dsr_requested` row.
- **Bazel `findMissingBlobs` was a full-GET + full-rehash existence probe → 4096× R2-egress/CPU
  amplification (rt-nuclear r34 #10).** The REAPI missing-blobs probe downloaded and re-hashed every
  candidate blob merely to test existence (~40 GiB egress + 40 GiB SHA-256 per ~$4 metered, repeatable).
  It now uses a HEAD existence probe (`CasReadHandler::exists` via `head_size` — no body, no rehash), the
  SOTA REAPI behavior.
- **`/_internal/dsr/erase` trusted the body-asserted `tenant_id` → shared-internal-key GDPR mass-erase
  (rt-nuclear #18/#19).** The 12-backend erasure orchestrator's docstring promised a tenant pre-check but
  never implemented it, so possession of the shared internal-auth key alone could erase ANY tenant's entire
  dataset by asserting a forged `tenant_id` in the request body (GDPR Art. 17 mass-erase / cross-tenant
  destruction). `process_erasure` now runs a legitimacy pre-check BEFORE the `started.v1` audit emit and
  BEFORE any backend fan-out: it binds the erase to a durable, D1-authenticated `dsr_requested` row
  (`migrations/d1/0069`) matching `(dsr_id, tenant_id)` with `status IN ('requested','verified')` — a row the
  legitimate Clerk `user.deleted` path always writes with a D1-authenticated tenant, and a forged request
  never has. New `DsrLegitimacyStore` trait (in-memory + allow-all-test + failing fixtures in
  `corelink-privacy-erasure-worker`; D1-backed `D1DsrLegitimacyStore` over `dsr_requested` in the container)
  injected via the new `InMemoryErasureWorker::try_new_with_legitimacy`. Fail-CLOSED on BOTH absence and
  store error (a D1 fault → `Rejected`, never erase — an irreversible op must DENY on ambiguity). No
  `started.v1` and no tombstone are emitted on the reject path (no fan-out, no state mutation); the SEV-1
  signal is the `Rejected` decision arm. The container route uses the D1 store on the configured path and an
  empty (fail-CLOSED) in-memory store on the unconfigured/placeholder path — never an allow-all store.
- **OCI writes bypassed the monthly request-count quota (rt-nuclear #8, request-count half).**
  PR #318 closed the OCI `$`-ceiling bypass but the SIBLING gap remained: OCI billable writes were never
  counted against the per-tenant monthly request cap (`monthly_request_counts`, migration 0071), because
  that metering is Worker-edge-only and the Worker forwards `/v2/*` + `/token` RAW (returning before its
  `checkRequestQuota` block, and stripping `x-corelink-tenant-id`). New container-side
  `request_count::RequestCountGate` (a Rust mirror of `worker/src/lib/quota.ts::checkRequestQuota`: the
  same atomic increment-and-check UPSERT, the same per-tier caps, the same fail-OPEN posture, 429 +
  Retry-After over the cap) is wired into the OCI router and metered per write method (PUT/POST/PATCH),
  keyed on the SAME verified-HMAC-bearer tenant `oci_quota_gate` already resolves for the `$`-ceiling
  (never a request header — the Worker strips it, and a write with no valid bearer is 401'd by the data
  plane, so it is left unmetered). `None` in dev/CI without a D1 storage env, mirroring the `$`-ceiling gate.
- **Worker-edge D1 cost-amplification on over-quota tenants (rt-nuclear #24).**
  The Worker quota pipeline ran `getTierForTenant` + `checkStorageQuota` BEFORE the cheap monthly
  request-count check, so a $-ceiling-capped / over-quota tenant re-paid the full storage-SUM D1 read
  on every request all month. The pipeline now does the single atomic monthly-counter UPSERT FIRST
  (`incrementMonthlyRequestCount`) and, once a tenant is over even the lowest tier cap (FREE = 500K/mo),
  rejects with 429 via the resolved-tier cap comparison BEFORE the storage-SUM read — skipping that read
  on the doomed path. Counting happens exactly once (no double increment); under-cap requests keep the
  same gates and the same order of the rest. `checkRequestQuota` is preserved (now a thin wrapper over the
  new `incrementMonthlyRequestCount` + `requestCapResultForCount` split); paid tenants over 500K still get
  full headroom.
- **DSR/erase internal-auth key mismatch in the full-split config (rt-nuclear #23).**
  Server PR #317 wired the container's DSR + CAS-erase surfaces to `erase_auth_key_from_env()`
  (`CORELINK_ERASE_AUTH_KEY`, shared-key fallback), and the main Worker's `/_internal/dsr/*` gate already
  resolves the erase consumer key the same way. The signup-worker — the live driver of the GDPR erasure
  path (`Clerk user.deleted` → queue → `/_internal/dsr/erase`, plus the 24h verify cron) — routes its
  calls THROUGH the main Worker but injected only the shared `CORELINK_INTERNAL_AUTH_KEY`. The moment a
  dedicated `CORELINK_ERASE_AUTH_KEY` is provisioned (the intended full-split config), the main Worker's
  erase gate would 401 those calls and silently break BOTH erase surfaces. The signup-worker now resolves
  the erase key erase-first / shared-fallback (new `resolveEraseAuthKey`, mirroring the container + main
  Worker) on both the erase consumer and the verify cron, so the keys agree in every config.
- **Turbo PUT charged storage bytes on every idempotent re-write (rt-nuclear #25).**
  The Turbo bridge's `CasWriteStore::write` returned `()`, so the turbo_v8 route accrued the body bytes
  on EVERY PUT — a CI cache re-pushing the same content-keyed artifact (the common case) double-charged
  storage on each re-run. `CasWriteStore::write` now returns `Result<bool>` (true = new key, false =
  overwrite), threaded through `TurboPutResponse::durable`; the route rolls back the byte reservation
  when `durable == false`, mirroring the `AccountingCasHandler` `durable` contract. The R2-backed store
  probes presence before the PUT (fail-CLOSED to durable on a probe error — never under-charge).
- **Concurrent double-DELETE over-released storage bytes (rt-nuclear #6/#10/#14).**
  The CAS and AC delete handlers measured the blob size with a HEAD and then issued a separate idempotent
  `DeleteObject`. Because S3 `DeleteObject` reports neither prior presence nor prior size, two concurrent
  deletes of the same key BOTH HEAD the size and BOTH report it reclaimed — the byte accountant then
  released the bytes twice, manufacturing free storage headroom (a quota-bypass primitive). Both planes
  now go through a new `R2S3Client::delete_if_present`, which serializes the measure-and-delete under a
  per-key in-process async lock and returns the reclaimed size to AT MOST ONE racer (`Some(size)`); every
  other racer HEADs the key absent and gets `None` → releases 0. The release now reflects what THIS
  request actually removed.
- **R2 CAS write always reported `durable=true` → byte double-charge on idempotent re-write (rt-nuclear #13).**
  `R2CasHandler::write` returned `CasWriteResponse::new(hash, true)` unconditionally, so every re-write of
  an already-stored content hash was reported as a fresh durable insert. The `AccountingCasHandler`
  decorator charges the bytes on the reservation and only rolls them back when `durable == false`, so an
  idempotent re-write was charged a SECOND time — a tenant could inflate (or, symmetrically, a churning
  client could drift) `bytes_used`. The CAS write now HEADs the content-addressed key before the PUT
  (mirroring the AC update's GET-and-compare): an already-present blob skips the re-PUT and returns
  `durable=false` (HEAD error fails CLOSED to the PUT, never dropping a write), so the decorator does not
  re-charge.

### Fixed
- **Storage cap frozen at first-write, never reseeded on tier downgrade (rt-nuclear #16).**
  `D1ByteStore::check_and_accrue`'s `ON CONFLICT DO UPDATE` updated `bytes_used`/timestamps but NOT
  `bytes_quota`, so a tenant whose tier was DOWNGRADED kept the old (higher) cap forever — the new
  lower cap never took effect and the tenant could keep storing past their entitlement. The conflict
  branch now RECONCILES `bytes_quota` to the incoming authoritative cap when it is a real finite value
  (`COALESCE(NULLIF(?5,0), …existing)`, so a genuinely-unlimited `Some(0)` carrier never clobbers a
  finite stored cap), and gates the write by the effective new cap so the downgrade is enforced on the
  very next write. The in-memory test store mirrors the same semantics.

### Security
- **npm/pip/brew cache adapters had NO container-side $-ceiling gate (rt-nuclear #22).**
  Unlike cargo and OCI, the npm/pip/brew adapter gates enforced cache scope + F27 write capability but
  did NOT charge the per-tenant monthly `$`-ceiling, so a tenant over its billing ceiling could keep
  driving cache ops on those surfaces (cost-control bypass). Each gate now carries the same optional
  `QuotaGate` cargo/OCI use and charges the flat per-op cost (server-trusted `x-corelink-tenant-id`
  cost-attribution, missing/empty skips fail-OPEN) after the scope/F27 checks and before the adapter
  runs — 402 over-ceiling / 503 fail-CLOSED.
- **Audit export/analytics had no Argon2id PAT-possession backstop → leaked PAT_SIGNING_KEY = cross-tenant audit exfil (rt-nuclear #17).**
  The `/v1/audit/export` and `/v1/audit/analytics/*` surfaces trusted the Worker-resolved tenant header
  without re-verifying PAT possession, so a leaked `PAT_SIGNING_KEY` (which lets an attacker HMAC-forge a
  bearer) could read any victim tenant's audit log / analytics. These routes now carry the SAME optional
  `NativePatGate` the native CAS/AC/Bazel/Turbo states use: when wired (prod), each handler re-runs the
  full Argon2id Option-B verify of the bearer against the authenticated tenant AFTER the scope+tenant
  gate and BEFORE any data access (401 forged/wrong-tenant, 503 verifier fault); `None` in dev/CI.
- **OCI in-flight byte ceiling was global-only → one tenant could starve all (rt-nuclear #3/#12).**
  The OCI blob-upload path enforced only a GLOBAL 512 MiB in-flight ceiling, so a single tenant could
  fill the entire ceiling (its session cap × layer size easily exceeds it) and `429` every other
  tenant's pushes — a cross-tenant availability DoS. `append_chunk` now ALSO reserves each chunk against
  a per-tenant byte budget (`OCI_MAX_INFLIGHT_BYTES_PER_TENANT = 1/8` of the global = 64 MiB), checked
  atomically under the `tenant_inflight` lock; a chunk over a tenant's own slice is rejected 429 (and
  rolls back its global reservation). The per-tenant counter is released on finalize / cancel / failed
  append / idle-reap, exactly like the global counter.
- **Native CAS/AC write planes had no per-tenant pre-buffer concurrency cap (rt-nuclear #11).**
  The native `PUT /v1/cas/:tenant/:hash` and `PUT /v1/ac/:tenant/:digest` handlers buffered the full
  request body into heap before any gate ran and, unlike the Bazel REAPI surface, had NO per-tenant
  concurrency limit — so a single authenticated tenant could open N concurrent PUTs and consume
  N × body-limit of heap (memory-exhaustion DoS). Both planes now reserve a per-tenant in-flight slot
  via a `FromRequestParts` extractor (`CasPutGuard` / `AcPutGuard`) declared AHEAD of `body: Bytes`,
  mirroring the proven `bazel_v2::BazelPutGuard`: a tenant already at the limit (`CAS/AC_WRITE_
  CONCURRENCY_LIMIT = 8`) is rejected `429 Too Many Requests` BEFORE the body is read, fail-CLOSED on
  a missing/sentinel tenant; the RAII slot releases on every return path.
- **npm metadata cache-poisoning via name normalization collision (rt-nuclear #7).**
  Unscoped npm package metadata is cached in the SHARED cross-tenant `_public` namespace keyed by the
  **normalized** name (`trim().to_ascii_lowercase()`), but the upstream fetch used the **raw** path
  name and never checked that the fetched JSON's canonical `name` matched the requested package. A
  read-only PAT (any tenant) could `GET /npm/<t>/<RawName>` where `<RawName>` normalizes to a popular
  package's `_public` key yet resolves upstream to different-identity content — poisoning every
  tenant's view of that package for the TTL (wrong versions / `dist.shasum` → broken or pinned
  installs). The metadata refresh now **binds the stored content's canonical `name` to the requested
  key** (`require_metadata_name_matches`, fail-CLOSED → `502 MetadataNameMismatch`, mirroring the
  tarball integrity-mismatch contract), and the cache-hit path **self-heals** (a name-mismatched
  entry is re-fetched, not served). Tarball bytes were already per-tenant + SHA-verified, so this was
  an integrity/availability attack, not RCE.
- **Read-only PAT could self-escalate to read-write via divergent scope matching (rt-nuclear #15).**
  The self-serve key-mint escalation gate (`routes::customer::mint_requests_write`) used exact-token
  matching while the D1 scope persister (`customer_d1::map_requested_scopes`) used substring matching
  (`s.contains("write")`). A scope token like `"writes"` / `"cache:write-x"` was therefore FALSE for
  the gate (so a read-only caller's scope was never checked) yet TRUE for the persister (so it stored
  a genuine `read-write` PAT) — letting any read-only credential mint itself a full read-write one.
  Both now share **one exact-token classifier** (`scope::classify_requested_scopes`, the single source
  of truth) and **unrecognized scope tokens are rejected (fail-CLOSED)** rather than silently mapped to
  a privilege.
- **OCI $-ceiling gate now resolves the tenant from the verified bearer (rt-nuclear #2/#8/#9).**
  The OCI push surface bypassed the per-tenant monthly $-ceiling entirely: the Worker strips
  `x-corelink-tenant-id` on the OCI pass-through, and `oci_quota_gate` keyed the charge on that
  (always-empty) header — so every billable OCI write (manifest/blob-upload/finalize) skipped the
  ceiling, letting any Free-tier PAT drive unbounded backend cost (margin attack). The gate now
  recovers the cost-attribution tenant by **HMAC-verifying the realm bearer** (`oci::auth::verify`,
  the same token the data plane checks) — never a forgeable header/claim. A write with no valid
  bearer is left uncharged because the data plane 401s it (no billable work). (The per-tenant OCI
  byte ceiling #3/#12, the `/token` Argon2id cost #4, and the Worker-edge request-count for OCI #8
  are tracked OCI follow-ups.)
- **Wire CAS-erase + DSR to the dedicated `CORELINK_ERASE_AUTH_KEY` (rt-nuclear #18–21).**
  The #297 per-consumer-key split never reached the destructive surfaces: `/_internal/cas/*/erase`
  was seeded from the ADMIN key and `/_internal/dsr/*` read the shared `CORELINK_INTERNAL_AUTH_KEY`
  directly — so an admin-key leak could drive irreversible erases and the GDPR mass-erase surface
  honored no dedicated key. Both now resolve `CORELINK_ERASE_AUTH_KEY` (via `erase_auth_key_from_env`),
  making the #297 split real on the erase plane. Non-breaking: still falls back to the shared key until
  the per-consumer secret is provisioned; ≥32-char fail-CLOSED floor preserved. (Removing the shared
  fallback for destructive consumers + per-tenant authz on erase/DSR are tracked follow-ups.)

### Added
- **Bazel REAPI v2 CAS — SHA-256 in a surface-tagged keyspace (concern D).**
  Genuine `bazel --remote_cache` uploads (SHA-256 content addressing, REAPI v2
  default) no longer fail the BLAKE3-only durable gate. The fix is **Option A**
  Genuine `bazel --remote_cache` uploads (SHA-256 content addressing, REAPI v2
  default) no longer fail the BLAKE3-only durable gate. The fix is **Option A**
  (surface-tagged keyspace), NOT a relaxation of the shared content-addressing
  gate:
  - Native CAS + sccache stay **BLAKE3-only**; Bazel blobs are stored under a
    **schema-versioned key prefix** `<region>/<tenant_prefix>/bazel/sha256/<digest>`
    — the "new type + schema-versioned prefix" path ADR-0044 §5 already documented.
    The two functions **never mix within a keyspace**.
  - The durable content-hash gate is **surface-partitioned**: it verifies the
    keyspace's canonical function (BLAKE3 native / SHA-256 Bazel) on **both write
    and read** (bitrot re-verify), selected by an explicit `DigestAlgo` enum
    threaded through `CasReadRequest`/`CasWriteRequest` — **never inferred from the
    hash string length** (which would be a silent gate). `corelink-hash::Digest`
    (the native sealed BLAKE3 newtype) is untouched.
  - The REAPI boundary (`routes/bazel_v2.rs::handle_cas_write`) verifies the
    client SHA-256 against the body before delegating (early clean **422** on
    mismatch; defense-in-depth with the durable gate).
  - GDPR Art.17 full-tenant erasure already covers the new keyspace (it is
    prefix-wide under `<region>/<tenant_prefix>/`); `INV-CAS-INTEGRITY` /
    `INV-CAS-IDEMPOTENCY` updated to record the surface-determined `hash_fn`.
- **`clw auth rotate` — atomic PAT rotation (`POST /internal/v1/auth/rotate`).**
  A new internal-auth-gated Worker route closes the `clw auth rotate` stub (which
  previously advised re-login and returned `rotated:false`). It rotates a PAT in
  one call — **mint an equivalent new PAT for the same tenant + revoke the old** —
  with no re-login:
  - Gated by the per-consumer `CORELINK_PAT_MINT_AUTH_KEY` (with fallback to the
    shared `CORELINK_INTERNAL_AUTH_KEY`, the #297 per-consumer-key pattern) — the
    clw backend holds the key; an end-user PAT cannot call it.
  - Reads the **old `pat` row** (`tenant_id`, `scope`, `expires_ms`,
    `revoked_at_ms`) by `pat_id`, so the new PAT inherits the old PAT's **tenant +
    scope exactly**. An unknown or already-revoked `pat_id` → **404** (never
    silently mints); a scope the single mint authority cannot reproduce (e.g.
    `read-only`) → **422** (never escalates).
  - **Mints NEW first, revokes OLD only after the mint succeeds** — so a mint
    failure never leaves the caller with zero valid PATs. The mint REUSES the
    single mint authority (`mintScopedPat` → the container's audited
    `/_internal/pat/mint` via the `_system` DO) — no second mint path, no new
    signing key. The revoke REUSES the existing idempotent
    `UPDATE pat SET revoked_at_ms ... WHERE revoked_at_ms IS NULL` surface
    (INV-PAT-REVOKE-PROPAGATION). The principal is derived `SHA-256(old pat_id) →
    UUID` for stable per-key audit correlation.

  Fully fail-CLOSED (missing secret → deny, bad body → 400, unknown/revoked pat →
  404, mint failure → propagate without revoking) and handled AT the Worker (a
  fresh server-trusted request to the DO, so client trust headers can never reach
  the mint route) — the same posture as `/internal/v1/runner/mint`.
- **D-9 — per-job runner PAT mint + revoke (corelink-runners seam).** Two new
  internal-auth-gated Worker routes let the trusted dispatcher provision a
  disposable runner with a cache credential without giving the runner a Clerk
  session or a bootstrap secret:
  - `POST /internal/v1/runner/mint` mints a short-TTL (5400s = 90-minute job
    hard-cap + margin), tenant-scoped `cas:rw` PAT. It is gated by the per-consumer
    `CORELINK_PAT_MINT_AUTH_KEY` (with fallback to the shared
    `CORELINK_INTERNAL_AUTH_KEY`, the #297 per-consumer-key pattern), then checks
    the dedicated `runners_entitlement` table (migration 0070) — a tenant with no
    row is **not entitled to Runners** (403), a SEPARATE authorization axis from
    the cache tier. The principal is derived `SHA-256(job_id) → UUID` for per-job
    audit correlation. `admin`/`owner` scope is **refused** (least privilege). The
    mint REUSES the single mint authority (`mintScopedPat` → the container's
    audited `/_internal/pat/mint` via the `_system` DO) — no second mint path and
    no new signing key — so the per-principal mint throttle rate-limits runaway
    runner-mint automatically.
  - `POST /internal/v1/runner/revoke` revokes a runner PAT by `pat_id` for the
    dispatcher's job-teardown (the TTL is the backstop), REUSING the existing
    revocation surface (the idempotent `UPDATE pat SET revoked_at_ms` write on the
    shared `pat` table that the customer revoke route performs;
    INV-PAT-REVOKE-PROPAGATION).

  Both paths are fully fail-CLOSED (missing secret/binding → deny, bad body → 400,
  not entitled → 403) and handled AT the Worker — the handler builds a fresh
  server-trusted request to the DO, so client trust headers can never reach the
  mint route (the same posture as `/internal/v1/auth/token-exchange`).

### Security
- **Storage-quota fail-OPEN on a fresh/unsynced tenant — fresh-row cap source (cluster N4).**
  The container's storage byte-accounting reservation
  (`crates/corelink-container/src/byte_accounting.rs`) seeded a **fresh**
  `tenant_storage_state` row with `bytes_quota = 0`, and its cap-check treats
  `0` as **UNLIMITED** — so a brand-new (or not-yet-synced) tenant was **uncapped**
  until some external sync wrote the real cap, and `0` was conflated with the
  genuine enterprise-unlimited sentinel. A fresh row is now seeded with the
  tenant's **real per-tier storage cap**, sourced from the quota-resolution
  authority (the Worker): on every data-plane write-forward the Worker injects the
  resolved cap as a new **server-trusted** header `x-corelink-storage-quota-bytes`
  (value = `QUOTAS[tier].storageBytesMax`; genuine-unlimited tiers send `"0"`),
  added to `stripClientTrustHeaders` so a client can never forge it (sole-setter,
  exactly like `x-corelink-tenant-id`). The container threads the cap to the
  reservation, which seeds the fresh row's `bytes_quota` with it; `0` is now
  reserved for genuine-unlimited ONLY. When the row is missing AND no cap is
  available the reservation **fails CLOSED** (503) — absence is never treated as
  unlimited (matching the canonical billing crate's missing-row posture). Existing
  rows (already seeded with a real cap) are unaffected.
  (`worker/src/lib/quota.ts` `storageQuotaHeaderValue`, `worker/src/index.ts`
  inject+strip; `crates/corelink-handler-cas` + `corelink-handler-ac`
  `with_storage_quota_bytes`; `corelink-bazel-bridge` `WriteCtx`; container
  `cas.rs` / `ac.rs` / `bazel_v2.rs` / `turbo_v8.rs` thread the header. Regression:
  Rust fresh-capped over/under-cap, genuine-unlimited, indeterminate fail-closed +
  decorator nets; worker vitest cap-injected + strip-list.)
- **Cluster D/E/G — fail-closed request quota, error-string scrub, mint rate limit (cycle-2 nuclear red-team).**
  - **Cluster D — monthly request-count quota failed OPEN in prod.** The Worker gated
    enforcement on `REQUEST_QUOTA_ENABLED === "true"`, which is unset in production, so
    the contracted per-month request cap was never enforced. The gate is now fail-CLOSED:
    enforcement is ON by default and disabled only by the explicit opt-OUT kill-switch
    `REQUEST_QUOTA_DISABLED === "true"` (dev/test). A missing prod env var keeps the cap
    live (`worker/src/index.ts`, `worker/src/lib/quota.ts`; vitest: request quota
    default-on / fail-closed). **Owner op: no env var to set in prod — leaving
    `REQUEST_QUOTA_DISABLED` unset is the enforced state; ensure it is NOT set to "true"
    in prod.**
  - **Cluster E — raw backend error strings + storage topology leaked into public HTTP
    responses.** The adapter + OCI error paths interpolated the inner backend `String`
    (raw Cloudflare D1 API errors incl. status/body and possibly SQL; R2 storage
    topology; the derived per-tenant R2 prefix; upstream host/transport) into the
    client-facing body. **A24 (unauth-reachable):** the OCI `/token` PAT-verify backend
    fault leaked the raw CF D1 error into the public 401. **A27/A28/A29:** the
    cargo/brew/npm/pip + OCI error envelopes surfaced the same internals verbatim. Every
    public-facing error path now returns an opaque, class-keyed message + a correlation
    `ref` (request id); the real detail is logged server-side only via `tracing::error!`.
    Existing REAPI/OCI envelope SHAPES + status codes are preserved — only the
    `message`/body content is scrubbed; safe variants (digest/integrity mismatch,
    oversized, not-found) keep their actionable messages (`crates/corelink-adapter-host`
    `{oci,cargo,brew,pip,npm}/error.rs` + `*/server.rs`).
  - **Cluster G — `/_internal/pat/mint` had concurrency but no RATE limit.** #297 added
    a concurrency semaphore, but an internal-auth holder firing SERIAL mints stayed under
    the concurrency cap while pinning Argon2id CPU/RAM indefinitely. A process-global
    fixed-window RATE limiter (`MintRateLimiter`, default 60 mints/min, env-tunable via
    `PAT_MINT_MAX_PER_MINUTE`) now bounds mint throughput; over-rate ⇒ 429. Checked after
    the auth gate (unauth floods shed at 401 first) and before the concurrency permit /
    Argon2id work. In-memory, per-container, resets on restart (documented). The
    concurrency semaphore is kept (`crates/corelink-container/src/routes/internal_pat.rs`).
- **Storage byte-accounting: sibling write surfaces + reserve-before-commit (cycle-2 nuclear red-team, clusters B/C/F).**
  - **Cluster B — byte accounting only covered the native plane.** Bazel REAPI, OCI,
    and the cargo/brew/npm/pip language adapters all drive the SAME shared
    `CasWriteHandler`/`AcUpdateHandler` trait objects but never accrued bytes, so a
    Free tenant could store unbounded TB at $0 via those planes; OCI also bypassed
    the `$`-ceiling quota path entirely. Byte accounting is now enforced at that
    single chokepoint by the new `byte_accounting::AccountingCasHandler` /
    `AccountingAcHandler` decorators (wrapping the write+delete trait objects in
    `routes::build_with_factory`), so native CAS/AC, Bazel, OCI, and every adapter
    inherit identical accounting. OCI write methods (PUT/POST/PATCH) are now also
    charged against the per-tenant monthly `$`-ceiling via an `oci_quota_gate` layer.
  - **Cluster C — accrue-after-commit race + dead release.** Accrual ran AFTER the
    R2 PUT with no pre-reservation (two concurrent writes could both pass the cap;
    an over-cap blob was durably committed before the 402) and deletes never
    decremented `bytes_used`. The decorators now **reserve → commit → release**:
    the atomic single-statement D1 UPSERT runs BEFORE the R2 PUT (over-cap ⇒ 402
    with NO blob written; accounting fault ⇒ 503 fail-CLOSED), an idempotent
    re-write or a failed inner write rolls the reservation back, and deletes
    `release` the reclaimed bytes (CAS/AC delete responses now carry
    `reclaimed_bytes`, sourced from a pre-delete R2 `HeadObject`). Turbo (its own
    `R2KvStore`, not the shared handler) was converted to reserve-before-commit at
    its route handler. Regression tests prove: two concurrent over-cap reservations
    cannot both pass, an over-cap write leaves no blob, and a delete decrements the
    counter.
  - **Cluster F — no concurrency cap on Bazel writes.** The Bazel CAS/AC write path
    buffered the full ~10 MiB body before any gate with no per-tenant concurrency
    cap. Added a `BazelPutGuard` `FromRequestParts` extractor (mirrors the Turbo
    `PutConcurrencyGuard`) that reserves a per-tenant slot BEFORE the body is
    buffered and rejects the over-cap write 429.
- **Red-team data-plane bundle (brutal red-team #1/#2/#4).**
  - **#1 (HIGH) — storage quota was structurally inert.** Nothing on the container
    data plane ever incremented `tenant_storage_state.bytes_used`, so per-tier
    storage caps never tripped (a Free tenant could store unbounded TB at $0). New
    `byte_accounting::ByteAccountant` does an ATOMIC check-and-accrue UPSERT against
    `tenant_storage_state` (`INSERT … ON CONFLICT(tenant_id, region) DO UPDATE SET
    bytes_used = bytes_used + ? WHERE bytes_quota = 0 OR bytes_used + ? <=
    bytes_quota RETURNING bytes_used`) wired into the CAS/AC/Turbo write handlers
    (over-cap ⇒ 402, transport fault ⇒ 503 fail-CLOSED), plus a saturating
    `release` for deletes. Env-gated (`None` in dev/CI), mirroring the `QuotaGate`.
  - **#2 (HIGH) — Turbo PUT buffered up to 100 MiB BEFORE the concurrency cap.**
    The per-tenant in-flight reservation lived inside `handle_put`, AFTER the
    `body: Bytes` extractor, so a burst of concurrent PUTs each buffered ~100 MiB
    before the cap-check ran. Converted to a `PutConcurrencyGuard`
    `FromRequestParts` extractor declared AHEAD of the body extractor, so a 5th
    concurrent PUT is rejected 429 BEFORE any body byte is read; the RAII `PutSlot`
    releases the slot on drop.
  - **#4 (HIGH) — native plane proved possession with HMAC only.** A leaked
    `PAT_SIGNING_KEY` could forge any tenant's PAT (the random secret, stored only
    as an Argon2id hash, was never checked on the native path). New
    `native_pat_gate::NativePatGate` re-runs the full Option-B verification (the
    shared `adapter_pat::PatVerifier` — Argon2id against the stored `pat_hash` +
    tenant binding) at the top of each billable CAS/AC/Bazel/Turbo handler, with a
    short-TTL verified-token cache keyed by SHA-256 fingerprint so the hot path
    skips Argon2id. Defense-in-depth ON TOP of the existing HMAC gate; env-gated.
- **Red-team nuclear cluster A (CRITICAL) — customer control plane + `/v1/users/me`
  were UN-gated by the `#4` possession backstop.** PR #297 wired
  `native_pat_gate::NativePatGate` onto the native CAS/AC/Bazel/Turbo planes but left
  `/v1/customer/*` and `/v1/users/me` without it. Because the Worker proves PAT
  possession with an HMAC-only fast check, a leaked `PAT_SIGNING_KEY` let an attacker
  HMAC-forge a PAT for ANY victim tenant, reach `customer.rs` with no possession check,
  and have `handle_keys_create` mint a GENUINE `cas:rw` PAT for the victim (durable,
  survives key rotation) → cross-tenant takeover + shared-cache poisoning. Separately
  the control plane enforced no scope gate, so a read-only PAT could self-mint a
  read-write PAT. Fix: thread the SAME `NativePatGate` into `CustomerRouteState` and a
  new `UsersRouteState`, and run the full Argon2id Option-B verify (bound to the claimed
  tenant) at the TOP of every `/v1/customer/*` handler and `/v1/users/me` BEFORE any
  storage/handler access — forged/wrong-tenant ⇒ 401, verifier fault ⇒ 503 (fail-CLOSED).
  Clerk-session callers (`x-corelink-token-prefix: clerk`, edge-verified, no bearer) skip
  the PAT check; `None` in dev/CI preserves current behavior. Added a scope gate on mint:
  a read-only principal (`x-corelink-scope` lacking cache-write) requesting a write/admin
  credential — or inviting a privileged `Owner`/`Admin` team role — is rejected 403 before
  the mutation (mirrors the native write-scope gate).
- **Red-team brutal #3/#6/#7 — control-plane hardening (container).**
  - **#3 (HIGH)** — a single `CORELINK_INTERNAL_AUTH_KEY` gated five high-privilege
    internal surfaces (any-tenant PAT mint, GDPR/CAS erase, admin, pilots); one leak
    granted all. Introduced per-consumer keys via additive fallback
    (`resolve_internal_auth_key` in `routes/admin.rs`): mint reads
    `CORELINK_PAT_MINT_AUTH_KEY`, admin/pilots read `CORELINK_ADMIN_AUTH_KEY`, erase
    reads `CORELINK_ERASE_AUTH_KEY`, each falling back to `CORELINK_INTERNAL_AUTH_KEY`
    when unset/blank/< 32 chars; both absent ⇒ `None` ⇒ fail CLOSED (403), unchanged.
    Deployable before prod secrets exist (mirrors the #8 OCI dual-name pattern).
  - **#6 (LOW)** — a brand-new tenant's FIRST billable op bypassed the monthly
    `$`-ceiling (the fresh-row path in `tenant_quota.rs` called `accrue` unconditionally).
    Added atomic `QuotaStore::seed_checked_accrue` (D1 `INSERT … ON CONFLICT … WHERE
    accrued + delta <= budget RETURNING`) so the first op is ceiling-checked too (402
    when it alone exceeds the cap); no TOCTOU.
  - **#7 (LOW)** — `/_internal/pat/mint` ran an unbounded Argon2id per call. Added a
    process-wide in-flight cap (`MintInflightLimiter`, default 16, env
    `PAT_MINT_MAX_INFLIGHT`); excess concurrent mints shed with `429` (fail-CLOSED).
- **Red-team #3 (worker) — per-consumer internal-auth key split.** The Worker's
  `/_internal/*` gate authenticated every internal surface (PAT mint, admin, erase) with the single
  shared `CORELINK_INTERNAL_AUTH_KEY`, so one leaked secret unlocked all of them. It now mirrors the
  container's just-merged Rust split: a new `resolveConsumerKey` (in `internal_auth.ts`) selects a
  PER-CONSUMER key by path — `/_internal/pat/mint` → `CORELINK_PAT_MINT_AUTH_KEY`, `/_internal/admin/*`
  → `CORELINK_ADMIN_AUTH_KEY`, `/_internal/dsr/*` (and other data-plane internal routes) →
  `CORELINK_ERASE_AUTH_KEY` — each used iff set AND ≥ 32 chars, else falling back to the shared key
  (≥ 32), else fail-CLOSED (403). The same padded `crypto.subtle.timingSafeEqual` compare is retained
  (no length oracle). The new env names are added to the Worker `Env` interface. **OPERATOR (launch
  step):** provision the three new secrets via `wrangler secret put` per env to complete the split;
  until then the shared-key fallback preserves current behaviour. (`scripts/secrets-mvp-allowlist.txt`
  needs the three new names appended.)
- **Red-team #5 — monthly request-count quota is now actually enforced.** `checkRequestQuota` was a
  hard-coded no-op (`requestsPerMonthMax` read by nothing, the `if (!requestCheck.ok)` branch dead, no
  counter table), so the contracted per-month request cap was unenforced. Added migration
  `0071_monthly_request_counts.sql` (`monthly_request_counts(tenant_id, year_month, request_count, …)`,
  PK `(tenant_id, year_month)`). `checkRequestQuota` is now async and does an ATOMIC
  increment-and-check UPSERT (`… ON CONFLICT DO UPDATE SET request_count = request_count + 1 RETURNING
  request_count`), compares the post-increment count to `QUOTAS[tier].requestsPerMonthMax`, and returns
  `429` + `Retry-After = secondsUntilNextMonthStart()` when exceeded. Gated by `REQUEST_QUOTA_ENABLED`
  (off → no counter write); fails OPEN on a D1 error and skips the write for uncapped tiers, matching
  the file's posture. Wired at the real call site in `index.ts` (replacing the no-op). Adds
  `checkRequestQuota` unit tests (under cap → ok; at cap → ok; over cap → 429; D1 error → fail-open;
  uncapped/unconfirmed-tier → ok).
- **CAA-360 #27/#29/#30 — worker auth hardening bundle.**
  - **#27** — `internal_auth.ts requireInternalAuth` compared the shared secret with `ctEqStr`, which
    returned early on a length mismatch (a length oracle). Replaced with the same padded
    `crypto.subtle.timingSafeEqual` gate `index.ts` uses for `/_internal/*`: the provided bytes are
    copied into a fixed buffer sized to the expected secret, one `timingSafeEqual` runs over
    equal-length buffers, then ANDed with a single length-equality bit — no branch depends on the
    provided length. (index.ts's own `ctEqStr` is retained: it is used only for non-secret PAT
    env-segment matching, which carries no oracle.)
  - **#29** — `extractAuth` docstring drift: it documented `SELECT tenant_id, pat_hash, expires_ms …
    WHERE … expires_ms > now_ms`, but the actual query is `SELECT tenant_id, expires_ms, scope …
    WHERE token_id = ?1 AND revoked_at_ms IS NULL` with expiry checked in application code. Docstring
    corrected to match (removes false assurance of a SQL-level expiry filter).
  - **#30** — `clerk_auth.ts` fell back to a weak issuer shape-check (`https` + host contains
    "clerk") when `CLERK_ISSUER_URL` was unset. In `ENVIRONMENT === "production"` it now fails CLOSED
    (issuer pin required) instead of shape-checking, with a loud operator log. **OPERATOR (launch
    step):** set `CLERK_ISSUER_URL` via `wrangler secret put CLERK_ISSUER_URL --env prod` BEFORE the
    next Worker deploy or Clerk session auth will 401 (merge ≠ deploy; the Worker deploys manually).
- **CAA-360 #25 — storage-quota gate fails CLOSED on D1 errors for write verbs.** `checkStorageQuota`
  previously returned `ok:true` (fail-OPEN) on every D1 error path — both the storage-`SUM` query
  `catch` and the unconfirmed-tier (`tierResult.d1Error`) branch — so a D1 outage let a tenant write
  past their storage cap unbounded at the edge. The gate is now **verb-aware**: it takes an
  `isMutating` flag and on a D1 error fails **CLOSED** (`429` + a short `Retry-After`) for byte-adding
  writes (`PUT`/`POST`) while keeping reads available (fail-OPEN), mirroring the residency gate's
  fail-closed posture. The DO's CAS quota FSM remains the deeper net; this closes the edge hole. Adds
  unit tests for read-fail-open / write-fail-closed on both the SUM-error and unconfirmed-tier paths
  (and repairs 5 pre-existing type-broken `checkStorageQuota` tests).
- **CAA-360 #16 — `_inMemoryMintCounts` mint-throttle backstop is now bounded (LRU).** The
  module-scoped per-isolate map in `session_exchange.ts` grew one entry per distinct principal for the
  isolate's lifetime (unbounded-growth / slow-leak under principal churn). Each access now LRU-touches
  the principal (delete-then-set on the insertion-ordered Map) and the least-recently-used entry is
  evicted once the map exceeds `MAX_IN_MEMORY_MINT_ENTRIES` (50 000). The durable D1 counter remains
  the primary throttle, so eviction never opens a hole in the persistent gate.

### Security
- **CAA-360 #5/#20 — tenant_quota cycle-roll TOCTOU eliminated (money path).** The cycle-roll /
  fresh-row path in `QuotaGuard::check` used a read-decide-absolute-`put`: two concurrent ops at the
  monthly-cycle boundary each computed `accrued = cost` and overwrote each other, so only ONE op's
  spend was counted (lost-update → $-ceiling over-admission). The roll now goes through a new atomic
  `roll_if_stale` (a single conditional `UPDATE … SET accrued=0, anchor=now WHERE anchor+cycle<=now`
  — idempotent; a no-op if a concurrent op already rolled) followed by the SAME atomic
  `check_and_accrue` as the steady path; brand-new rows seed via the atomic `accrue` (INSERT … ON
  CONFLICT). No boundary path does a non-atomic absolute write anymore.
- **CAA-360 #14/#18 (completed) — Bazel `findMissingBlobs` charges the monthly $-ceiling for EVERY
  digest in the batch, uncapped.** The batch quota gate previously charged at most
  `BATCH_QUOTA_ITERS_CAP = 64` op-units per request via a per-digest loop, so a batch over 64
  digests (up to the 4096 REAPI cap) was under-charged — a tenant could drive up to 64× more
  backend existence-probe work per accrued dollar than the cost model assumes. The gate now charges
  the full `n × cost` in a single atomic `QuotaGuard::check_batch` statement (one D1 round-trip, no
  iteration cap), via the new `QuotaGate::check_batch`. Adds route tests proving a 150-digest batch
  trips 402 (would have wrongly passed under the 64-cap) and an 80-digest batch under budget
  proceeds.

### Security
- **CAA-360 #9 (completed) — digest-format gate extended to the native CAS routes.** `GET/PUT/DELETE
  /v1/cas/:tenant/:hash` now reject a non-canonical `:hash` (not exactly 64 lowercase hex) with 400
  BEFORE it derives an R2 object key, reusing the shared `is_canonical_digest` validator (the AC
  routes were gated in the prior PR). The enumeration route (no single hash) is not gated.

### Security
- **CAA-360 #9 — strict digest-format gate on the native Action Cache routes.** The
  `:action_digest` path segment on `GET/PUT/DELETE /v1/ac/:tenant/:action_digest` was used to
  derive the R2 object key with no charset/length validation. The handlers now reject any digest
  that is not exactly 64 lowercase hex chars (BLAKE3-256 / SHA-256) with **400** BEFORE it reaches
  storage — defense-in-depth alongside the axum single-segment route (which already blocks
  `/`-based traversal). (CAS-hash defense-in-depth is a follow-up.)

### Fixed
- **CRITICAL — `mintScopedPat` now persists the `pat` D1 row (minted tokens never authenticated).**
  The shared mint chokepoint `mintScopedPat` (`worker/src/lib/session_exchange.ts`) called the
  container's `/_internal/pat/mint`, which COMPUTES a token + its Argon2id hash but — by contract —
  does NOT write the `pat` table; the CALLER must. Only the signup-worker + customer plane did. So
  every token from **`handleSessionExchange` (hugit), `handleTokenExchange` (githugr), and
  `handleRunnerMint` (D-9 runners)** was returned but never persisted → the Worker's `extractAuth`
  (`SELECT … FROM pat WHERE token_id = ?`) found no row → **401, the token never authenticated**
  (confirmed live: minted tokens 401'd identically to bogus ones until the row was hand-inserted).
  `mintScopedPat` now **INSERTs the `pat` row into `CONFIG_DB` using the mint's returned `hash`,
  BEFORE returning the token** — mirroring the signup-worker's `insertPat`. The fix is at the single
  chokepoint, so it repairs all three callers at once:
  - `pat_hash` = the container's Argon2id `hash` (previously received on the wire but discarded);
    a 200 mint with a missing/empty hash now **fails CLOSED (500)** rather than writing a hash-less row.
  - `scope` is **canonicalized to the D1 `CHECK (scope IN ('read-write','read-only','admin'))`**
    set (`cas:rw`/`read-write` → `read-write`, `admin` → `admin`, `read-only` → `read-only`); an
    unmappable scope **fails CLOSED (500)** instead of silently violating the CHECK.
  - `shown_once_token` (a UNIQUE column) is set to the per-pat-unique, non-secret `token_id` with
    `shown_once_consumed = 1` (the token is returned directly, not via the one-time dashboard reveal);
    the raw plaintext is never stored.
  - **Fail-CLOSED ordering:** any INSERT failure (FK to `tenant` absent, UNIQUE/CHECK violation,
    transport error) returns a 500 and the token is **NOT** returned — a token that cannot
    authenticate is strictly worse than an honest error.
- **CAA-360 #6 — real audit-event timestamps across CAS / AC / Bazel REAPI / Turbo.** Every audit
  event on these data-plane routes was stamped `now_ms = 0` (a `0u64` stand-in / `const fn now_ms()
  -> 0`), making the audit log un-orderable and un-correlatable. All 11 sites now use the production
  `SystemWallClock.now_ms()` (the same `WallClock` the admin-pilot routes use). No behavior change
  beyond truthful timestamps.

### Fixed (CAA-360 audit — storage hardening batch)
- **#19 OCI in-flight byte ceiling made atomic.** The 512 MiB cross-tenant upload ceiling used a
  load-check-then-`fetch_add`, so two concurrent `PATCH` appends could both pass a stale read and
  overrun it. Replaced with a `compare_exchange_weak` reserve loop (the reservation IS the credit);
  bytes are released on the session-not-found failure path.
- **#7 Turbo R2 build-failure now fails CLOSED (503), not silent in-memory.** When storage creds are
  present but `R2KvStore` refuses to build, the route previously fell back to the non-durable
  `InMemoryKvStore` silently (fail-OPEN → silent data loss). It now mounts a fail-CLOSED
  `UnavailableTurboHandler` whose every verb returns 503 (mirrors `cas.rs::UnavailableCasHandler`).
- **#21 D1 FK non-enforcement documented.** D1/SQLite doesn't enforce FOREIGN KEYs, so tenant-keyed
  tables (`tenant_quota`, `cas_tombstone`) intentionally omit FK clauses; referential integrity is an
  application invariant (DSR deletes children before the parent tenant; inserts require the tenant to
  pre-exist). Documented in the DSR adapter (no migration change — FK clauses would be inert on D1).

### Security
- **GDPR/DSR erasure completeness — `survey_responses` + `tenant_quota` added to the D1 erase-set
  (CAA-360 audit #4/#11).** Both tables are `tenant_id`-keyed and carried tenant data that a
  Right-to-Erasure (GDPR Art.17) request left behind: `survey_responses` holds NPS/CSAT PII
  (`recipient_hash`); `tenant_quota` is the per-tenant spend ledger. Neither is a legal-retention
  category (distinct from the retained `stripe_*` fiscal records), so both are now erased on a DSR
  and covered by the post-erase `remaining_rows` verify sweep. Regression test pins their presence.
### Fixed (CAA-360 audit — config/hygiene batch)
- **#8 OCI token-key env drift:** prod was deployed with `HUGR_OCI_TOKEN_KEY` but the code
  canonicalized to `CORELINK_OCI_TOKEN_KEY`, failing the OCI route CLOSED in prod. The mount now
  reads the canonical name first and falls back to the legacy `HUGR_OCI_TOKEN_KEY`, so the route
  works regardless (rename the prod secret to retire the fallback).
- **#32 dead constant-time code:** removed a discarded `plaintext.ct_eq(dummy_pt)` in
  `dummy_verify_for_constant_time` — dead code (result unused; `ct_eq` length-short-circuits) that
  provided neither timing equalization nor anti-DCE. The timing pad is the always-run Argon2id
  verify; doc corrected (no change to the constant-time guarantee).
- **#26 tier-list divergence:** documented the intentional asymmetry between
  `auth_introspect::VALID_TIERS` (resolve set, 8 — accepts back-compat `team`/`org`) and
  `admin::TIER_SELECTIONS_TIERS` (settable ladder, 6); settable ⊂ resolvable ⊂ D1-CHECK, cross-referenced.

### Added
- **Runners item-1 — per-tenant `max_concurrency` entitlement (Option B).** Migration
  `0070_runners_entitlement` adds the `runners_entitlement` table (keyed on `tenant_id`,
  `CHECK(max_concurrency > 0)` so absence = no entitlement, never a 0 row), and
  `/internal/v1/auth/introspect` now resolves `max_concurrency` from that entitlement table —
  NOT derived from the cache tier (the ratified Option-B axis). Fail-CLOSED: D1 fault → 503
  (never a guessed cap); absent row → field omitted. Conformance vector updated. Deploy step:
  apply 0070 across the 5 envs + ping the runners TL to flip `FABRIC_AUTH_BACKEND`.
- **OCI registry DoS hardening (audit #5/#6).** Per-tenant open-upload-session cap, a global
  in-flight byte ceiling (512 MiB) checked before buffering each PATCH chunk, a lazy reaper for
  stale sessions, and a 413 on oversized manifests — bounding memory against a malicious or
  runaway `docker push`. The in-flight byte-ceiling rejection now returns **429 + Retry-After**
  on the append path (was a generic 500) with an accurate operator message.
- **Container request rate-limit layer (audit #14/#16).** A per-tenant token-bucket Tower layer
  (`ratelimit_layer.rs`); over-burst → 429 + Retry-After. Availability-first + documented:
  absent/sentinel tenants pass through (handlers fail-closed downstream on auth); a future
  `#[non_exhaustive]` arm fails CLOSED to 429.
- **PAT signing-key rotation overlap.** Multi-key HMAC verification (`corelink-pat`) validates a
  PAT against the current key OR a still-trusted previous key (constant-time over all keys, no
  early return — never leaks which key matched); empty key set fails closed. Single-key callers
  are unchanged (delegate via `slice::from_ref`). TS mirror (`verifyPatHmacMulti`) matches the
  Rust semantics. Enables zero-downtime `PAT_SIGNING_KEY` rotation.

### Fixed
- **admin-ui render-smoke workflow never ran (false-green-defeating false alarm).** The
  `e2e-admin-ui-render` job installed Playwright into `apps/admin-ui/node_modules` but ran the
  smoke script from the repo root, so `import { chromium } from "playwright"` failed
  `ERR_MODULE_NOT_FOUND` before any browser launched — it had never once run green, while spamming
  a `sev-1` "E2E REGRESSION" issue every run. The smoke step now runs with
  `working-directory: apps/admin-ui` (relative script path), so it resolves Playwright and actually
  exercises prod. (Prod was verified healthy throughout — this was a broken harness, not a regression.)
- **Real customer onboarding rejected by Svix-PoP residency mis-derivation (launch-blocker).** The Clerk
  `user.created` webhook is delivered by **Svix** (server-to-server), so `request.cf.colo` is Svix's
  sender PoP — **not** the end-user's location. The signup-worker derived the tenant's data-residency
  region from that colo (`regionFromColo`), so when Svix routed a delivery via a European PoP (observed
  live: `sender-9YMgn` → `weur`) the signup was geo-assigned `weur` and then **rejected** by
  `PROVISIONED_MACROS` (US-only at launch) with a terminal 422 — a legitimate paying signup lost to the
  luck of Svix's routing. The webhook carries no reliable user-geo signal and at launch only enam/wnam
  (IAD) is actually served, so webhook-provisioned tenants now default to the launch-served region
  (`enam`); the Svix colo never drives provisioning. Per-tenant residency selection becomes a deliberate
  post-signup action when the EU/SAM serving build-out lands (residency Phase-2). Also corrects the
  `region_assigned` analytics `source` (`launch_default` when no real user-geo colo is supplied).
  Drive-by: fixed a pre-existing stale `webhook-e2e` idempotency test (the #269 WP-1 change keyed
  idempotency on tenant **AND** a live PAT; the test seeded only the tenant) + taught the in-memory D1
  fake to answer the pat-liveness query.

### Added
- **githugr cross-tenant authz — `/internal/v1/auth/tenant/lookup` (#3) + `/internal/v1/auth/token-exchange` (#1).**
  Two Worker-hosted, internal-auth-gated endpoints that unblock githugr's two audit CRITICALs (the window
  authenticates via the shared Clerk instance; fine-grained authorization is the engine's — ADR-0007).
  Ratified by the githugr GREENLIGHT (2026-06-15).
  - **#3 tenant lookup** — `POST /internal/v1/auth/tenant/lookup` resolves the shared Clerk `sub`
    (clerk_user_id) → `{ tenant_id, role:"owner", tier, tenant_state }` via a parameterized D1 read,
    **fail-CLOSED 404** when no tenant maps. Keyed on `sub` (no `github_id` column — both sides trust the
    same JWT). The email fallback is documented N/A: `tenant.email_hash` is a `SHA-256(clerk_user_id)`
    privacy surrogate, not a raw-email hash, so no email→tenant mapping exists (moot — `sub` is always
    available). githugr persists the returned `tenant_id` as each repo's `owner_tenant`.
  - **#1 RFC 8693 token exchange** — `POST /internal/v1/auth/token-exchange` exchanges
    `(Clerk session JWT + audience)` for a ~300s tenant-scoped `cas:rw` PAT, and **403s when
    `session.tenant ≠ audience`** — the exact cross-tenant-WRITE rejection githugr's engine relies on.
    Triple-gated: internal-auth (githugr backend) AND a valid user session AND audience match. Reuses the
    audited shared Clerk verifier (`lib/clerk_auth.ts`) and the single container mint authority
    (`/_internal/pat/mint` via the `_system` DO) — no second JWT verifier, no second mint path. Admin scope
    is refused (least privilege); the Argon2id hash is never leaked; the session token never leaves the edge.
  - Implemented Worker-side (`worker/src/lib/{internal_auth,tenant_lookup,session_exchange}.ts`,
    `worker/src/index.ts`) because the JWT verifier + D1 binding already live there — each endpoint sits
    where its core dependency is (introspect stays container-side for the Argon2id PatVerifier).

### Fixed
- **session-exchange tests (stale, drive-by):** `worker/tests/session_exchange.test.ts` carried two
  pre-existing reds — a `principal` assertion not updated after F-01 (the response now returns the opaque
  derived UUID, never the raw Clerk id) and two tests polluted by the F20 module-scoped in-memory
  mint-throttle counter (fixed via per-test principals). Product behavior was correct; the tests were
  corrected.

### Security
- **Pre-launch due-diligence audit remediation (82-agent audit 2026-06-15 → NO-GO → launch-blocker fixes).**
  An 82-agent adversarial audit (37 finders × 2-vote refutation) returned 21 confirmed CRITICAL/HIGH;
  this closes the launch-blocking subset (full report: `docs/security/2026-06-15-launch-due-diligence-audit.md`):
  - **signup orphan-tenant (CRITICAL #18):** the Svix-retry idempotency check short-circuited on tenant
    existence alone, so a retry after a mid-provision (post-tenant, pre-PAT) failure ack'd "already
    provisioned" and never re-issued the PAT — a permanent silent PAT-less orphan (a paid signup that can
    never authenticate). Idempotency is now keyed on provisioning being COMPLETE (tenant AND a live PAT);
    a retry re-issues the PAT + re-publishes Clerk metadata.
  - **GDPR salt fail-open (HIGH #19/#20):** `deriveErasureSalt` only fail-closes when `ENVIRONMENT`
    starts with `prod`, but the signup-worker `wrangler.toml` never bound `ENVIRONMENT` → prod silently
    used the predictable SHA-256 fallback salt (pseudonymization-unlinkability breach). `ENVIRONMENT="prod"`
    is now bound.
  - **storage fail-OPEN → fail-CLOSED (HIGH #2/#8/#4):** a non-derivable tenant (non-UUID id, or a
    missing/invalid `R2_TDK_HEX`) on the prod path degraded to an empty/predictable prefix and the op
    PROCEEDED — collapsing such tenants into one SHARED keyspace (cross-tenant read/overwrite/delete/list).
    CAS (`r2_s3.rs`), Turbo (`r2_kv.rs`) and the CAS erasure path (`cas_erase.rs`) now FAIL CLOSED
    (Internal/500, never a degraded prefix); the raw-pad fallback is `cfg(test)`-only; `build_r2_kv_from_env`
    fail-closes when creds are present but the TDK is absent (mirrors CAS/AC).
  - **OCI token-key env-name drift (HIGH #21):** the prod deploy gate + runbook provisioned/verified
    `HUGR_OCI_TOKEN_KEY` while the container reads `CORELINK_OCI_TOKEN_KEY` → the OCI route silently never
    mounted in prod behind a green gate. Reconciled the gate/tooling to `CORELINK_OCI_TOKEN_KEY` (operator
    must re-put the live CF secret under the new name — noted in the allowlist).
  - **admin-ui CSP nonce defeated (HIGH #12/#13):** prod served a Content-Security-Policy whose nonce was
    the hardcoded public literal `nonce-STATIC` (from `next.config.ts`), overriding the per-request
    middleware nonce — so the nonce gave zero XSS protection (`nonce="STATIC"` is reusable by anyone).
    Removed the static CSP; the per-request middleware nonce is now the single source and is set on the
    forwarded request headers so Next applies it to inline scripts.
  - **DPA/Privacy doc truthfulness (HIGH #10/#11):** the DPA sold WEUR/SAM residency (signup rejects them
    — US-only at launch) and BYOK / "CoreLink cannot decrypt your data" / Schrems-II at-rest guarantees the
    launched data plane does not deliver (R2_TDK_HEX is key-prefix derivation, not envelope encryption).
    Docs corrected to the actual launch posture (US-only; BYOK = planned, not yet wired).
  - **test-fixture compile break (D-7/D-8):** `fixture_unavailable()` (cas/ac route tests) was not updated
    when the route state gained `delete`/`list` fields — the crate's test target failed to compile (masked
    on CI by the shared-toolchain `os error 2` outage). Wired the fail-closed Unavailable handlers into both
    fixtures.
  Deferred to post-launch fast-follow (owner-waived 2026-06-15): data-plane rate-limiting, Bazel SHA-256
  digest, OCI DoS bounds, PAT signing-key rotation overlap.
- **CAA-360 adversarial audit + full remediation (35 confirmed findings, all severities).**
  A 16-agent 360° pen-test + multi-perspective review (3 Opus pentest + 3 Opus + 5 Sonnet +
  5 Haiku, adversarially verified — 12 false-positives refuted) found a coherent root-cause
  class: secrets/invariants that **fail OPEN and silent**. All 35 actionable findings were
  remediated to **fail CLOSED and loud** across 26 files. Highlights: (F1/F2) the R2
  tenant-prefix Tenant-Derivation-Key is now MANDATORY on the prod storage path — the
  public-UUID raw-padded fallback is gone (cfg(test) only), closing a same-millisecond
  cross-tenant CAS/AC co-residence risk; (F5) `R2AcHandler::update` re-enforces the
  divergent-body invariant (no silent overwrite → AC-poisoning); (F7/F8) CAS is now
  residency-aware (`R2_CAS_REGION`) and the worker→container env forward-list is complete;
  (F9/F18/F28/F15) `ERASURE_SALT_KEY` / `PAT_SIGNING_KEY` / `CORELINK_INTERNAL_AUTH_KEY`
  fail-closed + ≥32-char floors; (F6/F10) Stripe `success_url`/`cancel_url` host-allowlist
  (open-redirect); (F22) `timingSafeEqual` uses a real per-isolate HMAC key with no length
  branch; (F37) Vault auth structs redact secrets in Debug; plus quota/session fail-closed,
  OCI/Turbo buffer bounds, brew validation, and doc/comment-vs-code reconciliations. Verified:
  cargo check + clippy -D warnings + worker/signup tsc + corelink-server tests all green.
  Tracked architectural/infra follow-ups in `docs/security/2026-06-13-CAA-360-followups.md`;
  full chewed report in `docs/security/2026-06-13-CAA-360-audit-report.md`.
- **introspect M2: `max_concurrency`** added to `/internal/v1/auth/introspect` (additive
  top-level `Option<u32>`, ratified byte-compatible shape with corelink-runners; ladder
  Starter→20/Pro→40/Team→80/Scale→160/Max→320; absent for cache-only tenants).

### Fixed
- **admin-ui: Clerk sign-in/up widget rendered UNSTYLED, and sign-in bounced to
  the marketing home — two follow-on prod login defects fixed.** (1) `style-src`
  carried BOTH `'unsafe-inline'` and a per-request `'nonce-…'`; per CSP3 a nonce
  (or hash) makes the browser IGNORE `'unsafe-inline'`, so every inline style
  Clerk injects at runtime was refused and the widget rendered with no CSS
  ("tela toda bugada"). The Clerk widget cannot carry our per-request nonce, so
  the nonce is now dropped from `style-src` (kept on `script-src`, where it
  actually hardens XSS); `'unsafe-inline'` on style-src is accepted per OWASP
  (styles are not a script-execution vector). (2) After a successful sign-in
  users landed on `/` (the marketing home, which has no `ClerkProvider` and
  shows no signed-in state) — the Clerk instance's redirect "paths" are all
  null, so the default after-sign-in was `/`. `<SignIn>`/`<SignUp>` now set
  `forceRedirectUrl`/`fallbackRedirectUrl` to the authenticated dashboard
  (`/en/customer`) / `/en/welcome`. The render smoke now also FAILS on ANY CSP
  violation console message (`Refused to apply/load/execute`), closing the gap
  that let the unstyled-but-rendering widget pass the prior render check.
- **admin-ui: prod login was completely broken (`Application error: a
  client-side exception`) — three compounding root causes fixed.** (1) A
  Cloudflare zone rate-limit rule ("Wave 32", 10 req/10s/IP across all corelink
  hosts) counted the ~20 static-chunk requests every SPA page load fires from
  one IP, so `/sign-in` + `/sign-up` 429'd (CF error 1015) on their own JS
  bundles → `Loading chunk failed` → page crash. The rule now EXCLUDES only the
  immutable static prefixes (`/_next/static/`, `/assets/`, `/img/`, `/fonts/`,
  `/static/` — NOT `/_next/image` or `/_next/data`, which stay metered) and
  allows 50 req/10s/IP for dynamic requests (infra change on zone humangr.com).
  (2) The `/sign-in` + `/sign-up` widgets (`<SignIn>`/`<SignUp>`) rendered with
  NO `<ClerkProvider>` ancestor (those routes live outside the
  `[locale]/(authenticated)` provider group), so Clerk threw `useSession can
  only be used within the <ClerkProvider />`; both now mount their own provider
  inside the existing `ssr:false` dynamic boundary (`ClerkSignIn.tsx` /
  `ClerkSignUp.tsx`) — keeping `@clerk/nextjs` out of the edge-SSR pass. (3)
  CSP gaps (verified against Clerk's official policy): `script-src` was missing
  `https://challenges.cloudflare.com` (Clerk Smart-CAPTCHA / Turnstile), there
  was no `worker-src` so the Turnstile `blob:` Web Worker fell back to
  `default-src 'self'` and was refused, and `connect-src` was missing the
  first-party analytics sink + `https://clerk-telemetry.com`. Added
  `script-src challenges`, `worker-src 'self' blob:`, and the two connect-src
  hosts so enforce-mode CSP no longer blocks the widget.
  Hardening from a 3-agent adversarial review rode along: `images.unoptimized`
  (the app uses `next/image` zero times → removes the `/_next/image` optimizer
  as a cost/DoS surface), an `error.tsx` boundary for the authenticated group
  (a thrown Server Component degrades to a branded screen, not a bare 500), the
  `admin-ui-deploy` job timeout 20m→40m (the OpenNext build routinely runs
  18-20m on the shared Mac), and a browser render-smoke
  (`scripts/e2e-admin-ui-render-smoke.mjs` + `e2e-admin-ui-render.yml`) that
  loads the auth pages in headless Chromium and fails on a client-side
  exception — closing the gap that let `e2e-clerk-signup` stay green through
  this outage (it only tests the backend).
- **admin-ui: `/en/welcome` returned HTTP 500 (post-signup landing).** The
  enforcement middleware ran `auth.protect()` with no `signInUrl`, so a signed-out
  request couldn't build a redirect and threw; the middleware's `catch {}`
  swallowed it and fell through to render the protected page, whose server-side
  `auth()` then found no Clerk middleware context and threw during SSR → 500.
  Fixed by passing `{ signInUrl: "/sign-in" }` to `clerkMiddleware` (signed-out →
  clean 307), making the middleware catch FAIL CLOSED on enforced paths (redirect
  to `/sign-in`, never serve a protected page anonymously) with error logging,
  and wrapping the welcome page's `auth()` so a thrown context redirects to
  `/sign-in` (locale-less) instead of crashing — backed by a new
  `(authenticated)/error.tsx` boundary.
- **admin-ui: immutable edge-caching for `/_next/static` (`public/_headers`).**
  Workers Assets served the content-hashed chunks `cache-control: max-age=0,
  must-revalidate` (cf-cache MISS every request) so each SPA page load re-fetched
  ~20 chunks from the worker origin — the cost driver behind the "Wave 32" CF
  rate-limit rule and the per-load burst that crashed login. Adds the
  OpenNext-recommended `public/_headers` (`/_next/static/* →
  public,max-age=31536000,immutable`) so chunks become cf-cache HITs and stop
  hitting the origin.
- **dsr(WI-S11-008): close three GDPR Art.17 erasure gaps — truthful docstring,
  signed attestation on `VerifiedComplete`, and a durable pre-tombstone SLA
  anchor.**
  - **G1** — the `routes/dsr.rs` header docstring claimed a "WAVE 0 PLACEHOLDER …
    NO real data is deleted yet" while `build_d1_worker` already wires the Wave-1
    REAL transports (D1 erase-set per ADR-S11-013, R2 CAS/AC delete, Stripe
    pseudonymize, 8 documented NotApplicable). Rewritten to describe the live
    Wave-1 state so auditors no longer wrongly conclude nothing deletes.
  - **G3** — on the 24h verify sweep landing `VerifiedComplete`, the container now
    signs an Ed25519 erasure attestation (`corelink-erasure-attestation`) and
    indexes it in `erasure_attestations` (+ upserts the matching public key into
    `erasure_public_keys`) so the existing `GET /v1/public/keys/erasure/{region}.pub`
    verifier can serve it. Per-region key reproduced deterministically from a
    write-only seed (`ErasureSigningKey::from_seed`); idempotent + fail-OPEN.
  - **G4** — the verify cron only enumerated `dsr_erasure_log`, so a DSR that
    failed before ANY backend tombstone (audit-fail-closed) had no row and its SLA
    breach went undetected. New migration `0069_dsr_requested.sql` + a
    write-at-enqueue anchor in `handleUserDeleted`; the sweep now enumerates both
    sources (deduped), flipping the anchor to `verified` only on
    `verified_complete`.
- **Pre-merge adversarial-verification hardening (4-dimension fleet review of the
  above).** A multi-agent review (tenant-isolation / residency-leak / GDPR-Art.17 /
  build-contract) confirmed the bundle PASS on every dimension (cross-tenant
  delete/list impossible; EU→US leak impossible; attestation real-Ed25519 +
  VerifiedComplete-only; compiles + matches the clw wire contract) and surfaced
  these follow-ups, all closed here:
  - **Residency: launch only the regions we can actually serve.** `PROVISIONED_MACROS`
    dropped `weur`/`sam` → **`{wnam, enam}`** (both → the live `iad` colo). The
    regional Workers (`prod-lhr`/`sam`/`nrt`/`syd`) have no `[[services]]` binding and
    no per-region bucket yet, so the residency guard (correctly, fail-closed) 503s
    every request from a non-IAD tenant — provisioning such a region onboards a
    customer straight into a 503 wall. Signup now rejects those macros up front with
    the existing terminal 422; re-add a macro only once its regional serving infra is
    deployed (the EU/SAM build-out — residency Phase-2 follow-up).
  - **G3 attestation would have silently no-op'd in prod.** `secrets-checklist` #155
    documented `ERASURE_ATTESTATION_REGION = iad`, but `Region::parse()` accepts only
    macro codes (`wnam|enam|weur|sam`); `iad` → `None` → `sign_and_persist` returns
    early (fail-OPEN) so ZERO attestations would ever be emitted. Corrected the
    example to the macro (`enam`) + flagged #154 (`KEY_ID` is parsed as a `u64`).
  - **Residency build-guard widened to the full key surface.** The wrangler.toml
    invariant test now asserts `R2_AC_REGION` + `R2_CHUNK_REGION` per regional env
    (not just `R2_CAS_REGION`) — AC objects embed output digests/command metadata and
    are equally residency-bearing.
  - **G4 anchor no longer self-expires.** Dropped the 7-day lower bound on the
    `dsr_requested` enumeration — a permanently-stuck DSR is exactly the breach the
    durable anchor exists to surface, and the set is self-limiting (completed DSRs
    flip to `verified`).
  - **G3 attestation secrets are now forwarded to the container.** The DO
    `container.start({env})` forward-list was missing `ERASURE_ATTESTATION_SEED_HEX`
    / `_KEY_ID` / `_REGION` (the container reads all three via `env::var`), so
    setting the seed would never reach the container and attestation would silently
    no-op — the exact `ERASURE_SALT_KEY`-class gap. Added the three forwards (+ Env
    types); `check-env-contract.py` now passes (29/29 forwarded).
- **migrations(0064): make the `tenant` table rebuild D1-applicable — add
  `PRAGMA legacy_alter_table=ON` + recreate the residency triggers.** The
  0064 rebuild (widen `tenant.tier` CHECK to add `'max'`) failed on
  `corelink-config-prod` with `no such table: main.tenant` inside
  `trg_blob_meta_region_match_insert`: SQLite 3.25+ (the D1 fork) re-parses
  every trigger/view body during `ALTER TABLE … RENAME`, and five residency
  triggers on OTHER tables (blob_meta/ac_meta/audit_outbox) reference `tenant`
  in their bodies — during the DROP→RENAME window the re-parse hits a missing
  `tenant`. Fixed with `PRAGMA legacy_alter_table=ON` (SQLite "12-step" step 2,
  a connection flag honoured in-transaction by D1). Also recreates the three
  `trg_tenant_primary_region_*` triggers, which SQLite drops together with the
  table — the original file wrongly assumed they "survive name-bound", which
  would have silently dropped residency enforcement (INV-REGION-NO-CROSS-LEAK)
  post-rebuild. Migration stays additive in effect (verified: rows copied 1:1,
  CHECK set only grows). The failed prod apply rolled back atomically (D1 runs
  each migration file as one transaction) — prod was never left partial.
- **worker→container env: forward `ERASURE_SALT_KEY` + `FABRIC_INTROSPECT_AUTH_KEY`
  to the native container (`durable_object.ts` `container.start({env})`).** The DSR
  Wave 1 erasure adapters (#254) and the corelink-runners introspect route (#261)
  read these from the container's process env, but the Worker DO's explicit env
  forward-list never included them. Effect before this fix (caught during prod
  deploy 2026-06-13): `POST /internal/v1/auth/introspect` stayed unmounted (404),
  and the DSR erasure would have fallen back to a PREDICTABLE non-secret salt
  instead of `ERASURE_SALT_KEY`. Both are now forwarded; introspect mounts and the
  GDPR erasure path uses the real salt. (CAS-erase / tier-select / pat-mint were
  already forwarded and unaffected.) A follow-up audit then cross-checked EVERY
  `env::var` the container reads against the forward-list and closed the remaining
  (currently-unset, so no-op today) gaps — `R2_TDK_HEX`, `SIGNUP_TOKEN_KEY`,
  `CORELINK_PORTAL_RETURN_URL`, and the BYOK provider region/vault vars — so a
  future secret-set reaches the container instead of silently doing nothing.
- **CAA-360 wave-1 follow-ups: write-gate, fail-closed storage, and adapter
  hardening.** Closes the next remediation tranche on top of the 35-finding pass:
  (F27) the cargo/brew/npm/pip cache **write gate** now requires the PAT's
  `can_write` capability — derived from a SINGLE PAT verification via the
  resolver port (`resolve_with_capability`), not a redundant second verifier —
  in addition to the server-trusted `x-corelink-scope` header (two layers, one
  verification); (F8) the OCI adapter rejects session-table exhaustion with
  `429 Too Many Requests` + `Retry-After` (`TooManyOpenSessions`) instead of a
  silent overflow; storage handlers that fail to construct now resolve to a
  loud `UnavailableCas/AcHandler` that maps to **503** (fail-closed, never a
  silent 500/empty-200); and a new `scripts/check-env-contract.py` gate greps
  every container `env::var` against the Worker DO forward-list so an unforwarded
  secret is caught at CI, not in prod. Verified: `cargo check --tests` +
  `clippy --all-targets -D warnings` + `cargo test` (481 + adapter suites) all
  green. Wave-2 design (CAS true-residency, internal-auth Service-Binding,
  in-container PAT re-verify) pinned in `docs/security/2026-06-13-CAA-360-wave2-design.md`.

### Security
- **cas-erase: complete the WP-B CAS-erase WRITE path — wire the real R2
  `CasBlobEraser` (hugit-P2 seam B).** The `POST /_internal/cas/:tenant/:hash/erase`
  scaffold (constant-time internal-auth gate, cross-tenant path-echo check,
  digest charset-validation, delete-before-tombstone ordering, D1
  `cas_tombstone` 410-Gone store) shipped with its R2 byte-deletion seam gated
  OFF (`build_state_from_env` → `None`) until the DSR Wave 1 R2 primitives
  (#254) landed. Now that #254 is merged, the production `R2CasBlobEraser` is
  wired: it reuses `R2S3Client::{list_objects_v2, delete, blob_key}` and derives
  the tenant prefix the **same way the CAS writer did** (`Uuid::try_parse →
  derive_prefix(tdk, uuid)`, else the raw-padded 16-char fallback), then LISTs
  `<region>/<tenant_prefix>/<digest>` across the five canonical CAS regions
  (`sam/iad/lhr/nrt/syd`) and DELETEs the match — idempotent, so a re-erase of an
  absent blob is a no-op success. Key layout matches the stored object **by
  construction** (same `blob_key` leading path the writer keys under), closing
  the silent-no-op class of bug (the earlier `R2Ac` key-derivation mismatch).
  **fail-CLOSED:** the WRITE route mounts only when the internal-auth key, the
  R2 TDK (`R2_TDK_HEX`), and the D1 tombstone store all build from env — without
  the TDK the eraser cannot address the tenant's R2 objects, so it is never
  constructed and the route stays UNMOUNTED (it can never write a 410 tombstone
  for a blob whose bytes it could not delete). Mounted at the #254-merge seam in
  `main.rs`. Unmounted in dev/CI.
- **quota: wire the per-tenant monthly $-ceiling guard (ADR-0068) onto the
  billable data plane (hugit-P2 WP-G1).** `QuotaGuard` existed but no route
  called it (dead code). It is now mounted — alongside the existing scope/rate
  gate — on every billable surface (native CAS/AC, Bazel REAPI v2, Turborepo,
  sccache), charging a FLAT per-op cost (`QUOTA_COST_PER_OP_MICROS`, default
  `1000` = $0.001/op; the $5/mo tripwire ≈ 5000 ops/mo — a coarse preventive
  cap, not precise metering). Over-ceiling ⇒ `402`; store/clock fault ⇒ `503`
  (fail-CLOSED). Unenforced in dev/CI without D1.
- **quota: make accrual DB-atomic (TOCTOU lost-update).** `tenant_quota`
  accrual was a blind overwrite (`accrued = excluded.accrued`); concurrent ops
  lost each other's spend and under-counted. Accrual now does the add in D1
  (`accrued = tenant_quota.accrued + excluded.accrued`) via a dedicated atomic
  `tenant_quota_accrue`; the guard passes the per-op DELTA, and cycle-roll /
  seed keep an absolute write.
- **session-exchange: throttle PAT minting (per-principal, fail-CLOSED 429).**
  `/v1/session/exchange` minted PATs with no rate limit — one valid session
  could loop-mint unbounded PATs. Added a per-derived-principal fixed-window cap
  (10/60s) backed by an atomic D1 counter (migration 0068), rejecting `429` over
  the cap (fail-OPEN only on a throttle-store outage).
- **session-exchange: stop leaking the raw Clerk user id (F-01).** The exchange
  response returned `principal: <raw user_xxx>`; it now returns the opaque,
  SHA-256-derived principal UUID (the value already sent to the container).
- **admin-pilot: validate the pilot slug charset (F-03).** `POST
  /v1/admin/pilots` only checked non-empty + length; it now rejects any slug
  with a byte outside `[A-Za-z0-9_-]` (`400`), blocking null/control chars from
  reaching D1 / audit logs (defence-in-depth; SQLi already impossible via
  parameterised binds).
- **worker: sanitize the `x-request-id` passthrough (F-02).** A propagated
  `x-request-id` (echoed into JSON bodies + forwarded headers) was accepted with
  no charset check, enabling log-injection via control chars. It is now
  restricted to `[A-Za-z0-9._-]`; an invalid value falls back to a generated id.
- **Redact `Debug` on three secret-bearing structs (pre-launch audit, 2 HIGH).**
  `StorageEnv` (`storage.rs`) and `D1HttpClient` (`storage/d1_http.rs`) carried
  `#[derive(Debug)]` despite holding the R2 S3 secret access key, R2 access key
  ID, and the CF API token — so any `{:?}` / `dbg!` / `tracing` `?`-field / panic
  `{:#?}` would print production cloud credentials verbatim (and `StorageEnv`'s
  doc-comment falsely claimed Debug was "intentionally redacted"). Replaced both
  derives with manual `Debug` impls that emit `[REDACTED]` for every secret/
  identifier field (matching the existing `D1HttpCustomerDb`/Stripe redaction
  pattern). Also gave `RsaPrivateKeyPem` (`corelink-dpa-acceptance`) a redacting
  `Debug` so the RS256 signing key's PEM can't leak through a derived `Debug`
  (auto-fixing `DpaAcceptanceService`, which delegates to the field). Latent (no
  current call site formats these directly) but a one-line future log would have
  leaked the keys to storage/D1. No behaviour change beyond Debug output.

### Fixed
- **ci: unbreak the `wasm32-unknown-unknown` build (main red ~3 days).** The
  `WASM target build` gate (`corelink-worker` cargo-check on wasm32) had been
  failing since 2026-06-10 because `getrandom 0.4.2` entered the wasm dependency
  graph (via `uuid`'s `rng-getrandom` feature) without its `wasm_js` backend
  feature enabled — so it fell through to the `unsupported` backend stub and
  E0425'd on the missing `fill_inner`/`inner_u32`/`inner_u64`. `corelink-worker`'s
  `[target.'cfg(target_arch = "wasm32")'.dependencies]` already pinned the 0.2
  (`js`) and 0.3 (`wasm_js`) majors but not 0.4; added the matching
  `getrandom 0.4 features = ["wasm_js"]` entry (the `--cfg=getrandom_backend=
  "wasm_js"` rustflag was already set in `.cargo/config.toml`). wasm32 check now
  passes locally; native build/test graph unchanged (target-gated).
### Added
- **feat(container): per-tenant monthly $-ceiling — a fail-CLOSED spend cap
  (WP-FOUND-2 / G1, ADR-0068).** The container already enforced a per-tenant
  *rate* limit (`ratelimit_buckets`, velocity) but had **no monetary bound** — a
  tenant operating within the rate limit could still accrue unbounded monthly
  cost (the real blast-radius risk for the hugit campaign on cheap third-party
  infra). Adds a new `tenant_quota` D1 table (migration
  `0066_tenant_quota.sql`; additive `CREATE TABLE IF NOT EXISTS`, integer
  micro-dollars, $5/mo launch tripwire default) plus a quota middleware
  (`crates/corelink-container/src/tenant_quota.rs`): `QuotaGuard::check`
  fail-CLOSES — over the ceiling → `402 Payment Required`, quota-store error /
  clock-unavailable → `503` — and accrues on the allow path, rolling the cycle
  every ~30 days. Wired alongside the existing rate limit (the two together
  bound both axes — velocity AND cumulative dollars). Backed by
  `D1HttpClient::tenant_quota_lookup` / `tenant_quota_upsert` (parameterised,
  tenant-scoped). In-memory fake for dev/CI; `D1QuotaStore` in production.

### Security
- **deps: waive 3 rust-postgres DoS advisories (RUSTSEC-2026-0178 / -0179 /
  -0180).** Published 2026-06-12 in the postgres-protocol / tokio-postgres stack
  (short-`DataRow` panic, unbounded SCRAM iteration CPU-exhaustion,
  malformed-`hstore` decode panic) — they red'd cargo-deny / cargo-audit
  repo-wide. All three reach the tree SOLELY via `corelink-audit-chain`'s
  **optional**, feature-gated postgres audit sink (`tokio-postgres`,
  `optional = true`) plus dev test-containers; the shipped CF Workers runtime is
  D1/SQLite and opens no postgres connection, and all three require a
  malicious/compromised/MITM postgres *server*, which the product never connects
  to — zero production attack surface. Waived (not upgraded) because
  `cargo update` to the fixed tokio-postgres 0.7.18 force-DOWNGRADES unrelated
  shared workspace deps (windows-sys 0.61→0.48/0.52, socket2 0.6→0.5,
  getrandom 0.4→0.3) — unacceptable collateral churn for a dev/optional path.
  Mirror ignores added to `deny.toml` + `.cargo/audit.toml`; re-evaluate when
  tokio-postgres ships a clean-resolving fix or the optional sink is dropped.
- **CRITICAL — close the "paid tier without payment" enforcement hole (e2e
  adversarial audit 2026-06-11).** The checkout backend persists the requested
  paid tier into `tier_selections` at checkout-START with
  `subscription_state = 'pending_checkout'` (before any payment); the canonical
  access gate is `subscription_state = 'active'` (the live Stripe handler only
  flips the row to `active` on a *paid* `checkout.session.completed`). But the
  request-time tier-enforcement read
  (`worker/src/lib/quota.ts::getTierForTenant`, wired at `worker/src/index.ts`
  → `checkStorageQuota`/`checkRequestQuota`) read `tier_selections.tier` with
  **no state filter** — so a user could select a paid tier, abandon Stripe
  checkout, and be served full paid quota for free. The enforcement read now
  honours the canonical gate (`AND subscription_state = 'active'`), falling
  through to `tenant.tier` → `free` otherwise; the customer-dashboard plan read
  (`crates/corelink-container/src/customer_d1.rs`) mirrors the same filter so a
  pending checkout never displays as the active plan. Auth / tenant-isolation /
  PAT-revocation re-audited clean and unchanged. (The live Stripe webhook
  handler — `apps/signup-worker/src/webhooks/stripe.ts` — already maps real
  price ids, downgrades on `past_due`, and is process-then-claim idempotent, so
  no webhook-handler change was needed.)

### Added
- **Fabric PAT introspection endpoint (corelink-runners M1).** New
  `POST /internal/v1/auth/introspect` on the container
  (`crates/corelink-container` routes/auth_introspect.rs): the runners fabric
  resolves an inbound Bearer PAT to its owning tenant + plan via the shared
  Option-B [`PatVerifier`] pipeline. Caller auth is fail-CLOSED on the
  `X-Corelink-Internal-Auth` header bound to a NEW **dedicated** secret
  `FABRIC_INTROSPECT_AUTH_KEY` (≥32 chars; distinct from the mint secret — tight
  blast radius), reusing the constant-time `internal_auth_ok` gate; the route is
  NOT mounted when the secret/PAT-key/D1 are absent. Responses: `200 {valid:true,
  tenant_id, plan}` on a verified PAT (plan resolved by the new additive Rust
  `tier_for_tenant` mirror of `getTierForTenant` — active-subscription → tenant
  tier → `free`); uniform `200 {valid:false}` (no oracle, no tenant_id) on a bad
  PAT; **503** on a verifier-backend OR tier-query D1 fault (fail-CLOSED — never
  serve a guessed plan; the fabric maps 503 → `Err(Unreachable)`). The optional
  `max_concurrency` / `rate_ceiling_per_min` caps are OMITTED at M1 and
  forward-compatible (`skip_serializing_if`). Secrets matrix row #151 added.
- **DSR erasure — 24h verification sweep cron (WI-S11-008 Wave 1, increment 5).**
  New signup-worker Cron Trigger (`[triggers] crons = ["0 * * * *"]`, hourly) →
  `scheduled()` → `runDsrVerifySweep`: queries `dsr_erasure_log` (D1) for every
  DSR past its 24h SLA deadline (bounded 7d look-back, one verify per `dsr_id`)
  and POSTs the container `/_internal/dsr/verify` for each. Idempotent (re-sweeps
  are harmless); inert until `CORELINK_INTERNAL_AUTH_KEY` is bound (task #46).
  Closes the autonomous pipeline: Clerk `user.deleted` → queue → erase → 24h cron
  → verify → audit. (106/106 signup-worker tests green.)
- **DSR erasure — `POST /_internal/dsr/verify` endpoint (WI-S11-008 Wave 1,
  increment 5).** Drives the canonical 24h verification sweep
  (`ErasureWorker::verify_erasure`): re-fingerprints every backend for a `dsr_id`
  and lands the `verification_passed/failed` + `completed` audit arms. Same
  internal-auth gate as `/erase`, but a **light `DsrVerifyV1` wire shape**
  (`dsr_id` + `tenant_id` + `queued_at_ms` only) — the per-DSR `erasure_salt` and
  raw `subject_id` are NOT retained post-erasure and the sweep never needs them
  (it re-fingerprints by tenant). Returns a compact non-PII decision label
  (`verified_complete`/`verified_partial`/`sla_breached`/…), never the per-backend
  completions. Fired by the signup-worker cron (next).
- **DSR erasure — reconcile the 8 not-shipped backends to `NotApplicable`
  (WI-S11-008 Wave 1, increment 4 — completes the 12-backend wiring).** The
  canonical contract assumes a Neon-primary control-plane + WORM audit/evidence/
  legal-hold/PITR stores that were never shipped (cold-verified: Neon holds only
  `audit_events_shadow`; no `evidence-*`/`legal-hold`/audit-WORM R2 buckets in
  `wrangler.toml`; KV is caches; no active Loki sink). A new
  `NotApplicableAdapter` (parameterised by kind + a documented `reason`) replaces
  the silent `InMemory` placeholders for `NeonMain`/`NeonBilling`/`NeonPitrPseudo`/
  `Kv`/`Loki`/`R2AuditPseudo`/`R2CasLegalHoldPseudo`/`R2EvidencePseudo`, so the
  per-backend audit row records a truthful `not_applicable` instead of a no-op
  success. `build_d1_worker` now wires **all 12** canonical backends with real or
  reconciled adapters (zero `InMemory`).
- **DSR erasure — real Stripe pseudonymize adapter (`Stripe`, WI-S11-008 Wave 1,
  increment 4).** Pseudonymizes a tenant's Stripe customer(s) (redacts
  email/name/phone/address, stamps `pii_redacted` metadata) via the existing
  `StripeRealClient::pseudonymize_customer` — **never deletes** the customer
  (invoice/payment history must survive fiscal retention: GAAP ASC 606 + LGPD
  Art. 16). Resolves the ordering hazard (canonical fan-out runs `D1` before
  `Stripe`, and D1 deletes the `tenant` row that carries `stripe_customer_id`)
  by reading the id from the **retained** `stripe_customers` table (D1
  RETAIN-set). Idempotent (deterministic key per `(subject, customer)`);
  fails CLOSED when `STRIPE_SECRET_KEY` is unset. Wired into `build_d1_worker`
  (D1 + R2Ac + R2Cas + Stripe real; the remaining 8 backends — the not-shipped
  Neon/Kv + the WORM pseudonymized audit/evidence/legal-hold — stay `InMemory`
  pending their shipped-reality reconciliation).
- **DSR erasure — real R2 CAS erase adapter (`R2Cas`, WI-S11-008 Wave 1,
  increment 3 — completes increment 3).** Hard-deletes a tenant's
  content-addressed bytes from the single `corelink-cas-prod` bucket via
  **LIST-by-prefix** (`R2S3Client::list_objects_v2`, paginated) over
  `<region>/<tenant_prefix>/` across all five storage regions, then a defensive
  D1 cleanup of the CAS storage-layer tables (`chunks`, `manifest_chunks`,
  `multipart_sessions`, `blob_meta`). LIST-by-prefix is the only *complete*
  enumeration: cold verification confirmed the live prod CAS path is
  native-whole-blob-only (multipart NOT shipped) with **no durable D1 index**, so
  the tenant's bytes can only be reached by their derived prefix. The prefix is
  `derive_prefix(tdk, tenant)` — the same derivation the writer used (keys match
  by construction); no TDK ⇒ fail CLOSED. Wired into `build_d1_worker`
  (D1 + R2Ac + R2Cas real; the other 9 backends stay `InMemory`).
- **DSR erasure — real R2 Action-Cache erase adapter (`R2Ac`, WI-S11-008 Wave 1,
  increment 3).** Hard-deletes a tenant's REAPI Action-Cache result envelopes from
  the per-region `corelink-ac-<region>` R2 buckets, then the `ac_meta` D1 index.
  Fully D1-driven (no S3 LIST): `ac_meta` is the authoritative per-tenant AC index
  (`(tenant_id, action_digest)` + `region` + materialised `tenant_prefix`), so each
  row resolves to its exact `<region>/<tenant_prefix_hex>/<action_digest>` key.
  Reads the materialised prefix (rotation-correct) via a new `col_blob_hex` D1 BLOB
  decoder that **fails CLOSED** on an unrecognised wire form (never builds a wrong
  key and silently skips a PII object). R2 objects deleted BEFORE the D1 index rows
  (idempotent-retry-safe; `DeleteObject` is itself idempotent). Wired into
  `build_d1_worker`; the other 10 non-D1/AC backends stay `InMemory` placeholders.
  The `R2Cas` adapter is deferred within increment 3 — cold verification (the
  investigation logged in ADR-S11-013) confirmed the prod CAS write path is
  native-whole-blob-only (multipart/chunks NOT shipped) with no durable D1 index,
  so CAS erase needs `ListObjectsV2` by tenant prefix (next).
- **Storage — `R2S3Client::delete` (WI-S11-008 Wave 1, increment 3 prep).** Idempotent
  S3 `DeleteObject` primitive (deleting a missing key is a safe no-op, so a replayed
  erasure is harmless), mirroring the existing `put`/`get`. Required by the R2 CAS/AC
  GDPR erasure adapters. Method added; the adapters that consume it land in increment 3
  once the live CAS whole-blob-vs-chunk + multi-region residency storage model is
  cold-confirmed (see ADR-S11-013 §R2-erasure open questions).
- **DSR erasure — real D1-backed idempotency ledger + audit sink + effective D1
  erase adapter (WI-S11-008 Wave 1, increments 1+2, ADR-S11-013).** Three new
  transports under `corelink-container/src/routes/dsr/`: `D1ErasureIdempotencyLedger`
  (over `dsr_erasure_log`, `INSERT OR IGNORE` + `SELECT`-back → `Replayed`/`DivergentPayload`
  on a 4-field `(outcome, tenant_id, subject_id_hash, idempotency_key)` match, deterministic
  `log_id`); `D1ErasureAuditSink` (→ `audit_outbox` CloudEvents, **omits raw `subject_id`**
  per CTRL-PRIV-014); and `D1EraseAdapter` (23 cold-verified PII `DELETE`s — tenant_id /
  namespace / `signup_attempts` subquery, `tenant` row last, never touches the `_public`
  namespace, legal-hold → `NotApplicable`, retain-set guard test). Wired via
  `build_d1_worker()` (real D1 ledger+audit+D1 adapter; the other 11 backends stay
  `InMemory` placeholders pending increments 3-5). Pipeline remains inert in prod until
  task #46 provisioning (queues + `ERASURE_SALT_KEY` + `CORELINK_INTERNAL_AUTH_KEY`).
- **DSR erasure — `BackendCompletion.subject_id_hash` canonical field (WI-S11-008
  Wave 1, ADR-S11-013 gap #1).** The per-backend completion record now carries the
  canonical `sha256(subject_id ‖ erasure_salt)` subject pseudonym, populated by the
  orchestrator via `pseudonymize_subject_id`, so the real D1-backed idempotency
  ledger can write the `dsr_erasure_log.subject_id_hash NOT NULL` column (the pure
  trait surface previously could not). Additive + `#[serde(default)]` (pre-Wave-1
  `outcome_json` snapshots stay deserializable); the `outcome_json` round-trip
  property test passes with arbitrary subject-hash values.
- **DSR erasure — `StripeRealClient::pseudonymize_customer` (WI-S11-008 Wave 1,
  Stripe backend).** Pseudonymizes a customer's PII for a DSR erasure
  (`POST /v1/customers/:id`: overwrite email/name, clear phone/address, stamp
  `pii_redacted` + `erasure_dsr_id` metadata). By design exposes NO
  customer-delete primitive — deleting the customer would break invoice
  integrity (GAAP ASC 606 + LGPD Art. 16 fiscal retention). See ADR-S11-013.
- **DSR account deletion — Clerk `user.deleted` → erasure pipeline (WI-S11-008
  WP-F + WP-G).** The signup-worker Clerk webhook now handles `user.deleted`: it
  looks up the tenant by `clerk_user_id` and PRODUCES a frozen-contract
  `dsr.queued.v1` message (deterministic `dsr_id` for idempotent redelivery +
  HMAC-derived erasure salt) onto `DSR_QUEUE`. The same worker CONSUMES the queue
  (`queue` handler → `dsr_consumer.ts`) and forwards each message to the
  container's `/_internal/dsr/erase` endpoint (internal-auth gated; ack on 2xx,
  retry otherwise → dead-letter after 10 attempts). No tenant → 200 no-op;
  tenant present but queue unbound → **fail-loud 500** so a GDPR right-to-erasure
  obligation is never silently dropped. New infra: `corelink-dsr-erasure` queue
  + DLQ; new secret `ERASURE_SALT_KEY`.
  The container exposes the receiving endpoint `POST /_internal/dsr/erase`
  (WI-S11-008 **Wave 0**, `crates/corelink-container/src/routes/dsr.rs`,
  internal-auth gated) which maps the message to a canonical `ErasureRequest`
  and drives the 12-backend erasure orchestrator. **Wave 0 uses in-memory no-op
  backend adapters** — the full pipeline is wired and exercised end-to-end but no
  real data is deleted yet; **Wave 1** swaps each canonical adapter for its real
  transport (D1 / R2 / Stripe / KV / Loki).
- **AC handler — 409 Conflict on divergent-body PUT (hugit-P2 WP-A).** `InMemoryAcHandler::update`
  now refuses to overwrite a stored `(tenant, action_digest)` result with different bytes
  (`AcHandlerError::DivergentBody` → HTTP 409); a byte-identical re-PUT stays an idempotent
  no-op (`durable=false`). A proven cache result is immutable-once-stored — silent replacement
  is forbidden. Audit emits before the refusal.
- **Pilot-admin: create endpoint + D1-durable store (hugit-P2 WP-FOUND).**
  `POST /v1/admin/pilots` (and the `/_internal/admin/pilots` operator-edge alias)
  creates a new pilot tenant: mints a fresh `tenant_id`, persists it in the `NEW`
  lifecycle state (`tier=free`), and returns the created record (`201 Created`).
  The handler mirrors the existing `admin_pilot` auth/error/audit discipline
  exactly — operator-only `x-corelink-internal-auth` constant-time gate + scope
  re-check + fail-CLOSED audit-before-mutation. The `PilotStore` is now backed by
  a new D1-durable `D1PilotStore` (over `D1HttpClient`, sync↔async via
  `block_in_place`/`block_on`, CF-token-redacting Debug) so pilots survive
  container restarts; the non-durable `InMemoryPilotStore` remains the dev/CI
  fallback (env-gated). New additive migration `0065_pilot_tenants.sql` adds the
  `pilot_tenants` backing table (no destructive change; no ADR waiver needed).
- **Transparency-log submission seam — public Rekor witnessing (hugit-P2 seam
  E, ADR-0066).** New `corelink-transparency-log` crate: a thin submitter that
  witnesses CoreLink-signed audit / attestation entries on the **public
  sigstore/Rekor** transparency log — making them verifiable *against* CoreLink,
  not *via* CoreLink. Per ADR-0066 CoreLink integrates the public log rather
  than rebuilding one: it builds the canonical Rekor `hashedrekord` v0.0.1
  proposed entry from a `SignedEntry` (the JCS-canonical payload digest +
  detached Ed25519 signature + published public key — the payload bytes never
  leave CoreLink), submits it through the `RekorSubmitter` async seam, and
  records the returned `RekorWitnessRecord { log_index, inclusion_proof }`
  alongside the entry. The `witness_or_degrade` driver runs **post-hoc, off the
  write path** and **fails OPEN**: a Rekor outage degrades witnessing
  (`WitnessOutcome::Degraded`, queued for out-of-band retry) but never blocks or
  errors the durable write path. Ships pure-logic (entry builder + response
  parser + fail-open policy) with an `InMemoryRekor` fake pinning every
  invariant in CI; the real HTTPS transport to `rekor.sigstore.dev` is the
  binding portion deferred per ADR-0066.
- **Per-hash CAS erase + 410-Gone tombstone (hugit-P2 seam B, WP-B).** New
  operator/internal write-side endpoint `POST /_internal/cas/:tenant/:hash/erase`
  (constant-time `X-Corelink-Internal-Auth` gated, off the hot GET path) deletes
  a single content-addressed blob from the cold `corelink-cas-prod` R2 bucket and
  writes a durable tombstone (`cas_tombstone` D1 table, migration
  `0067_cas_tombstone.sql`). The CAS read path
  (`GET /v1/cas/:tenant/:hash`) now consults the tombstone FIRST and returns
  **HTTP 410 Gone** for an erased hash — never 404 ("never existed") and never
  200 (resurrected bytes); a re-erase is an idempotent no-op. Pure decision logic
  ships in the new `corelink-handler-cas-erase` crate (digest validation,
  cross-tenant gate, tombstone marker, read-gate). The R2 byte-deletion COMPOSES
  the DSR Wave 1 R2 CAS primitives (`R2S3Client::{delete, list_objects_v2}`, PR
  #254 / `feat/dsr-account-deletion`) rather than duplicating them; the erase
  WRITE route stays unmounted (fail-CLOSED) until that adapter lands, while the
  410 READ gate is live from env wherever D1 creds are present.
- **`EventLogDO` — thin, generic, per-tenant append-only event-log Durable
  Object primitive (ADR-0065, hugit-P2 seam D / WP-D).** A minimal ordering +
  durability primitive: `POST /_eventlog/append` returns a strictly-monotonic,
  gap-free, 1-based `{ seq, ts_ms }` under the DO's single-writer
  serialization, and `GET /_eventlog/read?from_seq=&limit=` returns entries in
  `seq` order. Deliberately NOT chain-aware (no hashing/Merkle/signatures) — the
  consumer (hugit) layers its integrity chain on top (ADR-0066). One DO instance
  per tenant (`idFromName(tenant_id)`); tenant-pinned (cross-tenant → 403). Owner
  file `worker/src/event_log_do.ts`; bound as `EVENT_LOG_DO` across all envs with
  migration `tag = "v3"` (`new_sqlite_classes = ["EventLogDO"]`).
- **Clerk session bridge for `customer_v1` — dual-auth dispatch (dashboard
  revival WP-1).** `/v1/customer/*` now accepts EITHER a CoreLink PAT (existing
  path, byte-identical — the dispatch guard is `parsePat(bearer) === null`, and
  a Clerk JWT can never parse as the canonical `corelink_<env>_…` PAT shape) OR
  a Clerk session JWT from the browser dashboard. The onboarding arm's Clerk
  verification (verifyToken + M1 azp presence/allowlist re-assert + M2 issuer
  exact-pin/shape-check + `tenant.clerk_user_id` resolution) is extracted
  verbatim into the shared helper `worker/src/lib/clerk_auth.ts`
  (`verifyClerkSessionAndResolveTenant`), which the onboarding arm now calls —
  zero behavior change (its tests are untouched and green). On a valid session
  the Worker forwards to the PER-TENANT DO with the server-trust headers
  (`x-corelink-tenant-id`, `x-corelink-token-prefix: clerk`,
  `x-corelink-scope: read-write`), dropping the Clerk JWT at the edge and —
  least privilege, unlike onboarding — WITHOUT `x-corelink-internal-auth`
  (customer routes don't need the operator-grade credential). The storage-quota
  gate is deliberately bypassed on this arm: an over-quota tenant must still
  see the dashboard to upgrade (same posture as onboarding). 12 new tests
  (PAT + revoked-PAT/0063 regressions, bad-sig/azp/issuer 401s, no-tenant 403,
  least-privilege forward assertions); worker suite 236 → 248.
- **`/_internal/admin/pilots…` alias + internal-edge identity synthesis
  (#218 §2.1-§2.2, ratified Q3 2026-06-10).** The three pilot-admin handlers
  (`list` / `grant-tier` / `checkin`) are now additionally bound under
  `/_internal/admin/pilots…`, making the operator surface reachable from the
  public edge through the Worker's existing `/_internal/*` channel
  (constant-time internal-auth verification + client-trust-header strip +
  `_system`-DO forward) with zero Worker changes. Because that channel strips
  `x-admin-principal`/`x-admin-scope` and `require_admin_scope` previously
  hard-403'd on an empty principal, every edge call would have 403'd
  (the drift the Q3 ratification fixed): after the PRIMARY internal-auth gate
  passes, an empty principal PLUS the server-set
  `x-corelink-route-kind: internal` (Worker-overwritten on every forward —
  not client-forgeable) now synthesizes the audit principal
  `internal-edge-operator` with the pilots scope treated as granted, so audit
  rows always carry a principal. Explicit-header requests keep today's
  behavior byte-identical; an empty principal without the internal route-kind
  still 403s; the route-kind header never substitutes for the internal-auth
  secret. The `POST /v1/admin/pilots` create-tenant endpoint itself (#218
  §2.3-§2.7, incl. the `max` create-time refusal and
  `seed_tier_selection=false` default) is the follow-up work package.
- **PAT soft-revocation schema (migration 0063) + enforced revoked filter in
  both lookups (dashboard-revival WP-2).** `migrations/d1/0063_pat_customer_keys.sql`
  additively adds `pat.name` (customer-facing key label for the dashboard key
  list) and `pat.revoked_at_ms` (soft-revocation timestamp; NULL = active —
  rows are retained for audit instead of deleted, per the additive-only
  auth-migration policy). Revocation is ENFORCED at both PAT lookups: the
  Worker hot-path (`worker/src/index.ts` `validatePat` step 4) and the
  container Option-B verifier (`crates/corelink-container/src/adapter_pat.rs`)
  now filter `AND revoked_at_ms IS NULL`, so a revoked PAT uniformly fails
  closed as 401 (`pat_not_found` / `InvalidPat` — indistinguishable from an
  unknown token; no revocation oracle on the wire). Both columns are consumed
  by the WP-3 `D1CustomerHandler` (key list / rename / revoke). Tests: worker
  vitest (revoked row → 401; explicit-NULL active row still resolves) +
  container unit tests (revoked ⇒ uniform `InvalidPat`; lookup-SQL shape
  guards both liveness filters).
- **Billing-tier → operational-tier mapping + per-tier retention promise in
  the rate card (task #35, PR #218 §3 — ratified 2026-06-10).** New single
  Rust authority
  `corelink_ratelimit::tier::tier_for_billing_label(&str) -> Tier` maps the
  FROZEN 6-tier billing taxonomy (plus D1 legacy values) onto the 5-tier
  operational ladder: free→Free, solo→Solo, starter→Team, pro→Business,
  max→Business (NOT Enterprise, ratified Q5a), org→Business, team→Team,
  enterprise→Enterprise, pilot/unknown→Team (zero behavior change —
  `RateLimitConfig::canonical()`'s implicit default is already Team; the
  enum wildcard-arm Enterprise fallback in `refill_rate_for_tier` is
  untouched). Tests pin totality over the taxonomy, the max≠Enterprise
  decision, the Team fallback, and price-ladder monotonicity. Because the
  mapped `Tier` also selects the eviction TTL ladder, the §3.5 ratified
  acceptance item ships in the same change: `TIER_RATE_CARD`
  (`apps/docs/src/lib/pricing.ts`) now carries the per-tier cache-retention
  promise (`retentionDays` 7/30/90/365/365/365 + Enterprise-only
  `retentionOverrideCapDays` 730 per CAP-EVICT-002), rendered on the public
  pricing page (tier-card bullet + "Cache retention" comparison row) and
  pinned by vitest (ladder values, Enterprise-only cap, monotonic
  non-decreasing retention, `formatRetention` rendering).
- **D1-backed customer handler (dashboard revival WP-3).** New
  `crates/corelink-container/src/customer_d1.rs`: `D1CustomerHandler`
  implements all 6 `corelink-handler-customer` traits over the live D1
  database (sync↔async bridge per `billing_d1_http`), replacing the
  `InMemoryCustomerHandler` 404-stub so real tenants get real dashboard
  data — HONEST v1: real data where a deployed table exists, explicit
  empty/zero/501 where it doesn't, never fabricated. Overview/usage read
  `tenant` (tier 0057) + `tenant_storage_state` SUM(bytes_used)/quota +
  `tenant_billing` (0055) + `byok_envelope` presence; billing maps the
  FROZEN status table (paid→active, past_due/incomplete→past_due,
  canceled→canceled, no-row→inactive); billing/portal creates a real
  Stripe billing-portal session from `tenant_billing.stripe_customer_id`
  (no customer → 404 "no billing account"); keys list/create/revoke run
  against the `pat` table (create = real `corelink_pat::mint` + INSERT,
  FROZEN scope map `["cache:read"]`→read-only / anything-with-write→
  read-write / admin NEVER grantable, token returned once + never
  logged; revoke = tenant-scoped idempotent `revoked_at_ms` UPDATE —
  depends on the WP-2 migration 0063 `pat.name`/`pat.revoked_at_ms`
  columns, parallel PR); team = synthesized Owner row from
  `tenant.clerk_user_id`; team/invite = new additive
  `CustomerHandlerError::NotImplemented` → explicit 501 ("team invites
  are coming soon"). Wiring: `routes.rs` now uses
  `customer::build_handlers_from_env()` — D1 env present → D1 handler,
  else InMemory (dev/CI), mirroring the `adapter_pat::PatVerifier::
  from_env` fail-closed pattern. Audit emit-before-lookup + SLI on every
  return path per the trait contracts; D1 transport errors fail CLOSED
  (500), never degrade to empty data. 30 new hermetic mock-D1 tests
  (per-endpoint happy paths, frozen status/scope maps, cross-tenant
  revoke → 404, idempotent revoke, portal-no-customer → 404, invite →
  501, fail-closed transport + audit-failure ordering).
- **admin-ui customer dashboard client wired to real Clerk auth (WP-4 of the
  dashboard revival).** `CustomerClient` accepts an optional
  `getToken?: () => Promise<string | null>` and attaches
  `Authorization: Bearer <token>` to every `/v1/customer/*` request when it
  resolves non-null (omitted otherwise — the E2E mock mode's request shape is
  byte-identical to before). All 6 customer components (Overview / Usage /
  Audit / Billing / Keys / Team) now build the client in-component from
  Clerk's `useAuth().getToken` via `useMemo` instead of an unauthenticated
  module-scope singleton. `CustomerBilling["status"]` (and the overview
  billing snapshot) additively widened with `"inactive"` for tenants without
  a Stripe subscription. New `tests/customer-client.test.ts` covers the
  token-attached / token-null / no-getToken request shapes.
- **Public-flip smoke harness (`scripts/smoke/`) — unauthenticated probes +
  real-Clerk-session Playwright spec.** Closes the smoke gap behind the
  2026-06-10 azp incident: a latent `401 clerk session azp invalid` on the
  user-facing host survived every prior smoke because none exercised a real
  browser Clerk session on `corelink-app.humangr.com`. Layer 1
  (`public-flip-smoke.sh`, curl-only, no secrets) pins the canonical
  unauthenticated expectations per prod host — app/admin landing 200 +
  security headers (XFO/XCTO/HSTS/Referrer/Permissions/CSP),
  `/upgrade?plan=solo` 307→`/en/upgrade` chain ending 200, `/sign-up` 200,
  docs 200, api fail-closed 404 root + `/health`+`/_health` — with per-probe
  PASS/FAIL and non-zero exit on any FAIL. Layer 2
  (`authenticated-smoke.spec.ts`, standalone Playwright — own package.json,
  NOT wired into admin-ui's e2e configs) signs in through the real Clerk UI
  with a dedicated smoke user (`SMOKE_USER_EMAIL`/`SMOKE_USER_PASSWORD` from
  env, never logged), asserts welcome/provisioning renders with a
  session-wide watcher proving no `401 azp invalid` on the wire, drives
  `/upgrade?plan=solo` to a minted `checkout.stripe.com` URL (navigation to
  Stripe route-aborted — no payment possible), and parks
  `test.skip(TODO(dashboard-wave))` 5xx assertions for the dashboard tabs.
  `SMOKE_*` vars allowlisted in both secrets gates (test-account creds, the
  `CORELINK_E2E_TOKEN`/`NEON_TEST_DSN` precedent). Run before/after every
  public-facing deploy; WP-5 acceptance gate. See `scripts/smoke/README.md`.
- **pnpm prod-advisory audit gate** (`.github/workflows/pnpm-audit.yml`). New
  CI workflow scanning npm/pnpm advisories on every PR that touches
  `pnpm-lock.yaml`, `package.json` or relevant workspace manifests, plus a
  daily cron at 03:47 UTC (staggered from existing nightlies). Runs on
  `ubuntu-latest` (pure registry call — no self-hosted Mac slot consumed).
  Steps: checkout → pnpm 10.32.1 + Node 22 → `pnpm install
  --frozen-lockfile --ignore-scripts` → blocking `pnpm audit --prod
  --audit-level high` (1 retry for registry 5xx flakes; carries
  `continue-on-error: true` with a `TODO(flip-to-blocking)` comment until
  the npm-advisories-override PR resolves the ~10 known prod vulns) →
  non-blocking full `pnpm audit` for dev-graph visibility. Advisory waivers
  via `pnpm.auditConfig.ignoreCves` in `package.json` require an ADR-style
  note, mirroring `.cargo/audit.toml` policy.
- **`tenant.tier` CHECK widened to include `'max'` — migration 0064 + ADR-0064
  (#218 §4-Q2 ratified follow-up).** Migration 0057 added `tenant.tier` with an
  inline CHECK that accepted `('free','solo','starter','team','pro','org','enterprise')`
  but omitted `'max'`. Migration 0062 already widened `tier_selections.tier` and
  `stripe_checkout_sessions.tier` to include `'max'`; leaving `tenant.tier`
  narrower would create silent quota-enforcement gaps for max-tier customers.
  PR #218 §4-Q2 ratified the fix as a non-blocking follow-up. Migration 0064
  applies the same 0062-style 12-step table rebuild: full 19-column explicit copy
  (zero rows dropped or mutated), all 9 indexes recreated verbatim, `PRAGMA
  defer_foreign_keys` around the DROP/RENAME window. `'pilot'` intentionally NOT
  added (ratified out at §4-Q2). ADR-0064 records the mechanism and the ratification
  quote. Draft PR — prod apply is owner-gated.
- **admin-ui `/upgrade?plan=<tier>` page — the public pricing CTAs now reach
  checkout (#49).** Every docs pricing CTA targets
  `corelink-app.humangr.com/upgrade?plan=<tier>`, but admin-ui had no
  `/upgrade` route — the money path's front door 404'd. Added
  `/[locale]/upgrade`: validates `?plan=` against the checkout-able set
  (solo/starter/team/pro/max; invalid/missing → anchor SKU `pro`), probes the
  Clerk session with the same predicate as `/api/checkout/session`
  (signed-out → `/sign-in?redirect_url=…` round-trip back to the page, plan
  preserved), and for signed-in visitors auto-fires the existing checkout
  POST via `<UpgradeButton autoStart />` (once-per-mount, StrictMode-safe)
  behind a minimal accessible tier card (rate-card name+price, `role=status`
  redirect notice, manual retry fallback). A locale-less `GET /upgrade`
  forwarder 307s the docs-CTA URL shape onto `/en/upgrade?plan=<normalized>`
  (same default-locale convention as `/` and `/sign-up`). The checkout
  route's `PAID_TIERS` gate and the page validation now share one source of
  truth (`CHECKOUT_TIER_IDS`, src/lib/pricing.ts) so the two surfaces cannot
  drift. 16 new tests (plan normalization, forwarder, signed-out redirect,
  tier-card render incl. legacy `team`, auto-fired POST handoff, no-flag
  no-fire).

### Fixed
- **security(audit 2026-06-11): pre-launch pentest hardening** — strip client
  `.js.map` source maps from the admin-ui OpenNext assets before deploy (was
  serving 138 maps publicly, leaking dep versions/module paths); add the 6
  launch-critical forwarded secrets (R2 S3 keys, internal-auth, PAT-signing,
  Stripe SOLO/MAX price ids) to the cf-deploy-prod REQUIRED gate (silent
  InMemory-storage/broken-checkout on omission); pin `wrangler@4.95.0` exact in
  the two deploy workflows; correct the pnpm-audit runner comments.
- **fix(deps): prod npm graph → 0 known vulnerabilities** — pnpm overrides for the 8
  pre-existing prod advisories (shell-quote 1.8.4, rollup 3.30.0 scoped backport,
  js-cookie 3.0.8, serialize-javascript 7.0.5, postcss dedupe, qs 6.15.2, uuid 11.1.1
  + admin-ui direct bump); pnpm-audit CI gate flipped to BLOCKING on the prod graph
  and routed to the self-hosted fleet (ubuntu-latest unavailable in this repo).
- **fix(ci): secrets-checklist gate red on main** — the rust-bundle merge resolution
  duplicated the `ALLOWLIST_REGEX` assignment (second overwrote the first, dropping
  `CORELINK_PORTAL_RETURN_URL`); merged into a single assignment.
- **Container fail-closed hardening: method gates, `/v1/users/me` tenant
  parity, brew `_public` pre-store integrity.** (1) The four adapter
  method-dispatch scope gates (`routes/cargo.rs`, `npm.rs`, `pip.rs`,
  `brew.rs`) ended in `_ => true` — an unmapped HTTP method bypassed the
  per-operation scope check and relied on the adapter's routing to 405 it.
  Now `_ => false` (fail-CLOSED; a future adapter route can never ship
  without an explicit scope decision), with per-gate DELETE/PATCH-with-
  `cas:rw`→403 tests. (2) `GET /v1/users/me` defaulted a missing/sentinel
  tenant to an `"_unknown"` echo — the one v1 surface off the fail-closed
  posture. It now uses the same `AuthTenant` extractor as cas/ac/turbo
  (missing/sentinel `x-corelink-tenant-id` → 401). (3) The brew adapter
  stored upstream-fetched bottle bytes into the shared `_public` moat
  namespace without pre-store verification (`integrity:"best-effort"`).
  ghcr.io serves bottles as content-addressed OCI blobs
  (`…/blobs/sha256:<hex>`), so the fetch path
  (`corelink-adapter-host::brew::bottle`) now verifies the fetched bytes
  against the URL-declared sha256 BEFORE the store: mismatch → refusal
  (502, nothing stored or served) + a
  `corelink.brew.bottle.integrity_mismatch.v1` audit row; verified fills
  carry `integrity:"verified-sha256"`; non-content-addressed paths (e.g.
  manifest-by-tag) keep `"best-effort"`. Spec
  (`specs/_proposals/adapters/brew.md` §3) updated to match.
- **cas-foundation heavy gate: `cargo fmt --all -- --check` RED on `main`
  cleared.** The nightly/heavy "Workspace build + test (convergence)" job died
  at the rustfmt step (before clippy ever ran) on formatting drift in 4 files
  that landed unformatted: `crates/corelink-container/src/routes/admin.rs`
  (`TIER_SELECTIONS_TIERS` const) and three `corelink-tier-selection`
  test/array sites (`src/tier.rs`, `tests/mutation_kills.rs`,
  `tests/prop_tier_selection.rs`). Pure `cargo fmt` mechanical reflow — zero
  semantic change; clippy `-D warnings` + tests on both touched crates green.
  (The run's other two reds are infra, not code: the reproducible-build smoke
  hit the shared-runner `rustc … (never executed)` / os-error-2 toolchain
  race, and the TLC canonical job finished its model checks then got
  cancelled by the lane timeout.)
- **Repo-root wrangler version hygiene — stale v3 broke bare `npx wrangler`.**
  An out-of-band npm install of `@cloudflare/next-on-pages` (~2026-05-14) had
  dropped a stale `wrangler@3.114.17` into the pnpm-managed root
  `node_modules`; with no root `wrangler` devDependency, bare `npx wrangler`
  resolved v3 and died parsing `wrangler.toml` (`"containers" should be an
  object, but got an array` — the containers array is v4 syntax). Root
  `package.json` now pins `wrangler ^4.95.0` (locked to 4.95.0, matching
  `worker/`), so the repo-root `node_modules/.bin/wrangler` is always 4.x,
  and `scripts/apply-d1-migrations-prod.sh` now defaults to the repo-pinned
  binary (`$REPO_ROOT/node_modules/.bin/wrangler`) instead of
  `npx wrangler@latest` (the `WRANGLER` env override is preserved).
  `scripts/deploy-pages-docs-prod.sh` already resolved the repo-local binary
  via `_pages-deploy-common.sh` — its "run pnpm install" hint now actually
  installs a root wrangler. (Task #44)
- **CI tool installs → SHA-pinned taiki-e prebuilt binaries (os-error-2 compile-race).**
  `cargo-audit`, `cargo-mutants`, and `cargo-nextest` were installed via `cargo install`
  (from source), pulling the heavy `aws-lc-sys` build that intermittently fails with
  `No such file or directory (os error 2)` on the contended self-hosted Mac — cancelling
  the cargo-audit + cargo-mutants gates. Switched all 13 call sites to the SHA-pinned
  `taiki-e/install-action@fd2f5e3d…` (v2.81.9) prebuilt binary (versions unchanged; pure
  mechanism swap). Lighter + reliable. Governance: ADR-S12-045 v1.3.0 + §14.s12.004.1
  Security review (owner-approved 2026-06-10).
- **Stripe webhook money-path fail-safety: payment_status activation gate +
  BILLING_DB fail-closed + price↔tier defense-in-depth assert + tenant_billing
  terminal-state guard** (`apps/signup-worker/src/webhooks/stripe.ts`,
  closes four audited findings; stacks on the `metadata[tier]` fail-loud fix
  below). (1) `checkout.session.completed` activated entitlement without ever
  reading `payment_status` — an async payment method (SEPA/ACH) completes the
  session with `payment_status='unpaid'` and the money may never arrive.
  Activation (tenant_billing upsert + tier_selections activation + MRR emit,
  now one shared helper) is gated on `paid`/`no_payment_required`; `unpaid` →
  200 with ZERO writes; an unknown/absent payment_status fails loud (500).
  New handlers: `checkout.session.async_payment_succeeded` runs the identical
  shared activation once the delayed payment clears, and
  `…async_payment_failed` logs + acks (nothing was granted, nothing to
  revoke). (2) A missing `BILLING_DB` binding no-op'd every money-path write
  yet still acked 200 — Stripe never retries, so a PAID checkout was silently
  dropped with no recovery. The handler now fails closed (503
  `billing_db_unbound`, mirroring the `STRIPE_WEBHOOK_SECRET` check) so
  Stripe retries until the deploy misconfiguration is fixed. (3)
  Defense-in-depth price↔tier assert: server-set `metadata[tier]` could be
  desynced from the actually-subscribed price by a checkout-backend bug;
  `customer.subscription.created`/`.updated` (the first payloads carrying
  both signals — the checkout-session payload has no price data, line_items
  is expand-only) now 500 `subscription_tier_price_mismatch` before any
  write when they disagree, instead of silently entitling either SKU. (4)
  Terminal-state guard: Stripe webhooks are unordered — a late
  `customer.subscription.updated(status=active)` after
  `customer.subscription.deleted` resurrected `tenant_billing` to `'paid'`.
  The status UPDATEs now carry `AND status != 'canceled'` (matching the
  canonical tier_selections gate, where activation only happens via
  checkout); only a NEW checkout can move a canceled row forward. Tests:
  76 → 87 (+11 new incl. unpaid-completed zero-writes, async-succeeded
  identical activation, async-failed no-writes, unbound-DB 503, mismatch
  500s on created/updated + agreeing-tier no-false-positive, and the
  deleted-then-late-active resurrection pin).
- **Stripe webhook: a paid `checkout.session.completed` without
  `metadata[tier]` was silently billing-rowed as `starter`** — the
  fallback in `apps/signup-worker/src/webhooks/stripe.ts` defaulted any
  missing/unparseable tier metadata to `"starter"`, so an anomalous session
  (our checkout backend always sets `metadata[tier]`, client.rs:630) would
  record the WRONG plan with no signal (a Max $149 checkout persisted as
  Starter $35). Now fails loud — 500 `checkout_missing_tier` before any
  write, same class as the existing missing-tenant/customer guard, so
  Stripe redelivers and the anomaly is visible. Also: the
  `subscription_canceled` analytics event hardcoded `from_plan: "starter"`
  for every cancel; it now derives the real prior plan via
  `resolveSubscriptionTier` (metadata\[tier\] / price→tier map, `"unknown"`
  fallback). Tests: +1 fail-loud case; the fixtures that leaned on the
  buggy default (`metadata.plan`, never read) now send `metadata.tier`.
- **fix(worker): Clerk azp allowlist now includes `corelink-app.humangr.com`** — the
  user-facing sign-up host (#219) was missing from `ONBOARDING_AZP_ALLOWLIST` and
  `authorizedParties`, so every session minted on corelink-app was 401-rejected on
  the onboarding/checkout funnel (found by the 2026-06-10 dashboard-wiring design
  review; latent — no real users yet).
- **Security hardening sweep (worker, CI workflows, runbook).** Four precise
  mechanical changes: (1) `x-corelink-tenant-id` added to `CLIENT_TRUST_HEADERS`
  strip list in `worker/src/index.ts` — the invariant is now structural (strip
  happens before any forward path can set or delete the header) rather than
  per-path discipline; (2) prominent `pull_request_target` + merge-ref checkout
  sentinel comments added to `.github/workflows/dependabot-policy.yml` — guards
  the self-hosted-runner / PR-head-checkout combination against future unsafe
  additions; (3) `SEARCH_HAYSTACK` in `.github/workflows/api-deprecation-check.yml`
  built with `printf '%s\n'` instead of bare shell concatenation — prevents
  shell-control content in PR title/body from altering downstream `grep` parsing;
  (4) `SECURITY.md` operational note added: fork-PR approval requirement when the
  repo goes public + SHA-pinning baseline verified 2026-06-10.
- **Worker test harness: miniflare D1 mock drifted from the real PAT lookup —
  5/22 miniflare integration tests failed on a fresh `pnpm install`.** The
  hand-seeded `pat` table in `worker/tests/do_miniflare_integration.miniflare.test.ts`
  lacked the `scope` column that the Worker's auth query
  (`SELECT tenant_id, expires_ms, scope FROM pat …`, H1 scope enforcement)
  selects, so the D1 query threw and `extractAuth` fail-closed every valid
  token to 401 (reason `d1_lookup_error`) — the seeded schema now mirrors the
  canonical migrations (`0037` + `0054`, incl. `pat_hash`/`scope`/`token_id`).
  Also: `miniflare` v4 is now a declared `worker/` devDependency (it was a
  phantom import that only resolved via a stale root `node_modules` leftover);
  the `/v1/rollouts/*` expectation updated for the deliberate generic `/v1/*`
  reapi_v1 PAT-gate arm (401, not 404); and the two `quota.test.ts` rate-card
  assertions left stale by the #209 6-tier sweep now assert the signed launch
  ladder (free 10 GB/500K, solo 50 GB/2M, starter 150 GB/6M). Test
  harness/expectations only — no Worker runtime behavior changed; full worker
  suite 233/233 + miniflare 22/22 green.
- **admin-ui auth was decorative — the edge middleware never enforced
  anything and the production auth provider rejected everyone.** Five audited
  fixes: (1) `middleware.ts` now calls `auth.protect()` inside the
  `clerkMiddleware` handler for protected paths, so unauthenticated requests
  redirect to sign-in (security headers — nonce/CSP — still applied to every
  response including Clerk's redirects; the no-publishable-key dev/test
  fallback is preserved); the route matcher gained the genuinely-public
  surfaces enforcement would otherwise break (`/` landing, locale-prefixed
  pricing/legal/privacy/security/403, `/api/newsletter/subscribe`) plus a
  self-gated class for `/upgrade` (Clerk context without middleware protect —
  the page owns its `?plan=`-preserving sign-in round-trip). (2)
  `src/lib/auth.ts` resolves the real Clerk session in production (lazy
  `@clerk/nextjs/server` `auth()`; `orgRole` mapped onto `ClerkOrgRole`,
  solo users without an org get Viewer-minimum `corelink-member`,
  `corelink-admin` only ever from an explicit org role; `mfa_verified_at`
  from the `fva` claim; fail-closed on any error) instead of always returning
  `role: null`, which left RbacGuard/CustomerGuard rejecting every real user;
  the double-gated E2E cookie path is unchanged. (3) `customer/*` and
  `admin/*` moved under the `(authenticated)` route group (URLs unchanged) so
  ClerkProvider is mounted and client-side `useAuth`/`useUser` work. (4)
  `/api/newsletter/subscribe` (public POST) is now per-IP rate-limited
  (`CF-Connecting-IP`, same limiter as `/api/csp-report`; 429 + retry-after
  with CORS headers). (5) `redactTokens` also scrubs JWT-shaped substrings
  (Clerk session tokens) from error bodies, not just CoreLink PATs.
- **Docs pricing copy: purged the stale "Pro $25/mo or $250/yr" / "Three
  plans" launch shape from every public docs page** (public-launch blocker —
  the published prose contradicted the live checkout). All copy now matches
  the canonical 6-tier rate card (`apps/docs/src/lib/pricing.ts`
  TIER_RATE_CARD): Free $0 / Solo $15 / Starter $35 / Pro $50 ($500/yr) /
  Max $149 / Enterprise contact. Touched: `pricing.tsx` (meta description,
  hero, header comment), `pricing/calculator.tsx` (meta description),
  `legal/terms.tsx` §3 (three plans → six tiers), and all six
  `compare/vs-*.mdx` pages — including the derived arithmetic ($300/yr Pro
  TCO → $600/yr; the vs-turborepo Scenario B differential and the
  vs-sccache 500 GB verdict recomputed honestly at $50/mo; "smallest paid
  tier" cells now anchor Solo $15/mo). Competitor prices left untouched.
  Plus the dead 5-tier `Free|Solo|Team|Org|Enterprise` comment in
  `apps/admin-ui/src/app/[locale]/pricing/page.tsx` and a stale
  `Free / Pro / Enterprise` test name in `pricing.test.ts`.
- **`apps/analytics-worker` legacy toolchain (wrangler `^3.62.0` → `^4.20.0`,
  vitest `^1.6.0` → `^4.1.8`) — clears the last 9 dev-graph npm advisories
  rooted in its wrangler-3/vitest-1 dependency chain** (undici 5.x ×5 via
  miniflare 3, esbuild ≤0.24 / vite 5 via vitest 1, ws via miniflare 3,
  vitest 1.x itself). It was the repo's last wrangler-3 holdout; the bump
  aligns it with the versions every other Worker package already uses and
  evicts miniflare 3 / undici 5 / vitest 1 / wrangler 3 from `pnpm-lock.yaml`
  entirely. `worker/package.json` now declares `miniflare ^4.20260609.0`
  explicitly (it previously leaned on the v3 hoisted from analytics-worker's
  wrangler 3; the miniflare integration suite keeps running through the
  transition — the same explicit pin lands in #228). Remaining `pnpm audit`
  findings all trace through the `apps/admin-ui` and `apps/docs` dev chains,
  none through the Worker packages.
- **`corelink-app.humangr.com` (public app entry, all docs pricing CTAs) served
  the dead Pages build — `/` returned literal `"Not Found"` and `/sign-up`
  500'd** (pre-existing since ≥ 2026-05-27, launch-flip blocker). Root cause:
  the 2026-05-30 Pages→Worker migration (`5435fd8a`) moved only
  `corelink-admin.humangr.com` to the OpenNext Worker and left
  `corelink-app.humangr.com` attached to the abandoned `corelink-admin-ui`
  Pages project (broken next-on-pages build, commit `3daebca6`). Fix:
  `apps/admin-ui/wrangler.toml` now binds `corelink-app.humangr.com` as a
  Worker custom domain; the two legacy Pages deploy scripts are hard-deprecated
  (`FORCE_LEGACY_PAGES_DEPLOY=1` escape hatch); owner-gated flip steps in
  `docs/operator/corelink-app-domain-flip-runbook.md`. No app code, env or
  secret changes — the identical Worker already serves these routes 200 on
  `corelink-admin.humangr.com`.
- **Docs CI: the four pre-existing reds greened (task #42).** (1) The Vale
  prose-lint jobs (`docs-ci.yml` + `docs-vale.yml`) moved to GitHub-hosted
  `ubuntu-latest` — `errata-ai/vale-action` downloads a Linux x86_64 reviewdog
  binary that ENOEXECs (`spawn Unknown system error -8`) on the self-hosted
  macOS fleet; prose lint needs no secrets or self-hosted hardware. (2)
  `apps/docs/tests/cli-reference.test.ts` now reads the CLI clap source at its
  real location `tools/cli/src/main.rs` (moved from `crates/corelink-cli` in
  wave-33 stage 2.D.3, 360042b8). (3) `apps/docs/tests/sidebars.test.ts`
  expected-category list updated to the current canonical sidebar (adds the
  intentional Integrations / Concepts / API categories from 3e1eb226). (4)
  `Health.mdx` REAPI reference regenerated — legitimate provenance drift after
  `apps/server/proto/health.proto` moved to
  `crates/corelink-container/proto/health.proto` (wave-33, 1d0c221e); the
  generator is deterministic. Also repointed the stale
  `apps/server/proto/**` docs-ci trigger path at the live
  `crates/corelink-container/proto/**` so future proto edits re-run the
  drift gate instead of silently skipping it.
- **cargo-deny `0.16.4` → `0.19.8` — CVSS 4.0 advisory parsing (repo-wide red gate).**
  cargo-deny `0.16.4` could not parse CVSS 4.0 advisory vectors, so the new
  `RUSTSEC-2026-0073` advisory hard-failed the advisories gate at database load on
  every PR touching `crates/*/src/**`. Bumped to `0.19.8` (prebuilt via SHA-pinned
  `taiki-e/install-action@fd2f5e3d…` v2.81.9) across all four call sites. Strictly
  positive security posture (parses *more* advisories; `deny.toml` policy unchanged,
  verified `advisories/bans/licenses/sources ok`). Governance: ADR-S12-045 v1.2.0 +
  §14.s12.004.1 Security review (owner-approved 2026-06-10).
- **6-tier completeness sweep across every TypeScript surface.** The 6-tier
  launch (Solo $15 / Max $149) had shipped Rust-complete but left stale 5-tier
  unions on the TS edge. Closed in one sweep, all aligned to the signed launch
  rate card (`apps/docs/src/lib/pricing.ts` TIER_RATE_CARD): (1) **money-path
  bug** — the signup-worker `checkout.session.completed` activation gate used
  an inline starter/team/pro triple, so a paid Solo/Max checkout never
  activated `tier_selections` (pay-but-not-entitled); now uses the canonical
  `asPaidTier` set, with end-to-end regression tests for solo/max activation
  AND the price→tier reverse map. (2) Worker edge `quota.ts`: `max` was
  missing entirely (a Max tenant fell back to the `free` quota class); quotas
  now mirror the rate card (50 GB/150 GB/500 GB/2 TB + per-tier request caps).
  (3) admin-ui `pricing.ts` rewritten from the pre-S19 $5/$30/$150 ladder to
  the 6-tier card; checkout route + UpgradeButton + plan unions + e2e fixtures
  widened; docs `PricingCalculator` retyped to the canonical `TierId`; the
  public pricing page `ctaForTier` gained the missing Solo/Starter/Max CTAs;
  POSITIONING.md ladder updated from "illustrative" to the signed rate card.
  Also greens two suites that were red on main: the stale 3-tier/$25-Pro
  `pricing.test.ts` now pins the 6-tier card, and the signup-worker
  webhook-e2e fixtures carry `CORELINK_INTERNAL_AUTH_KEY` (required since the
  #195 fail-loud gate).
- **Migration 0062: drop/recreate the dependent `stripe_tier_drift_view` around
  the `tier_selections` rebuild.** The 0048 reconciliation view reads
  `tier_selections`; D1 aborts the rebuild batch on the dangling reference
  ("error in view stripe_tier_drift_view: no such table") even though local
  sqlite3 tolerates the window — the SQLite 12-step "drop and recreate views"
  step was missing. Also adds `PRAGMA defer_foreign_keys = true` (the
  D1-honoured in-transaction mechanism; `PRAGMA foreign_keys` is a documented
  no-op inside a transaction). Verified red→green against a restored prod
  replica: 6-tier CHECKs land, view recreated verbatim + queryable, indexes
  rebuilt, zero rows lost.

### Changed
- **Billing tier taxonomy 5 → 6 (Free/Solo/Starter/Pro/Max/Enterprise).** Adds a
  **Solo** ($15) entry SKU and a **Max** ($149) premium SKU and removes **Team**.
  `TierKind` + `requires_stripe_checkout` (now the 4 paid: Solo/Starter/Pro/Max)
  + the Stripe `plan_*`→tier map + `RequestedTier`/checkout adapters + the public
  pricing page (lists all six) move together. Spec governance: **ADR-S19-001**
  amends the sealed R-S19-7 tier count *without editing sealed history*;
  **ADR-0062** records the D1 migration mechanism. Additive D1 migration **0062**
  widens the `tier_selections` / `stripe_checkout_sessions` tier CHECKs (adds
  `solo`/`max`, retains `team`) via the SQLite 12-step rebuild (zero data loss).
  New `scripts/ops/stripe-setup-tiers.sh` (idempotent, safety-gated, not run)
  provisions the live products and emits `STRIPE_PRICE_ID_{SOLO,STARTER,PRO,MAX}`.
- **6-tier money-path wiring through the TypeScript edge.** Completes the change
  on the worker side (the Rust/container side shipped above): the Durable Object
  now forwards `STRIPE_PRICE_ID_{SOLO,MAX}` into the container env (alongside
  Starter/Pro) so Solo/Max checkout sessions resolve a price instead of 500-ing,
  and the `signup-worker` Stripe webhook's reverse price→tier map + `PaidTier`
  union + `asPaidTier` learn `solo`/`max` so a paid Solo/Max subscription
  activates `tier_selections.tier` (without it those customers pay but stay
  un-entitled). `stripe-setup-tiers.sh` fixed: `--live` is a per-command flag and
  a live `STRIPE_API_KEY` is auto-detected.
### Added
- **Durable Turborepo remote cache** — `storage::r2_kv::R2KvStore` backs the
  `/v8/artifacts/*` surface with R2 when storage creds are present (closes the
  `turbo_v8` `TODO(v2)`: artifacts now persist across container restarts;
  in-RAM `InMemoryKvStore` remains the dev/CI fallback). Per-tenant HMAC prefix
  isolation; opaque keys (no content-hash verify); a `KvBackend` seam makes the
  full behavioral suite runnable against an in-process fake (no network). Core
  suite proves the merge-blockers: tenant isolation incl. path-traversal-as-
  literal [P0], durability-across-rebuild, no-false-404 on backend error, and
  proptest invariants (object-key determinism/injectivity/opacity). Full SOTA
  scenario matrix (307 scenarios, 7 lenses) in `docs/TEST-PLAN-kv-cache.md`.

### Fixed
- **Release container build: `corelink-reapi` protoc codegen now resolves the
  protobuf well-known types.** The builder stage installed `protobuf-compiler`
  (the `protoc` binary) but not `libprotobuf-dev`, so
  `proto/google/rpc/status.proto`'s `import "google/protobuf/any.proto"` failed
  with `File not found` during `docker build` (host + CI cargo builds were
  unaffected). Added `libprotobuf-dev` to the builder stage so
  `/usr/include/google/protobuf/*.proto` is present on protoc's include path.

### Added
- **Durable D1-HTTP billing writer for the Stripe-webhook materializer
  (money-path launch-blocker #27 Item 7b).** Added
  `corelink-container::billing_d1_http::D1HttpBillingWriter`, a native
  `BillingD1Writer` that persists Stripe-webhook state (customers /
  subscriptions / invoices / disputes / refunds / tier + the idempotency dedup
  row) DURABLY to Cloudflare D1 over the REST API, replacing the in-memory
  mirror that lost all billing state on container restart. The
  `BillingD1Writer` trait (and the whole `WebhookDispatcher` pipeline) is
  **sync by charter** — it is shared with the wasm32 CF Worker whose
  `worker::D1Database` `JsFuture`s are `!Send` — so the new writer bridges the
  sync trait to the async `D1HttpClient` via
  `tokio::task::block_in_place(|| Handle::current().block_on(…))` (the native
  server is `#[tokio::main]` multi-thread); **no trait was made async** and the
  shared leaf / `stripe-real` / Worker path is untouched. Wired in `main.rs`:
  env-gated on `StorageEnv::from_env()` — durable D1 in prod, `InMemoryBillingD1`
  in dev/CI. Fail-CLOSED (any D1 transport/non-2xx error → `Transient` → HTTP
  500 → Stripe retries; the dedup row prevents double-materialization);
  parameterised SQL only; secret-redacting `Debug`.
- **Stripe-materializer SQL reconciled to the deployed D1 schema (same
  launch-blocker).** The canonical billing SQL literals were materially WRONG
  vs the deployed migrations and would have silently failed every webhook write
  in production. Promoted them to a single source of truth in
  `corelink-billing-stripe-materializer::d1` (re-exported `SQL_*`, transcribed
  by both the wasm32 binder and the native writer) and fixed each against the
  deployed columns: the JSON column is **`payload_json`** (was `payload`) on all
  `0048` tables; `ON CONFLICT` targets each table's natural PK (was a composite
  `(tenant_id, …)`); `stripe_subscriptions.status` and `stripe_invoices.outcome`
  (NOT-NULL, no default — sourced from the handler payload) are now bound;
  `stripe_refunds` uses its real `stripe_charge_id` PK (the non-existent
  `stripe_refund_id` is gone); and the `0044` dedup INSERT now writes the real
  un-tenanted columns `(event_id, event_type, processed_at_ms, outcome,
  correlation_id)` — `outcome` literal `'dispatched'`, the Stripe `event_id` as
  `correlation_id` — with `RETURNING event_id` for insert-vs-replay detection
  (the trait signatures are unchanged). New `d1::tests` pin every corrected
  shape under the default `cargo test --lib`.
- **sccache → CoreLink cargo build-cache surface (Phase A — code).** Mounted the
  `corelink_adapter_host::cargo` adapter at `/cargo/<tenant>/<key>`, closing the
  three integration gaps from `FINDING-sccache-adapter-gaps.md`: (1) the Worker
  already forwarded `/cargo/*` but the container never mounted the adapter (every
  request 404'd) — now wired via `routes/cargo.rs` (`nest_service("/cargo", …)`
  over the adapter's catch-all route + a per-operation scope gate); (2) **Option B**
  container-side PAT validation —
  a new D1-backed `TenantResolver` (`cargo_pat_resolver::D1PatTenantResolver`)
  re-verifies the bearer PAT against the D1 `pat` store (HMAC fast-reject →
  token_id lookup → Argon2id possession check → fail-CLOSED cache-scope gate),
  the Argon2id check the Worker skips under its cpu_ms budget; (3) the stale
  adapter PAT prefix `hugr-pat_` → `corelink_` so production PATs are accepted.
  Tenant is PAT-derived (never the path segment); per-operation read/write scope
  is enforced at the route from the Worker-set `x-corelink-scope`. The `/cargo`
  route is env-gated (mounted only when `PAT_SIGNING_KEY` + the D1/R2 `StorageEnv`
  are present; fail-CLOSED / unmounted in dev/CI). Deploy + measurement are
  Phase B/C (owner-gated).
- **brew + npm + pip cache surfaces (Phase B) + shared PAT-verifier unification.**
  Wired `corelink_adapter_host::{brew,npm,pip}` into the container on the SAME
  Option-B auth + a new 2-level content-dedup moat: `/brew/<tenant>/<path>`
  (public Homebrew bottles, cross-tenant deduped under `_public`),
  `/npm/<tenant>/<rest>` (registry.npmjs.org mirror; tarball bytes per-tenant,
  metadata in D1 `adapter_npm_meta`), `/pip/<tenant>/<path>` (PyPI PEP 503/691
  simple index in D1 `adapter_pip_index` + content-addressed wheels). Bytes
  content-address through `adapter_cache::MoatCache` behind a D1
  `(namespace, url_hash) → content_hash` map (migrations 0058–0060); the moat
  also re-verifies on READ that served bytes re-hash to the mapped content-hash
  (refuse-and-self-heal), so the shared `_public` namespace can never serve
  un-content-addressed bytes even under a poisoned map row (L6-review hardening).
  **Zero-debt
  unification:** the cargo-only `cargo_pat_resolver::D1PatTenantResolver` (#163)
  is replaced by ONE trait-agnostic `adapter_pat::PatVerifier` shared across
  cargo/brew/npm/pip — each adapter wraps it in a thin `TenantResolver` newtype,
  so the HMAC → D1 → Argon2id → fail-CLOSED-scope pipeline lives exactly once.
  SSRF guards pin each upstream to its configured origin; the adapter PAT prefix
  is `corelink_`. All four routes are env-gated (fail-CLOSED / unmounted in
  dev/CI). Deploy/measurement owner-gated.
- **OCI Distribution v1.1 registry surface (Phase B) — the 5th + final adapter.**
  Wired `corelink_adapter_host::oci` into the container + Worker: `/v2/*` +
  `/token` (docker / podman / buildah / containerd / Helm OCI). Two-leg auth —
  the client exchanges a PAT (HTTP Basic, Option-B re-verified) at `/token` for
  a short-lived HMAC registry bearer; `/v2` ops verify the bearer locally (no
  D1/Argon2id per request) + enforce its repo scope. The Worker forwards OCI
  RAW (pass-through to a dedicated `_oci` DO — it cannot resolve a PAT scope for
  the two-leg flow), so the container is the SOLE OCI auth authority. Image
  blobs dedup through the content-addressed moat (inheriting its read-side
  integrity check); mutable manifests + tag lists live in the durable D1
  `adapter_oci_kv` table (migration 0061). **SECURITY (scope-escalation fix):**
  the `/token` exchange now downscopes the minted bearer to the PAT's real
  capability — a read-only (`cas:r`) PAT can no longer obtain a `push` token
  (it gets pull-only; the data-plane `scope.allows(repo,"push")` then denies the
  write). New env `CORELINK_OCI_TOKEN_KEY` (≥32-byte session-HMAC key, distinct
  from `PAT_SIGNING_KEY`); env-gated mount (fail-CLOSED). Completes all 5 cache
  adapters (cargo / brew / npm / pip / oci). Deploy + measurement owner-gated.

### Security
- **Money path fails LOUD instead of silently mis-provisioning (brutal-audit B4).**
  Two signup/payment paths that silently returned 200 on a misconfiguration —
  dropping a paying customer with no retry and no operator signal — now fail
  loud so the upstream (Stripe / Svix) redelivers and the failure is visible:
  (1) `apps/signup-worker/src/webhooks/stripe.ts` — a `checkout.session.completed`
  missing `tenant_id` or the Stripe `customer` now returns **500 before the
  idempotency claim** (was: silent 200 with no entitlement write); the customer
  paid, so we never silently ack a checkout we could not apply.
  (2) `apps/signup-worker/src/webhooks/clerk.ts` — `issuePat` with an absent
  `CORELINK_INTERNAL_AUTH_KEY` now **throws** (→ webhook 500 → Svix retries)
  instead of returning a fake `corelink_pat_DEVSTUB` that looked valid to the
  user but authenticated nothing while the webhook 200'd (so Svix never retried
  and the new user was permanently, silently broken). Regression tests added for
  both. NOTE: the related checkout `metadata[tier]`-absent default-to-`starter`
  under-provisioning is **deferred** to a dedicated PR — fixing it correctly
  requires also making `customer.subscription.updated` RE-activate the
  entitlement gate (the SCA/out-of-order lockout), and the two must land
  together to avoid a fail-closed regression.
  `corelink-signup-worker` Worker owns the secrets that gate the entire
  signup→provision→first-payment path (`CLERK_WEBHOOK_SECRET`, `CLERK_SECRET_KEY`,
  `CORELINK_INTERNAL_AUTH_KEY`, `STRIPE_WEBHOOK_SECRET`, `STRIPE_PRICE_ID_*`), but
  it is deployed separately from the main worker (single default env) and was
  therefore OUTSIDE the scope of `put-secrets-prod.sh` and the
  `cf-deploy-prod.yml` secret gate — and `CLERK_WEBHOOK_SECRET` was entirely
  absent from the secrets matrix, so nothing tracked or verified it. Added the
  `CLERK_WEBHOOK_SECRET` matrix row (#150, with the topology note), documented
  the signup-worker gate gap in the checklist Notes, and added
  `scripts/verify-signup-worker-secrets.sh` — a read-only operator pre-launch
  check that lists the signup-worker's deployed secret NAMES and fails if any
  required one is missing (never reads values; intentionally NOT CI-wired so it
  cannot false-fail an unrelated deploy). Follow-up (owner-topology decision): a
  signup-worker deploy workflow that runs this verifier as a hard gate. (NOTE:
  the audit's literal suggestion to add `CLERK_WEBHOOK_SECRET` to the main-worker
  MVP allowlist / `cf-deploy-prod.yml` REQUIRED list was rejected — both target
  the main worker, so it would have pushed the secret to the wrong Worker and
  false-failed the gate.)
- **CAS content-addressing now ENFORCED on the durable R2 path — closes a
  cache-poisoning hole (brutal-audit B1).** `R2CasHandler` previously stored
  bytes under the client-claimed digest WITHOUT computing `blake3(bytes)`, and
  served bytes from R2 WITHOUT re-verifying them — so any client (or a buggy
  uploader) could persist arbitrary bytes under a fabricated digest and poison
  the cache for every subsequent reader, and silent R2 bitrot/tampering was
  served as trusted content. The route doc claimed "the handler enforces hash
  equality" but only the in-memory fake did (hence green tests, blind prod).
  Added `verify_content_hash` and wired it into BOTH `<R2CasHandler as
  CasWriteHandler>::write` (verify BEFORE the PUT — poisoned bytes are never
  persisted) and `<R2CasHandler as CasReadHandler>::read` (re-verify bytes
  returned by R2 before serving). A mismatch emits a `CorrectnessViolation`
  audit + a `CorrectnessCas` SLI failure and returns `422 HashMismatch`. Because
  every CAS write/read surface — native `/v1/cas`, the Bazel REAPI v2 bridge,
  and sccache — funnels through `R2CasHandler`, this single gate closes the hole
  across all of them. A malformed (non-canonical-hex) claimed digest is itself a
  mismatch (never persisted/served). NOTE: Action-Cache (`R2AcHandler`)
  result-payload integrity is a SEPARATE follow-up — the AC action-digest is a
  *key*, not a content hash of the payload, so its integrity needs the
  `corelink-ac` Merkle/HKDF signed-result envelope wired into the update/lookup
  path (tracked, not in this PR).
- **Pilot-signup route env-gated — killed the hardcoded public dev HMAC key.**
  The `/v1/signup/pilot` route was mounted UNCONDITIONALLY in prod with a public
  constant signing key (`DEV_TOKEN_KEY` in the open repo), making pilot activation
  tokens forgeable by anyone. `signup::build_state_from_env()` now reads the
  hex-encoded `SIGNUP_TOKEN_KEY` secret (≥ 32 bytes decoded) and the route mounts
  ONLY when it is present + valid; absent/invalid → fail-CLOSED (route unmounted,
  warning logged), mirroring `internal_pat`'s `PAT_SIGNING_KEY` env-gate. The
  token-verification logic is unchanged — only the key SOURCE (env, not hardcoded)
  and the mount gate. `build_state()` (dev key) is retained for tests/dev only.
- **Audit-round-2 hardening (brutal 13-agent audit follow-ups).** Closed the
  real findings: signup rate-limit no longer keyed off the client-forgeable
  `x-forwarded-for` — the Worker strips it and forwards Cloudflare's trusted
  `cf-connecting-ip` as `x-corelink-client-ip` (denylisted); the container reads
  that, fail-closed to a single shared bucket when absent. Eliminated two
  internal-auth timing oracles (`tier_select.rs` and the Worker `/_internal/*`
  gate) by padding to a single constant-time compare (matching the other gates).
  Turbo `events`/`status` now require the `AuthTenant` extractor; Bazel and
  customer routes fail-CLOSED (401) on a missing/sentinel tenant instead of a
  `"_unknown"` fallback. Cargo adapter hashes the tenant id in debug logs
  (INV-NO-PII-IN-LOGS). Added the `CLERK_ISSUER_URL` secrets-matrix row + wrangler
  doc; corrected stale `admin.rs`/`admin_pilot.rs` comments. (Audit false-positives
  — "scope spine absent", "health exposes storage" — were a stale-checkout artifact;
  the spine and the Worker storage-redaction are present on main.)
- **Auth spine: PAT scope enforcement (H1) + possession-model decision (H2).**
  The Worker now resolves the PAT's `scope` from D1 and forwards it as the
  server-trusted `x-corelink-scope` header (added to the strip denylist so a
  client can never forge it); previously scope was written to D1 but never read,
  so every PAT was effectively unscoped. The container enforces it on the cache
  surfaces (CAS/AC/Turbo AND Bazel REAPI v2): writes require `cas:rw` (or `admin`), reads require
  `cas:rw`/`cas:r`/`admin`, fail-CLOSED on a missing scope. `admin` is treated as
  a cache superset so enforcement is decoupled from the prod scope back-fill
  (admin→cas:rw) — no deploy-ordering dependency. Today every prod PAT grants
  cache rw, so this is a NO-OP for live traffic; it ESTABLISHES the gate so
  read-only / tiered tokens work later. (H2) the HMAC-SHA256 fast-fail with the
  prod-bound `PAT_SIGNING_KEY` is documented as the cryptographic possession gate;
  per-request Argon2id verification is deliberately NOT added (hot-path latency) —
  an explicit engineering decision, addable later (amortized) if signing-key
  compromise becomes a concern.
- **Route-layer negative tests + Clerk issuer exact-pin (pentest follow-ups).**
  Added HTTP route-layer negative tests for CAS / AC / audit-export proving the
  `AuthTenant` wiring: missing `x-corelink-tenant-id` → 401, path/query tenant ≠
  header → 403 (closes the PR #150 cold-review coverage gap — a refactor dropping
  the extractor now fails CI). Completed the onboarding Clerk JWT issuer pin: when
  the `CLERK_ISSUER_URL` secret is set the `iss` claim must equal it exactly
  (else 401); without it the transitional https/"clerk" shape-check applies. (Owner
  activates by `wrangler secret put CLERK_ISSUER_URL --env prod`.)
- **internal_pat + hygiene hardening (pentest 2026-06-06 follow-ups).** (M2)
  `/_internal/pat/mint`'s `x-corelink-internal-auth` verify no longer leaks the
  secret length via a non-constant-time length short-circuit — now a single
  padded constant-time compare (mirrors the PR #152 admin gate). (M3) the auth
  header is checked BEFORE the JSON body is parsed (was `Json` extractor first),
  so an unauthenticated caller can't force body parsing pre-auth. (M6) the mint
  log hashes the tenant id (`hash_for_log`) instead of logging the raw UUID
  (INV-NO-PII-IN-LOGS). (L1) `/_health/container` redacts the `storage`
  backend field so an unauthenticated probe can't detect the InMemory fallback.
  (L3) corrected a stale `corelink-turbo-bridge` doc claiming a removed
  `team_id == caller_tenant` check. `AdminMutateBody.initiator*` made
  wire-optional (`serde(default)`) since the internal-auth gate now owns the
  admin decision. (M7 — response still returns the Argon2id hash — noted as a
  follow-up entangled with the signup-worker D1 write.)
- **Worker now strips client-suppliable trust headers on every forward path
  (header-smuggling fix, H4).** The data-plane Worker forwarded `x-admin-scope`,
  `x-admin-principal`, `x-admin-tenant`, `x-corelink-internal-auth`, and
  `x-corelink-fanout-from` from the client to the DO/container verbatim except on
  the onboarding path — letting any client smuggle them to a trusting handler. A
  `stripClientTrustHeaders` helper now deletes them on all four forward blocks
  (main, region-fanout, internal, onboarding) before the server values are set
  (delete-then-set where the Worker legitimately sets one). Also fixes a latent
  bug where the internal forward leaked the client's `x-corelink-internal-auth`
  instead of re-setting it from the server secret.
- **Closed three CRITICAL cross-tenant / control-plane exposures (pentest
  2026-06-06).** (1) `/v1/audit/analytics/{event-count,timeline}` keyed its
  tenant off the client-forgeable `x-tenant-id` header (the Worker never
  set/stripped it) over a tenant-addressable Neon shadow — any authenticated PAT
  could read any tenant's audit analytics; now bound to the `AuthTenant`
  extractor (`x-corelink-tenant-id`, fail-closed), mirroring the audit-export
  fix. (2) `/v1/admin/{read,mutate}` ran with hardcoded `is_admin:true` /
  body-supplied `initiator_is_admin` and (3) `/v1/admin/pilots/*` authorized on
  the client-forgeable `x-admin-scope` header — any PAT could read any tenant's
  admin record and mutate any tenant's billing tier. Both admin surfaces are now
  gated behind the `CORELINK_INTERNAL_AUTH_KEY` shared secret (operator-only,
  constant-time verify, fail-closed — same posture as `/_internal/pat/mint`);
  the admin-mutate decision no longer trusts the request body. Full audit:
  `security-audit-2026-06-06` (the deeper PAT-scope-enforcement + Argon2id
  possession spine is tracked as a follow-up).
- **DoS hardening (pentest 2026-06-06).** Added a global request-body limit
  (`DefaultBodyLimit`, 10 MiB; 100 MiB on the Turbo artifact route) — previously
  any authenticated PAT could OOM the shared container with an unbounded `PUT
  /v8/artifacts` or JSON body. Turbo `teamId` is now validated (≤256 chars,
  `[A-Za-z0-9_-]` only — rejects empty / `/` / `../` / control bytes) before it
  is used in the storage key, closing heap-amplification and intra-tenant
  slot-aliasing. Audit-export now caps the query window to 30 days (400 on
  exceed) so a single `from=0&to=now` cannot stream an unbounded history.
- **Signup / money-path hardening (pentest 2026-06-06).** (H3) `issuePat` ignored
  its `scope` param and minted the first self-serve customer PAT with the `admin`
  scope (hardcoded in the mint request and the D1 insert) — privilege-by-default;
  now honors the caller-supplied `cas:rw`. (H6) the Svix webhook verify checked the
  HMAC but never validated `svix-timestamp` freshness, so a captured signed
  `user.created` webhook was replayable indefinitely (duplicate tenant/PAT
  provisioning); now rejects timestamps outside a ±5-minute window (and
  missing/non-numeric). (M1) the onboarding Clerk JWT verify is bypassable when the
  token omits `azp` (the library skips the `authorizedParties` check); now asserts
  `azp` is present and allow-listed after verify, plus an explicit issuer
  shape-check (TODO: exact `CLERK_ISSUER_URL` pin).
- **Container cache surfaces now bind tenant isolation to the authenticated
  tenant, not a client-controlled value (cross-tenant read/write fix).** The
  native container keyed CAS (`/v1/cas/:tenant/:hash`), AC
  (`/v1/ac/:tenant/:digest`), Turbo (`/v8/artifacts?teamId=`), and audit-export
  (`/v1/audit/:tenant/export`) isolation off a client-supplied path/query value
  while ignoring the DO-injected, Worker-overwritten `x-corelink-tenant-id`
  header — so any authenticated PAT could read or write any tenant's artifacts /
  action cache / audit log. New `auth_tenant::AuthTenant` extractor (fail-closed
  on missing/sentinel header) is the sole isolation tenant; the path/query tenant
  must equal it (403 on mismatch). Turbo additionally demotes `teamId` to a
  sub-namespace within the authenticated tenant (`key = "<teamId>/<hash>"`) and
  drops the tautological `team_id == caller_tenant` check; the injective HMAC
  prefix path is now always reached for UUID tenants. Mirrors the Bazel surface,
  which already bound the header. Full analysis in
  `docs/FINDING-turbo-tenant-isolation.md`.

### Fixed
- **admin-ui CSP allowed a dead Clerk host (`clerk.corelink.humangr.com`) → the
  sign-in widget would have been CSP-blocked in production.** The owner created the
  Clerk **production** instance (Frontend API `clerk.corelink-app.humangr.com`);
  repointed `script-src`/`connect-src`/`frame-src` (+ the `csp.test.ts` assertions +
  README) to it. 16/16 csp tests green locally. (The prod `pk_live` + the 5 Clerk DNS
  CNAMEs are the operator's launch-day step.)
- **The CLI install one-liner told users to `curl https://get.corelink.io | sh` — a
  domain we do NOT own (`corelink.io` belongs to a third party).** A takeover of that
  domain could serve malware to anyone running the installer. Repointed the served
  install script (`apps/get-corelink-worker/src/install.ts`), the worker comments, and
  the `E2E_INSTALL_URL` default to the live, owned `corelink-get.humangr.com` route
  (already bound in `wrangler.toml`). Also relaxed the prod-surface test's `set -eu`
  shebang regex (it required `set -eu` on line 2, but the script has header comments
  first → it failed against the real script regardless of host). Worker unit tests
  26/26 green.
- **Stripe webhook silently dropped entitlement/billing writes on a D1 failure —
  customer paid, no access, no retry (money-path durability; #37, follow-up to
  #34/#36/#178).** The entitlement/billing D1 writes in
  `apps/signup-worker/src/webhooks/stripe.ts` were dispatched FIRE-AND-FORGET
  (`ctx.waitUntil(p.catch(swallow))`) and the handler returned 200 regardless, so
  a D1 error/throttle lost the write: Stripe saw its 200, never redelivered, and
  the paying customer had no access and no recovery. The writes are now AWAITED
  and, if ANY required write throws, the handler returns a non-2xx (**500**) so
  Stripe redelivers (every write is an idempotent `ON CONFLICT` upsert / guarded
  `UPDATE`, so the retry re-runs them harmlessly). The durable dedup claim was
  also moved from BEFORE processing to AFTER the writes succeed
  (**process-then-claim**): claiming first was wrong once writes are awaited +
  retried, because a first delivery whose write FAILED would already have claimed
  the event, so Stripe's retry would see a duplicate and SKIP the
  `paid_subscription_started` emit → MRR undercount. With the claim made only
  after the writes commit, the claim row exists iff a delivery SUCCEEDED, so the
  analytics emit is **exactly-once on the first successful delivery**: a first
  attempt that failed never claimed (it 500'd first) so the retry completes the
  writes + emits; a retry after a prior success hits the PK conflict and skips the
  emit (no double MRR) while the idempotent writes re-run. The emit is
  best-effort (analytics, not money-path): an emit failure is caught + logged and
  does NOT fail the webhook — writes succeeding + 200 is the success contract. All
  existing handler logic (payment_failed terminal-only, cancel/downgrade
  propagation, deleted-without-`customer` fallback, subscription.created backfill)
  is unchanged — only the await + the 500-on-failure + the claim ordering moved.
  New tests cover: write-failure→500→redelivery→exactly-once emit, happy path,
  true-duplicate redelivery (no second emit), and emit-failure-still-200.
- **Stripe webhook double-counted MRR on redelivery and could leave a
  cancel-without-`customer` entitled (money-path; #36, follow-ups to #34/#178).**
  Wired the durable dedup table `stripe_webhook_events_processed` (migration
  0044) into `apps/signup-worker/src/webhooks/stripe.ts`: immediately after
  signature verification and before any mutation/emit, the handler claims
  `event.id` via `INSERT OR IGNORE` on the `event_id` PRIMARY KEY and inspects
  `meta.changes`, then uses the outcome to gate ONLY the non-idempotent
  analytics emit (option b) — NOT the writes. The idempotent entitlement/billing
  writes ALWAYS run: a first delivery (changes=1) processes + emits; a Stripe
  redelivery (changes=0, PK conflict) STILL re-runs the idempotent upserts
  (re-converging D1 — the recovery path for a prior fire-and-forget write that
  failed) but SKIPS the emit so `paid_subscription_started` MRR no longer
  double-counts. This deliberately does NOT whole-handler short-circuit a
  duplicate: because the entitlement writes are dispatched via `ctx.waitUntil`
  (not awaited before the 200), short-circuiting a redelivery would permanently
  lose a prior delivery's failed entitlement write (Stripe stops at the 200 →
  customer paid, no access, no recovery). The claim is claim-then-process and
  fail-safe: a D1 claim error is treated as a first delivery (process + emit),
  never dropping a genuine first-time entitlement/revenue event. Also mirrored
  the #178 FIX-2 fallback onto
  `customer.subscription.deleted`: when the deleted subscription object carries
  no top-level `customer`, revocation falls back to
  `deactivateTierSelectionBySubscription` (keyed by subscription id via
  `tenant_billing`) so a cancel always revokes the canonical access gate.
  Hardened the re-subscribe test's activation-timestamp assertion to anchor on
  the handler's real wall clock instead of the signature timestamp.
- **Cutover runbook contradicted the live prod reality — two launch-day landmines.**
  (1) `scripts/apply-d1-migrations-prod.sh` HARD-PAUSED when the migration file count
  ≠ 52, but the adapter/billing/pilot work added migrations 0053–0061 → the live count
  is **60**. The runner would have refused to apply on launch day. Updated
  `EXPECTED_FILE_COUNT` 52→60 and `EXPECTED_TABLE_COUNT` 75→80 (re-derived for the
  60-migration set; post-apply floor check). (2) `scripts/dns-prod-plan.sh` +
  `dns-prod-verify.sh` still encoded the DEAD dotted `*.corelink.humangr.com` CNAMEs
  (plus 4 dead wave-29 extras: acme-dev/staging/sandbox/go), contradicting the live
  flat hosts AND the already-flat `smoke-prod-corelink.sh`. Reconciled both to the flat
  scheme (5 live hosts + the deliberate `status` BetterUptime CNAME). Adds
  `docs/operator/launch-day-sequence-2026-06-09.md` (the verified operator launch
  sequence + owner-flags).
- **Live operational configs routed to DEAD dotted hosts
  (`*.corelink.humangr.com`) instead of the canonical flat hosts
  (`corelink-<surface>.humangr.com`).** Production serves on the flat scheme
  (api/app/docs/signup/admin); the dotted scheme does not resolve (NXDOMAIN). A
  partial Wave-32 flat-rename left stale dotted references that (a) kept `e2e-prod`
  red since 2026-06-04 (`ENOTFOUND docs.corelink.humangr.com`), (b) made
  `apps/docs/functions/_middleware.ts` 301-redirect `*.pages.dev` traffic to a dead
  host, and (c) pointed the docs `SITE_URL`/canonical + a customer-facing docs link
  at dead hosts. Repointed to flat: `e2e-prod.yml` E2E url defaults, docs middleware
  `CANONICAL_HOST`, `docusaurus.config.ts` `SITE_URL` + "Admin" nav link, the
  admin-ui audit-export docs link, and test fixtures (`portal.rs`,
  `middleware-pages.test.ts`). Decision recorded in
  `docs/operator/host-scheme-canonical-2026-06-09.md` (flat = canonical), which also
  flags owner-only items: the CSP Clerk host (auth-critical), `get.corelink.io`, the
  still-dotted `dns-prod-plan.sh`, the customer `.mdx` sweep (PR 2), and the stubbed
  billing portal. Surgical — sealed audits, WebAuthn vectors (`evil.corelink…`), and
  historical docs untouched.
- **Stripe webhook did not propagate cancel / payment-failure / downgrade to the
  CANONICAL access gate, and an unknown status failed OPEN (money-path,
  launch-blocking; #34).** `apps/signup-worker/src/webhooks/stripe.ts` only handled
  `checkout.session.completed` + `customer.subscription.updated/deleted`, and the
  updated/deleted arms wrote only `tenant_billing` (the SECONDARY mirror) — they
  left `tier_selections.subscription_state='active'`, which is the CANONICAL gate
  read by the container's `has_active_subscription` (tier_select_store.rs). Result:
  a canceled/non-paying tenant kept full entitlement (and was blocked by
  `AlreadyActive` from re-subscribing). Fixes: (1) added `invoice.payment_failed`
  — on a TERMINAL dunning failure (`next_payment_attempt === null`) set
  `tenant_billing.status='past_due'` (status-only, preserves `current_period_end_ms`)
  AND flip `tier_selections.subscription_state → 'inactive'`; transient first
  attempts are a no-op (fail-safe). (2) `customer.subscription.deleted` now also
  flips `tier_selections.subscription_state → 'inactive'` (revoke + allow
  re-subscribe). (3) `customer.subscription.updated` no longer coerces an UNKNOWN
  Stripe status to `'paid'` (was fail-OPEN; now defaults to `'incomplete'`),
  propagates an in-place price change to `tier_selections.tier` (reverse
  `STRIPE_PRICE_ID_{TIER}` map; unknown price → left untouched), and deactivates
  the gate whenever the status no longer grants access. (4) added
  `customer.subscription.created` to backfill `current_period_end_ms`. All D1
  mutations are idempotent (safe upserts / guarded no-op UPDATEs). FLAGGED:
  analytics MRR emits are not yet deduped against the 0044
  `stripe_webhook_events_processed` table — deferred as a larger change.
  Adversarial-review follow-ups (#178): (a) `activatePaidTierSelection`'s
  `ON CONFLICT DO UPDATE` now also rewrites `subscription_started_at_ms` to the
  fresh activation timestamp — a returning customer whose prior cancel/failure
  had NULLed it would otherwise hit the re-activation arm with
  (`state='active' AND started_at IS NULL`), violating the 0039
  `subscription_started_when_active` CHECK → swallowed throw → paying
  re-subscriber locked out (and able to double-subscribe). (b)
  `customer.subscription.updated` revocation no longer requires the subscription
  object to carry a `customer` field: a non-granting status with no `customer`
  now deactivates the gate by `stripe_subscription_id` (resolved to the tenant
  via `tenant_billing`) — previously fail-OPEN. Added coverage for the activation
  gate write, the re-subscribe sequence, and the no-`customer` revocation.
- **signup-worker bound a customer's PAT to an ORPHAN tenant under concurrent
  duplicate Clerk delivery (correctness race).** `apps/signup-worker/.../clerk.ts`
  `createTenant` generated a random `tenantId`, ran `INSERT OR IGNORE INTO tenant`,
  then returned its OWN generated id. When a Svix retry / a second concurrent
  isolate had already inserted a row for this Clerk user, the UNIQUE
  `idx_tenant_clerk_user_id` index (migration 0056) silently skipped the insert —
  but the caller still minted a PAT and wrote that orphan `tenant_id` into Clerk
  metadata, leaving the customer holding a PAT pointing at a non-existent tenant
  (D1 FKs are off in CF Workers, so nothing caught it). Fix: after the
  `INSERT OR IGNORE`, `SELECT tenant_id FROM tenant WHERE clerk_user_id = ?1` and
  return THAT id — our row if we won the insert, the pre-existing row if we were
  skipped — so the PAT + metadata always bind to the durable tenant.
- **Operator `set_tenant_tier` flowed an unvalidated `tier` string to the D1
  CHECK (#35).** The operator admin mutate path (`routes/admin.rs`
  `into_request` / `into_request_gated`) built `MutateOp::SetTenantTier` from the
  raw JSON `tier` with no enum validation and no case normalization, then mirrored
  it to `UPDATE tier_selections SET tier = ?1`
  (`storage/d1_http.rs::tenant_set_tier`). A capitalized label (`"Team"` — what the
  codebase's own admin tests send) or a `tenant.tier`-only alias (`solo` / `org`,
  valid in migration 0057 but NOT in 0039's `tier_selections.tier` CHECK) produced
  an opaque D1 CHECK-violation 500. Now a shared `normalize_tier_selection` helper
  trims + lower-cases the input and rejects anything outside the canonical 0039
  enum (`free`/`starter`/`team`/`pro`/`enterprise`) with a clean
  `400 invalid_tier` at the route boundary, before the D1 write. **Divergence
  follow-up:** `solo`/`org` exist in `tenant.tier` (0057) but not in
  `tier_selections.tier` (0039); they are rejected for now (additive-only
  auth-migration rule — no destructive 0039 widening).
- **`--mvp-only` prod-secrets push silently omitted launch-critical secrets
  (signups / billing / storage) — MVP launch would silent-disable core features
  (#33).** The container reads several secrets via `from_env`/Option (the route or
  feature is skipped, not errored, when absent), but they were missing from
  `scripts/secrets-mvp-allowlist.txt`, so a `put-secrets-prod.sh --mvp-only`
  launch would NOT push them. Added the genuinely-MVP-core ones to the allowlist:
  `CORELINK_INTERNAL_AUTH_KEY` + `CORELINK_DPA_VERSION` (without them
  `tier_select.rs` and `internal_pat.rs` do NOT mount → **no signups**);
  `STRIPE_WEBHOOK_SECRET` (without it `main.rs:337` skips the webhook route →
  **customer pays, no tier**); `STRIPE_PRICE_ID_TEAM` + `STRIPE_PRICE_ID_PRO`
  (`client.rs:649` resolves price per tier → Team/Pro checkout 502'd, only
  STARTER worked); `R2_S3_ACCESS_KEY_ID` + `R2_S3_SECRET_ACCESS_KEY`
  (`StorageEnv::from_env` → absent ⇒ InMemory fallback ⇒ **CAS/AC data loss**).
  Also fixed the `CLOUDFLARE_API_TOKEN` → `CF_API_TOKEN` name in the allowlist to
  match what the container actually reads (`storage.rs:85`), and added the missing
  `CLOUDFLARE_ACCOUNT_ID` to `scripts/put-secrets-regional.sh`'s per-region secret
  set (`storage.rs:84` requires it; `wrangler.toml:551` already documented it).
  No code-vs-matrix drift introduced — every added secret already has a
  `secrets-checklist.md` row; `validate_secrets_matrix.py` stays `code_only=0` and
  `secrets-checklist-verify.sh` stays no-drift.
- **Stripe webhook was 401-rejected at the Worker edge — paid checkouts never
  granted a tier (LAUNCH-BLOCKER).** `worker/src/index.ts`'s `matchRoute` had no
  `/v1/billing/*` arm, so `POST /v1/billing/stripe-webhook` fell into the generic
  `/v1/*` `reapi_v1` bucket and hit the Bearer-PAT gate (`extractAuth`). Stripe
  sends only a `Stripe-Signature` header (no PAT), so the Worker 401'd the request
  before it reached the container — no Stripe event was ever processed. Added a
  `billing_webhook` route-kind + an EXACT-path `matchRoute` arm (placed before the
  generic `/v1/*` arm) and a pass-through branch (mirroring the OCI carve-out)
  that forwards the webhook to the shared `_system` DO with NO PAT gate. The raw
  request body is forwarded UNCHANGED (`new Request(request, { headers })` — never
  read/parsed/re-serialized) so the container's Stripe HMAC verifies over the
  exact signed bytes; `Stripe-Signature` is preserved; client-suppliable trust
  headers (`x-corelink-scope`/`x-admin-*`/`x-corelink-tenant-id`) are stripped
  (the container is the sole tenant authority, deriving it from the signed event).
  The Worker adds NO signature/PAT validation — the container's HMAC verify
  (constant-time, replay-windowed) is the sole authority.
- **PAT scope launch-blocker — provisioning wrote a value the D1 CHECK rejects.**
  The prod `pat.scope` column is constrained to `('read-write','read-only','admin')`
  (migration 0037), but `signup-worker/clerk.ts` persisted `scope='cas:rw'` — so the
  first real self-serve signup would have failed its PAT INSERT with a CHECK
  violation and the customer would never receive a token. Verified on live
  `corelink-prod-d1` (25 rows, all legacy `admin`; the `cas:rw` path never wrote).
  Auth migrations are additive-only (INV-AUTH-MIGRATION-ADDITIVE, HIGH — no
  destructive `pat` rebuild without an ADR), so the fix is code-side: `clerk.ts`
  now provisions `read-write` (CHECK-valid, **non-admin** → least privilege), and
  `scope.rs` `requires_cache_read/write` additively accept `read-write` (→ rw) and
  `read-only` (→ read) alongside the colon grammar. One change covers every surface
  (the unified `verify_capability`, OCI `/token` downscope, all five adapters); the
  mint route already mapped `read-write`→cache-rw and the signed bitset is unchanged.
  Gated least-privilege back-fill of the 25 legacy admin rows (`admin → read-write`,
  no migration) in `scripts/backfill-admin-scope-prod.sh`; deploy + follow-ups in
  `docs/operator/launch-pat-scope-fix-runbook.md`.
- **`tier_selections` UPSERT now writes columns that actually exist.** The wasm32
  production tier binder's `SQL_UPSERT_TIER`
  (`corelink-billing-stripe-materializer::wasm32_binders`) referenced a
  non-existent `materialized_at_ms` column and omitted the NOT-NULL
  `subscription_state`, so every webhook-driven tier change would have failed at
  bind/execute against the real D1 schema (`migrations/d1/0039_tier_selection.sql`).
  The statement now binds `(tenant_id, tier, subscription_started_at_ms,
  correlation_id)` and writes `subscription_state = 'active'` (the materializer owns
  the active transition, consistent with `tier_select_store.rs::persist_free_active`),
  satisfying the `subscription_started_when_active` table CHECK. `upsert_tier` gained
  a `now_ms: i64` activation-timestamp arg threaded from the
  `persist_tier_change` caller through the trait, the InMemory mirror (no behavior
  regression), and the wasm32 binder; bind order keeps `tenant_id` first so the
  `verify_first_bind` ct-eq tenant probe is preserved. Added a unit test pinning the
  statement shape (four binds, `'active'`, timestamp column, no `materialized_at_ms`).
- **Prod deploy gate now asserts the Team + Pro Stripe price IDs are
  populated.** `cf-deploy-prod.yml`'s required-prod-secrets check listed only
  the Stripe key/webhook secret, so a Cloudflare env missing
  `STRIPE_PRICE_ID_TEAM`/`STRIPE_PRICE_ID_PRO` could deploy a Worker whose
  Team/Pro upgrade buttons resolve no price and return a 502 at checkout. Both
  price IDs are added to the deploy's `REQUIRED[]` assertion array, given rows
  in `docs/internal/secrets-checklist.md` (the gate-enforced matrix, #109/#110),
  and recorded in the new `specs/_compliance/secrets-matrix.md` GA-cutover
  anchor (which also resolves the dangling `RB-GA-CUTOVER.md` §1.3 reference to
  that path). Secrets-matrix gates (`validate_secrets_matrix.py` code_only=0,
  `secrets-checklist-verify.sh` no drift) stay green.
- **admin-ui Lighthouse a11y `document-title` on `/en/privacy` +
  `/en/consent/new`.** Both routes are dynamic (they `await params`) under the
  `force-dynamic` root layout and previously carried no page-level metadata, so
  they inherited the root layout's `<title>` — which Next 15 streams via
  `AsyncMetadataOutlet`/`MetadataBoundary`. Lighthouse's headless run strips
  streamed metadata from the post-hydration DOM, failing the `document-title`
  audit (≈0.77 a11y) even though the title renders in real prod. Each page now
  exports a static `metadata` (title hoisted into the static `<head>`), matching
  the existing `security/policy/page.tsx` pattern; the page bodies stay dynamic
  + locale-aware and the localized in-page headings are unchanged. (`/` was
  already covered by the static `metadata` on the new landing `app/page.tsx`.)
- **Container env contract: empty string now means "use default"** for
  `R2_AC_BUCKET`/`R2_AC_REGION`/`R2_CAS_BUCKET`/`R2_CAS_REGION` (new
  `storage::env_or` helper). The DO forwards `?? ""` with a documented
  absent/empty→default contract that the Rust side violated — every
  `/v1/ac/*` op in prod returned 500 ("failed to construct request" on an
  empty bucket name) since the 2026-05-30 deploy. Found by clw dogfood
  day-1; reproduced locally byte-for-byte. Also: `[env.prod]` now sets
  `R2_AC_BUCKET`/`R2_AC_REGION` explicitly (codifies the 2026-06-05
  API-applied hotfix so the next deploy cannot regress it);
  `cf-deploy-prod.yml` installs wrangler@4 (v3 cannot parse the current
  `[[env.*.containers]]` schema and would fail before deploying);
  `mint-pilot-token.sh` header points at the live flat hostname.
- **Lighthouse CI no longer orphans its server + headless Chrome onto the
  shared self-hosted Mac.** `lighthouse-ci.yml` (admin-ui) runs LHCI with a
  `startServerCommand` standalone `node server.js` (:3000) + headless Chrome but
  had no teardown; with `concurrency: cancel-in-progress: true` on the sieged
  single-Mac runner fleet, cancelled runs left those processes reparented to
  launchd, accumulating across days (observed: a 6-day-old 186 MB server + 18
  stranded Chrome, contributing to the RAM exhaustion + swap thrash that
  inflated the host load average into the hundreds). Both `lighthouse-ci.yml`
  and the `docs-ci.yml` lighthouse job gain an `if: always()` step that reaps
  ONLY orphaned (PPID == 1) LHCI servers + headless Chrome — safe on the shared
  host (a live sibling job's processes are never PPID == 1) and self-healing
  (each run also sweeps prior runs' orphans).

### Added

- **Clerk edge-verification bridge — completes the self-serve money path
  (launch blocker GAP-5).** `worker/src/index.ts`: a new `/v1/onboarding/*`
  Worker route arm. The browser presents a Clerk SESSION JWT (not a CoreLink
  PAT), so the entire authenticated admin-ui→API surface was severed in prod —
  the Worker only accepted PATs and had no Clerk verification, so every
  onboarding/checkout call 401'd before the backend. The bridge verifies the
  Clerk JWT at the edge (`@clerk/backend` `verifyToken` — signature + issuer +
  expiry + `azp` pinned to the app origin), resolves the CoreLink tenant from the
  verified Clerk user id (`CONFIG_DB`: `tenant.clerk_user_id`, migration 0056),
  then forwards to the tenant Durable Object with `x-corelink-internal-auth` +
  `x-corelink-tenant-id` injected — the exact contract the container `tier_select`
  route requires. Inbound `x-corelink-internal-auth` / `x-corelink-tenant-id` /
  `authorization` are STRIPPED before injection, so a client can never spoof the
  internal-auth secret or the tenant id. Fail-CLOSED: missing
  `CORELINK_INTERNAL_AUTH_KEY`/`CLERK_SECRET_KEY` → 403, bad token → 401, no
  tenant → 403. (Launch-day: confirm the live Clerk session `azp` matches the app
  origin `https://corelink-admin.humangr.com`.)
- **Public landing page** (ROADMAP-TO-GA Phase 1 L1). `apps/admin-ui/src/app/page.tsx`
  replaces the bare "CoreLink Admin" admin shell with a real first-visitor landing
  page, porting positioning from `marketing/launch/PILOT-LANDING-PAGE-COPY.md`: hero
  (what CoreLink IS — a shared, multi-tenant content-addressable cache for builds /
  packages / ML), **primary CTA → sign-up** (secondary sign-in kept), three feature
  blocks (TLA+ tenant isolation, Ed25519 Merkle audit chain, 4-region residency), a
  wireable 30-second-demo slot (env-driven `NEXT_PUBLIC_DEMO_URL`, no fabricated video),
  and a locale-aware footer linking `/pricing`, `/legal/terms`, `/privacy`. New
  `landing.*` i18n keys added at parity across all four locales (en/pt/es/de).
- **admin-ui `/[locale]/upgraded` checkout-success page** (Launch L3). The Stripe
  Checkout `success_url` minted by `apps/admin-ui/src/app/api/checkout/session/route.ts`
  (`${origin}/${locale}/upgraded?session_id={CHECKOUT_SESSION_ID}`) had no matching
  route, so customers hit a 404 immediately after paying. Adds the missing server
  component (`apps/admin-ui/src/app/[locale]/upgraded/page.tsx`) mirroring the
  `[locale]/pricing` conventions: a clean "Subscription started — your plan activates
  in a moment" confirmation with links back to the dashboard and billing. Purely
  informational by design — `session_id` is NOT treated as proof of payment; tier
  activation stays owned by the idempotent `checkout.session.completed` webhook in
  `corelink-tier-selection`. Adds the `upgraded.*` message keys to
  `src/i18n/locales/en.json`.
- **Self-serve tier-select checkout backend — ironclad core** (WI-S19-004 PRR
  wiring). `crates/corelink-container/src/routes/tier_select.rs`: the
  transport-agnostic, fail-CLOSED core of `POST /v1/onboarding/tier-select`.
  Constant-time internal-auth gate + edge-verified `x-corelink-tenant-id` only
  (no client-supplied tenant) + the durable orchestration behind
  `TierSelectStore` / `CheckoutCreator` / `TierSelectAudit` trait seams:
  audit-before-mutate; INV-ONBOARD-DPA-FIRST (Stripe is never called without DPA,
  proven by spy); durable 60s lock → 409; UNIQUE active subscription → 409;
  Stripe failure → 502 + lock release; free → instant. 17 adversarial tests.
  Production D1/Stripe adapters + the route mount land in a follow-up.
- **Tier-select production wiring — scaffold** (WI-S19-004; L3 money path).
  Frozen adapter surfaces for the three `tier_select.rs` trait seams:
  `routes/tier_select_store.rs` (`D1HttpTierSelectStore` over the durable
  D1-over-HTTP transaction), `routes/tier_select_checkout.rs`
  (`StripeCheckoutCreator` over `StripeRealClient` via `spawn_blocking`),
  and `routes/tier_select_audit.rs` (`TierSelectAuditAdapter`,
  fail-CLOSED). `TierSelectRouteState` now carries the wired collaborators
  (`store` / `checkout` / `audit` / `current_dpa_version`) alongside the
  redacted `internal_auth_key`; every adapter `Debug` redacts its
  credential. Method bodies are `todo!()` stubs (the effects land in the
  follow-up WPs). **Email-seam decision:** the `CheckoutCreator` trait
  stays email-free and the production adapter passes an empty
  `customer_email` — Stripe's hosted Checkout page collects the buyer email
  itself (the SOTA pattern), so no PII is threaded from the Worker and
  `CheckoutSessionRequest` is left unchanged.
- **Tier-select production wiring — adapters + handler implemented** (WI-S19-004;
  L3 money path; the follow-up the scaffold deferred). The three trait seams now
  carry real effects: `D1HttpTierSelectStore` runs the durable lock /
  DPA-acceptance / active-subscription / persist statements via `D1HttpClient::query`
  against migrations 0039/0038 (UPSERT keeps each write a single statement —
  D1-over-HTTP has no cross-request transaction, so the daily reconciliation cron
  is the UPDATE→INSERT drift backstop); `StripeCheckoutCreator` calls the real
  `StripeRealClient::create_checkout_session` under `spawn_blocking` (empty
  `customer_email`); `TierSelectAuditAdapter` emits a structured `tracing` audit
  (durable D1 audit-chain write deferred to Wave-37, mirroring `internal_pat`).
  The axum `handle` + `router` (WP-E.1) wire the gate + orchestration; the body is
  taken as raw `Bytes` and parsed so every failure returns the one
  `{ "error": code }` envelope. Adds `CheckoutSessionRequest::new` (the
  `#[non_exhaustive]` constructor downstream crates need, mirroring its siblings).
  clippy `-D warnings` green; 20 lib tests pass. **WP-E.2 (mount) now landed:**
  `build_state_from_env` assembles the production `TierSelectRouteState`
  (fail-safe — the route mounts ONLY when the internal-auth secret ≥16 chars +
  D1 config + Stripe config + `CORELINK_DPA_VERSION` are ALL present; a missing
  one leaves `/v1/onboarding/tier-select` unmounted, 404, rather than half-wired)
  and `main.rs` merges `router()` behind the same env gate as `internal_pat`;
  clippy `-D warnings` green on lib + bins. **Remaining before it serves
  traffic:** the live-D1 / live-Stripe `#[ignore]` integration tests (need a test
  D1 with migrations 0038/0039 + `sk_test_` keys).
- Customer-facing CHANGELOG generation tooling (`scripts/generate-changelog.sh`)
  and PR-level enforcement workflow (`.github/workflows/changelog-validate.yml`).
- Customer-facing **release-notes auto-generator** (`scripts/generate-release-notes.py`)
  with `--from / --to / --dry-run` flags, tag-trigger CI
  (`.github/workflows/release-notes.yml`), polish template
  (`releases/TEMPLATE.md`), and operator editorial guide
  (`marketing/launch/RELEASE-NOTES-EDITORIAL-GUIDE.md`). On every `v*` tag
  push, CI generates `releases/RELEASE-<version>.md`, opens an editorial PR,
  and creates a draft GitHub Release.
- **Wave-36 Trigger A** — new leaf crate `corelink-billing-stripe-traits`
  (4 traits + 9 types) extracted to break the
  `corelink-stripe-real ↔ corelink-billing-materializer` dep-graph cycle
  surfaced during Wave-33 stage-2.C consumer migration.
  Tag `wave-36-final-sealed`. SEAL:
  `specs/_audits/sealed/2026-05-27-w36-trigger-a-seal.md`.
- **Wave-36 proptest follow-ups** — +14 proptests across three adapter
  crates: 6 in `corelink-wasm` (`put` / `get` / `stat` JS/TS surface),
  4 in `corelink-clerk-cf`, 4 in `corelink-statuspage-real`. SEALs:
  `specs/_audits/sealed/2026-05-26-w36-proptest-wasm-seal.md`,
  `specs/_audits/sealed/2026-05-26-w36-proptest-fu-002-seal.md`.

### Changed

- **Prod deploy gate no longer requires the Slack webhook secrets.** The
  `cf-deploy-prod.yml` required-secrets check listed
  `SLACK_WEBHOOK_URL_ALERTS_SEV1/SEV2` + `..._BREACH_NOTIFICATIONS`, but
  operational alerting currently routes via PagerDuty (`PAGERDUTY_ROUTING_KEY`,
  still gated) and direct Slack delivery is a not-yet-wired enhancement
  (`corelink-slack-real`'s `WebhookRegistry::from_env` already omits unset
  channels and dispatch fails closed with `ChannelUnconfigured` — no startup
  break). The gate was blocking deploys on secrets nothing consumes; the three
  `SLACK_WEBHOOK_URL_*` entries are dropped from `REQUIRED[]` (with an in-place
  comment to re-add them if Slack becomes load-bearing). The secrets-checklist
  rows + crate are unchanged.
- **Public API-reference + subprocessors docs regenerated** to catch up with
  sources that advanced without a docs refresh, greening the two `--check`
  drift gates. `openapi/corelink-v1.yaml` was synced 2026-05-30 (f10f0c54) but
  the 50-endpoint MDX set under `apps/docs/docs/reference/api/endpoints/` was
  last regenerated 2026-05-16 (4094165b); `python3 scripts/gen-api-reference.py`
  brings them current (+24 new endpoint pages, 27 updated, 1 stale removed).
  `apps/docs/docs/trust/subprocessors.mdx` re-derived from the
  `VENDOR-RISK-REGISTER` via `gen-public-subprocessors.py` (deterministic
  `last_updated` from the register, not wall-clock).
- **Wave-35 Phase-2 absorption campaign** — 9 absorption SEALs collapsed
  the bulk of Wave-33 satellite crates into umbrella canonical paths
  (CAS, TELEMETRY, ADAPTER-HOST, REPLICATION, BILLING, PRIVACY, OPS, AC,
  BYOK). Workspace `members` reduced wave-over-wave per each per-umbrella
  SEAL audit. Tag `wave-35-phase-2-sealed`. SEAL audits live at
  `specs/_audits/sealed/2026-05-26-w35-p2-{cas,telemetry,adapter-host,
  replication,billing,privacy,ops,ac,byok}-absorption.md`.
- **Wave-36 Stage 2.C** — 5 consumer migration sites flipped from
  absorbed-adapter direct imports to the canonical umbrella paths
  (`corelink_billing::stripe::real::*` and peers). SEAL:
  `specs/_audits/sealed/2026-05-26-w36-stage2c-closure.md`. Tag
  `wave-36-stage-2-sealed`.

### Deprecated

- (none)

### Removed

- (none)

### Fixed

- **admin-ui CI had been dark for 40+ runs, masking a backlog of real failures.**
  Root cause: `pnpm/action-setup` installs pnpm into a shared `~/setup-pnpm`, and
  all five self-hosted runners share one `$HOME` on the builder Mac — so
  concurrent runs corrupted each other's pnpm (`MODULE_NOT_FOUND …/dist/worker.js`)
  and `pnpm install` died before typecheck/lint/test/build ever ran. Fixed with a
  `./.github/actions/setup-pnpm` composite that uses the runner's pre-installed
  pnpm (offline, no shared-dir race; falls back to the pinned download only when a
  runner lacks pnpm). With CI able to run again, three latent admin-ui breaks
  surfaced and are fixed here: (1) **jest-dom matchers never registered** — jest-dom
  (a single hoisted instance, no vitest peer) resolved `vitest@4.1.7`, the highest
  in this multi-version monorepo, and extended the wrong `expect`, so 120 tests
  failed with "Invalid Chai property"; the setup files now extend the local
  vitest@3.0.7 `expect` directly (and a `matchers.d.ts` `import()` type-query fixes
  the matching type side) — 316/316 green; (2) **nine `dsr`/`consent`/`admin`
  routes read Next 15's now-`Promise` `params` synchronously** — migrated to
  `await params`; (3) an invalid **`aria-readonly` on the consent third-parties
  list** (a `list`-role `ul`) was an accessibility violation — removed.
- **Paid subscriptions never reached the `active` state the money path reads —
  returning paid customers could open a second subscription (launch blocker
  GAP-6).** The live Stripe webhook (`apps/signup-worker/src/webhooks/stripe.ts`)
  wrote only `tenant_billing` on `checkout.session.completed`, but the container's
  `has_active_subscription` guard reads `tier_selections.subscription_state =
  'active'` — and nothing on the live path advanced it from `pending_checkout`
  (the `corelink-tier-selection::ledger` that does is test-only). So a paid tenant
  stayed `pending_checkout`, the `already_active` re-charge guard stayed inert, and
  a returning customer who picked another tier opened a second Stripe subscription.
  Added an idempotent `tier_selections` UPSERT → `active` in the webhook's
  `checkout.session.completed` handler. Also fixed a latent bug there: it read the
  never-set `metadata.plan` (recording `"starter"` for every customer) instead of
  the real `metadata.tier`. (Follow-up hardening noted: the webhook writes are
  `waitUntil` fire-and-forget; a failed write isn't retried — and a read-side
  `tenant_billing` fallback would add defense-in-depth.)
- **Production hostname mismatch — runtime code pointed at the dead
  `*.corelink.humangr.com` form while the live DNS is the flat `corelink-*`
  pattern (launch blockers GAP-2/GAP-3).** `dig` confirms `api.corelink.humangr.com`
  and `app.corelink.humangr.com` do **not** resolve, whereas `corelink-api`,
  `corelink-admin`, `corelink-app`, `corelink-docs` are live (Wave-32 sign-off).
  The admin-ui server API client (`api-client.ts`/`dsr-client.ts`) defaulted to
  the dead `https://api.corelink.humangr.com`, so absent the `CORELINK_API_URL`
  env the server could not reach the backend; the CSP `connect-src` and the
  Worker CORS allowlist (`worker/src/index.ts`) likewise allowed only dead
  origins. Repointed all runtime references to the live flat hosts: admin-ui API
  client + DSR client defaults, CSP `connect-src` → `corelink-api`, CSP-report +
  billing-portal return + newsletter CORS fallback, and the Worker CORS
  allowlist → `corelink-admin`/`corelink-app`/`corelink-docs`. `clerk.corelink.humangr.com`
  is intentionally retained (Clerk's required Frontend-API CNAME format). The
  Stripe checkout redirect is origin-relative (`originFromRequest`) and already
  self-corrects to the serving host. (Deferred to a post-launch follow-up:
  WebAuthn RP origins, multi-region `{region}.api.*`, pilot-signup + rate-limit
  doc links, and e2e/OpenAPI-JSON references — none on the launch path.)
- **L3 money-path route 404'd in production — the Durable Object never forwarded
  the Stripe + DPA env to the container (launch blocker GAP-4).** `/v1/onboarding/
  tier-select` runs INSIDE the container and reads `STRIPE_SECRET_KEY`,
  `STRIPE_PRICE_ID_{STARTER,TEAM,PRO}`, `STRIPE_AUTH_MODE`, `STRIPE_WEBHOOK_SECRET`,
  and `CORELINK_DPA_VERSION` from its own process env (`build_state_from_env` +
  `StripeRealClient::from_env`), but `worker/src/durable_object.ts`'s
  `container.start({env})` allowlist forwarded only R2/CF/D1/PAT/internal-auth —
  so `build_state_from_env` returned `None` (missing DPA version) and the route
  stayed UNMOUNTED (404); paid checkout would also 500 (missing price ids). Added
  all seven vars to the DO allowlist + declared them on the Worker `Env`
  interface (`worker/src/index.ts`).
- **admin-ui deploy workflow pointed at the dead Cloudflare Pages path — would
  fail on every run (launch blocker GAP-1).** `.github/workflows/admin-ui-deploy.yml`
  invoked `pnpm pages:build` + `npx wrangler pages deploy .vercel/output/static`,
  but `apps/admin-ui` migrated Pages → Worker (`@cloudflare/next-on-pages` returned
  HTTP 500 on every Next 15 SSR route) and `apps/admin-ui/package.json` exposes only
  `cf:build` (`opennextjs-cloudflare build`) and `cf:deploy` (`wrangler deploy`) — the
  missing `pages:*` scripts meant the build step errored immediately. Rewrote the
  workflow onto the Worker path that `package.json` + `wrangler.toml` actually support:
  `pnpm cf:build` → `pnpm cf:deploy`, dropping the Pages-only `pages deploy` flags
  (`--project-name`/`--branch`/`--commit-*`) since `wrangler deploy` reads name, entry
  (`.open-next/worker.js`), assets, and the custom-domain route from `wrangler.toml`.
  Trigger, runner, concurrency, timeout, permissions, the `NEXT_PUBLIC_*` build-arg
  injection, and the CF secrets are preserved unchanged. `actionlint` clean. Also
  refreshed the admin-ui README deploy section to the Worker path and flagged a
  follow-up: the `*.pages.dev` host-block in `functions/_middleware.ts` (a CF Pages
  Functions file) does not run under the OpenNext Worker, so default-subdomain
  lockdown is now an open security item.
- **L3 tier-select Stripe checkout — racy money-path bug fixed (caught by the
  live verification).** `StripeRealClient` wraps a persistent
  `reqwest::blocking::Client`, which reqwest forbids using inside a tokio
  runtime; `tokio::spawn_blocking` does NOT make it safe (its pool threads still
  carry the runtime context), so `StripeCheckoutCreator::create` intermittently
  panicked at tokio's blocking-runtime shutdown. `create` now ferries the
  blocking Stripe call to a DEDICATED `std::thread` (zero tokio context) and
  awaits over a `oneshot` — reqwest's prescribed pattern for a blocking client in
  async. Verified end-to-end: a real Stripe test-mode Checkout Session was
  created (valid `cs_`/`cus_`/`https` response). Adds the `#[ignore]` live harness
  documenting the manual command; removes an unused `use super::*` in the store
  test module.
- **CI heavy-gate stampede on every `main` merge — removed `push: main` from the
  17 compute-heavy gates (root cause of the 2026-06-04 self-hosted-Mac load
  meltdowns).** A docs-only merge (#131) fired the full heavy on-main suite at
  once across the Mac runner fleet → load 398 (later 665, stacked with the
  operator's own terraform + game). These gates already carry staggered nightly
  `schedule:` crons + `workflow_dispatch`; the redundant `push: branches:[main]`
  trigger is removed so a merge no longer stampedes them — they now run
  **nightly (staggered) + on-demand** (the documented intent). The path-filtered
  ship gates (`s07`/`s08`/`s09`/`gc`/`region_pinning`) keep their targeted
  `pull_request: paths:` triggers (PR rigor preserved where it is cheap);
  `s07`/`s08`/`s09`/`gc` gain staggered nightly crons (08:00/08:30/09:00/11:00,
  matching `s10`). Light gates (lint/docs/validate) keep `push: main` — they are
  fast. `actionlint` clean across all 17. Files: `coverage`, `reproducible-build`,
  `ffi-matrix-ci`, `cas_foundation`, `codeql`, `semgrep`, `s07`–`s10`+`gc`
  ship-gates, `tla_*` (5), `region_pinning`.
- **CI heavy-gate stampede — extension: removed `push: branches:[main]` from the
  remaining Rust-COMPILING gates the first sweep missed.** After the 17-gate sweep
  above (977225d3), a handful of cargo-compiling gates still carried
  `push: branches:[main]` and re-fired (and had to be cancelled) on the next main
  merge, re-loading the self-hosted macOS fleet (load 398/665, 2026-06-04). Moved
  **9 gates** off `push: main` → **nightly (staggered) + targeted-PR + on-demand**:
  `cargo-audit` (compiles the tool via `cargo install --locked`; keeps its 06:00
  cron), `cargo-deny` (resolves + builds the full dep graph; keeps its 06:30 cron),
  `proptest-density-gate` (compiles the workspace — the documented near-OOM driver;
  had **no** cron, so gained a staggered `31 4 * * *` + `workflow_dispatch`), and
  the six per-crate component gates that run `cargo clippy` + `cargo test`
  (debug + release) on the Mac fleet — `corelink-hash` (04:37), `corelink-meta`
  (04:47), `corelink-reapi` (04:53), `corelink-worker` (04:53),
  `corelink-client-verify` (04:41), `tenant-path` (04:17) — each keeping its
  existing nightly cron. Every one retains its `pull_request: paths:` lane (PR
  rigor preserved where it is cheap + targeted). `push: branches:[main]` blocks
  with nested `paths:` lists were removed whole (no orphaned `paths:`).
  Deliberately **left on `push: main` for the orchestrator to review** (not
  in-scope for a "Rust-compile/benchmark" move): `buck2-starter-ci` (runs on a
  separate `[self-hosted, Linux, X64]` pool, and its push lane is load-bearing —
  it auto-commits `BENCHMARK.md` on main) and `smoke-install` (a `docker build` +
  `docker run` of a *prebuilt* binary — no cargo/source compile, and already
  self-skips when Docker is absent). `perf-regression` already carried no
  `push: main` (PR + `workflow_dispatch` only) — no change needed. Light gates
  keep `push: main`. `actionlint` clean across all 9 changed files.
- **`slo-instrumentation` gate — sealed audit-doc reference-rot.**
  `scripts/validate_slo_instrumentation.py` hardcoded
  `specs/_audits/2026-05-14-slo-instrumentation-gaps.md`, but the `_sealed/`
  archival reorg (2026-05-31) moved it under `_audits/sealed/`, so the gate
  failed with `[FAIL] missing audit doc … (load-bearing reference)`. `AUDIT_DOC`
  now resolves to whichever location exists (sealed-first), surviving a re-seal
  in either direction. Gate green; the full validator passes (30 declared SLOs,
  0 MISSING, 0 orphan SLI slugs). Same reference-rot class as the BASELINE_PATH
  / sprint-contract repoints.
- **`spec-validation` gate — §4 TLA+ obligation matrix completed for 16 reconciled INVs (completes #121).** The 15 HIGH proptest/algorithmic invariants (BAZEL, CLERK, STATUSPAGE, WASM families) from `specs/03_architecture/invariant_registry.md §3.31` now have §4.3 non-TLA+ obligation rows; `INV-CROSS-TENANT-DENIED` (CRITICAL) is placed in §4.3 as an algorithmic refinement of the GREEN `tenant_isolation.tla` spec with a `<!-- techlead-review -->` flag for confirmation. `check_tla_obligations.py` exits 0 (was 16 errors). Also carries PR #121 gate fixes: `validate_canonical_consistency.py` `BASELINE_PATH` repointed to `specs/_audits/sealed/`; false-positive `INV-OPS`/`INV-pin` prose tokens fixed in `corelink-ops`, `corelink-container`, `corelink-core`.
- **Complete Wave-35 rename-rot sweep — stale `cargo --test/--bench` target
  names in CI workflows and runbook scripts.** Wave-35 absorption prefixed test
  and bench filenames with a module name (e.g. `tests/prop_quota.rs` ->
  `tests/quota_core_prop_quota.rs`), but 40+ `cargo test --test <X>` /
  `cargo bench --bench <X>` invocations across `.github/` and `scripts/` still
  used the old basenames, causing `error: no test target named <X>` (exit 101)
  in ~1.5 s. PR #126 fixed 9; this sweep closes the remaining 13 distinct stale
  refs (30+ invocation sites) across 8 files:
  `nightly.yml` (8 refs), `region_pinning.yml` (3 refs),
  `d1-migration-validate.yml` (1 ref + comment),
  `scripts/rb_fm_250_dry_run.sh` (5 refs incl. package rename
  `corelink-edge` -> `corelink-cas`, `corelink-quota-cas` -> `corelink-billing`,
  `corelink-abuse` -> `corelink-billing`),
  `scripts/rb_fm_059_dry_run.sh` (4 refs, `corelink-quota` -> `corelink-billing`),
  `scripts/rb_billing_001_replay_forensic_dry_run.sh` (5 refs,
  `corelink-billing-replay` -> `corelink-billing`),
  `scripts/rb_fm_302_billing_drift_dry_run.sh` (1 ref),
  `scripts/rb_region_leak_dry_run.sh` (4 refs,
  `corelink-privacy-residency-enforcement` -> `corelink-privacy`).
  Old -> new mapping: `prop_dedup` -> `dedup_prop`, `prop_quota` ->
  `quota_core_prop_quota`, `prop_lru` -> `lru_tracker_prop`, `prop_edge` ->
  `edge_prop`, `prop_quota_cas` -> `quota_cas_prop_quota_cas`, `prop_abuse` ->
  `abuse_prop_abuse`, `prop_quota_fsm` -> `quota_fsm_prop_quota_fsm`,
  `prop_billing_replay` -> `replay_prop_billing_replay`, `calibration_abuse` ->
  `abuse_calibration_abuse`, `migration_canonical_0011` ->
  `edge_migration_canonical_0011`, `d1_migration_integration` ->
  `migrations_d1_migration_integration`, `property_region_pinning_30k` ->
  `residency_property_region_pinning_30k`, `region_adversarial` ->
  `residency_region_adversarial`. Zero stale refs remain; all 51 active
  test/bench target refs validated against `find crates -path '*/tests/*.rs'`
  and `find crates -path '*/benches/*.rs'`. Supersedes #123 + #126.

- **`spec-validation` CI gate — `check_cost_regression.py` now resolves sealed
  sprint contracts.** S07 and S09 are real HIGH_RISK hot-path sprints (eviction +
  audit/metrics) that were sealed and moved to `specs/04_sprints/_sealed/`; their
  `_spec_contract.md` files already document the §14.10 cost regression gate.  The
  script was only checking the active path `specs/04_sprints/<sprint>/` and therefore
  reported "not found" for both, making the gate fail.  Added `find_sprint_contract()`
  which checks the active path first then falls back to `_sealed/<sprint>/`; REQUIRED
  set is unchanged (`S07 S08 S09 S10 S14`).

- **`corelink-billing` / `s10-ship-gate` lib-test gate — restored
  `clippy::unwrap_used` discipline in `tier_select.rs` tests.** PR #70 landed the
  tier-select ironclad core but its `#[cfg(test)] mod tests` block omitted the
  crate-standard `#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic,
  reason = …)]` attribute that every other `corelink-container` route test module
  carries (`internal_pat`, `signup`, `cas`, `bazel_v2`, … — 10 siblings). Under the
  ship-gate's restriction-lint lib-test compile this tripped 40 `unwrap_used`
  errors ("could not compile `corelink-server` (lib test)"). Added the standard
  allow block verbatim; no test logic changed.
- **`TLC region_residency` gate (WI-S14-009) — un-stuck the model check, fixed
  two masked spec defects, and made it CI-tractable.** The gate had been failing
  at *config parse* (`ConfigFileException: … expecting ]` at line 18): a TLC
  `.cfg` cannot hold a function literal (`PrimaryRegionOf = [t1 |-> "WNAM", …]`).
  Moved the concrete mapping into the spec as `PrimaryRegionOfImpl`, injected via
  a `CONSTANT PrimaryRegionOf <- PrimaryRegionOfImpl` override. Because the model
  had **never actually run**, fixing the parse un-masked two real defects: (1)
  `NoCrossRegionLeak` flagged the *audit-log entry* of a correctly **rejected**
  cross-region write — scoped the read-leak invariant to `{read, failover_read}`
  (writes are covered at the storage level by `NoCrossRegionWrite`); (2)
  `ReplicationEventuallyConverges` was violated because `replica_lag` (set to
  `MaxLag` on write) could never drain once the single write-once blob was
  replicated — added a fairness-driven `LagTick` that drains the abstract clock
  independent of per-blob `ReplicaSync`. Tightened the never-validated bounds
  (`MaxRequests` 20→2, `Blobs` 5→2) that exploded to >1e6 states; the full
  exhaustive check now passes (34 282 distinct states, depth 11, ~21 s, under the
  60 s CI budget). ASCII-normalised this `.cfg`'s comments while here.
- **`TLA+ Model Check — Runbooks` gate — robust `tlc`-wrapper build on the macOS
  fleet.** `.github/workflows/tla_runbooks_check.yml` built its `tlc` wrapper with
  a quoted heredoc and then injected `$RUNNER_TEMP` via `sed`; the runner path's
  `/` slashes collided with sed's substitution syntax (`sed: extra characters at
  the end of g command`), failing the step before any TLC run. Dropped `sed`
  entirely and now build the wrapper with an unquoted heredoc (shell expands
  `$RUNNER_TEMP` at write-time, `\$@` stays literal), mirroring the working
  `tla_check.yml` / `tla_billing_check.yml` pattern. SHA-256 pin + run logic
  unchanged.
- **`terraform validate` — sensitive `for_each` in `cloudflare-secrets`
  (sealed the secret-leak vector).** With the `terraform-lint` gate finally
  *running* validate (after the `setup-terraform` SHA repin above), the
  staging env failed `Error: Invalid for_each argument` at
  `infra/terraform/modules/cloudflare-secrets/main.tf:53` — `for_each =
  var.secrets` fed the **sensitive** `map(string)` (secret name → value)
  straight into `for_each`, which Terraform forbids because the resulting
  resource-instance keys surface in plan output and could expose the secret.
  Fixed the SOTA-secure way: iterate over the **names only** via
  `for_each = nonsensitive(toset(keys(var.secrets)))` (names are not
  sensitive; verified via `terraform console` to render as the plain key set)
  and look the still-**sensitive** value up *by key* inside the resource —
  `sha256(var.secrets[each.key])` for the re-trigger hash and
  `SECRET_VALUE = var.secrets[each.key]` for the local-exec env (both confirmed
  to render as `(sensitive value)`). The plaintext secret VALUES therefore
  never enter `for_each` / instance keys / plan output. `terraform validate`
  on `environments/staging` (the gate target, TF 1.7.5) now exits 0;
  `terraform fmt -check` stays clean. (`cloudflare-base`'s `for_each` over the
  non-sensitive `dkim_records` map was already valid — untouched.)
- **Owner-key-gated CI gates now pass-or-skip cleanly pre-launch instead of
  falsely red.** Four checks were reddening on owner secrets / a paid feature
  that intentionally do not exist before launch; each is now gated gracefully
  and reactivates the instant the owner provides the key/feature — no test was
  weakened. (1) **CodeQL** (`codeql.yml`): the SARIF upload to the Security tab
  requires **GitHub Advanced Security** (code scanning), which is not enabled on
  this private repo, so the upload returned "Code scanning is not enabled for
  this repository" and reddened the whole job even though the scan passed. Split
  the upload out of `analyze` (now `upload: never`, writing SARIF locally for the
  severity gate) into a dedicated `github/codeql-action/upload-sarif` step marked
  `continue-on-error: true` (`if: always()`) — mirroring the existing tfsec
  pattern in `terraform-lint.yml`. The job status now tracks the actual scan +
  severity gate; once GHAS is enabled the SARIF populates the Security tab with
  no further change. (2) Same one-line `continue-on-error: true` on the
  `upload-sarif` step in **`semgrep.yml`** (its scan already drives pass/fail via
  its own exit code). (3) **`e2e-clerk-signup.yml`** + (4) **`e2e-stripe-checkout.yml`**
  hit LIVE Clerk/Stripe prod via launch-day secrets (`CLERK_SECRET_KEY`;
  `STRIPE_SECRET_KEY` + `STRIPE_WEBHOOK_SECRET`) that are unset pre-launch — added
  a tiny `gate` job that routes the secret(s) through `env` (the `secrets` context
  is not usable in a job-level `if:`) and emits a `run` output; the `e2e` job now
  `needs: gate` + `if: needs.gate.outputs.run == 'true'`, so it **skips** (not
  fails) when the key is absent and runs once set (mirrors the needs-output gate
  in `pentest-findings-sync.yml`). **`smoke-install.yml`**'s existing Docker
  preflight was extended to also skip (green, with a `::notice::`) when its repo
  secret `CORELINK_TEST_TOKEN_CI` is unset. All five workflows `actionlint`-clean.
  GHAS (paid) + the live Clerk/Stripe keys remain the owner's to provide for full
  coverage.
- **TLA+ `billing_atomicity` — fixed the `AggregateCounter` partial-function
  crash + the two unsound strict-equality invariants it was masking (real TLC
  violations, launch-critical billing).** TLC v1.8.0 crashed at State 4 with
  `Attempted to apply function: <<>> to argument <<t1, sku1, p1>>, which is
  not in the domain of the function`. Three findings, all **spec-modeling
  bugs, NOT billing-code bugs** (the production `corelink-billing-aggregator`
  is an UPSERT-safe ledger — `INSERT … ON CONFLICT … DO UPDATE`, row-absent →
  `Inserted` via the `else` branch of `if let Some(prior) = rows.get(&key)` —
  and `corelink-billing-reconcile/src/drift.rs` reconciles the three layers
  with a bounded relative-error drift ladder, i.e. transient inter-layer drift
  is expected, not a defect):
  1. **Function-domain crash** (the reported violation). `AggregateCounter` and
     `GenerateInvoice` gated a counter/line-item read with a `\/` disjunct
     (`key \notin DOMAIN f \/ f[key] # bucket`). TLA+ `\/` is **commutative**,
     so TLC may evaluate the second disjunct even when the domain check is TRUE,
     applying the empty function `<<>>` (the `Init` state of `counters` /
     `invoice_line_items`) to a key outside its domain. Fix: the lazily-
     evaluated `IF key \in DOMAIN f THEN f[key] # bucket ELSE TRUE` (the idiom
     the `counters'` / `invoice_line_items'` writes already used).
  2. **`INV_BILLING_CHAIN_INTEGRITY` was unsound** (surfaced once the crash was
     gone — the abort had masked it). It asserted `invoice_line_items[k] =
     counters[k]` as a per-state invariant, but the invoice line item is a
     **snapshot** taken by `GenerateInvoice` while the aggregator legitimately
     keeps advancing `counters[k]` as later events drain into R2. Corrected to
     the sound, intent-preserving **monotone containment**
     `invoice_line_items[k] \subseteq counters[k]` (every invoiced event is a
     real aggregated event — no phantom / over-billing) + an explicit
     `k \in DOMAIN counters` guard.
  3. **`INV_LAYER_1_RECONCILE` had the same flaw** at the Stripe layer: strict
     `Cardinality(stripe_invoiced[k]) = Cardinality(live R2 bucket)` breaks the
     instant an event drains to R2 after the (frozen, idempotent) Stripe charge.
     Corrected to the sound zero-**over**-drift bound `<=` (Stripe is never
     billed for more events than physically exist in R2 — the load-bearing
     financial tooth; transient under-count is the drift the production
     reconciler handles).
  No invariant was weakened to pass: the strict-equality forms were genuinely
  **unsound** for this async emit→aggregate→invoice→Stripe pipeline (a frozen
  snapshot can never equal a still-growing live set every instant); the
  containment/`<=` forms are the precise atomicity guarantees (no loss / no dup /
  no phantom-billing), with no-loss + no-dup still pinned by
  `INV_BILLING_NO_LOSS` + `INV_BILLING_NO_DUP`. Re-verified with the pinned TLC
  (`scripts/run_tlc_corelink.sh billing_atomicity`, SHA `237332bd…`): the
  State-4 crash is gone and the bounded model graph explores **past the
  previously-failing depth with zero invariant violations**. Spec-only change
  (`specs/tla/billing_atomicity.tla`).
- **Terraform CI cluster — un-broke the whole `terraform-lint` gate.** Three
  tangled fixes landed together: (1) native `tfsec` (the Docker action is
  Linux-only) with the one real finding (BYOK aws-kms `kms:ReEncrypt*` wildcard,
  scoped to customer ARNs) justified-ignored + flagged for review; (2) resolved
  the pre-existing `tflint` warnings (unused decls, missing provider/version
  constraints) across the terraform modules; (3) repinned the **non-existent**
  `hashicorp/setup-terraform@e9ce11f7` (the `# v3.0.0` SHA 404s — broke
  `terraform fmt`/`validate` at "Set up job") to the real `@b9cd54a3` (v3.1.2).
  Plus a `docker info` preflight guard on `smoke-install`. (Consolidates #78 + #83.)
- **Key Python CI validators → system `python3` (setup-python is unprovisionable
  on the mac fleet).** Five validation workflows — `spec_validation`,
  `openapi-validate`, `canonical-consistency`, `dashboard_validation`,
  `api-deprecation-check` — used `actions/setup-python`, which hard-failed
  fleet-wide with `mkdir: /Users/runner: Permission denied`: on the self-hosted
  macOS runners `RUNNER_TOOL_CACHE` is unset, so the action falls back to the
  GitHub-hosted default `/Users/runner` tool-cache path, which is unwritable and
  not overridable without sudo. Every one of these jobs died at the *Set up
  Python* step before reaching its `python3 scripts/*.py` payload. Dropped the
  `setup-python` step from each and run on the system `python3` (3.14.5, present
  on every runner) — the same fix proven green on `main` since #75 for
  `action-sha-audit` + `secrets-drift`. The three workflows that `pip install`
  deps (`spec_validation` → `requirements-ci.txt`; `openapi-validate` →
  `pyyaml`/`openapi-spec-validator`; `api-deprecation-check` → `pyyaml`) now
  isolate those deps in a repo-local venv (system Python is externally-managed)
  and prepend the venv `bin` to `$GITHUB_PATH` so subsequent steps resolve it;
  the two stdlib-only workflows (`canonical-consistency`, `dashboard_validation`)
  just drop the step. `python3 scripts/validate_specs.py` → 463/0 on the system
  interpreter; all five `actionlint` clean.
- **`audit-chain-daily-verify` 7-day window → portable date math
  (BSD/macOS `date`).** The `Compute 7-day window date set` step used
  GNU-only `date -u -d "$i days ago"`; BSD `date` on the all-macOS
  self-hosted fleet rejects `-d` (`date: illegal option -- d`), and under
  `set -euo pipefail` that hard-failed the `seven-day-verify` job before any
  R2 list/verify ran. Replaced the GNU-date loop with system `python3`
  (`datetime` + UTC), emitting the identical descending `YYYY-MM-DD` set
  (today … today-6) the downstream paginated CF-API-v4 R2 walk consumes
  unchanged. Date computation only — the audit-chain verification logic is
  untouched. (`.github/workflows/audit-chain-daily-verify.yml`.)
- **`ffi-matrix-ci` pyo3 build → system python3 (≥ 3.10).** The Rust
  unit-test job (`rust-unit`, plus `cross-language-verify`) in
  `.github/workflows/ffi-matrix-ci.yml` compiles `corelink-py`, which pulls
  `pyo3` with `abi3-py310` and therefore requires an interpreter ≥ 3.10. The
  job had no working Python pin (setup-python is broken on the self-hosted
  fleet — it provisions into an unwritable `/Users/runner`), so pyo3
  auto-detected a stale 3.8 on PATH and failed with `cannot set a minimum
  Python version 3.10 higher than the interpreter version 3.8 (abi3-py310)`.
  Set `PYO3_PYTHON: python3` at job level so pyo3 builds against the fleet's
  system `python3` (3.14), which satisfies the abi3-py310 floor. FFI matrix
  logic unchanged.
- **Bazel starter cold build → add missing `rules_cc` bzlmod dep** (WI-S15-002).
  `examples/bazel-starter` declared `rules_cc` only via `http_archive` in
  `WORKSPACE`, but modern Bazel (Bazelisk's default, no `.bazelversion`) runs in
  Bzlmod mode and does not load `WORKSPACE`, so the `cc_binary`/`cc_library` loads
  from `@rules_cc//cc:defs.bzl` failed with
  `@rules_cc could not be resolved: No repository visible as '@rules_cc'`, breaking
  the customer-facing `bazel-starter-ci` quickstart gate. Added a `MODULE.bazel`
  with `bazel_dep(name = "rules_cc", version = "0.0.17")` (the WORKSPACE
  `http_archive` is kept only as a legacy `--enable_workspace` fallback).
- **Phantom-Linux-runner nightlies → `workflow_dispatch`-only.**
  `endurance-2h-nightly` and `load-test-nightly` are pinned to
  `runs-on: [self-hosted, Linux, X64]`, but the self-hosted fleet is all-macOS
  (zero Linux runners), so their `schedule:` crons queued forever — perpetually
  pending / red, never executing. Dropped the `schedule` trigger from both
  (kept `workflow_dispatch:` so the k6 endurance/load suites can still be run on
  demand). The `runs-on` pin is intentionally unchanged — these k6 suites are
  too heavy for the macOS builder fleet (the Mac *is* the fleet). Re-add the
  nightly `schedule` once a Linux self-hosted runner is registered.
- **`corelink-reapi` mutation-coverage gap — `http_read` auth extractors.**
  `cargo mutants -p corelink-reapi` reported surviving mutants in
  `crates/corelink-reapi/src/http_read.rs`: the `extract_bearer_http` token-slice
  arithmetic and both `extract_request_id_http` return-value mutants
  (`String::new()` / `"xyzzy".into()`) were never asserted. Added four targeted
  unit tests that pin the EXACT extracted bearer token (incl. scheme/token
  boundary + interior-space tokens) and the EXACT echoed `x-request-id` (present
  case) plus the minted-UUID invariant (absent case), so each killable mutant now
  changes asserted output and fails. Tests only — no production-logic change.
- **`admin-ui lighthouse` gate — probed 404 routes (`/en`, `/onboarding/tenant`).**
  `apps/admin-ui/lighthouserc.cjs` collected `http://localhost:3000/en` and
  `/en/onboarding/tenant`, both of which 404 (`ERRORED_DOCUMENT_REQUEST`), failing
  the gate on every `apps/admin-ui/**` PR. The admin-ui serves its homepage
  un-prefixed at `/` (`src/app/page.tsx`; locale is resolved per-request in the
  root layout — the `[locale]` segment has **no** root `page.tsx`, so `/en` itself
  has never been a route), and the legacy `/onboarding/tenant` wizard was collapsed
  into a `/welcome` redirect by the Phase-0 PLG change. Re-pointed the URL list at
  four routes that actually return 200 — `/`, `/en/privacy`, `/en/consent/new`,
  `/en/admin/audit` — matching the S-16 §6 DoD canonical set. Config-only; no
  workflow or app change. (WI-S16-007 deliverable 2.)
- **`dtolnay/rust-toolchain` ↔ host rustup `bin/cargo` conflict on the
  self-hosted Macs.** The cargo-fuzz / cargo-mutants jobs (and the
  `region_pinning.yml` Rust jobs) used `dtolnay/rust-toolchain@…` to
  provision a toolchain, but the self-hosted Macs already ship
  rustup+cargo, so the action's component install collided with
  `error: failed to install component: 'cargo-x86_64-apple-darwin',
  detected conflict: 'bin/cargo'` (with a companion `cargo: command not
  found` downstream). Replaced those provisioning steps with a `run:` that
  puts the **pre-installed host toolchain** on `$GITHUB_PATH` (the CLAUDE.md
  "rustup proxy is broken" pattern) — `nightly-x86_64-apple-darwin` for the
  cargo-fuzz jobs (cargo-fuzz needs nightly) and `1.91.1-x86_64-apple-darwin`
  (the `rust-toolchain.toml`-pinned channel) for the cargo-mutants +
  region-pinning jobs. Touches the offending jobs only in
  `corelink-worker.yml`, `corelink-meta.yml`, `corelink-hash.yml`,
  `corelink-reapi.yml`, `tenant-path.yml`, and `region_pinning.yml`; the
  `pr-gate`/`wasm-build` steps (which request `components`/`targets`) are
  left untouched, and the heavy nightly fuzz/mutants jobs stay
  `if: schedule`-gated.
- **Welcome greeting → native `gh` (Docker-on-mac keystone).** The
  first-PR welcome workflow used `actions/first-interaction` — a Docker
  *container action* (Linux-only) that hard-failed `Container action is only
  supported on Linux` on the all-macOS fleet, on **every** PR. Because it runs
  via `pull_request_target` (base-branch workflow), that single red blocked the
  pre-merge gate-check on every open PR at once. Replaced with a native `gh`
  first-timer greeter (same idiom as `size-label.yml`), keeping
  `pull_request_target` for the fork-PR write token. (This is the keystone that
  un-jams the merge queue; the sibling tfsec/smoke Docker-on-mac conversions
  land in #78.)
- **SBOM workflow `cargo-cyclonedx` output flag** — the `Generate SBOM`
  step in `sbom.yml` (pinned `cargo-cyclonedx 0.5.4`) passed
  `--output-cdx sbom.cdx.json`, a flag that does not exist in the 0.5.x
  CLI (it was dropped with the `--output-prefix`/`--output-pattern`
  removal in 0.5.0), so the gate failed with
  `error: unexpected argument '--output-cdx' found`. Switched to the
  0.5.x-supported `--override-filename sbom.cdx`; with `--format json`
  the tool appends the format extension to the override, emitting the
  literal `sbom.cdx.json` that every downstream job (artifact upload,
  NTIA validate, TSA attest, DT ingest, release upload) already
  references. Matches the `--override-filename` convention used by
  `scripts/sbom-aggregate.sh` / `sbom-consolidated.yml`.
- **CodeQL gate — scope extended query suites per-language.**
  `.github/workflows/codeql.yml` applied `queries:
  security-extended,security-and-quality` to *every* matrix leg, but the Rust
  CodeQL pack (`codeql/rust-queries`) ships no `*-security-extended` /
  `*-security-and-quality` suites, so the rust leg failed `Initialize CodeQL`
  with `Query pack rust-security-extended cannot be found`. Moved `queries:`
  into a per-language matrix value — empty (default `codeql/rust-queries`) for
  rust, `security-extended,security-and-quality` for the mature js/ts + python
  packs — wired via `with: queries: ${{ matrix.queries }}`.
- **BYOK key-provider-isolation matrix gate had silently never run.**
  `.github/workflows/byok_matrix_weekly.yml` invoked
  `cargo test --package corelink-byok-matrix-test --test {byok_matrix_test,
  prop_byok_per_provider,adversarial_byok_per_provider}` across all three jobs,
  but that crate was physically absorbed into `crates/corelink-byok/tests/`
  (wave-33 stream-b.2c, commits `cfda9f26` / `ea7e7e12`). Every run died at the
  build-config step with `package ID specification 'corelink-byok-matrix-test'
  did not match any packages` — never reaching a single matrix cell, so the
  weekly 4-providers × 4-ops isolation gate (and its proptest + adversarial
  tiers) had **never actually executed**. Re-pointed all three jobs at the real
  targets in the `corelink-byok` umbrella: `--package corelink-byok --features
  _matrix-test --test {matrix,matrix_prop,matrix_adversarial}` (the
  `_matrix-test` feature is mandatory — it is the `required-features` gate that
  compiles all four internal provider modules into one binary). No test logic
  changed; only the build-config invocation. (The job's `SEV-2 matrix cell
  break` echo was a red herring — the failure was a build-config error, not a
  cell failure.)
- **Unresolvable action SHA pins in `terraform-drift.yml`** — the drift-detection
  workflow failed at "Set up job" with `Unable to resolve action … unable to find
  version` because `hashicorp/setup-terraform` and `slackapi/slack-github-action`
  were pinned to non-existent commit SHAs (a bad SHA-pinning pass; both returned
  HTTP 404 from the GitHub git-refs API). Repinned each `uses:` (and its trailing
  comment) to the real commit SHA for the version in the comment:
  `setup-terraform` → `b9cd54a3c349d3f38e8881555d616ced269862dd` (`v3.1.2`);
  `slack-github-action` → `485a9d42d3a73031f12ec201c457e2162c45d02d` (`v2.0.0`).
  Every `uses:` remains SHA-pinned (supply-chain constraint WI-S01-007 / WI-S13-004).
- **`mutation-nightly` gate reported false `0.0%` kill-rates on large/slow
  crates.** The run step invoked `cargo mutants --output ./mutants.out`, and
  cargo-mutants *unconditionally* creates a subdirectory literally named
  `mutants.out` **inside** the `--output` directory (`in_dir.join("mutants.out")`,
  doc: *"Create `mutants.out` within this directory"*) — so results actually
  landed at `./mutants.out/mutants.out/`. The gate's read path was internally
  consistent with that, so the doubled path was not itself the defect. The real
  defect: cargo-mutants writes `mutants.json` *before* the baseline build and
  *before* any scenario, but the `caught/missed/unviable/timeout.txt` lists only
  as scenarios complete; when a large crate's baseline build/test fails (or the
  run is killed before a single scenario finishes), `total>0` with **zero**
  recorded outcomes — which the harness silently reported as a fake `0.0%`
  *kill-rate regression* (e.g. `corelink-auth` 454-mutant / `corelink-pat`
  203-mutant lanes), while crates whose baseline completed read correctly
  (`corelink-billing` 96.89%). Fix: (1) pass `--output .` so results land at the
  plain `./mutants.out/` and both the gate and summary read that single path
  (removing the confusing nesting); (2) when `total>0` but no outcomes are
  recorded, fail with a **distinct "Incomplete mutation run — HARNESS failure"**
  error instead of a fake `0.0%` regression, and mark the per-crate summary
  `incomplete` (`kill_rate_pct: null`) so the aggregator never opens a false
  sub-floor regression issue; (3) count `timeout.txt` survivors and guard
  `mutants.json` with a clear diagnostic. The `≥80%`/`baseline − 5pp` gate
  threshold is unchanged — a genuine sub-floor kill-rate still fails. NOTE: the
  large crates additionally need a higher per-mutant `--timeout` (and may need a
  package-scoped baseline) to actually *complete* a sweep on the self-hosted Mac;
  that capacity work is tracked separately and is out of scope for this path fix.
- **Semgrep SAST gate produced ZERO signal — dead `returntocorp` placeholder +
  zero-runner label.** `.github/workflows/semgrep.yml` could never run, so the
  static-analysis security gate was silently blind. Two compounding faults:
  (1) it ran in a job `container:` pinned to
  `docker.io/returntocorp/semgrep@sha256:000…0` — a PLACEHOLDER zeroed digest
  (`failed to resolve reference … not found` → `Value cannot be null
  (ContainerId)`) — and invoked the deprecated `returntocorp/semgrep-action`
  (returntocorp rebranded to `semgrep/semgrep` long ago; the action repo is
  archived); (2) it was pinned to `runs-on: [self-hosted, Linux, X64]`, a label
  set matching ZERO runners after the 2026-05-31 macOS-only cutover — the same
  fault class as the `actionlint` / `action-sha-audit` regressions below — so it
  would have hung pending even with a valid image, and a job `container:` cannot
  run on the Docker-less mac fleet anyway. Repointed to a native Semgrep
  invocation: re-targeted to `[self-hosted, mac, corelink-builder]`, dropped the
  container, and run pinned `semgrep==1.164.0` via the host `python3`/`pip3` in a
  throwaway venv (no Docker; not `actions/setup-python`, whose
  `RUNNER_TOOL_CACHE` provisioning is unwritable on the fleet). The same bundled
  rulepacks + repo-local CoreLink custom pack (`p/security-audit`, `p/rust`,
  `p/typescript`, `p/python`, `p/owasp-top-ten`, `p/cwe-top-25`, `./semgrep.yml`)
  now drive `semgrep scan --error --sarif`; the existing
  `codeql-action/upload-sarif` upload + findings-summary steps are preserved
  (`outputs.sarif` is now set by the new step). Expect first-run Security-tab
  alerts: this gate has been emitting no findings, so its first real execution
  may surface a backlog. (2026-06-02)
- **`license-policy` gate false positive — first-party UNLICENSED crates.**
  `scripts/license-audit.sh` ran `cargo license --json` over the whole
  workspace and flagged every first-party crate as a license violation
  (e.g. `corelink-failover-router`, `e2e-failover-router`,
  `e2e-replication-failover` — ≈87 reported), because each workspace member
  inherits the intentional `license = "UNLICENSED"` default from
  `[workspace.package]` (correct for the ~85 proprietary server crates) and
  `cargo-license` has no concept of private workspace members. The audit now
  enumerates workspace members via `cargo metadata --no-deps` and excludes
  them before the allow-list check, so only THIRD-PARTY dependencies are
  license-checked — mirroring `cargo deny check licenses`, which already skips
  them via `[licenses.private] ignore = true` in `deny.toml`. No crate was
  relicensed; the allow-list semantics for third-party crates are unchanged,
  so a genuinely-forbidden copyleft dep (GPL/AGPL/SSPL) is still caught. OUR
  OWN OSS-tagged crates' literal `MIT OR Apache-2.0` tags remain asserted by
  the companion `scripts/check-oss-license-tags.sh`.
- **macOS self-hosted CI-fleet hardening — migrate-to-self-hosted
  regressions.** The 2026-05-31 cutover to the macOS self-hosted runner fleet
  (5× `corelink-builder`, all macOS, zero Linux) left several gates silently
  broken; the new pre-merge gate-check surfaced them. Each had been failing at
  an *infra* step *before* its real check could run — masking both the
  breakage and, in one case, a real finding underneath:
  - **`actionlint` hung pending forever** — pinned to `runs-on:
    [self-hosted, Linux, X64]`, a label set matching ZERO runners. Re-targeted
    to the mac fleet. Its `docker://rhysd/actionlint` action is Linux-only and
    `taiki-e/install-action` does not package actionlint (it is not a cargo
    crate), so it now runs the fleet's host `actionlint` binary (1.7.12),
    falling back to the official pinned-release installer if a runner lacks it.
  - **`action-sha-audit` + `secrets-drift` failed at `actions/setup-python`**
    with `mkdir: /Users/runner: Permission denied` — the self-hosted runners
    have no working `RUNNER_TOOL_CACHE`, so `setup-python` cannot provision an
    interpreter and falls back to the unwritable hosted-runner path (it is the
    interpreter *provisioning*, not the Python version, that fails). Dropped
    `setup-python` from both — they only need `python3`, which ships on every
    runner. With `action-sha-audit` finally able to run, it caught a genuine
    masked violation: `smoke-install.yml` referenced `actions/checkout@v4`
    (a tag, not a 40-char SHA) — now pinned.
  - Cleared the one `SC2129` shellcheck finding `actionlint` flagged in
    `release-notes.yml` once it could finally run (grouped the
    `$GITHUB_OUTPUT` redirects).
  Same fallout class as the earlier `size-label` container-action fix. The
  remaining broken gates — the repo-wide `setup-python` → system-`python3`
  migration, the Docker-on-mac `cargo-deny` / tfsec / welcome conversions, and
  the supply-chain findings the broken `cargo-deny` was masking — land in a
  dedicated follow-up rather than a per-PR avalanche on the 5-runner Mac.
- **`cargo-audit` supply-chain gate was un-auditable — pinned tool too old for
  CVSS-4.0 advisories.** The daily cron (and PR gate) hard-failed at advisory-DB
  load with `error loading advisory database: … RUSTSEC-2026-0073.md: TOML parse
  error at line 5, column 8 — unsupported CVSS version: 4.0`. Root cause: the
  pinned `cargo-audit` `0.21.2` bundles a pre-4.0 `cvss` crate (CVSS-4.0 support
  landed in `cvss` 2.1.0, 2025-06-06), so it rejected the `cvss = "CVSS:4.0/…"`
  field on the first CVSS-4.0 advisory now present upstream. `rustsec`'s DB
  loader propagates that first per-advisory parse error and aborts the *entire*
  load (`Entries::load_file(path)?` in `database.rs`), so one modern advisory
  silently left the whole repo **un-audited** — NOT a real CVE in our dependency
  tree, and NOT fixable by pinning advisory-db (no DB commit is loadable by an
  older-than-CVSS-4.0 tool). Fix: bump `cargo-audit` `0.21.2 → 0.22.1` (rustsec
  lib 0.32 / `cvss` 2.2.0, which parses CVSS 4.0; MSRV 1.85 ≤ repo 1.91.1) —
  governed by ADR-S12-045 §6 / §14.s12.004.1 (tooling-pin bump → ADR + Security
  review; this PR's `/techlead` + Owner sign-off is that review). Additionally
  SHA-pinned the advisory DB for deterministic, reproducible audits (both jobs
  `git clone` + `git checkout` a vetted-good `rustsec/advisory-db` commit,
  `501c03f38eadd16a79d6712df424fd7d38369088`, 2026-06-02 — verified loadable by
  the bumped tool across all 1082 advisories incl. CVSS-4.0) and run
  `cargo audit --db <pinned> --no-fetch --stale`. The PR gate stays fail-closed
  (`--deny warnings`); the cron tries the live upstream DB first (fresh detection
  within SLA) and falls back to the pinned DB only if the live load fails, so it
  is never dark again. The CRITICAL→exit-1 / HIGH→warn classifier is unchanged —
  real RUSTSEC advisories in our deps still fail/alert exactly as before. Bump
  the pinned commit when refreshing the advisory floor.
  Un-blinding the gate surfaced 3 advisories that are ALREADY waived in
  `deny.toml` `[advisories].ignore` (so cargo-deny and cargo-audit must agree):
  `RUSTSEC-2023-0071` (rsa Marvin sidechannel — signing-only on operator inputs,
  mitigated, ADR-S20-RSA-MARVIN-MITIGATION), `RUSTSEC-2025-0119` (number_prefix
  unmaintained, via `indicatif → corelink-cli`, operator tool only; also added
  to `deny.toml` by #77), and `RUSTSEC-2025-0134` (rustls-pemfile unmaintained,
  dev/test-only via `bollard → testcontainers`). Mirrored those exact 3 ids
  (with the same justifications) into a new committed **`.cargo/audit.toml`**
  `[advisories].ignore` — auto-loaded by `cargo audit` from the repo root — so
  cargo-audit waives EXACTLY what `deny.toml` already waives (one logical source
  of truth, two tools), with a header cross-reference keeping the two in sync.
  Nothing not already in `deny.toml` is ignored; any NEW/unwaived advisory still
  exits 1 (verified: `.cargo/audit.toml` ignoring an unrelated id still fails on
  the live finding).
  Ratification: the `cargo-audit` `0.21.2 → 0.22.1` bump (ADR-S12-045 §6 /
  §14.s12.004.1 tooling-pin review) was **ratified by Owner + techlead on
  2026-06-02** — required for CVSS-4.0 parsing, MSRV 1.85 ≤ repo 1.91.1; this
  satisfies the §14.s12.004.1 ADR + Security review for the bump.
- **`cargo-deny` CI gate native-ized on the macOS fleet + masked supply-chain
  findings closed** (the promised `cargo-deny` follow-up above). The
  `EmbarkStudios/cargo-deny-action` is a Docker container action (Linux-only)
  and hard-failed on every macOS runner with "Container action is only
  supported on Linux" — so the gate had been RED at an *infra* step before its
  real policy check ever ran. Replaced that step in both `cargo-deny.yml` and
  `cas_foundation.yml::cargo-deny` with the same native path
  `dependabot-policy.yml` already uses (`dtolnay/rust-toolchain` +
  `taiki-e/install-action` pinning `cargo-deny@0.16.4`, all SHA-pinned). With
  the gate finally able to run, it surfaced TWO genuine findings it had been
  masking, both now closed: (1) `RUSTSEC-2025-0119` — `number_prefix` 0.4.0
  unmaintained-ONLY (no vuln), reached solely via `indicatif 0.17.11 →
  corelink-cli` with no safe upgrade available; added to `deny.toml`
  `[advisories].ignore` with a re-evaluate-on-indicatif-bump justification.
  (2) a `bans.wildcards` violation — the publishable `corelink-client-verify`
  (`publish = true`, OSS SDK) depended on `corelink-hash` via a path-only
  workspace dep, which crates.io disallows for public crates; added `version =
  "0.1.0"` to the `corelink-hash` workspace dep (in lockstep with the
  workspace `[package].version`). No `publish = false` anywhere. cargo-deny now
  exits 0 (`advisories ok, bans ok, licenses ok, sources ok`).
- **Secrets-matrix verify-gate scanned build output** — both validators
  (`scripts/secrets-checklist-verify.sh`, `scripts/validate_secrets_matrix.py`)
  walked gitignored `.open-next`/`.wrangler` bundles, whose embedded
  Sentry/OpenNext SDK references ~130–148 vendor CI-detection env vars
  (`CIRCLE_SHA1`, `VERCEL_*`, `ZEIT_*`, …) — masking the real matrix↔code drift
  (the gate was red only on a machine where admin-ui had been built). Excluded
  build output from the scan; added 5 previously-undocumented secret rows
  (#141–#145: `PAT_SIGNING_KEY`, `CORELINK_INTERNAL_AUTH_KEY`, `R2_TDK_HEX`,
  `CORELINK_CLI_RELEASE_TOKEN`, `SENTRY_AUTH_TOKEN`); reconciled the
  `CF_API_TOKEN`/`CLOUDFLARE_API_TOKEN` dual-name drift; classified ~23
  non-secret vars into the allowlists; removed 2 deleted orphans
  (`HUGR_AUDIT_CHAIN_HMAC_KEY`, `HUGR_SESSION_HMAC_KEY`, confirmed absent on all
  5 prod workers). Both gates green; `validate_specs.py` 463/0. See
  `specs/_audits/2026-06-02-secrets-naming-reconciliation.md`.
- **OSS license-tag regression (DEBT-002 reopened)** — the Wave 33-36 reorg
  silently re-inherited every crate to the `UNLICENSED` workspace default,
  wiping the `MIT OR Apache-2.0` tags on the OSS crates (0 of 13 actually
  tagged). Re-tagged the 4 pre-launch crates (`corelink-hash`,
  `corelink-client-verify`, `corelink-tenant-path`, `corelink-rate-headers`)
  with literal license + `publish = true` + `repository`; removed the dead
  `corelink-ratelimit` dep from `corelink-rate-headers` (publish blocker since
  ratelimit is closed); reclassified `corelink-audit` closed (re-export
  facade); added `scripts/check-oss-license-tags.sh` + a `license-policy.yml`
  guard step. See `specs/_audits/2026-05-31-oss-split-prep.md`.
- **Wave-36 Trigger A** — resolved `corelink-stripe-real ↔
  corelink-billing-materializer` dep-graph cycle by inverting the
  dependency direction onto the new leaf `corelink-billing-stripe-traits`
  crate (see Added). Unblocks Wave-33 stage-2.C closure path (c).
- **Wave-36 Trigger B** — `corelink-ops` platform-gated under
  `cfg(not(target_arch = "wasm32"))` inside the
  `dsr-statuspage-scheduler` consumption site; restores green wasm32
  build for `corelink-wasm` worker target. SEAL:
  `specs/_audits/sealed/2026-05-26-w36-trigger-b-seal.md`.
- **Self-hosted parse-smoke jobs + CodeQL — two infra gates fixed
  (system python3 + GHAS preflight).** Two unrelated red nightly gates
  on the macOS self-hosted fleet:
  (1) **`workflow-yaml-smoke`** in `s10-ship-gate.yml` (and the identical
  `dashboard-smoke` job in `s07`, `s08`, `s09`, `gc-ship-gate`) used
  `actions/setup-python` to provision Python 3.12 before calling
  `python3 -c "import yaml; yaml.safe_load(...)"`. On the self-hosted mac
  fleet `RUNNER_TOOL_CACHE` is unset so the action fell back to the
  unwritable GitHub-hosted default `/Users/runner` path, dying with
  `mkdir: /Users/runner: Permission denied` at "Set up Python" — before
  reaching any parse logic. Dropped `actions/setup-python` and replaced
  the dep-install step with a lightweight inline check: if `import yaml`
  succeeds on the system `python3` (it does on every runner), the step
  exits immediately; otherwise it creates a minimal repo-local venv and
  prepends it to `$GITHUB_PATH`. Mirrors the established
  `spec_validation.yml` pattern (2026-06-03 comment). pyyaml/json are
  stdlib or already present; no version-specific Python requirement.
  (2) **CodeQL** (`codeql.yml`) hard-failed at `database init` (exit
  code 2: `There's no CodeQL extractor named 'rust' installed` + `Code
  scanning is not enabled for this repository`). PR #113 (a72b4f6c)
  already split the SARIF upload into a `continue-on-error` step, but
  the `init` step itself was unguarded and still died first. Added a
  `GHAS code-scanning preflight` step (id: `preflight`) that queries
  `gh api repos/{repo}/code-scanning/alerts`; if the call fails (HTTP
  403/404 = GHAS off) it sets `enabled=false` and emits a
  `::notice::` log line. Every downstream heavy step (`Install Rust
  toolchain`, `Cargo cache`, `Initialize CodeQL`, `Build Rust
  workspace`, `Perform CodeQL Analysis`, `Upload SARIF`, `Severity
  gate`) is now gated on `steps.preflight.outputs.enabled == 'true'`.
  The job exits SUCCESS (all steps skipped) when GHAS is disabled, and
  runs fully once the owner enables Advanced Security — no workflow
  change needed at that point. No existing action SHA was altered; no
  new third-party action introduced.

### Security

- **TLC `tla2tools.jar` v1.8.0 SHA-256 re-pin — RATIFIED + COMPLETE.**
  The TLA+ project re-published a non-reproducible (timestamped) `tla2tools.jar`
  to the **same `v1.8.0` tag**, so the pinned SHA-256 drifted and all TLA+
  model-check gates + `nightly` failed fail-closed at the pin check.
  Independently verified the new official asset (3× download byte-identical, two
  hash tools agree, confirmed via `gh api` it is `lemmy`'s re-cut release asset,
  re-cut 2026-05-26): OLD `d5d07d5d…dbc8ef7f` → NEW `237332bd…ff2a4fb`
  (size 4356704 → 4357560 B). **Re-pinned `TLC_SHA256_PINNED` across EVERY active
  reference:** `tla_check.yml`, `tla_region_residency_check.yml`,
  `tla_runbooks_check.yml`, `tla_dsr_erasure_check.yml`, `tla_billing_check.yml`,
  `nightly.yml`, **`cas_foundation.yml` (`tlc-canonical` job only — its
  `cargo-deny` step is owned by PR #77 and left untouched)**, and
  **`scripts/run_tlc_corelink.sh` (comment + shell var)**. A whole-repo grep
  confirms zero active references to the old hash remain (sealed/audit/spec
  records keep it as historical record, intentionally not rewritten). **Ratified
  by the tech-lead under owner delegation (2026-06-02);** §A1 sign-off marked
  SATISFIED. Ceremony + evidence in `ADR-0042-gc-worker-scheduler.md` §A1 and
  `specs/_audits/2026-06-02-tlc-v1.8.0-repin-upstream-republish.md`.
  **FOLLOW-UP (next hardening PR, not done here):** the jar is non-reproducible so
  this WILL recur on any upstream re-cut — **vendor the verified jar to immutable
  storage we control (R2 or a repo-owned release asset)** and fetch under the
  pinned SHA, removing the mutable-tag dependency.
- **Wave-36 Stage 3 — cargo-deny lockdown.** Added `[bans] deny` rules
  for the 4 absorbed-but-canonical adapter HTTPS crates
  (`corelink-stripe-real`, `corelink-statuspage-real`,
  `corelink-slack-real`, `corelink-clerk-cf`) with surgical
  `wrappers` allowlists permitting only the Wave-33 umbrella
  re-export shims to import them directly. New workspace consumers
  must route through the canonical façades
  (`corelink-billing::stripe`, `corelink-ops::statuspage` /
  `corelink-ops::slack`, `corelink-auth::clerk_cf`,
  `corelink-adapters-cloud::{stripe,statuspage,slack,clerk}`). Closes
  Wave-33 → Wave-34 follow-up #1 closure path (c). SEAL:
  `specs/_audits/sealed/2026-05-27-w36-stage-3-seal.md`. Tag
  `wave-36-final-sealed`.
- **Wave-33 → Wave-34 closure-followups #1–#5** flipped
  `audit_status: ACTIVE → CLOSED` (5/5 deferrals delivered).

---

## [1.0.0] - DRAFT — pending `framework-v1-0-0-ga` tag + Owner approval

> **DRAFT.** This section is the technical changelog companion to
> `RELEASE-NOTES-v1.0.0-GA.md`. Publication is gated on the
> `framework-v1-0-0-ga` tag and the 2-key Owner + on-call SRE approval
> recorded in `specs/_audits/sealed/2026-05-16-ga-readiness-final.md` §13.
> Wave references below trace to `specs/_audits/2026-05-16-wave{N}-closure.md`.

### Wave summary — production wiring + adversarial review + DEBT closure

The 21-sprint spec-corpus phase (S-00 → S-20) is captured in the `[0.x]`
and per-sprint sections below. The post-S-20 production wiring + GA
readiness phase ran across **29 waves** dispatched on `main` between
the `ga-engineering-gate-complete` tag (2026-05-14) and the GA
cutover window (2026-05-16+). Each wave layered adversarial review +
debt closure + production wiring + chaos / endurance evidence on top
of the sealed sprint contracts.

| Wave | Focus | SEAL evidence |
|---|---|---|
| **R-prep + wave-1..17** | Per-sprint SEAL cadence; spec-corpus build-out; production wiring layer additions (real Stripe wasm32, real BYOK providers, real CF bindings, real Neon driver, replica coordinator, DSR worker production, customer dashboard, statuspage init, breach notification templates, etc.) | Per-sprint `_audits/sprint-close-round-*.md` |
| **Wave-18** | Audit-export production wiring (Stream A); Neon shadow analytics plane (Stream B); first cold-tool adversarial review pass | `specs/_audits/sealed/2026-05-16-wave18-aggregate-closure.md` (9.5 / 10) |
| **Wave-19** | S-18 pentest scope freeze; CLI `verify-ndjson` HTTP wiring; SDK example expansion | `specs/_audits/sealed/2026-05-16-wave19-adversarial-review.md` (8.86 / 10) |
| **Wave-20** | Audit-export streaming + payload column; 10-stream adversarial review | `specs/_audits/sealed/2026-05-16-wave20-closure.md` (9.40 / 10) |
| **Wave-21** | WallClock cross-route closure; DEBT-008 mutation sweep (hash 77.78 → 97.22 %); tenant-config region resolver (9.85 / 10) | `specs/_audits/sealed/2026-05-16-wave21-closure.md` (9.55 / 10) |
| **Wave-22** | Tenant-path UUID fix (9.9 / 10); Stripe MatClock wasm32 (9.7 / 10); chaos campaign harness (8 isolated fail-CLOSED scenarios); 24h endurance harness | `specs/_audits/sealed/2026-05-16-wave22-closure.md` (9.45 / 10) |
| **Wave-23** | Chaos combined-failures matrix (executor-loss × replication-lag × tenant-isolation); pilot onboarding E2E rig; CS playbook; beta-feedback triage; LFPDPPP MX attorney-package; INV-PAT-REVOKE-PROPAGATION promotion | `specs/_audits/sealed/2026-05-16-wave23-closure.md` (9.20 / 10) |
| **Wave-24** | GA cutover dry-run (RB-GA-CUTOVER §3, G1..G6 GREEN); GA readiness final audit (CONDITIONAL GO); DEBT-008 wave-24 closure batch; PAT-revoke TLA-exempt registration; ADR-0034b dual-hat path | `specs/_audits/sealed/2026-05-16-wave24-closure.md` (codex-Opus pass in flight) |
| **Wave-25** | External pentest engagement scope freeze (RFP + shortlist + SOW); DEBT-015-BUILD path-(3) ssgRequire; endurance 10-min dress-rehearsal (0 SLO / 0 INV violations); statuspage init dress-run; tenant-config CF prod-wire; pre-GA security attestation; GA-readiness DEFER drift detector (8 → 7 scrub); wave-24 adversarial-review pass (recovery cherry-picks `d172a4a` + `8fa1c22`) | `specs/_audits/sealed/2026-05-16-wave25-closure.md` |
| **Wave-26** | **GA-1 feature freeze** (`74b8faa` — engineering corpus feature-complete from here forward); INV-CRITICAL TLA final audit (61 / 61 CRITICAL TLA+-proved, **Z = 0 milestone established**); Lote 6 v1.0.0 GA RC2 absorption; wasm32 baseline lock + getrandom fix; CF Worker prefetch wire; release notes v1.0.0 GA DRAFT (`6ce134b`); **production-tier dress-run scoring 9.36 / 10 PROCEED** + v1.0.0-GA tag draft; wave-25 adversarial review (9.00 / 10 PASS); DEBT-026 RFP tracker | `specs/_audits/sealed/2026-05-16-wave26-closure.md` |
| **Wave-27** | **GA cutover wave** (anchor) — final cutover-readiness verdict CONDITIONAL GO; 7-day endurance soak streak harness dispatched; Statuspage T-7d provisioning rehearsal; ShadowSinkFactory partial consumer adoption; wave-26 adversarial review prep; post-GA continuity runbook; pilot admin shell scripts + dashboard SSOT | `specs/_audits/sealed/2026-05-16-wave27-closure.md` |
| **Wave-28** | Cutover-prep automation wave — AWS Artifact fetch automation (DEBT-003 engineering-CLOSED); pilot-announcement comms package; Statuspage provisioning automation (DEBT-016 engineering-CLOSED); pentest finding absorption framework (7-state machine, 48 test cases); LFPDPPP MX engagement final (DEBT-025 engineering-CLOSED); pentest RFP send ceremony (DEBT-026 engineering-CLOSED); pre-cutover weekly verification cron; wave-28 adversarial review **8.96 / 10 PASS** | `specs/_audits/sealed/2026-05-16-wave28-adversarial-review.md` |
| **Wave-29** | **Cutover-wait-state + customer-acquisition wave** — engineering corpus feature-complete since wave-26 GA-1 freeze. signup.corelink.humangr.com backend + landing page + pilot admin web UI (DEBT-027 engineering-CLOSED); ShadowSinkFactory full adoption (wave-21 → -27 → -29 follow-on chain); customer-facing artefacts (audit-chain viz UI + pricing page calculator + trust center publish); perf-baseline GA freeze snapshot; DEBT register: **8 nominally OPEN → 5 engineering-CLOSED operator-bound + 3 engineering-side P1 partial** | `specs/_audits/sealed/2026-05-16-wave29-closure.md` |

### Added — production wiring + customer-facing surfaces

- **Audit export** — NDJSON streaming via signed URL + offline verifier
  (`corelink audit verify-ndjson`) + payload column + Merkle proof
  embed (waves 18 + 20).
- **Neon shadow analytics plane** — wired with RLS WITH CHECK at the
  SQL layer; real driver; replication SLO §4.27 – §4.29 observation
  streak active (waves 18 + 22).
- **BYOK 4-provider matrix** — AWS KMS, GCP KMS, Azure Key Vault,
  HashiCorp Vault — real provider pattern documented and exercised
  (R-prep + wave-25 attestation rollup).
- **Customer-facing audit export** + **customer dashboard** + **Stripe
  customer portal** (R-prep + waves 18 – 25).
- **Customer breach notification templates** (R-prep).
- **`RB-GA-CUTOVER.md`** + `RB-GA-LAUNCH-ROLLBACK.md` +
  `RB-LAUNCH-WAR-ROOM-COORDINATION.md` + 13 additional SEV-class
  runbooks (waves 19 – 24).
- **Chaos campaign** — 8 isolated fail-CLOSED scenarios + 3
  combined-failure scenarios under `cargo test --features chaos`
  (waves 22 – 23).
- **24-hour endurance harness** — built wave-22; 10-minute dress-run
  wave-25; soak scheduled in the pre-cutover T-24h window.
- **Statuspage** at `status.corelink.humangr.com` — URL-substitution mechanism
  (wave-24), dress-rehearsed wave-25.
- **External pentest engagement** — scope frozen wave-25
  (`specs/_audits/sealed/2026-05-16-pre-ga-pentest-scope.md` +
  `specs/_audits/sealed/pentest/SOW-S20-EXTERNAL-PENTEST.md`); vendor engagement
  scheduled 2026-Q3.
- **Pre-GA security attestation package** — wave-25 rollup
  (`specs/_audits/sealed/2026-05-16-pre-ga-security-attestation.md`).
- **Pilot onboarding E2E rig** + **CS playbook** + **beta-feedback
  triage pipeline** (wave-23).
- **GA gate** — `specs/_compliance/GA-GATE-CRITERIA.md` (59 criteria
  across 6 tracks) + `GA-GATE-GO-NOGO-TEMPLATE.md` (2-key signature
  template) + ADR-0034 / ADR-0034b PRR staffing + dual-hat waiver
  paths.
- **GA-readiness DEFER drift detector** — CI gate that prevents
  silent regression of the DEFER population between wave-25 and
  cutover (wave-25 stream #4).
- **GA-1 feature freeze** (wave-26 `74b8faa`) — engineering corpus
  feature-complete from this commit forward; post-freeze admits only
  `specs/` / `docs/` / `.github/` / `scripts/` / `apps/` changes per
  ADR-0034b §3 scope-fence.
- **Production-tier dress-run** (wave-26) — 13 / 13 steps PASS,
  6 / 6 greenlights GREEN, 0 / 6 rollback triggers fired,
  prep-ring isolation guard verified, **GA-readiness 9.36 / 10
  PROCEED** (`specs/_audits/sealed/2026-05-16-prod-deploy-dressrun.md`);
  pre-authored v1.0.0-GA tag draft at `docs/release/v1.0.0-GA-tag-draft.txt`.
- **7-day endurance soak streak** harness (wave-27 stream #4) —
  wall-clock 168 h SLO observation streak feeding NO-GO trigger #5.
- **Post-GA continuity runbook** (`RB-POST-GA-CONTINUITY.md`) —
  wave-27 anchor.
- **Pilot admin path** — shell scripts (`grant-pilot-tier.sh`,
  `list-pilot-tenants.sh`, `pilot-24h-checkin.sh`) wave-27;
  Grafana dashboard SSOT `dashboards/grafana/dash-pilot-tenants.yml`;
  **Owner-facing pilot admin web UI** wave-29 stream #3.
- **AWS Artifact PDF recorder + fetch automation** (wave-28) —
  DEBT-003 engineering-CLOSED.
- **Statuspage provisioning automation** (wave-28) — DEBT-016
  engineering-CLOSED; operator DNS + ORG-ID swap is the only
  remaining T-7d step.
- **Pentest finding absorption framework** (wave-28) — 7-state
  machine (`RECEIVED → TRIAGED → IN_FIX → FIXED → RETEST_SUBMITTED →
  RETEST_PASSED → ABSORBED`); CVSS / P-tier coherence enforced; 48
  test cases passing.
- **Pre-cutover weekly verification cron** (wave-28) — automated
  weekly green-light digest against the cutover commit base.
- **Pilot announcement comms package** (wave-28).
- **signup.corelink.humangr.com backend + landing page** (wave-29 streams #1, #2) —
  token-based pilot-slot reservation; idempotent token issuance + 24h
  replay-safe consumption; 4-section public landing (hero / value-prop
  / 3-tier pricing-summary / signup form). DEBT-027 engineering-CLOSED.
- **Customer-facing audit-chain visualisation UI** (wave-29 stream #6) —
  tenant-scoped Merkle-path inspector + tamper-evidence proof viewer.
- **Public pricing page calculator + 4-tier comparison + internal
  cost-worksheet** (wave-29 stream #7).
- **Trust center publish pipeline** (wave-29 stream #8) — consolidates
  SOC 2 / ISO 27001 / PCI DSS / LGPD / FedRAMP-informational status +
  DEBT-003 AWS Artifact PDF link (gated by DEBT-003 closure).
- **Perf-baseline GA freeze snapshot** (wave-29 stream #9) — 5 SLO
  families pinned at the cutover commit base; consumed by NO-GO
  trigger #5 (endurance 7-day soak streak).
- **ShadowSinkFactory full adoption** (wave-21 → wave-27 → wave-29
  follow-on chain) — zero direct `ShadowSink::new(...)` call sites in
  consumer crates post-SEAL; all sinks issued via
  `ShadowSinkFactory::for_tenant(tenant_id)` with region-aware
  resolution.
- **Release-notes editorial-polish audit**
  (`specs/_audits/sealed/2026-05-16-release-notes-editorial-polish.md`) and
  **customer-facing FAQ** (`RELEASE-NOTES-v1.0.0-GA-FAQ.md`) — wave-30
  stream #10.

### Changed

- **Invariant registry** — 197 declared (61 CRITICAL, 132 HIGH,
  4 MEDIUM); **82 TLA+ verified**; **all 61 CRITICAL TLA+-proved**
  (Z = 0 milestone established wave-26 stream #9, preserved across
  waves 26 → 29); 0 orphan refs; 143 / 143 WI-coverage; 15 legacy →
  canonical aliases documented; 0 UNKNOWN severity classifications.
- **DEFER counter** scrubbed wave-25 (`specs/_audits/sealed/2026-05-16-ga-readiness-defer-scrub.md`):
  stale "Docs CI billing reinstatement" row removed (CI runs locally
  per `feedback_ci_local`); current counter is **7 external items**
  (5 user-bound + 1 vendor-bound + 1 mixed); drift detector exit 0
  held stable across waves 25 → 29.
- **`canary` → `staging-only`** chaos discipline at GA per S-17
  cross-functional decision (production chaos not authorised on
  the GA cutover day).

### Fixed

- **DEBT-001** — secrets matrix tighten (closed wave-15);
  `validate_secrets_matrix.py` code-only false-positive resolved
  wave-22.
- **DEBT-002, DEBT-004, DEBT-005, DEBT-006, DEBT-007, DEBT-009,
  DEBT-011, DEBT-012, DEBT-014, DEBT-017 – DEBT-020, DEBT-022,
  DEBT-024** — closed (no waiver) across waves 15 – 24.
- **DEBT-008** — mutation kill-rate baseline empirically CLOSED for 8
  of 15 crates (≥ 75 % floor on the remaining 5 via the
  `mutation-nightly.yml` CI-nightly matrix).
- **WallClock cross-route** — wave-21 closure (`f3462c6`);
  9.55 / 10 adversarial score.
- **`INV-PAT-REVOKE-PROPAGATION`** — promoted to CRITICAL wave-23;
  TLA+ exempt under §4.3 (wall-clock obligation, not consensus
  property); empirical sub-second propagation verified via mutation
  sweep + runbook drill.

### Security

- **External pentest** — scope frozen (wave-25); **RFP send ceremony
  executed wave-28** (DEBT-026 engineering-CLOSED); vendor 30-day
  selection clock running. Earliest retest letter target 2026-07-29.
  Pentest finding absorption framework SEAL'd wave-28 (7-state
  machine; 48 test cases). HIGH / CRITICAL findings gate any future
  `GA-Full` / `v1.1.0` promotion. **No external pentest report is
  yet published**; customer-facing security claims do not depend on
  a completed external pentest at v1.0.0 GA.
- **Adversarial review** — 10 consecutive waves; last-5 PASS-trend
  rolling mean **9.32 / 10** (raw chronological 8.83; charter
  rolling-window framing 9.41); wave-28 review re-anchored at
  **8.96 / 10 PASS**; 0 P0 / 0 outstanding P1 at any wave boundary
  since wave-19 SEAL. Wave-24 6.95 / 10 CONDITIONAL recovered via
  wave-25 cherry-picks `d172a4a` + `8fa1c22` to a 9.40 projection.
- **GA-readiness** — wave-26 production-tier dress-run **9.36 / 10
  PROCEED** (13 / 13 steps PASS; 6 / 6 greenlights; 0 / 6 rollback
  triggers); wave-27 final cutover-readiness verdict CONDITIONAL GO.
- **Compliance** — SOC 2 Type I ready, ISO 27001 Stage-1 eligible,
  GDPR / LGPD / PCI-SAQ-A / CCPA ready; **LFPDPPP MX engineering-CLOSED
  wave-28** (DEBT-025; attorney sign-off operator-paced); FedRAMP
  Moderate documented as not in scope for GA.
- **BYOK** — 4-provider FIPS attestation matrix; **AWS Artifact
  recorder + fetch automation SEAL'd wave-28** (DEBT-003
  engineering-CLOSED; operator T-7d download remains).

---

## [1.0.0-rc.1] - 2026-05-14 — GA Engineering Gate complete

Tag: `ga-engineering-gate-complete` (HEAD `f09d640`, alias of `s20-impl-sealed`).

The **GA Engineering Gate** is the binary technical readiness boundary —
distinct from `Launch Orchestration` (CAP-LAUNCH-001, marketing/PR/Product
Hunt). This tag asserts that 21 sprint specs are SEALED, ~70 Rust crates
compile and test, 8 TLA+ specs check, 62 runbooks exist, and 14 canonical
sources are green. Production wiring (Wave R-2..R-4) and external evidence
(Wave R-5..R-7) follow under `ROADMAP-TO-GA.md`.

### Added — GA Engineering Gate

- **CAP-GA-001** — Engineering gate `CONDITIONALLY_APPROVED` per PRR-S20-GA
  pending 8 D+60 evidence items (`ROADMAP-TO-GA.md` §5 Wave R-5/R-7).
- **CAP-GA-002** — External pentest engagement contract template + report
  intake workflow (Schellman / A-LIGN); EVT-040 evidence event registered.
- **CAP-GA-003** — SOC 2 gap analysis preparation (Drata / Vanta) with
  concrete GAP-XX items and fix timeline for Type I engagement at D+180.
- **CAP-GA-004** — Lighthouse customer migration framework (2 team-tier,
  1 enterprise-BYOK); SLA-claim-met-in-30d criterion encoded.
- **CAP-GA-005** — SLA contractual terms + DPA v1 template published in
  `legal/`; ready for 3-customer signature flow.
- **CAP-GA-006** — Incident response 24/7 PagerDuty schedule covering
  3 regions; response-time < 5 min tested.

### Changed

- "10 canonical sources" → **14 canonical sources** (codex finding;
  alignment with §3 of `_spec_contract.md`).
- "Full SBOM v1.0" → **CycloneDX 1.5+** (alignment with S-12 R-S12-3).
- Roadmap-to-GA marketing/launch concerns separated from engineering
  gate per codex feedback; `CAP-LAUNCH-001` carved out of `CAP-GA-*`.

### Security

- 14 canonical sources verified green at gate (SECURITY-MODEL,
  PRIVACY-MODEL, AUTH-MODEL, KEY-MANAGEMENT, COMPLIANCE-MATRIX,
  STORAGE-SEMANTICS-MATRIX, RESILIENCE-PATTERNS, OBSERVABILITY-MODEL,
  SLO-CATALOG, FAILURE-MODES, INVARIANT-REGISTRY, DATA-MODEL,
  REMOTE-CACHE-PRODUCT-PROFILE, FRAMEWORK-00).

---

## [0.20.0] - 2026-05-14 — S-20: GA Readiness

Tag: `s20-impl-sealed` (HEAD `f09d640`).

### Added

- **WI-S20-001..008** — PRR global execution + external pentest contract
  + 30d-sustained-staging evidence framework + SOC 2 gap-analysis prep
  + 3-lighthouse-customer migration scaffold + SLA/DPA v1 publish
  + incident-response 24/7 PagerDuty rotation + launch orchestration kit.
- **WI-S20-007** — 30d staging evidence framework + TLA+ 4 runbooks
  + 90d SBOM retention + PRR-S20 closing audit.
- **WI-S20-008** — Launch orchestration prep: press release + 5 blog
  posts + 3 case studies + Product Hunt kit + social media kit
  + launch runbook + launch-metrics dashboard. **Final WI of final sprint.**

### Changed

- Sprint-close round-1 P0 remediation (7.2/10 → SEAL approved); see
  `_audits/sprint-close-round-1.md`.
- Synthetic-page-drills migration renumbered `0042` → `0043` to remove
  conflict with `0042_lighthouse_customers.sql`.

### Fixed

- P0 audit findings round-1 cascade — engineering-gate-vs-launch
  separation enforced in §4 capability table.

### Security

- External pentest report intake gated on EVT-040; HIGH/CRITICAL
  remediation pre-condition for `GA-Full` tag promotion.

---

## [0.19.0] - 2026-05-14 — S-19: Customer Onboarding

Tag: `s19-impl-sealed` (HEAD `e967c65`).

### Added

- **WI-S19-001..006** — Self-service signup business logic + DPA
  click-through (CTRL-PRIV-CONSENT-001..006 capture, EVT-049, signed
  JWT receipt) + tier selection + Stripe Checkout integration
  + enterprise inquiry form with white-glove handoff
  + conversion-funnel instrumentation + DPA versioning re-acceptance.
- **WI-S19-004** — Tier selection + Stripe Checkout + INV-ONBOARD-DPA-FIRST
  + D1 row-lock atomicity (subscription activation requires DPA signed).
- **WI-S19-005** — Enterprise inquiry + Slack/CRM atomic outbox + 24h
  auto-reply SLA.
- 4 D1 migrations: `0037_signup_orchestration` · `0038_dpa_acceptances`
  · `0039_tier_selection` · `0040_enterprise_inquiries`
  · `0041_dpa_versioning`.

### Changed

- Lane upgrade STANDARD → **HIGH_RISK** per codex finding
  (FF-HR-009 customer-facing contract).
- Region pinning derives from rendered-locale cookie `corelink_locale`
  (set by S-16 middleware), not `Accept-Language` header
  (Lote 10.19 codex P1 canonical fix).
- Sprint-close round-1 P1 remediation (8.4/10 → SEAL approved).

### Security

- FF-HR-009 enforced: DPA + Terms click-through cryptographically
  proven via signed JWT receipt (legal-exposure mitigation).
- Stripe webhook signature verification + D1 idempotency keys
  (`0044_stripe_webhook_events_processed`).

---

## [0.18.0] - 2026-05-14 — S-18: Public Docs + API Reference + Pricing

Tag: `s18-impl-sealed` (HEAD `a1d00f1`).

### Added

- **WI-S18-001..005** — Docusaurus 3.x at `apps/docs/` deployed to CF
  Pages with Diátaxis taxonomy (tutorial / how-to / reference /
  explanation) + 5-min Bazel/Buck2/Native quickstart + REAPI v2
  auto-generated reference + SDK guides (Python/Go/JS/CLI) + compliance
  & security page (SOC 2 timeline + SBOM access + pentest exec summary)
  + pricing page (5 tiers + feature matrix + calculator).
- **CAP-DOCS-007** — i18n (en/pt-BR/es) + WCAG 2.2 AA + Lighthouse ≥ 95.
- **CAP-DOCS-008** — Vale tone-lint + lychee broken-link CI gates.

### Changed

- Sprint-close round-1 P0 remediation (5.5/10 → SEAL target reached).
- Sprint-close round-2 P1 — replace 28 i18n stub relative imports with
  `@site/src/` alias.
- Pin Node 20 + add `.npmrc` / `.nvmrc` — root-cause Docusaurus build
  failure under Node 22.

### Fixed

- Cross-functional anti-scope gate (§10): pricing/security claims
  require Finance + Legal + Security review before publish.

---

## [0.17.0] - 2026-05-14 — S-17: Ops Maturity

Tag: `s17-impl-sealed` (HEAD `bbbd99a`).

### Added

- **WI-S17-001..006** — Chaos engineering automation (weekly staging
  chaos, deterministic seed, ≥ 8 FMs covered, auto-rollback on SEV-1)
  + DR drill scheduler (semestral cadence, full region outage simulation)
  + runbook dry-run tracker EVT-017 (monthly P0/P1 runbook cadence)
  + incident + blameless post-mortem templates + oncall rotation with
  fatigue tracking + chaos catalog + game-day tabletop exercises.
- **WI-S17-003** — Runbook dry-run tracker + 3 P0/P1 monthly cadence
  (PAT-RUNBOOK-DRILL-001).
- **WI-S17-004** — Incident + blameless post-mortem templates +
  1 synthetic SEV-2 post-mortem + RB-POSTMORTEM-PROCESS.
- **WI-S17-005** — Oncall scheduler + fatigue tracking + PagerDuty
  integration trait + Grafana dashboard.
- 4 D1 migrations: `0033_chaos_runs` · `0034_dr_drill_runs`
  · `0035_runbook_drills` · `0036_oncall_pages`.

### Changed

- Sprint-close round-1 P0 remediation (7.6/10 → SEAL approved).
- Sprint duration corrected 2.5 → 4 weeks to accommodate parallel
  4-week chaos test (codex finding).

### Security

- Chaos discipline: production chaos NOT authorized at GA;
  staging-only weekly for 4 weeks pre-GA.

---

## [0.16.0] - 2026-05-14 — S-16: Frontend Admin UI

Tag: `s16-impl-sealed` (HEAD `5d70701`).

### Added

- **WI-S16-001..007** — Next.js 15 at `apps/web/` deployed to CF Pages
  with: tenant onboarding flow + per-tenant usage dashboard
  + audit-log viewer (CloudEvents R2 query proxy) + consent management
  UI (6-field proof: notice_text_hash + version + locale + wording_id
  + ui_capture_ts + submission_ts) + DSR request form (6 rights, MFA
  re-auth, JWT receipt) + PAT management + privacy/sub-processors
  pages + billing overview.
- **WI-S16-007** — Playwright e2e + Lighthouse + axe sweep + CSP
  enforce + UX workshop + PRR-S16.
- **CAP-UI-009** — i18n (en/pt-BR/es) + WCAG 2.2 AA baseline.

### Changed

- Sprint-close round-1 P0 remediation (7.4/10 → SEAL prep).
- HF-S17-001 — remove nested `<html>` from `[locale]/layout.tsx`.
- **CAP-UI-002** (full Grafana embed) **DEFERRED** post-S-16 to S-18 or
  post-GA (Lote 10.16 codex P0 fix); S-16 ships basic plan/quota
  progress widget + audit-viewer link + billing overview.

### Deprecated

- Inline `--telemetry=on` CLI flag (rejected per Lote 10.15 alignment;
  telemetry only via persistent `~/.corelink/config.toml`).

### Security

- Hardened CSP `default-src 'none'` + explicit allowlists enforced;
  XSS-exfiltration of PAT mitigated.

---

## [0.15.0] - 2026-05-14 — S-15: CLI + SDK Integration

Tag: `s15-impl-sealed` (HEAD `88cd55b`).

### Added

- **WI-S15-001..006** — `corelink` CLI (7 subcommands: ls / get / put
  / stat / bench / doctor / version) cross-OS signed (macOS notarized,
  Linux GPG-signed, Windows Authenticode-signed) + Bazel starter project
  with credential-helper-protocol + Buck2 starter project + FFI wrappers
  (Python pyO3, Go cgo, JS/TS WASM) with client-verify default-on
  + CI templates (GitHub Actions + GitLab + CircleCI).
- **WI-S15-006** — Fuzz 1M + 3-OS signing + 2 OSS-proof
  conformance + PRR-S15 + adversarial summary.
- `corelink doctor` 8-check actionable diagnostic (network, auth,
  storage write, storage read, BYOK, region, quota, client-verify).

### Changed

- Sprint-close round-1 P0 remediation (8.7/10 final).
- Bazel `.bazelrc` uses credential-helper protocol (Bazel 6+) — PAT
  via stdout-JSON, never in `argv` (CTRL-CRED-001 enforcement;
  Lote 10.15 canonical fix).

### Security

- Tokens never in CLI args; only env-var `CORELINK_PAT` or
  `~/.corelink/config.toml`.

---

## [0.14.0] - 2026-05-14 — S-14: Region Expansion + BYOK

Tag: `s14-impl-sealed` (HEAD `70ea887`).

### Added

- **WI-S14-001..009** — 4 production regions (WNAM us-west, ENAM us-east,
  WEUR eu-west, SAM sa-east) + tenant primary_region pinning + hot-blob
  cross-region replication (top 1% via offline aggregation, escaping
  INV-OBS-CARDINALITY-BUDGET) + PAT-REGION-FAILOVER-001 read failover.
- **CAP-BYOK-001..006** — BYOK adapter trait `crates/corelink-byok`
  with 4 KMS providers: AWS KMS (FIPS 140-3 L1), GCP KMS (FIPS 140-2 L1),
  Azure Key Vault Premium (FIPS 140-2 L2), HashiCorp Vault Enterprise
  (FIPS 140-3 L1).
- **WI-S14-007** — Ed25519 (FIPS 186-5) erasure attestation + JCS
  canonicalization + 7y retention + verify path.
- **WI-S14-008** — DPA amendment + Schrems II TIA + Legal external
  review path.
- **WI-S14-009** — TLA+ `region_residency` spec + RB-BYOK-REVOKE
  prod-grade + 3 RB dry-runs + pentest stub + PRR-S14.
- 6 D1 migrations: `0027_region_provisioning` · `0028_tenant_primary_region`
  · `0029_hot_blobs` · `0030_byok_envelope` · `0031_byok_tenant_status`
  · `0032_erasure_attestation`.

### Changed

- Customer kill switch SLA ≤ **6 min p99** (60s detection + 5min DEK
  cache TTL hard, codex-corrected from initial 5 min target).
- **DEK derivation** — random 32 bytes via `getrandom::getrandom`
  CSPRNG, **not** BLAKE3-derived from blob hash (Lote 10.14 codex P1
  fix; deterministic DEK = compromise propagation across blobs).
- AES-256-GCM (FIPS 197 + FIPS 140-3 approved) with 96-bit random
  nonce; per-blob envelope encryption.
- Sprint-close P0+P1 remediation cascade (6.48/10 FAIL → 8.5+).

### Fixed

- Port `corelink-byok-revocation` + `customer-alerts` to canonical
  trait surface (sprint-close P0-1 + P0-2 resolution).

### Security

- **R1-9!** FIPS-mode toggle documented per provider in
  `compliance/byok-fips-matrix.md`.
- Cross-region tenant isolation property-tested at 20k iter, 0 leaks.
- Customer kill switch end-to-end runbook RB-BYOK-REVOKE dry-run
  evidence committed.

---

## [0.x] - 2026-04 to 2026-05 — Spec corpus phase (S-00 → S-13)

Tags: `s00-impl-sealed` ... `s13-impl-sealed` (14 tags).

This collapsed section records the **pre-1.0 framework history**: the
foundation sprints that delivered the spec corpus (262 docs · 136 invariants),
the reference Rust crates (~70 crates wired against `InMemoryFake` traits),
the 8 TLA+ specifications, and the canonical-source bedrock that
everything from S-14 onwards inherits from.

| Sprint | Date | Name | Lane | Highlights |
|---|---|---|---|---|
| **S-00** | 2026-04-15 | Roadmap & Planning | n/a | 14 canonical sources skeleton + sprint waveform planned. |
| **S-01** | 2026-04-29 | CAS Foundation (write path + HMAC + integrity) | HIGH_RISK | `corelink-hash` BLAKE3 + `corelink-worker` R2 PUT + `corelink-reapi` REAPI v2 gRPC handlers (BatchUpdateBlobs, Capabilities, ByteStream::Write); vendored bazelbuild/remote-apis @ v2.12.0 proto subset; CloudEvents 1.0 audit envelope; INV-CAS-INTEGRITY enforced. |
| **S-02** | 2026-04-30 | CAS Read Path + Client Verify | HIGH_RISK | Server-and-client BLAKE3 verify default-on; `corelink-client-verify` crate; CTRL-CAS-002. |
| **S-03** | 2026-05-01 | Auth Real | HIGH_RISK | Clerk integration + PAT scope model + MFA + `corelink-clerk` + `corelink-clerk-cf`. |
| **S-04** | 2026-05-01 | Action Cache (AC) | HIGH_RISK | AC put/get + HKDF-keyed-MAC signature + dedup-safe; `corelink-ac::merkle` RFC 6962-style domain separation. |
| **S-05** | 2026-05-01 | Multipart Upload + Chunking + Merkle (blobs > 5 MiB) | HIGH_RISK | `corelink-chunker` + `corelink-manifest` Merkle manifest with O(1) streaming-memory verify (INV-MULTIPART-STREAMING-MEMORY); MAX_CHUNKS_PER_BLOB = 81920. |
| **S-06** | 2026-05-01 | Garbage Collection: Mark & Sweep + INV-GC-001/004 | HIGH_RISK | `corelink-gc` worker binary + scheduler + 8 GcEventType audit taxonomy + degrade overload-detector + partial-UNIQUE running-status invariant. |
| **S-07** | 2026-05-02 | Dedup + Eviction Policy (intra-tenant default; cross-tenant backlog) | STANDARD | LRU + LFU + size-tiered eviction; intra-tenant dedup default; cross-tenant deferred. |
| **S-08** | 2026-05-03 | Rate Limiting Multi-Camada + Quotas + Abuse Detection | HIGH_RISK | Token-bucket multi-layer + abuse-score + edge blocklist + quota FSM + circuit breaker. |
| **S-09** | 2026-05-05 | Observability Stack | HIGH_RISK | OTLP traces + structured logs + 4-burn-rate SLO alerts + audit-log CloudEvents R2 + cardinality budget INV-OBS-CARDINALITY-BUDGET. |
| **S-10** | 2026-05-07 | Billing Pipeline | HIGH_RISK | Stripe webhook idempotency + usage-event-idem + billing replay audit + reconciliation drift detector + Stripe Checkout. |
| **S-11** | 2026-05-09 | Privacy Pipeline | HIGH_RISK | CTRL-PRIV-CONSENT-001..006 6-field consent capture + 6 DSR rights + erasure log + notice-text-hash canonicalization. |
| **S-12** | 2026-05-11 | Supply Chain Hardening | HIGH_RISK | CycloneDX 1.5+ SBOM + cosign signing + SLSA L3 attestation + cargo-audit + cargo-deny + Dependabot auto-merge + Bazel/Buck2 starter CI. |
| **S-13** | 2026-05-13 | Admin Plane | HIGH_RISK | Admin op-log + rotation-state + admin surfaces gated behind feature-flag for tenant emergency ops. |

**Cumulative deliverables at end of phase:**

- ~70 Rust crates compiling + testing under `cargo test --workspace`.
- 26 D1 migrations (`0001` ... `0026`) all additive-only (INV-AUTH-MIGRATION-ADDITIVE).
- 8 TLA+ specifications.
- 244 vitest cases in `apps/web` + 264 in `apps/docs`.
- 62 runbooks in `specs/_runbooks/`.
- 14 canonical sources SEALED (`SECURITY-MODEL`, `PRIVACY-MODEL`,
  `AUTH-MODEL`, `KEY-MANAGEMENT`, `COMPLIANCE-MATRIX`,
  `STORAGE-SEMANTICS-MATRIX`, `RESILIENCE-PATTERNS`,
  `OBSERVABILITY-MODEL`, `SLO-CATALOG`, `FAILURE-MODES`,
  `INVARIANT-REGISTRY`, `DATA-MODEL`, `REMOTE-CACHE-PRODUCT-PROFILE`,
  `FRAMEWORK-00`).

---

## Migration Notes (post-1.0)

### Applying D1 migrations

CoreLink ships **43 D1 migrations** (`migrations/d1/0001_blob_meta.sql` ...
`migrations/d1/0044_stripe_webhook_events_processed.sql`, with one
renumbering hop `0042_lighthouse_customers.sql` introduced in S-20).
All migrations are **additive-only** (INV-AUTH-MIGRATION-ADDITIVE):
no destructive `DROP`, no breaking column rename without dual-write
transition window.

Apply via the canonical runner:

```bash
./scripts/d1-migration-runner.sh <staging|prod> <d1-binding-id>
```

Pre-conditions:

- Cloudflare auth (`wrangler login`) from an operator workstation.
- D1 binding ID for the target environment.
- Read `specs/_runbooks/RB-D1-MIGRATION-APPLY.md` before promoting to prod.

Offline pre-flight (CI also runs these):

```bash
python3 scripts/check_migrations_additive.py
python3 scripts/d1-migration-verify.py --schema-only
cargo test -p corelink-d1-migrations --test d1_migration_integration
```

### Required environment variables

The following secrets MUST be set in the operator workstation or CF
Workers binding before a fresh deploy will boot:

| Var | Scope | Notes |
|---|---|---|
| `CORELINK_PAT` | CLI / SDK | Tenant PAT; never pass via CLI argv (CTRL-CRED-001). |
| `CLERK_SECRET_KEY` | Worker | Server-side Clerk JWT verify. |
| `CLERK_PUBLISHABLE_KEY` | Worker / Web | Client-side. |
| `STRIPE_SECRET_KEY` | Worker | Subscription + webhook. |
| `STRIPE_WEBHOOK_SECRET` | Worker | Signature verify. |
| `PAGERDUTY_INTEGRATION_KEY` | Worker | Events API v2 (S-17). |
| `SLACK_WEBHOOK_URL` | Worker | Enterprise inquiry notify (S-19). |
| `HUBSPOT_API_KEY` | Worker | CRM atomic outbox (S-19). |
| `AWS_KMS_KEY_ARN` | Tenant BYOK | Per-tenant; required if `tenant.byok_provider = aws`. |
| `GCP_KMS_KEY_NAME` | Tenant BYOK | Per-tenant. |
| `AZURE_KEY_VAULT_URI` | Tenant BYOK | Per-tenant. |
| `VAULT_TRANSIT_KEY` | Tenant BYOK | Per-tenant. |
| `GRAFANA_CLOUD_PUSH_TOKEN` | Worker | Metrics push. |
| `DRATA_API_TOKEN` | Worker | SOC 2 evidence collection (S-20). |

### Breaking changes warning — major version bumps

When bumping the major version (e.g., `2.0.0`):

1. Review all entries under `### Removed` and `### Changed` since the
   previous major in this changelog.
2. Run `scripts/d1-migration-verify.py --diff-from <prev-major-tag>`
   to catalogue destructive operations introduced under the new major.
3. **Required**: 6-month deprecation window for any public REAPI v2
   surface change; cross-reference `specs/_canonical/REMOTE-CACHE-PRODUCT-PROFILE.md`
   §wire-compat-matrix.
4. **Required**: signed customer notice 30d before any breaking change
   that affects PAT scope, BYOK envelope format, or audit event schema.
5. Re-run external pentest (Schellman or A-LIGN) before tagging `vN.0.0`.
6. Bump `compliance/version_pins.yaml` and trigger SOC 2 Type II
   continuous-monitoring re-baseline.

[Unreleased]: https://github.com/humangr-labs/corelink/compare/ga-engineering-gate-complete...HEAD
[1.0.0-rc.1]: https://github.com/humangr-labs/corelink/releases/tag/ga-engineering-gate-complete
[0.20.0]: https://github.com/humangr-labs/corelink/releases/tag/s20-impl-sealed
[0.19.0]: https://github.com/humangr-labs/corelink/releases/tag/s19-impl-sealed
[0.18.0]: https://github.com/humangr-labs/corelink/releases/tag/s18-impl-sealed
[0.17.0]: https://github.com/humangr-labs/corelink/releases/tag/s17-impl-sealed
[0.16.0]: https://github.com/humangr-labs/corelink/releases/tag/s16-impl-sealed
[0.15.0]: https://github.com/humangr-labs/corelink/releases/tag/s15-impl-sealed
[0.14.0]: https://github.com/humangr-labs/corelink/releases/tag/s14-impl-sealed
[0.x]: https://github.com/humangr-labs/corelink/compare/s00-impl-sealed...s13-impl-sealed
