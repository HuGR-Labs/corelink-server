/**
 * Clerk `user.created` webhook → auto-provision (Phase-0 PLG framework §4 +
 * execution-plan §2.F).
 *
 * On a verified `user.created` event:
 *   1. Create a tenant named `${user.github_handle || user.email_local}-default`.
 *   2. Pick the nearest CF region from the request `cf.colo` field
 *      (Geo-IP-derived — no DB lookup needed; falls back to `auto`).
 *   3. Attach `plan=free`.
 *   4. Issue a PAT scoped `read-write`, return plaintext **once** to the user
 *      via Clerk's `publicMetadata` so `/welcome` can render it.
 *   5. Emit `signup_completed`, `tenant_created`, `region_assigned`,
 *      `pat_issued` events to the `analytics_events` D1 table.
 *
 * Idempotency: Clerk re-fires webhooks on transient failure. We short-circuit
 * ONLY when provisioning is COMPLETE — the tenant row AND a still-live PAT
 * exist. A tenant row alone is NOT proof (a prior attempt may have died after
 * the tenant write but before the PAT mint); in that case we re-enter the
 * idempotent provisioning flow so the PAT + Clerk publicMetadata are re-issued
 * before we ack.
 */

import {
  buildErasureQueueMessage,
  claimClerkProvision,
  completeClerkProvision,
  deriveErasureSalt,
  deterministicDsrId,
  handleUserDeleted,
  isValidClerkUserId,
  MIN_INTERNAL_AUTH_KEY_LEN,
} from "./clerk_erasure.js";
import type { ClerkUserDeletedEvent, DsrQueuedV1 } from "./clerk_erasure.js";
import {
  d1AnalyticsEmitter,
  emailHashCandidates,
  emailHashFor,
  emailHashLegacy,
  isProvisionedMacro,
  orgMapKeyFor,
  primaryEmailOf,
  PROVISIONED_MACROS,
  regionFromColo,
  tenantSlugFor,
  UnprovisionedRegionError,
} from "./clerk_identity.js";
import type { AnalyticsEmitter, ApiClient, MacroRegion } from "./clerk_identity.js";
import { isProductionEnvironment } from "../../../../config/production_environment.js";
import {
  insertPatStatement,
  insertTenantStatement,
  insertTenantOrgMapStatement,
  insertTenant,
  insertTenantOrgMap,
  seedTenantEntitlementStatements,
  seedTenantEntitlements,
} from "../lib/d1.js";
import {
  signupOwnershipContext,
  writeSignupArtifactBatch,
} from "../signup_writer_ownership.js";
import type { StagingOwnershipContext } from "../staging_load_test_ownership.js";
export {
  d1AnalyticsEmitter,
  emailHashCandidates,
  emailHashFor,
  emailHashLegacy,
  isProvisionedMacro,
  orgMapKeyFor,
  primaryEmailOf,
  PROVISIONED_MACROS,
  regionFromColo,
  tenantSlugFor,
  UnprovisionedRegionError,
} from "./clerk_identity.js";
export type { AnalyticsEmitter, ApiClient, MacroRegion } from "./clerk_identity.js";
export {
  buildErasureQueueMessage,
  deriveErasureSalt,
  deterministicDsrId,
  handleUserDeleted,
  isValidClerkUserId,
} from "./clerk_erasure.js";
export type { ClerkUserDeletedEvent, DsrQueuedV1 } from "./clerk_erasure.js";

export interface ClerkUserCreatedEvent {
  type: "user.created";
  data: {
    id: string;
    email_addresses: Array<{
      id: string;
      email_address: string;
      verification?: { status?: string | null } | null;
    }>;
    primary_email_address_id?: string | null;
    external_accounts?: Array<{
      provider: string;
      username?: string | null;
    }>;
    username?: string | null;
    /**
     * Clerk organization id (`org_...`) when the provisioning event is scoped to
     * an org. A plain individual `user.created` carries none — those principals
     * fall back to the user `id` (sub) as the `tenant_org_map` key. See
     * {@link orgMapKeyFor}.
     */
    organization_id?: string | null;
    org_id?: string | null;
  };
}

export interface AutoProvisionEnv {
  CLERK_WEBHOOK_SECRET: string;
  CORELINK_API_BASE: string;
  /** @deprecated CORELINK_API_TOKEN is no longer used; PAT mint goes via internal route */
  CORELINK_API_TOKEN?: string;
  ANALYTICS_DB?: D1Database;
  /**
   * D1 CONFIG_DB binding — holds the `tenant` and `pat` tables.
   * Required for direct D1 provisioning (Stream-5).
   */
  CONFIG_DB?: D1Database;
  /**
   * Shared secret for `X-Corelink-Internal-Auth` header on
   * `/_internal/pat/mint` calls to the container.
   * Required for Stream-5. Bound via `wrangler secret put CORELINK_INTERNAL_AUTH_KEY`.
   */
  CORELINK_INTERNAL_AUTH_KEY?: string;

  /**
   * DEDICATED secret for the `X-Corelink-Internal-Auth` header on
   * `/_internal/pat/mint`. The container mint gate REQUIRES this key with NO
   * shared fallback (DD-HIGH, WP1); the shared `CORELINK_INTERNAL_AUTH_KEY` no
   * longer authorizes the mint once this is provisioned. Bound via
   * `wrangler secret put CORELINK_PAT_MINT_AUTH_KEY`. Unset ⇒ the mint falls
   * back to the shared key (additive; local dev / pre-provision).
   */
  CORELINK_PAT_MINT_AUTH_KEY?: string;

