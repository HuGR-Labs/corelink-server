---
type: "LaunchControl"
title: "Signup -> tenant onboarding flow"
description: "How a new customer becomes a provisioned tenant: the Clerk signup -> Svix webhook -> tenant + PAT chain plus the container's HMAC-gated pilot-signup reservation route."
source_files:
  - crates/corelink-container/src/routes/signup.rs
  - crates/corelink-container/src/signup_d1_http.rs
  - docs/operator/e2e-signup-sealed-2026-05-30.md
checkpoint_sha: "2d179926f3f3f3952cc189e1987ce6bf8fa89a4d"
provenance: "AUTHORED"
tags:
  - launch
  - signup
  - onboarding
  - clerk
  - tenant-provisioning
  - pat
timestamp: "2026-06-26T00:00:00Z"
---

Onboarding is where a stranger becomes a billable, cache-using tenant, so it is the gate that the whole self-serve launch funnel depends on. Two surfaces carry the load: the live Clerk signup chain (Clerk `user.created` -> Svix-verified webhook -> tenant row + minted PAT -> Clerk metadata), validated end-to-end in the sealed e2e walkthrough, and the container's pilot-signup route, which redeems a signed pilot token into a `RESERVED` tenant reservation. This concept transcribes both as frozen. Siblings: launch/money-path (the Stripe/tier checkout that runs after a tenant exists) and launch/go-live-readiness (the broader cutover gate).

# Role

The signup surfaces turn an authenticated identity (Clerk) or a signed operator-issued pilot token into a provisioned tenant: a D1 tenant row, a usable PAT, and the metadata/reservation record that lets the customer start using the cache. They sit pre-auth (no tenant id exists yet) and must fail CLOSED — a forgeable key or a closed audit pipeline must not silently provision.

# How it works

- The live signup chain fires on Clerk `user.created` -> `POST /webhooks/clerk` on `corelink-signup.humangr.com`, which Svix-verifies before doing any work (`docs/operator/e2e-signup-sealed-2026-05-30.md:57-58`).
- `autoProvisionFromClerkEvent` runs three steps: `createTenant` D1-inserts a `tenant` row with `clerk_user_id` and `primary_region='enam'` default (region resolves dynamically — the sealed run resolved to `dub`) (`docs/operator/e2e-signup-sealed-2026-05-30.md:59-62`).
- PAT issuance calls a Service Binding to the main worker `/_internal/pat/mint`, which returns `token_plaintext` plus an Argon2id `hash`, then D1-inserts the `pat` row (`docs/operator/e2e-signup-sealed-2026-05-30.md:63-65`).
- `publishUserMetadata` PATCHes the Clerk Backend API `/v1/users/{id}` to set `publicMetadata.{tenant_id, region, pat_plaintext}`, returns HTTP 200, and marks `metadata_published=true` (`docs/operator/e2e-signup-sealed-2026-05-30.md:65-67`).
- The chain is validated by using the resulting PAT against `/v1/users/me` -> HTTP 200 with a resolved `tenant_id` that matches the freshly created row (`docs/operator/e2e-signup-sealed-2026-05-30.md:68-69`).
- The container's pilot route is registered at `/v1/signup/pilot/{token}` (axum 0.8 `{token}` capture) and dispatches `POST` to `handle_pilot_signup` (`crates/corelink-container/src/routes/signup.rs:132`, `crates/corelink-container/src/routes/signup.rs:824-828`).
- The pilot token format is `pilot_<env>_<unix_ms>_<16-hex-random>.<hmac-hex>`, where `env` segregates `staging` from `prod` so a staging mint never opens a prod slot (`crates/corelink-container/src/routes/signup.rs:15-30`).
- `parse_and_verify_pilot_token` splits body/signature on a single `.`, requires exactly 4 underscore fields via `splitn(4)`, checks the `pilot` literal, the env allowlist, a `u64` timestamp, and a 16-char hex random (`crates/corelink-container/src/routes/signup.rs:257-280`).
- Signature verify computes `HMAC-SHA256(SIGNUP_TOKEN_KEY, body)` and compares constant-time via `subtle::ConstantTimeEq`, bailing on length mismatch first (`crates/corelink-container/src/routes/signup.rs:282-292`).
- TTL is enforced last: a token is `Expired` once `now_ms >= minted_at_ms + PILOT_TOKEN_TTL_MS` (14 days); future-minted tokens are accepted as a clock-skew tolerance (`crates/corelink-container/src/routes/signup.rs:150`, `crates/corelink-container/src/routes/signup.rs:296-303`).
- The handler runs the per-IP rate-limit gate BEFORE token verify so an adversary cannot probe the HMAC space at high QPS (`crates/corelink-container/src/routes/signup.rs:869-876`).
- On success the handler mints a UUID v7 `tenant_id`, reserves a `PilotSignupRecord` with state `"RESERVED"` via `insert_or_existing` (idempotent on email/token_id), emits the `pilot_reserved.v1` audit row, then returns `201` with `tenant_id`, `activation_url`, and `state` (`crates/corelink-container/src/routes/signup.rs:947-994`).
- Production route state is built fail-CLOSED from `SIGNUP_TOKEN_KEY` (hex, >= 32 decoded bytes); a missing or short key returns `None` and the route is simply not mounted rather than running with the forgeable dev key (`crates/corelink-container/src/routes/signup.rs:757-771`).

