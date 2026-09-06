/**
 * CoreLink Cloudflare Worker shim — HTTP entry point, route table, auth
 * middleware, and request forwarding to the CoreLinkServer Durable Object.
 *
 * Architecture:
 *   Internet HTTPS → this Worker (auth + routing + error mapping)
 *                         ↓  env.CORELINK_SERVER.idFromName(tenantId)
 *                    CoreLinkServer DO (container lifecycle manager)
 *                         ↓  container_run_grpc(:50051)
 *                    Rust gRPC binary (corelink-server)
 *
 * Charter constraints:
 *   - Zero `any` types — all bindings typed via Env interface.
 *   - INV-NO-BODY-IN-LOGS: body bytes NEVER logged; request-id only.
 *   - INV-NO-PII-IN-LOGS: auth headers sanitized to hashed token prefix.
 *   - Constant-time auth compare via crypto.subtle.timingSafeEqual.
 *   - Timing-padding for 404s per design-pattern-01.
 *   - Tenant isolation: DO ID derived solely from tenant_id.
 *   - CORS per Access-Control-Allow-* config.
 */

import type { D1Database, DurableObjectNamespace, ExecutionContext } from "@cloudflare/workers-types";
import type { InternalConsumer } from "./lib/internal_auth.js";

// ──────────────────────────────────────────────────────────────────────────────
// Types
// ──────────────────────────────────────────────────────────────────────────────

