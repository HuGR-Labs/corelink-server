/**
 * CoreLinkServer Durable Object — container lifecycle manager + gRPC proxy.
 *
 * Responsibilities:
 *   1. Container lifecycle: start on cold wake, stop on idle timeout.
 *   2. Health probe: HTTP GET /_health on container port 50051.
 *   3. Request multiplexing: HTTP request → gRPC call on the container.
 *   4. Lifecycle telemetry: emit audit events BEFORE each state mutation.
 *   5. Per-tenant pinning: DO ID = idFromName(tenantId) — never cross-tenant.
 *
 * Cloudflare Containers beta API (wrangler 4.x / workers-types 4.20260526.1):
 *   - `state.container` — the Container instance bound to this DO.
 *   - `container.start(options?)` — starts the container (returns void).
 *   - `container.running` — true if container is live.
 *   - `container.getTcpPort(port)` → `Fetcher` — HTTP fetcher for container port.
 *   - `container.monitor()` → `Promise<void>` — resolves when container exits.
 *   - `container.destroy()` — terminate the container.
 *   - `container.setInactivityTimeout(ms)` — auto-destroy on idle.
 *
 * Charter constraints:
 *   - Audit emit BEFORE state mutation.
 *   - INV-NO-BODY-IN-LOGS: body bytes NEVER logged.
 *   - INV-NO-PII-IN-LOGS: tenant IDs hashed before tracing.
 *   - Constant-time PAT compare via crypto.subtle.timingSafeEqual.
 *   - Container is started fresh per cold-start; idle timeout triggers stop.
 */

import type {
  DurableObject,
  DurableObjectState,
  DurableObjectStorage,
  Container,
  Fetcher,
} from "@cloudflare/workers-types";
import type { Env } from "./index.js";

// ──────────────────────────────────────────────────────────────────────────────
// Types
// ──────────────────────────────────────────────────────────────────────────────

/** Lifecycle state persisted in DO storage. */
interface LifecycleState {
  readonly containerStatus: ContainerStatus;
  readonly lastHealthCheckMs: number;
  readonly coldStartCount: number;
  readonly tenantId: string | null;
  /**
   * Wall-clock (ms) when `containerStatus` last flipped to `"starting"`. Drives
   * stale-`"starting"` recovery in `ensureContainerRunning`. Optional for
   * back-compat with lifecycle states persisted before this field existed
   * (an absent value reads as 0 → treated as immediately stale → self-heals).
   */
  readonly startingAt_ms?: number;
}

type ContainerStatus = "stopped" | "starting" | "running" | "degraded";

/** Telemetry event shape (emitted to PagerDuty change events API + audit). */
interface LifecycleEvent {
  readonly event_type: string;
  readonly routing_key: string;
  readonly payload: {
    readonly summary: string;
    readonly severity: "info" | "warning" | "error" | "critical";
    readonly source: string;
    readonly custom_details: {
      readonly tenant_id_hash: string;
      readonly do_id_hash: string;
      readonly cold_start_count: number;
      readonly environment: string;
      readonly timestamp_ms: number;
    };
  };
}

// ──────────────────────────────────────────────────────────────────────────────
// Constants
// ──────────────────────────────────────────────────────────────────────────────

const CONTAINER_PORT = 50051;
/** Idle timeout before container is destroyed (ms). 5 minutes. */
const IDLE_TIMEOUT_MS = 5 * 60 * 1000;
/** Health check interval when container is running (ms). */
const HEALTH_CHECK_INTERVAL_MS = 30_000;
/** Max consecutive health-check failures before marking degraded. */
const MAX_HEALTH_FAILURES = 3;
/** Container startup health-poll timeout (ms). */
// Bumped to 90s on 2026-05-28: the container's routes-build path now eagerly
// constructs both the R2 CAS S3 client and the R2 AC S3 client (block_in_place
// + block_on against the real R2 endpoint), each ~10-15s for DNS + SigV4 +
// initial connection. 30s wasn't enough on cold start; observed real failures
// at 26s. Per-handler lazy init would cut this back, but the bump is the
// surgical worker-only fix.
const STARTUP_TIMEOUT_MS = 90_000;

/**
 * A `"starting"` status older than this is treated as a DEAD start (the
 * initiating isolate was evicted, or the start was interrupted mid-flight) and
 * self-healed by restarting. WITHOUT this, a stuck persisted `"starting"` (loaded
 * from storage on every isolate) traps every request in `waitForContainerReady`
 * forever — the 2026-06-23 `_system` wedge (F-020). Must be > `STARTUP_TIMEOUT_MS`
 * so a legitimately in-flight cold start is never pre-empted.
 */
const STALE_STARTING_MS = STARTUP_TIMEOUT_MS + 30_000;

// ──────────────────────────────────────────────────────────────────────────────
// Hashing helpers (INV-NO-PII-IN-LOGS)
// ──────────────────────────────────────────────────────────────────────────────

async function hashForLog(value: string): Promise<string> {
  const enc = new TextEncoder();
  const buf = await crypto.subtle.digest("SHA-256", enc.encode(value));
  const arr = new Uint8Array(buf);
  return Array.from(arr.slice(0, 8))
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}

// ──────────────────────────────────────────────────────────────────────────────
// Constant-time comparison
// ──────────────────────────────────────────────────────────────────────────────

/**
 * Per-isolate random HMAC key for {@link timingSafeEqual}.
 *
 * F22 (2026-06-13 audit): the previous implementation used an all-zero key,
 * which offers no confidentiality if an attacker can observe the HMAC output.
 * The key MUST be a real secret. It is generated once per isolate from a CSPRNG
 * and never leaves this module; HMAC-ing both inputs under a key the attacker
 * does not know makes the post-HMAC byte comparison non-forgeable.
 */