  /**
   * Service binding to the main CoreLink Worker. Set in wrangler.toml under
   * `[[services]] binding = "CORELINK_API_SVC"`. When present, used for the
   * `/_internal/pat/mint` call to bypass Cloudflare edge error 1014
   * (CNAME Cross-User Banned) that blocks Worker→Worker custom-domain fetches.
   */
  CORELINK_API_SVC?: { fetch: typeof fetch };
  /**
   * Clerk Backend API secret key (`sk_test_...` or `sk_live_...`).
   * Required for `publishUserMetadata`. Bound via `wrangler secret put CLERK_SECRET_KEY`.
   */
  CLERK_SECRET_KEY?: string;

  /**
   * Cloudflare Queue producer for DSR erasure (`dsr.queued.v1`). A verified
   * Clerk `user.deleted` event enqueues here; the consumer (WI-S11-008) drives
   * the erasure orchestrator. When a deletion has a tenant to erase but this
   * binding is absent, the handler FAILS LOUD (500) rather than silently
   * dropping a GDPR right-to-erasure obligation. Absent + no tenant → 200 no-op.
   */
  DSR_QUEUE?: { send(message: unknown): Promise<void> };

  /**
   * Secret key for deriving the per-DSR erasure salt (HMAC-SHA256 over the
   * deterministic `dsr_id`). Yields an unlinkable pseudonymization salt.
   * **Required in prod** — when absent in a prod environment the handler
   * fails closed (500 → Svix retries) rather than silently falling back to
   * a predictable non-secret SHA-256 that breaks GDPR pseudonymization.
   * In non-prod environments (dev/CI) the predictable fallback is still used
   * so tests can run without secrets. Bound via `wrangler secret put ERASURE_SALT_KEY`.
   */
  ERASURE_SALT_KEY?: string;

  /**
   * Deployment environment name (e.g. `"prod"`, `"staging"`, `"dev"`).
   * Used to enforce that production-required secrets like `ERASURE_SALT_KEY`
   * are present before driving GDPR erasure. Bound via `[vars]` in wrangler.toml.
   */
  ENVIRONMENT?: string;

  /** Staging-only key shared with the container admission verifier. */
  CORELINK_STAGING_LOAD_TEST_ADMISSION_KEY?: string;

  /**
   * Server-held salt for the CTRL-PRIV-001 `email_hash` pseudonym (secrets
   * checklist #171). MUST be the SAME value the container reads, or salted
   * team-invites never bind on accept. UNSET → legacy unsalted SHA-256 (the
   * pre-salt scheme, zero regression). The accept-match LOOKUP dual-reads
   * salted-OR-legacy so a pre-salt invite still binds after the salt is set.
   * Bound via `wrangler secret put EMAIL_HASH_SALT`.
   */
  EMAIL_HASH_SALT?: string;
}

/** Clerk `user.deleted` webhook payload (account-deletion → GDPR erasure). */
export interface AutoProvisionResult {
  tenant_id: string;
  region: string;
  plan: "free";
  pat_id: string;
  pat_plaintext: string;
  /**
   * Whether the Clerk metadata PATCH succeeded — `public_metadata` set to
   * `{ tenant_id, region }` and `private_metadata` set to
   * `{ pat_plaintext, pat_revealed_at }`. False means the Clerk PATCH returned
   * a 4xx (typically 404 — user deleted between webhook emit and handler run).
   * Tenant + PAT rows are still persisted in D1; the metadata gap can be
   * reconciled out-of-band. 5xx Clerk failures are re-thrown so Svix retries
   * the whole webhook.
   */
  metadata_published: boolean;
}

interface VerifyContext {
  body: string;
  svixId: string;
  svixTimestamp: string;
  svixSignature: string;
  secret: string;
  /**
   * Current time in unix seconds, used for the `svix-timestamp` freshness
   * (anti-replay) check. Defaults to `Date.now() / 1000`. Injectable for tests.
   */
  nowSeconds?: number;
  /**
   * Max allowed clock skew (seconds) between `svix-timestamp` and `nowSeconds`.
   * Defaults to 300 (5 min) — the Svix canonical default tolerance.
   */
  toleranceSeconds?: number;
}

/** Svix canonical default replay-tolerance window: 5 minutes. */
const SVIX_TIMESTAMP_TOLERANCE_SECONDS = 300;

/**
 * Verify the Svix signature on a Clerk webhook payload.
 *
 * Algorithm (per Svix docs): HMAC-SHA256 of `${svix-id}.${svix-timestamp}.${body}`
 * with the webhook secret (base64-decoded after the `whsec_` prefix). The
 * `svix-signature` header is a space-separated list of `v1,<base64sig>` values
 * — at least one must match in constant time.
 *
 * Anti-replay: in addition to the signature, the `svix-timestamp` (unix seconds)
 * must be within ±`toleranceSeconds` (default 300s) of the server clock. Without
 * this, a captured signed `user.created` webhook would be replayable forever,
 * re-driving tenant/PAT provisioning. A missing/non-numeric timestamp is rejected
 * with the same `false` as a bad signature.
 */