/** Worker environment bindings — matches wrangler.toml. */
export interface Env {
  CORELINK_SERVER: DurableObjectNamespace;
  // ADR-0065 — per-tenant append-only event-log DO (hugit-P2 seam D).
  // One DO instance per tenant: idFromName(tenant_id). Bound in wrangler.toml
  // `[[durable_objects.bindings]]` (name = "EVENT_LOG_DO"). Optional in the
  // type so existing test envs that omit it still typecheck.
  EVENT_LOG_DO?: DurableObjectNamespace;
  // WI-MULTI-REGION-V1 — the SINGLE global replication-coordinator DO
  // (idFromName(REPLICATION_COORDINATOR_SINGLETON)). Its single-instance
  // guarantee IS the split-brain-safe promotion lock; its `alarm()` is the
  // scheduled evaluate→promote driver. Bound in wrangler.toml
  // `[[durable_objects.bindings]]` (name = "REPLICATION_COORDINATOR_DO").
  // Optional so existing test envs that omit it still typecheck.
  REPLICATION_COORDINATOR_DO?: DurableObjectNamespace;
  // P3 edge-local request-metering DOs (ACCEPTED, docs/design/2026-08-19-adr-edge-
  // local-do-request-metering.md). RequestMeterCoordinatorDO — 1/tenant, the
  // monthly-cap token-lease authority; RequestMeterShardDO — 1/(tenant,region),
  // the edge-local lease balance. Bound in wrangler.toml but INERT until the
  // per-region EDGE_DO_METER flag is flipped; optional so envs that omit them (and
  // test envs) still typecheck.
  REQUEST_METER_COORDINATOR_DO?: DurableObjectNamespace;
  REQUEST_METER_SHARD_DO?: DurableObjectNamespace;
  ENVIRONMENT: string;
  // D1 CONFIG_DB — control-plane database. Holds the `pat` table queried
  // during PAT validation (WP-A1). Bound in wrangler.toml `[[d1_databases]]`.
  CONFIG_DB: D1Database;
  // Secrets (bound via `wrangler secret put`)
  CLERK_SECRET_KEY?: string;
  // Activate exact-issuer pin: `wrangler secret put CLERK_ISSUER_URL --env prod`
  // Value = Clerk Frontend API / issuer URL, e.g. https://<slug>.clerk.accounts.dev
  // or the prod issuer shown in the Clerk dashboard → API Keys → Frontend API URL.
  // Without this secret production auth fails closed; only non-production
  // development/test requests use the conservative shape-check fallback.
  CLERK_ISSUER_URL?: string;
  // ── githugr multi-issuer (exchange/token-exchange paths only) ────────────────
  // The SEPARATE githugr Clerk instance accepted on the exchange seams IN ADDITION
  // to CoreLink's, WITHOUT touching CLERK_ISSUER_URL (so CoreLink's own dashboard +
  // onboarding keep validating CoreLink's instance). Both must be set to arm the
  // githugr path; absent ⇒ githugr sessions simply 401 (no behavior change).
  //   GITHUGR_CLERK_ISSUER_URL — `https://clerk.githugr.com` (the iss to accept).
  //   GITHUGR_CLERK_JWT_KEY    — githugr's PUBLIC instance key (PEM) for networkless
  //                              verify; public, no secret crosses products.
  // PILOT ISOLATION FIX: the former `GITHUGR_TENANT_ID` (a single shared tenant for
  // every githugr user) is REMOVED — a githugr session now provisions-or-looks-up
  // a per-user isolated tenant keyed on the Clerk `sub` (see lib/clerk_auth.ts →
  // verifyGithugrSession / lib/githugr_provision.ts).
  GITHUGR_CLERK_ISSUER_URL?: string;
  GITHUGR_CLERK_JWT_KEY?: string;
  STRIPE_SECRET_KEY?: string;
  // L3 money path — forwarded to the container by the DO (see durable_object.ts);
  // the tier-select route is unmounted (404) without CORELINK_DPA_VERSION, and
  // Stripe checkout needs the per-tier price ids.
  STRIPE_AUTH_MODE?: string;
  STRIPE_WEBHOOK_SECRET?: string;
  STRIPE_PRICE_ID_SOLO?: string;
  STRIPE_PRICE_ID_STARTER?: string;
  STRIPE_PRICE_ID_TEAM?: string;
  STRIPE_PRICE_ID_PRO?: string;
  STRIPE_PRICE_ID_MAX?: string;
  // Runners-tier prices (forwarded to the container's seed handler).
  STRIPE_PRICE_ID_RUNNER_STARTER?: string;
  STRIPE_PRICE_ID_RUNNER_PRO?: string;
  STRIPE_PRICE_ID_RUNNER_TEAM?: string;
  STRIPE_PRICE_ID_RUNNER_SCALE?: string;
  STRIPE_PRICE_ID_RUNNER_MAX?: string;
  CORELINK_DPA_VERSION?: string;
  // PAT HMAC signing key (raw hex, ≥ 32 bytes decoded) — used for the
  // HMAC-SHA256 fast-fail layer in PAT validation (WP-A1 step 2).
  // Bound via: `wrangler secret put PAT_SIGNING_KEY`
  // Key derivation: HKDF-SHA256(CORELINK_MASTER_KEY, "corelink-pat-signing-salt-v1",
  //   b"corelink-v1-pat-signing-key", 32) per key_management.md §3.2.1.
  // REQUIRED: extractAuth() fails closed (503) when this secret is absent
  // or decodes to fewer than 32 bytes. Never optional in any deployed env.
  PAT_SIGNING_KEY: string;
  // DevEnv DO cross-worker binding (WP-08)
  RUNNER_DEVENV_DO?: DurableObjectNamespace;
  // Public base URL (e.g. "https://corelink-api.humangr.com")
  // OPTIONAL rotation overlap keys (key_management.md §3.2.1, 24h overlap).
  // During a PAT_SIGNING_KEY rotation, bind the OUTGOING key as
  // PAT_SIGNING_KEY_PREV (and/or stage the INCOMING key as
  // PAT_SIGNING_KEY_NEW) so a PAT minted under either sibling still
  // HMAC-verifies through the overlap window — rotation (incl. rotate-on-
  // compromise) is then NOT an instant fleet-wide auth outage. Each is a
  // hex string (≥ 32 bytes decoded); malformed siblings fail CLOSED (503),
  // never silently shrinking the overlap set. Bound via:
  // `wrangler secret put PAT_SIGNING_KEY_PREV` / `..._NEW`.
  PAT_SIGNING_KEY_PREV?: string;
  PAT_SIGNING_KEY_NEW?: string;
  // Stream-5: shared secret for `/_internal/*` (passed to container at boot +
  // verified before forwarding). Bound via:
  // `wrangler secret put CORELINK_INTERNAL_AUTH_KEY`
  //
  // This is the SHARED fallback. The per-consumer key split below mirrors the
  // container's just-merged Rust split (red-team #3): each internal consumer
  // gets its OWN key so a single leak does not unlock every internal surface.
  // Resolution per consumer (see lib/internal_auth.ts `resolveConsumerKey`):
  // use the consumer-specific key iff set AND >= 32 chars; if it is SET but
  // shorter, fail-CLOSED (never widened to the shared key); only when it is UNSET
  // does the shared key serve, iff >= 32; else fail-CLOSED. FROZEN names:
  CORELINK_INTERNAL_AUTH_KEY?: string;
  // Per-consumer internal-auth keys (red-team #3 split). Each falls back to
  // CORELINK_INTERNAL_AUTH_KEY when UNSET (a set-but-short key is refused
  // fail-closed — see lib/internal_auth.ts). Provisioned by the operator
  // (`wrangler secret put …`) — see LEAD FLAGS in the PR. The container reads
  // the same names on its side.
  CORELINK_PAT_MINT_AUTH_KEY?: string; // gate for `/_internal/pat/mint`
  CORELINK_ADMIN_AUTH_KEY?: string;    // gate for admin `/_internal/*` routes
  CORELINK_ADMIN_APPROVER_AUTH_KEY?: string; // H5 dual-approve gate for `POST /v1/admin/approve` (DISTINCT from the mutate/admin key → two-person control)
  CORELINK_ERASE_AUTH_KEY?: string;    // gate for erase `/_internal/*` routes
  CORELINK_ERASE_AUTH_KEY_PREVIOUS?: string; // outgoing erase key during a rotation (dual-key; forwarded to the container)
  CORELINK_AUDIT_ATTEMPTED_AUTH_KEY?: string; // dedicated key for the container's /_internal/audit/cas-attempted (no shared fallback; forwarded to the container)
  CORELINK_DSR_ANCHOR_AUTH_KEY?: string; // gate for `/_internal/dsr/anchor` (per-user DSR legitimacy anchor; held by githugr, distinct from the eraser's ERASE key)
  // Money onboarding endpoints have distinct authorities. The container uses
  // these names with the same 32-char resolver and shared fallback.
  CORELINK_TIER_SELECT_AUTH_KEY?: string; // `/v1/onboarding/tier-select`
  CORELINK_DPA_ACCEPT_AUTH_KEY?: string; // `/v1/onboarding/dpa-accept`
  DSR_RECEIPT_SIGNING_KEY?: string; // HMAC signer for DSR customer-portal receipt JWTs (union #717; read by dsr/portal.rs, forwarded to the container)
  DPA_RECEIPT_SIGNING_KEY?: string; // RS256 (RSA PKCS#8/PKCS#1 PEM) signer for DPA-acceptance receipt JWTs (read by routes/dpa_accept.rs, forwarded to the container; route unmounts fail-CLOSED when absent)
  CORELINK_RUNNER_MINT_AUTH_KEY?: string; // gate for `/internal/v1/runner/{mint,revoke}` (runner dispatcher; scoped away from signup's pat_mint)
  CORELINK_QUOTA_READ_AUTH_KEY?: string; // gate for `/_internal/tenant/{tenant_id}/quota` (read-only tenant-quota lookup; low-privilege read consumer, distinct from mint/erase/admin)
  // Per-tier quota enforcement (worker/src/lib/quota.ts).
  // Storage quota is always enforced for finite-quota tiers.
  // Monthly request-count quota is backed by the monthly_request_counts table
  // (migration 0071) and is ENFORCED BY DEFAULT (fail-CLOSED). It is an explicit
  // opt-OUT kill-switch: enforcement runs (atomic increment-and-check) UNLESS
  // this flag === "true". Set REQUEST_QUOTA_DISABLED="true" ONLY in dev/test to
  // skip the counter; an unset var in prod keeps the contracted cap LIVE.
  REQUEST_QUOTA_DISABLED?: string;
  // Container storage credentials (WP-S1 StorageEnv contract). The DO forwards
  // these to the native container via container.start({ env }) so it can reach
  // R2 (S3 API) + D1 (HTTP API). Absent → container falls back to InMemory
  // (dev/CI). R2_S3_ENDPOINT + D1_DATABASE_ID are non-secret vars; the two R2
  // keys + CF_API_TOKEN are secrets (wrangler secret put). CLOUDFLARE_ACCOUNT_ID
  // is an existing secret reused here.
  R2_S3_ENDPOINT?: string;
  R2_S3_ACCESS_KEY_ID?: string;
  R2_S3_SECRET_ACCESS_KEY?: string;
  CF_API_TOKEN?: string;
  D1_DATABASE_ID?: string;
  CLOUDFLARE_ACCOUNT_ID?: string;
  // ADR-MULTI-REGION-V1 — per-region container bucket + region vars.
  // Set in each [env.prod-<region>] block's `vars`. The DO forwards these to
  // container.start({ env }) so the Rust binary writes to the correct regional
  // R2 bucket. Absent → container defaults to IAD (corelink-ac-iad / iad).
  // R2_AC_BUCKET:    AC envelope bucket for this region (e.g. corelink-ac-lhr)
  // R2_AC_REGION:    AC bucket region code (e.g. lhr)
  // R2_CHUNK_BUCKET: Multipart chunk bucket for this region (e.g. corelink-chunk-lhr)
  // R2_CHUNK_REGION: Chunk bucket region code (e.g. lhr)
  R2_AC_BUCKET?: string;
  R2_AC_REGION?: string;
  R2_CHUNK_BUCKET?: string;
  R2_CHUNK_REGION?: string;
  // CAS residency (F7/F8 — 2026-06-13 audit): per-region CAS storage, forwarded
  // to the container via container.start({env}). Region-correct keying; the
  // physical per-region CAS buckets are an infra follow-up.
  R2_CAS_REGION?: string;
  R2_CAS_BUCKET?: string;
  R2_AC_BUCKET_PREFIX?: string;
  R2_TURBO_BUCKET?: string;
  // Container env-contract (forwarded via durable_object.ts container.start):
  // secrets + provider vars the native container reads from its own process env.
  R2_TDK_HEX?: string;
  // F3.3 worker-native `_public` cache-HIT read. `CAS_BUCKET` is the native R2
  // binding (already deployed on every env) the edge read fetches bytes from
  // directly; `EDGE_PUBLIC_READ` is the runtime flag: "shadow" = compute+compare
  // only (serve the container), "serve" = edge-authoritative HIT. Unset = off.
  CAS_BUCKET: R2Bucket;
  EDGE_PUBLIC_READ?: string;
  /**
   * F1 of the edge-native `findMissingBlobs`
   * (`docs/design/2026-08-25-adr-edge-native-find-missing.md`). `"shadow"`
   * computes the edge answer on a background clone and logs whether it agrees
   * with the container's; it NEVER serves. `"on"` SERVES the edge answer, but
   * only after the `ReadAttempted` rows committed through the container's
   * `/_internal/audit/cas-attempted` — see
   * `docs/design/2026-08-26-adr-edge-find-missing-audit-seam.md`. Any reason to
   * doubt (BYOK tenant, over cap, probe error, audit not committed) falls
   * through to the container unchanged. Unset ⇒ neither runs, which is today's
   * behaviour exactly.
   */
  EDGE_FIND_MISSING?: string;
  // F3.2 WP-E client-side `_public` dedup flag, forwarded to the container. "0"
  // (Roll-1) = client OCI finalize routes per-tenant (never `_public`); a later
  // roll flips it on so client finalizes dedup into the server-seeded shared blob.
  OCI_PUBLIC_DEDUP_ENABLED?: string;
  OCI_UPSTREAM_ON_MISS?: string;
  // P3 edge-local request metering (ADR 2026-08-19). Runtime flag: unset/off =
  // today's D1-UPSERT request-count path (unchanged); "shadow" = additionally run
  // the DO metering path off the response path and log its verdict vs D1 (D1 still
  // authoritative, no user impact); "serve" = DO-authoritative (NOT flip-ready
  // until the WP-4 DO→D1 reconcile lands). Per-region, owner-flipped.
  EDGE_DO_METER?: string;
  // Off-by-default diagnostic: unset/anything-but-"on" = today's header exactly
  // (`wdb`, `qtier`, `qbatch`, `qresid`, no `qdo`, no `qcontrol`). "on" additionally
  // publishes `qdo` (the awaited `serveViaDO` hop) and `qcontrol` (framework work).
  // Gated, not default-on, because `qdo`'s presence/absence is a CONFIRMATION
  // ORACLE: `meter` (which gates `serveGateActive`, which gates whether `qdo`
  // runs at all) is `!isFanout && requestQuotaEnabled`, and `isFanout` is a
  // constant-time compare of the CLIENT-supplied `x-corelink-fanout-from`
  // header against `CORELINK_INTERNAL_AUTH_KEY` — the same oracle the
  // `qmeter`+`qstor` → `qbatch` merge exists to close (see the comment at the
  // `qbatch` assignment). A future reader must not flip this default without
  // understanding that "on" leaks one bit of the internal key's correctness
  // per request on a capped tier. Operator-flipped only; not in wrangler.toml.
  SERVER_TIMING_WDB_DETAIL?: string;
  // Forwarded to the CONTAINER (see `durable_object.ts` container.start env
  // forward-list), not read by the Worker itself: "on" arms the container's
  // `oargon`/`opermit`/`ortier` detail phases. Declared here so the forward is
  // typed. Off by default — see `detail_phases_enabled` in origin_timing.rs.
  CORELINK_ORIGIN_TIMING_DETAIL?: string;
  ERASURE_SALT_KEY?: string;
  ERASURE_ATTESTATION_SEED_HEX?: string;
  ERASURE_ATTESTATION_KEY_ID?: string;
  ERASURE_ATTESTATION_REGION?: string;
  ERASURE_ATTESTATION_SINGLE_REGION?: string;
  AUDIT_CHAIN_SIGNING_SEED_HEX?: string;
  AUDIT_CHAIN_SIGNING_KEY_ID?: string;
  AUDIT_CHAIN_TRUST_UNSIGNED_RESUME?: string;
  AUDIT_DRAIN_BATCH_LIMIT?: string;
  R2_AUDIT_BUCKET?: string;
  AUDIT_ARCHIVE_BATCH_LIMIT?: string;
  EMAIL_HASH_SALT?: string;
  STRIPE_LAUNCH_COUPON?: string;
  FABRIC_INTROSPECT_AUTH_KEY?: string;
  FABRIC_INTROSPECT_AUTH_KEY_HUGR?: string; // HuGR toolkits introspect consumer (#398) — forwarded to the container
  BILLING_INGEST_AUTH_KEY?: string; // ASK-2 runner billing usage-push ingest — gate for `/internal/v1/billing/usage`; forwarded to the container
  SIGNUP_TOKEN_KEY?: string;
  CORELINK_OCI_TOKEN_KEY?: string;
  // Legacy alias of CORELINK_OCI_TOKEN_KEY (CAA-360 #8 name drift) — the prod
  // Worker secret. Forwarded to the container, which reads it via the routes.rs
  // `.or_else(...)` fallback. See durable_object.ts forward block.
  HUGR_OCI_TOKEN_KEY?: string;
  CORELINK_PORTAL_RETURN_URL?: string;
  AWS_REGION?: string;
  GCP_REGION?: string;
  CORELINK_BYOK_AZURE_REGION?: string;
  CORELINK_BYOK_AZURE_VAULT_URL?: string;
  CORELINK_BYOK_VAULT_REGION?: string;
  // WI-MULTI-REGION-V1 Service Bindings: prod env can fan-out to the 4
  // regional Workers. Set in [[env.prod.services]] blocks. Used by the
  // per-tenant routing logic: tenant.primary_region in D1 → dispatch via
  // the matching binding. Absent → request stays on IAD (default).
  PROD_SAM?: { fetch: typeof fetch };
  PROD_LHR?: { fetch: typeof fetch };
  PROD_NRT?: { fetch: typeof fetch };
  PROD_SYD?: { fetch: typeof fetch };
  // ── Onboarding funnel telemetry (PLG §7.1) ─────────────────────────────────
  // Service binding to the analytics ingest Worker (`corelink-analytics-prod`),
  // resolved to its `AnalyticsIngest` RPC entrypoint by
  // `entrypoint = "AnalyticsIngest"` in [[env.prod.services]] — without that
  // line the stub is the target's default `fetch` export and has no RPC methods.
  // Only that Worker holds the ANALYTICS_DB binding for `analytics_events`, and
  // a Worker→Worker fetch over the public custom domain is edge-rejected (error
  // 1014, CNAME Cross-User Banned) — so the binding is the ONLY path. NO
  // companion secret: the platform authenticates the caller, so `first_cli_authed`
  // needs no `ANALYTICS_INGEST_KEY` (see lib/onboarding_events.ts). Absent ⇒ the
  // emit is a silent no-op.
  ANALYTICS_SVC?: import("./lib/onboarding_events.js").AnalyticsIngestStub;
  // Scheduled drill hand-off. This is deliberately a same-account Worker
  // Service Binding, never a PagerDuty URL or routing-key-bearing HTTP call.
  // The receiving Worker owns the drill-specific delivery integration; an
  // absent binding is a visible scheduled failure rather than a pretend drill.
  SCHEDULED_DRILL_DELIVERY?: { fetch: typeof fetch };
  // ── Observability (Sentry error tracking) ───────────────────────────────────
  // OPTIONAL. The Sentry hook (see `export default` at the bottom of this file)
  // is a COMPLETE no-op until the operator sets SENTRY_DSN via
  // `wrangler secret put SENTRY_DSN --env prod` (and per regional env). When
  // unset the SDK init receives an empty DSN and never sends — so tests + the
  // pre-launch posture stay inert. Mirrors apps/analytics-worker/src/index.ts.
  SENTRY_DSN?: string;
  // OPTIONAL release tag surfaced on Sentry events (deploy SHA / version).
  SENTRY_RELEASE?: string;
}