let hmacKeyPromise: Promise<CryptoKey> | undefined;
function getHmacKey(): Promise<CryptoKey> {
  if (hmacKeyPromise === undefined) {
    const raw = crypto.getRandomValues(new Uint8Array(32));
    hmacKeyPromise = crypto.subtle.importKey(
      "raw",
      raw,
      { name: "HMAC", hash: "SHA-256" },
      false,
      ["sign"],
    );
  }
  return hmacKeyPromise;
}

/**
 * Constant-time bytes equality.
 *
 * HMAC-SHA256s both inputs under a per-isolate random key, then XOR-compares
 * the two 32-byte tags. Because HMAC-SHA256 always yields a fixed 32-byte
 * output regardless of input length, NO length branch is taken — equal and
 * unequal-length inputs run the exact same two HMACs and the same fixed-width
 * compare, so there is no length-equality timing oracle (F22). The random key
 * (not a zero key) means the post-HMAC tags cannot be forged or replayed.
 * Equivalent to Rust's `subtle::ConstantTimeEq`.
 */
async function timingSafeEqual(a: string, b: string): Promise<boolean> {
  const enc = new TextEncoder();
  const aBytes = enc.encode(a);
  const bBytes = enc.encode(b);
  const key = await getHmacKey();
  // Both HMACs run unconditionally regardless of length — HMAC-SHA256 emits a
  // fixed 32-byte tag, so the comparison below is always over equal widths and
  // there is no length-dependent fast path.
  const [sigA, sigB] = await Promise.all([
    crypto.subtle.sign("HMAC", key, aBytes),
    crypto.subtle.sign("HMAC", key, bBytes),
  ]);
  const viewA = new Uint8Array(sigA);
  const viewB = new Uint8Array(sigB);
  let diff = 0;
  for (let i = 0; i < viewA.length; i++) {
    diff |= (viewA[i] ?? 0) ^ (viewB[i] ?? 0);
  }
  return diff === 0;
}

// Keep the export so tests can call it directly
export { timingSafeEqual };

// ──────────────────────────────────────────────────────────────────────────────
// PagerDuty telemetry (fire-and-forget)
// ──────────────────────────────────────────────────────────────────────────────

/**
 * Emit a lifecycle event to PagerDuty Change Events API.
 * ALWAYS called BEFORE the state mutation it describes.
 *
 * INV-NO-PII-IN-LOGS: tenant_id is pre-hashed before emission.
 */
async function emitLifecycleEvent(
  routingKey: string,
  eventType: string,
  summary: string,
  severity: "info" | "warning" | "error" | "critical",
  tenantIdHash: string,
  doIdHash: string,
  coldStartCount: number,
  environment: string,
): Promise<void> {
  if (routingKey.length === 0) return;

  const event: LifecycleEvent = {
    event_type: eventType,
    routing_key: routingKey,
    payload: {
      summary,
      severity,
      source: "corelink-do",
      custom_details: {
        tenant_id_hash: tenantIdHash,
        do_id_hash: doIdHash,
        cold_start_count: coldStartCount,
        environment,
        timestamp_ms: Date.now(),
      },
    },
  };

  try {
    const resp = await fetch("https://events.pagerduty.com/v2/change/enqueue", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(event),
    });
    if (!resp.ok) {
      console.error(`[do] PagerDuty emit failed status=${resp.status}`);
    }
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message.slice(0, 80) : "unknown";
    console.error(`[do] PagerDuty emit threw: ${msg}`);
  }
}

// ──────────────────────────────────────────────────────────────────────────────
// Container proxy helpers
// ──────────────────────────────────────────────────────────────────────────────

/**
 * Forward an HTTP request to the running container via the Fetcher obtained
 * from `container.getTcpPort(CONTAINER_PORT)`.
 *
 * The Rust gRPC server (corelink-server) speaks HTTP/2 natively (tonic +
 * tonic-web). The Fetcher routes HTTP to the container port. We preserve the
 * full path + query string so the gRPC-gateway transcoding inside the Rust
 * binary handles protocol routing.
 *
 * INV-NO-BODY-IN-LOGS: we never read or log the body.
 */
async function proxyToContainer(request: Request, fetcher: Fetcher): Promise<Response> {
  const url = new URL(request.url);
  const containerUrl = `http://localhost:${CONTAINER_PORT}${url.pathname}${url.search}`;

  const proxied = new Request(containerUrl, {
    method: request.method,
    headers: request.headers,
    body: request.method !== "GET" && request.method !== "HEAD" ? request.body : null,
    // @ts-expect-error duplex is required for streaming request bodies
    duplex: request.method !== "GET" && request.method !== "HEAD" ? "half" : undefined,
  });

  return fetcher.fetch(proxied);
}

// ──────────────────────────────────────────────────────────────────────────────
// Durable Object class
// ──────────────────────────────────────────────────────────────────────────────

export class CoreLinkServer implements DurableObject {
  private readonly state: DurableObjectState;
  private readonly storage: DurableObjectStorage;
  private readonly env: Env;
  private lifecycleState: LifecycleState = {
    containerStatus: "stopped",
    lastHealthCheckMs: 0,
    coldStartCount: 0,
    tenantId: null,
  };
  private healthFailures = 0;
  private idleTimer: ReturnType<typeof setTimeout> | null = null;
  private doIdHash = "";

