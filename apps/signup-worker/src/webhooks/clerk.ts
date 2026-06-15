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
 * Idempotency: Clerk re-fires webhooks on transient failure. We treat the
 * webhook's `svix-id` header as the idempotency key — if a row already
 * exists in `analytics_events` with `properties.svix_id` matching, the
 * handler exits 200 without re-provisioning.
 */

import { insertTenant } from "../lib/d1.js";

export interface ClerkUserCreatedEvent {
  type: "user.created";
  data: {
    id: string;
    email_addresses: Array<{ id: string; email_address: string }>;
    primary_email_address_id?: string | null;
    external_accounts?: Array<{
      provider: string;
      username?: string | null;
    }>;
    username?: string | null;
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
}

/** Clerk `user.deleted` webhook payload (account-deletion → GDPR erasure). */
export interface ClerkUserDeletedEvent {
  type: "user.deleted";
  data: { id: string; deleted?: boolean };
}

/**
 * Frozen wire contract for the DSR erasure queue message (`dsr.queued.v1`).
 * The Rust queue consumer (WI-S11-008) deserializes this into the
 * orchestrator's `ErasureRequest`. `erasure_salt_hex` is 64 hex chars (32
 * bytes); `subject_id == tenant_id` because CoreLink provisions exactly one
 * tenant per Clerk user, so the tenant IS the unit of deletion.
 */
export interface DsrQueuedV1 {
  schema: "dev.hugr.corelink.dsr.queued.v1";
  dsr_id: string;
  tenant_id: string;
  subject_id: string;
  erasure_salt_hex: string;
  queued_at_ms: number;
  legal_hold: boolean;
  source: "clerk.user.deleted";
  clerk_user_id: string;
}

function bytesToHex(buf: ArrayBuffer): string {
  return Array.from(new Uint8Array(buf))
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}

/**
 * Deterministic name-based (v5-shaped) UUID from a Clerk user id. STABLE across
 * Svix redeliveries, so the same account deletion always maps to ONE `dsr_id`
 * and the erasure orchestrator (which dedups per `(dsr_id, backend)`) is
 * idempotent — a redelivered `user.deleted` never double-runs erasure.
 */
export async function deterministicDsrId(clerkUserId: string): Promise<string> {
  const digest = await crypto.subtle.digest(
    "SHA-256",
    new TextEncoder().encode(`corelink-dsr-v1:${clerkUserId}`),
  );
  const b = new Uint8Array(digest).slice(0, 16);
  b[6] = ((b[6] ?? 0) & 0x0f) | 0x50; // version 5 (name-based)
  b[8] = ((b[8] ?? 0) & 0x3f) | 0x80; // RFC 4122 variant
  const h = bytesToHex(b.buffer);
  return `${h.slice(0, 8)}-${h.slice(8, 12)}-${h.slice(12, 16)}-${h.slice(16, 20)}-${h.slice(20)}`;
}

/**
 * 32-byte erasure salt (hex). `HMAC-SHA256(ERASURE_SALT_KEY, dsr_id)` when the
 * key is set (secret → unlinkable pseudonymization per GDPR Art. 4(5)).
 *
 * Fail-closed in prod (F9, CAA-360 2026-06-13): when `key` is absent and
 * `environment` starts with `"prod"`, throws an error so the caller returns 500
 * and Svix retries the erasure — the right-to-erasure obligation stays alive
 * while the operator misconfiguration is corrected.
 *
 * In non-prod environments (dev/CI) the deterministic `SHA-256("erasure-salt:"
 * + dsr_id)` fallback is still used so tests run without secrets; it is NOT
 * secret and MUST NOT reach production.
 */
export async function deriveErasureSalt(
  dsrId: string,
  key: string | undefined,
  environment?: string,
): Promise<string> {
  if (key && key.length > 0) {
    const k = await crypto.subtle.importKey(
      "raw",
      new TextEncoder().encode(key),
      { name: "HMAC", hash: "SHA-256" },
      false,
      ["sign"],
    );
    const sig = await crypto.subtle.sign("HMAC", k, new TextEncoder().encode(dsrId));
    return bytesToHex(sig);
  }
  // ERASURE_SALT_KEY absent — fail CLOSED in prod (the predictable fallback
  // breaks GDPR pseudonymization unlinkability).
  if (environment && environment.startsWith("prod")) {
    throw new Error(
      "ERASURE_SALT_KEY is not configured — refusing to derive a predictable erasure salt in prod (fail-CLOSED)",
    );
  }
  // Non-prod fallback: deterministic but NOT secret. Never reaches production.
  const d = await crypto.subtle.digest(
    "SHA-256",
    new TextEncoder().encode(`erasure-salt:${dsrId}`),
  );
  return bytesToHex(d);
}

/**
 * Build the `dsr.queued.v1` erasure message for a deleted Clerk user. Pure
 * given its inputs (deterministic `dsr_id` + salt) → fully testable + idempotent.
 *
 * Throws when `saltKey` is absent and `environment` starts with `"prod"` (F9,
 * CAA-360 2026-06-13) — the caller must surface this as a 500 for Svix retry.
 */
export async function buildErasureQueueMessage(input: {
  clerkUserId: string;
  tenantId: string;
  nowMs: number;
  saltKey: string | undefined;
  environment?: string;
}): Promise<DsrQueuedV1> {
  const dsrId = await deterministicDsrId(input.clerkUserId);
  const saltHex = await deriveErasureSalt(dsrId, input.saltKey, input.environment);
  return {
    schema: "dev.hugr.corelink.dsr.queued.v1",
    dsr_id: dsrId,
    tenant_id: input.tenantId,
    subject_id: input.tenantId, // 1 Clerk user : 1 tenant — tenant is the deletion unit
    erasure_salt_hex: saltHex,
    queued_at_ms: input.nowMs,
    legal_hold: false,
    source: "clerk.user.deleted",
    clerk_user_id: input.clerkUserId,
  };
}

/**
 * Handle a verified Clerk `user.deleted` event: look up the tenant and enqueue
 * a GDPR erasure request. FAIL-LOUD (500 → Svix retries) when a tenant exists
 * but the queue binding is absent, so a right-to-erasure obligation is never
 * silently dropped. No tenant (deleted pre-provision or already erased) → 200 no-op.
 */
export async function handleUserDeleted(
  event: ClerkUserDeletedEvent,
  env: AutoProvisionEnv,
  svixId: string,
): Promise<Response> {
  const clerkUserId = event.data?.id;
  if (!clerkUserId) {
    return Response.json({ ok: true, erasure_enqueued: false, reason: "no_user_id" });
  }

  let tenantId: string | null = null;
  if (env.CONFIG_DB) {
    try {
      const row = await env.CONFIG_DB.prepare(
        "SELECT tenant_id FROM tenant WHERE clerk_user_id = ?1 LIMIT 1",
      )
        .bind(clerkUserId)
        .first<{ tenant_id: string }>();
      tenantId = row?.tenant_id ?? null;
    } catch {
      // D1 error — 500 so Svix retries. We must NOT silently drop a deletion.
      return new Response(
        JSON.stringify({ ok: false, error: "tenant_lookup_failed" }),
        { status: 500, headers: { "content-type": "application/json" } },
      );
    }
  }

  if (!tenantId) {
    // No provisioned tenant — nothing to erase. Ack so Svix stops retrying.
    return Response.json({ ok: true, erasure_enqueued: false, reason: "no_tenant" });
  }

  if (!env.DSR_QUEUE) {
    console.error(
      `[clerk-webhook] user.deleted tenant=${tenantId} but DSR_QUEUE unbound — ` +
        `cannot honor erasure; returning 500 for Svix retry (svix=${svixId})`,
    );
    return new Response(
      JSON.stringify({ ok: false, error: "dsr_queue_unconfigured" }),
      { status: 500, headers: { "content-type": "application/json" } },
    );
  }

  let msg: DsrQueuedV1;
  try {
    msg = await buildErasureQueueMessage({
      clerkUserId,
      tenantId,
      nowMs: Date.now(),
      saltKey: env.ERASURE_SALT_KEY,
      environment: env.ENVIRONMENT,
    });
  } catch (saltErr) {
    // F9 (CAA-360 2026-06-13): ERASURE_SALT_KEY absent in prod → fail CLOSED.
    // Return 500 so Svix retries — the erasure obligation stays alive until
    // the operator provisions the secret.
    console.error(
      `[clerk-webhook] erasure salt derivation failed for tenant=${tenantId} svix=${svixId}: ${String(saltErr)}`,
    );
    return new Response(
      JSON.stringify({ ok: false, error: "erasure_salt_key_unconfigured" }),
      { status: 500, headers: { "content-type": "application/json" } },
    );
  }
  // G4 (WI-S11-008): write a durable "DSR requested" anchor BEFORE enqueue so the
  // 24h verify sweep can detect an SLA breach even when the erasure fails before
  // ANY backend tombstone lands (audit-fail-closed → no dsr_erasure_log row).
  // Idempotent: dsr_id is the deterministic name-based UUID, so a Svix
  // redelivery is an INSERT-OR-IGNORE no-op. Best-effort: a D1 failure here must
  // NOT block the enqueue (the queue + orchestrator + audit chain are the
  // primary obligation path); the sweep degrades to its dsr_erasure_log source.
  if (env.CONFIG_DB) {
    try {
      await env.CONFIG_DB.prepare(
        "INSERT OR IGNORE INTO dsr_requested (dsr_id, tenant_id, requested_at, status) " +
          "VALUES (?1, ?2, ?3, 'requested')",
      )
        .bind(msg.dsr_id, tenantId, msg.queued_at_ms)
        .run();
    } catch (reqErr) {
      console.error(
        `[clerk-webhook] dsr_requested write failed dsr_id=${msg.dsr_id} svix=${svixId}: ${String(reqErr)}`,
      );
    }
  }

  await env.DSR_QUEUE.send(msg);
  // dsr_id is a pseudonymous id; tenant_id is not secret. erasure_salt is NEVER logged.
  console.log(
    `[clerk-webhook] erasure enqueued dsr_id=${msg.dsr_id} tenant=${tenantId} svix=${svixId}`,
  );
  return Response.json({
    ok: true,
    erasure_enqueued: true,
    dsr_id: msg.dsr_id,
    tenant_id: tenantId,
  });
}

export interface AutoProvisionResult {
  tenant_id: string;
  region: string;
  plan: "free";
  pat_id: string;
  pat_plaintext: string;
  /**
   * Whether Clerk publicMetadata was successfully patched with
   * `{ tenant_id, region, pat_plaintext }`. False means the Clerk PATCH
   * returned a 4xx (typically 404 — user deleted between webhook emit and
   * handler run). Tenant + PAT rows are still persisted in D1; the metadata
   * gap can be reconciled out-of-band. 5xx Clerk failures are re-thrown so
   * Svix retries the whole webhook.
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
export type MacroRegion = "wnam" | "enam" | "weur" | "sam" | "apac" | "afr";

/**
 * Macro regions whose serving infra is actually DEPLOYED; signup MUST reject the
 * rest with a terminal 422 (never silently downgrade to US — backlog #29).
 *
 * Launch (Phase 1) = US-only. Only `wnam`/`enam` map to a colo (`iad`) that has a
 * live DO + container + R2 bucket. The other canonical macros (`weur`→lhr,
 * `sam`→sam, `apac`→nrt, `afr`) have NO regional Worker `[[services]]` binding and
 * NO per-region CAS/AC bucket yet, so the residency guard would (correctly,
 * fail-closed) 503 every request from such a tenant. Provisioning a region we
 * cannot serve onboards customers straight into a 503 wall — so signup rejects
 * them up front instead. Re-add a macro here ONLY once its regional Worker
 * binding + per-region bucket + container region var are deployed (the EU/SAM
 * serving build-out — tracked as the residency Phase-2 follow-up).
 */
export const PROVISIONED_MACROS: ReadonlySet<MacroRegion> = new Set<MacroRegion>([
  "wnam",
  "enam",
]);

/**
 * Thrown when a tenant's geo-derived macro region is a VALID canonical region
 * but is NOT provisioned in Phase 1 (apac/afr today). Caught by the webhook
 * handler and mapped to a TERMINAL 422 (no Svix retry) — signup MUST reject the
 * tenant rather than silently downgrade them to a US region (backlog #29).
 */
export class UnprovisionedRegionError extends Error {
  readonly region: string;
  constructor(region: string) {
    super(`data-residency region '${region}' is not provisioned`);
    this.name = "UnprovisionedRegionError";
    this.region = region;
  }
}

/**
 * Map a CF colo code (the closest PoP to the end-user, e.g. "FRA", "GRU",
 * "NRT") to a canonical MACRO residency region. The mapping is geo-coarse:
 * European colos → weur, South-American → sam, Asia-Pacific → apac, everything
 * else (incl. unknown/absent) → enam (the genuine US-east default).
 *
 * NOTE: this returns the macro region the tenant SHOULD be assigned. Whether
 * that region is actually servable is a SEPARATE check (`PROVISIONED_MACROS`) —
 * an unprovisioned macro is REJECTED at signup, never silently downgraded.
 */
export function regionFromColo(colo: string | undefined | null): MacroRegion {
  if (!colo || typeof colo !== "string") return "enam";
  const c = colo.trim().toUpperCase();
  if (c.length === 0) return "enam";
  // Western-European colos → weur (the EU residency region; closing the leak).
  const WEUR = new Set([
    "LHR", "LCY", "MAN", "EDI", // UK + Ireland-adjacent
    "DUB",
    "FRA", "MUC", "DUS", "HAM", "STR", "TXL", "BER", // Germany
    "CDG", "MRS", "LYS", // France
    "AMS", "BRU", "ARN", "CPH", "HEL", "OSL", "VIE", "ZRH", "GVA",
    "MAD", "BCD", "BCN", "LIS", "MXP", "FCO", "PMO", "WAW", "PRG", "BUD",
  ]);
  // South-American colos → sam.
  const SAM = new Set([
    "GRU", "GIG", "BSB", "POA", "FOR", "REC", "CWB", "CNF", // Brazil
    "EZE", "SCL", "BOG", "LIM", "UIO", "MDE", "MVD", "ASU",
  ]);
  // Asia-Pacific colos → apac (valid macro but NOT provisioned in Phase 1).
  const APAC = new Set([
    "NRT", "KIX", "ITM", "HND", // Japan
    "ICN", "TPE", "HKG", "SIN", "KUL", "BKK", "CGK", "MNL",
    "BOM", "DEL", "MAA", "BLR", "HYD", "CCU",
    "SYD", "MEL", "PER", "BNE", "AKL", // Oceania
  ]);
  if (WEUR.has(c)) return "weur";
  if (SAM.has(c)) return "sam";
  if (APAC.has(c)) return "apac";
  // North-American + African + Middle-Eastern + anything unknown → enam
  // (the genuine US-east default for unspecified/unmapped geos).
  return "enam";
}

/** True iff the macro region is provisioned in Phase 1 (signup-acceptable). */
export function isProvisionedMacro(region: string): region is MacroRegion {
  return (PROVISIONED_MACROS as ReadonlySet<string>).has(region);
}

/**
 * Derive a tenant slug from a Clerk user payload.
 *
 *   1. Prefer GitHub external-account username (`@github` provider).
 *   2. Else use the local-part of the primary email.
 *   3. Append `-default` and lower-case the whole thing.
 *   4. Strip anything not [a-z0-9-] to satisfy the tenant validator.
 */
export function tenantSlugFor(user: ClerkUserCreatedEvent["data"]): string {
  const github = user.external_accounts?.find(
    (e) => e.provider === "oauth_github" || e.provider === "github",
  );
  const primary = user.primary_email_address_id
    ? user.email_addresses.find((e) => e.id === user.primary_email_address_id)
    : user.email_addresses[0];
  const emailLocal = primary?.email_address.split("@")[0];
  const seed = github?.username ?? user.username ?? emailLocal ?? user.id;
  const cleaned = seed
    .toLowerCase()
    .replace(/[^a-z0-9-]+/g, "-")
    .replace(/-+/g, "-")
    .replace(/^-|-$/g, "");
  // Tenant name validator (admin-ui `validators.ts`) requires 3–64 chars.
  const safe = cleaned.length >= 3 ? cleaned : `user-${user.id.slice(0, 12)}`;
  return `${safe}-default`.slice(0, 64);
}

interface ApiClient {
  /**
   * Insert the tenant row with its data-residency MACRO region (backlog #29).
   * `region` is the geo-derived, provisioned macro (wnam/enam/weur/sam) — the
   * caller has already rejected unprovisioned macros. The region is persisted as
   * `tenant.primary_region` (NO LONGER hardcoded to 'enam').
   */
  createTenant(
    name: string,
    ownerUserId: string,
    region: MacroRegion,
  ): Promise<{ id: string }>;
  configureTenant(
    tenantId: string,
    region: string,
    plan: "free",
  ): Promise<void>;
  issuePat(
    tenantId: string,
    scope: "read-write",
  ): Promise<{ id: string; plaintext: string }>;
  publishUserMetadata(
    userId: string,
    metadata: Record<string, unknown>,
  ): Promise<void>;
}

interface AnalyticsEmitter {
  emit(
    eventName: string,
    tenantId: string | null,
    userId: string | null,
    properties: Record<string, unknown>,
  ): Promise<void>;
}

/**
 * Default analytics emitter backed by the D1 binding owned by the
 * Phase-0 analytics-worker. No-op when the binding is absent so the
 * provisioning path stays alive in environments where agent G's worker
 * hasn't shipped yet.
 */
export function d1AnalyticsEmitter(
  db: D1Database | undefined,
): AnalyticsEmitter {
  return {
    async emit(
      eventName: string,
      tenantId: string | null,
      userId: string | null,
      properties: Record<string, unknown>,
    ): Promise<void> {
      if (!db) return;
      const id = crypto.randomUUID();
      await db
        .prepare(
          "INSERT INTO analytics_events " +
            "(id, event_name, tenant_id, user_id, session_id, properties, created_at) " +
            "VALUES (?1, ?2, ?3, ?4, NULL, ?5, ?6)",
        )
        .bind(
          id,
          eventName,
          tenantId,
          userId,
          JSON.stringify(properties),
          new Date().toISOString(),
        )
        .run();
    },
  };
}

/**
 * Pure provisioning orchestration. Tests inject `api` + `analytics` so the
 * unit tests run without HTTP or D1.
 */
export async function autoProvisionFromClerkEvent(input: {
  event: ClerkUserCreatedEvent;
  colo: string | undefined | null;
  svixId: string;
  api: ApiClient;
  analytics: AnalyticsEmitter;
}): Promise<AutoProvisionResult> {
  const user = input.event.data;
  const name = tenantSlugFor(user);
  const region = regionFromColo(input.colo);

  // backlog #29: REJECT unprovisioned macro regions (apac/afr today) BEFORE any
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

  // 2. Configure region + plan.
  await input.api.configureTenant(tenant.id, region, "free");
  await input.analytics.emit("region_assigned", tenant.id, user.id, {
    region,
    source: "geo_ip_cf_colo",
    svix_id: input.svixId,
  });

  // 3. Issue PAT (read-write).
  const pat = await input.api.issuePat(tenant.id, "read-write");
  await input.analytics.emit("pat_issued", tenant.id, user.id, {
    pat_id: pat.id,
    scope: "read-write",
    svix_id: input.svixId,
  });

  // 4. Push tenant_id + region + one-time PAT into Clerk session metadata so
  //    /welcome can render it without round-tripping the API. PAT plaintext
  //    is removed from publicMetadata by a follow-up scheduled action after
  //    the user's first session — for Phase-0 it lives there until they
  //    log out (acceptable per CTRL-CRED-001 because (a) it's shown once,
  //    (b) Clerk metadata is encrypted at rest, (c) admin-ui never logs it).
  //
  // Idempotency: tenant + PAT are already persisted in D1 above. If Clerk
  // rejects the metadata patch with a 4xx (404 = user deleted between webhook
  // emit and handler run; 4xx-others = permanent client error), we log and
  // proceed — re-driving the webhook would only INSERT-OR-IGNORE the same
  // tenant/PAT rows and re-attempt the same failing PATCH. 5xx is transient
  // and re-thrown so Svix retries the whole event with backoff.
  let metadataPublished = false;
  try {
    await input.api.publishUserMetadata(user.id, {
      tenant_id: tenant.id,
      region,
      pat_plaintext: pat.plaintext,
    });
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
  apiFactory: (env: AutoProvisionEnv) => ApiClient,
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

  let parsed: { type?: string; data?: { id?: string } };
  try {
    parsed = JSON.parse(body) as { type?: string; data?: { id?: string } };
  } catch {
    return new Response("invalid_json", { status: 400 });
  }

  // Account deletion → enqueue a GDPR right-to-erasure request (WI-S11-008).
  if (parsed.type === "user.deleted") {
    return handleUserDeleted(parsed as ClerkUserDeletedEvent, env, svixId);
  }
  if (parsed.type !== "user.created") {
    return new Response("ignored", { status: 200 });
  }
  const event = parsed as ClerkUserCreatedEvent;

  const colo =
    (request as Request & { cf?: { colo?: string } }).cf?.colo ?? null;

  // Idempotency: if we've already provisioned a tenant for this Clerk user,
  // return the cached tenant_id without re-provisioning. Checks D1 CONFIG_DB
  // by clerk_user_id (migration 0055).
  if (env.CONFIG_DB) {
    try {
      const existing = await env.CONFIG_DB
        .prepare("SELECT tenant_id FROM tenant WHERE clerk_user_id = ?1 LIMIT 1")
        .bind(event.data.id)
        .first<{ tenant_id: string }>();
      if (existing !== null) {
        return Response.json({ ok: true, tenant_id: existing.tenant_id, idempotent: true });
      }
    } catch {
      // D1 errors on idempotency check are non-fatal — fall through to provision.
    }
  }

  // FAIL-LOUD before any tenant is created (brutal-audit B4). Provisioning
  // REQUIRES CORELINK_INTERNAL_AUTH_KEY to mint the PAT (step 3 of
  // autoProvisionFromClerkEvent). CONFIG_DB is a declarative binding (always
  // present once deployed), but the auth key is a `wrangler secret put` value
  // that can be absent independently — the documented launch landmine. If we
  // let provisioning START without it, `createTenant` commits a tenant row and
  // THEN `issuePat` throws; on Svix redelivery the idempotency check ABOVE
  // short-circuits on that orphan tenant and NEVER re-issues the PAT, leaving
  // the user permanently PAT-less. Checking HERE — after the idempotency read,
  // before the first write — makes a missing key 500 with ZERO side effects,
  // so redelivery cleanly re-provisions once the secret is set. (issuePat also
  // throws as a backstop.)
  if (!env.CORELINK_INTERNAL_AUTH_KEY) {
    console.error(
      `[clerk-webhook] CORELINK_INTERNAL_AUTH_KEY absent — cannot mint PAT; ` +
        `returning 500 before any tenant write (user=${event.data.id}, svix=${svixId})`,
    );
    return new Response(
      JSON.stringify({ ok: false, error: "internal_auth_key_unconfigured" }),
      { status: 500, headers: { "content-type": "application/json" } },
    );
  }

  try {
    const result = await autoProvisionFromClerkEvent({
      event,
      colo,
      svixId,
      api: apiFactory(env),
      analytics: d1AnalyticsEmitter(env.ANALYTICS_DB),
    });
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
export function defaultApiClient(env: AutoProvisionEnv): ApiClient {
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
      await insertTenant(env.CONFIG_DB, {
        tenantId,
        primaryRegion: region,
        tenantSlug: name,
        emailHash,
        clerkUserId: ownerUserId,
        nowMs,
      });

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
      if (!env.CORELINK_INTERNAL_AUTH_KEY) {
        // FAIL-LOUD (signup money path): without the internal auth key we
        // CANNOT mint a real PAT. The old behavior returned a fake
        // "corelink_pat_DEVSTUB" that looked valid to the user but
        // authenticated NOTHING, while the Clerk webhook still returned 200 —
        // so Svix never retried and the new user was permanently broken with
        // no signal. Throw instead: the webhook handler maps this to a 500,
        // Svix redelivers, and the signup self-heals once the secret is set.
        throw new Error(
          "CORELINK_INTERNAL_AUTH_KEY is not configured — refusing to issue a stub PAT",
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
            "x-corelink-internal-auth": env.CORELINK_INTERNAL_AUTH_KEY,
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
        await env.CONFIG_DB.prepare(
          "INSERT OR IGNORE INTO pat " +
            "(pat_id, tenant_id, pat_hash, scope, expires_ms, token_id, " +
            " shown_once_token, shown_once_consumed, created_ms) " +
            "VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1, ?8)",
        )
          .bind(
            mint.pat_id,
            tenantId,
            mint.hash,
            scope, // persist the caller-requested scope (read-write), NOT admin
            mint.expires_ms,
            mint.token_id,
            crypto.randomUUID(), // shown_once_token (pre-consumed)
            nowMs,
          )
          .run();
      }

      // Return plaintext — NEVER log this value.
      return { id: mint.pat_id, plaintext: mint.token_plaintext };
    },

    // ── publishUserMetadata ───────────────────────────────────────────────────
    // Writes tenant_id + region + pat_plaintext to Clerk public metadata.
    // The session claims become available on the user's NEXT session refresh
    // (Clerk propagates metadata to JWT on next token issue).
    async publishUserMetadata(
      userId: string,
      metadata: Record<string, unknown>,
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
          // NEVER log this body — `metadata.pat_plaintext` is a secret.
          body: JSON.stringify({ public_metadata: metadata }),
        },
      );
      if (!resp.ok) {
        throw new Error(`clerk_metadata_update_failed_${resp.status}`);
      }
    },
  };
}

// Minimal local D1 type — we don't import @cloudflare/workers-types here
// to keep the unit-test surface independent of the runtime types package.
interface D1PreparedStatement {
  bind(...values: unknown[]): D1PreparedStatement;
  run(): Promise<{ success: boolean; error?: string }>;
  all<T = unknown>(): Promise<{ results?: T[] }>;
  first<T = unknown>(): Promise<T | null>;
}
interface D1Database {
  prepare(query: string): D1PreparedStatement;
}