export async function verifySvixSignature(ctx: VerifyContext): Promise<boolean> {
  if (!ctx.secret.startsWith("whsec_")) return false;

  // Anti-replay: reject stale or malformed timestamps before doing any HMAC work.
  const tolerance = ctx.toleranceSeconds ?? SVIX_TIMESTAMP_TOLERANCE_SECONDS;
  const nowSeconds = ctx.nowSeconds ?? Date.now() / 1000;
  const svixTs = Number(ctx.svixTimestamp);
  if (
    ctx.svixTimestamp.trim() === "" ||
    !Number.isFinite(svixTs) ||
    Math.abs(nowSeconds - svixTs) > tolerance
  ) {
    return false;
  }

  const rawSecret = ctx.secret.slice("whsec_".length);
  const secretBytes = Uint8Array.from(atob(rawSecret), (c) => c.charCodeAt(0));
  const key = await crypto.subtle.importKey(
    "raw",
    secretBytes,
    { name: "HMAC", hash: "SHA-256" },
    false,
    ["sign"],
  );
  const toSign = new TextEncoder().encode(
    `${ctx.svixId}.${ctx.svixTimestamp}.${ctx.body}`,
  );
  const sigBytes = new Uint8Array(await crypto.subtle.sign("HMAC", key, toSign));
  const expected = btoa(String.fromCharCode(...sigBytes));

  const candidates = ctx.svixSignature
    .split(" ")
    .map((entry) => entry.split(","))
    .filter((parts) => parts[0] === "v1" && typeof parts[1] === "string")
    .map((parts) => parts[1] as string);

  // Constant-time compare per candidate.
  for (const cand of candidates) {
    if (cand.length !== expected.length) continue;
    let diff = 0;
    for (let i = 0; i < cand.length; i++) {
      diff |= cand.charCodeAt(i) ^ expected.charCodeAt(i);
    }
    if (diff === 0) return true;
  }
  return false;
}

/**
 * Canonical CoreLink data-residency MACRO region codes. These — NOT colo codes —
 * are what `tenant.primary_region` holds (D1 CHECK in migrations 0023/0028). The
 * MUST mirror `worker/src/region-map.ts` MacroRegion + the Rust region_map.rs.
 */
export async function autoProvisionFromClerkEvent(input: {
  event: ClerkUserCreatedEvent;
  colo: string | undefined | null;
  svixId: string;
  api: ApiClient;
  analytics: AnalyticsEmitter;
  /**
   * A1 auto-provision: write the `tenant_org_map` row (`clerk_org_id →
   * tenant_id`) that `resolve-tenant` reads. Called with the resolved principal
   * key (org_id else sub, per {@link orgMapKeyFor}) and the created tenant id.
   *
   * LOAD-BEARING: if this throws, the whole provision throws so the webhook
   * returns non-2xx and Svix retries — a tenant WITHOUT its map row is the exact
   * `org_not_mapped` lockout A1 fixes, so it must NOT be swallowed. Idempotent
   * on retry (`INSERT OR IGNORE`). Absent (dev/CI without CONFIG_DB) → the map
   * write is a no-op, mirroring the other D1 writes in this flow.
   */
  writeOrgMap?: (clerkOrgId: string, tenantId: string) => Promise<void>;
  /**
   * Seed the free-tier entitlement row-family (`tier_selections('free','active')`,
   * `tenant_quota`, `runners_entitlement('free')`) for the created tenant —
   * everything a FULLY-provisioned tenant needs beyond `tenant`/`tenant_org_map`/
   * `pat`. WITHOUT these rows the tenant reports billing `inactive` and the
   * quota/runner gates read empty → degraded dashboard; this CONVERGES the
   * Clerk-signup path with the login-time provisioner
   * `worker/src/lib/githugr_provision.ts` (the canonical row shape).
   *
   * LOAD-BEARING (same posture as {@link writeOrgMap}): if this throws, the whole
   * provision throws → the webhook returns non-2xx and Svix retries. Idempotent
   * on retry (`INSERT OR IGNORE`). Absent (dev/CI without CONFIG_DB) → a no-op,
   * mirroring the other D1 writes in this flow. Invoked BEFORE {@link issuePat}
   * so the handler's tenant+live-PAT idempotency guard implies these rows exist.
   */
  seedEntitlements?: (tenantId: string) => Promise<void>;
}): Promise<AutoProvisionResult> {
  const user = input.event.data;
  const name = tenantSlugFor(user);
  const region = regionFromColo(input.colo);

  // backlog #29: REJECT unprovisioned macro regions (sam/afr today) BEFORE any
  // tenant write. Silently downgrading them to a US region is the residency leak
  // we're closing. This throws BEFORE createTenant, so no orphan row is created;
  // the webhook handler maps it to a terminal 422 (no Svix retry — it is a
  // permanent "region not available", not a transient failure).
  if (!isProvisionedMacro(region)) {
    throw new UnprovisionedRegionError(region);
  }

  // 1. Create tenant with its resolved residency macro region.
  const tenant = await input.api.createTenant(name, user.id, region);
  await input.analytics.emit("signup_completed", tenant.id, user.id, {
    auth_provider:
      user.external_accounts?.[0]?.provider ?? "email",
    svix_id: input.svixId,
  });
  await input.analytics.emit("tenant_created", tenant.id, user.id, {
    name,
    svix_id: input.svixId,
  });

  // 1b. A1 auto-provision: write the tenant_org_map row (clerk_org_id →
  //     tenant_id) that POST /internal/v1/auth/resolve-tenant reads. WITHOUT
  //     this, resolve-tenant 404s `org_not_mapped` for every real new user and
  //     locks them out — so it is part of "provisioning COMPLETE" (tenant AND
  //     PAT AND map), NOT best-effort. The key is the Clerk principal id githugr
  //     scopes a token to: org_id if present, else the user sub (orgMapKeyFor).
  //     If writeOrgMap throws, it propagates out of this function → the webhook
  //     returns non-2xx and Svix retries (idempotent: INSERT OR IGNORE).
  if (input.writeOrgMap) {
    await input.writeOrgMap(orgMapKeyFor(user), tenant.id);
  }

  // 2. Configure region + plan.
  await input.api.configureTenant(tenant.id, region, "free");
  await input.analytics.emit("region_assigned", tenant.id, user.id, {
    region,
    // Honest source: when a real user-geo colo is supplied it is geo-derived;
    // the webhook path supplies none (Svix's PoP ≠ the user), so it is the
    // launch-served default (enam). See the colo note in the webhook handler.
    source: input.colo ? "geo_ip_cf_colo" : "launch_default",
    svix_id: input.svixId,
  });

  // 2b. Seed the free-tier entitlement row-family so the tenant is FULLY
  //     provisioned — NOT just tenant+org_map+pat. Without tier_selections
  //     ('free','active'), tenant_quota, and runners_entitlement('free') the
  //     tenant reports billing `inactive` and the quota/runner gates read empty,
  //     yielding a degraded dashboard. This CONVERGES the Clerk-signup path with
  //     the parallel login-time provisioner worker/src/lib/githugr_provision.ts
  //     (the canonical row shape). Wired only when CONFIG_DB is present (prod);
  //     dev/CI without it is a no-op, like the other D1 writes here. Runs BEFORE
  //     issuePat (step 3) so the handler's tenant+live-PAT idempotency guard
  //     implies these rows already exist — a partial failure here (before the
  //     PAT) leaves no "looks-provisioned-but-isn't" tenant. LOAD-BEARING +
  //     fail-CLOSED: a throw propagates → webhook non-2xx → Svix retries
  //     (idempotent: INSERT OR IGNORE), same posture as writeOrgMap.
  if (input.seedEntitlements) {
    await input.seedEntitlements(tenant.id);
  }

  // 3. Issue PAT (read-write).
  const pat = await input.api.issuePat(tenant.id, "read-write");
  await input.analytics.emit("pat_issued", tenant.id, user.id, {
    pat_id: pat.id,
    scope: "read-write",
    svix_id: input.svixId,
  });

  // 4. Push tenant_id + region into Clerk PUBLIC metadata (legit session
  //    claims) and the one-time PAT plaintext into Clerk PRIVATE metadata
  //    (backend-only — never in the JWT, never client-readable) so /welcome
  //    can reveal it once via a server-side Clerk Backend API read. The PAT
  //    plaintext is cleared from private_metadata two ways (CTRL-CRED-001):
  //    (a) the client-driven /welcome reveal PATCHes it to null, and (b) a
  //    guaranteed server-side scrub cron (pat_scrub_cron.ts) clears it for any
  //    user whose reveal is older than a short TTL — so an un-visited /welcome
  //    cannot leave the secret resident. `pat_revealed_at` (epoch-ms) is the
  //    reveal-age clock the cron reads.
  //
  // Idempotency: tenant + PAT are already persisted in D1 above. If Clerk
  // rejects the metadata patch with a 4xx (404 = user deleted between webhook
  // emit and handler run; 4xx-others = permanent client error), we log and
  // proceed — re-driving the webhook would only INSERT-OR-IGNORE the same
  // tenant/PAT rows and re-attempt the same failing PATCH. 5xx is transient
  // and re-thrown so Svix retries the whole event with backoff.
  let metadataPublished = false;
  try {
    await input.api.publishUserMetadata(
      user.id,
      // public_metadata — legit session claims only (NO secret).
      { tenant_id: tenant.id, region },
      // private_metadata — the one-time secret + its reveal-age clock.
      { pat_plaintext: pat.plaintext, pat_revealed_at: Date.now() },
    );
    metadataPublished = true;
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    const statusMatch = msg.match(/_(\d{3})$/);
    const status = statusMatch ? parseInt(statusMatch[1] as string, 10) : 0;
    if (status >= 500 || status === 0) {
      // Transient / unknown — let Svix retry.
      throw err;
    }
    // 4xx: dead-letter the metadata write, keep tenant+PAT.
    console.log(
      `[clerk-metadata-skipped] status=${status} user_id=${user.id} ` +
      `tenant_id=${tenant.id} svix_id=${input.svixId}`,
    );
  }

  return {
    tenant_id: tenant.id,
    region,
    plan: "free",
    pat_id: pat.id,
    pat_plaintext: pat.plaintext,
    metadata_published: metadataPublished,
  };
}