  constructor(state: DurableObjectState, env: Env) {
    this.state = state;
    this.storage = state.storage;
    this.env = env;

    // Restore persisted lifecycle state on DO wakeup
    void this.state.blockConcurrencyWhile(async () => {
      const stored = await this.storage.get<LifecycleState>("lifecycle");
      if (stored !== undefined) {
        this.lifecycleState = stored;
      }
      this.doIdHash = await hashForLog(state.id.toString());
    });
  }

  // ──────────────────────────────────────────────────────────────────────────
  // fetch — DO entry point
  // ──────────────────────────────────────────────────────────────────────────

  async fetch(request: Request): Promise<Response> {
    const requestId = request.headers.get("x-request-id") ?? crypto.randomUUID();
    const url = new URL(request.url);

    // Internal DO management paths
    if (url.pathname === "/_do/health") {
      return this.handleHealthProbe(requestId);
    }
    if (url.pathname === "/_do/stop") {
      return this.handleStop(requestId);
    }

    // ── Tenant resolution (WP-T1) ─────────────────────────────────────────────
    // The Worker sets x-corelink-tenant-id to the PAT-resolved tenant before
    // forwarding. Bind it into the lifecycle state the first time we see it
    // (or on every request — idempotent since DO ID is already tenant-derived).
    // This resolves the null tenantId that was hardcoded before WP-T1.
    const incomingTenantId = request.headers.get("x-corelink-tenant-id");
    if (
      incomingTenantId !== null &&
      incomingTenantId.length > 0 &&
      incomingTenantId !== this.lifecycleState.tenantId
    ) {
      await this.updateLifecycleState({
        ...this.lifecycleState,
        tenantId: incomingTenantId,
      });
    }

    // Ensure container is running before forwarding
    const started = await this.ensureContainerRunning(requestId);
    if (!started.ok) {
      return new Response(
        JSON.stringify({
          error: "CONTAINER_UNAVAILABLE",
          message: started.reason,
          request_id: requestId,
        }),
        {
          status: 503,
          headers: { "Content-Type": "application/json", "X-Request-Id": requestId },
        },
      );
    }

    const container = this.state.container;
    if (container === undefined || !container.running) {
      return new Response(
        JSON.stringify({
          error: "INTERNAL_ERROR",
          message: "container not running after start",
          request_id: requestId,
        }),
        {
          status: 500,
          headers: { "Content-Type": "application/json", "X-Request-Id": requestId },
        },
      );
    }

    // Reset idle timer on every request
    this.resetIdleTimer();

    // Get the TCP-port Fetcher for gRPC port 50051
    const fetcher = container.getTcpPort(CONTAINER_PORT);

    try {
      const resp = await proxyToContainer(request, fetcher);
      return resp;
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message.slice(0, 80) : "unknown";
      console.error(`[${requestId}] container proxy error: ${msg}`);

      // Mark container degraded — next request will attempt restart
      await this.transitionStatus("degraded", requestId);

      return new Response(
        JSON.stringify({
          error: "UPSTREAM_ERROR",
          message: "container error",
          request_id: requestId,
        }),
        {
          status: 502,
          headers: { "Content-Type": "application/json", "X-Request-Id": requestId },
        },
      );
    }
  }

  // ──────────────────────────────────────────────────────────────────────────
  // Container lifecycle
  // ──────────────────────────────────────────────────────────────────────────

  /** Ensure the container is running, starting it if necessary. */
  private async ensureContainerRunning(
    requestId: string,
  ): Promise<{ ok: true } | { ok: false; reason: string }> {
    const container = this.state.container;

    if (
      this.lifecycleState.containerStatus === "running" &&
      container !== undefined &&
      container.running
    ) {
      return { ok: true };
    }

    // STALE-RUNNING DETECTION:
    // If our persisted lifecycle says "running" but the Container binding
    // is gone or no longer running, we hit this case after a wrangler
    // deploy rotation (CF tears down the old container; lifecycleState
    // still has the pre-rotation status). The previous logic fell through
    // to "unexpected_lifecycle_state" and never self-healed — clients got
    // permanent 503 until manual intervention. Treat this exactly like
    // "stopped" and restart the container.
    if (this.lifecycleState.containerStatus === "running") {
      console.warn(
        `[${requestId}] lifecycleState=running but container is not — treating as stopped`,
      );
      await this.transitionStatus("stopped", requestId);
      return this.startContainer(requestId);
    }

    if (
      this.lifecycleState.containerStatus === "degraded" ||
      this.lifecycleState.containerStatus === "stopped"
    ) {
      return this.startContainer(requestId);
    }

    if (this.lifecycleState.containerStatus === "starting") {
      // STALE-STARTING DETECTION (mirrors STALE-RUNNING above): a `"starting"`
      // older than STALE_STARTING_MS means the start that set it died WITHOUT
      // transitioning (isolate evicted mid-start, or a request flood interrupted
      // it). Because lifecycleState is loaded from storage on every isolate, that
      // stale `"starting"` would otherwise trap EVERY request in
      // waitForContainerReady forever (no self-heal — unlike `"running"` above);
      // this is the 2026-06-23 `_system` wedge (F-020). Treat it as stopped and
      // restart. A legitimately in-flight start has a recent startingAt_ms
      // (< STALE_STARTING_MS, which is > STARTUP_TIMEOUT_MS) → still waits.
      const startedAt = this.lifecycleState.startingAt_ms ?? 0;
      if (Date.now() - startedAt > STALE_STARTING_MS) {
        console.warn(
          `[${requestId}] lifecycleState=starting but stale (>${STALE_STARTING_MS}ms, no live start) — treating as stopped`,
        );
        await this.transitionStatus("stopped", requestId);
        return this.startContainer(requestId);
      }
      return this.waitForContainerReady(requestId);
    }

    return { ok: false, reason: "unexpected_lifecycle_state" };
  }