/**
 * Cloudflare Durable Object location hints (the `locationHint` option of
 * `DurableObjectNamespace.get`). A DO with NO hint homes at the colo of first
 * access; the hint pins WHERE a brand-new DO — and, for `CoreLinkServer`, its
 * attached Rust container — is created.
 */
export type DoLocationHint =
  | "wnam"
  | "enam"
  | "sam"
  | "weur"
  | "eeur"
  | "apac"
  | "oc"
  | "afr"
  | "me";

/**
 * Map this Worker's own serving region (`R2_CAS_REGION`, a colo code) to the CF
 * DO location hint for the region it serves.
 *
 * WHY THIS EXISTS (multi-region container-serving bug, 2026-08-18): the region
 * fan-out is a CO-LOCATED Service Binding — the regional Worker (`PROD_LHR` /
 * `PROD_NRT` / …) executes in the CALLER's entry colo, not its named region.
 * Combined with a hint-LESS `CORELINK_SERVER.get()`, a regional tenant's DO +
 * its container homed at the entry colo instead of the region. When that colo is
 * not a CF Containers metro (or cannot start the container) the DO's container
 * never became reachable → `container_health_check_failed`. Hinting the DO to
 * this Worker's own region makes placement DETERMINISTIC and in-region.
 *
 * `sam` has NO Cloudflare region (documented platform limit — see
 * region-map.ts) so a sam-serving Worker pins to `enam` (US), matching where
 * sam-labelled data physically lands today. An unknown/unset region returns
 * `undefined` (no hint) → today's exact behaviour, never worse.
 */
