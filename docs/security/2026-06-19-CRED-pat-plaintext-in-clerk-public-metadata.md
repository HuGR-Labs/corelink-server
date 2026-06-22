# SECURITY FINDING — PAT plaintext in Clerk public_metadata (JWT-broadcast + permanent)

> 2026-06-19 · CoreLink Server TL · severity **HIGH** (credential mishandling, launch-relevant).
> Discovered while provisioning the runners dogfood tenant (the owner's PAT was found sitting in his
> Clerk `public_metadata`). Owner authorized the end-to-end fix.

## Finding

The signup flow writes the freshly-minted **PAT plaintext** into the user's Clerk **`public_metadata`**
(`{tenant_id, region, pat_plaintext}`), so the `/welcome` page can show it once via
`session.sessionClaims.pat_plaintext`.

- **`public_metadata` is client-readable** (`useUser()` Frontend API) **and embedded in the session JWT**
  (sessionClaims) → the PAT is **broadcast in every session token** to every service that validates the
  session — **including githugr** (reads `sessionClaims.publicMetadata`) — plus browser storage, logs, caches.
- The intended cleanup (PATCH `pat_plaintext: null`) is **client-driven** — it only runs if the user opens
  `/welcome` (admin-ui `welcome/actions.ts`). **If they never do, the PAT persists FOREVER.** Confirmed: the
  owner's full read-write PAT was still in `public_metadata` weeks after signup. (The `clerk-metadata.ts`
  comment "cleared by a follow-up scheduled action (deferred to Phase-1)" is **stale** — the only clear is the
  unreliable client one.)
- "Clerk encrypts metadata at rest" is **no mitigation** — it is served in plaintext to clients + JWTs.

**Code:** `apps/signup-worker/src/lib/clerk-metadata.ts` (`updateClerkUserMetadata`), called from
`apps/signup-worker/src/webhooks/clerk.ts` (`publishUserMetadata`, ~lines 688/711). Read side:
`apps/admin-ui/src/app/[locale]/(authenticated)/welcome/page.tsx` (reads `claims.pat_plaintext`),
cleared in `welcome/actions.ts`. Enforced by `scripts/e2e-clerk-signup.sh:344` (fails if absent).

**Bootstrap constraint (why signup mints it server-side):** `POST /v1/customer/keys` (the authenticated
create-PAT endpoint) is **PAT-gated** — it can't bootstrap the first key from a session-only `/welcome`.
So the signup-time server mint stays; only the **delivery channel** must change.

## Fix (SOTA) — move the secret off all client/JWT surfaces

1. **WP-A — signup-worker:** write `pat_plaintext` to Clerk **`private_metadata`** (backend-only; never in
   the JWT, never readable by `useUser()`). `public_metadata` keeps ONLY `{tenant_id, region}` (legit session
   claims). (`clerk-metadata.ts` shape + `clerk.ts` caller + unit tests.)
2. **WP-B — admin-ui welcome:** read `pat_plaintext` **server-side via the Clerk Backend API**
   (`clerkClient().users.getUser()` private_metadata) instead of `sessionClaims`; render once; clear by
   PATCHing **`private_metadata.pat_plaintext: null`**. (`welcome/page.tsx` + `actions.ts` + tests.)
3. **WP-C — guaranteed scrub cron (signup-worker):** scheduled job that clears
   `private_metadata.pat_plaintext` for any user whose row is older than a short TTL (e.g. 1h) — so an
   un-visited `/welcome` cannot leave the secret resident. (Sibling of `dsr_verify_cron`.)
4. **WP-D — e2e:** `scripts/e2e-clerk-signup.sh` checks `private_metadata` (+ that `public_metadata` does NOT
   contain `pat_plaintext`) and exercises the server-side reveal.
5. **WP-E — remediation (prod, lead):** scrub `public_metadata.pat_plaintext` from ALL existing users +
   **rotate** the exposed PATs (they rode JWTs → treat as compromised: revoke the `pat` rows, users re-reveal).

**Residual (documented, accepted):** the plaintext still lives transiently in Clerk `private_metadata`
(a subprocessor's backend store) until the clear/cron. This kills the client/JWT/permanent vectors; a
future hardening could move the one-time reveal into our own D1 single-use store (no plaintext at Clerk at
all) — tracked as a follow-up, not blocking.

## Status
Owner-authorized 2026-06-19. WPs in flight under TL review. The just-minted **dogfood PAT is clean**
(delivered chmod-600, never in any metadata).