  /**
   * Start the container.
   *
   * Telemetry order (charter requirement — BEFORE mutation):
   *   1. Emit corelink.do.cold_start.v1 BEFORE calling container.start().
   *   2. Emit corelink.do.container_started.v1 after health confirmed.
   *   3. Emit corelink.do.container_died.v1 if start fails.
   */
  private async startContainer(
    requestId: string,
  ): Promise<{ ok: true } | { ok: false; reason: string }> {
    const container = this.state.container;
    if (container === undefined) {
      return { ok: false, reason: "no_container_binding" };
    }

    // CONCURRENT-START GUARD (REV-S2):
    // Cloudflare DOs are single-threaded but ASYNC-concurrent — each `await`
    // below (emitLifecycleEvent, transitionStatus, container.start) is a yield
    // point at which another queued fetch() can run. If two requests arrive
    // while status is "stopped"/"degraded", both pass the ensureContainerRunning
    // check and both enter startContainer, double-calling container.start() and
    // double-counting cold starts. Closing the race requires flipping the
    // IN-MEMORY status to "starting" SYNCHRONOUSLY here — before the first await
    // — so any concurrent request that runs ensureContainerRunning next sees
    // "starting" and falls into the waitForContainerReady branch instead of
    // re-entering this method. (We avoid blockConcurrencyWhile here so we do not
    // serialize ALL fetches for the full ~90s startup window; the in-memory flip
    // is sufficient because the check and this flip are in the same microtask
    // turn with no intervening await.) The persisted write happens via
    // transitionStatus below; the in-memory field is the load-bearing guard.
    if (this.lifecycleState.containerStatus === "starting") {
      // A concurrent caller already won the start; defer to the wait path.
      return this.waitForContainerReady(requestId);
    }
    // Synchronous in-memory flip (containerStatus is readonly → replace the object).
    // Stamp startingAt_ms so a start that later dies without transitioning is
    // detectable as stale by ensureContainerRunning (F-020 self-heal).
    this.lifecycleState = {
      ...this.lifecycleState,
      containerStatus: "starting",
      startingAt_ms: Date.now(),
    };

    const tenantHash = await hashForLog(this.lifecycleState.tenantId ?? "_unknown");
    const newColdStartCount = this.lifecycleState.coldStartCount + 1;

    // AUDIT BEFORE MUTATION
    await emitLifecycleEvent(
      this.env.PAGERDUTY_ROUTING_KEY ?? "",
      "corelink.do.cold_start.v1",
      `CoreLink DO cold start #${newColdStartCount} for tenant ${tenantHash}`,
      "info",
      tenantHash,
      this.doIdHash,
      newColdStartCount,
      this.env.ENVIRONMENT,
    );

    // Persist the "starting" status (the in-memory flip above already closed the
    // concurrent-start race; this durably records it across DO eviction).
    await this.transitionStatus("starting", requestId);

    try {
      // Start container — returns void; container begins asynchronously
      container.start({
        // Egress required (P0-5): the native container reaches R2 (S3 API) and
        // D1 (HTTP API) over the public internet — DECISION-GATE-1 Option A.
        // CF Workers has no VPC-style internal route to R2 for native containers.
        enableInternet: true,
        entrypoint: ["/usr/local/bin/corelink-server"],
        env: {
          RUST_LOG: "info",
          PORT: String(CONTAINER_PORT),
          // WP-S1 StorageEnv contract: all six must be present + non-empty for
          // the container to use real R2/D1 storage. Any missing/empty → the
          // container falls back to InMemory (dev/CI without secrets). These are
          // sourced from Worker vars (endpoint, db id) + secrets (keys, token).
          R2_S3_ENDPOINT: this.env.R2_S3_ENDPOINT ?? "",
          R2_S3_ACCESS_KEY_ID: this.env.R2_S3_ACCESS_KEY_ID ?? "",
          R2_S3_SECRET_ACCESS_KEY: this.env.R2_S3_SECRET_ACCESS_KEY ?? "",
          CLOUDFLARE_ACCOUNT_ID: this.env.CLOUDFLARE_ACCOUNT_ID ?? "",
          CF_API_TOKEN: this.env.CF_API_TOKEN ?? "",
          D1_DATABASE_ID: this.env.D1_DATABASE_ID ?? "",
          // Stream-5: internal PAT mint route gate secrets.
          // Container mounts `/_internal/pat/mint` only when both are non-empty.
          CORELINK_INTERNAL_AUTH_KEY: this.env.CORELINK_INTERNAL_AUTH_KEY ?? "",
          // CP-1 (go-live audit): the container's `resolve_internal_auth_key`
          // prefers a per-consumer DEDICATED key and falls back to the shared
          // one. Those dedicated keys MUST be forwarded or (a) the blast-radius
          // isolation is inert (everything gates on the shared key) AND (b) the
          // moment an operator provisions a dedicated key, the container — never
          // receiving it — 401s every mint/admin/erase call (a self-inflicted
          // outage). Forward them (empty when unset ⇒ shared fallback, unchanged).
          CORELINK_PAT_MINT_AUTH_KEY: this.env.CORELINK_PAT_MINT_AUTH_KEY ?? "",
          CORELINK_ADMIN_AUTH_KEY: this.env.CORELINK_ADMIN_AUTH_KEY ?? "",
          CORELINK_ERASE_AUTH_KEY: this.env.CORELINK_ERASE_AUTH_KEY ?? "",
          PAT_SIGNING_KEY: this.env.PAT_SIGNING_KEY ?? "",
          // L3 money path: `POST /v1/onboarding/tier-select` runs INSIDE the
          // container and reads these from its OWN process env
          // (`tier_select::build_state_from_env` + `StripeRealClient::from_env`).
          // They MUST be forwarded or the route stays unmounted (404, missing
          // CORELINK_DPA_VERSION) and Stripe checkout 500s (missing price ids).
          STRIPE_SECRET_KEY: this.env.STRIPE_SECRET_KEY ?? "",
          STRIPE_AUTH_MODE: this.env.STRIPE_AUTH_MODE ?? "",
          STRIPE_WEBHOOK_SECRET: this.env.STRIPE_WEBHOOK_SECRET ?? "",
          STRIPE_PRICE_ID_SOLO: this.env.STRIPE_PRICE_ID_SOLO ?? "",
          STRIPE_PRICE_ID_STARTER: this.env.STRIPE_PRICE_ID_STARTER ?? "",
          STRIPE_PRICE_ID_TEAM: this.env.STRIPE_PRICE_ID_TEAM ?? "",
          STRIPE_PRICE_ID_PRO: this.env.STRIPE_PRICE_ID_PRO ?? "",
          STRIPE_PRICE_ID_MAX: this.env.STRIPE_PRICE_ID_MAX ?? "",
          // Runners-tier prices: the container's seed handler
          // (`build_runners_resolver` in main.rs) reads these from its OWN env
          // to map a Runners-tier Stripe subscription → `runners_entitlement`.
          // Absent ⇒ the Runners seed stays dormant (cache path only).
          STRIPE_PRICE_ID_RUNNER_STARTER: this.env.STRIPE_PRICE_ID_RUNNER_STARTER ?? "",
          STRIPE_PRICE_ID_RUNNER_PRO: this.env.STRIPE_PRICE_ID_RUNNER_PRO ?? "",
          STRIPE_PRICE_ID_RUNNER_TEAM: this.env.STRIPE_PRICE_ID_RUNNER_TEAM ?? "",
          STRIPE_PRICE_ID_RUNNER_SCALE: this.env.STRIPE_PRICE_ID_RUNNER_SCALE ?? "",
          STRIPE_PRICE_ID_RUNNER_MAX: this.env.STRIPE_PRICE_ID_RUNNER_MAX ?? "",
          CORELINK_DPA_VERSION: this.env.CORELINK_DPA_VERSION ?? "",
          // DSR Wave 1 (#254): the container's erasure adapters derive the
          // pseudonymization/idempotency salt from ERASURE_SALT_KEY. If absent the
          // container falls back to a PREDICTABLE non-secret salt — forward it so
          // the real secret is used (launch-required GDPR path).
          ERASURE_SALT_KEY: this.env.ERASURE_SALT_KEY ?? "",
          // DSR G3 (#269): the container signs an Ed25519 erasure attestation on
          // VerifiedComplete, reading the seed/key-id/region from these env vars
          // (`routes/dsr/attestation.rs` from_seed/key_id/resolve_region). They
          // MUST be forwarded or setting ERASURE_ATTESTATION_SEED_HEX later never
          // reaches the container and attestation silently no-ops (fail-OPEN) —
          // the exact ERASURE_SALT_KEY-class gap. (caught by check-env-contract.py)
          ERASURE_ATTESTATION_SEED_HEX: this.env.ERASURE_ATTESTATION_SEED_HEX ?? "",
          ERASURE_ATTESTATION_KEY_ID: this.env.ERASURE_ATTESTATION_KEY_ID ?? "",
          // CF-6: OPTIONAL dedicated audit-chain head-signing key (defaults to
          // reusing the erasure-attestation seed/key above when unset).
          AUDIT_CHAIN_SIGNING_SEED_HEX: this.env.AUDIT_CHAIN_SIGNING_SEED_HEX ?? "",
          AUDIT_CHAIN_SIGNING_KEY_ID: this.env.AUDIT_CHAIN_SIGNING_KEY_ID ?? "",
          ERASURE_ATTESTATION_REGION: this.env.ERASURE_ATTESTATION_REGION ?? "",
          // Brutal-audit #1 fix: the single-region assertion flag gates whether a
          // post-deletion attestation may sign with the env-default region. MUST be
          // forwarded or the operator flag silently never reaches the container.
          ERASURE_ATTESTATION_SINGLE_REGION: this.env.ERASURE_ATTESTATION_SINGLE_REGION ?? "",
          // corelink-runners auth seam (#261): `POST /internal/v1/auth/introspect`
          // mounts in the container only when FABRIC_INTROSPECT_AUTH_KEY (+ PAT +
          // D1) are present. Forward it or the route stays unmounted (404).
          FABRIC_INTROSPECT_AUTH_KEY: this.env.FABRIC_INTROSPECT_AUTH_KEY ?? "",
          // HuGR toolkits introspect consumer (#398): the per-consumer key the
          // container's auth_introspect gate ALSO accepts. Must be forwarded too —
          // else setting the worker secret never reaches the container and HuGR's
          // token 401s (the check-env-contract gap that caught this).
          FABRIC_INTROSPECT_AUTH_KEY_HUGR:
            this.env.FABRIC_INTROSPECT_AUTH_KEY_HUGR ?? "",
          // ASK-2 runner billing usage-push ingest: `POST /internal/v1/billing/usage`
          // mounts in the container only when BILLING_INGEST_AUTH_KEY (+ D1) are
          // present. Forward it or the route stays unmounted (404) — the same
          // env-contract class as FABRIC_INTROSPECT_AUTH_KEY above (check-env-contract.py).
          BILLING_INGEST_AUTH_KEY: this.env.BILLING_INGEST_AUTH_KEY ?? "",
          // Complete the env contract (2026-06-13 audit): every var the container
          // reads via env::var MUST be forwarded, else setting the secret later
          // silently never reaches the container (the class of bug that hid the
          // ERASURE_SALT_KEY gap). These are unset in prod today (features off /
          // CAS fallback-derivation), so forwarding empty strings is a no-op now
          // but makes a future secret-set "just work".
          R2_TDK_HEX: this.env.R2_TDK_HEX ?? "",
          SIGNUP_TOKEN_KEY: this.env.SIGNUP_TOKEN_KEY ?? "",
          // OCI token-mint signing key — read by the container's OCI adapter.
          // (Gap caught by scripts/check-env-contract.py on its first run, 2026-06-13.)
          CORELINK_OCI_TOKEN_KEY: this.env.CORELINK_OCI_TOKEN_KEY ?? "",
          // Legacy alias of CORELINK_OCI_TOKEN_KEY (CAA-360 #8 name drift): the
          // prod Worker holds the OCI signing key under HUGR_OCI_TOKEN_KEY, and the
          // container reads it via the routes.rs `.or_else(...)` fallback. Forward
          // it too — otherwise that fallback is a silent no-op (the value never
          // reaches the container). Retire once the prod secret is renamed to the
          // canonical name. See crates/corelink-container/src/routes/oci.rs
          // (OCI_TOKEN_KEY_ENV_LEGACY).
          HUGR_OCI_TOKEN_KEY: this.env.HUGR_OCI_TOKEN_KEY ?? "",
          CORELINK_PORTAL_RETURN_URL: this.env.CORELINK_PORTAL_RETURN_URL ?? "",
          // BYOK (enterprise) provider regions/vault — off for the SMB launch.
          AWS_REGION: this.env.AWS_REGION ?? "",
          GCP_REGION: this.env.GCP_REGION ?? "",
          CORELINK_BYOK_AZURE_REGION: this.env.CORELINK_BYOK_AZURE_REGION ?? "",
          CORELINK_BYOK_AZURE_VAULT_URL: this.env.CORELINK_BYOK_AZURE_VAULT_URL ?? "",
          CORELINK_BYOK_VAULT_REGION: this.env.CORELINK_BYOK_VAULT_REGION ?? "",
          // ADR-MULTI-REGION-V1 — per-region R2 bucket overrides.
          // Absent/empty → container defaults to IAD (corelink-ac-iad / iad).
          // Set by [env.prod-<region>].vars in wrangler.toml.
          R2_AC_BUCKET: this.env.R2_AC_BUCKET ?? "",
          R2_AC_REGION: this.env.R2_AC_REGION ?? "",
          R2_CHUNK_BUCKET: this.env.R2_CHUNK_BUCKET ?? "",
          R2_CHUNK_REGION: this.env.R2_CHUNK_REGION ?? "",
          // F7/F8 (2026-06-13 audit) — CAS residency. The container reads
          // R2_CAS_REGION/R2_CAS_BUCKET (`routes/cas.rs:125-126`) but they were
          // NOT forwarded, so the F7 residency fix (set R2_CAS_REGION per
          // regional env) would have silently no-op'd: the operator sets the
          // var, deploy succeeds, container keeps keying CAS under "iad". Each
          // [env.prod-<region>].vars now sets R2_CAS_REGION so EU/regional CAS
          // bytes key to their own region. Absent/empty → container defaults to
          // IAD (corelink-cas-prod / iad).
          R2_CAS_REGION: this.env.R2_CAS_REGION ?? "",
          R2_CAS_BUCKET: this.env.R2_CAS_BUCKET ?? "",
          // F8 — remaining container-read env vars missing from the forward
          // list: the AC R2 bucket prefix (`dsr/adapter_r2_ac.rs:68`) and the
          // Turborepo bucket (`storage/r2_kv.rs:178`). Unset in prod today
          // (defaults match intended values), but any operator override would
          // silently no-op without these — the ERASURE_SALT_KEY-class bug.
          R2_AC_BUCKET_PREFIX: this.env.R2_AC_BUCKET_PREFIX ?? "",
          R2_TURBO_BUCKET: this.env.R2_TURBO_BUCKET ?? "",
          // Brutal-audit M3 — container tuning knobs read via a const-aliased
          // `std::env::var(CONST)` (invisible to the old check-env-contract.py
          // string-literal scan, now caught). Each falls back to a built-in
          // default when unset, so an operator `wrangler secret put` was being
          // SILENTLY ignored — the ERASURE_SALT_KEY class. Forward them so an
          // override actually reaches the container (empty ⇒ default, unchanged):
          //   PAT_MINT_MAX_INFLIGHT     routes/internal_pat.rs:165 (MAX_INFLIGHT_MINTS_ENV)
          //   PAT_MINT_MAX_PER_MINUTE   routes/internal_pat.rs:298 (MAX_MINTS_PER_WINDOW_ENV)
          //   QUOTA_COST_PER_OP_MICROS  tenant_quota.rs:98       (COST_PER_OP_MICROS_ENV)
          //   EXPORT_ROW_BUFFER_BYTES   routes/audit_export/stream.rs:320 (ENV_EXPORT_ROW_BUFFER_BYTES)
          PAT_MINT_MAX_INFLIGHT: this.env.PAT_MINT_MAX_INFLIGHT ?? "",
          PAT_MINT_MAX_PER_MINUTE: this.env.PAT_MINT_MAX_PER_MINUTE ?? "",
          QUOTA_COST_PER_OP_MICROS: this.env.QUOTA_COST_PER_OP_MICROS ?? "",
          EXPORT_ROW_BUFFER_BYTES: this.env.EXPORT_ROW_BUFFER_BYTES ?? "",
        },
      });

      // Poll health until container is responsive or timeout
      const healthy = await this.waitForContainerHealth(requestId, container);
      if (!healthy) {
        await emitLifecycleEvent(
          this.env.PAGERDUTY_ROUTING_KEY ?? "",
          "corelink.do.container_died.v1",
          `CoreLink container failed health check on start for tenant ${tenantHash}`,
          "error",
          tenantHash,
          this.doIdHash,
          newColdStartCount,
          this.env.ENVIRONMENT,
        );
        await this.transitionStatus("stopped", requestId);
        return { ok: false, reason: "container_health_check_failed" };
      }

      await this.updateLifecycleState({
        ...this.lifecycleState,
        containerStatus: "running",
        lastHealthCheckMs: Date.now(),
        coldStartCount: newColdStartCount,
      });

      // AUDIT AFTER SUCCESSFUL START
      await emitLifecycleEvent(
        this.env.PAGERDUTY_ROUTING_KEY ?? "",
        "corelink.do.container_started.v1",
        `CoreLink container started for tenant ${tenantHash}`,
        "info",
        tenantHash,
        this.doIdHash,
        newColdStartCount,
        this.env.ENVIRONMENT,
      );

      this.resetIdleTimer();

      // Schedule alarm for periodic health checks
      const nextAlarm = Date.now() + HEALTH_CHECK_INTERVAL_MS;
      await this.storage.setAlarm(nextAlarm);

      return { ok: true };
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message.slice(0, 80) : "unknown";
      console.error(`[${requestId}] container start error: ${msg}`);

      await emitLifecycleEvent(
        this.env.PAGERDUTY_ROUTING_KEY ?? "",
        "corelink.do.container_died.v1",
        `CoreLink container start threw for tenant ${tenantHash}`,
        "error",
        tenantHash,
        this.doIdHash,
        newColdStartCount,
        this.env.ENVIRONMENT,
      );

      await this.transitionStatus("stopped", requestId);
      return { ok: false, reason: "container_start_threw" };
    }
  }