export function doLocationHintForRegion(region: string | undefined): DoLocationHint | undefined {
  switch (region) {
    case "iad":
      return "enam";
    case "lhr":
      return "weur";
    case "nrt":
      return "apac";
    case "syd":
      return "oc";
    case "sam":
      // Cloudflare has no SAM region; sam data lands in US R2 today. Pin the DO
      // (and its container) to ENAM so it starts in a supported container metro
      // rather than homing non-deterministically at the caller's entry colo.
      return "enam";
    default:
      return undefined;
  }
}

/**
 * Build the options bag for `CORELINK_SERVER.get(id, opts)`: a region location
 * hint when this Worker's region is known, else `undefined` (bare `.get(id)`).
 * Every `CoreLinkServer` DO owns a container, so ALL `.get()` sites route
 * through this so the container homes in-region deterministically.
 */
export function serverGetOpts(env: Env): { locationHint: DoLocationHint } | undefined {
  const hint = doLocationHintForRegion(env.R2_CAS_REGION);
  return hint ? { locationHint: hint } : undefined;
}

/**
 * Extract the tenant UUID from an OCI bearer for routing only.
 *
 * OCI's bearer is minted by the container and is authenticated there; the
 * Worker deliberately does not treat this structural hint as authorization.
 * It is nevertheless safe to use for residency fan-out because a forged hint
 * can only select a regional service binding, while the receiving container
 * still verifies the complete HMAC before serving or storing any bytes. A
 * malformed/opaque bearer stays on the shared OCI leg and is rejected by the
 * container as usual.
 */