/**
 * HTTP entry point — call from the Worker `fetch` handler. Returns a
 * Response so callers can compose with other webhook routes.
 */
export async function handleClerkWebhook(
  request: Request,
  env: AutoProvisionEnv,
  apiFactory: (env: AutoProvisionEnv, context?: StagingOwnershipContext | null) => ApiClient,
): Promise<Response> {
  if (request.method !== "POST") {
    return new Response("method_not_allowed", { status: 405 });
  }
  const svixId = request.headers.get("svix-id") ?? "";
  const svixTimestamp = request.headers.get("svix-timestamp") ?? "";
  const svixSignature = request.headers.get("svix-signature") ?? "";
  if (!svixId || !svixTimestamp || !svixSignature) {
    return new Response("missing_svix_headers", { status: 400 });
  }
  const body = await request.text();
  const ok = await verifySvixSignature({
    body,
    svixId,
    svixTimestamp,
    svixSignature,
    secret: env.CLERK_WEBHOOK_SECRET,
  });
  if (!ok) {
    return new Response("invalid_signature", { status: 401 });
  }

  // The container boot gate protects its own email-hash writer, but this
  // separate deployment writes the same pseudonym. Refuse production writes
  // when the shared salt is absent so the six targets cannot silently drift
  // back to reversible unsalted hashes.
  if (isProductionEnvironment(env) && !env.EMAIL_HASH_SALT?.trim()) {
    return new Response("email_hash_salt_unconfigured", { status: 500 });
  }

  let parsed: { type?: string; data?: { id?: string } };
  try {
    parsed = JSON.parse(body) as { type?: string; data?: { id?: string } };
  } catch {
    return new Response("invalid_json", { status: 400 });
  }

  // Validate the provider identity before dispatching either lifecycle arm.
  // Do not let a malformed id reach a tenant lookup, D1 key, or DSR hash.
  if (
    (parsed.type === "user.created" || parsed.type === "user.deleted") &&
    !isValidClerkUserId(parsed.data?.id)
  ) {
    return new Response("invalid_clerk_user_id", { status: 400 });
  }

  // Account deletion → enqueue a GDPR right-to-erasure request (WI-S11-008).
  if (parsed.type === "user.deleted") {
    return handleUserDeleted(parsed as ClerkUserDeletedEvent, env, svixId);
  }
  if (parsed.type !== "user.created") {
    return new Response("ignored", { status: 200 });
  }
  const event = parsed as ClerkUserCreatedEvent;

  // Clerk can emit `user.created` before an address is verified. Provisioning
  // at that point creates a tenant and a usable PAT for an identity that has
  // not proved control of the mailbox (and lets disposable/fake addresses
  // enter the money path). This is deliberately before idempotency reads and
  // before every provisioning side effect. A later verified webhook may retry;
  // the safe outcome here is an acknowledged, non-provisioning response.
  // Do not fall back to the first array element: Clerk's primary id is the
  // authoritative binding, and an absent/mismatched id must fail closed rather
  // than verifying a different address supplied by the event.
  const primaryEmail = event.data.primary_email_address_id
    ? event.data.email_addresses.find(
        (address) => address.id === event.data.primary_email_address_id,
      )
    : undefined;
  if (primaryEmail?.verification?.status !== "verified") {
    return Response.json(
      { ok: false, reason: "email_not_verified" },
      { status: 202 },
    );
  }

  let ownershipContext: StagingOwnershipContext | null;
  try {
    ownershipContext = await signupOwnershipContext(
      request,
      env.ENVIRONMENT,
      env.CORELINK_STAGING_LOAD_TEST_ADMISSION_KEY,
    );
  } catch {
    return new Response("invalid_staging_ownership", { status: 403 });
  }

  // RESIDENCY SOURCE — do NOT derive the tenant region from this webhook's
  // `cf.colo`. A Clerk webhook is delivered by SVIX (server-to-server), so
  // `request.cf.colo` is SVIX's sender PoP, NOT the end-user's location. Deriving
  // residency from it is a category error: e.g. a US user whose webhook Svix routes
  // via a South-American PoP (observed live: `sender-9YMgn` → `sam`) would be
  // assigned `sam` and then REJECTED by PROVISIONED_MACROS (`sam` is recognised but
  // NOT provisionable — PROD_SAM still has no SAM-jurisdiction bucket) — a
  // legitimate paying signup lost to the luck of Svix's routing. The webhook
  // carries no reliable user-geo signal, so webhook-provisioned tenants default to
  // a provisioned region (enam, via regionFromColo's null default; see
  // PROVISIONED_MACROS = {wnam, enam, weur, apac}). Real per-tenant residency selection
  // is a deliberate post-signup action; SAM stays gated until its
  // jurisdiction-correct bucket lands (residency Phase-2). (Svix's sender PoP is
  // still captured in the request logs for diagnostics — it just never drives
  // provisioning.)
  const colo = null;

  // Idempotency: short-circuit ONLY when provisioning is COMPLETE — i.e. the
  // tenant row exists AND a still-live PAT exists for it. Keying idempotency on
  // tenant existence ALONE (finding #18) is a fail-open trap: if a prior attempt
  // committed the tenant row but then the PAT mint (step 3) failed transiently,
  // a Svix redelivery would see the orphan tenant and ack "already provisioned"
  // WITHOUT ever re-issuing the PAT, leaving a paying signup permanently unable
  // to authenticate to the cache, silently. So when the tenant exists but has no
  // live PAT, we FALL THROUGH and re-enter autoProvisionFromClerkEvent — which is
  // idempotent (createTenant INSERT-OR-IGNORE + read-back adopts the existing
  // tenant; issuePat mints a fresh PAT row; publishUserMetadata re-PATCHes Clerk)
  // — so the PAT + publicMetadata are re-issued and only THEN do we ack.
  if (env.CONFIG_DB) {
    try {
      const existing = await env.CONFIG_DB
        .prepare("SELECT tenant_id FROM tenant WHERE clerk_user_id = ?1 LIMIT 1")
        .bind(event.data.id)
        .first<{ tenant_id: string }>();
      if (existing !== null) {
        if (ownershipContext !== null) {
          return new Response("duplicate_staging_signup", { status: 409 });
        }
        // A tenant row is NOT proof of complete provisioning — require a live
        // (non-expired) PAT before treating the signup as done.
        const livePat = await env.CONFIG_DB
          .prepare(
            "SELECT pat_id FROM pat WHERE tenant_id = ?1 AND expires_ms > ?2 LIMIT 1",
          )
          .bind(existing.tenant_id, Date.now())
          .first<{ pat_id: string }>();
        if (livePat !== null) {
          await completeClerkProvision(env.CONFIG_DB, event.data.id);
          return Response.json({ ok: true, tenant_id: existing.tenant_id, idempotent: true });
        }
        // Tenant exists but NO live PAT — a prior attempt died mid-provision.
        // Do NOT ack; fall through to re-issue the PAT + re-publish metadata.
        console.error(
          `[clerk-webhook] tenant ${existing.tenant_id} exists with NO live PAT — ` +
            `re-completing provisioning (user=${event.data.id}, svix=${svixId})`,
        );
      }
    } catch {
      // D1 errors on idempotency check are non-fatal — fall through to provision.
    }
  }

  // FAIL-LOUD before any tenant is created (brutal-audit B4). Provisioning
  // REQUIRES a mint auth key to mint the PAT (step 3 of
  // autoProvisionFromClerkEvent). The container mint gate prefers the DEDICATED
  // CORELINK_PAT_MINT_AUTH_KEY (no shared fallback on the container — DD-HIGH,
  // WP1) and falls back to the shared CORELINK_INTERNAL_AUTH_KEY only when the
  // dedicated is unset (additive) — same resolution as issuePat below, so this
  // pre-flight must accept EITHER or it would 500 when only the dedicated key
  // is bound. CONFIG_DB is a declarative binding (always present once deployed),
  // but the auth key is a `wrangler secret put` value that can be absent
  // independently — the documented launch landmine. If we let provisioning
  // START without it, `createTenant` commits a tenant row and THEN `issuePat`
  // throws; on Svix redelivery the idempotency check ABOVE short-circuits on
  // that orphan tenant and NEVER re-issues the PAT, leaving the user
  // permanently PAT-less. Checking HERE — after the idempotency read, before
  // the first write — makes a missing key 500 with ZERO side effects, so
  // redelivery cleanly re-provisions once the secret is set. (issuePat also
  // throws as a backstop.)
  // Resolve dedicated-if-set, then fail-loud on a blank or sub-floor resolved
  // key too (not just unset). A short dedicated key must not widen back to the
  // shared key; the container's internal-auth contract has a 32-char floor.
  const preflightAuthKey =
    env.CORELINK_PAT_MINT_AUTH_KEY ?? env.CORELINK_INTERNAL_AUTH_KEY;
  if (!preflightAuthKey || preflightAuthKey.length < MIN_INTERNAL_AUTH_KEY_LEN) {
    console.error(
      `[clerk-webhook] CORELINK_PAT_MINT_AUTH_KEY / CORELINK_INTERNAL_AUTH_KEY absent — cannot mint PAT; ` +
        `returning 500 before any tenant write (user=${event.data.id}, svix=${svixId})`,
    );
    return new Response(
      JSON.stringify({ ok: false, error: "internal_auth_key_unconfigured" }),
      { status: 500, headers: { "content-type": "application/json" } },
    );
  }

  // A UNIQUE tenant.clerk_user_id only serializes tenant creation. Claim a
  // durable per-user lease before the PAT mint as well, otherwise two
  // concurrent deliveries can both pass the tenant/live-PAT read and create
  // two live credentials. The key preflight above intentionally runs first so
  // a misconfigured delivery does not consume a five-minute retry lease.
  // A held lease returns 409 so Svix retries; a crashed owner becomes
  // reclaimable after the bounded lease expires.
  if (env.CONFIG_DB) {
    let claimed: boolean;
    try {
      claimed = await claimClerkProvision(env.CONFIG_DB, event.data.id);
    } catch {
      // Missing/unavailable lock storage is a server fault, never permission
      // to continue into a non-idempotent mint.
      return new Response("provisioning_lock_unavailable", { status: 503 });
    }
    if (!claimed) {
      try {
        const existing = await env.CONFIG_DB
          .prepare("SELECT tenant_id FROM tenant WHERE clerk_user_id = ?1 LIMIT 1")
          .bind(event.data.id)
          .first<{ tenant_id: string }>();
        if (existing !== null) {
          const livePat = await env.CONFIG_DB
            .prepare(
              "SELECT pat_id FROM pat WHERE tenant_id = ?1 AND expires_ms > ?2 LIMIT 1",
            )
            .bind(existing.tenant_id, Date.now())
            .first<{ pat_id: string }>();
          if (livePat !== null) {
            return Response.json({ ok: true, tenant_id: existing.tenant_id, idempotent: true });
          }
        }
      } catch {
        return new Response("provisioning_lock_unavailable", { status: 503 });
      }
      return new Response("provisioning_in_progress", { status: 409 });
    }
  }

  try {
    // A1 auto-provision: the real tenant_org_map writer. Bound only when
    // CONFIG_DB is present (prod/deployed); in dev/CI without the binding it is
    // undefined and the map write is a no-op (like the other D1 writes here).
    // A D1 failure here PROPAGATES (insertTenantOrgMap does not swallow) → the
    // catch below maps it to 500 and Svix retries; the map row is load-bearing.
    const configDb = env.CONFIG_DB;
    const writeOrgMap = configDb
      ? async (clerkOrgId: string, tenantId: string): Promise<void> => {
          const nowMs = Date.now();
          const params = {
            clerkOrgId,
            tenantId,
            nowMs,
          };
          if (ownershipContext === null) {
            await insertTenantOrgMap(configDb, params);
          } else {
            await writeSignupArtifactBatch(
              configDb,
              ownershipContext,
              `${tenantId}:org-map`,
              [insertTenantOrgMapStatement(configDb, params)],
              nowMs,
            );
          }
        }
      : undefined;

    // Same CONFIG_DB-gated pattern as writeOrgMap: seed the free-tier
    // entitlement row-family (tier_selections/tenant_quota/runners_entitlement)
    // so the tenant is FULLY provisioned (billing `active`, non-empty quota +
    // runner gates), converging with worker/src/lib/githugr_provision.ts. A D1
    // failure PROPAGATES (seedTenantEntitlements does not swallow) → the catch
    // below maps it to 500 and Svix retries; these rows are load-bearing.
    const seedEntitlements = configDb
      ? async (tenantId: string): Promise<void> => {
          const nowMs = Date.now();
          const params = {
            tenantId,
            nowMs,
          };
          if (ownershipContext === null) {
            await seedTenantEntitlements(configDb, params);
          } else {
            await writeSignupArtifactBatch(
              configDb,
              ownershipContext,
              `${tenantId}:entitlements`,
              seedTenantEntitlementStatements(configDb, params),
              nowMs,
            );
          }
        }
      : undefined;

    const result = await autoProvisionFromClerkEvent({
      event,
      colo,
      svixId,
      api: apiFactory(env, ownershipContext),
      analytics: d1AnalyticsEmitter(env.ANALYTICS_DB),
      writeOrgMap,
      seedEntitlements,
    });

    if (env.CONFIG_DB) {
      await completeClerkProvision(env.CONFIG_DB, event.data.id);
    }

    return Response.json({
      ok: true,
      tenant_id: result.tenant_id,
      metadata_published: result.metadata_published,
    });
  } catch (err) {
    // backlog #29: an unprovisioned residency region is a TERMINAL, permanent
    // condition — NOT a transient failure. Return 422 (no Svix retry) so the
    // tenant is cleanly rejected rather than redelivered forever. No tenant row
    // was written (the throw is BEFORE createTenant).
    if (err instanceof UnprovisionedRegionError) {
      console.error(
        `[clerk-webhook] rejecting signup: region '${err.region}' not provisioned ` +
          `(user=${event.data.id}, svix=${svixId})`,
      );
      return new Response(
        JSON.stringify({ ok: false, error: "region_not_provisioned", region: err.region }),
        { status: 422, headers: { "content-type": "application/json" } },
      );
    }
    // Webhook returns 500 so Svix retries with backoff. Do not leak error
    // body — log a redacted summary upstream.
    return new Response(
      JSON.stringify({ ok: false, error: (err as Error).message.slice(0, 200) }),
      { status: 500, headers: { "content-type": "application/json" } },
    );
  }
}