  /** Wait briefly for a container in "starting" state to become ready. */
  private async waitForContainerReady(
    requestId: string,
  ): Promise<{ ok: true } | { ok: false; reason: string }> {
    const deadline = Date.now() + STARTUP_TIMEOUT_MS;
    const container = this.state.container;
    while (Date.now() < deadline) {
      if (
        this.lifecycleState.containerStatus === "running" &&
        container !== undefined &&
        container.running
      ) {
        return { ok: true };
      }
      // M1: FAST-DEATH EXIT. A bad deploy (binary OOM/panic) flips the status to
      // the terminal-dead "stopped" state — there is nothing left starting to
      // wait for, so spinning the full STARTUP_TIMEOUT_MS only makes every queued
      // request hang ~90s before the inevitable 503. Bail immediately so the
      // caller returns a prompt 503 (and the next request triggers a restart via
      // ensureContainerRunning's stopped→startContainer branch). We break ONLY on
      // "stopped" (genuinely dead); "starting" is transient and still waits out
      // the 90s ceiling below, and "degraded" is handled on the next request.
      if (this.lifecycleState.containerStatus === "stopped") {
        console.error(`[${requestId}] container in terminal "stopped" state — fast-exit (no wait)`);
        return { ok: false, reason: "container_dead" };
      }
      await new Promise<void>((r) => setTimeout(r, 100));
    }
    console.error(`[${requestId}] timeout waiting for container to start`);
    return { ok: false, reason: "container_start_timeout" };
  }