export function ociRoutingTenantId(request: Request): string | undefined {
  const authorization = request.headers.get("authorization");
  if (authorization === null || !authorization.startsWith("Bearer ")) return undefined;
  const token = authorization.slice("Bearer ".length);
  const fields = token.split(".");
  if (fields.length !== 6 || fields[0] !== "corelink-oci") return undefined;
  const tenant = fields[1];
  return /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(tenant)
    ? tenant
    : undefined;
}

/**
 * Route prefix of the DO D1-placement probe: `/_internal/do-d1-probe/{tenant_id}`.
 *
 * A DIAGNOSTIC, not a product surface. It forwards to the tenant's own
 * `CoreLinkServer` DO `/_do/health`, whose body carries two timed `SELECT 1`
 * reads (D1 primary vs nearest replica) measured from INSIDE the DO. That is
 * the one unmeasured fact the "route the container's D1 reads through the DO"
 * proposal hinges on: a DO-issued primary read ≤25 ms means the DO is
 * co-located with the ENAM primary (the proposal wins ~60-75 ms); ≥100 ms means
 * it is far (the proposal is a regression).
 *
 * The tenant id is REQUIRED and caller-supplied because DO placement is
 * per-DO-id: `idFromName(tenant)` — the number is only comparable to a measured
 * container `opat` if it comes from the SAME DO that serves that tenant.
 */
