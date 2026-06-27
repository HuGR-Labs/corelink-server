---
type: "Flow"
title: "Signup auto-provision + account-deletion erasure (Clerk webhook)"
description: "The edge signup-worker webhook: a Svix-verified Clerk user.created auto-provisions a tenant + PAT, and a user.deleted enqueues a GDPR right-to-erasure request fail-CLOSED (never a silent drop), with Svix/Stripe signature headers scrubbed from error telemetry."
source_files:
  - "apps/signup-worker/src/webhooks/clerk.ts"
  - "apps/signup-worker/src/index.ts"
  - "apps/signup-worker/src/lib/d1.ts"
  - "apps/signup-worker/src/lib/clerk-metadata.ts"
checkpoint_sha: "d0e4f8bd669cb1e982f7511de9a895c602a5ee45"
provenance: "AUTHORED"
tags: ["flows", "signup", "clerk", "webhook", "dsr", "erasure", "worker-edge"]
timestamp: "2026-06-27T00:00:00Z"
---

# Signup auto-provision + account-deletion erasure (Clerk webhook)

This is the live edge entry point where a Clerk identity event becomes (or un-becomes) a CoreLink tenant. The `signup-worker` Cloudflare Worker receives Clerk's `user.created` and `user.deleted` webhooks (delivered by Svix), verifies the Svix signature, and then drives two opposite obligations: `user.created` auto-provisions a tenant + a read-write PAT so a stranger can start using the cache, and `user.deleted` enqueues a GDPR right-to-erasure request. Both are auth-state mutations with outsized blast radius, so the whole path is engineered to fail CLOSED — a missing secret or unbound queue returns a 500 that Svix retries, never a silent success. This concept covers the deletion/erasure trigger and the worker wiring; the provisioning happy-path detail and the container-side pilot reservation are in the sibling [signup -> tenant onboarding flow](/launch/signup-onboarding.md), and the Stripe activation that follows is [the Stripe activation webhook](/launch/stripe-activation-webhook.md).

# Role

The webhook is the pre-tenant boundary: it turns a Clerk-authenticated identity into a provisioned tenant (`user.created`), or honors that identity's right-to-be-forgotten by enqueuing erasure (`user.deleted`). It sits in the `signup-worker` worker, distinct from the main API worker, and is the sole place a Clerk lifecycle event crosses into CoreLink's tenant/PAT/D1 state. Because it provisions and erases, it must be idempotent under Svix redelivery and must never silently drop either obligation.

# How it works

- The worker `route()` dispatches `POST /webhooks/clerk` to `handleClerkWebhook`, `POST /webhooks/stripe` to the Stripe handler, and `/health` to a liveness JSON; anything else is 404 (`apps/signup-worker/src/index.ts:26-35`).
- `handleClerkWebhook` requires `POST`, reads the `svix-id`/`svix-timestamp`/`svix-signature` headers, and rejects a missing header set with 400 before any work (`apps/signup-worker/src/webhooks/clerk.ts:834-842`).
- Svix verification HMAC-SHA256s `${svix-id}.${svix-timestamp}.${body}` and constant-time compares each `v1,<sig>` candidate; an anti-replay window rejects a `svix-timestamp` outside ±300s before any HMAC work; failure is 401 (`apps/signup-worker/src/webhooks/clerk.ts:451-497`, `apps/signup-worker/src/webhooks/clerk.ts:851-853`).
- After verify, the body is parsed and `user.deleted` routes to `handleUserDeleted`, `user.created` falls through to provisioning, and any other event type is a 200 `ignored` no-op (`apps/signup-worker/src/webhooks/clerk.ts:862-868`).
- `user.created` provisioning is gated FAIL-LOUD: a missing `CORELINK_INTERNAL_AUTH_KEY` returns 500 with ZERO side effects (before the first tenant write) so a Svix redelivery cleanly re-provisions once the secret is set (`apps/signup-worker/src/webhooks/clerk.ts:940-949`).
- `deterministicDsrId` derives a stable v5-shaped UUID from `SHA-256("corelink-dsr-v1:" + clerkUserId)`, so the same deleted account always maps to ONE `dsr_id` and the erasure orchestrator (which dedups per `(dsr_id, backend)`) is idempotent across redeliveries (`apps/signup-worker/src/webhooks/clerk.ts:135-145`, `apps/signup-worker/src/webhooks/clerk.ts:216`).
- `handleUserDeleted` short-circuits to a 200 `erasure_enqueued:false` no-op when the event carries no user id (`reason:"no_user_id"`) or when no provisioned tenant maps to the Clerk user (`reason:"no_tenant"`) — nothing exists to erase (`apps/signup-worker/src/webhooks/clerk.ts:293-296`, `apps/signup-worker/src/webhooks/clerk.ts:316-319`).
- The tenant is resolved by `SELECT tenant_id FROM tenant WHERE clerk_user_id = ?1`; a D1 read error returns 500 (not a no-op) so Svix retries rather than dropping the deletion (`apps/signup-worker/src/webhooks/clerk.ts:301-313`).
- When a tenant DOES exist but `DSR_QUEUE` is unbound, the handler logs and returns 500 `dsr_queue_unconfigured` — it refuses to ack a deletion it cannot honor (`apps/signup-worker/src/webhooks/clerk.ts:321-330`).
- On the happy path it builds the `dsr.queued.v1` message (deterministic `dsr_id` + erasure salt + resolved legal-hold), writes an `INSERT OR IGNORE` `dsr_requested` SLA anchor, `DSR_QUEUE.send(msg)`s it, and returns 200 `erasure_enqueued:true` — logging only the pseudonymous `dsr_id`/`tenant_id`, never the salt (`apps/signup-worker/src/webhooks/clerk.ts:341-385`, `apps/signup-worker/src/webhooks/clerk.ts:388-395`).
- **The tenant + PAT D1 writes are race-safe and self-serve-active.** `insertTenant` writes the new row `INSERT OR IGNORE` with `tenant_state` hard-coded to `'active'` (self-serve bypasses the DPA-pending pilot flow), and `insertPat` writes `shown_once_consumed = 1` so the one-time reveal endpoint cannot re-surface a plaintext the PLG `/welcome` session already delivered; both are `INSERT OR IGNORE` so a double Svix delivery is a no-op (`apps/signup-worker/src/lib/d1.ts:76-95`, `apps/signup-worker/src/lib/d1.ts:108-130`).
- **A team-seat acceptance is keyed by the SHA-256 `email_hash`, never the raw email.** `acceptTeamInvitation` selects an outstanding `team_member` row by `email_hash` (CTRL-PRIV-001) and re-asserts `status='invited'` on the UPDATE so a concurrent acceptance cannot double-flip a seat (`apps/signup-worker/src/lib/d1.ts:154-177`).
- **The PAT plaintext is classified into PRIVATE metadata, the tenant claims into PUBLIC.** `updateClerkUserMetadata` PATCHes Clerk `public_metadata` with only `{tenant_id, region}` (the legit session-JWT claims) and routes the one-time `pat_plaintext` into `private_metadata` (backend-only, never in the JWT, never readable by `useUser()`) — the secret-classification boundary that keeps the credential out of the client-visible session (`apps/signup-worker/src/lib/clerk-metadata.ts:67-86`, `apps/signup-worker/src/lib/clerk-metadata.ts:82-85`).