  /**
   * Poll container health endpoint until healthy or timeout.
   * Uses HTTP GET /_health on the container port via getTcpPort fetcher.
   */
  private async waitForContainerHealth(
    requestId: string,
    container: Container,
  ): Promise<boolean> {
    const deadline = Date.now() + STARTUP_TIMEOUT_MS;
    let attempts = 0;
    while (Date.now() < deadline) {
      attempts++;
      if (!container.running) {
        // Container exited unexpectedly during startup
        break;
      }
      try {
        const fetcher = container.getTcpPort(CONTAINER_PORT);
        const healthReq = new Request(`http://localhost:${CONTAINER_PORT}/_health`, {
          method: "GET",
          headers: { "x-request-id": requestId },
        });
        const resp = await fetcher.fetch(healthReq);
        if (resp.status === 200) {
          this.healthFailures = 0;
          return true;
        }
      } catch (_err: unknown) {
        // Container still starting — retry
      }
      await new Promise<void>((r) => setTimeout(r, 500));
    }
    console.error(`[${requestId}] container health timed out after ${attempts} attempts`);
    return false;
  }

  /**
   * Health probe handler (/_do/health).
   * Also used by wrangler dev --local smoke test.
   */
  private async handleHealthProbe(requestId: string): Promise<Response> {
    const container = this.state.container;
    const containerRunning = container !== undefined && container.running;
    const status = this.lifecycleState.containerStatus;

    const body = JSON.stringify({
      status: containerRunning ? "ok" : status,
      container_status: status,
      container_running: containerRunning,
      cold_start_count: this.lifecycleState.coldStartCount,
      last_health_check_ms: this.lifecycleState.lastHealthCheckMs,
      request_id: requestId,
    });
    return new Response(body, {
      status: containerRunning ? 200 : 503,
      headers: { "Content-Type": "application/json", "X-Request-Id": requestId },
    });
  }

