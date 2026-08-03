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
 * Only `apps/analytics-worker` holds the `ANALYTICS_DB` binding for
 * `analytics_events`, and a Worker→Worker fetch over a public custom domain on
 * the same zone is rejected by Cloudflare's edge with error 1014 (CNAME
 * Cross-User Banned) — documented from real experience in the `[[services]]`
 * comment block at `apps/signup-worker/wrangler.toml:92-95`. So this module
 * dispatches EXCLUSIVELY through the `ANALYTICS_SVC` service binding; when the
 * binding is absent the emit is a silent no-op (there is no public-hostname
 * fallback, by design).
 *
 * WHY RPC AND NOT `svc.fetch(...)` — AND WHY THERE IS NO KEY ANY MORE
 * ------------------------------------------------------------------
 * The binding resolves to the `AnalyticsIngest` WorkerEntrypoint
 * (apps/analytics-worker/src/index.ts:153), declared caller-side by
 * `entrypoint = "AnalyticsIngest"` on the `[[env.prod.services]]` block
 * (wrangler.toml). Without that line the binding resolves to the analytics
 * Worker's DEFAULT `fetch` export, which exposes no RPC methods at all — so
 * the entrypoint declaration is load-bearing, not cosmetic.
 *
 * `ingestServerEvent` passes `trusted = true` unconditionally because a service
 * binding is authenticated BY THE PLATFORM: the stub is resolved by the
 * Cloudflare control plane at deploy time from this Worker's own config, the
 * call never leaves the runtime, and there is no hostname to point at nor
 * header to forge. That identity proof is strictly stronger than the shared
 * `X-Corelink-Ingest-Key` secret the HTTP path uses — which is why this module
 * no longer holds a key at all, and why the emit is finally LIVE rather than
 * blocked on an operator running `wrangler secret put` on `corelink-prod`.
 * The server-only-event rule (`first_cli_authed` is on
 * `SERVER_ONLY_EVENT_NAMES`, apps/analytics-worker/src/ingest.ts:51) is still
 * honoured — `trusted = true` is exactly what satisfies it. The spoofable
 * `Origin` browser path over HTTP still rejects the event, unchanged.
 *
 * DEPLOY ORDERING (load-bearing)
 * ------------------------------
 * `corelink-analytics-prod` must ship `AnalyticsIngest` BEFORE `corelink-prod`
 * deploys with `entrypoint =` set; a binding naming an entrypoint the target
 * Worker does not export is rejected at deploy time.
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
 *  (apps/analytics-worker/src/ingest.ts:25 and :51). The RPC entrypoint passes
 *  `trusted = true`, which is what makes a server-only name admissible. */
export const FIRST_CLI_AUTHED_EVENT = "first_cli_authed";

/** Max event-id length enforced by ingest validation (apps/analytics-worker/src/ingest.ts:112). */
export const ANALYTICS_EVENT_ID_MAX_LEN = 64;

/** Max tenant_id length enforced by ingest validation (apps/analytics-worker/src/ingest.ts:125). */
export const ANALYTICS_TENANT_ID_MAX_LEN = 128;

/**
 * Synthetic tenant sentinels used by the Worker's route matcher. None of them
 * is a real customer, so none may produce an onboarding row.
 */
const SENTINEL_TENANTS: ReadonlySet<string> = new Set(["_anonymous", "_system", "_pending"]);

/**
 * The event shape accepted by `AnalyticsIngest.ingestServerEvent`. Structural
 * mirror of `EventPayload` (apps/analytics-worker/src/types.ts) narrowed to the
 * fields this producer sets; `created_at` is deliberately omitted so ingest
 * stamps its own clock (apps/analytics-worker/src/ingest.ts:209 COALESCE).
 */
export interface AnalyticsServerEvent {
  readonly id: string;
  readonly event_name: string;
  readonly tenant_id?: string;
  readonly properties?: Record<string, unknown>;
}

/** Return shape of `ingestServerEvent` — `IngestResult`, apps/analytics-worker/src/ingest.ts:63. */
export interface AnalyticsIngestResult {
  readonly accepted: number;
  readonly rejected: number;
  readonly errors?: ReadonlyArray<{ id?: string; reason: string }>;
}