# Invariants

- An erasure obligation is NEVER silently dropped: every failure mode after a tenant is resolved (unbound queue, salt-derivation failure, D1 lookup error) returns 500 so Svix redelivers, instead of a misleading 200 (`apps/signup-worker/src/webhooks/clerk.ts:321-330`, `apps/signup-worker/src/webhooks/clerk.ts:351-362`, `apps/signup-worker/src/webhooks/clerk.ts:307-313`).
- The `dsr_id` is deterministic per Clerk user, so a Svix redelivery of the same `user.deleted` enqueues an idempotent message and the `dsr_requested` anchor is an `INSERT OR IGNORE` no-op — erasure never double-runs (`apps/signup-worker/src/webhooks/clerk.ts:135-145`, `apps/signup-worker/src/webhooks/clerk.ts:373-377`).
- The erasure salt is derived `HMAC-SHA256(ERASURE_SALT_KEY, dsr_id)`; when the key is absent in a `prod` environment `deriveErasureSalt` THROWS, the caller returns 500, and the predictable non-secret SHA-256 fallback can never reach production (`apps/signup-worker/src/webhooks/clerk.ts:160-189`).
- Provisioning is fail-CLOSED on its mint secret: `handleClerkWebhook` 500s on an absent `CORELINK_INTERNAL_AUTH_KEY` before writing a tenant row, so a half-provisioned PAT-less tenant is never committed (`apps/signup-worker/src/webhooks/clerk.ts:940-949`).
- Secret-bearing webhook headers are scrubbed from Sentry telemetry: the `SENSITIVE_HEADER_PATTERN` regex filters `authorization`/`cookie`/`svix-signature`/`svix-id`/`svix-timestamp`/`stripe-signature` (and more) to `[Filtered]` in `beforeSend` (`apps/signup-worker/src/index.ts:125-138`).
- The DSR erasure queue consumer rethrows on error so the Cloudflare queue runtime redelivers the whole batch; redelivery is safe because the erasure orchestrator is idempotent (`apps/signup-worker/src/index.ts:69-79`).
- The PAT plaintext NEVER enters a client-readable surface: `updateClerkUserMetadata` writes it ONLY to Clerk `private_metadata` (backend-only) and the legit `{tenant_id, region}` claims to `public_metadata`, so the secret is never embedded in the session JWT nor exposed via `useUser()` (`apps/signup-worker/src/lib/clerk-metadata.ts:82-85`).
- A double Svix delivery of the same `user.created` can never create a second tenant or a second PAT: both `insertTenant` and `insertPat` are `INSERT OR IGNORE`, so the second writer is silently dropped (`apps/signup-worker/src/lib/d1.ts:82-85`, `apps/signup-worker/src/lib/d1.ts:114-117`).
- A team-seat flip is isolated to the SHA-256 `email_hash` and cannot be double-applied: `acceptTeamInvitation` re-asserts `status='invited'` in the `WHERE` so a concurrent acceptance clobbers nothing (`apps/signup-worker/src/lib/d1.ts:168-175`).