export const INTERNAL_DO_D1_PROBE_PREFIX = "/_internal/do-d1-probe/";

/**
 * Map a `/_internal/*` path to its auth CONSUMER (red-team #3 key split).
 *
 * The container exposes three internal surfaces with distinct blast radii:
 *   - `/_internal/pat/mint`  → `pat_mint` (CORELINK_PAT_MINT_AUTH_KEY)
 *   - `/_internal/admin/*`   → `admin`    (CORELINK_ADMIN_AUTH_KEY)
 *   - `/_internal/dsr/*`     → `erase`    (CORELINK_ERASE_AUTH_KEY)
 *
 * Anything else under `/_internal/*` (e.g. `/_internal/cas/*`) defaults to the
 * most-privileged data-plane consumer, `erase` — its key (and, via fallback,
 * the shared key) gates the CAS delete surface used by the DSR/erasure path.
 * Every consumer falls back to the shared CORELINK_INTERNAL_AUTH_KEY when its
 * dedicated key is unset (see resolveConsumerKey), so this never widens access.
 *
 * `pathSuffix` is the server-derived route path (NOT client-suppliable beyond
 * the URL itself, which already selected the `internal` routeKind).
 */
export function internalConsumerForPath(pathSuffix: string): InternalConsumer {
  if (pathSuffix === "/_internal/pat/mint") {
    return "pat_mint";
  }
  if (pathSuffix.startsWith("/_internal/admin/")) {
    return "admin";
  }
  // The per-user DSR legitimacy ANCHOR (`/_internal/dsr/anchor`, #634) is a
  // SEPARATE authority from the eraser: githugr holds `CORELINK_DSR_ANCHOR_AUTH_KEY`
  // (distinct from the eraser's ERASE key — the anti-forge two-authority split that
  // gates the irreversible physical-erase cascade). It MUST be matched before the
  // `/_internal/dsr/*` erase catch-all below, or the anchor caller is gated on the
  // wrong (erase) key and always 401s (the go-live blocker: the worker front-gate
  // rejected githugr's anchor key before it ever reached the container's own anchor
  // gate, so binding/forwarding the anchor key alone could never help).
  if (pathSuffix === "/_internal/dsr/anchor") {
    return "dsr_anchor";
  }
  // Read-only tenant-quota lookup (`/_internal/tenant/{tenant_id}/quota`, #quota-read):
  // a DEDICATED read consumer (`CORELINK_QUOTA_READ_AUTH_KEY`, shared-key fallback)
  // so a leak of this low-privilege read secret cannot mint, erase, or admin. Matched
  // BEFORE the erase catch-all below.
  if (pathSuffix.startsWith("/_internal/tenant/") && pathSuffix.endsWith("/quota")) {
    return "quota_read";
  }
  // Edge-probe audit emit (`/_internal/audit/cas-attempted`): its own consumer,
  // NOT the `erase` catch-all below. The container gates it on the dedicated
  // `CORELINK_AUDIT_ATTEMPTED_AUTH_KEY` with no shared fallback, so falling into
  // the catch-all would have the edge demand the ERASE key while the container
  // demands the audit key — a mismatch that 401s at the edge and leaves the
  // endpoint unreachable no matter which secret the operator binds.
  if (pathSuffix === "/_internal/audit/cas-attempted") {
    return "audit_attempted";
  }
  // Read-only DO D1-placement probe (`/_internal/do-d1-probe/{tenant_id}`) — the
  // diagnostic instrument that answers "where does this tenant's DO sit relative
  // to the ENAM D1 primary?". It mutates nothing (two `SELECT 1`s), so it gates
  // on the SAME low-privilege READ consumer as the quota lookup rather than on
  // the erase catch-all: an operator reading a latency number must not need the
  // key that can delete a tenant's bytes. No new key is introduced.
  if (pathSuffix.startsWith(INTERNAL_DO_D1_PROBE_PREFIX)) {
    return "quota_read";
  }
  // DSR erase surface (`/_internal/dsr/*`) and any other internal data-plane
  // route (`/_internal/cas/*`, …) gate on the erase consumer key.
  return "erase";
}