/**
 * The `ANALYTICS_SVC` stub, narrowed to the RPC surface. `fetch` is deliberately
 * NOT on this type: the HTTP ingest path is not reachable from here any more,
 * so no call-site can fall back to it by accident.
 *
 * The runtime still capability-checks `ingestServerEvent` before calling it
 * (see {@link emitFirstCliAuthed}): a deploy that set the binding before
 * `corelink-analytics-prod` shipped `AnalyticsIngest` would hand back a stub
 * for the DEFAULT `fetch` export, and that must degrade to a no-op rather than
 * throw a `TypeError` on the customer's request path.
 */
export interface AnalyticsIngestStub {
  readonly ingestServerEvent: (event: AnalyticsServerEvent) => Promise<AnalyticsIngestResult>;
}

/** The subset of `Env` this module needs. Keeps the lib unit-testable. */
export interface AnalyticsIngestEnv {
  /**
   * Service binding to `corelink-analytics-prod`, resolved to the
   * `AnalyticsIngest` entrypoint by `entrypoint = "AnalyticsIngest"` on the
   * `[[env.prod.services]]` block. No companion secret: the platform
   * authenticates the caller (see the module header).
   */
  readonly ANALYTICS_SVC?: AnalyticsIngestStub;
}

/** Options for {@link emitFirstCliAuthed}. */
export interface EmitOpts {
  /**
   * `ctx.waitUntil` — extends the request lifetime so the fire-and-forget RPC
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
 * entrypoint, sentinel tenant, over-long id, RPC throw, RPC never settling,
 * rejected event) is absorbed here and can never surface to the caller.
 */
export function emitFirstCliAuthed(
  env: AnalyticsIngestEnv,
  tenantId: string,
  opts: EmitOpts = {},
): void {
  try {
    const svc = env.ANALYTICS_SVC;
    // No binding ⇒ no emit. There is NO public-hostname fallback: Worker→Worker
    // over the custom domain is edge-rejected with error 1014.
    if (svc === undefined || svc === null) return;
    // Binding present but no RPC method ⇒ the binding resolved to the target's
    // DEFAULT `fetch` export because `entrypoint = "AnalyticsIngest"` is missing
    // (or the target predates the entrypoint). Degrade instead of throwing.
    // The declared type says this is always a function; the check defends the
    // one case the type system cannot see — a stale/mis-declared BINDING.
    if (typeof svc.ingestServerEvent !== "function") return;
    if (typeof tenantId !== "string" || tenantId.length === 0) return;
    if (SENTINEL_TENANTS.has(tenantId)) return;
    if (tenantId.length > ANALYTICS_TENANT_ID_MAX_LEN) return;

    const id = firstCliAuthedEventId(tenantId);
    if (id.length > ANALYTICS_EVENT_ID_MAX_LEN) return;

    const emit = (async (): Promise<void> => {
      try {
        // ONE event, no batch envelope, no key, no URL — the RPC method is the
        // whole contract (apps/analytics-worker/src/index.ts:170). It documents
        // itself as never-throwing, but a cross-isolate RPC fault can still
        // surface as an exception here, so the catch below stays load-bearing.
        const res = await svc.ingestServerEvent({
          id,
          event_name: FIRST_CLI_AUTHED_EVENT,
          tenant_id: tenantId,
          properties: { surface: "cli" },
        });
        if (res.rejected > 0) {
          const reason = res.errors?.[0]?.reason ?? "unknown";
          console.warn(`[analytics] first_cli_authed rejected: ${reason}`);
        }
      } catch (err) {
        // Analytics MUST NOT break the request. Log and drop.
        console.warn(`[analytics] first_cli_authed emit failed: ${(err as Error).message}`);
      }
    })();

    if (opts.waitUntil) {
      opts.waitUntil(emit);
    } else {
      // No waitUntil (tests): leave it floating. `emit` never rejects.
      void emit;
    }
  } catch {
    // Belt-and-braces: nothing on the synchronous leg may escape either.
  }
}