/**
 * Production API client: provisions tenant + mints PAT via direct D1 writes
 * and the container's `/_internal/pat/mint` route, then updates Clerk metadata
 * via the Clerk Backend API.
 *
 * Architecture (Option A per the Stream-5 charter):
 *   1. `createTenant` → INSERT into D1 CONFIG_DB (tenant row with clerk_user_id).
 *   2. `configureTenant` → no-op (region is embedded in the tenant row at INSERT).
 *   3. `issuePat` → POST `/_internal/pat/mint` on container, INSERT pat row to D1.
 *   4. `publishUserMetadata` → PATCH Clerk Backend API `/v1/users/{userId}`.
 *
 * When CONFIG_DB, CORELINK_INTERNAL_AUTH_KEY, or CLERK_SECRET_KEY are missing,
 * each method falls back to a no-op stub that returns a fake result so the
 * webhook handler stays alive in dev/CI without real secrets.
 *
 * Hard rules:
 *   - `token_plaintext` from the internal mint response is NEVER logged.
 *   - All D1 queries are parameterized.
 *   - Internal auth header uses the CORELINK_INTERNAL_AUTH_KEY binding.
 */
export function defaultApiClient(
  env: AutoProvisionEnv,
  ownershipContext: StagingOwnershipContext | null = null,
): ApiClient {
  const nowMs = Date.now();
  // Per-year TTL for auto-provisioned PATs (365 days).
  const patTtlSeconds = 365 * 24 * 60 * 60;

  return {
    // ── createTenant ──────────────────────────────────────────────────────────
    // Generates a UUIDv7 tenant_id, inserts the tenant row to D1 CONFIG_DB with
    // the geo-derived residency MACRO region (backlog #29 — NO LONGER hardcoded
    // to 'enam'). Returns { id: tenant_id }.
    async createTenant(
      name: string,
      ownerUserId: string,
      region: MacroRegion,
    ): Promise<{ id: string }> {
      const tenantId = crypto.randomUUID();
      if (!env.CONFIG_DB) {
        if (ownershipContext !== null) {
          throw new Error("staging signup D1 is unavailable");
        }
        // Dev/CI without D1 binding — return a stable fake.
        return { id: tenantId };
      }
      // Hash the Clerk user id as the email_hash surrogate (no email stored).
      const emailHashBuf = await crypto.subtle.digest(
        "SHA-256",
        new TextEncoder().encode(ownerUserId),
      );
      const emailHash = Array.from(new Uint8Array(emailHashBuf))
        .map((b) => b.toString(16).padStart(2, "0"))
        .join("");

      // backlog #29: use the PARAMETERIZED insertTenant helper (it threads the
      // chosen `primary_region` instead of the old hardcoded 'enam' inline
      // INSERT). insertTenant is INSERT OR IGNORE — same race-safety as before.
      const tenantParams = {
        tenantId,
        primaryRegion: region,
        tenantSlug: name,
        emailHash,
        clerkUserId: ownerUserId,
        nowMs,
      };
      if (ownershipContext === null) {
        await insertTenant(env.CONFIG_DB, tenantParams);
      } else {
        const prior = await env.CONFIG_DB
          .prepare("SELECT tenant_id FROM tenant WHERE clerk_user_id = ?1 LIMIT 1")
          .bind(ownerUserId)
          .first<{ tenant_id: string }>();
        if (prior !== null) throw new Error("duplicate staging signup rejected");
        await writeSignupArtifactBatch(
          env.CONFIG_DB,
          ownershipContext,
          `${tenantId}:tenant`,
          [insertTenantStatement(env.CONFIG_DB, tenantParams)],
          nowMs,
        );
      }

      // Read back the WINNING tenant by clerk_user_id. Under concurrent
      // duplicate Clerk delivery, INSERT OR IGNORE may have skipped our row
      // (the UNIQUE clerk_user_id index, migration 0056); the durable tenant is
      // whichever insert won. Adopt it so the PAT + Clerk metadata bind to the
      // REAL tenant, never an orphan (D1 FKs are off in Workers — this read-back
      // is the only guard).
      const winner = await env.CONFIG_DB.prepare(
        "SELECT tenant_id FROM tenant WHERE clerk_user_id = ?1 LIMIT 1",
      )
        .bind(ownerUserId)
        .first<{ tenant_id: string }>();
      return { id: winner?.tenant_id ?? tenantId };
    },

    // ── configureTenant ───────────────────────────────────────────────────────
    // The region is now persisted in the tenant row at INSERT time by
    // createTenant (backlog #29 — the geo-derived macro, not a hardcoded 'enam').
    // This stays a no-op: primary_region is IMMUTABLE post-INSERT (migration 0028
    // trigger), so there is nothing for configureTenant to update. The
    // region_assigned analytics event is still emitted by the orchestrator.
    async configureTenant(
      _tenantId: string,
      _region: string,
      _plan: "free",
    ): Promise<void> {
      // no-op: tenant row already written in createTenant.
    },

    // ── issuePat ──────────────────────────────────────────────────────────────
    // Calls `/_internal/pat/mint` with the caller-requested `scope` (self-serve
    // PATs are `read-write`, never `admin`), inserts the PAT row to D1 with that
    // same scope, and returns the plaintext.
    async issuePat(
      tenantId: string,
      scope: "read-write",
    ): Promise<{ id: string; plaintext: string }> {
      // Prefer the DEDICATED CORELINK_PAT_MINT_AUTH_KEY — the container's
      // /_internal/pat/mint gate REQUIRES it with NO shared fallback (DD-HIGH,
      // WP1) — falling back to the shared key only when the dedicated is unset
      // (additive; once provisioned the shared no longer authorizes mint).
      // Same posture as the 4 main-Worker callers (commit c88508f1).
      const internalAuthKey =
        env.CORELINK_PAT_MINT_AUTH_KEY ?? env.CORELINK_INTERNAL_AUTH_KEY;
      // Fail-loud on an absent or sub-floor resolved key — the container's
      // internal-auth contract requires at least 32 characters. A dedicated
      // short key also blocks shared fallback, preserving isolation.
      if (!internalAuthKey || internalAuthKey.length < MIN_INTERNAL_AUTH_KEY_LEN) {
        // FAIL-LOUD (signup money path): without the internal auth key we
        // CANNOT mint a real PAT. The old behavior returned a fake
        // "corelink_pat_DEVSTUB" that looked valid to the user but
        // authenticated NOTHING, while the Clerk webhook still returned 200 —
        // so Svix never retried and the new user was permanently broken with
        // no signal. Throw instead: the webhook handler maps this to a 500,
        // Svix redelivers, and the signup self-heals once the secret is set.
        throw new Error(
          "CORELINK_PAT_MINT_AUTH_KEY / CORELINK_INTERNAL_AUTH_KEY is not configured — refusing to issue a stub PAT",
        );
      }

      // Call the container's internal mint endpoint. Prefer the service
      // binding (bypasses CF edge error 1014); fall back to the public URL
      // when the binding is absent (local dev).
      const mintReq = new Request(
        `${env.CORELINK_API_BASE}/_internal/pat/mint`,
        {
          method: "POST",
          headers: {
            "content-type": "application/json",
            "x-corelink-internal-auth": internalAuthKey,
          },
          body: JSON.stringify({
            tenant_id: tenantId,
            // Use the tenant_id as the principal_id for the first PAT.
            principal_id: tenantId,
            // Honor the caller-requested scope (self-serve PATs are read-write,
            // NEVER admin). The container mint route authorizes per-scope.
            scopes: scope,
            ttl_seconds: patTtlSeconds,
          }),
        },
      );
      const mintResp = env.CORELINK_API_SVC
        ? await env.CORELINK_API_SVC.fetch(mintReq)
        : await fetch(mintReq);
      if (!mintResp.ok) {
        throw new Error(`internal_pat_mint_failed_${mintResp.status}`);
      }
      const mint = (await mintResp.json()) as {
        token_plaintext: string;
        pat_id: string;
        token_id: string;
        expires_ms: number;
        hash: string;
      };

      // Insert the PAT row to D1 CONFIG_DB.
      if (env.CONFIG_DB) {
        const patStatement = insertPatStatement(env.CONFIG_DB, {
          patId: mint.pat_id,
          tenantId,
          patHash: mint.hash,
          scope, // persist the caller-requested scope (read-write), NOT admin
          expiresMs: mint.expires_ms,
          tokenId: mint.token_id,
          shownOnceToken: crypto.randomUUID(), // shown_once_token (pre-consumed)
          nowMs,
        });
        if (ownershipContext === null) {
          await patStatement.run();
        } else {
          await writeSignupArtifactBatch(
            env.CONFIG_DB,
            ownershipContext,
            `${tenantId}:pat:${mint.pat_id}`,
            [patStatement],
            nowMs,
          );
        }
      }

      // Return plaintext — NEVER log this value.
      return { id: mint.pat_id, plaintext: mint.token_plaintext };
    },

    // ── publishUserMetadata ───────────────────────────────────────────────────
    // Writes tenant_id + region to Clerk PUBLIC metadata (legit session claims
    // that ride the JWT) and pat_plaintext to Clerk PRIVATE metadata
    // (backend-only — never in the JWT, never client-readable). The public
    // claims become available on the user's NEXT session refresh; the private
    // secret is read server-side by /welcome (Clerk Backend API) and cleared by
    // the /welcome reveal + the scrub cron (CTRL-CRED-001).
    async publishUserMetadata(
      userId: string,
      publicMetadata: Record<string, unknown>,
      privateMetadata: Record<string, unknown>,
    ): Promise<void> {
      if (!env.CLERK_SECRET_KEY) {
        // Dev/CI without Clerk secret — silently skip.
        return;
      }
      const resp = await fetch(
        `https://api.clerk.com/v1/users/${encodeURIComponent(userId)}`,
        {
          method: "PATCH",
          headers: {
            authorization: `Bearer ${env.CLERK_SECRET_KEY}`,
            "content-type": "application/json",
          },
          // NEVER log this body — `private_metadata.pat_plaintext` is a secret.
          body: JSON.stringify({
            public_metadata: publicMetadata,
            private_metadata: privateMetadata,
          }),
        },
      );
      if (!resp.ok) {
        throw new Error(`clerk_metadata_update_failed_${resp.status}`);
      }
    },
  };
}