// ──────────────────────────────────────────────────────────────────────────────
// Error envelope builders (REAPI error shapes)
// ──────────────────────────────────────────────────────────────────────────────

interface ReapiErrorEnvelope {
  readonly error: string;
  readonly message: string;
  readonly request_id: string;
}

export function reapiError(error: string, message: string, status: number, requestId: string): Response {
  const body: ReapiErrorEnvelope = { error, message, request_id: requestId };
  return new Response(JSON.stringify(body), {
    status,
    headers: {
      "Content-Type": "application/json",
      "X-Request-Id": requestId,
    },
  });
}

// ──────────────────────────────────────────────────────────────────────────────
// Worker state (module-level, reset per isolate cold start)
// ──────────────────────────────────────────────────────────────────────────────

/**
 * One-time server nonce for timing-pad seed mixing.
 * Initialized lazily from crypto.getRandomValues on first request.
 * Module-scoped: stable within a single isolate lifetime.
 */
let serverNonce: number | null = null;

export function getServerNonce(): number {
  if (serverNonce === null) {
    const buf = new Uint32Array(1);
    crypto.getRandomValues(buf);
    serverNonce = buf[0] ?? 0;
  }
  return serverNonce;
}

/** The sole declared cron in wrangler.toml and its internal drill kind. */
const SCHEDULED_DRILL_BY_CRON: Readonly<Record<string, "synthetic_page">> = {
  "0 14 * * 1": "synthetic_page",
};

