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

import type { D1Database, DurableObjectNamespace, ExecutionContext, ExportedHandler } from "@cloudflare/workers-types";
import * as Sentry from "@sentry/cloudflare";
import { scrubSentryEvent } from "./sentry-scrub.js";
import { CoreLinkServer } from "./durable_object.js";
import { RolloutController } from "./rollout_controller.js";
import { EventLogDO } from "./event_log_do.js";
import { RequestMeterCoordinatorDO } from "./request_meter_coordinator_do.js";
import { RequestMeterShardDO } from "./request_meter_shard_do.js";
import {
  runQuotaBatch,
  requestCapResultForCount,
  storageQuotaHeaderValue,
  storageCapIsFinite,
  FREE_REQUEST_CAP,
  STORAGE_QUOTA_HEADER,
  QUOTAS,
  currentYearMonthUtc,
  secondsUntilNextMonthStart,
} from "./lib/quota.js";
import { meterViaDO, serveViaDO, serveGateActive } from "./lib/edge_do_meter.js";
import { checkStorageQuotaCachedRead } from "./lib/quota_storage_cache.js";
import {
  tryFastRequestCount,
  decideFastPath,
  populateRequestCountKv,
} from "./lib/quota_request_cache.js";
import { verifyClerkSessionAndResolveTenant } from "./lib/clerk_auth.js";
import { isTenantSuspended } from "./lib/tenant_suspend_gate.js";
import {
  resolveTenantResidency,
  RESIDENCY_UNRESOLVED,
} from "./lib/tenant_residency_cache.js";
import { resolveTenantTierCached } from "./lib/tenant_tier_cache.js";
import { verifyPatRowCached, type KvReader } from "./lib/pat_verify_cache.js";
import { handleSessionExchange, handleTokenExchange } from "./lib/session_exchange.js";
import { handleRunnerMint, handleRunnerRevoke } from "./lib/runner_mint.js";
import {
  readPublicHit,
  shadowCompareEdgePublicRead,
  parseByteRange,
  PUBLIC_BLOB_CONTENT_TYPE,
  writePublicBlocklistKv,
} from "./lib/edge_public_read.js";
import { handleAuthRotate } from "./lib/auth_rotate.js";
import { handleTenantLookup } from "./lib/tenant_lookup.js";
import {
  resolveConsumerKey,
  constantTimeSecretEqual,
  type InternalConsumer,
} from "./lib/internal_auth.js";
import { emitFirstCliAuthed } from "./lib/onboarding_events.js";
import { coloForMacro } from "./region-map.js";
import {
  ReplicationCoordinatorDO,
  REPLICATION_COORDINATOR_SINGLETON,
} from "./replication_coordinator_do.js";

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
  // Without this secret the worker falls back to the shape-check (https + "clerk").
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
  // OPTIONAL rotation overlap keys (key_management.md §3.2.1, 24h overlap).
  // During a PAT_SIGNING_KEY rotation, bind the OUTGOING key as
  // PAT_SIGNING_KEY_PREV (and/or stage the INCOMING key as
  // PAT_SIGNING_KEY_NEW) so a PAT minted under either sibling still
  // HMAC-verifies through the overlap window — rotation (incl. rotate-on-
  // compromise) is then NOT an instant fleet-wide auth outage. Each is a
  // hex string (≥ 32 bytes decoded); a malformed sibling is ignored (the
  // current key remains the load-bearing gate). Bound via:
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
  CORELINK_DSR_ANCHOR_AUTH_KEY?: string; // gate for `/_internal/dsr/anchor` (per-user DSR legitimacy anchor; held by githugr, distinct from the eraser's ERASE key)
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
  ERASURE_SALT_KEY?: string;
  ERASURE_ATTESTATION_SEED_HEX?: string;
  ERASURE_ATTESTATION_KEY_ID?: string;
  ERASURE_ATTESTATION_REGION?: string;
  ERASURE_ATTESTATION_SINGLE_REGION?: string;
  AUDIT_CHAIN_SIGNING_SEED_HEX?: string;
  AUDIT_CHAIN_SIGNING_KEY_ID?: string;
  AUDIT_CHAIN_TRUST_UNSIGNED_RESUME?: string;
  AUDIT_DRAIN_BATCH_LIMIT?: string;
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
type DoLocationHint =
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
function serverGetOpts(env: Env): { locationHint: DoLocationHint } | undefined {
  const hint = doLocationHintForRegion(env.R2_CAS_REGION);
  return hint ? { locationHint: hint } : undefined;
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
const INTERNAL_DO_D1_PROBE_PREFIX = "/_internal/do-d1-probe/";

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

/** Parsed route context derived from matching the request URL. */
interface RouteMatch {
  readonly tenantId: string;
  readonly pathSuffix: string;
  readonly routeKind: RouteKind;
}

/** Canonical route kinds that this worker handles. */
type RouteKind =
  | "health"
  | "health_serving"
  | "oci_v2"
  | "oci_token"
  | "billing_webhook"
  | "fabric_introspect"
  | "billing_ingest"
  | "npm"
  | "pip"
  | "brew"
  | "cargo"
  | "reapi_v2"
  | "customer_v1"
  | "public_attestation"
  | "reapi_v1"
  | "bazel_v2"
  | "turbo_v8"
  | "signup"
  | "onboarding"
  | "session_exchange"
  | "tenant_lookup"
  | "token_exchange"
  | "runner_mint"
  | "runner_revoke"
  | "auth_rotate"
  | "internal"
  | "health_container"
  | "not_found";

/** Auth extraction result from the Authorization header. */
type AuthResult =
  | {
      readonly ok: true;
      readonly tenantId: string;
      readonly tokenPrefix: string;
      /**
       * The PAT's D1-resolved `scope` column (security: H1). The Worker is the
       * SOLE authority for this value — it is read from the trusted D1 `pat`
       * mirror, never from the client, and forwarded to the DO/container as the
       * `x-corelink-scope` server-trust header so the container can ENFORCE it.
       * Defaults to `""` for older rows whose `scope` is NULL/absent.
       */
      readonly scope: string;
      /**
       * cf-multitenant WP5a: the D1-resolved `pat.runner_job_ac_key` marking a
       * NARROWED runner-job PAT. `null` = a normal PAT (no narrowing → no extra
       * headers forwarded). A non-null value (`"*"` = deny-DELETE only, or a
       * BLAKE3 hex = also exact-key AC restricted) causes the Worker to forward
       * `x-corelink-runner-job: 1` + `x-corelink-ac-key-allow: <value>` as
       * server-trust headers the container (WP5b) enforces. Read from the trusted
       * D1 `pat` mirror only — never the client.
       */
      readonly runnerJobAcKey: string | null;
      /**
       * Which tier served the PAT row (`l1` isolate / `kv` L2 / `d1` primary).
       * Surfaced into the `Server-Timing` `auth` desc for client latency probes;
       * NOT a trust signal and never forwarded to the container. Absent on the
       * anonymous/signup path (no PAT verify runs there).
       */
      readonly patSource?: "l1" | "kv" | "d1";
    }
  | { readonly ok: false; readonly reason: string };

// ──────────────────────────────────────────────────────────────────────────────
// Constants
// ──────────────────────────────────────────────────────────────────────────────

/**
 * P3 EDGE_DO_METER lease block `L` (tokens a shard leases from the coordinator per
 * refill). Bounds under-serve (≤ #regions·L stranded) and the coordinator-hop rate
 * (~1 hop per L requests). Capped by the tier's own cap at the call-site so a
 * small-cap tier never leases more than its ceiling.
 */
const DO_METER_LEASE_BLOCK = 1_000;

/** Target p99 wall-clock for 404 timing-padding (ms). Covers slowest arm. */
const TIMING_PAD_TARGET_MS = 80;
/** Jitter range ±% applied to pad target. */
const TIMING_PAD_JITTER_PCT = 15;
/** Minimum pad (ms) — safety floor so sleep is never negative. */
const TIMING_PAD_MIN_MS = 5;

const ALLOWED_ORIGINS = [
  "https://humangr.com",
  "https://corelink-docs.humangr.com",
];

const CORS_HEADERS: ReadonlyArray<readonly [string, string]> = [
  ["Access-Control-Allow-Methods", "GET, HEAD, POST, PUT, PATCH, DELETE, OPTIONS"],
  ["Access-Control-Allow-Headers", "Authorization, Content-Type, X-Request-Id, Accept"],
  ["Access-Control-Expose-Headers", "X-Request-Id, X-Corelink-Tenant-Id, Server-Timing"],
  ["Access-Control-Max-Age", "86400"],
];

// ──────────────────────────────────────────────────────────────────────────────
// Request ID middleware
// ──────────────────────────────────────────────────────────────────────────────

/**
 * Allowed charset for a propagated `x-request-id` (F-02): RFC-style token
 * bytes only — letters, digits, `.`, `_`, `-`. The supplied id lands in JSON
 * response bodies AND forwarded request headers, so unconstrained values
 * (control chars, newlines, quotes) enable log-injection / header-shape
 * tampering. A value with any other char is rejected and replaced with a
 * freshly generated id.
 */
const REQUEST_ID_CHARSET = /^[A-Za-z0-9._-]+$/;

/** Generate or propagate a request-id. Never exposes body or PII. */
function resolveRequestId(request: Request): string {
  const incoming = request.headers.get("x-request-id");
  if (
    incoming !== null &&
    incoming.length > 0 &&
    incoming.length <= 128 &&
    REQUEST_ID_CHARSET.test(incoming)
  ) {
    return incoming;
  }
  return crypto.randomUUID();
}

// ──────────────────────────────────────────────────────────────────────────────
// CORS helpers
// ──────────────────────────────────────────────────────────────────────────────

function corsOriginHeader(request: Request): string | null {
  const origin = request.headers.get("origin");
  if (origin === null) return null;
  if (ALLOWED_ORIGINS.includes(origin)) return origin;
  return null;
}

function applyCors(response: Response, request: Request): Response {
  const allowed = corsOriginHeader(request);
  if (allowed === null) return response;
  const headers = new Headers(response.headers);
  headers.set("Access-Control-Allow-Origin", allowed);
  headers.set("Vary", "Origin");
  for (const [k, v] of CORS_HEADERS) {
    headers.set(k, v);
  }
  return new Response(response.body, { status: response.status, statusText: response.statusText, headers });
}

function handlePreflight(request: Request): Response | null {
  if (request.method !== "OPTIONS") return null;
  const allowed = corsOriginHeader(request);
  if (allowed === null) return new Response(null, { status: 204 });
  const headers = new Headers();
  headers.set("Access-Control-Allow-Origin", allowed);
  headers.set("Vary", "Origin");
  for (const [k, v] of CORS_HEADERS) {
    headers.set(k, v);
  }
  return new Response(null, { status: 204, headers });
}

// ──────────────────────────────────────────────────────────────────────────────
// Server-trust header hygiene (security: H4)
// ──────────────────────────────────────────────────────────────────────────────

/**
 * Client-suppliable "server-trust" headers that the Worker is solely
 * responsible for establishing (or that must NEVER be client-set). A client
 * could otherwise SMUGGLE these straight through to the DO/container, which
 * trusts some of them on its admin routes and its internal-auth gate.
 *
 * On EVERY path where the Worker forwards a client request by cloning
 * `request.headers`, these MUST be deleted BEFORE the Worker sets its own
 * verified values (delete-then-set). On data-plane paths the Worker does not
 * set internal-auth, so deleting it makes the container's internal-auth-gated
 * admin routes Worker-unreachable by design (operator-only posture).
 *
 * NOTE: `x-corelink-route-kind` is not listed — the Worker unconditionally
 * `.set()`s it on every forward, so any client value is already overwritten.
 * `x-corelink-token-prefix` IS listed (F-012, overnight red-team): the prior
 * "always overwritten" assumption was FALSE on the OCI / billing-webhook / fabric
 * forward arms (which forward raw and never re-set it), so a client could smuggle
 * a forged token-prefix there. The Worker is the sole legitimate setter (from the
 * resolved PAT), so strip any client value structurally on EVERY forward.
 *
 * `x-corelink-tenant-id` IS listed (structural strip): the Worker always
 * `.set()`s it AFTER strip on PAT-backed, internal, onboarding, and fanout
 * forwards; the OCI and billing-webhook carve-outs explicitly `.delete()` it
 * instead (no tenant on those paths). The per-path explicit `.delete()` calls
 * remain as belt-and-braces but the invariant is now structural — a client can
 * never smuggle a forged tenant-id past this list onto any forward path.
 *
 * `x-corelink-scope` (security: H1) IS listed: unlike the always-overwritten
 * headers above, the Worker only `.set()`s scope on PAT-backed forwards, so it
 * MUST be deleted here too — otherwise a client could SMUGGLE a forged scope
 * (e.g. `admin`) straight through on any path. The Worker is the sole setter of
 * the scope value (read from the trusted D1 `pat.scope`), never the client.
 */
/**
 * DSR destructive-arm MFA step-up freshness window, in MINUTES.
 *
 * The `/v1/privacy/*` erasure/rectification gate in the container
 * (`routes/dsr/portal.rs`) is fail-CLOSED on the Worker-trusted
 * `x-corelink-mfa-verified: 1` marker. The Worker is its SOLE setter, and must
 * only stamp it when the Clerk session's factor-verification age is FRESH —
 * otherwise a stolen/XSS/CSRF long-lived dashboard session could trigger
 * irreversible cross-region tenant-data destruction with NO re-auth.
 *
 * Freshness = `clerkAuth.fvaMinutes` (verified `fva[0]`, minutes since the first
 * factor was last verified) `<= MFA_FVA_FRESH_MAX_MINUTES`. `undefined`
 * (absent/malformed `fva`) is treated as NOT fresh (fail-CLOSED, never `0`),
 * mirroring the session/token-exchange freshness signal (lib/session_exchange.ts).
 * The 5-minute window matches the canonical admin step-up TTL
 * (`corelink_auth::webauthn::admin_step_up_default_ttl()` = 300s) — this is the
 * SAME step-up policy, not a new one.
 */
const MFA_FVA_FRESH_MAX_MINUTES = 5;

const CLIENT_TRUST_HEADERS: ReadonlyArray<string> = [
  "x-admin-scope",
  "x-admin-principal",
  "x-admin-tenant",
  "x-corelink-internal-auth",
  "x-corelink-fanout-from",
  "x-corelink-scope",
  // Structural tenant-id strip: the Worker is the SOLE setter of
  // x-corelink-tenant-id (from D1-resolved PAT or server constant) on every
  // forward path. Stripping here means no client can smuggle a forged
  // tenant-id regardless of which path is taken.
  "x-corelink-tenant-id",
  // Storage-quota seed strip (storage-quota fail-open fix): the Worker is the
  // SOLE setter of x-corelink-storage-quota-bytes (the tenant's resolved
  // per-tier storage cap, from QUOTAS[tier].storageBytesMax). The container
  // seeds a fresh tenant_storage_state row's bytes_quota from it, so a forged
  // value could let a client seed an arbitrarily-large (or unlimited "0") cap.
  // Strip any client value on every forward — same posture as the tenant-id.
  STORAGE_QUOTA_HEADER,
  // F1: the forgeable client-supplied XFF must be stripped on every forward —
  // the container's signup rate-limit now reads the server-trusted
  // x-corelink-client-ip (set by the Worker from cf-connecting-ip), never XFF.
  "x-forwarded-for",
  // F1: the Worker is the SOLE setter of x-corelink-client-ip (from the
  // unforgeable cf-connecting-ip). Strip any client-supplied value first so a
  // client can never smuggle a forged client IP past the rate-limiter.
  "x-corelink-client-ip",
  // backlog #29 (residency): the Worker is the SOLE setter of
  // x-corelink-primary-region (from the trusted D1 tenant.primary_region) on the
  // regional fan-out path. Strip any client value on EVERY forward so a client
  // can never smuggle a forged residency macro past the container's residency
  // guard. The local DO/container path never sets it (IAD-resident by default).
  "x-corelink-primary-region",
  // F-012 (overnight red-team): the Worker is the sole legitimate setter of
  // x-corelink-token-prefix (the resolved PAT prefix, for log/rate-limit keying).
  // It was NOT structurally stripped and the OCI/billing/fabric arms forward raw
  // without re-setting it, so a client could smuggle a forged prefix there. Strip
  // on every forward — same posture as x-corelink-tenant-id.
  "x-corelink-token-prefix",
  // cf-multitenant WP5a: the NARROWED runner-job PAT markers. The Worker is the
  // SOLE setter of both — it sets them ONLY when the D1-resolved PAT row carries
  // a non-NULL `runner_job_ac_key`, from that trusted value, never the client.
  // A client MUST NOT be able to smuggle a forged `x-corelink-runner-job` (which
  // would falsely mark its request narrowed — harmless) NOR, more importantly, a
  // forged `x-corelink-ac-key-allow` (which could try to widen/redirect the
  // container's exact-key enforcement). Strip both structurally on EVERY forward
  // so only the Worker's D1-derived values ever reach the container.
  "x-corelink-runner-job",
  "x-corelink-ac-key-allow",
  // anti AC-squat (ac-create-only): the create-only (deny-overwrite) marker the
  // Worker sets ONLY for a genuine runner-job cred (from the trusted D1 runner-job
  // narrowing), never the client. A client MUST NOT be able to smuggle a forged
  // `x-corelink-ac-create-only` (harmless if it self-narrows, but the invariant is
  // that ONLY the Worker sets it). Strip it structurally on EVERY forward so only
  // the Worker's runner-job-derived value ever reaches the container.
  "x-corelink-ac-create-only",
  // DSR portal MFA step-up freshness marker: the Worker is the SOLE setter (it
  // stamps `1` on the /v1/privacy/* plane for an edge-verified Clerk session).
  // A client MUST NOT be able to smuggle a forged `x-corelink-mfa-verified` to
  // bypass the destructive-arm (erasure/rectification) step-up gate in
  // routes/dsr/portal.rs — strip it structurally on EVERY forward.
  "x-corelink-mfa-verified",
  // Team RBAC role (migration 0074): the Worker is the SOLE setter of
  // x-corelink-role (the D1-resolved team_member role — `owner`/`admin`/`member`/
  // `viewer`), forwarded on the customer plane so the container can gate
  // role-restricted operations (e.g. OWNER-only account deletion). A client MUST
  // NOT be able to smuggle a forged role to escalate — strip it structurally on
  // EVERY forward so only the Worker's D1-derived value reaches the container.
  "x-corelink-role",
];

/**
 * Strip every client-suppliable server-trust header from a forwarded request's
 * Headers. Call this BEFORE any `h.set(...)` of Worker-established trust values
 * (delete-then-set) on every DO/container forward path.
 */
function stripClientTrustHeaders(h: Headers): void {
  for (const name of CLIENT_TRUST_HEADERS) {
    h.delete(name);
  }
}

// ──────────────────────────────────────────────────────────────────────────────
// Route table
// ──────────────────────────────────────────────────────────────────────────────

/**
 * Match the request URL against the CoreLink route surface.
 *
 * Route priority (first match wins):
 *   /health                 → health check (legacy body: {"status":"ok",...})
 *   /api/health             → health check alias (body: {"status":"SERVING",...})
 *                              — matches e2e suite Journey 1 exact-string assertion
 *                              `assert_eq!(body["status"], "SERVING")`
 *                              (tests/e2e-user-journeys/src/main.rs:151).
 *   /v2/*                   → OCI Distribution Spec v1.1
 *   /npm/*                  → npm registry proxy
 *   /pip/*                  → PyPI proxy
 *   /brew/*                 → Homebrew tap proxy
 *   /cargo/*                → Cargo registry proxy
 *   /api/v2/*               → REAPI v2 (CoreLink native HTTP API)
 *   /v1/customer/*          → Customer portal (overview, usage, billing, keys, team,
 *                              audit) — dual-auth (WP-1): CoreLink PAT (tenant from
 *                              PAT) OR Clerk session JWT (tenant from clerk_user_id).
 *                              Checked BEFORE the generic /v1/* arm (specificity order).
 *   /v1/users/me            → REAPI v1 — tenant comes from PAT (urlTenant=_anonymous)
 *   /v1/cas/*               → REAPI v1 CAS — tenant from PAT
 *   /v1/admin/*             → REAPI v1 admin — tenant from PAT (admin scope enforced
 *                              in container)
 *   /v1/signup/*            → Pre-tenant signup flow (token in path IS the auth
 *                              artifact, not a Bearer PAT). tenantId=_anonymous.
 *   *                       → not_found
 *
 * Tenant extraction:
 *   - For OCI/npm/pip/brew/cargo: first path segment after the protocol
 *     prefix is the tenant namespace (e.g., `/v2/<tenant>/…`).
 *   - For REAPI v2: `X-Corelink-Tenant-Id` header or first path segment.
 *   - For REAPI v1 (/v1/*): tenant is NOT in URL — resolved by DO from PAT;
 *     Worker uses urlTenant="_anonymous" which preserves any downstream
 *     path-spoof gate semantics (the gate only fires when urlTenant is a
 *     real tenant id; "_anonymous" means "Worker is deferring to PAT").
 *   - For /v1/signup/*: pre-tenant (customer has no tenant yet), so
 *     urlTenant="_anonymous"; DO pins all anon signup traffic to one DO
 *     instance keyed by "_anonymous" (simplest dispatch — no per-anon DO
 *     proliferation; signup throughput is low and rate-limited upstream).
 *   - For /health: tenant = "_system".
 *   - For /api/health: tenant = "_system" (alias).
 */
function matchRoute(url: URL): RouteMatch {
  const path = url.pathname;

  // Container health deep-probe — /_health/container forwards through the
  // _system DO to the container's own /_health endpoint, exposing A4's
  // `storage` field (r2 vs inmemory) that the Worker's fast-path /_health
  // never returns.  Publicly probeable, no auth required.
  if (path === "/_health/container" || path === "/_health/container/") {
    return { tenantId: "_system", pathSuffix: "/_health", routeKind: "health_container" };
  }

  // Health: /health (legacy CF/customer liveness) + /_health (smoke-prod check
  // [2]; the container's DO-side probe uses /_health on the container's private
  // port via getTcpPort, not via this public route — independent paths).
  // Both return the legacy {"status":"ok",...} body; the SERVING alias lives at
  // /api/health below to match the e2e suite's exact-string assertion.
  if (path === "/health" || path === "/health/" || path === "/_health" || path === "/_health/") {
    return { tenantId: "_system", pathSuffix: path, routeKind: "health" };
  }

  // Health alias — /api/health (body: {"status":"SERVING",...})
  // Matches e2e Journey 1 exact-string assertion on "SERVING".
  if (path === "/api/health" || path === "/api/health/") {
    return { tenantId: "_system", pathSuffix: "/api/health", routeKind: "health_serving" };
  }

  // OCI token endpoint — EXACT /token (the second leg of OCI two-leg auth).
  // The OCI client GETs /token with `Authorization: Basic base64(user:<PAT>)`;
  // the container verifies the PAT (Option-B) and mints a short-lived HMAC
  // Bearer. There is NO tenant path segment, so tenantId is the shared "_oci"
  // sentinel (same dedicated DO as /v2/*); the container derives the real
  // tenant from the PAT. The Worker is a pure forwarder here (no PAT gate).
  if (path === "/token") {
    return { tenantId: "_oci", pathSuffix: path, routeKind: "oci_token" };
  }

  // OCI v2 — /v2[/…]
  // OCI has NO tenant path segment: the first /v2/ segment is the repository
  // NAME (e.g. /v2/alpine/blobs/...), NOT a tenant. tenantId is the shared
  // "_oci" sentinel so the dedicated OCI DO is used; the container does its own
  // two-leg auth (Option-B PAT verify at /token, HMAC Bearer on /v2) and
  // derives + namespaces the tenant from the OCI token. pathSuffix is forwarded
  // UNCHANGED.
  if (path === "/v2" || path === "/v2/" || path.startsWith("/v2/")) {
    return { tenantId: "_oci", pathSuffix: path, routeKind: "oci_v2" };
  }

  // Bazel remote cache (REAPI v2) — /bazel/v2/<instance>/...
  // <instance> = tenant_id per CoreLink convention. The container
  // (`crates/corelink-container/src/routes/bazel_v2.rs`) enforces
  // caller_tenant equality against the URL :instance.
  if (path.startsWith("/bazel/v2/")) {
    const rest = path.slice("/bazel/v2/".length);
    const tenant = extractFirstSegment("/" + rest) ?? "_anonymous";
    return { tenantId: tenant, pathSuffix: path, routeKind: "bazel_v2" };
  }

  // Stock-Bazel HTTP cache alias — /bazel/cache/{cas,ac}/<hash>
  // What vanilla `bazel --remote_cache=https://host/bazel/cache` actually sends:
  // GET/PUT on /cas/<hash> and /ac/<hash> with NO tenant segment in the URL.
  // Buck2 is NOT a client of this alias — it speaks REAPI over gRPC only.
  // Unlike /bazel/v2/<instance>/…, the tenant is NOT in the path — it is
  // resolved from the PAT and injected as
  // x-corelink-tenant-id (tenantId="_anonymous" defers to the PAT, exactly like
  // turbo_v8 / reapi_v1, and skips the URL-vs-PAT spoof check). Reuses the
  // bazel_v2 routeKind: same container router, same PAT gate + metering + DO.
  if (path.startsWith("/bazel/cache/")) {
    return { tenantId: "_anonymous", pathSuffix: path, routeKind: "bazel_v2" };
  }

  // Vercel /v8/artifacts (Turborepo remote cache).
  // Hash is the URL leaf; tenant comes from `?teamId=...` query string,
  // resolved by the container handler against caller_tenant.
  if (path.startsWith("/v8/artifacts")) {
    return { tenantId: "_anonymous", pathSuffix: path, routeKind: "turbo_v8" };
  }

  // npm — /npm/<tenant>/…
  if (path.startsWith("/npm/")) {
    const rest = path.slice(5); // strip "/npm/"
    const tenant = extractFirstSegment("/" + rest) ?? "_anonymous";
    return { tenantId: tenant, pathSuffix: path, routeKind: "npm" };
  }

  // pip — /pip/<tenant>/…
  if (path.startsWith("/pip/")) {
    const rest = path.slice(5);
    const tenant = extractFirstSegment("/" + rest) ?? "_anonymous";
    return { tenantId: tenant, pathSuffix: path, routeKind: "pip" };
  }

  // brew — /brew/<tenant>/…
  if (path.startsWith("/brew/")) {
    const rest = path.slice(6);
    const tenant = extractFirstSegment("/" + rest) ?? "_anonymous";
    return { tenantId: tenant, pathSuffix: path, routeKind: "brew" };
  }

  // cargo — /cargo/<tenant>/…
  if (path.startsWith("/cargo/")) {
    const rest = path.slice(7);
    const tenant = extractFirstSegment("/" + rest) ?? "_anonymous";
    return { tenantId: tenant, pathSuffix: path, routeKind: "cargo" };
  }

  // REAPI v2 — /api/v2/…
  if (path.startsWith("/api/v2/")) {
    const rest = path.slice(8);
    const tenant = extractFirstSegment("/" + rest) ?? "_anonymous";
    return { tenantId: tenant, pathSuffix: path, routeKind: "reapi_v2" };
  }

  // REAPI v1 signup — /v1/signup/… (pre-tenant flow; token in path is the auth artifact)
  // Checked BEFORE the generic /v1/* arms so signup never falls into the
  // PAT-required reapi_v1 bucket. tenantId=_anonymous pins all anon signup
  // traffic to one DO instance.
  if (path.startsWith("/v1/signup/") || path === "/v1/signup") {
    return { tenantId: "_anonymous", pathSuffix: path, routeKind: "signup" };
  }

  // REAPI v1 customer portal — /v1/customer/* (overview, usage, billing, keys, team, audit)
  // Checked BEFORE the generic /v1/* arm so customer paths never fall into the
  // reapi_v1 bucket. Tenant is NOT in the URL — resolved by the DO from the PAT
  // (same pattern as reapi_v1 / /v1/users/me). The spoof gate doesn't fire because
  // urlTenant="_anonymous" (no path tenant in /v1/customer/<resource> URLs).
  // Auth: dual (WP-1) — a canonical CoreLink PAT takes the generic PAT gate
  // (unchanged), anything else takes the Clerk-session bridge (edge-verified
  // JWT, tenant from clerk_user_id). Customer routes are NOT pre-tenant
  // (unlike signup).
  if (path.startsWith("/v1/customer/") || path === "/v1/customer") {
    return { tenantId: "_anonymous", pathSuffix: path, routeKind: "customer_v1" };
  }

  // DSR self-service portal — /v1/privacy/dsr/* (access, portability,
  // rectification, erasure, restriction, objection, status, verify-mfa). This is
  // a Clerk-session dashboard surface (the browser holds a Clerk session, not a
  // PAT), so it REUSES the customer_v1 forward: same edge Clerk-verify + tenant
  // resolution + `x-corelink-tenant-id`/`x-corelink-token-prefix: clerk` stamp.
  // The container routes it to routes/dsr/portal.rs by path. Checked BEFORE the
  // generic /v1/* arm so it never falls into the PAT-required reapi_v1 bucket.
  if (path.startsWith("/v1/privacy/") || path === "/v1/privacy") {
    return { tenantId: "_anonymous", pathSuffix: path, routeKind: "customer_v1" };
  }

  // Onboarding — /v1/onboarding/* — Clerk-authenticated self-serve flow (tier
  // select checkout, etc.). The browser presents a Clerk SESSION JWT, not a
  // CoreLink PAT, so this arm is verified at the edge (Clerk JWKS) in the fetch
  // handler — NOT via the PAT path. Tenant is resolved there from the verified
  // Clerk user id (not the URL), so urlTenant stays "_anonymous". Checked BEFORE
  // the generic /v1/* arm so onboarding never falls into the PAT-only bucket.
  if (path.startsWith("/v1/onboarding/") || path === "/v1/onboarding") {
    return { tenantId: "_anonymous", pathSuffix: path, routeKind: "onboarding" };
  }

  // Session→token exchange — EXACT /v1/session/exchange (hugit-P2 WP-C, seam C).
  // The caller presents a Clerk SESSION JWT (server-side only, never client-
  // exposed per ADR-0002); the Worker verifies it at the EDGE (same shared
  // pipeline as onboarding/customer) and mints a short-lived tenant-scoped
  // CoreLink PAT by REUSING the container's audited /_internal/pat/mint route.
  // Tenant is NOT in the URL — resolved from the verified Clerk user id — so
  // urlTenant stays "_anonymous". Checked BEFORE the generic /v1/* arm so it is
  // never swallowed into the PAT-required reapi_v1 bucket (the caller holds a
  // session, not a PAT).
  if (path === "/v1/session/exchange") {
    return { tenantId: "_anonymous", pathSuffix: path, routeKind: "session_exchange" };
  }

  // Internal routes — /_internal/* — gated by X-Corelink-Internal-Auth shared
  // secret. NOT gated by PAT auth. Intended for Worker-to-Worker calls
  // (signup-worker → this Worker → DO → container). The shared-secret check
  // is performed in the fetch handler (not in matchRoute) so the route is
  // never accidentally skipped on 404-padding paths.
  //
  // SECURITY NOTE (F4 — CLOSED 2026-08-19, Inc-2): /_internal/* is no longer
  // publicly reachable unauthenticated. A Cloudflare Access self-hosted app
  // (Service-Auth) gates corelink-api.humangr.com/_internal* at the NETWORK
  // layer — no `CF-Access-Client-Id`/`-Secret` service token ⇒ 403 before this
  // Worker. Same-account callers (signup pat/mint + DSR crons + erase fan-out)
  // use Service Bindings, which bypass the edge/CF Access, so are unaffected.
  // The per-consumer x-corelink-internal-auth compare below stays UNDER Access
  // (defense-in-depth). Runbook: docs/internal/inc2-cf-access-lockdown.md.
  if (path.startsWith("/_internal/")) {
    return { tenantId: "_system", pathSuffix: path, routeKind: "internal" };
  }

  // corelink-runners fabric introspect — EXACT /internal/v1/auth/introspect (no
  // underscore, per the ratified runners contract — distinct from the /_internal/*
  // family above). The container mounts this route and is the SOLE auth authority,
  // gated by FABRIC_INTROSPECT_AUTH_KEY (a DEDICATED secret, NOT the shared
  // CORELINK_INTERNAL_AUTH_KEY of /_internal/*). So the Worker is a pure
  // pass-through: it forwards the caller's x-corelink-internal-auth (the FABRIC
  // secret) UNCHANGED to the _system DO and applies NO edge gate. (Wiring gap from
  // #261: the container had the route but the Worker never forwarded this path,
  // so introspect 404'd end-to-end — fixed 2026-06-13.)
  if (path === "/internal/v1/auth/introspect") {
    return { tenantId: "_system", pathSuffix: path, routeKind: "fabric_introspect" };
  }

  // resolve-tenant — EXACT /internal/v1/auth/resolve-tenant. The container mounts
  // it on the SAME router as introspect and is the SOLE auth authority, gated by
  // the SAME FABRIC_INTROSPECT_AUTH_KEY (+ optional _HUGR). So this reuses the
  // fabric_introspect pass-through: forward the caller's x-corelink-internal-auth
  // UNCHANGED to the _system DO, NO edge gate. Lets a fabric consumer (githugr)
  // resolve clerk_org_id(=sub)→tenant_id for isolation verification + per-tenant
  // reads. (Same #261-class wiring gap as introspect: the container had the route
  // but the Worker never forwarded this path — it 404'd end-to-end until here.)
  if (path === "/internal/v1/auth/resolve-tenant") {
    return { tenantId: "_system", pathSuffix: path, routeKind: "fabric_introspect" };
  }

  // corelink-runners billing usage-push INGEST — EXACT
  // /internal/v1/billing/usage (no underscore, mirrors the fabric introspect
  // contract). The container mounts this route and is the SOLE auth authority,
  // gated by BILLING_INGEST_AUTH_KEY (a DEDICATED secret, NOT the shared
  // CORELINK_INTERNAL_AUTH_KEY of /_internal/*, NOR the FABRIC_INTROSPECT_AUTH_KEY).
  // So the Worker is a pure pass-through: it forwards the caller's
  // x-corelink-internal-auth (the ingest secret) UNCHANGED to the _system DO and
  // applies NO edge gate (mirrors the introspect / billing-webhook carve-outs).
  if (path === "/internal/v1/billing/usage") {
    return { tenantId: "_system", pathSuffix: path, routeKind: "billing_ingest" };
  }

  // githugr authz #3 — tenant lookup. EXACT /internal/v1/auth/tenant/lookup.
  // Handled AT the Worker (not forwarded): a parameterized D1 read of the
  // tenant by clerk_user_id, gated by the shared CORELINK_INTERNAL_AUTH_KEY (the
  // gate runs inside handleTenantLookup). Distinct from /_internal/* (underscore)
  // and from the runners FABRIC introspect secret above.
  if (path === "/internal/v1/auth/tenant/lookup") {
    return { tenantId: "_system", pathSuffix: path, routeKind: "tenant_lookup" };
  }

  // githugr authz #1 — RFC 8693 token exchange. EXACT
  // /internal/v1/auth/token-exchange. Handled AT the Worker: verifies the Clerk
  // session, 403s on session.tenant ≠ audience (the cross-tenant-write
  // rejection), and mints a ~300s tenant-scoped PAT via the container. Internal-
  // auth gated (githugr backend) AND session-gated (user) — both inside the
  // handler. The caller holds a session + the internal secret, not a PAT, so this
  // is matched BEFORE the generic PAT-required /v1/* arms.
  if (path === "/internal/v1/auth/token-exchange") {
    return { tenantId: "_anonymous", pathSuffix: path, routeKind: "token_exchange" };
  }

  // corelink-runners D-9 — per-job runner PAT mint. EXACT
  // /internal/v1/runner/mint. Handled AT the Worker (like token-exchange):
  // internal-auth gated (pat_mint consumer key + shared fallback), runners-
  // entitlement checked, then a short-TTL tenant-scoped PAT is minted via the
  // container's single mint authority. The trusted DISPATCHER calls this — no
  // Clerk session, no PAT — so it is matched BEFORE the generic /v1/* arms and
  // distinct from the /_internal/* (underscore) family.
  if (path === "/internal/v1/runner/mint") {
    return { tenantId: "_system", pathSuffix: path, routeKind: "runner_mint" };
  }

  // corelink-runners D-9 — runner PAT revoke by pat_id. EXACT
  // /internal/v1/runner/revoke. Internal-auth gated (same pat_mint key); reuses
  // the existing `UPDATE pat SET revoked_at_ms` revocation surface. Called by the
  // dispatcher on job teardown (TTL is the backstop).
  if (path === "/internal/v1/runner/revoke") {
    return { tenantId: "_system", pathSuffix: path, routeKind: "runner_revoke" };
  }

  // clw `auth rotate` seam — atomic mint-new + revoke-old. EXACT
  // /internal/v1/auth/rotate. Internal-auth gated (same pat_mint consumer key as
  // runner-mint/revoke); reads the old `pat` row for its tenant + scope, mints an
  // equivalent new PAT via the container's single mint authority, then revokes the
  // old pat_id via the existing `UPDATE pat SET revoked_at_ms` surface. The clw
  // backend (holding the internal key) calls this — no Clerk session, no PAT — so
  // it is matched BEFORE the generic /v1/* arms and distinct from the /_internal/*
  // (underscore) family.
  if (path === "/internal/v1/auth/rotate") {
    return { tenantId: "_system", pathSuffix: path, routeKind: "auth_rotate" };
  }

  // Stripe billing webhook — EXACT /v1/billing/stripe-webhook (mounted in the
  // container at crates/corelink-container/src/webhook.rs). Stripe authenticates
  // with a `Stripe-Signature` HMAC header, NOT a Bearer PAT, so this is a pure
  // pass-through (mirrors the OCI carve-out): the Worker forwards the RAW body +
  // Stripe-Signature to the container, which is the SOLE authority for verifying
  // the signature (constant-time, replay-windowed) and deriving the tenant from
  // the signed event metadata. There is NO URL tenant — route to the shared
  // "_system" DO. Checked BEFORE the generic /v1/* arm so it is never swallowed
  // into the PAT-required reapi_v1 bucket (which would 401 the un-PAT'd webhook).
  if (path === "/v1/billing/stripe-webhook") {
    return { tenantId: "_system", pathSuffix: path, routeKind: "billing_webhook" };
  }

  // PUBLIC erasure-attestation verifier — /v1/public/* (Artifact 1, WP-C1).
  // UNAUTHENTICATED by design: anyone can verify a GDPR erasure offline (fetch the
  // signed bundle + the region public key, recompute the Ed25519 signature). The
  // container mounts these GET routes OUTSIDE its auth/ratelimit/residency layers
  // (same as the /_internal/* family) and is the SOLE authority. So the Worker is a
  // pure pass-through: NO PAT gate, NO internal-auth. Checked BEFORE the generic
  // /v1/* arm so it is never swallowed into the PAT-required reapi_v1 bucket.
  // tenantId="_anonymous" (no tenant in the URL; an erasure proof is public).
  if (path.startsWith("/v1/public/")) {
    return { tenantId: "_anonymous", pathSuffix: path, routeKind: "public_attestation" };
  }

  // REAPI v1 — /v1/users/me, /v1/cas/blobs/<digest>/<size>, /v1/admin/audit/events, …
  // Generic /v1/* fallthrough — only reached when no more-specific arm matched above.
  // Arms checked before this one (specificity order, most-specific first):
  //   1. /v1/signup/*    → "signup"      (pre-tenant, no PAT required)
  //   2. /v1/customer/*  → "customer_v1" (PAT required, tenant from PAT)
  //   3. /v1/*           → "reapi_v1"    ← this arm (PAT required, tenant from PAT)
  // Tenant is NOT in the URL — resolved by the DO from the PAT.
  // urlTenant="_anonymous" preserves the future path-spoof gate semantics
  // (gate only fires when urlTenant is a concrete tenant id).
  if (path.startsWith("/v1/")) {
    return { tenantId: "_anonymous", pathSuffix: path, routeKind: "reapi_v1" };
  }

  return { tenantId: "_system", pathSuffix: path, routeKind: "not_found" };
}

/**
 * Extract the first path segment (no leading slash in result).
 * "/foo/bar/baz" → "foo"
 * "/foo"         → "foo"
 * "/"            → null
 * ""             → null
 */
function extractFirstSegment(path: string): string | null {
  const stripped = path.startsWith("/") ? path.slice(1) : path;
  if (stripped.length === 0) return null;
  const slash = stripped.indexOf("/");
  return slash === -1 ? stripped : stripped.slice(0, slash);
}

// ──────────────────────────────────────────────────────────────────────────────
// Auth middleware
// ──────────────────────────────────────────────────────────────────────────────

/**
 * Extract and validate the Bearer PAT from the Authorization header.
 *
 * Full validation pipeline (WP-A1, P0 wave Phase 1):
 *
 *   1. Extract Bearer token; validate printable ASCII shape.
 *   2. Parse PAT canonical format: `corelink_<env>_<token_id>.<random_secret>.<hmac_sig>`
 *      (95 or 96 chars). Anything that is not this shape → 401.
 *   3. HMAC-SHA256 fast-fail (if PAT_SIGNING_KEY is bound): reject in < 1ms
 *      if the `hmac_sig` segment does not match the signing key. This blocks
 *      random-string brute-force without touching D1 at all.
 *   4. D1 lookup by `token_id` — SELECT tenant_id, expires_ms, scope FROM pat
 *      WHERE token_id = ?1 AND revoked_at_ms IS NULL LIMIT 1. Any token not in
 *      the D1 store → 401. This kills the "any 32–256 char string accepted"
 *      vulnerability. (Expiry is NOT filtered in SQL — it is checked in
 *      application code in step 5 below, so an expired token is fetched then
 *      rejected, distinguishing `pat_expired` from `pat_not_found`.)
 *   5. Expiry check (application-side): reject when expires_ms <= Date.now().
 *   6. Return resolved tenant_id (UUID string from D1 row).
 *
 * Constant-time discipline:
 *   - Token bytes are scanned in full before any early-return.
 *   - HMAC compare uses crypto.subtle.timingSafeEqual.
 *   - SHA-256 prefix for log correlation is computed from the raw token bytes
 *     (non-reversible; see INV-NO-PII-IN-LOGS).
 *
 * NOTE: Argon2id verification is NOT performed in the CF Worker (OWASP-2024
 * cost parameters m=64MiB, t=3, p=4 exceed the Worker's cpu_ms budget) AND
 * is NOT performed by the Durable Object or the native container plane either
 * (F3/F17). Argon2id is wired only for the cache ADAPTER routes (cargo/brew/
 * npm/pip/oci) via adapter_pat::PatVerifier. The native CAS/AC/Bazel/Turbo/
 * customer plane trusts the Worker-injected x-corelink-tenant-id directly.
 * Possession on the native plane therefore rests SOLELY on this HMAC gate
 * (PAT_SIGNING_KEY required). Wiring adapter_pat::PatVerifier onto the native
 * plane as a container-side second layer is tracked as a TODO (F3 fix item 1).
 *
 * The Worker never logs the token value — only a 6-char hashed prefix
 * for correlation tracing.
 *
 * INV-NO-PII-IN-LOGS: tenant ID is NOT logged at this layer; only the DO
 * writes hashed-form tenant IDs to audit events.
 */

/**
 * Decode an HTTP Basic credential (the text AFTER `Basic `) and return the
 * PASSWORD, which on the pip adapter surface IS the PAT: pip/uv emit
 * `https://hugr:<PAT>@host/…` → `Authorization: Basic base64("hugr:<PAT>")`.
 * The username (`hugr`) is a label and is deliberately ignored — possession
 * rests SOLELY on the password being a valid PAT, verified downstream by the
 * exact same HMAC + D1 gate as a Bearer PAT (this function performs NO auth).
 *
 * Returns `null` on any malformed input (not standard base64, non-UTF8 payload,
 * missing `:` separator, or empty password) so the caller rejects with a 401 —
 * there is no path here that yields a usable token from a malformed credential.
 */
function extractBasicAuthPassword(b64: string): string | null {
  let decoded: string;
  try {
    // `atob` yields a binary (latin1) string; a canonical PAT is pure ASCII so
    // this is a faithful round-trip. `atob` throws on non-base64 input → null.
    // Any non-ASCII byte that slips through is caught downstream by extractAuth's
    // printable-ASCII (0x21–0x7e) scan, which rejects the token — no bypass.
    decoded = atob(b64);
  } catch {
    return null;
  }
  const colon = decoded.indexOf(":");
  if (colon < 0) {
    return null;
  }
  const password = decoded.slice(colon + 1);
  if (password.length === 0) {
    return null;
  }
  return password;
}

async function extractAuth(
  request: Request,
  env: Env,
  // pip/uv natively emit ONLY URL-embedded HTTP Basic (`https://hugr:<PAT>@host/…`
  // → `Authorization: Basic base64("hugr:<PAT>")`) and NEVER `Authorization:
  // Bearer` — so the pip adapter surface is unusable unless the Worker accepts
  // Basic. When `allowBasicAuth` is true (set ONLY for the `pip` adapter route by
  // the sole caller), a Basic credential is decoded and its PASSWORD is taken as
  // the PAT, then verified through the EXACT same HMAC + D1 path as a Bearer PAT
  // (username is an ignored label — `hugr`). Defaults to false so every other
  // route keeps rejecting non-Bearer schemes with `invalid_scheme` (no bypass:
  // native CAS/AC, browser, and all other adapters are unchanged).
  allowBasicAuth = false,
  // `ctx.waitUntil`, forwarded to the PAT-verify KV write-behind so it survives
  // the response (a bare fire-and-forget kv.put is cancelled → KV never warms).
  waitUntil?: (p: Promise<unknown>) => void,
): Promise<AuthResult> {
  // F18: PAT_SIGNING_KEY is the SOLE possession gate for the native plane (F3/F17).
  // Fail CLOSED and LOUD when it is absent or too short — never silently skip the
  // HMAC check. The 32-byte minimum mirrors the NIST 128-bit floor for symmetric
  // auth secrets. The caller maps this reason to HTTP 503 so operators are alerted.
  const signingKeyRaw = env.PAT_SIGNING_KEY;
  if (!signingKeyRaw || signingKeyRaw.length === 0) {
    console.error(
      JSON.stringify({
        event: "pat_signing_key_absent",
        severity: "CRITICAL",
        message: "PAT_SIGNING_KEY is unset — extractAuth failing closed (503). Provision the secret and redeploy.",
      }),
    );
    return { ok: false, reason: "signing_key_not_configured" };
  }
  // Validate the decoded key length: the hex string encodes raw bytes, so
  // length/2 gives decoded byte count. A key < 64 hex chars = < 32 bytes.
  if (signingKeyRaw.length < 64) {
    console.error(
      JSON.stringify({
        event: "pat_signing_key_too_short",
        severity: "CRITICAL",
        message: `PAT_SIGNING_KEY decodes to fewer than 32 bytes (hex length ${signingKeyRaw.length}) — failing closed (503).`,
      }),
    );
    return { ok: false, reason: "signing_key_not_configured" };
  }

  const authHeader = request.headers.get("authorization");
  if (authHeader === null || authHeader.length === 0) {
    return { ok: false, reason: "missing_authorization_header" };
  }

  const bearerPrefix = "Bearer ";
  const basicPrefix = "Basic ";
  let token: string;
  if (authHeader.startsWith(bearerPrefix)) {
    token = authHeader.slice(bearerPrefix.length).trim();
  } else if (allowBasicAuth && authHeader.startsWith(basicPrefix)) {
    // pip-only Basic path (see `allowBasicAuth` doc above). Decode the credential
    // and take the PASSWORD as the PAT. Malformed Basic (not base64, non-UTF8, no
    // `:`, or empty password) is rejected with the SAME 401 shape as an
    // unsupported scheme — never a bypass. From here the token flows through the
    // identical length/char/format/HMAC/D1 checks as a Bearer PAT.
    const basicPat = extractBasicAuthPassword(authHeader.slice(basicPrefix.length).trim());
    if (basicPat === null) {
      return { ok: false, reason: "invalid_scheme" };
    }
    token = basicPat;
  } else {
    return { ok: false, reason: "invalid_scheme" };
  }
  if (token.length < 32 || token.length > 256) {
    return { ok: false, reason: "invalid_token_length" };
  }

  // Validate printable ASCII (0x21–0x7E) — no spaces, no control chars.
  // Scan all bytes before early-exit to avoid byte-position timing oracle.
  const enc = new TextEncoder();
  const tokenBytes = enc.encode(token);
  let invalid = 0;
  for (const b of tokenBytes) {
    invalid |= (b < 0x21 || b > 0x7e) ? 1 : 0;
  }
  if (invalid !== 0) {
    return { ok: false, reason: "invalid_token_chars" };
  }

  // Derive a safe log prefix (first 6 chars of base64url of SHA-256 of token).
  // Deterministic per token value but non-reversible.
  const hashBuf = await crypto.subtle.digest("SHA-256", tokenBytes);
  const hashArr = new Uint8Array(hashBuf);
  const tokenPrefix = base64url(hashArr).slice(0, 6);

  // ── Step 2: Parse canonical PAT format ───────────────────────────────────
  // corelink_<env>_<token_id>.<random_secret>.<hmac_sig>
  // Total length: 95 (env=2: ci/ro) or 96 (env=3: pat).
  const parsed = parsePat(token);
  if (parsed === null) {
    // Not a CoreLink PAT format. Reject — we only accept canonical PATs.
    return { ok: false, reason: "invalid_pat_format" };
  }

  // ── Step 3: HMAC-SHA256 fast-fail (PAT_SIGNING_KEY — always required) ───
  // Verify the hmac_sig segment before touching D1. The key is guaranteed
  // non-empty and ≥ 32 decoded bytes by the guard at the top of extractAuth.
  // Pre-image: `<token_id>.<random_secret>` (the same preimage used by the
  // Rust verify crate).
  //
  // ── Possession model (security: H2 — explicit engineering DECISION) ───────
  // This HMAC-SHA256 check IS the sole cryptographic possession gate for the
  // native plane (F3/F17): a caller cannot present a token whose hmac_sig
  // verifies without holding the server signing key — forging a PAT requires
  // that key, not merely a stolen/guessed token_id. This is what makes the
  // scope (H1) and tenant_id we read from D1 trustworthy to forward.
  //
  // We deliberately do NOT additionally Argon2id-verify `random_secret` against
  // the stored `pat_hash` on EVERY request: Argon2id is intentionally expensive
  // (~tens-of-ms) and the Worker runs under a tight cpu_ms budget on the hot
  // cache path — per-request Argon2id would dominate latency for every CAS/AC
  // hit. The HMAC gate already binds possession to the signing key. Per-request
  // Argon2id is acceptable defence-in-depth to add LATER (amortised/cached per
  // token_id) ONLY if signing-key compromise becomes a credible concern — at
  // which point key rotation is the primary response. This is the decided
  // posture, not a TODO. Wiring adapter_pat::PatVerifier onto the native plane
  // as a container-side backstop is tracked as a TODO (F3 fix item 1).
  // Overlap key set: the current key plus any rotation siblings
  // (PAT_SIGNING_KEY_PREV / _NEW). A PAT minted under any of them
  // HMAC-verifies during the rotation overlap window so rotation is not
  // a fleet-wide auth outage.
  //
  // Finding #7 (HIGH) — symmetry with the container's fail-CLOSED stance:
  // a sibling that is ABSENT (unset / empty) is benign and simply skipped,
  // but a sibling that is PRESENT (env var set, non-empty) yet FAILS the
  // validity check (decodes to < 32 bytes / wrong length) is a LOUD FATAL
  // config error — never silently dropped. Silently dropping it would let a
  // one-character typo in a rotation sibling at deploy silently shrink the
  // overlap set (the container's `from_env` already refuses to mount in that
  // case), so we fail CLOSED (503) and alert the operator instead.
  const signingKeySet: string[] = [signingKeyRaw];
  for (const [name, sibling] of [
    ["PAT_SIGNING_KEY_PREV", env.PAT_SIGNING_KEY_PREV],
    ["PAT_SIGNING_KEY_NEW", env.PAT_SIGNING_KEY_NEW],
  ] as const) {
    // ABSENT (undefined / null / empty) → benign skip.
    if (typeof sibling !== "string" || sibling.length === 0) {
      continue;
    }
    // PRESENT but INVALID → LOUD fatal config error, fail CLOSED. Symmetric
    // with the container's `Some(None) => return None` refusal in
    // `adapter_pat::from_env`, which mirrors `hex::decode` + `>= 32 bytes`:
    // a valid signing key is an EVEN-length, all-hex string of >= 64 chars
    // (>= 32 bytes). This rejects ALL of finding #7's named malformations —
    // a short value, an odd-length hex string, and a non-hex typo — none of
    // which must be silently dropped (which would shrink the overlap set).
    const isValidHexKey =
      sibling.length >= 64 &&
      sibling.length % 2 === 0 &&
      /^[0-9a-fA-F]+$/.test(sibling);
    if (!isValidHexKey) {
      console.error(
        JSON.stringify({
          event: "pat_signing_key_sibling_malformed",
          severity: "CRITICAL",
          sibling: name,
          message: `${name} is PRESENT but is not a valid signing key (need an even-length all-hex string of >= 64 chars / >= 32 bytes; got hex length ${sibling.length}) — failing closed (503). Fix or unset the rotation sibling and redeploy.`,
        }),
      );
      return { ok: false, reason: "signing_key_not_configured" };
    }
    signingKeySet.push(sibling);
  }
  const hmacOk = await verifyPatHmacMulti(
    signingKeySet,
    parsed.hmacPreimage,
    parsed.hmacSigBytes,
  );
  if (!hmacOk) {
    return { ok: false, reason: "invalid_pat_hmac" };
  }

  // ── Step 4: PAT-row lookup by token_id (replica-read, cached) ────────────
  // Any token whose token_id is not in the D1 store → 401. This kills the
  // "any 32–256 char string accepted" vulnerability (P0-2). The D1 read
  // (`SELECT … FROM pat WHERE token_id = ?1 AND revoked_at_ms IS NULL`) is the
  // per-request auth cost; trans-continental to the D1 primary it was ~0.5–0.7 s
  // (perf #99). ROOT FIX: route it through a D1 read-replication session
  // (`first-unconstrained` → nearest replica, ~tens of ms globally) with a
  // PRIMARY fallback for read-after-write freshness (a just-minted PAT that has
  // not yet replicated is re-checked on the primary — see readPatRow), so a new
  // token still authenticates immediately while a token revoked on the primary
  // is honored only for the ≤replication-lag window (sub-second; well within
  // ADR-0030's 60 s p99 revocation SLA). Still fronted by the per-isolate 5 s
  // single-flight cache (herd protection). Consulted ONLY here, AFTER the HMAC
  // possession proof; positive results only; D1 stays the source-of-truth.
  // See lib/pat_verify_cache.ts.
  // Feature-detect the Sessions API: on a runtime (or a test double) without
  // read replication, degrade gracefully to the primary handle rather than
  // crash. When present, `first-unconstrained` reads the nearest replica.
  const readSession =
    typeof env.CONFIG_DB.withSession === "function"
      ? env.CONFIG_DB.withSession("first-unconstrained")
      : env.CONFIG_DB;
  // L2: globally-replicated, per-colo KV cache — the latency fix for callers far
  // from the ENAM D1 primary (SAM has no D1 replica region). Feature-detected so
  // a build without the binding just skips L2. Reuses METADATA_KV (`patrow:` prefix).
  const kvBinding = (env as unknown as { METADATA_KV?: KvReader }).METADATA_KV;
  const verify = await verifyPatRowCached(readSession, parsed.tokenId, {
    primaryDb: env.CONFIG_DB,
    ...(kvBinding ? { kv: kvBinding } : {}),
    ...(waitUntil ? { waitUntil } : {}),
  });
  if (verify.kind === "error") {
    // D1 errors (network partition, DB unavailable) must not fail-open. Return a
    // distinct reason; the caller maps d1_lookup_error to 503 (transient,
    // retryable) — still fail-closed (access denied), NOT 401 "bad credentials" (H1).
    return { ok: false, reason: "d1_lookup_error" };
  }
  if (verify.kind === "not_found") {
    // token_id not in D1 (unknown or revoked) — reject.
    return { ok: false, reason: "pat_not_found" };
  }
  const row = verify.row;

  // ── Step 5: Expiry check ──────────────────────────────────────────────────
  // `expires_ms === 0` is the canonical "never expires" sentinel (mint:
  // internal_pat.rs:400) — the container SQL honors it (adapter_pat.rs:115:
  // `expires_ms = 0 OR expires_ms > now`). The edge MUST match, else a no-TTL
  // PAT works in the container but is dead-on-arrival here (split-brain, looks
  // like a forged token). Guard the sentinel.
  if (row.expires_ms !== 0 && row.expires_ms <= Date.now()) {
    return { ok: false, reason: "pat_expired" };
  }

  // ── Step 6: Tenant fast-suspend gate (go-live GAP G4) ─────────────────────
  // A valid, unexpired PAT is NOT sufficient if its tenant has been suspended
  // or erased (tenant_offboarding_state.state ∈ {suspended, erased}, migration
  // 0046). Without this gate a suspended/abusive tenant keeps full CAS/AC read
  // + write access until every one of its PATs is individually revoked. The
  // check is a three-tier read — L1 isolate → L2 KV (`tsusp:` on METADATA_KV) →
  // L3 D1 (the replica session), mirroring the pat read's L2 (ADR-0070) — so it
  // adds no uncached per-request far-D1 round-trip on the SAM hot path. It reads
  // through the same `first-unconstrained` replica session as the pat lookup
  // (perf #99) and, on a cold L1+L2, serves the rest of the colo's traffic from
  // the edge-local KV. A newly-suspended tenant is honored only for the bounded
  // ≤KV-TTL enforcement window (ADR-0070's ratified trade-off; PAT revoke stays
  // the immediate lever). Fail-OPEN on a D1 fault (availability), but a
  // KNOWN-suspended cached value still denies. The caller maps `tenant_suspended`
  // to 403 (fail-closed, distinct from the 401 bad-credential and 503 infra arms).
  if (
    await isTenantSuspended(readSession, row.tenant_id, {
      ...(kvBinding ? { kv: kvBinding } : {}),
      ...(waitUntil ? { waitUntil } : {}),
    })
  ) {
    return { ok: false, reason: "tenant_suspended" };
  }

  // Resolved tenant_id + scope from D1. The Worker forwards `scope` to the
  // container as the server-trusted `x-corelink-scope` header (H1) so scopes
  // written to D1 are actually ENFORCED downstream (previously they were stored
  // but never read here, leaving every PAT effectively unscoped). NULL scope on
  // older rows is normalised to "" so the container sees an explicit value.
  return {
    ok: true,
    tenantId: row.tenant_id,
    tokenPrefix,
    // ADR-0071: a FIND-ONLY PAT (`pat.find_only = 1`, migration 0093) stores the
    // CHECK-safe base `read-only` but is NARROWED here to the find-missing
    // capability ONLY — the Worker (sole `x-corelink-scope` authority) forwards
    // the literal `find-missing`, so the container grants `can_find_missing()`
    // and 403s CAS read/write. NULL/0 = normal PAT → forward the base scope.
    scope: row.find_only === 1 ? "find-missing" : (row.scope ?? ""),
    // WP5a: carry the narrowed runner-job marker (NULL on normal PATs). The
    // forward sites set the runner-job headers only when this is non-NULL.
    runnerJobAcKey: row.runner_job_ac_key ?? null,
    // Observability only (Server-Timing `auth` desc) — which tier served the row.
    patSource: verify.source,
  };
}

// ──────────────────────────────────────────────────────────────────────────────
// PAT format parsing (pure TS port of crates/corelink-pat/src/format.rs)
// ──────────────────────────────────────────────────────────────────────────────

/** Parsed PAT segments needed by the Worker auth path. */
interface PatParsed {
  /** 16-char Crockford b32 token_id (D1 lookup key). */
  readonly tokenId: string;
  /** The HMAC preimage: bytes of `<token_id>.<random_secret>`. */
  readonly hmacPreimage: Uint8Array;
  /** 16 raw bytes of the HMAC-SHA256 truncated signature. */
  readonly hmacSigBytes: Uint8Array;
}

/**
 * Parse a CoreLink PAT plaintext `corelink_<env>_<token_id>.<random_secret>.<hmac_sig>`.
 *
 * Returns null for any non-canonical input (wrong prefix, wrong length,
 * wrong charset, wrong env) — the caller treats null as auth failure.
 *
 * Constant-time notes: this function performs character-by-character scans in
 * fixed loops (no short-circuit on first bad byte) for the prefix and env
 * segments, consistent with the Rust `parse_plaintext` constant-time contract.
 * Full timing-indistinguishability across all valid env variants is preserved
 * by running all three env comparisons before selecting a match.
 */
function parsePat(token: string): PatParsed | null {
  // Canonical total lengths: 95 (env=2: "ci"/"ro") or 96 (env=3: "pat").
  const totalLen = token.length;
  if (totalLen !== 95 && totalLen !== 96) {
    return null;
  }
  const envLen = totalLen === 95 ? 2 : 3;

  // Prefix scan: constant-time byte-by-byte over all 9 chars.
  const PREFIX = "corelink_";
  let prefixBad = 0;
  for (let i = 0; i < PREFIX.length; i++) {
    prefixBad |= token.charCodeAt(i) !== PREFIX.charCodeAt(i) ? 1 : 0;
  }
  if (prefixBad !== 0) {
    return null;
  }

  // Env segment: compare all valid literals (constant-time — all three run
  // regardless of match to prevent timing discrimination between valid envs).
  const envStart = PREFIX.length; // 9
  const envSeg = token.slice(envStart, envStart + envLen);
  const ENV_2 = ["ci", "ro"] as const;
  const ENV_3 = ["pat"] as const;
  let envMatched = 0;
  if (envLen === 2) {
    for (const e of ENV_2) {
      envMatched |= ctEqStr(envSeg, e) ? 1 : 0;
    }
    // Also run the length-3 comparison to equalize timing across envLen branches.
    ctEqStr("pat", "pat"); // no-op; keeps the compiler from eliding the path
  } else {
    // envLen === 3
    for (const e of ENV_3) {
      envMatched |= ctEqStr(envSeg, e) ? 1 : 0;
    }
    // Run length-2 comparisons to equalize timing.
    for (const e of ENV_2) {
      ctEqStr(envSeg.slice(0, 2), e); // discarded result
    }
  }
  if (envMatched === 0) {
    return null;
  }

  // Separator `_` after env.
  const sepAfterEnvIdx = envStart + envLen;
  if (token.charCodeAt(sepAfterEnvIdx) !== 0x5f /* '_' */) {
    return null;
  }

  // token_id: 16 Crockford b32 chars.
  const TOKEN_ID_LEN = 16;
  const tokenIdStart = sepAfterEnvIdx + 1;
  const tokenIdEnd = tokenIdStart + TOKEN_ID_LEN;
  const tokenIdSeg = token.slice(tokenIdStart, tokenIdEnd);
  if (!isCrockfordB32(tokenIdSeg)) {
    return null;
  }

  // Separator `.` after token_id.
  if (token.charCodeAt(tokenIdEnd) !== 0x2e /* '.' */) {
    return null;
  }

  // random_secret: 43 base64url-no-pad chars.
  const SECRET_LEN = 43;
  const secretStart = tokenIdEnd + 1;
  const secretEnd = secretStart + SECRET_LEN;
  const secretSeg = token.slice(secretStart, secretEnd);
  if (!isBase64Url(secretSeg)) {
    return null;
  }

  // Separator `.` after random_secret.
  if (token.charCodeAt(secretEnd) !== 0x2e /* '.' */) {
    return null;
  }

  // hmac_sig: 22 base64url-no-pad chars.
  const SIG_LEN = 22;
  const sigStart = secretEnd + 1;
  const sigEnd = sigStart + SIG_LEN;
  if (sigEnd !== totalLen) {
    return null;
  }
  const sigSeg = token.slice(sigStart, sigEnd);
  if (!isBase64Url(sigSeg)) {
    return null;
  }

  // Decode hmac_sig bytes (16 raw bytes from 22 base64url chars).
  const hmacSigBytes = base64urlDecode(sigSeg);
  if (hmacSigBytes === null || hmacSigBytes.length !== 16) {
    return null;
  }

  // HMAC preimage = bytes of `<token_id>.<random_secret>`.
  const enc = new TextEncoder();
  const hmacPreimage = enc.encode(token.slice(tokenIdStart, secretEnd));

  return { tokenId: tokenIdSeg, hmacPreimage, hmacSigBytes };
}

/**
 * Constant-time string equality. Runs to completion even on a mismatch.
 * Both strings must have the same length for a meaningful comparison;
 * length-mismatched inputs always return false (constant-time: the loop
 * still runs for `a.length` iterations).
 */
function ctEqStr(a: string, b: string): boolean {
  if (a.length !== b.length) {
    return false;
  }
  let diff = 0;
  for (let i = 0; i < a.length; i++) {
    diff |= (a.charCodeAt(i) ^ (b.charCodeAt(i)));
  }
  return diff === 0;
}

/** Return true if every character in `s` is a valid Crockford b32 char (uppercase). */
function isCrockfordB32(s: string): boolean {
  for (let i = 0; i < s.length; i++) {
    const c = s.charCodeAt(i);
    // 0-9: 0x30..0x39; A-H: 0x41..0x48; J,K: 0x4A,0x4B; M,N: 0x4D,0x4E;
    // P-T: 0x50..0x54; V-Z: 0x56..0x5A  (excludes I=0x49, L=0x4C, O=0x4F, U=0x55)
    const valid =
      (c >= 0x30 && c <= 0x39) || // 0-9
      (c >= 0x41 && c <= 0x48) || // A-H
      c === 0x4a || c === 0x4b || // J,K
      c === 0x4d || c === 0x4e || // M,N
      (c >= 0x50 && c <= 0x54) || // P-T
      (c >= 0x56 && c <= 0x5a);   // V-Z
    if (!valid) return false;
  }
  return true;
}

/** Return true if every character in `s` is a valid base64url-no-pad char. */
function isBase64Url(s: string): boolean {
  for (let i = 0; i < s.length; i++) {
    const c = s.charCodeAt(i);
    const valid =
      (c >= 0x41 && c <= 0x5a) || // A-Z
      (c >= 0x61 && c <= 0x7a) || // a-z
      (c >= 0x30 && c <= 0x39) || // 0-9
      c === 0x2d ||                // -
      c === 0x5f;                  // _
    if (!valid) return false;
  }
  return true;
}

/** Decode a base64url-no-pad string to bytes. Returns null on decode error. */
function base64urlDecode(s: string): Uint8Array | null {
  try {
    // Re-pad to standard base64 then decode.
    const padded = s.replace(/-/g, "+").replace(/_/g, "/");
    const rem = padded.length % 4;
    const padded2 = rem === 0 ? padded : padded + "=".repeat(4 - rem);
    const binary = atob(padded2);
    const bytes = new Uint8Array(binary.length);
    for (let i = 0; i < binary.length; i++) {
      bytes[i] = binary.charCodeAt(i);
    }
    return bytes;
  } catch {
    return null;
  }
}

// ──────────────────────────────────────────────────────────────────────────────
// HMAC-SHA256 PAT fast-fail (WebCrypto)
// ──────────────────────────────────────────────────────────────────────────────

/**
 * Verify the PAT HMAC-SHA256 signature using the Worker's PAT_SIGNING_KEY.
 *
 * The signing key is a hex-encoded byte string (≥ 32 bytes decoded).
 * Pre-image: the `hmacPreimage` bytes (`<token_id>.<random_secret>`).
 * Expected: the first 16 raw bytes of HMAC-SHA256(key, preimage).
 *
 * The comparison uses crypto.subtle.timingSafeEqual for constant-time
 * equality — constant-time compare of the 16-byte truncated MAC.
 *
 * Returns true only if the MAC matches; false on any mismatch or error.
 */
/**
 * Verify the PAT HMAC against an OVERLAP KEY SET (key_management.md §3.2.1).
 *
 * Mirrors the Rust `verify_hmac_sig_multi`: a PAT minted under ANY key in the
 * set verifies, so an operator can rotate PAT_SIGNING_KEY (incl. rotate-on-
 * compromise) while old-key tokens stay valid through the overlap window —
 * instead of an instant fleet-wide auth outage.
 *
 * Constant-time discipline: every key is evaluated (no early-return on the
 * first match) and the per-key results are folded with a bitwise OR, so the
 * observable latency does not leak WHICH key matched (which would reveal
 * whether a token is on the old vs. new key during rotation). Each per-key
 * compare itself uses crypto.subtle.timingSafeEqual.
 *
 * Fail-closed: an empty key set returns false (nothing verifies). The caller
 * guarantees the current key is present and well-formed before calling.
 */
async function verifyPatHmacMulti(
  signingKeysHex: string[],
  hmacPreimage: Uint8Array,
  expectedSigBytes: Uint8Array,
): Promise<boolean> {
  // Fail-closed: no key set bound ⇒ nothing verifies.
  if (signingKeysHex.length === 0) {
    return false;
  }
  let matched = 0;
  for (const keyHex of signingKeysHex) {
    // Do NOT early-return on a match: fold every key so the matching-key
    // identity does not leak via timing.
    const ok = await verifyPatHmac(keyHex, hmacPreimage, expectedSigBytes);
    matched |= ok ? 1 : 0;
  }
  return matched !== 0;
}

async function verifyPatHmac(
  signingKeyHex: string,
  hmacPreimage: Uint8Array,
  expectedSigBytes: Uint8Array,
): Promise<boolean> {
  try {
    // Decode hex signing key.
    const keyBytes = hexDecode(signingKeyHex);
    if (keyBytes === null || keyBytes.length < 32) {
      return false;
    }

    // Import the key for HMAC-SHA256.
    const cryptoKey = await crypto.subtle.importKey(
      "raw",
      keyBytes,
      { name: "HMAC", hash: "SHA-256" },
      false,
      ["sign"],
    );

    // Compute full HMAC-SHA256 over the preimage.
    const macBuf = await crypto.subtle.sign("HMAC", cryptoKey, hmacPreimage);
    // Truncate to first 16 bytes (128-bit truncated MAC per auth_model.md §2.2).
    const macBytes = new Uint8Array(macBuf, 0, 16);

    // Constant-time equality compare of 16-byte buffers.
    return crypto.subtle.timingSafeEqual(macBytes, expectedSigBytes);
  } catch {
    return false;
  }
}

/** Decode a hex-encoded string to bytes. Returns null on invalid input. */
function hexDecode(hex: string): Uint8Array | null {
  if (hex.length % 2 !== 0) return null;
  const bytes = new Uint8Array(hex.length / 2);
  for (let i = 0; i < hex.length; i += 2) {
    const hi = hexNibble(hex.charCodeAt(i));
    const lo = hexNibble(hex.charCodeAt(i + 1));
    if (hi === -1 || lo === -1) return null;
    bytes[i >> 1] = (hi << 4) | lo;
  }
  return bytes;
}

function hexNibble(c: number): number {
  if (c >= 0x30 && c <= 0x39) return c - 0x30;      // '0'-'9'
  if (c >= 0x61 && c <= 0x66) return c - 0x61 + 10; // 'a'-'f'
  if (c >= 0x41 && c <= 0x46) return c - 0x41 + 10; // 'A'-'F'
  return -1;
}

/** Encode a Uint8Array to base64url (no padding). */
function base64url(bytes: Uint8Array): string {
  let bin = "";
  for (let i = 0; i < bytes.length; i++) {
    bin += String.fromCharCode(bytes[i] ?? 0);
  }
  return btoa(bin).replace(/\+/g, "-").replace(/\//g, "_").replace(/=/g, "");
}

// ──────────────────────────────────────────────────────────────────────────────
// Timing padding for 404 responses (design-pattern-01)
// ──────────────────────────────────────────────────────────────────────────────

/**
 * Pad the wall-clock of a 404 response to a constant-ish target.
 *
 * Mirrors the Rust Tower layer's approach:
 *   pad_target = TIMING_PAD_TARGET_MS ± jitter%
 *   sleep_until(requestStart + pad_target)
 *
 * Jitter is seeded from (server_nonce XOR request counter XOR requestId-hash)
 * to prevent attacker-controlled-seed attacks (design-pattern-01 §3.1).
 *
 * Note: Worker CPU time ≠ wall time, but `await`ing a scheduler-based
 * sleep is the correct primitive (zero CPU spin).
 */
async function applyTimingPad(
  requestStart: number,
  requestId: string,
  requestCounter: number,
  serverNonce: number,
): Promise<void> {
  const enc = new TextEncoder();
  const idHash = await crypto.subtle.digest("SHA-256", enc.encode(requestId));
  const idView = new DataView(idHash);
  const idSeed = idView.getUint32(0, true);

  // Triple-source seed mix (mirrors padding.rs:74-93 splitmix_u64 logic)
  const seed = splitmix32(serverNonce ^ (requestCounter & 0xffffffff) ^ idSeed);
  // seed is 0..2^32; map to [-jitter, +jitter] pct
  const jitter = ((seed / 0xffffffff) * 2 - 1) * TIMING_PAD_JITTER_PCT;
  const padMs = Math.max(
    TIMING_PAD_MIN_MS,
    TIMING_PAD_TARGET_MS * (1 + jitter / 100),
  );

  const deadline = requestStart + padMs;
  const remaining = deadline - Date.now();
  if (remaining > 0) {
    await new Promise<void>((resolve) => setTimeout(resolve, remaining));
  }
}

/** splitmix32 finalizer — single round. */
function splitmix32(x: number): number {
  let z = (x + 0x9e3779b9) | 0;
  z = Math.imul(z ^ (z >>> 16), 0x85ebca6b);
  z = Math.imul(z ^ (z >>> 13), 0xc2b2ae35);
  return (z ^ (z >>> 16)) >>> 0;
}

// ──────────────────────────────────────────────────────────────────────────────
// Error envelope builders (REAPI error shapes)
// ──────────────────────────────────────────────────────────────────────────────

interface ReapiErrorEnvelope {
  readonly error: string;
  readonly message: string;
  readonly request_id: string;
}

function reapiError(error: string, message: string, status: number, requestId: string): Response {
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
let requestCounter = 0;

function getServerNonce(): number {
  if (serverNonce === null) {
    const buf = new Uint32Array(1);
    crypto.getRandomValues(buf);
    serverNonce = buf[0] ?? 0;
  }
  return serverNonce;
}

// ──────────────────────────────────────────────────────────────────────────────
// Main fetch handler
// ──────────────────────────────────────────────────────────────────────────────

const baseHandler: ExportedHandler<Env> = {
  async fetch(request: Request, env: Env, ctx: ExecutionContext): Promise<Response> {
    const requestStart = Date.now();
    // Server-Timing phase clocks (observability — clw latency probe self-serve).
    // Date.now() in a Worker advances only across I/O (Spectre coarsening), which
    // is exactly what we want: these deltas measure the wall time spent in the
    // auth read, the worker-side D1 reads, and the DO/container subrequest — the
    // three phases that split "is it auth or the downstream D1 reads". 0 = phase
    // not reached (e.g. anon/error paths that never emit the header).
    let stAuthStart = 0;
    let stAuthEnd = 0;
    let stOriginStart = 0;
    let stOriginEnd = 0;
    let stPatSource: "l1" | "kv" | "d1" | undefined;
    // `wdb` sub-phase clocks — the THREE awaits between `auth` and `origin`
    // (tier resolve → the batched metering-UPSERT + storage-SUM round trip →
    // residency resolve).
    //
    // It was four, and attributing them is what produced this shape. The
    // 2026-08-04 prod measurement (warm, n=30) read `qmeter` 152/158/163 ms,
    // `qstor` 120/126/130 ms, `qtier` 0/0/3 ms, `qresid` 0/0/2 ms against a
    // `wdb` of 277/284/302 ms — sum(4) − wdb = 0 on 30 of 30 requests. So `wdb`
    // was entirely TWO serial uncached round trips to the ENAM D1 primary, and
    // the two cache-backed phases cost nothing. The two now travel as a single
    // `db.batch` and report as one phase, `qbatch`: the header describes the
    // round trips that actually happen, which is the only reason to trust it.
    //
    // `-1` = "this phase did not run" (anon/system tenant, an early 429, or —
    // for `qbatch` — both of its statements skipped, so no round trip was
    // issued). That sentinel is what lets `dur=0` MEAN "ran, and cost less
    // than the clock can see" instead of being indistinguishable from a phase
    // that never executed — the ambiguity that forced an inference from the
    // `auth` phase's 3-of-30 emission rate in probe run 30916725902.
    let stQTierMs = -1;
    let stQBatchMs = -1;
    let stQResidMs = -1;
    const requestId = resolveRequestId(request);
    requestCounter = (requestCounter + 1) | 0;

    // CORS preflight
    const preflight = handlePreflight(request);
    if (preflight !== null) {
      return preflight;
    }

    const url = new URL(request.url);
    const route = matchRoute(url);

    // Health check — no auth required, no DO forwarding
    // Legacy /health keeps {"status":"ok",...}; /api/health returns "SERVING"
    // to match the e2e suite's Journey 1 exact-string assertion
    // (tests/e2e-user-journeys/src/main.rs:151).
    if (route.routeKind === "health" || route.routeKind === "health_serving") {
      const statusLiteral = route.routeKind === "health_serving" ? "SERVING" : "ok";
      // F19: omit `env` — deployment environment must not be disclosed on
      // unauthenticated endpoints. Serve env detail only on internal/authed paths.
      const body = JSON.stringify({ status: statusLiteral });
      const resp = new Response(body, {
        status: 200,
        headers: {
          "Content-Type": "application/json",
          "X-Request-Id": requestId,
        },
      });
      return applyCors(resp, request);
    }

    // Container health deep-probe — /_health/container — no auth required.
    // Forwards to the container's /_health via the _system DO.
    //
    // Security (L1): the container body includes a `storage` field (`r2` vs
    // `inmemory`) that leaks when prod cold-starts into the InMemory fallback.
    // Strip `storage` from the JSON before returning to unauthenticated callers:
    // parse the container's JSON response, delete `storage`, re-serialize. The
    // liveness `status` field is preserved so monitoring tools still work.
    if (route.routeKind === "health_container") {
      const systemDoId = env.CORELINK_SERVER.idFromName("_system");
      const systemStub = env.CORELINK_SERVER.get(systemDoId, serverGetOpts(env));
      const containerHealthUrl = new URL(request.url);
      containerHealthUrl.pathname = "/_health";
      const containerReq = new Request(containerHealthUrl.toString(), {
        method: "GET",
        headers: (() => {
          const h = new Headers();
          h.set("x-request-id", requestId);
          h.set("x-corelink-route-kind", "health_container");
          h.set("x-corelink-tenant-id", "_system");
          return h;
        })(),
      });
      let containerResp: Response;
      try {
        containerResp = await systemStub.fetch(containerReq);
      } catch (err: unknown) {
        const message = err instanceof Error ? err.message : "unknown error";
        console.error(`[${requestId}] health_container DO fetch failed: ${message.slice(0, 80)}`);
        return applyCors(
          reapiError("INTERNAL_ERROR", "container health upstream error", 500, requestId),
          request,
        );
      }
      const containerHeaders = new Headers(containerResp.headers);
      if (!containerHeaders.has("x-request-id")) {
        containerHeaders.set("x-request-id", requestId);
      }
      // Strip the `storage` field (L1 fix): parse JSON, delete `storage`,
      // re-serialize. If the body is not valid JSON (container returned an
      // error body or non-JSON), pass it through unmodified — liveness
      // semantics are preserved by the upstream status code.
      let redactedBody: BodyInit;
      try {
        const raw = await containerResp.json() as Record<string, unknown>;
        delete raw["storage"];
        redactedBody = JSON.stringify(raw);
        containerHeaders.set("Content-Type", "application/json");
      } catch {
        // Non-JSON body (e.g. container down, returned plain-text error):
        // fall back to streaming the raw body through without redaction.
        redactedBody = containerResp.body ?? "";
      }
      return applyCors(
        new Response(redactedBody, {
          status: containerResp.status,
          statusText: containerResp.statusText,
          headers: containerHeaders,
        }),
        request,
      );
    }

    // Internal routes — `/_internal/*` — authenticated by X-Corelink-Internal-Auth.
    // Bypasses PAT auth entirely; DO forwards directly to the container.
    //
    // Security (red-team #3): the gate uses the PER-CONSUMER key split, mirroring
    // the container's Rust split — a leak of one consumer's secret must not unlock
    // every internal surface. The consumer is derived from the path prefix; each
    // consumer key falls back to the shared CORELINK_INTERNAL_AUTH_KEY when its
    // dedicated key is UNSET (resolveConsumerKey) — a dedicated key that is set
    // but under the floor is REFUSED rather than widened to the shared key. If
    // neither qualifies, deny (fail-CLOSED — never open an unauthenticated proxy).
    if (route.routeKind === "internal") {
      const internalConsumer = internalConsumerForPath(route.pathSuffix);
      const internalAuthKey = resolveConsumerKey(env, internalConsumer);
      if (!internalAuthKey || internalAuthKey.length === 0) {
        // No properly sized key bound for this consumer — deny (fail-CLOSED).
        return applyCors(
          reapiError("FORBIDDEN", "internal route unavailable", 403, requestId),
          request,
        );
      }
      // Verify the caller supplied the correct shared secret (constant-time).
      const provided = request.headers.get("x-corelink-internal-auth") ?? "";
      const enc2 = new TextEncoder();
      const expectedBytes = enc2.encode(internalAuthKey);
      const providedBytes = enc2.encode(provided);
      // Constant-time auth WITHOUT a secret-length oracle. The previous
      // `providedBytes.length === expectedBytes.length` branch took a timing
      // path that depended on the PROVIDED length (and the else-branch compared
      // the secret to itself, not to the provided bytes) — both distinguishable
      // → a length oracle. Instead: copy the provided bytes into a fixed buffer
      // sized to the EXPECTED length (pad with zeros / truncate the overflow),
      // run exactly ONE timingSafeEqual over equal-length buffers, then AND with
      // a constant-time length-equality bit. No early branch depends on the
      // provided length. Mirrors the padded Rust internal_pat.rs/admin.rs gates.
      const fixed = new Uint8Array(expectedBytes.length);
      const copyLen =
        providedBytes.length < expectedBytes.length
          ? providedBytes.length
          : expectedBytes.length;
      fixed.set(providedBytes.subarray(0, copyLen));
      const bytesEqual = crypto.subtle.timingSafeEqual(fixed, expectedBytes);
      // Length-equality bit — a length mismatch can never authenticate (a wrong
      // length that pads to the same prefix bytes is still rejected). This is a
      // single integer compare, not a per-character path, so it carries no
      // length oracle: timingSafeEqual already ran over equal-length buffers.
      const lenEqual = providedBytes.length === expectedBytes.length;
      const authOk = bytesEqual && lenEqual;
      if (!authOk) {
        return applyCors(
          reapiError("UNAUTHORIZED", "internal auth required", 401, requestId),
          request,
        );
      }

      // WI-MULTI-REGION-V1 — the replication-coordinator singleton DO.
      // `/_internal/replication/*` is served by ReplicationCoordinatorDO (NOT the
      // container): the SINGLE global instance
      // (idFromName(REPLICATION_COORDINATOR_SINGLETON)) whose single-writer
      // guarantee IS the split-brain-safe promotion lock. We intercept it here —
      // AFTER the shared internal-auth gate above, BEFORE the generic container
      // forward — mapping `/_internal/replication/<op>` → the DO's `/_repl/<op>`.
      if (route.pathSuffix.startsWith("/_internal/replication/")) {
        if (!env.REPLICATION_COORDINATOR_DO) {
          return applyCors(
            reapiError("NOT_IMPLEMENTED", "replication coordinator DO not bound", 501, requestId),
            request,
          );
        }
        const coordId = env.REPLICATION_COORDINATOR_DO.idFromName(REPLICATION_COORDINATOR_SINGLETON);
        const coordStub = env.REPLICATION_COORDINATOR_DO.get(coordId);
        const coordUrl = new URL(request.url);
        coordUrl.pathname = route.pathSuffix.replace("/_internal/replication", "/_repl");
        const coordHeaders = new Headers(request.headers);
        stripClientTrustHeaders(coordHeaders);
        coordHeaders.set("x-request-id", requestId);
        const coordReq = new Request(coordUrl.toString(), {
          method: request.method,
          headers: coordHeaders,
          body:
            request.method === "GET" || request.method === "HEAD"
              ? undefined
              : await request.clone().arrayBuffer(),
        });
        return applyCors(await coordStub.fetch(coordReq), request);
      }

      // DO D1-PLACEMENT PROBE — `/_internal/do-d1-probe/{tenant_id}` → that
      // tenant's CoreLinkServer DO `/_do/health`. Same shape as the replication
      // forward above (rewrite the path, strip client trust headers, forward to
      // a DO stub, return its body verbatim), intercepted AFTER the internal-auth
      // gate and BEFORE the generic `_system` container forward.
      //
      // WHY THE TENANT IS IN THE PATH: DO placement is per-DO-id. The id is
      // derived here with `env.CORELINK_SERVER.idFromName(tenantId)` — the
      // IDENTICAL derivation the tenant data path uses ("Route to the per-tenant
      // DO" below), against the same namespace binding in the same Worker, so
      // this reaches the SAME DO instance that serves that tenant's traffic (a
      // fresh DO would measure a different placement and answer nothing).
      //
      // CAVEAT (must be understood before trusting the number): a tenant whose
      // `primary_region` maps to a non-IAD colo is served by a REGIONAL Worker
      // via a Service Binding (see the residency fan-out below), and that
      // regional Worker has its OWN CORELINK_SERVER namespace — so its DO is a
      // different instance living in a different region. `/_internal/*` does NOT
      // fan out. For such a tenant, probe the REGIONAL Worker, not this one.
      if (route.pathSuffix.startsWith(INTERNAL_DO_D1_PROBE_PREFIX)) {
        const rawTenant = route.pathSuffix.slice(INTERNAL_DO_D1_PROBE_PREFIX.length);
        let probeTenantId: string;
        try {
          probeTenantId = decodeURIComponent(rawTenant);
        } catch {
          probeTenantId = "";
        }
        if (probeTenantId.length === 0 || probeTenantId.includes("/")) {
          return applyCors(
            reapiError(
              "INVALID_ARGUMENT",
              "usage: /_internal/do-d1-probe/{tenant_id}[?colo=1]",
              400,
              requestId,
            ),
            request,
          );
        }
        const probeDoId = env.CORELINK_SERVER.idFromName(probeTenantId);
        const probeStub = env.CORELINK_SERVER.get(probeDoId, serverGetOpts(env));
        const probeUrl = new URL(request.url);
        probeUrl.pathname = "/_do/health";
        const probeHeaders = new Headers(request.headers);
        // Strip every client-suppliable trust header (incl. the caller's
        // internal-auth secret and any x-corelink-tenant-id): the probe reads a
        // health page, it must never smuggle authority into the DO.
        stripClientTrustHeaders(probeHeaders);
        probeHeaders.set("x-request-id", requestId);
        // Always GET: /_do/health is a read, and this must not be a body-carrying
        // path into the DO.
        const probeReq = new Request(probeUrl.toString(), {
          method: "GET",
          headers: probeHeaders,
        });
        // Body verbatim. NOTE: /_do/health answers 503 while the tenant's
        // container is not running — the d1_probe numbers are still in the body,
        // so read the BODY, not the status.
        return applyCors(await probeStub.fetch(probeReq), request);
      }

      // Route to the _system DO which hosts the LOCAL (this-region) container.
      const systemDoId = env.CORELINK_SERVER.idFromName("_system");
      const systemStub = env.CORELINK_SERVER.get(systemDoId, serverGetOpts(env));
      // Build server-trusted internal headers (client trust headers stripped, then
      // re-established). `fanoutFrom`, when set, marks a request as ALREADY fanned
      // out so the receiving regional worker does not re-fan (loop guard).
      const buildInternalHeaders = (src: Headers, fanoutFrom?: string): Headers => {
        const h = new Headers(src);
        // Strip ALL client-suppliable trust headers BEFORE re-establishing them
        // (delete-then-set): a client must never smuggle x-admin-* / fanout-from,
        // nor a forged internal-auth.
        stripClientTrustHeaders(h);
        h.set("x-request-id", requestId);
        h.set("x-corelink-route-kind", "internal");
        h.set("x-corelink-tenant-id", "_system");
        h.set("x-corelink-token-prefix", "internal");
        h.set("x-corelink-internal-auth", internalAuthKey);
        if (fanoutFrom) h.set("x-corelink-fanout-from", fanoutFrom);
        return h;
      };

      // ── Operator container force-recycle ──────────────────────────────────────
      // `POST /_internal/admin/recycle-system` destroys the `_system` container so
      // the NEXT request boots it FRESH — the only reliable way to apply a rotated
      // START-TIME secret (e.g. a rotated CORELINK_ERASE_AUTH_KEY) to an
      // already-running DO container. A container-app ROLLOUT does NOT restart a
      // live DO container, and a hot `_system` container does not self-cycle within
      // any useful window (observed: 44+ min uptime past a key rotation), so a
      // rotated erase key silently 401s every fan-out leg until forced. Reuses the
      // existing DO management path `/_do/stop` (handleStop → destroyContainer →
      // container.destroy()); the operator gate is THIS apex internal-auth check
      // (admin consumer key). Best-effort + idempotent: destroying a cold/fresh
      // container is harmless (it just reboots), so — unlike the erase fan-out — it
      // reports per-region status instead of failing closed. Fans to every regional
      // worker so one call converges the whole fleet onto the current start-env.
      if (route.pathSuffix === "/_internal/admin/recycle-system") {
        const stopLocal = async (): Promise<boolean> => {
          const stopUrl = new URL(request.url);
          stopUrl.pathname = "/_do/stop";
          try {
            const r = await systemStub.fetch(
              new Request(stopUrl.toString(), {
                method: "POST",
                headers: buildInternalHeaders(request.headers),
              }),
            );
            return r.ok;
          } catch (err: unknown) {
            const m = err instanceof Error ? err.message : "unknown error";
            console.error(
              `[${requestId}] recycle-system local /_do/stop threw: ${m.slice(0, 80)}`,
            );
            return false;
          }
        };
        const jsonResp = (obj: unknown, status: number): Response =>
          new Response(JSON.stringify(obj), {
            status,
            headers: { "Content-Type": "application/json", "X-Request-Id": requestId },
          });
        // A fan-out target (a regional worker) recycles ONLY its own local container.
        if (request.headers.has("x-corelink-fanout-from")) {
          const ok = await stopLocal();
          return applyCors(
            jsonResp({ recycled: ok, scope: "regional-local", request_id: requestId }, ok ? 200 : 502),
            request,
          );
        }
        const localOk = await stopLocal();
        const recycleRegionals: Array<[string, { fetch: typeof fetch } | undefined]> = [
          ["lhr", env.PROD_LHR],
          ["sam", env.PROD_SAM],
          ["nrt", env.PROD_NRT],
          ["syd", env.PROD_SYD],
        ];
        const regionOutcomes = await Promise.all(
          recycleRegionals.map(async ([region, binding]) => {
            if (binding === undefined) {
              return { region, recycled: false, reason: "binding_absent" };
            }
            try {
              const r = await binding.fetch(
                new Request(request.url, {
                  method: "POST",
                  headers: buildInternalHeaders(request.headers, "iad"),
                }),
              );
              return { region, recycled: r.ok };
            } catch (err: unknown) {
              const m = err instanceof Error ? err.message : "unknown error";
              return { region, recycled: false, reason: m.slice(0, 80) };
            }
          }),
        );
        return applyCors(
          jsonResp(
            { recycled_local: localOk, regions: regionOutcomes, request_id: requestId },
            200,
          ),
          request,
        );
      }

      // ── GDPR Art.17 erasure completeness across residency (CAA-360 CRITICAL) ──
      // A DSR erase MUST run in EVERY jurisdiction the tenant could have data. An
      // EU tenant's CAS/AC bytes live in the prod-lhr container's EU buckets
      // (corelink-cas-eu / corelink-ac-eu), which the local (IAD) container's R2
      // client cannot (and must not) reach. When THIS worker is the erase ORIGIN
      // (not itself a fan-out target), fan the erase out to every regional worker
      // (each erases its own jurisdiction's buckets) and return "complete" (the
      // local 2xx) ONLY when the local AND every regional sweep confirm — else fail
      // CLOSED (502) so the queue consumer retries and NO false VerifiedComplete is
      // ever signed. Fail-closed by construction: a missing binding, transport
      // error, or non-2xx from any region → not complete → retry.
      const isDsrErase = isDsrEraseFanoutPath(route.pathSuffix); // allowlist, NOT startsWith — see fn doc
      const isFanoutTarget = request.headers.has("x-corelink-fanout-from");
      let internalResp: Response;
      if (isDsrErase && !isFanoutTarget) {
        // Buffer the body ONCE — it is replayed to the local container + each region.
        const eraseBody = await request.arrayBuffer();
        let localResp: Response;
        try {
          localResp = await systemStub.fetch(
            new Request(request.url, {
              method: request.method,
              headers: buildInternalHeaders(request.headers),
              body: eraseBody,
            }),
          );
        } catch (err: unknown) {
          const message = err instanceof Error ? err.message : "unknown error";
          console.error(`[${requestId}] dsr-erase local DO fetch failed: ${message.slice(0, 80)}`);
          return applyCors(
            reapiError("INTERNAL_ERROR", "internal upstream error", 500, requestId),
            request,
          );
        }
        const regionals: Array<[string, { fetch: typeof fetch } | undefined]> = [
          ["lhr", env.PROD_LHR],
          ["sam", env.PROD_SAM],
          ["nrt", env.PROD_NRT],
          ["syd", env.PROD_SYD],
        ];
        const regionResults = await Promise.all(
          regionals.map(async ([region, binding]) => {
            if (binding === undefined) {
              // A missing regional binding on the ORIGIN worker means we cannot
              // prove that jurisdiction was erased → fail CLOSED (never assume).
              console.error(
                `[${requestId}] dsr-erase fan-out: PROD_${region.toUpperCase()} binding absent`,
              );
              return { region, ok: false };
            }
            try {
              const resp = await binding.fetch(
                new Request(request.url, {
                  method: request.method,
                  headers: buildInternalHeaders(request.headers, "iad"),
                  body: eraseBody,
                }),
              );
              return { region, ok: resp.ok };
            } catch (err: unknown) {
              const message = err instanceof Error ? err.message : "unknown error";
              console.error(
                `[${requestId}] dsr-erase fan-out to ${region} threw: ${message.slice(0, 80)}`,
              );
              return { region, ok: false };
            }
          }),
        );
        const failedRegions = regionResults.filter((r) => !r.ok).map((r) => r.region);
        if (!localResp.ok || failedRegions.length > 0) {
          console.error(
            `[${requestId}] dsr-erase INCOMPLETE (fail-closed): local_ok=${localResp.ok} failed_regions=${
              failedRegions.join(",") || "none"
            }`,
          );
          return applyCors(
            reapiError(
              "INTERNAL_ERROR",
              "dsr erase incomplete across residency regions; retrying",
              502,
              requestId,
            ),
            request,
          );
        }
        internalResp = localResp;
      } else if (route.pathSuffix === "/_internal/public/revoke") {
        // B1b — collapse the edge revocation window. Forward the revoke to the
        // container (the authoritative D1 blocklist + cache_map delete + R2 erase),
        // and on SUCCESS also write the content_hash-keyed edge blocklist KV so a
        // revoked `_public` hash stops edge-serving within KV propagation
        // (~seconds) instead of the map-cache TTL (~60s). Buffer the body once: it
        // is both forwarded to the container AND parsed here for the content_hash.
        // The KV write is best-effort (ctx.waitUntil) — a KV fault must NEVER fail
        // the revoke, which the container has already applied authoritatively.
        const revokeBody = await request.arrayBuffer();
        try {
          internalResp = await systemStub.fetch(
            new Request(request.url, {
              method: request.method,
              headers: buildInternalHeaders(request.headers),
              body: revokeBody,
            }),
          );
        } catch (err: unknown) {
          const message = err instanceof Error ? err.message : "unknown error";
          console.error(`[${requestId}] public-revoke DO fetch failed: ${message.slice(0, 80)}`);
          return applyCors(
            reapiError("INTERNAL_ERROR", "internal upstream error", 500, requestId),
            request,
          );
        }
        const revokeKv = (env as unknown as { METADATA_KV?: KvReader }).METADATA_KV;
        if (internalResp.ok && revokeKv) {
          // Mark the brew/pip edge `pubblock:<hash>` KV from the RESOLVED
          // content_hash the container returns in its RESPONSE — authoritative for
          // BOTH revoke spaces. Parsing the REQUEST body (as before) missed the
          // revoke-by-`upstream_digest` incident path, whose request carries no
          // `content_hash`, leaving poisoned brew/pip bytes edge-serving for up to
          // ~60 s after an authoritative container revoke (finding F-1). Buffer the
          // body and rebuild the Response so the client leg below still streams it.
          let respText = "";
          try {
            respText = await internalResp.text();
          } catch {
            respText = "";
          }
          internalResp = new Response(respText, {
            status: internalResp.status,
            statusText: internalResp.statusText,
            headers: internalResp.headers,
          });
          try {
            const parsed = JSON.parse(respText) as { content_hash?: unknown };
            const ch =
              typeof parsed.content_hash === "string"
                ? parsed.content_hash.toLowerCase()
                : "";
            if (/^[0-9a-f]{64}$/.test(ch)) {
              ctx.waitUntil(writePublicBlocklistKv(revokeKv, ch));
            }
          } catch {
            // Response was not the expected {content_hash} JSON — the container
            // still revoked authoritatively; the edge falls back to the ~60 s map window.
          }
        }
      } else {
        // Non-erase internal route (pat/mint, admin, …) OR a fan-out target (a
        // regional worker running the erase for its own jurisdiction): single local
        // container hit, byte-identical to the pre-fan-out behaviour.
        const internalAugmented = new Request(request, {
          headers: buildInternalHeaders(request.headers),
        });
        try {
          internalResp = await systemStub.fetch(internalAugmented);
        } catch (err: unknown) {
          const message = err instanceof Error ? err.message : "unknown error";
          console.error(`[${requestId}] internal DO fetch failed: ${message.slice(0, 80)}`);
          return applyCors(
            reapiError("INTERNAL_ERROR", "internal upstream error", 500, requestId),
            request,
          );
        }
      }
      const internalHeaders = new Headers(internalResp.headers);
      if (!internalHeaders.has("x-request-id")) {
        internalHeaders.set("x-request-id", requestId);
      }
      return applyCors(
        new Response(internalResp.body, {
          status: internalResp.status,
          statusText: internalResp.statusText,
          headers: internalHeaders,
        }),
        request,
      );
    }

    // Onboarding — /v1/onboarding/* — Clerk-authenticated self-serve (tier-select
    // checkout). The browser holds a Clerk SESSION JWT, NOT a CoreLink PAT, so the
    // EDGE is the trust boundary: verify the JWT against Clerk's JWKS (fetched +
    // cached per isolate, keyed by CLERK_SECRET_KEY), resolve the tenant from the
    // user id, then forward to the tenant's DO with the internal-auth contract the
    // container's tier_select route requires (x-corelink-internal-auth +
    // x-corelink-tenant-id). Fail-CLOSED on any missing binding or bad token. The
    // internal-auth secret NEVER leaves the backend and is NEVER accepted from the
    // client (inbound trust headers are stripped before injection).
    if (route.routeKind === "onboarding") {
      const internalAuthKey = env.CORELINK_INTERNAL_AUTH_KEY;
      const clerkSecretKey = env.CLERK_SECRET_KEY;
      if (!internalAuthKey || internalAuthKey.length === 0 || !clerkSecretKey) {
        // A required server secret is unbound — deny (fail-CLOSED).
        return applyCors(
          reapiError("FORBIDDEN", "onboarding route unavailable", 403, requestId),
          request,
        );
      }

      // Verify the Clerk session + resolve the tenant via the SHARED pipeline
      // (worker/src/lib/clerk_auth.ts — dashboard revival WP-1 extraction).
      // The helper carries the full hardened flow verbatim: bearer extraction
      // (401), verifyToken with the shared azp allowlist (M1 post-verify
      // re-assert), issuer exact-pin / shape-check (M2), claims.sub required,
      // and the tenant lookup by clerk_user_id (no row → 403; D1 error → 500).
      // `clerkSecretKey` presence was already asserted by the arm-level
      // fail-CLOSED guard above, so the helper's own secret guard never fires
      // here (onboarding behavior unchanged).
      const onbClerkAuth = await verifyClerkSessionAndResolveTenant(request, env, requestId);
      if (!onbClerkAuth.ok) {
        return applyCors(onbClerkAuth.response, request);
      }
      const onbTenantId = onbClerkAuth.tenantId;

      // Forward to the tenant's DO. CRITICAL: strip ALL client-supplied trust
      // headers FIRST (the browser must never spoof internal-auth or the tenant
      // id), then set them from server-trusted values. Drop the Clerk JWT — the
      // container authenticates via internal-auth, not the session token.
      const onbDoId = env.CORELINK_SERVER.idFromName(onbTenantId);
      const onbStub = env.CORELINK_SERVER.get(onbDoId, serverGetOpts(env));
      const onbAugmented = new Request(request, {
        headers: (() => {
          const h = new Headers(request.headers);
          // Strip the FULL set of client-suppliable trust headers (x-admin-*,
          // fanout-from, scope, tenant-id, internal-auth) BEFORE re-establishing
          // them from server-trusted values (delete-then-set).
          stripClientTrustHeaders(h);
          // Belt-and-braces: x-corelink-tenant-id is now in the strip list so
          // the line above already removed any client value; kept for clarity.
          h.delete("x-corelink-tenant-id");
          h.delete("authorization");
          h.set("x-request-id", requestId);
          h.set("x-corelink-route-kind", "onboarding");
          h.set("x-corelink-token-prefix", "clerk");
          h.set("x-corelink-tenant-id", onbTenantId);
          h.set("x-corelink-internal-auth", internalAuthKey);
          return h;
        })(),
      });
      let onbResp: Response;
      try {
        onbResp = await onbStub.fetch(onbAugmented);
      } catch (err: unknown) {
        const message = err instanceof Error ? err.message : "unknown error";
        console.error(`[${requestId}] onboarding DO fetch failed: ${message.slice(0, 80)}`);
        return applyCors(
          reapiError("INTERNAL_ERROR", "onboarding upstream error", 500, requestId),
          request,
        );
      }
      const onbHeaders = new Headers(onbResp.headers);
      if (!onbHeaders.has("x-request-id")) {
        onbHeaders.set("x-request-id", requestId);
      }
      return applyCors(
        new Response(onbResp.body, {
          status: onbResp.status,
          statusText: onbResp.statusText,
          headers: onbHeaders,
        }),
        request,
      );
    }

    // Session→token exchange — POST /v1/session/exchange (hugit-P2 WP-C, seam C).
    // The caller presents a Clerk SESSION JWT; the edge verifies it (shared
    // pipeline) and exchanges it for a short-lived tenant-scoped CoreLink PAT,
    // REUSING the container's audited /_internal/pat/mint via the _system DO.
    // The handler is fully fail-CLOSED (missing secret → 403, bad/expired
    // session → 401, no tenant → 403, upstream fault → 500) and never forwards
    // the session token past the edge. CORS is applied here, mirroring the
    // onboarding/customer arms.
    if (route.routeKind === "session_exchange") {
      const sessResp = await handleSessionExchange(request, env, requestId);
      return applyCors(sessResp, request);
    }

    // githugr authz #3 — tenant lookup (POST /internal/v1/auth/tenant/lookup).
    // Internal-auth gated; parameterized D1 read; fail-CLOSED 404 when no tenant
    // maps to the subject. CORS applied here, mirroring the arms above.
    if (route.routeKind === "tenant_lookup") {
      const lookupResp = await handleTenantLookup(request, env, requestId);
      return applyCors(lookupResp, request);
    }

    // githugr authz #1 — token exchange (POST /internal/v1/auth/token-exchange).
    // Internal-auth + Clerk-session gated; 403 on session.tenant ≠ audience (the
    // cross-tenant-write rejection); mints a ~300s tenant-scoped PAT via the
    // container. Fully fail-CLOSED; never forwards the session token past the edge.
    if (route.routeKind === "token_exchange") {
      const xchgResp = await handleTokenExchange(request, env, requestId);
      return applyCors(xchgResp, request);
    }

    // corelink-runners D-9 — runner PAT mint (POST /internal/v1/runner/mint).
    // Internal-auth gated (pat_mint consumer key + shared fallback) + runners-
    // entitlement checked; mints a job-bounded tenant-scoped PAT via the
    // container's single mint authority. Handled AT the Worker: the handler
    // builds a FRESH server-trusted request to the _system DO (it never forwards
    // the inbound request), so client trust headers can never reach the mint
    // route — the same posture as token-exchange. CORS applied here.
    if (route.routeKind === "runner_mint") {
      const mintResp = await handleRunnerMint(request, env, requestId);
      return applyCors(mintResp, request);
    }

    // corelink-runners D-9 — runner PAT revoke (POST /internal/v1/runner/revoke).
    // Internal-auth gated; revokes by pat_id via the existing
    // `UPDATE pat SET revoked_at_ms` surface (INV-PAT-REVOKE-PROPAGATION). The
    // handler touches CONFIG_DB directly — no inbound headers are forwarded. CORS
    // applied here.
    if (route.routeKind === "runner_revoke") {
      const revokeResp = await handleRunnerRevoke(request, env, requestId);
      return applyCors(revokeResp, request);
    }

    // clw `auth rotate` (POST /internal/v1/auth/rotate). Internal-auth gated
    // (pat_mint consumer key + shared fallback); reads the old PAT's tenant +
    // scope, mints an equivalent new PAT via the container's single mint authority
    // (FRESH server-trusted request to the _system DO — no inbound headers
    // forwarded), then revokes the old pat_id ONLY after the mint succeeds (no
    // zero-valid-PAT window). Fully fail-CLOSED. CORS applied here.
    if (route.routeKind === "auth_rotate") {
      const rotateResp = await handleAuthRotate(request, env, requestId);
      return applyCors(rotateResp, request);
    }

    // Not found — timing-padded to prevent cross-tenant enumeration
    if (route.routeKind === "not_found") {
      await applyTimingPad(
        requestStart,
        requestId,
        requestCounter,
        getServerNonce(),
      );
      const resp = reapiError("NOT_FOUND", "The requested resource does not exist.", 404, requestId);
      return applyCors(resp, request);
    }

    // OCI two-leg auth pass-through — /v2/* and /token.
    // OCI does its OWN auth in the container (Option-B PAT verify at /token,
    // HMAC Bearer on /v2). The Worker is a pure forwarder: no extractAuth, no
    // tenant/scope injection, no path-spoof/quota/region-tenant binding (there
    // is no edge-resolved tenant for OCI). Route to a dedicated shared DO; the
    // container does per-tenant CAS namespacing from the OCI-token tenant.
    if (route.routeKind === "oci_v2" || route.routeKind === "oci_token") {
      const ociDoId = env.CORELINK_SERVER.idFromName("_oci");
      const ociStub = env.CORELINK_SERVER.get(ociDoId, serverGetOpts(env));
      const ociReq = new Request(request, {
        headers: (() => {
          const h = new Headers(request.headers);
          stripClientTrustHeaders(h);          // delete any client-forged x-corelink-*
          // Belt-and-braces: x-corelink-tenant-id is now in the strip list so
          // the delete above already removed any client value. The explicit
          // delete below is kept as belt-and-braces documentation that the OCI
          // pass-through deliberately never sets a tenant-id header.
          h.delete("x-corelink-tenant-id");
          h.set("x-request-id", requestId);
          h.set("x-corelink-route-kind", route.routeKind);
          // F-016 (OCI DoS fairness): the OCI plane has no edge-resolved tenant,
          // so the container's per-request velocity gate falls back to a per-repo
          // key — but to also bound abuse per-SOURCE it needs the UNFORGEABLE
          // client IP. Forward cf-connecting-ip as the server-trusted
          // x-corelink-client-ip (stripClientTrustHeaders above already deleted
          // any client-supplied value, so a client cannot spoof it). This header
          // IS the per-IP enforcement input: the only per-source cap on the OCI
          // plane is the in-container rate-limit bucket PARTITION keyed on it
          // (see `routes::ratelimit_layer`), shipped 2026-08-04. There is no
          // edge cap behind it — verified against the live Cloudflare API, the
          // zone's only `http_ratelimit` rule covers `corelink-signup` /
          // `corelink-admin` / `corelink-app` / `corelink-docs`, and
          // `corelink-oci` is NOT in it. An earlier revision of this comment
          // claimed such a WAF rule as the real cap "in the meantime"; that
          // claim was false, and the residual it papered over is real and OPEN:
          // a distributed attacker who rotates source IPs still earns a fresh
          // bucket per IP, and blunting THAT needs an actual edge per-IP/ASN cap.
          h.set("x-corelink-client-ip", request.headers.get("cf-connecting-ip") ?? "");
          // Deliberately NOT set: x-corelink-tenant-id / x-corelink-scope.
          // The OCI adapter derives the tenant from the OCI Bearer/PAT and
          // enforces per-op scope from its own HMAC bearer token. The raw
          // Authorization header (OCI Basic at /token, OCI Bearer at /v2) is
          // preserved by the `new Headers(request.headers)` clone above —
          // stripClientTrustHeaders does NOT remove Authorization.
          return h;
        })(),
      });
      let ociResp: Response;
      try {
        ociResp = await ociStub.fetch(ociReq);
      } catch (err: unknown) {
        const message = err instanceof Error ? err.message : "unknown error";
        // Do NOT include error detail that could leak internal topology
        // (parity with the PAT-path DO forward catch below).
        console.error(`[${requestId}] OCI DO fetch failed: ${message.slice(0, 80)}`);
        return applyCors(
          reapiError("INTERNAL_ERROR", "upstream error", 500, requestId),
          request,
        );
      }
      return applyCors(ociResp, request);
    }

    // Stripe billing webhook pass-through — POST /v1/billing/stripe-webhook.
    // Mirrors the OCI carve-out: Stripe authenticates with a `Stripe-Signature`
    // HMAC header (NOT a Bearer PAT), so the Worker is a pure forwarder here —
    // no extractAuth, no tenant/scope injection. The container is the SOLE
    // authority: it re-computes the Stripe HMAC over the EXACT raw body bytes
    // (constant-time, replay-windowed) and derives the tenant from the signed
    // event metadata. Route to the shared "_system" DO (the webhook has no URL
    // tenant). CRITICAL CORRECTNESS: forward the body UNCHANGED — `new Request(
    // request, { headers })` preserves the body stream unread, so the bytes the
    // container hashes are byte-identical to what Stripe signed. We do NOT
    // read/clone/parse the body (a re-serialized body would break the signature).
    if (route.routeKind === "billing_webhook") {
      const billingDoId = env.CORELINK_SERVER.idFromName("_system");
      const billingStub = env.CORELINK_SERVER.get(billingDoId, serverGetOpts(env));
      const billingReq = new Request(request, {
        headers: (() => {
          const h = new Headers(request.headers);
          // Strip any client-forged x-corelink-* server-trust headers BEFORE we
          // set our own (delete-then-set). stripClientTrustHeaders does NOT
          // remove `stripe-signature` (the webhook's auth) nor `authorization`
          // — both are preserved by the `new Headers(request.headers)` clone.
          stripClientTrustHeaders(h);
          h.set("x-request-id", requestId);
          h.set("x-corelink-route-kind", route.routeKind);
          // Belt-and-braces: the container derives the tenant SOLELY from the
          // signed Stripe event, never from a header. x-corelink-tenant-id is
          // now in the strip list so the delete above already removed any client
          // value; the explicit delete below documents that the billing-webhook
          // path deliberately never sets a tenant-id header.
          h.delete("x-corelink-tenant-id");
          h.delete("x-corelink-scope");
          // Deliberately NOT set: x-corelink-tenant-id / x-corelink-scope.
          return h;
        })(),
      });
      let billingResp: Response;
      try {
        billingResp = await billingStub.fetch(billingReq);
      } catch (err: unknown) {
        const message = err instanceof Error ? err.message : "unknown error";
        // Do NOT include error detail that could leak internal topology
        // (parity with the OCI / PAT-path DO forward catches).
        console.error(`[${requestId}] billing webhook DO fetch failed: ${message.slice(0, 80)}`);
        return applyCors(
          reapiError("INTERNAL_ERROR", "upstream error", 500, requestId),
          request,
        );
      }
      return applyCors(billingResp, request);
    }

    // PUBLIC erasure-attestation verifier — /v1/public/* (Artifact 1, WP-C1).
    // Pure pass-through to the _anonymous DO → container, which is the SOLE
    // authority. These are UNAUTHENTICATED GETs (an erasure proof is publicly
    // verifiable) — NO PAT gate, NO internal-auth (mirrors the billing-webhook
    // carve-out, minus any signature). Client-forged x-corelink-* trust headers
    // are stripped; the container routes are mounted outside its auth layers.
    if (route.routeKind === "public_attestation") {
      const pubDoId = env.CORELINK_SERVER.idFromName("_anonymous");
      const pubStub = env.CORELINK_SERVER.get(pubDoId, serverGetOpts(env));
      const pubReq = new Request(request, {
        headers: (() => {
          const h = new Headers(request.headers);
          stripClientTrustHeaders(h);
          h.set("x-request-id", requestId);
          h.set("x-corelink-route-kind", "public_attestation");
          h.set("x-corelink-tenant-id", "_anonymous");
          // M22(a): this arm did NOT set the trusted client-IP header (unlike
          // the cache/OCI/signup arms), so the container's scoped per-IP
          // rate limiter on `/v1/public/*` had nothing to key on. Forward
          // CF's unforgeable client IP as x-corelink-client-ip
          // (stripClientTrustHeaders above already deleted any
          // client-supplied value, so a client cannot spoof it).
          h.set("x-corelink-client-ip", request.headers.get("cf-connecting-ip") ?? "");
          return h;
        })(),
      });
      let pubResp: Response;
      try {
        pubResp = await pubStub.fetch(pubReq);
      } catch (err: unknown) {
        const message = err instanceof Error ? err.message : "unknown error";
        console.error(`[${requestId}] public attestation DO fetch failed: ${message.slice(0, 80)}`);
        return applyCors(
          reapiError("INTERNAL_ERROR", "upstream error", 500, requestId),
          request,
        );
      }
      return applyCors(pubResp, request);
    }

    // corelink-runners fabric introspect — pure pass-through to the _system DO →
    // container, which is the SOLE auth authority (FABRIC_INTROSPECT_AUTH_KEY).
    // The Worker forwards the caller's x-corelink-internal-auth (the FABRIC secret)
    // UNCHANGED and applies NO edge gate (mirrors the billing-webhook carve-out,
    // where the container verifies the Stripe signature). See the matchRoute note.
    if (route.routeKind === "fabric_introspect") {
      const fbDoId = env.CORELINK_SERVER.idFromName("_system");
      const fbStub = env.CORELINK_SERVER.get(fbDoId, serverGetOpts(env));
      // Capture the FABRIC secret BEFORE stripping client trust headers.
      const fabricAuth = request.headers.get("x-corelink-internal-auth") ?? "";
      const fbReq = new Request(request, {
        headers: (() => {
          const h = new Headers(request.headers);
          stripClientTrustHeaders(h);
          h.set("x-request-id", requestId);
          h.set("x-corelink-route-kind", "fabric_introspect");
          h.set("x-corelink-tenant-id", "_system");
          // Re-forward the caller's FABRIC secret unchanged — the container's
          // introspect gate is the sole authority; the Worker never inspects it.
          h.set("x-corelink-internal-auth", fabricAuth);
          return h;
        })(),
      });
      let fbResp: Response;
      try {
        fbResp = await fbStub.fetch(fbReq);
      } catch (err: unknown) {
        const message = err instanceof Error ? err.message : "unknown error";
        console.error(`[${requestId}] fabric introspect DO fetch failed: ${message.slice(0, 80)}`);
        return applyCors(
          reapiError("INTERNAL_ERROR", "upstream error", 500, requestId),
          request,
        );
      }
      return applyCors(fbResp, request);
    }

    // corelink-runners billing usage-push ingest — pure pass-through to the
    // _system DO → container, which is the SOLE auth authority
    // (BILLING_INGEST_AUTH_KEY). The Worker forwards the caller's
    // x-corelink-internal-auth (the ingest secret) UNCHANGED and applies NO edge
    // gate (mirrors the fabric_introspect / billing-webhook carve-outs). See the
    // matchRoute note.
    if (route.routeKind === "billing_ingest") {
      const biDoId = env.CORELINK_SERVER.idFromName("_system");
      const biStub = env.CORELINK_SERVER.get(biDoId, serverGetOpts(env));
      // Capture the INGEST secret BEFORE stripping client trust headers.
      const ingestAuth = request.headers.get("x-corelink-internal-auth") ?? "";
      const biReq = new Request(request, {
        headers: (() => {
          const h = new Headers(request.headers);
          stripClientTrustHeaders(h);
          h.set("x-request-id", requestId);
          h.set("x-corelink-route-kind", "billing_ingest");
          h.set("x-corelink-tenant-id", "_system");
          // Re-forward the caller's INGEST secret unchanged — the container's
          // ingest gate is the sole authority; the Worker never inspects it.
          h.set("x-corelink-internal-auth", ingestAuth);
          return h;
        })(),
      });
      let biResp: Response;
      try {
        biResp = await biStub.fetch(biReq);
      } catch (err: unknown) {
        const message = err instanceof Error ? err.message : "unknown error";
        console.error(`[${requestId}] billing ingest DO fetch failed: ${message.slice(0, 80)}`);
        return applyCors(
          reapiError("INTERNAL_ERROR", "upstream error", 500, requestId),
          request,
        );
      }
      return applyCors(biResp, request);
    }

    // Customer portal dual-auth dispatch (dashboard revival WP-1) —
    // /v1/customer/* accepts EITHER a CoreLink PAT (existing path, byte-identical
    // — handled by the generic PAT gate below) OR a Clerk session JWT (the
    // browser dashboard holds a Clerk session, not a PAT).
    //
    // Dispatch guard: parsePat() accepts ONLY the canonical
    // `corelink_<env>_<token_id>.<random_secret>.<hmac_sig>` shape — a Clerk JWT
    // (or any non-PAT bearer) can NEVER parse as one, so any parseable PAT
    // (including expired/revoked ones) falls through to the PAT gate exactly as
    // before; the PAT surface is untouched. The token is derived the same way
    // extractAuth derives it (slice "Bearer " + trim) so the dispatch decision
    // and the PAT gate's parse can never disagree.
    //
    // Clerk arm: verify via the SHARED pipeline (lib/clerk_auth.ts — same M1
    // azp re-assert + M2 issuer pin + tenant lookup as onboarding), then
    // forward to the PER-TENANT DO. Mirrors the onboarding forward but
    // deliberately WITHOUT x-corelink-internal-auth — customer routes resolve
    // the tenant from the server-trust x-corelink-tenant-id header and do not
    // need the internal-auth key (least privilege: the dashboard surface must
    // not carry the operator-grade credential).
    //
    // Storage-quota gate is BYPASSED on this arm (deliberate): an over-quota
    // tenant must still see the dashboard to upgrade — same posture as
    // onboarding (which also never passes through the quota gate).
    if (route.routeKind === "customer_v1") {
      const custAuthz = request.headers.get("authorization") ?? "";
      const custToken = custAuthz.startsWith("Bearer ")
        ? custAuthz.slice("Bearer ".length).trim()
        : "";
      if (parsePat(custToken) === null) {
        const custClerkAuth = await verifyClerkSessionAndResolveTenant(request, env, requestId);
        if (!custClerkAuth.ok) {
          return applyCors(custClerkAuth.response, request);
        }
        const custTenantId = custClerkAuth.tenantId;

        // Forward to the tenant's DO. CRITICAL: strip ALL client-supplied trust
        // headers FIRST (the browser must never spoof internal-auth or the
        // tenant id), then set them from server-trusted values. Drop the Clerk
        // JWT — the container trusts the Worker-set x-corelink-tenant-id, and
        // the session token must not travel further than the edge.
        const custDoId = env.CORELINK_SERVER.idFromName(custTenantId);
        const custStub = env.CORELINK_SERVER.get(custDoId, serverGetOpts(env));
        const custAugmented = new Request(request, {
          headers: (() => {
            const h = new Headers(request.headers);
            // Strip the FULL set of client-suppliable trust headers (x-admin-*,
            // fanout-from, scope, tenant-id, internal-auth) BEFORE re-establishing
            // them from server-trusted values (delete-then-set).
            stripClientTrustHeaders(h);
            h.delete("authorization");
            h.set("x-request-id", requestId);
            h.set("x-corelink-route-kind", "customer_v1");
            h.set("x-corelink-token-prefix", "clerk");
            h.set("x-corelink-tenant-id", custTenantId);
            // RBAC scope (team_member role 0074), sole setter:
            //   viewer        → read-only  (no write, no billing)
            //   member        → read-write (cache write; NO `billing`)
            //   owner / admin → read-write billing (H17: the billing capability a
            //                   cache PAT never carries — only an owner/admin
            //                   dashboard human clears the F-018 billing/PII gate)
            // The billing carve-out is now OWNER/ADMIN-only: a plain `member` must
            // not open the billing portal / cancel the subscription / read financial
            // PII (RBAC hardening — this is the sole place `billing` is granted).
            // Team-management ops (invite/remove) gate on the `x-corelink-role`
            // header separately in the container.
            h.set(
              "x-corelink-scope",
              custClerkAuth.role === "viewer"
                ? "read-only"
                : custClerkAuth.role === "owner" || custClerkAuth.role === "admin"
                  ? "read-write billing"
                  : "read-write",
            );
            // Team RBAC role (0074): forward the D1-resolved role so the container
            // can gate OWNER-only operations (account deletion erases the WHOLE
            // tenant — a non-owner seat must not trigger it). Sole setter; the
            // client copy was stripped above. `x-corelink-scope` only distinguishes
            // viewer vs the rest, so it cannot express "is owner" — the role does.
            h.set("x-corelink-role", custClerkAuth.role);
            // DSR portal (/v1/privacy/*) destructive-arm MFA step-up: the Worker
            // is the SOLE setter of x-corelink-mfa-verified (stripped above). The
            // container gate (routes/dsr/portal.rs) is fail-CLOSED on this trusted
            // marker. We stamp it ONLY when the Clerk session's factor-verification
            // age is FRESH — `fvaMinutes != null && fvaMinutes <= threshold` — so a
            // stolen/XSS/CSRF long-lived dashboard session can NOT trigger
            // irreversible cross-region erasure with no re-auth. `undefined`
            // (absent/malformed `fva`) ⇒ NOT fresh (fail-CLOSED, never 0), the same
            // freshness signal the session/token-exchange paths forward
            // (lib/session_exchange.ts). When NOT fresh we leave the marker unset:
            // the container gate then fails closed and records the DSR ticket
            // `pending` with `mfa_required=true` (the caller completes step-up via
            // POST /v1/privacy/dsr/{id}/verify-mfa) — matching existing missing-marker
            // behaviour. Only ever stamped for the privacy plane.
            if (
              route.pathSuffix.startsWith("/v1/privacy/") &&
              custClerkAuth.fvaMinutes !== undefined &&
              custClerkAuth.fvaMinutes <= MFA_FVA_FRESH_MAX_MINUTES
            ) {
              h.set("x-corelink-mfa-verified", "1");
            }
            // Deliberately NOT set: x-corelink-internal-auth (least privilege —
            // customer routes don't need the operator-grade credential).
            return h;
          })(),
        });
        let custResp: Response;
        try {
          custResp = await custStub.fetch(custAugmented);
        } catch (err: unknown) {
          const message = err instanceof Error ? err.message : "unknown error";
          console.error(`[${requestId}] customer clerk DO fetch failed: ${message.slice(0, 80)}`);
          return applyCors(
            reapiError("INTERNAL_ERROR", "customer upstream error", 500, requestId),
            request,
          );
        }
        const custHeaders = new Headers(custResp.headers);
        if (!custHeaders.has("x-request-id")) {
          custHeaders.set("x-request-id", requestId);
        }
        return applyCors(
          new Response(custResp.body, {
            status: custResp.status,
            statusText: custResp.statusText,
            headers: custHeaders,
          }),
          request,
        );
      }
      // else: the bearer parses as a canonical PAT — fall through to the
      // generic PAT gate below (byte-identical to the pre-WP-1 behavior).
    }

    // Auth gate — all other routes require a valid Bearer PAT, EXCEPT signup
    // which is pre-tenant: the :token in /v1/signup/pilot/:token IS the auth
    // artifact, not a Bearer PAT. The container validates the path token against
    // the signup-tokens store.
    //
    // For everything else: extractAuth performs parse format → HMAC fast-fail
    // (if PAT_SIGNING_KEY bound) → D1 existence + expiry check → resolve
    // tenant_id. Any token not in the D1 store → 401 (WP-A1 P0-2 fix).
    type AuthOk = Extract<AuthResult, { ok: true }>;
    let auth: AuthOk;
    if (route.routeKind === "signup") {
      // Signup is pre-tenant: the path :token IS the auth artifact, not a PAT,
      // so there is no D1-resolved scope — forward an empty scope (H1).
      // WP5a: signup is pre-tenant and never a runner-job PAT → runnerJobAcKey null.
      auth = { ok: true, tenantId: "_anonymous", tokenPrefix: "signup", scope: "", runnerJobAcKey: null };
    } else {
      // Scope guard (security): Basic auth is accepted ONLY on the `pip` adapter
      // route. pip/uv can emit nothing but URL-embedded Basic; every other
      // surface (native CAS/AC, npm `_authToken` Bearer, cargo/sccache Bearer,
      // browser) still rejects non-Bearer schemes with `invalid_scheme`.
      stAuthStart = Date.now();
      const result = await extractAuth(
        request,
        env,
        route.routeKind === "pip",
        ctx.waitUntil.bind(ctx),
      );
      stAuthEnd = Date.now();
      if (result.ok) stPatSource = result.patSource;
      if (!result.ok) {
        // OCI (oci_v2 / oci_token) never reaches here — it is handled by the
        // dedicated pass-through branch ABOVE (which forwards to the container
        // for its own two-leg auth), so this PAT-gate path only sees PAT routes.
        //
        // F18: signing_key_not_configured means PAT_SIGNING_KEY is absent or
        // too short — the operator MUST be alerted via 503 (not 401, which would
        // silently look like a bad client credential). The structured error log
        // is emitted inside extractAuth; here we map to 503 Service Unavailable.
        //
        // H1: d1_lookup_error is a TRANSIENT D1 infra fault (network partition /
        // DB unavailable) raised by the PAT D1 lookup — NOT a bad credential. It
        // MUST map to 503 (retryable) too, otherwise a D1 hiccup makes every
        // client see "bad credentials" → CI failures + spurious PAT rotation +
        // on-call chasing the wrong thing. Genuine bad/unknown PATs
        // (pat_not_found / pat_expired / invalid_*) still fall through to 401.
        if (
          result.reason === "signing_key_not_configured" ||
          result.reason === "d1_lookup_error"
        ) {
          return applyCors(
            reapiError("SERVICE_UNAVAILABLE", "authentication service unavailable", 503, requestId),
            request,
          );
        }
        // G4: the PAT is valid but its tenant is suspended/erased
        // (tenant_offboarding_state.state ∈ {suspended, erased}). This is an
        // authorization denial, NOT a bad credential — map to 403 fail-closed
        // (distinct from the 401 unknown/expired/malformed-PAT arms), so a
        // suspended tenant is fast-denied on the customer CAS/AC hot path
        // without waiting for every PAT to be individually revoked.
        if (result.reason === "tenant_suspended") {
          return applyCors(
            reapiError("FORBIDDEN", "tenant suspended", 403, requestId),
            request,
          );
        }
        return applyCors(
          reapiError("UNAUTHORIZED", "authentication required", 401, requestId),
          request,
        );
      }
      auth = result;
    }

    // ── Tenant routing (WP-T1) ────────────────────────────────────────────────
    // auth.tenantId is the PAT-resolved tenant produced by WP-A1's D1 lookup.
    // We use it exclusively for DO routing — never the raw URL path segment.
    //
    // WP-A1 contract: resolves real tenant → stores in auth.tenantId.
    // Until WP-A1 lands the value is "_pending" (WP-A1 stub in extractAuth).
    // Once WP-A1 is merged, auth.tenantId will carry the real tenant ID.
    const resolvedTenantId = auth.tenantId;

    // Path-spoof defence (P1-2): if the URL path carries a tenant namespace
    // AND the PAT has been resolved to a real tenant (not the "_pending" stub),
    // the URL tenant MUST match the PAT tenant. Mismatch → 403.
    //
    // The guard `resolvedTenantId !== "_pending"` is the WP-A1 activation gate:
    //   - "_pending" = WP-A1 not yet merged → skip spoof check (transitional)
    //   - Any other value = WP-A1 resolved → enforce tenant match
    const urlTenant = route.tenantId;
    const isRealTenant = resolvedTenantId !== "_pending";

    if (isRealTenant && urlTenant !== "_anonymous" && urlTenant !== resolvedTenantId) {
      // PAT tenant ≠ URL path tenant — potential path-spoof.
      // Return 403 without leaking which side mismatched. (OCI never reaches
      // this PAT-gate path — see the dedicated pass-through branch above.)
      return applyCors(
        reapiError("FORBIDDEN", "tenant mismatch", 403, requestId),
        request,
      );
    }

    // ── Onboarding funnel: `first_cli_authed` producer (PLG §7.1) ─────────────
    // The CLI's FIRST authenticated call is `GET /v1/users/me` (`corelink
    // whoami` — tools/cli/src/client.rs:138 — and the doctor auth probe —
    // tools/cli/src/doctor.rs:37). This is the earliest point where BOTH facts
    // the event asserts are established: the PAT verified (extractAuth returned
    // ok, above) and the tenant is a real, non-spoofed tenant (the path-spoof
    // guard immediately above has just cleared). It is deliberately BEFORE the
    // quota/residency/DO legs so a later 429/503 can never suppress a signal
    // that is about AUTH, not about the response body.
    //
    // Fire-and-forget: `emitFirstCliAuthed` returns void (impossible to await
    // inline), swallows all its own errors, and its promise is handed to
    // ctx.waitUntil so it survives the response (#859 — a bare floating promise
    // is cancelled on return, see lib/tenant_suspend_gate.ts:115).
    // Dedup is the deterministic id `first_cli_authed:<tenant_id>` against the
    // analytics_events PRIMARY KEY + ingest's INSERT OR IGNORE — see
    // lib/onboarding_events.ts.
    if (
      route.routeKind === "reapi_v1" &&
      request.method === "GET" &&
      url.pathname === "/v1/users/me"
    ) {
      emitFirstCliAuthed(env, resolvedTenantId, { waitUntil: ctx.waitUntil.bind(ctx) });
    }

    // ── Per-tier quota enforcement ────────────────────────────────────────────
    // Quota checks run AFTER auth and BEFORE forwarding to the DO.
    // Skip for system/anonymous tenants (no billing record exists for them).
    //
    // Storage quota: enforced from SUM(tenant_storage_state.bytes_used).
    // Request quota: enforced BY DEFAULT via the monthly_request_counts atomic
    // counter (fail-CLOSED) — disabled only by REQUEST_QUOTA_DISABLED="true"
    // (dev/test). See worker/src/lib/quota.ts.
    //
    // D1-error posture is verb-aware (CAA-360 #25): reads fail OPEN for
    // availability, byte-adding writes (PUT/POST) fail CLOSED so an outage
    // cannot be used to write past the cap. The DO's CAS quota enforcement
    // (quota_fsm_state) provides the deeper safety net on mutations.
    //
    // The resolved per-tier storage cap is also forwarded to the container as
    // the server-trusted STORAGE_QUOTA_HEADER so the container's byte-accounting
    // reservation seeds a FRESH tenant_storage_state row with the REAL cap
    // (not the legacy uncapped `0`). It is set ONLY for a real tenant with a
    // confirmed tier; `null` (system/anon/pending, or a D1-error tier) ⇒ the
    // header is omitted and the container fails closed on an unseeded tenant.
    let storageQuotaHeader: string | null = null;
    if (resolvedTenantId !== "_anonymous" && resolvedTenantId !== "_system" && resolvedTenantId !== "_pending") {
      // Helper: emit the shared 429 quota-exceeded response shape.
      const quotaExceeded = (reason: string, retryAfterSec: number): Response =>
        applyCors(
          new Response(
            JSON.stringify({
              error: "QUOTA_EXCEEDED",
              message: reason,
              request_id: requestId,
            }),
            {
              status: 429,
              headers: {
                "Content-Type": "application/json",
                "Retry-After": String(retryAfterSec),
                "X-Request-Id": requestId,
              },
            },
          ),
          request,
        );

      // Request-count quota is ENFORCED BY DEFAULT (fail-CLOSED). The monthly
      // per-tenant request cap is a CONTRACTED ceiling, so an unset env var in
      // prod must NOT silently disable it (Cluster D). The gate is an explicit
      // opt-OUT kill-switch: enforcement is live UNLESS REQUEST_QUOTA_DISABLED
      // === "true" (set only in dev/test). Mirrors the fail-closed posture of
      // the other quota/security gates in this file.
      const requestQuotaEnabled = env.REQUEST_QUOTA_DISABLED !== "true";

      // ── #11: multi-region fan-out over-count fix ──────────────────────────
      // A regional Worker invocation that is itself an INTERNAL fan-out sub-
      // request of one logical client request must NOT re-meter that request:
      // the PRIMARY Worker already incremented the monthly counter once before
      // it fanned out. Counting again here double-charges multi-region tenants
      // (customer-unfavourable).
      //
      // FORGERY-SAFE (tech-lead review of b5ba30c1): the fan-out marker MUST be
      // a value a client cannot forge. The public edge does NOT ingress-strip
      // x-corelink-fanout-from, so a PRESENCE check on the header alone would let
      // ANY client send `x-corelink-fanout-from: anything` to SKIP metering → a
      // request-quota BYPASS (fail-OPEN — worse than the over-count it replaced).
      // So the marker carries the shared server-to-server secret
      // CORELINK_INTERNAL_AUTH_KEY (bound on [env.prod] AND every regional worker
      // env — prod-sam/lhr/nrt/syd — per ADR-MULTI-REGION-V1 §Consequences and the
      // wrangler.toml per-region secret block). The primary Worker sets the header
      // to that secret AFTER stripClientTrustHeaders on the fan-out forward, and it
      // travels ONLY over the service binding (never to a client). We treat the
      // request as a fan-out ONLY on a CONSTANT-TIME match against that secret:
      // a forged value ("prod", a random guess, "") does NOT match → metering
      // still happens. Fail-SAFE: if the secret is unbound, the match can never
      // succeed → every request meters (no bypass).
      //
      // SCOPE: this gates ONLY the metering (the increment + the request-cap
      // comparison). Tier resolution (getTierForTenant) and the server-trusted
      // STORAGE_QUOTA_HEADER forwarding stay UNCONDITIONAL below — a fan-out
      // sub-request still needs the resolved storage cap forwarded to its
      // regional container, and gating those would re-introduce the regional
      // storage-header regression.
      const fanoutHeader = request.headers.get("x-corelink-fanout-from");
      const isFanout =
        fanoutHeader !== null &&
        typeof env.CORELINK_INTERNAL_AUTH_KEY === "string" &&
        env.CORELINK_INTERNAL_AUTH_KEY.length > 0 &&
        constantTimeSecretEqual(env.CORELINK_INTERNAL_AUTH_KEY, fanoutHeader);

      // UNCONDITIONAL (also on fan-out): the served path — including a fan-out
      // sub-request forwarding to its regional container — needs the resolved
      // tier for the server-trusted STORAGE_QUOTA_HEADER. Gating these on
      // !isFanout would re-introduce the regional storage-header regression.
      // Latency WP slice 2: resolve the tier through the L1→KV-L2→D1 cache
      // (`ttier:`), mirroring the residency KV-L2 (slice 1) — collapses the tier
      // pair of the `wdb` quota trio for far-from-D1 (SAM/GRU) callers. Returns
      // the same TierResult shape; an unconfirmed (`d1Error`) result is never
      // cached, so the F21 fail-open posture is preserved.
      //
      // Resolved FIRST (it used to run between the counter UPSERT and the storage
      // SUM) because the batch below needs the tier to decide whether the storage
      // read is worth issuing at all. It is cache-served in steady state — the
      // 2026-08-04 prod measurement puts `qtier` at 0/0/3 ms — and it was already
      // UNCONDITIONAL, so nothing pays extra for the reorder: the same reads
      // happen, in a different order.
      const tierKv = (env as unknown as { METADATA_KV?: KvReader }).METADATA_KV;
      const tierStart = Date.now();
      const quotaTier = await resolveTenantTierCached(env.CONFIG_DB, resolvedTenantId, {
        ...(tierKv ? { kv: tierKv } : {}),
        waitUntil: ctx.waitUntil.bind(ctx),
      });
      stQTierMs = Date.now() - tierStart;
      storageQuotaHeader = storageQuotaHeaderValue(quotaTier);

      // A storage-increasing op is a write verb (PUT uploads / POST). DELETE
      // reduces storage and reads (GET/HEAD) cannot grow it, so both stay
      // available during a D1 outage.
      const isStorageMutating = request.method === "PUT" || request.method === "POST";
      const meter = !isFanout && requestQuotaEnabled;

      // ── P3 EDGE_DO_METER serve (ADR 2026-08-19): the edge-local DO is the
      //    AUTHORITATIVE monthly request-count cap ──────────────────────────
      // When EDGE_DO_METER === "serve" (per-region flag) AND this is a
      // genuinely-counted request on a CAPPED tier with both DOs + a region
      // bound, we meter this request through the shard/coordinator SYNCHRONOUSLY
      // (awaited — the verdict is a serve decision, not telemetry) with
      // reconcile→D1 ON, and treat the DO's `withinCap` as the request-cap
      // verdict. On success we DROP the per-request D1 counter UPSERT (pass
      // `meter:false` to runQuotaBatch, storage-only) and SKIP the async-`on`
      // fast path and the D1 `withinFreeCap` gate below — the DO owns the cap.
      //
      // FAIL-OPEN (money-path safety): if serveViaDO THROWS (any DO hop non-2xx
      // / outage), we swallow it to `serveVerdict = null` and fall straight back
      // to today's exact D1 count path (meter stays true, async/D1 metering +
      // withinFreeCap run). A DO outage must NEVER break a request nor fail it
      // closed. Storage quota is ALWAYS enforced regardless (serve concerns ONLY
      // the request-count cap). FAN-OUT: the gate requires meter===true, so a
      // fan-out sub-request (meter=false) never hits the DO — no double count.
      const doServeCap = QUOTAS[quotaTier.tier].requestsPerMonthMax;
      const doServeActive = serveGateActive({
        mode: (env as unknown as { EDGE_DO_METER?: string }).EDGE_DO_METER,
        meter,
        hasShardNs: env.REQUEST_METER_SHARD_DO !== undefined,
        hasCoordNs: env.REQUEST_METER_COORDINATOR_DO !== undefined,
        region: env.R2_CAS_REGION,
        cap: doServeCap,
      });
      let serveVerdict: { withinCap: boolean } | null = null;
      // The extra env truthiness checks are always-true given doServeActive
      // (serveGateActive already required them) — they are here solely so TS
      // narrows the optional DO namespaces + region, mirroring the shadow branch.
      if (
        doServeActive &&
        env.REQUEST_METER_SHARD_DO &&
        env.REQUEST_METER_COORDINATOR_DO &&
        env.R2_CAS_REGION
      ) {
        const shardNs = env.REQUEST_METER_SHARD_DO;
        const coordNs = env.REQUEST_METER_COORDINATOR_DO;
        try {
          serveVerdict = await serveViaDO(
            { shard: shardNs, coordinator: coordNs },
            {
              tenantId: resolvedTenantId,
              region: env.R2_CAS_REGION,
              yearMonth: currentYearMonthUtc(),
              cap: doServeCap,
              block: Math.min(doServeCap, DO_METER_LEASE_BLOCK),
              lowWater: 0,
            },
          );
        } catch {
          // Fail-OPEN: fall back to the normal D1 count path below.
          serveVerdict = null;
        }
      }
      // True iff the DO authoritatively metered this request (verdict in hand).
      const serveHandled = serveVerdict !== null;

      // ── WP-B (async metering): take the quota trio off the WARM READ path ──
      // For a metered, tier-CONFIRMED, NON-mutating request whose tenant is
      // provably far under its request cap, serve with ZERO synchronous D1: the
      // monthly-counter increment runs via ctx.waitUntil (off the hot path) and
      // the storage verdict comes from the B1 KV cache (a READ cannot grow
      // storage, so a ≤60s-stale byte count is safe and customer-favorable).
      // Every OTHER case — a WRITE verb, a fan-out/kill-switched request, an
      // unconfirmed (d1Error) tier, a KV miss, or a tenant within burstMargin of
      // its cap — falls through to the EXACT `runQuotaBatch` path below,
      // byte-identical to before. Flag `EDGE_ASYNC_METER`: unset/off ⇒ exact path
      // (today's behaviour); `shadow` ⇒ exact path + a no-PII divergence log to
      // prove the arming rule on real traffic; `on` ⇒ serve the fast path.
      // ADR: docs/design/2026-08-17-adr-edge-async-metering.md.
      const asyncMeterMode = (env as unknown as { EDGE_ASYNC_METER?: string }).EDGE_ASYNC_METER;
      const asyncMeterEligible =
        meter && !isStorageMutating && !quotaTier.d1Error;
      let handledFast = false;

      if (!serveHandled && asyncMeterMode === "on" && asyncMeterEligible) {
        const fast = await tryFastRequestCount(env.CONFIG_DB, resolvedTenantId, quotaTier.tier, {
          ...(tierKv ? { kv: tierKv } : {}),
          waitUntil: ctx.waitUntil.bind(ctx),
        });
        if (fast !== null) {
          // Request cap: armed ⇒ headroom > burstMargin ⇒ within cap by
          // construction. Storage: B1 cached read verdict (same verdict the live
          // SUM would return, from a ≤60s-fresh byte count).
          const storageCheck = await checkStorageQuotaCachedRead(
            env.CONFIG_DB,
            resolvedTenantId,
            quotaTier.tier,
            false,
            storageCapIsFinite(quotaTier.tier),
            { ...(tierKv ? { kv: tierKv } : {}), waitUntil: ctx.waitUntil.bind(ctx) },
          );
          if (!storageCheck.ok) {
            return quotaExceeded(storageCheck.reason, storageCheck.retryAfterSec);
          }
          handledFast = true;
        }
      } else if (asyncMeterMode === "shadow" && asyncMeterEligible) {
        // Canary: compute the arming DECISION (no side effect, no double-count)
        // and log it; the exact path below still serves. No tenant id (no PII).
        ctx.waitUntil(
          decideFastPath(resolvedTenantId, quotaTier.tier, {
            ...(tierKv ? { kv: tierKv } : {}),
          })
            .then((d) => {
              console.log(
                JSON.stringify({
                  evt: "async_meter_shadow",
                  tier: quotaTier.tier,
                  arm: d.arm,
                  reason: d.reason,
                  cachedCount: d.cachedCount,
                }),
              );
            })
            .catch(() => {
              /* telemetry must never break the request */
            }),
        );
      }

      // ── ONE round trip for both uncached D1 statements ────────────────────
      // The monthly-counter UPSERT and the storage `SUM(bytes_used)` read used to
      // run as two SERIAL awaits, and the 2026-08-04 prod measurement (warm, n=30)
      // showed they were the ENTIRE `wdb` phase: `qmeter` 152/158/163 ms + `qstor`
      // 120/126/130 ms = `wdb` 277/284/302 ms, with sum(4) − wdb = 0 on 30/30
      // requests. Neither reads the other's result, so the seriality bought
      // nothing and cost a full ENAM round trip. `db.batch` issues both in one.
      //
      // The counter is still incremented SYNCHRONOUSLY here, and deliberately: it
      // is a billing/quota counter whose post-increment value IS the 429 decision
      // below, and `ctx.waitUntil` carries no durability guarantee (an evicted
      // isolate drops the pending write → a silently under-counted tenant). What
      // is counted, when, and against which cap is byte-for-byte unchanged; only
      // the number of network round trips changed. (`runQuotaBatch` documents the
      // one real consequence: a D1 batch is a transaction, so a fault now rolls
      // the increment back too — an UNDER-count, the direction this counter's
      // fail-open design already tolerates, never an over-count.)
      //
      // #11: skip the metering UPSERT on an internal fan-out sub-request (the
      // primary Worker already counted this logical request). `counted:false`
      // yields `withinFreeCap === true`, so the request-cap comparison below is
      // also skipped — a fan-out sub-request is never re-metered nor 429'd on the
      // request cap. The storage read is likewise skipped for an unlimited-storage
      // tier or an unconfirmed (`d1Error`) one, exactly as before.
      //
      // rt-nuclear #24 (accepted, narrowed): the counter UPSERT used to run FIRST
      // so a tenant already over even the LOWEST tier cap was 429'd without paying
      // the storage SUM. Batched, an over-cap tenant now also pays that read — but
      // in the SAME round trip, so the cost is rows-read, not latency, and it is
      // bounded (a SUM over that tenant's handful of `tenant_storage_state` rows).
      // The alternative — remembering that this tenant was over-cap to skip the
      // read — is a cached authorization decision, which this path must not have.
      if (!handledFast) {
      const qbatchStart = Date.now();
      const quotaBatch = await runQuotaBatch(env.CONFIG_DB, resolvedTenantId, quotaTier, {
        // P3 serve: the DO authoritatively counted this request, so DROP the
        // per-request D1 counter UPSERT (storage-only batch). Otherwise unchanged.
        meter: serveHandled ? false : meter,
        isMutating: isStorageMutating,
      });
      // `-1` (phase omitted) iff no round trip was issued at all — both statements
      // skipped. Anything else is a real measurement of the one round trip, 0
      // included. Unlike the `qmeter`/`qstor` pair it replaces, `qbatch` does NOT
      // reveal WHICH statements were in it, which incidentally closes the
      // confirmation oracle the old `qmeter` omission gave a fan-out caller (an
      // absent `qmeter` beside a present `qtier`/`qstor`/`qresid` confirmed that
      // their `x-corelink-fanout-from` matched CORELINK_INTERNAL_AUTH_KEY). What
      // remains is strictly narrower: `qbatch` is absent only when the metering
      // AND the storage statement are both skipped, i.e. a fan-out (or
      // kill-switched) request from an unlimited-STORAGE tenant — enterprise only.
      if (quotaBatch.ranD1) stQBatchMs = Date.now() - qbatchStart;
      const inc = quotaBatch.increment;

      // P3 EDGE_DO_METER shadow (ADR 2026-08-19): run the edge-local DO metering
      // path in parallel to the authoritative D1 verdict and log agreement. D1
      // still enforces below; the DO path performs NO enforcement here. Runs OFF
      // the response path (`ctx.waitUntil`) so it never adds latency, and only for
      // a genuinely-counted request (`inc.counted` — a fan-out sub-request is not
      // re-metered) on a CAPPED tier (uncapped never denies). No tenant id is
      // logged (region is a colo code + tier are not PII). NOTE: an UNSEEDED
      // shadow diverges from D1 mid-month by construction (the DO count starts at
      // 0 while D1 already holds this month's count) — this shadow validates the
      // DO MECHANISM (lease math, hop health, error rate, zero user impact), not
      // absolute mid-month parity; parity requires the WP-4 DO→D1 reconcile/seed.
      const doMeterMode = (env as unknown as { EDGE_DO_METER?: string }).EDGE_DO_METER;
      if (
        doMeterMode === "shadow" &&
        inc.counted &&
        env.REQUEST_METER_SHARD_DO &&
        env.REQUEST_METER_COORDINATOR_DO &&
        env.R2_CAS_REGION
      ) {
        const doMeterCap = QUOTAS[quotaTier.tier].requestsPerMonthMax;
        if (doMeterCap !== Number.MAX_SAFE_INTEGER) {
          const d1WithinCap =
            inc.count <= FREE_REQUEST_CAP ||
            requestCapResultForCount(inc.count, quotaTier.tier).ok;
          const tier = quotaTier.tier;
          const region = env.R2_CAS_REGION;
          const shardNs = env.REQUEST_METER_SHARD_DO;
          const coordNs = env.REQUEST_METER_COORDINATOR_DO;
          ctx.waitUntil(
            meterViaDO(
              { shard: shardNs, coordinator: coordNs },
              {
                tenantId: resolvedTenantId,
                region,
                yearMonth: currentYearMonthUtc(),
                cap: doMeterCap,
                block: Math.min(doMeterCap, DO_METER_LEASE_BLOCK),
                lowWater: 0,
              },
            )
              .then((v) => {
                console.log(
                  JSON.stringify({
                    evt: "do_meter_shadow",
                    tier,
                    region,
                    d1WithinCap,
                    doWithinCap: v.withinCap,
                    agree: d1WithinCap === v.withinCap,
                    refilled: v.refilled,
                  }),
                );
              })
              .catch(() => {
                /* telemetry must never break the request */
              }),
          );
        }
      }

      // Monthly request-count quota (red-team #5): compare the already-counted
      // value against the tenant's RESOLVED tier cap. No re-increment. Within
      // the free cap → already within every cap, skip the comparison. Over the
      // free cap → a free tenant rejects HERE; a paid tenant under its (higher)
      // cap passes through. Checked BEFORE the storage verdict so the 429 a
      // client sees for a doubly-over-cap tenant is unchanged by the batching.
      const withinFreeCap = !inc.counted || inc.count <= FREE_REQUEST_CAP;
      if (!withinFreeCap) {
        const requestCheck = requestCapResultForCount(inc.count, quotaTier.tier);
        if (!requestCheck.ok) {
          return quotaExceeded(requestCheck.reason, requestCheck.retryAfterSec);
        }
      }

      // Storage quota verdict. (OCI never reaches this PAT-gate path — see the
      // dedicated pass-through branch above; the container enforces OCI quota.)
      // Storage is ALWAYS enforced — serve concerns ONLY the request-count cap.
      const storageCheck = quotaBatch.storage;
      if (!storageCheck.ok) {
        return quotaExceeded(storageCheck.reason, storageCheck.retryAfterSec);
      }

      // P3 serve: the edge-local DO is AUTHORITATIVE for the request-count cap.
      // Enforce its verdict here — AFTER storage — REPLACING the D1 `withinFreeCap`
      // gate above (which is inert on the serve path: meter=false ⇒ inc.counted=
      // false ⇒ withinFreeCap=true, so it never fires). `serveVerdict` is non-null
      // only when the DO metered this request (serve active + no DO error). The
      // 429 shape matches requestCapResultForCount: Retry-After = seconds to the
      // next UTC-month reset (the DO count is authoritative, so no D1 `count` is
      // available to echo).
      if (serveVerdict !== null && !serveVerdict.withinCap) {
        return quotaExceeded(
          `Monthly request quota exceeded: limit is ${doServeCap} (tier: ${quotaTier.tier})`,
          secondsUntilNextMonthStart(),
        );
      }

      // WP-B2: seed the request-count KV from this authoritative count so
      // subsequent reads for this tenant can arm the async fast path. Only when
      // we actually counted (not a fan-out / kill-switch / uncounted fault) and
      // the flag is engaged (shadow or on). Best-effort, off the response path.
      if (
        (asyncMeterMode === "on" || asyncMeterMode === "shadow") &&
        inc.counted &&
        tierKv !== undefined
      ) {
        populateRequestCountKv(resolvedTenantId, inc.count, {
          kv: tierKv,
          waitUntil: ctx.waitUntil.bind(ctx),
        });
      }
      }
    }

    // WI-MULTI-REGION-V1: per-tenant region routing. Look up tenant.primary_region
    // from D1, map the MACRO code to its serving colo via the FROZEN region map,
    // and fan-out to the regional Worker via Service Binding when the colo is
    // non-IAD. (backlog #29 — Schrems II residency leak.)
    //
    // The previous code routed by literal colo strings ("lhr"/"nrt"/"syd") that
    // the macro `primary_region` NEVER equals (D1 holds wnam/enam/weur/sam/...),
    // so EU `weur` tenants never matched "lhr" and stayed on IAD = US storage.
    // The colo map removes that vocabulary mismatch.
    //
    // FAIL-CLOSED for non-IAD residency regions: if the macro maps to a non-IAD
    // colo but the matching Service Binding is missing, OR if the D1 lookup
    // throws, we return 503 — we DO NOT fall through to the IAD path, because
    // that fall-through is precisely the cross-border leak. wnam/enam map to IAD
    // (the local path) so they legitimately fall through to the DO below.
    // F-014: recompute the forgery-safe fan-out marker in THIS scope (the metering
    // block's isFanout above is scoped to that sibling block). Same constant-time
    // secret check — a fanned-out request (already on a regional Worker) must not
    // re-route through the region fan-out below (the regional envs lack PROD_*
    // bindings → 503). Pure check, no side effects.
    const regionFanoutHeader = request.headers.get("x-corelink-fanout-from");
    const isFanout =
      regionFanoutHeader !== null &&
      typeof env.CORELINK_INTERNAL_AUTH_KEY === "string" &&
      env.CORELINK_INTERNAL_AUTH_KEY.length > 0 &&
      constantTimeSecretEqual(env.CORELINK_INTERNAL_AUTH_KEY, regionFanoutHeader);

    // F-015 (overnight red-team): primaryRegion is hoisted OUT of the block so the
    // LOCAL DO path below can also stamp x-corelink-primary-region (not just the
    // fan-out path). Without that stamp, a client hitting a public regional host
    // (e.g. lhr.corelink-api.humangr.com) directly with a non-matching tenant
    // reaches that region's container with NO residency header → the container
    // guard's absent→Allow → cross-region placement. Stamping it lets the
    // container backstop (residency.rs) reject the mismatch.
    let primaryRegion: string | undefined;
    if (
      resolvedTenantId !== "_anonymous" &&
      resolvedTenantId !== "_system" &&
      resolvedTenantId !== "_pending"
    ) {
      // Residency is resolved through the three-tier cache (L1 isolate → L2 KV
      // `tres:` → L3 D1) so a far-from-D1 (e.g. SAM) caller no longer pays a
      // synchronous D1-PRIMARY round-trip per request for this near-immutable
      // value (latency WP — the `wdb` phase). FAIL-CLOSED is preserved: an
      // unresolved region (D1 fault with no cached fallback) still 503s rather
      // than IAD-leaking; a stale cached region cannot SILENTLY leak because the
      // container residency backstop (residency.rs) 409s any real cross-region
      // mismatch.
      const residencyKv = (env as unknown as { METADATA_KV?: KvReader }).METADATA_KV;
      const residStart = Date.now();
      const residency = await resolveTenantResidency(env.CONFIG_DB, resolvedTenantId, {
        ...(residencyKv ? { kv: residencyKv } : {}),
        waitUntil: ctx.waitUntil.bind(ctx),
      });
      stQResidMs = Date.now() - residStart;
      if (residency === RESIDENCY_UNRESOLVED) {
        // D1 hiccup with no cached region: we cannot establish residency.
        // FAIL-CLOSED — refuse rather than risk routing an EU tenant to US
        // storage on a transient error.
        return applyCors(
          new Response(
            JSON.stringify({
              error: "RESIDENCY_UNAVAILABLE",
              message: "could not resolve tenant data-residency region",
              request_id: requestId,
            }),
            {
              status: 503,
              headers: { "Content-Type": "application/json", "X-Request-Id": requestId },
            },
          ),
          request,
        );
      }
      // A `null` region (no tenant row / no pin) ⇒ `undefined`, preserving the
      // existing `primaryRegion === undefined` IAD-local fall-through below.
      primaryRegion = residency.region ?? undefined;

      const colo = primaryRegion !== undefined ? coloForMacro(primaryRegion) : undefined;
      // Non-IAD residency: must fan-out to the matching regional Service Binding.
      // colo === undefined here means an unknown/unprovisioned macro (e.g. afr) —
      // also fail-closed (never serve such a tenant from IAD).
      // F-014 (overnight red-team): gate the fan-out on !isFanout. A request that
      // ALREADY arrived as a fan-out (isFanout — running on a regional Worker)
      // must NOT re-route: the regional envs lack the PROD_* service bindings, so
      // re-entering this branch would hit `regionalBinding === undefined` → 503 for
      // every non-IAD tenant. A fanned-out request falls through to the local DO
      // path on the regional Worker (which serves its own region) instead.
      if (!isFanout && primaryRegion !== undefined && colo !== "iad") {
        let regionalBinding: { fetch: typeof fetch } | undefined;
        if (colo === "lhr") regionalBinding = env.PROD_LHR;
        else if (colo === "sam") regionalBinding = env.PROD_SAM;
        else if (colo === "nrt") regionalBinding = env.PROD_NRT;

        if (regionalBinding === undefined) {
          // FAIL-CLOSED: non-IAD residency but the regional binding is missing
          // (or the macro is unprovisioned/unknown). Refuse — do NOT fall
          // through to IAD, which would store the tenant's data cross-border.
          return applyCors(
            new Response(
              JSON.stringify({
                error: "RESIDENCY_UNAVAILABLE",
                message: `data-residency region '${primaryRegion}' is not currently servable`,
                request_id: requestId,
              }),
              {
                status: 503,
                headers: { "Content-Type": "application/json", "X-Request-Id": requestId },
              },
            ),
            request,
          );
        }

        // Forward verbatim to the regional Worker. The regional Worker re-runs
        // PAT validation (PAT_SIGNING_KEY is shared across envs) + writes to
        // its regional R2 bucket. Service Binding bypasses CF edge error 1014
        // (CNAME Cross-User Banned). See cf_worker_to_worker_service_binding.
        const regionalReq = new Request(request, {
          headers: (() => {
            const h = new Headers(request.headers);
            // Strip ALL client-suppliable trust headers BEFORE the Worker
            // sets its own (delete-then-set). The Worker legitimately sets
            // x-corelink-fanout-from + x-corelink-primary-region below from
            // server-trusted values.
            stripClientTrustHeaders(h);
            h.set("x-request-id", requestId);
            h.set("x-corelink-route-kind", route.routeKind);
            h.set("x-corelink-token-prefix", auth.tokenPrefix);
            h.set("x-corelink-tenant-id", resolvedTenantId);
            // Storage-quota fail-open fix: forward the resolved per-tier storage
            // cap so the regional container seeds a fresh tenant_storage_state
            // row with the REAL cap (not uncapped `0`). Omitted when null
            // (system/anon/pending or D1-error tier) ⇒ container fails closed.
            // stripClientTrustHeaders above already deleted any client value.
            if (storageQuotaHeader !== null) {
              h.set(STORAGE_QUOTA_HEADER, storageQuotaHeader);
            }
            // H1: forward the D1-resolved PAT scope as a server-trust header.
            // stripClientTrustHeaders above already deleted any client value.
            h.set("x-corelink-scope", auth.scope);
            // WP5a: forward the NARROWED runner-job markers when the resolved PAT
            // carries a non-NULL runner_job_ac_key. Same posture as x-corelink-scope:
            // stripClientTrustHeaders above already deleted any client-supplied copies
            // of both headers (the Worker is the sole setter, from trusted D1). A
            // normal PAT (null) sets NEITHER header.
            if (auth.runnerJobAcKey !== null) {
              h.set("x-corelink-runner-job", "1");
              h.set("x-corelink-ac-key-allow", auth.runnerJobAcKey);
              // anti AC-squat: EVERY runner-job cred is create-only (deny-overwrite)
              // on the AC — the AC analog of the deny-DELETE narrowing at the SAME
              // chokepoint. The container (scope.rs::RunnerJob::ac_create_only) reads
              // this as first-writer-wins: CREATE ok, OVERWRITE of an existing entry
              // ⇒ 409. Stripped above, so only the Worker's value reaches the container.
              h.set("x-corelink-ac-create-only", "1");
            }
            // F1: forward CF's unforgeable client IP as x-corelink-client-ip
            // (the client-forgeable x-forwarded-for was stripped above) so the
            // regional Worker/container rate-limits signup off a trusted IP.
            h.set("x-corelink-client-ip", request.headers.get("cf-connecting-ip") ?? "");
            // #11 (forgery-safe): mark the internal fan-out with the shared
            // server-to-server secret CORELINK_INTERNAL_AUTH_KEY, NOT a guessable
            // literal. The regional Worker accepts the fan-out (and so SKIPS
            // re-metering) only on a constant-time match against this same secret
            // (see the isFanout gate above) — a client-forged x-corelink-fanout-from
            // can never match, so it can never bypass metering. The value flows
            // ONLY over this service binding (set AFTER stripClientTrustHeaders),
            // never back to a client. If the secret is unbound the header is
            // omitted ⇒ the regional Worker meters (fail-SAFE over-count, never a
            // bypass).
            if (env.CORELINK_INTERNAL_AUTH_KEY && env.CORELINK_INTERNAL_AUTH_KEY.length > 0) {
              h.set("x-corelink-fanout-from", env.CORELINK_INTERNAL_AUTH_KEY);
            }
            // backlog #29: the trusted residency macro the container's residency
            // guard cross-checks against its own R2_CAS_REGION (defence-in-depth
            // against a mis-bound regional Worker). Set AFTER the strip so no
            // client value survives.
            h.set("x-corelink-primary-region", primaryRegion as string);
            return h;
          })(),
        });
        const regionalResp = await regionalBinding.fetch(regionalReq);
        return applyCors(regionalResp, request);
      }
      // colo === "iad" (wnam/enam) or primaryRegion undefined (tenant with no
      // row): fall through to the local IAD DO path below.
    }

    // Route to the per-tenant DO. idFromName(resolvedTenantId) guarantees
    // each tenant gets its own isolated DO — never the shared "_pending_auth".
    const doId = env.CORELINK_SERVER.idFromName(resolvedTenantId);
    const stub = env.CORELINK_SERVER.get(doId, serverGetOpts(env));

    // Augment request with correlation headers (no body inspection — INV-NO-BODY-IN-LOGS)
    const augmented = new Request(request, {
      headers: (() => {
        const h = new Headers(request.headers);
        // Security (H4): strip ALL client-suppliable server-trust headers on the
        // data-plane forward. The Worker does NOT set internal-auth here, so the
        // container's internal-auth-gated admin routes become Worker-unreachable
        // by design — admin is operator-only (internal-auth path) posture.
        stripClientTrustHeaders(h);
        h.set("x-request-id", requestId);
        h.set("x-corelink-route-kind", route.routeKind);
        // Pass token prefix for DO-side audit correlation (NOT the raw token).
        h.set("x-corelink-token-prefix", auth.tokenPrefix);
        // Pass the PAT-resolved tenant to the DO so it can bind tenantId in
        // lifecycle state (resolves the null tenantId — WP-T1 DoD 4). Set AFTER
        // Worker auth; the DO MUST NOT trust any client-supplied value for it
        // (overwritten here unconditionally).
        h.set("x-corelink-tenant-id", resolvedTenantId);
        // Storage-quota fail-open fix: forward the resolved per-tier storage cap
        // so the container's byte-accounting reservation seeds a fresh
        // tenant_storage_state row with the REAL cap (not the legacy uncapped
        // `0`). Omitted when null (system/anon/pending or a D1-error tier) ⇒ the
        // container fails closed on an unseeded tenant (absence ≠ unlimited).
        // stripClientTrustHeaders above already deleted any client value (the
        // Worker is the sole setter — exactly like x-corelink-tenant-id).
        if (storageQuotaHeader !== null) {
          h.set(STORAGE_QUOTA_HEADER, storageQuotaHeader);
        }
        // H1: forward the D1-resolved PAT scope as a server-trust header so the
        // container can ENFORCE it. stripClientTrustHeaders above already deleted
        // any client-supplied x-corelink-scope (the Worker is the sole setter).
        h.set("x-corelink-scope", auth.scope);
        // WP5a: forward the NARROWED runner-job markers when the resolved PAT
        // carries a non-NULL runner_job_ac_key so the container (WP5b) enforces
        // deny-DELETE (+ optional exact-key). Same posture as x-corelink-scope:
        // stripClientTrustHeaders above already deleted any client-supplied copies
        // of both headers (the Worker is the sole setter, from trusted D1). A
        // normal PAT (null) sets NEITHER header.
        if (auth.runnerJobAcKey !== null) {
          h.set("x-corelink-runner-job", "1");
          h.set("x-corelink-ac-key-allow", auth.runnerJobAcKey);
          // anti AC-squat: EVERY runner-job cred is create-only (deny-overwrite) on
          // the AC — the AC analog of the deny-DELETE narrowing at the SAME
          // chokepoint. The container (scope.rs::RunnerJob::ac_create_only) reads this
          // as first-writer-wins: CREATE ok, OVERWRITE of an existing entry ⇒ 409.
          // Stripped above, so only the Worker's value reaches the container.
          h.set("x-corelink-ac-create-only", "1");
        }
        // F1: forward Cloudflare's UNFORGEABLE client IP as the server-trusted
        // x-corelink-client-ip so the container's signup rate-limit keys off it
        // (NOT the client-forgeable x-forwarded-for, which stripClientTrustHeaders
        // already deleted above). cf-connecting-ip is set by the CF edge and a
        // client cannot spoof it. The signup routeKind reaches THIS forward.
        h.set("x-corelink-client-ip", request.headers.get("cf-connecting-ip") ?? "");
        // Keep raw Authorization on the forwarded request: the DO proxies it
        // to the container. NOTE (F3/F17): the native plane (CAS/AC/Bazel/Turbo)
        // does NOT perform Argon2id re-verify — possession rests solely on the
        // Worker's HMAC gate above. The DO is trusted; it never logs the raw
        // value (INV-NO-PII-IN-LOGS enforced in durable_object.ts).
        // TODO(F3): wire adapter_pat::PatVerifier onto the native plane as a
        // container-side second possession layer (Option-B extension).
        // F-015: stamp the trusted residency macro on the LOCAL path too (not only
        // the fan-out path), so the container's residency backstop (residency.rs)
        // can cross-check it against its own R2_CAS_REGION. For an IAD-resident
        // tenant on the IAD container this maps enam/wnam→iad==iad → Allow (no
        // change); for a client that picked a regional public host with a
        // non-matching tenant it maps to a different colo → 409 residency_violation.
        // Omitted for anon/system/pending (primaryRegion undefined). stripClientTrustHeaders
        // already deleted any client-supplied value (the Worker is the sole setter).
        if (primaryRegion !== undefined) {
          h.set("x-corelink-primary-region", primaryRegion);
        }
        return h;
      })(),
    });

    // F3.3 F2 SERVE: on a brew/pip `_public` HIT, serve the bytes from the Worker
    // edge (native CONFIG_DB map + CAS_BUCKET R2) and SKIP the container round-trip
    // entirely — the whole point of F3.3, removing `origin` (~585 ms) from the HIT
    // path. Any miss / revocation / re-hash mismatch / fault yields null and we fall
    // through to the unchanged container path (which owns the upstream fill), so this
    // can only make a HIT faster, never change correctness. Reached only on the
    // tenant's home-region leg, so env.CAS_BUCKET/R2_CAS_REGION are correct here.
    // We deliberately do NOT stamp stOrigin*, so Server-Timing omits `origin` — the
    // absence of that phase IS the wire-level proof the container was bypassed.
    let edgeServed: Response | null = null;
    if (
      env.EDGE_PUBLIC_READ === "serve" &&
      request.method === "GET" &&
      (route.routeKind === "brew" || route.routeKind === "pip")
    ) {
      try {
        const edge = await readPublicHit(env, route.routeKind, route.pathSuffix, ctx);
        if (edge) {
          // $-CEILING EXEMPTION (owner decision 2026-08-16): a `_public` cache
          // HIT served from the edge deliberately does NOT pass through the
          // container's per-op $-ceiling gate (ADR-0068). A shared, deduped,
          // content-addressed public read is ~free to serve, and the product's
          // whole promise is "the cache is cheap+fast" — charging the spend cap
          // on the cheapest, most-shared traffic class is off-brand. Request-count
          // + storage quota still apply (runQuotaBatch, above). Documented as an
          // invariant in the F3.3 ADR; not an accidental bypass.
          const total = edge.bytes.byteLength;
          const baseHeaders: Record<string, string> = {
            "x-cache": "HIT",
            // Faithful type for opaque CAS blobs (bottles/wheels); the container
            // binary read returns the same. Previously dropped on the edge path.
            "Content-Type": PUBLIC_BLOB_CONTENT_TYPE,
            "Accept-Ranges": "bytes",
          };
          const range = parseByteRange(request.headers.get("Range"), total);
          if (range === "unsatisfiable") {
            edgeServed = new Response(null, {
              status: 416,
              headers: { ...baseHeaders, "Content-Range": `bytes */${total}` },
            });
          } else if (range) {
            edgeServed = new Response(edge.bytes.slice(range.start, range.end + 1), {
              status: 206,
              headers: {
                ...baseHeaders,
                "Content-Range": `bytes ${range.start}-${range.end}/${total}`,
              },
            });
          } else {
            edgeServed = new Response(edge.bytes, { status: 200, headers: baseHeaders });
          }
        }
      } catch (e: unknown) {
        // An edge-read fault must NEVER fail a request that the container can serve.
        console.error(
          `[${requestId}] edge_public_serve error: ${String(e).slice(0, 80)}`,
        );
      }
    }

    let doResponse: Response;
    if (edgeServed) {
      doResponse = edgeServed;
    } else {
      try {
        stOriginStart = Date.now();
        doResponse = await stub.fetch(augmented);
        stOriginEnd = Date.now();
      } catch (err: unknown) {
        const message = err instanceof Error ? err.message : "unknown error";
        // Do NOT include error detail that could leak internal topology
        console.error(`[${requestId}] DO fetch failed: ${message.slice(0, 80)}`);
        // OCI never reaches this PAT-gate DO forward — see the dedicated
        // pass-through branch above (which has its own DO forward + error path).
        return applyCors(
          reapiError("INTERNAL_ERROR", "upstream error", 500, requestId),
          request,
        );
      }
    }

    // F3.3 SHADOW: prove Worker-native `_public` edge-read parity vs the container
    // on real traffic, with ZERO user impact — we serve the container's response
    // unchanged and only compare a CLONE in the background (`ctx.waitUntil`).
    // Reached only on the tenant's home-region leg (non-local regions early-return
    // above), so `env.CAS_BUCKET`/`env.R2_CAS_REGION` are correct for this tenant.
    // Gate on the "shadow" flag; brew/pip GETs only (the `_public` byte surfaces).
    if (
      env.EDGE_PUBLIC_READ === "shadow" &&
      request.method === "GET" &&
      (route.routeKind === "brew" || route.routeKind === "pip") &&
      doResponse.ok
    ) {
      const shadowClone = doResponse.clone();
      const shadowKind = route.routeKind;
      const shadowPath = route.pathSuffix;
      ctx.waitUntil(
        shadowCompareEdgePublicRead(env, shadowKind, shadowPath, shadowClone)
          .then((verdict) =>
            console.log(
              `[${requestId}] edge_public_shadow routeKind=${shadowKind} verdict=${verdict}`,
            ),
          )
          .catch((e) =>
            console.error(
              `[${requestId}] edge_public_shadow error: ${String(e).slice(0, 80)}`,
            ),
          ),
      );
    }

    // Map DO 404 responses through timing-pad (cross-tenant enumeration defence)
    if (doResponse.status === 404) {
      await applyTimingPad(
        requestStart,
        requestId,
        requestCounter,
        getServerNonce(),
      );
    }

    // Attach request-id to outbound response if DO didn't already set it
    const finalHeaders = new Headers(doResponse.headers);
    if (!finalHeaders.has("x-request-id")) {
      finalHeaders.set("x-request-id", requestId);
    }
    // Server-Timing (observability, self-serve latency probes): split the authed
    // hot path into `auth` (PAT verify — KV-served ⇒ single-digit ms), `wdb` (the
    // worker-side quota + residency reads to the ENAM primary) and its three
    // serial sub-phases, and `origin` (the DO/container subrequest) and ITS
    // sub-phases — `ohop` (the DO hop, derived here) plus `opat` / `oquota` /
    // `ostore` / `oother`, which the container reports on the subresponse and
    // `originSubPhases` merges. This is what lets a client distinguish "auth is
    // slow" from "the downstream D1 reads are slow" from "the container's own
    // per-request D1 read is slow" WITHOUT a log grep. `desc` on `auth` carries
    // the cache tier (`kv`/`l1`/`d1`).
    //
    // EMISSION CONTRACT — a phase that RAN is emitted, INCLUDING at `dur=0`; only
    // a phase that did NOT run is omitted. This matters more than it looks: a
    // Worker's `Date.now()` advances only across I/O, so a KV-served or L1-served
    // phase genuinely measures 0, and suppressing it would make "served from cache,
    // faster than the clock resolves" indistinguishable from "never executed" on
    // the wire. `auth`/`wdb`/`origin` used to be emitted on a strict `>`, which is
    // exactly that bug: the 2026-08-04 production probe reported `auth` on 3 of 30
    // responses and the missing 27 had to be INFERRED to be KV-served rather than
    // skipped. They now gate on "did this phase run?" like the sub-phases do.
    //
    // Durations are coarse by construction (Date.now advances across I/O only) —
    // directional attribution, never a profile.
    {
      const st: string[] = [];
      // `stAuthEnd > 0` ⇔ the auth phase ran (both clocks start at 0 and are only
      // ever set to a real Date.now()). Pre-tenant `signup` never enters it.
      if (stAuthEnd > 0) {
        const d = stAuthEnd - stAuthStart;
        st.push(stPatSource ? `auth;dur=${d};desc="${stPatSource}"` : `auth;dur=${d}`);
      }
      // `wdb` spans auth-end → origin-start, so it ran iff BOTH ends are stamped.
      // The `stAuthEnd > 0` half is load-bearing beyond the ran-check: on a route
      // that skipped auth it would otherwise compute `stOriginStart - 0`, i.e. the
      // epoch, and publish it as a duration.
      if (stAuthEnd > 0 && stOriginStart > 0) {
        st.push(`wdb;dur=${stOriginStart - stAuthEnd}`);
      }
      // The three awaits `wdb` is made of, in execution order. `-1` is the
      // did-not-run sentinel (see the declarations); anything >= 0 ran and is
      // reported at its real cost, 0 included. `qbatch` is ONE D1 round trip
      // carrying the metering UPSERT and the storage SUM — the two phases that
      // shipped as `qmeter` and `qstor` and were the whole of `wdb`.
      for (const [name, ms] of [
        ["qtier", stQTierMs],
        ["qbatch", stQBatchMs],
        ["qresid", stQResidMs],
      ] as const) {
        if (ms >= 0) st.push(`${name};dur=${ms}`);
      }
      // `origin` and, immediately after it, the sub-phases it decomposes into —
      // the same parent-then-children order `wdb` and its `q*` phases use. The
      // container reports its own share on the SUBRESPONSE's `Server-Timing`
      // (see `crate::origin_timing` in the Rust container); the Worker owns the
      // `ohop` residue because only the Worker can see both ends of the DO hop.
      // The container's raw header never reaches the client: `finalHeaders` is a
      // copy of the DO response's headers and the `set` below overwrites it.
      if (stOriginEnd > 0) {
        const originMs = stOriginEnd - stOriginStart;
        st.push(`origin;dur=${originMs}`);
        for (const sub of originSubPhases(originMs, doResponse.headers.get("server-timing"))) {
          st.push(sub);
        }
      }
      st.push(`total;dur=${Date.now() - requestStart}`);
      if (st.length > 0) finalHeaders.set("Server-Timing", st.join(", "));
    }
    const finalResponse = new Response(doResponse.body, {
      status: doResponse.status,
      statusText: doResponse.statusText,
      headers: finalHeaders,
    });

    return applyCors(finalResponse, request);
  },
};

// ──────────────────────────────────────────────────────────────────────────────
// Sentry error-tracking wrapper (observability)
// ──────────────────────────────────────────────────────────────────────────────
//
// Mirrors apps/analytics-worker/src/index.ts EXACTLY: init is gated on
// `env.SENTRY_DSN` (empty DSN ⇒ the SDK treats init as a no-op), so the hook is
// COMPLETELY inert until the operator sets the secret. `withSentry` captures any
// unhandled error thrown out of the fetch handler before the runtime 500s, with
// no behavior change to the (already error-mapped) success paths.
//
// INV-NO-PII-IN-LOGS: scrub PII/secrets from every event before it leaves the
// Worker — not just header KEYS but message/exception bodies + extra/contexts
// VALUES (see ./sentry-scrub.ts) — and sendDefaultPii=false so Sentry never
// auto-attaches request bodies / IPs.

const handler = Sentry.withSentry(
  (env: Env) => ({
    // Empty string when the secret is unset → Sentry SDK init is a no-op.
    dsn: env.SENTRY_DSN ?? "",
    environment: env.ENVIRONMENT,
    release: env.SENTRY_RELEASE ?? "unknown",
    sendDefaultPii: false,
    tracesSampleRate: 0.1,
    sampleRate: 1.0,
    beforeSend(event: Sentry.ErrorEvent) {
      return scrubSentryEvent(event);
    },
    beforeSendTransaction(event) {
      return scrubSentryEvent(event);
    },
  }),
  // `@sentry/cloudflare` re-bundles `@cloudflare/workers-types`; the cast keeps
  // both type graphs happy without weakening the inner handler's types.
  baseHandler as unknown as Parameters<typeof Sentry.withSentry>[1],
) as ExportedHandler<Env>;

export default handler;
export {
  CoreLinkServer,
  RolloutController,
  EventLogDO,
  ReplicationCoordinatorDO,
  RequestMeterCoordinatorDO,
  RequestMeterShardDO,
};

/**
 * Whether an internal route participates in the GDPR cross-residency erase
 * FAN-OUT (see the "erasure completeness across residency" block in `fetch`).
 * The fan-out replays a per-jurisdiction BYTE side-effect to every regional
 * worker and treats the local 2xx as "complete" only when every region confirms
 * — so it is correct ONLY for routes whose cross-region work is a side-effect
 * confirmed by STATUS, never for routes whose response BODY is the payload or
 * that write the single global D1. Hence an explicit allowlist, NOT
 * `startsWith("/_internal/dsr/")`:
 *   • `/erase`  — deletes the region's R2 CAS/AC bytes           → fan out
 *   • `/verify` — re-confirms deletion + signs VerifiedComplete  → fan out
 * Everything else under `/_internal/dsr/*` takes the single-local path:
 * `/access`, `/portability` return a gathered export from the ONE global D1 (the
 * caller returns the LOCAL body, so fanning them out is useless and only adds
 * spurious 502s); `/rectification`, `/anchor` are single global-D1 writes.
 * Fanning `/anchor` was the live GDPR 502 — it 502'd unless all 4
 * (anchor-unprovisioned) regions also 2xx'd. (Hoisted; used in `fetch` above.)
 */
export function isDsrEraseFanoutPath(pathSuffix: string): boolean {
  return pathSuffix === "/_internal/dsr/erase" || pathSuffix === "/_internal/dsr/verify";
}

/**
 * The `origin` sub-phases the CONTAINER reports, in emission order.
 *
 * They are produced by `crates/corelink-container/src/origin_timing.rs` on the
 * subresponse's own `Server-Timing`, and they partition the time the container
 * held the request:
 *
 *   - `opat`   — the container's per-request D1 `pat` row read (#1022 kept it
 *                so a revocation takes effect immediately)
 *   - `oquota` — the per-tenant monthly `$`-ceiling check/accrue (ADR-0068)
 *   - `ostore` — the moat storage lookup: url-map read + CAS/R2 blob
 *   - `oother` — the container's own residue (routing, rate-limit layer, HMAC,
 *                Argon2id or its memo hit, response assembly)
 *
 * `oother` is a residue the container computes against its OWN whole-request
 * clock, so these always sum to the container's total. (Hoisted; used in
 * `fetch` above.)
 */
const ORIGIN_CONTAINER_PHASES = ["opat", "oquota", "ostore", "oother"] as const;

/**
 * Decompose `origin` into `ohop` + the container's own phases.
 *
 * # Why the residue is the Worker's job
 *
 * `origin` is the Worker's clock around `stub.fetch()`. The container can time
 * everything it does, but it cannot see the DO hop that brackets it, so the
 * Worker derives that by SUBTRACTION and publishes it as an explicit phase:
 *
 *   `ohop` = `origin` − Σ(container phases)
 *
 * `ohop` covers the Worker→DO dispatch + placement RPC, the DO's own prologue
 * (tenant-id lifecycle bind, `ensureContainerRunning`, the `getAlarm()` re-arm
 * read), the DO→container HTTP wire, and the response travelling back. Those
 * four are NOT separated: splitting the DO's prologue out needs the DO to
 * rewrite the subresponse headers on the hot path, which is a behaviour change
 * this instrumentation deliberately does not make. If `ohop` turns out to
 * dominate, that is the next split — the decomposition, not a guess, decides.
 *
 * # The reconciliation rule
 *
 * `ohop` + Σ(container phases) === `origin`, exactly, always. Two cases would
 * break that and both are handled by refusing to publish a split rather than by
 * publishing one that does not add up:
 *
 *   - **no container report** (an older container image, or a response the DO
 *     synthesized itself — a 503 `CONTAINER_UNAVAILABLE` never reached the
 *     container): return nothing. `origin` stands alone exactly as before, which
 *     is what lets this Worker deploy ahead of the container repin.
 *   - **a report that does not fit** (Σ > `origin`, or a known phase name with
 *     an unparseable duration): return a single
 *     `ohop;dur=<origin>;desc="unreconciled"`. The split is dropped whole, and
 *     the `desc` says so on the wire. A split that silently redistributes a
 *     phase it could not read is worse than no split — it invites a confident
 *     wrong conclusion, which is the entire failure mode this instrumentation
 *     exists to prevent.
 *
 * ⚠️ The two sides do NOT share a clock resolution. The Worker's `Date.now()`
 * advances only across I/O, so a Worker phase at `dur=0` means "no I/O"; the
 * container measures a real monotonic `Instant`, so a container phase at
 * `dur=0` means "under a millisecond of wall time". `ohop` is a difference of
 * the two and carries ±1 ms of truncation either way — directional attribution,
 * never a profile.
 */
export function originSubPhases(originMs: number, containerTiming: string | null): string[] {
  if (containerTiming === null) return [];
  const reported = new Map<string, number>();
  let malformed = false;
  for (const part of containerTiming.split(",")) {
    // `name;dur=<int>` with optional trailing parameters (e.g. a `desc`).
    const m = /^\s*([A-Za-z0-9_-]+)\s*;\s*dur=([0-9]+)\s*(?:;.*)?$/.exec(part);
    const name = m === null ? part.trim().split(";")[0]?.trim() : m[1];
    if (name === undefined || !(ORIGIN_CONTAINER_PHASES as readonly string[]).includes(name)) {
      // Not one of ours — a future container phase, or a stray metric. Ignore
      // it rather than treating the whole report as broken.
      continue;
    }
    if (m === null) {
      // A name we DO consume, carrying a duration we cannot read. Its
      // milliseconds would silently land in `ohop` and be attributed to the
      // network. Refuse the split instead.
      malformed = true;
      continue;
    }
    reported.set(name, Number(m[2]));
  }
  if (reported.size === 0) return [];
  let sum = 0;
  for (const v of reported.values()) sum += v;
  if (malformed || sum > originMs) {
    return [`ohop;dur=${originMs};desc="unreconciled"`];
  }
  const out = [`ohop;dur=${originMs - sum}`];
  for (const name of ORIGIN_CONTAINER_PHASES) {
    const v = reported.get(name);
    // Emission contract, identical to the `wdb` sub-phases: a phase that RAN is
    // emitted INCLUDING at `dur=0`; only a phase the container did not run at
    // all is absent.
    if (v !== undefined) out.push(`${name};dur=${v}`);
  }
  return out;
}