  /**
   * Stop the container (/_do/stop or idle-timeout trigger).
   * Telemetry: emits corelink.do.container_died.v1 BEFORE destroying.
   */
  private async handleStop(requestId: string): Promise<Response> {
    const tenantHash = await hashForLog(this.lifecycleState.tenantId ?? "_unknown");

    // AUDIT BEFORE MUTATION
    await emitLifecycleEvent(
      this.env.PAGERDUTY_ROUTING_KEY ?? "",
      "corelink.do.container_died.v1",
      `CoreLink container stopped (requested) for tenant ${tenantHash}`,
      "info",
      tenantHash,
      this.doIdHash,
      this.lifecycleState.coldStartCount,
      this.env.ENVIRONMENT,
    );

    await this.destroyContainer(requestId);

    return new Response(
      JSON.stringify({ stopped: true, request_id: requestId }),
      {
        status: 200,
        headers: { "Content-Type": "application/json", "X-Request-Id": requestId },
      },
    );
  }

  /** Destroy the container and update persisted state. */
  private async destroyContainer(requestId: string): Promise<void> {
    const container = this.state.container;
    if (container !== undefined && container.running) {
      try {
        await container.destroy();
      } catch (err: unknown) {
        const msg = err instanceof Error ? err.message.slice(0, 80) : "unknown";
        console.error(`[${requestId}] container.destroy() threw: ${msg}`);
      }
    }
    await this.transitionStatus("stopped", requestId);
    this.clearIdleTimer();
  }