# Gotchas

- The webhook's `request.cf.colo` is SVIX's sender PoP, NOT the end-user's geo, so residency is deliberately NOT derived from it; webhook-provisioned tenants default to the launch-served region (`apps/signup-worker/src/webhooks/clerk.ts:871-885`).
- Idempotency on `user.created` keys on a tenant row AND a still-live PAT — a tenant row alone is treated as a half-failed prior attempt and falls through to re-issue the PAT, never acked as done (`apps/signup-worker/src/webhooks/clerk.ts:887-926`).
- `legal_hold` is resolved from D1 (`tenant_legal_hold` existence) and carried into the message; the read posture on a query error is `false` (erase proceeds) — deliberately, so a not-yet-provisioned hold table cannot silently no-op every deletion (`apps/signup-worker/src/webhooks/clerk.ts:258-280`).

# Citations

1. `apps/signup-worker/src/index.ts:26-35` — `route()` path table (`/webhooks/clerk`, `/webhooks/stripe`, `/health`, 404).
2. `apps/signup-worker/src/index.ts:69-79` — DSR erasure queue consumer (capture + rethrow → batch redelivery).
3. `apps/signup-worker/src/index.ts:125-138` — `SENSITIVE_HEADER_PATTERN` + `scrubAuthorization` (Svix/Stripe sig-header scrub).
4. `apps/signup-worker/src/webhooks/clerk.ts:135-145` — `deterministicDsrId` (stable v5-shaped dsr_id).
5. `apps/signup-worker/src/webhooks/clerk.ts:160-189` — `deriveErasureSalt` (HMAC salt; fail-CLOSED in prod).
6. `apps/signup-worker/src/webhooks/clerk.ts:216` — `deterministicDsrId` call inside `buildErasureQueueMessage`.
7. `apps/signup-worker/src/webhooks/clerk.ts:258-280` — `tenantUnderLegalHold` (D1 read; error → false).
8. `apps/signup-worker/src/webhooks/clerk.ts:293-296` — `no_user_id` short-circuit (200 no-op).
9. `apps/signup-worker/src/webhooks/clerk.ts:301-313` — tenant resolve `SELECT ... WHERE clerk_user_id`; D1 error → 500.
10. `apps/signup-worker/src/webhooks/clerk.ts:316-319` — `no_tenant` short-circuit (200 no-op).
11. `apps/signup-worker/src/webhooks/clerk.ts:321-330` — `DSR_QUEUE` unbound → 500 fail-closed.
12. `apps/signup-worker/src/webhooks/clerk.ts:341-385` — build message + `dsr_requested` SLA anchor (`INSERT OR IGNORE`).
13. `apps/signup-worker/src/webhooks/clerk.ts:351-362` — salt-derivation failure → 500.
14. `apps/signup-worker/src/webhooks/clerk.ts:388-395` — `DSR_QUEUE.send` enqueue + redacted log + 200 `erasure_enqueued:true`.
15. `apps/signup-worker/src/webhooks/clerk.ts:451-497` — `verifySvixSignature` (HMAC + anti-replay + constant-time).
16. `apps/signup-worker/src/webhooks/clerk.ts:834-868` — `handleClerkWebhook` header gate, verify, event dispatch.
17. `apps/signup-worker/src/webhooks/clerk.ts:871-885` — Svix-PoP residency gotcha (colo = null).
18. `apps/signup-worker/src/webhooks/clerk.ts:887-926` — `user.created` idempotency (tenant + live PAT).
19. `apps/signup-worker/src/webhooks/clerk.ts:940-949` — `CORELINK_INTERNAL_AUTH_KEY` fail-LOUD before any write.
20. `apps/signup-worker/src/lib/d1.ts:76-95` — `insertTenant` (`tenant_state='active'`, `INSERT OR IGNORE`); the `'active'` literal + idempotent VALUES: `apps/signup-worker/src/lib/d1.ts:82-85`.
21. `apps/signup-worker/src/lib/d1.ts:108-130` — `insertPat` (`shown_once_consumed = 1`, `INSERT OR IGNORE`); the `, 1, ` consumed flag in VALUES: `apps/signup-worker/src/lib/d1.ts:114-117`.
22. `apps/signup-worker/src/lib/d1.ts:154-177` — `acceptTeamInvitation` (`email_hash`-keyed seat lookup, re-asserted `status='invited'` on UPDATE); the isolation UPDATE: `apps/signup-worker/src/lib/d1.ts:168-175`.
23. `apps/signup-worker/src/lib/clerk-metadata.ts:67-86` — `updateClerkUserMetadata` (public-vs-private metadata secret-classification boundary); the PATCH body split routing `pat_plaintext` to `private_metadata`: `apps/signup-worker/src/lib/clerk-metadata.ts:82-85`.