export const SYNTHETIC_PAGE_CONTRACT = {
  service: "synthetic-drill",
  event_action: "trigger",
  severity: "info",
  synthetic_severity: "sev2_synthetic",
} as const;

/** Stable week number used by the four-week synthetic page rotation. */
export function scheduledWeekNumber(scheduledTime: number): number {
  // 1970-01-05 was a Monday; all declared crons fire on Mondays. Floor
  // instead of rounding so a malformed pre-epoch test timestamp cannot move
  // into the following week.
  return Math.floor((scheduledTime - Date.UTC(1970, 0, 5)) / (7 * 24 * 60 * 60 * 1_000));
}

export function syntheticRegionForWeek(week: number): "americas" | "emea" | "apac" | "boundary_handoff" {
  switch (((week % 4) + 4) % 4) {
    case 0:
      return "americas";
    case 1:
      return "emea";
    case 2:
      return "apac";
    default:
      return "boundary_handoff";
  }
}

/**
 * The Monday cron is not the boundary-handoff emit window. For rotation week 3
 * the receiver must defer the real page to the following Sunday at 23:59 UTC.
 */
export function syntheticEmitAtMs(scheduledTime: number, week: number): number {
  if (((week % 4) + 4) % 4 !== 3) return scheduledTime;
  const scheduled = new Date(scheduledTime);
  const sunday = Date.UTC(
    scheduled.getUTCFullYear(),
    scheduled.getUTCMonth(),
    scheduled.getUTCDate() + (7 - scheduled.getUTCDay()),
    23,
    59,
    0,
    0,
  );
  return sunday;
}

export function scheduledDrillForCron(cron: string): "synthetic_page" | undefined {
  return SCHEDULED_DRILL_BY_CRON[cron];
}

// ──────────────────────────────────────────────────────────────────────────────
// Main fetch handler
// ──────────────────────────────────────────────────────────────────────────────