  // ──────────────────────────────────────────────────────────────────────────
  // Idle timeout
  // ──────────────────────────────────────────────────────────────────────────

  private resetIdleTimer(): void {
    this.clearIdleTimer();
    this.idleTimer = setTimeout(() => {
      void this.onIdleTimeout();
    }, IDLE_TIMEOUT_MS);
  }

  private clearIdleTimer(): void {
    if (this.idleTimer !== null) {
      clearTimeout(this.idleTimer);
      this.idleTimer = null;
    }
  }

  private async onIdleTimeout(): Promise<void> {
    const tenantHash = await hashForLog(this.lifecycleState.tenantId ?? "_unknown");
    const requestId = "idle-timeout-" + crypto.randomUUID().slice(0, 8);

    // AUDIT BEFORE MUTATION
    await emitLifecycleEvent(
      this.env.PAGERDUTY_ROUTING_KEY ?? "",
      "corelink.do.container_died.v1",
      `CoreLink container stopped (idle timeout) for tenant ${tenantHash}`,
      "info",
      tenantHash,
      this.doIdHash,
      this.lifecycleState.coldStartCount,
      this.env.ENVIRONMENT,
    );

    await this.destroyContainer(requestId);
  }

  // ──────────────────────────────────────────────────────────────────────────
  // State helpers
  // ──────────────────────────────────────────────────────────────────────────

  private async transitionStatus(
    status: ContainerStatus,
    _requestId: string,
  ): Promise<void> {
    await this.updateLifecycleState({ ...this.lifecycleState, containerStatus: status });
  }

  private async updateLifecycleState(newState: LifecycleState): Promise<void> {
    this.lifecycleState = newState;
    await this.storage.put("lifecycle", newState);
  }

  // ──────────────────────────────────────────────────────────────────────────
  // Alarm — periodic health check
  // ──────────────────────────────────────────────────────────────────────────

  async alarm(): Promise<void> {
    const container = this.state.container;
    if (
      this.lifecycleState.containerStatus !== "running" ||
      container === undefined ||
      !container.running
    ) {
      return;
    }

    const requestId = "alarm-health-" + crypto.randomUUID().slice(0, 8);
    const now = Date.now();

    if (now - this.lifecycleState.lastHealthCheckMs < HEALTH_CHECK_INTERVAL_MS) {
      return;
    }

    try {
      const fetcher = container.getTcpPort(CONTAINER_PORT);
      const healthReq = new Request(`http://localhost:${CONTAINER_PORT}/_health`, {
        method: "GET",
        headers: { "x-request-id": requestId },
      });
      const resp = await fetcher.fetch(healthReq);
      if (resp.status === 200) {
        this.healthFailures = 0;
        await this.updateLifecycleState({ ...this.lifecycleState, lastHealthCheckMs: now });
      } else {
        this.healthFailures++;
        if (this.healthFailures >= MAX_HEALTH_FAILURES) {
          const tenantHash = await hashForLog(this.lifecycleState.tenantId ?? "_unknown");
          await emitLifecycleEvent(
            this.env.PAGERDUTY_ROUTING_KEY ?? "",
            "corelink.do.container_died.v1",
            `CoreLink container health degraded (${this.healthFailures} failures) for tenant ${tenantHash}`,
            "error",
            tenantHash,
            this.doIdHash,
            this.lifecycleState.coldStartCount,
            this.env.ENVIRONMENT,
          );
          await this.transitionStatus("degraded", requestId);
        }
      }
    } catch (_err: unknown) {
      this.healthFailures++;
      if (this.healthFailures >= MAX_HEALTH_FAILURES) {
        await this.transitionStatus("degraded", requestId);
      }
    }

    // Schedule next alarm
    const nextAlarm = now + HEALTH_CHECK_INTERVAL_MS;
    await this.storage.setAlarm(nextAlarm);
  }
}

// Re-export env augmentation so Env picks up secrets added in the DO layer
declare module "./index.js" {
  interface Env {
    PAGERDUTY_ROUTING_KEY?: string;
    // Brutal-audit M3 — container tuning knobs forwarded by container.start()
    // above (read in the container via const-aliased std::env::var). Optional:
    // unset ⇒ the container uses its built-in default.
    PAT_MINT_MAX_INFLIGHT?: string;
    PAT_MINT_MAX_PER_MINUTE?: string;
    QUOTA_COST_PER_OP_MICROS?: string;
    EXPORT_ROW_BUFFER_BYTES?: string;
  }
}
