/**
 * Onboarding funnel telemetry — the `first_cli_authed` producer (PLG §7.1).
 *
 * WHY THIS LIVES IN THE WORKER (and not the container)
 * ----------------------------------------------------
 * `first_cli_authed` fires on the CLI's first authenticated call, which is
 * `GET /v1/users/me` (`corelink whoami` — tools/cli/src/client.rs:138 — and the
 * doctor auth probe — tools/cli/src/doctor.rs:37). That route is *served* by the
 * Rust container, but the container reaches D1 through a single
 * `D1_DATABASE_ID` pointing at the CONTROL PLANE database, while
 * `analytics_events` lives ONLY in `corelink-analytics-prod`
 * (`apps/analytics-worker/wrangler.toml` → `[[env.prod.d1_databases]]`
 * `database_id = "d7fe391f-9fe0-4544-a1fb-64747bcd2639"`). The container
 * physically cannot write that table, and its charter forbids `tokio::spawn` in
 * `src/` (crates/corelink-container/src/routes/audit_export/stream.rs:44) so it
 * has no per-request fire-and-forget either. The Worker is already on the same
 * request path, already authenticates the PAT, and already owns `ctx.waitUntil`.
 *
 * WHY A SERVICE BINDING (and never the public hostname)
 * ----------------------------------------------------
 * Only `apps/analytics-worker` holds the `ANALYTICS_DB` binding, so the write
 * must go through its ingest route (`POST /v1/event`,
 * `apps/analytics-worker/src/ingest.ts:143`). A Worker→Worker fetch over a
 * public custom domain on the same zone is rejected by Cloudflare's edge with
 * error 1014 (CNAME Cross-User Banned) — documented from real experience in the
 * `[[services]]` comment block at `apps/signup-worker/wrangler.toml:92-95`. So
 * this module dispatches EXCLUSIVELY through the `ANALYTICS_SVC` service
 * binding; when the binding is absent the emit is a silent no-op (there is no
 * public-hostname fallback, by design).
 *
 * IDEMPOTENCY
 * -----------
 * `analytics_events` has exactly one uniqueness constraint — `id TEXT NOT NULL
 * PRIMARY KEY` (apps/analytics-worker/migrations/0001_create_analytics_events.sql:7)
 * — and ingest inserts with `INSERT OR IGNORE`
 * (apps/analytics-worker/src/ingest.ts:207). A DETERMINISTIC id therefore turns
 * the primary key itself into a once-per-tenant lock with zero extra round
 * trips: the first `/v1/users/me` for a tenant inserts, every later one is
 * coalesced by SQLite. (Contrast: `apps/signup-worker/src/lib/analytics-server.ts:24`
 * mints a random id, so it has no dedup at all — do NOT copy that here.)
 *
 * FAILURE POSTURE
 * ---------------
 * This emit must NEVER add latency to, nor be able to fail, the
 * `/v1/users/me` response. {@link emitFirstCliAuthed} therefore returns `void`
 * (it is structurally impossible to `await` inline), swallows every error on
 * both the synchronous and asynchronous legs, and hands its promise to
 * `ctx.waitUntil` — a promise NOT given to `waitUntil` is CANCELLED when the
 * response returns (bug #859, see worker/src/lib/tenant_suspend_gate.ts:115).
 */

/** Canonical event name — on the ingest allow-list AND its server-only list
 *  (apps/analytics-worker/src/ingest.ts:25 and :51), so the trusted-key header
 *  below is mandatory: the browser/Origin path would reject it. */
export const FIRST_CLI_AUTHED_EVENT = "first_cli_authed";

/**
 * Ingest URL. The hostname is inert for a service-binding dispatch (the request
 * is routed inside Cloudflare's runtime by the binding, never resolved over the
 * public edge); only the PATH `/v1/event` is load-bearing
 * (apps/analytics-worker/src/index.ts route → `handleIngest`).
 */
export const ANALYTICS_INGEST_URL = "https://corelink-analytics.humangr.com/v1/event";

/** Trusted-server ingest header — read at apps/analytics-worker/src/ingest.ts:160. */
export const ANALYTICS_INGEST_KEY_HEADER = "X-Corelink-Ingest-Key";

/** Max event-id length enforced by ingest validation (apps/analytics-worker/src/ingest.ts:112). */
export const ANALYTICS_EVENT_ID_MAX_LEN = 64;

/** Max tenant_id length enforced by ingest validation (apps/analytics-worker/src/ingest.ts:125). */
export const ANALYTICS_TENANT_ID_MAX_LEN = 128;

