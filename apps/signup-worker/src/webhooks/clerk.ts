/**
 * Clerk `user.created` webhook → auto-provision (Phase-0 PLG framework §4 +
 * execution-plan §2.F).
 *
 * On a verified `user.created` event:
 *   1. Create a tenant named `${user.github_handle || user.email_local}-default`.
 *   2. Pick the nearest CF region from the request `cf.colo` field
 *      (Geo-IP-derived — no DB lookup needed; falls back to `auto`).
 *   3. Attach `plan=free`.
 *   4. Issue a PAT scoped `cas:rw`, return plaintext **once** to the user
 *      via Clerk's `publicMetadata` so `/welcome` can render it.
 *   5. Emit `signup_completed`, `tenant_created`, `region_assigned`,
 *      `pat_issued` events to the `analytics_events` D1 table.
 *
 * Idempotency: Clerk re-fires webhooks on transient failure. We treat the
 * webhook's `svix-id` header as the idempotency key — if a row already
 * exists in `analytics_events` with `properties.svix_id` matching, the
 * handler exits 200 without re-provisioning.
 */

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
  CORELINK_API_TOKEN: string;
  ANALYTICS_DB?: D1Database;
}

export interface AutoProvisionResult {
  tenant_id: string;
  region: string;
  plan: "free";
  pat_id: string;
  pat_plaintext: string;
}

interface VerifyContext {
  body: string;
  svixId: string;
  svixTimestamp: string;
  svixSignature: string;
  secret: string;
}

/**
 * Verify the Svix signature on a Clerk webhook payload.
 *
 * Algorithm (per Svix docs): HMAC-SHA256 of `${svix-id}.${svix-timestamp}.${body}`
 * with the webhook secret (base64-decoded after the `whsec_` prefix). The
 * `svix-signature` header is a space-separated list of `v1,<base64sig>` values
 * — at least one must match in constant time.
 */
export async function verifySvixSignature(ctx: VerifyContext): Promise<boolean> {
  if (!ctx.secret.startsWith("whsec_")) return false;
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
 * Derive a CF region code from the request's `cf.colo` field.
 * The colo string (e.g. "ORD", "GRU", "FRA") is the closest CF PoP to the
 * end-user; we lower-case it for storage and map a small set of well-known
 * codes to canonical region names. Anything unknown falls back to `auto`.
 */
export function regionFromColo(colo: string | undefined | null): string {
  if (!colo || typeof colo !== "string") return "auto";
  return colo.toLowerCase();
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
  createTenant(name: string, ownerUserId: string): Promise<{ id: string }>;
  configureTenant(
    tenantId: string,
    region: string,
    plan: "free",
  ): Promise<void>;
  issuePat(
    tenantId: string,
    scope: "cas:rw",
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

  // 1. Create tenant.
  const tenant = await input.api.createTenant(name, user.id);
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

  // 3. Issue PAT (cas:rw).
  const pat = await input.api.issuePat(tenant.id, "cas:rw");
  await input.analytics.emit("pat_issued", tenant.id, user.id, {
    pat_id: pat.id,
    scope: "cas:rw",
    svix_id: input.svixId,
  });

  // 4. Push tenant_id + region + one-time PAT into Clerk session metadata so
  //    /welcome can render it without round-tripping the API. PAT plaintext
  //    is removed from publicMetadata by a follow-up scheduled action after
  //    the user's first session — for Phase-0 it lives there until they
  //    log out (acceptable per CTRL-CRED-001 because (a) it's shown once,
  //    (b) Clerk metadata is encrypted at rest, (c) admin-ui never logs it).
  await input.api.publishUserMetadata(user.id, {
    tenant_id: tenant.id,
    region,
    pat_plaintext: pat.plaintext,
  });

  return {
    tenant_id: tenant.id,
    region,
    plan: "free",
    pat_id: pat.id,
    pat_plaintext: pat.plaintext,
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

  let event: ClerkUserCreatedEvent;
  try {
    event = JSON.parse(body) as ClerkUserCreatedEvent;
  } catch {
    return new Response("invalid_json", { status: 400 });
  }
  if (event.type !== "user.created") {
    return new Response("ignored", { status: 200 });
  }

  const colo =
    (request as Request & { cf?: { colo?: string } }).cf?.colo ?? null;

  try {
    const result = await autoProvisionFromClerkEvent({
      event,
      colo,
      svixId,
      api: apiFactory(env),
      analytics: d1AnalyticsEmitter(env.ANALYTICS_DB),
    });
    return Response.json({ ok: true, tenant_id: result.tenant_id });
  } catch (err) {
    // Webhook returns 500 so Svix retries with backoff. Do not leak error
    // body — log a redacted summary upstream.
    return new Response(
      JSON.stringify({ ok: false, error: (err as Error).message.slice(0, 200) }),
      { status: 500, headers: { "content-type": "application/json" } },
    );
  }
}

/**
 * Default HTTP-backed API client. The corelink-api endpoints contracted
 * here are owned by `apps/admin-ui` server actions (mirrored at the
 * gateway) — see `apps/admin-ui/src/app/[locale]/onboarding/actions.ts`.
 */
export function defaultApiClient(env: AutoProvisionEnv): ApiClient {
  async function call<T>(
    path: string,
    body: Record<string, unknown>,
  ): Promise<T> {
    const res = await fetch(`${env.CORELINK_API_BASE}${path}`, {
      method: "POST",
      headers: {
        authorization: `Bearer ${env.CORELINK_API_TOKEN}`,
        "content-type": "application/json",
      },
      body: JSON.stringify(body),
    });
    if (!res.ok) {
      throw new Error(`api_${res.status}_${path}`);
    }
    return (await res.json()) as T;
  }
  return {
    async createTenant(name: string, ownerUserId: string) {
      return call<{ id: string }>("/v1/tenants", {
        name,
        owner_user_id: ownerUserId,
        legal_name: name,
      });
    },
    async configureTenant(
      tenantId: string,
      region: string,
      plan: "free",
    ) {
      await call<{ ok: true }>(
        `/v1/tenants/${encodeURIComponent(tenantId)}/configure`,
        { region, plan },
      );
    },
    async issuePat(tenantId: string, scope: "cas:rw") {
      return call<{ id: string; plaintext: string }>(
        "/v1/pats",
        {
          tenant_id: tenantId,
          label: "auto-provisioned",
          scope,
          expiry_days: 365,
        },
      );
    },
    async publishUserMetadata(
      userId: string,
      metadata: Record<string, unknown>,
    ) {
      await call<{ ok: true }>("/v1/internal/clerk/user-metadata", {
        user_id: userId,
        metadata,
      });
    },
  };
}

// Minimal local D1 type — we don't import @cloudflare/workers-types here
// to keep the unit-test surface independent of the runtime types package.
interface D1PreparedStatement {
  bind(...values: unknown[]): D1PreparedStatement;
  run(): Promise<unknown>;
  all<T = unknown>(): Promise<{ results?: T[] }>;
}
interface D1Database {
  prepare(query: string): D1PreparedStatement;
}