# Invariants

- Body validation requires all four fields non-empty and <= `MAX_FIELD_LEN` (256) chars, and `email` must contain `@`; a violation emits a `bad_request` audit row and returns `400` (`crates/corelink-container/src/routes/signup.rs:358-377`, `crates/corelink-container/src/routes/signup.rs:931-944`).
- Audit emit happens BEFORE the response on every arm and fails CLOSED to `503` (`audit pipeline closed`) per `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER` (`crates/corelink-container/src/routes/signup.rs:515-522`, `crates/corelink-container/src/routes/signup.rs:967-985`).
- Rate limit is burst 5, refill 1/s, with a 720s `Retry-After` floor (`RateLimitConfig::with_overrides(1, 5, 720, …)`); an over-limit request surfaces `429` with `Retry-After: 720`, and a `#[non_exhaustive]` future decision variant falls back to deny (`crates/corelink-container/src/routes/signup.rs:633-637`, `crates/corelink-container/src/routes/signup.rs:879-904`).
- The rate-limit key is anchored on the server-trusted `x-corelink-client-ip` header only; a client-forged `x-forwarded-for` is ignored and a missing/empty trusted header collapses to the shared `"_no_ip"` bucket (fail-CLOSED) (`crates/corelink-container/src/routes/signup.rs:841-848`).
- Pre-auth requests anchor the rate-limit bucket on the nil UUID tenant so `BucketKey::per_ip` collapses to per-IP only (`crates/corelink-container/src/routes/signup.rs:856`, `crates/corelink-container/src/routes/signup.rs:872`).
- The audit emit never logs the full token signature — only the first 32 chars via `token_prefix` (HMAC values are operator-internal forensic data) (`crates/corelink-container/src/routes/signup.rs:885`, `crates/corelink-container/src/routes/signup.rs:997-1002`).
- Store insertion is idempotent on `email` OR `token_id`: a duplicate returns the original record, and the route reports `exit_status` `"duplicate"` instead of `"reserved"` (`crates/corelink-container/src/routes/signup.rs:602-608`, `crates/corelink-container/src/routes/signup.rs:969-974`).
- The sealed e2e run leaves zero orphan rows: the D1 tenant + pat rows and the Clerk user are all deleted in cleanup (`docs/operator/e2e-signup-sealed-2026-05-30.md:73-79`).

- Production persistence is `D1HttpSignupStore`, bound ONLY by `build_state_from_env`: it writes the reservation to D1 over the HTTP client against the `migrations/d1/0053_pilot_signups.sql` schema (`crates/corelink-container/src/signup_d1_http.rs:146`, `crates/corelink-container/src/routes/signup.rs:784`), and the audit row goes to the durable `audit_outbox` seam (`crates/corelink-container/src/routes/signup.rs:794`).

# Gotchas

- `build_state_with_key` returns the IN-MEMORY store + audit sink and is the dev/CI path ONLY. Production wiring binding it was a real shipped bug: the route answered `201 Created` while `pilot_signups` and `pilot_tenants` in prod D1 stayed at `COUNT(*)=0`, so reservations died with the container and the 10-slot cap was unenforceable across the 5 regional workers. `build_state_from_env` now fails CLOSED when the D1 config is absent rather than falling back to the fakes.

- The Clerk webhook secret is the silent killer: orchestrator sessions overwrote `CLERK_WEBHOOK_SECRET` with fake `whsec_` keys, and the canonical secret character is `/` (a Mac-Finder `:` substitution masked it) — set it with `printf '%s'`, never `echo` (`docs/operator/e2e-signup-sealed-2026-05-30.md:30-37`).
- Two latent e2e-script bugs masked a working chain: the D1 name must be `corelink-prod-d1` (CF returns empty `[]` for a wrong name rather than erroring), and the lookup must prefer wrangler v4.95 over v3.114 which silently errors on the `[[containers]]` schema (`docs/operator/e2e-signup-sealed-2026-05-30.md:39-51`).
- `DEV_TOKEN_KEY` lives in the open repo and is forgeable — production MUST mount via `build_state_from_env`; never the hardcoded dev key (`crates/corelink-container/src/routes/signup.rs:716-725`).
- The wire path uses axum 0.8 `{token}` capture, matching the `{token}` the audit-doc cross-reference writes in prose — they describe the same segment (`crates/corelink-container/src/routes/signup.rs:123-132`).

# Citations

- `crates/corelink-container/src/routes/signup.rs:132` — pilot route path constant.
- `crates/corelink-container/src/routes/signup.rs:252-309` — `parse_and_verify_pilot_token` (structure + HMAC + TTL).
- `crates/corelink-container/src/routes/signup.rs:757-771` — fail-CLOSED `build_state_from_env`.
- `crates/corelink-container/src/routes/signup.rs:860-995` — `handle_pilot_signup` (rate-limit -> verify -> validate -> reserve -> audit -> 201).
- `crates/corelink-container/src/signup_d1_http.rs:146-276` — durable D1-backed `SignupStore` (`D1HttpSignupStore` + idempotent `insert_or_existing`).
- `docs/operator/e2e-signup-sealed-2026-05-30.md:55-69` — the confirmed Clerk -> Svix -> tenant + PAT -> metadata chain.
- `docs/operator/e2e-signup-sealed-2026-05-30.md:25-51` — the two webhook-secret / script bugs found during validation.