/**
 * Synthetic tenant sentinels used by the Worker's route matcher. None of them
 * is a real customer, so none may produce an onboarding row.
 */
const SENTINEL_TENANTS: ReadonlySet<string> = new Set(["_anonymous", "_system", "_pending"]);

/** The subset of `Env` this module needs. Keeps the lib unit-testable. */
export interface AnalyticsIngestEnv {
  /** Service binding to `corelink-analytics-prod` ([[env.prod.services]]). */
  readonly ANALYTICS_SVC?: { fetch: typeof fetch };
  /**
   * Shared trusted-ingest secret. Same value as the analytics-worker's
   * `INGEST_KEY` (secrets matrix row 138, docs/internal/secrets-checklist.md);
   * named `ANALYTICS_INGEST_KEY` on the CALLER side, mirroring
   * `apps/signup-worker/src/lib/analytics-server.ts:9`.
   */
  readonly ANALYTICS_INGEST_KEY?: string;
}

/** Options for {@link emitFirstCliAuthed}. */
export interface EmitOpts {
  /**
   * `ctx.waitUntil` — extends the request lifetime so the fire-and-forget POST
   * survives the response. Without it the promise is cancelled on return (#859).
   * Absent (tests) ⇒ the promise is left floating; it never rejects.
   */
  readonly waitUntil?: (p: Promise<unknown>) => void;
}

/**
 * Deterministic, once-per-tenant event id: `first_cli_authed:<tenant_id>`.
 *
 * Stable across calls by construction — that stability IS the dedup mechanism
 * (see the module header). Never randomised.
 */
export function firstCliAuthedEventId(tenantId: string): string {
  return `${FIRST_CLI_AUTHED_EVENT}:${tenantId}`;
}

/**
 * Fire-and-forget `first_cli_authed` for `tenantId`.
 *
 * Returns `void` — deliberately NOT a promise, so no call-site can accidentally
 * `await` it onto the hot path. Every failure mode (missing binding, missing
 * key, sentinel tenant, over-long id, `new Request` throw, service-binding
 * throw, non-2xx) is absorbed here and can never surface to the caller.
 */
export function emitFirstCliAuthed(
  env: AnalyticsIngestEnv,
  tenantId: string,
  opts: EmitOpts = {},
): void {
  try {
    const svc = env.ANALYTICS_SVC;
    const key = env.ANALYTICS_INGEST_KEY;
    // No binding ⇒ no emit. There is NO public-hostname fallback: Worker→Worker
    // over the custom domain is edge-rejected with error 1014.
    if (svc === undefined || svc === null) return;
    // `first_cli_authed` is on ingest's SERVER_ONLY list, so an unkeyed POST is
    // rejected as `server_only_event`. Skip rather than emit a guaranteed reject.
    if (typeof key !== "string" || key.length === 0) return;
    if (typeof tenantId !== "string" || tenantId.length === 0) return;
    if (SENTINEL_TENANTS.has(tenantId)) return;
    if (tenantId.length > ANALYTICS_TENANT_ID_MAX_LEN) return;

    const id = firstCliAuthedEventId(tenantId);
    if (id.length > ANALYTICS_EVENT_ID_MAX_LEN) return;

    const post = (async (): Promise<void> => {
      try {
        const req = new Request(ANALYTICS_INGEST_URL, {
          method: "POST",
          headers: {
            "Content-Type": "application/json",
            [ANALYTICS_INGEST_KEY_HEADER]: key,
          },
          // Batch envelope (`{ events: [...] }`) — accepted at
          // apps/analytics-worker/src/ingest.ts:184; batch of 1 is within the
          // trusted cap of 100 (ingest.ts:191). `created_at` is omitted so the
          // ingest worker stamps its own clock (ingest.ts:209 COALESCE).
          body: JSON.stringify({
            events: [
              {
                id,
                event_name: FIRST_CLI_AUTHED_EVENT,
                tenant_id: tenantId,
                properties: { surface: "cli" },
              },
            ],
          }),
        });
        const res = await svc.fetch(req);
        if (!res.ok) {
          console.warn(`[analytics] first_cli_authed non-2xx ${res.status}`);
        }
      } catch (err) {
        // Analytics MUST NOT break the request. Log and drop.
        console.warn(`[analytics] first_cli_authed emit failed: ${(err as Error).message}`);
      }
    })();

    if (opts.waitUntil) {
      opts.waitUntil(post);
    } else {
      // No waitUntil (tests): leave it floating. `post` never rejects.
      void post;
    }
  } catch {
    // Belt-and-braces: nothing on the synchronous leg may escape either.
  }
}
