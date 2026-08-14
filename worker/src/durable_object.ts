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
  /**
   * Wall-clock (ms) of the last REAL proxied request (or container start).
   * Drives the DURABLE idle reaper in `alarm()`. Touched in-memory on the
   * request hot path (zero storage cost) and persisted by the health-check
   * alarm's existing lifecycle write, so it is never more than one
   * `HEALTH_CHECK_INTERVAL_MS` stale after an isolate eviction — negligible
   * against `IDLE_TIMEOUT_MS`. Optional for back-compat: an absent value
   * (pre-fix persisted state) starts the idle clock at the next alarm, so a
   * previously-immortal container dies one idle window after this deploys.
   */
  readonly lastActivityMs?: number;
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

/**
 * One D1 read path (primary or nearest-replica) as measured by the
 * `/_do/health` placement instrument.
 *
 * The three failure modes are DELIBERATELY distinguishable — collapsing them is
 * how a broken instrument reports a fast number it never measured:
 *   - `available: false`            → the path could not be attempted at all.
 *   - `available: true, ok: false`  → attempted and THREW (`error` is non-null).
 *   - `available: true, ok: true`   → measured; `samples_ms` are real.
 */
interface D1PathProbeResult {
  /** Could this path be attempted at all (binding bound / Sessions API present)? */
  readonly available: boolean;
  /** Did every attempted sample succeed? False whenever `error` is non-null. */
  readonly ok: boolean;
  /** Wall-clock ms per sample, in order. Empty when the path was unavailable. */
  readonly samples_ms: readonly number[];
  /** Fastest sample — the number to read. `null` when nothing was measured. */
  readonly min_ms: number | null;
  /** Non-null iff a read threw or was unavailable. NEVER silently a number. */
  readonly error: string | null;
  /** D1 `meta.served_by_region` (e.g. "ENAM") of the last successful sample. */
  readonly served_by_region: string | null;
  /** D1 `meta.served_by_primary` — false proves a real replica served the read. */
  readonly served_by_primary: boolean | null;
  /** D1 `meta.served_by_colo` (e.g. "MIA") of the last successful sample. */
  readonly served_by_colo: string | null;
}

/** The `d1_probe` object added to the `/_do/health` body. Purely additive. */
interface D1ProbeReport {
  readonly probe_version: number;
  readonly samples: number;
  readonly binding_bound: boolean;
  readonly sessions_api_available: boolean;
  /** Uncounted first read that absorbs connection setup (the "cold" number). */
  readonly warmup_ms: number | null;
  readonly warmup_error: string | null;
  readonly primary: D1PathProbeResult;
  readonly replica: D1PathProbeResult;
  /** Serving colo of THIS DO — only with `?colo=1`, best-effort. */
  readonly do_colo: string | null;
  readonly do_colo_error: string | null;
}

/** Minimal structural shape of a D1 read handle (a DB or a session). */
interface D1ProbeHandle {
  prepare(query: string): { all(): Promise<unknown> };
}

/** A `CONFIG_DB` binding: a read handle that MAY expose the Sessions API. */
interface D1ProbeBinding extends D1ProbeHandle {
  withSession?: (constraint: string) => D1ProbeHandle;
}

// ──────────────────────────────────────────────────────────────────────────────
// Constants
// ──────────────────────────────────────────────────────────────────────────────

const CONTAINER_PORT = 50051;
/** Timed samples taken per D1 read path on `/_do/health`. */
const D1_PROBE_SAMPLES = 3;
/** Per-sample ceiling (ms). A hung D1 must not hang the health probe. */
const D1_PROBE_TIMEOUT_MS = 5_000;
/** Ceiling (ms) for the best-effort `?colo=1` trace fetch. */
const DO_COLO_TIMEOUT_MS = 2_000;
/** Bumped whenever the `d1_probe` shape changes, so a reader can tell. */
const D1_PROBE_VERSION = 1;
/**
 * Idle timeout before container is destroyed (ms). 30 minutes.
 *
 * Raised from 5→30min (2026-07-01): the ~2.5s cold-start is only paid on the
 * FIRST request after the container is reaped, and warm steady-state is already
 * fast (post-#368, sub-second). A 30-min idle window keeps a tenant's container
 * warm across normal work-session gaps (a coffee break / a meeting) so they
 * rarely re-pay the cold-start, while the container STILL dies after a bounded
 * idle tail — so the COGS is proportional to real activity (recently-active
 * tenants only), NOT a global always-on warm pool. The cheap, infra-free
 * version of WP-3 (`docs/perf/2026-06-19-cas-hot-path-latency.md`).
 *
 * ENFORCED IN `alarm()` (durable), NOT a `setTimeout`: the original in-memory
 * idle timer evaporated on every isolate eviction while the health-check
 * alarm chain survived, so any once-started container became immortal and
 * was billed 24/7 (2026-07 invoice: ~13 GiB resident memory around the
 * clock / $84 per month of Container Memory against $0.00 of billable
 * traffic). The reaper compares `lastActivityMs` (persisted lifecycle state)
 * against this window on every alarm tick and destroys the container +
 * ends the alarm chain when it expires.
 */
const IDLE_TIMEOUT_MS = 30 * 60 * 1000;
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
// D1 placement instrument helpers (used ONLY by /_do/health)
// ──────────────────────────────────────────────────────────────────────────────

/** Short, bounded error text. Never carries a body or a secret. */
function errText(err: unknown): string {
  return err instanceof Error ? err.message.slice(0, 160) : "unknown error";
}

/**
 * Reject after `ms` if `p` has not settled. The loser's rejection is absorbed
 * (`void p.catch`) so a late failure cannot surface as an unhandled rejection.
 */
function withTimeout<T>(p: Promise<T>, ms: number, label: string): Promise<T> {
  void p.catch(() => {});
  let timer: ReturnType<typeof setTimeout> | undefined;
  const timeout = new Promise<never>((_resolve, reject) => {
    timer = setTimeout(() => reject(new Error(`${label} timed out after ${ms}ms`)), ms);
  });
  return Promise.race([p, timeout]).finally(() => {
    if (timer !== undefined) clearTimeout(timer);
  }) as Promise<T>;
}

/** Extract D1's own `meta.served_by_*` provenance from a result, defensively. */
function readServedBy(result: unknown): {
  region: string | null;
  primary: boolean | null;
  colo: string | null;
} {
  const empty = { region: null, primary: null, colo: null };
  if (typeof result !== "object" || result === null) return empty;
  const meta = (result as { meta?: unknown }).meta;
  if (typeof meta !== "object" || meta === null) return empty;
  const m = meta as Record<string, unknown>;
  return {
    region: typeof m["served_by_region"] === "string" ? m["served_by_region"] : null,
    primary: typeof m["served_by_primary"] === "boolean" ? m["served_by_primary"] : null,
    colo: typeof m["served_by_colo"] === "string" ? m["served_by_colo"] : null,
  };
}

/**
 * One timed `SELECT 1` against a D1 read handle.
 *
 * `SELECT 1` touches no table, so what is measured is the ROUND TRIP to whatever
 * D1 instance serves the handle — which is exactly the placement question.
 */
async function timedD1Read(
  handle: D1ProbeHandle,
): Promise<{ ms: number | null; error: string | null; result: unknown }> {
  const started = Date.now();
  try {
    const result = await withTimeout(
      handle.prepare("SELECT 1").all(),
      D1_PROBE_TIMEOUT_MS,
      "d1 probe read",
    );
    return { ms: Date.now() - started, error: null, result };
  } catch (err: unknown) {
    // A throw is reported as an EXPLICIT error — never as a fast number and
    // never as a missing field. That distinction is the whole instrument.
    return { ms: null, error: errText(err), result: null };
  }
}

/** A path that could not be attempted at all (distinct from attempted-and-failed). */
function unavailablePath(reason: string): D1PathProbeResult {
  return {
    available: false,
    ok: false,
    samples_ms: [],
    min_ms: null,
    error: reason,
    served_by_region: null,
    served_by_primary: null,
    served_by_colo: null,
  };
}

/** Take {@link D1_PROBE_SAMPLES} timed reads on one handle; stop at the first throw. */
async function probeD1Path(handle: D1ProbeHandle): Promise<D1PathProbeResult> {
  const samples: number[] = [];
  let error: string | null = null;
  let servedBy: { region: string | null; primary: boolean | null; colo: string | null } = {
    region: null,
    primary: null,
    colo: null,
  };

  for (let i = 0; i < D1_PROBE_SAMPLES; i++) {
    const sample = await timedD1Read(handle);
    if (sample.error !== null || sample.ms === null) {
      error = sample.error ?? "no timing produced";
      break;
    }
    samples.push(sample.ms);
    const provenance = readServedBy(sample.result);
    if (provenance.region !== null || provenance.primary !== null || provenance.colo !== null) {
      servedBy = provenance;
    }
  }

  return {
    available: true,
    ok: error === null && samples.length === D1_PROBE_SAMPLES,
    samples_ms: samples,
    min_ms: samples.length > 0 ? Math.min(...samples) : null,
    error,
    served_by_region: servedBy.region,
    served_by_primary: servedBy.primary,
    served_by_colo: servedBy.colo,
  };
}

/**
 * Best-effort serving colo of THIS DO (`/_do/health?colo=1` only).
 *
 * An outbound `fetch` from a DO egresses through the colo the DO runs in, so
 * `cdn-cgi/trace` reports that colo. ADVISORY: it is a inference from an
 * external call, not a platform-attested placement API — read it as a hint that
 * EXPLAINS the timings, never as the timing itself. Off by default so the plain
 * health probe makes no external request.
 */
async function resolveDoColo(): Promise<{ colo: string | null; error: string | null }> {
  try {
    const resp = await withTimeout(
      fetch("https://workers.cloudflare.com/cdn-cgi/trace", {
        method: "GET",
        headers: { "Cache-Control": "no-cache" },
      }),
      DO_COLO_TIMEOUT_MS,
      "colo trace",
    );
    if (!resp.ok) return { colo: null, error: `trace status ${resp.status}` };
    const text = await resp.text();
    const line = text.split("\n").find((l) => l.startsWith("colo="));
    return line === undefined
      ? { colo: null, error: "no colo line in trace" }
      : { colo: line.slice("colo=".length).trim(), error: null };
  } catch (err: unknown) {
    return { colo: null, error: errText(err) };
  }
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
      // `?colo=1` additionally resolves this DO's serving colo (one best-effort
      // external fetch). Opt-in so the ordinary probe stays network-free.
      return this.handleHealthProbe(requestId, url.searchParams.get("colo") === "1");
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

    // Touch the durable idle clock on every REAL proxied request. In-memory
    // only (zero hot-path storage cost): the health-check alarm's periodic
    // lifecycle write persists it within one HEALTH_CHECK_INTERVAL_MS.
    this.lifecycleState = { ...this.lifecycleState, lastActivityMs: Date.now() };

    // SELF-HEAL the alarm chain. The alarm owns the reaper, so a lost chain =
    // an immortal container. `ensureContainerRunning` returns early (ok) for an
    // already-running container without arming anything, so a chain dropped by
    // CF (retries exhausted on a throwing tick) would never come back on its
    // own. `getAlarm()` is a cheap cached read; re-arm only when it is null.
    if ((await this.storage.getAlarm()) === null) {
      await this.storage.setAlarm(Date.now() + HEALTH_CHECK_INTERVAL_MS);
    }

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
          // H5 dual-approval: the container's `POST /v1/admin/approve` gate reads
          // a DEDICATED `CORELINK_ADMIN_APPROVER_AUTH_KEY` (distinct from the
          // mutate/admin key so approve+mutate need different keys — real
          // two-person control). Forward it or the container 401s every approve
          // call the moment the dedicated key is bound (the CP-1 self-inflicted
          // outage this block guards against). Empty when unset ⇒ shared fallback.
          CORELINK_ADMIN_APPROVER_AUTH_KEY: this.env.CORELINK_ADMIN_APPROVER_AUTH_KEY ?? "",
          // #634: the per-user DSR legitimacy-anchor route (`/_internal/dsr/anchor`)
          // reads a dedicated `CORELINK_DSR_ANCHOR_AUTH_KEY`; forward it too or the
          // container 401s every anchor call the moment the dedicated key is bound
          // (the exact CP-1 self-inflicted-outage this block guards against).
          CORELINK_DSR_ANCHOR_AUTH_KEY: this.env.CORELINK_DSR_ANCHOR_AUTH_KEY ?? "",
          // Read-only tenant-quota lookup (`/_internal/tenant/{tenant_id}/quota`):
          // the container's `tenant_quota_read::build_state_from_env` reads a
          // dedicated `CORELINK_QUOTA_READ_AUTH_KEY` (shared-key fallback). Forward
          // it or the container 401s every quota-read call the moment the dedicated
          // key is bound (the CP-1 self-inflicted-outage this block guards against).
          CORELINK_QUOTA_READ_AUTH_KEY: this.env.CORELINK_QUOTA_READ_AUTH_KEY ?? "",
          // DSR customer portal (union #717): the receipt-JWT signer
          // (`dsr/portal.rs:659`) reads `DSR_RECEIPT_SIGNING_KEY`; forward it or a
          // bound CF secret silently no-ops and the portal falls back to a weak
          // default (the F8/ERASURE_SALT class of self-inflicted bug).
          DSR_RECEIPT_SIGNING_KEY: this.env.DSR_RECEIPT_SIGNING_KEY ?? "",
          // DPA click-through acceptance (money-path unblock): the container's
          // `/v1/onboarding/dpa-accept` route (`dpa_accept::build_state_from_env`)
          // reads `DPA_RECEIPT_SIGNING_KEY` (RSA PKCS#8/PKCS#1 PEM) from its OWN
          // process env to RS256-sign the acceptance receipt. It MUST be forwarded
          // or the route stays UNMOUNTED (fail-CLOSED) and every paid checkout
          // 403s `dpa_required` (the DPA row never gets written).
          DPA_RECEIPT_SIGNING_KEY: this.env.DPA_RECEIPT_SIGNING_KEY ?? "",
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
          AUDIT_CHAIN_TRUST_UNSIGNED_RESUME: this.env.AUDIT_CHAIN_TRUST_UNSIGNED_RESUME ?? "",
          // Non-secret tuning knob (secrets-matrix #189): per-call row budget for
          // the audit/drain sweep. "" ⇒ container default (200). Forwarded so a
          // Worker-side var actually reaches the container process.
          AUDIT_DRAIN_BATCH_LIMIT: this.env.AUDIT_DRAIN_BATCH_LIMIT ?? "",
          // CTRL-PRIV-001: server-held salt for the email_hash pseudonym. Unset →
          // legacy unsalted SHA-256 (zero regression); set → HMAC-SHA256. MUST be
          // forwarded or the container can't see it when the owner registers it.
          EMAIL_HASH_SALT: this.env.EMAIL_HASH_SALT ?? "",
          // Optional launch coupon id. Set → checkout pre-applies discounts[0][coupon]
          // (clean checkout→$0); unset → allow_promotion_codes=true (promo-code field).
          // MUST be forwarded or the container's Stripe client can't see it when set.
          STRIPE_LAUNCH_COUPON: this.env.STRIPE_LAUNCH_COUPON ?? "",
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
          // reads via env::var MUST be forwarded UNCONDITIONALLY, else setting the
          // secret later silently never reaches the container (the class of bug
          // that hid the ERASURE_SALT_KEY gap). Some of these ARE set in prod
          // (verified 2026-08-10: R2_TDK_HEX is populated on prod + all regionals,
          // so CAS uses real per-tenant HMAC prefix derivation — NOT the fallback;
          // the OCI signing key is set under the legacy HUGR_OCI_TOKEN_KEY name
          // below). Others remain unset; forwarding an empty string when a var is
          // unset is a harmless no-op that makes a future secret-set "just work".
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

      // Arm the PLATFORM idle reaper immediately — before the health poll, so
      // a container that starts and then wedges on /_health is still reaped by
      // Cloudflare even if every line below this one fails to run.
      await this.armInactivityTimeout(container, requestId);

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
        // DESTROY, don't just re-label. `container.start()` already ran, so the
        // container may well be RESIDENT (up but wedged on /_health — the R2/S3
        // init alone was measured at 26s+). Marking lifecycle "stopped" without
        // destroying leaves it resident and BILLED with no alarm chain to reap
        // it — the same immortality this fix exists to close, entered through
        // the failure door. (This is the per-DO `container_start_threw` wedge
        // that previously cleared only on an image roll.)
        await this.destroyContainer(requestId);
        return { ok: false, reason: "container_health_check_failed" };
      }

      await this.updateLifecycleState({
        ...this.lifecycleState,
        containerStatus: "running",
        lastHealthCheckMs: Date.now(),
        lastActivityMs: Date.now(),
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

      // Schedule alarm for periodic health checks (which also runs the
      // durable idle reaper — see alarm())
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

      // DESTROY, don't just re-label — see the health-check arm above: the
      // throw may have happened AFTER container.start() took effect, leaving a
      // resident, billed, un-reapable container.
      await this.destroyContainer(requestId);
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
   * Arm Cloudflare's OWN idle auto-destroy for this container.
   *
   * `container.setInactivityTimeout(ms)` is the platform-side reaper: workerd
   * destroys the container after `ms` without activity, with no help from this
   * Worker. It has been named in this file's header doc-comment since day one
   * (see the `state.container` capability list) and was NEVER CALLED — the repo
   * hand-rolled the alarm reaper in `alarm()` instead. That reaper shipped in
   * #927 and never moved the live instance count off 35 (5 regions x 7 x 4 GiB
   * resident, `active: 0` in regions with zero traffic).
   *
   * Platform-side is strictly stronger than ours: it survives DO eviction, a
   * broken alarm chain and a wedged isolate — precisely the failure modes that
   * made containers immortal. The alarm reaper is KEPT as defence in depth and
   * because it also stops re-arming the chain, letting the DO itself hibernate
   * (a live alarm chain bills DO duration on its own).
   *
   * ⚠️ The type declaration (`worker-configuration.d.ts`, `interface Container`)
   * carries NO doc-comment, so it is UNSPECIFIED whether the timer restarts on
   * container activity or is an absolute deadline from the moment it is armed.
   * Under the absolute reading, arming once at start would kill a BUSY
   * container mid-request one window later. So this is called at start AND
   * re-armed from `alarm()` while the container is non-idle — correct under
   * both readings: idempotent if the platform already tracks activity, a
   * sliding window if it does not. Re-arming rides the existing alarm and
   * never the request path; a per-request `await` here would tax the hot path.
   *
   * Never throws — a container that cannot arm its idle timer must still
   * serve. But it must not fail SILENTLY: an unarmed timer is exactly how this
   * leak survived a whole fix cycle, so a failure is logged loudly.
   */
  private async armInactivityTimeout(container: Container, requestId: string): Promise<void> {
    if (typeof container.setInactivityTimeout !== "function") {
      // Older workerd, or a test double predating the API: the alarm reaper in
      // alarm() remains the only reaper. Not an error, but not silent either.
      console.warn(
        `[${requestId}] container.setInactivityTimeout unavailable — ` +
          `platform idle auto-destroy NOT armed; relying on the alarm reaper alone`,
      );
      return;
    }
    try {
      await container.setInactivityTimeout(IDLE_TIMEOUT_MS);
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      console.error(
        `[${requestId}] container.setInactivityTimeout(${IDLE_TIMEOUT_MS}) threw: ${msg} — ` +
          `platform idle auto-destroy NOT armed; relying on the alarm reaper alone`,
      );
    }
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
   *
   * Additionally carries the D1-placement INSTRUMENT (`d1_probe`) — see
   * {@link probeD1Latency}. Existing fields are untouched (something may parse
   * this); the instrument is purely additive.
   *
   * @param includeColo when true (`/_do/health?colo=1`) also resolve the DO's
   *   serving colo via a best-effort `cdn-cgi/trace` fetch. OFF by default so
   *   the ordinary probe makes no external request.
   */
  private async handleHealthProbe(requestId: string, includeColo = false): Promise<Response> {
    const container = this.state.container;
    const containerRunning = container !== undefined && container.running;
    const status = this.lifecycleState.containerStatus;

    // The instrument. Never throws (every failure is reported as a field).
    const d1Probe = await this.probeD1Latency(includeColo);

    const body = JSON.stringify({
      status: containerRunning ? "ok" : status,
      container_status: status,
      container_running: containerRunning,
      cold_start_count: this.lifecycleState.coldStartCount,
      last_health_check_ms: this.lifecycleState.lastHealthCheckMs,
      request_id: requestId,
      d1_probe: d1Probe,
    });
    return new Response(body, {
      status: containerRunning ? 200 : 503,
      headers: { "Content-Type": "application/json", "X-Request-Id": requestId },
    });
  }

  /**
   * D1 PLACEMENT INSTRUMENT — measures, from INSIDE this DO, how far the DO is
   * from the D1 primary (ENAM) and from the nearest read replica.
   *
   * ## Why this exists
   *
   * The container reads D1 over the public REST API
   * (`crates/corelink-container/src/storage/d1_http.rs:92`), which ALWAYS hits
   * the ENAM primary — measured 79/86/99 ms in prod. A proposal to route those
   * reads through this DO (which holds the `CONFIG_DB` binding) is worth
   * somewhere between −70 ms and +65 ms, and the sign hinges on ONE unmeasured
   * fact: where this DO sits relative to the ENAM primary. A DO-issued primary
   * read of ≤25 ms means co-location (the proposal wins ~60-75 ms); ≥100 ms
   * means the DO is far (the proposal is a regression — the Worker's own
   * primary read from SAM measures 156 ms). This function IS that measurement.
   *
   * ## What it does NOT do
   *
   * It runs ONLY on `/_do/health`. Nothing on the request-serving path calls it,
   * and it neither starts nor touches the container.
   *
   * ## How to read the numbers
   *
   *   - `warmup_ms` is an UNCOUNTED first read whose sole job is to absorb
   *     connection setup so it is not charged to `primary`. It is reported, not
   *     hidden — it is also the honest "cold" number.
   *   - `primary.min_ms` is the decision number for the primary path;
   *     `samples_ms` is every sample in order so a warm/cold spread is visible.
   *   - `served_by_region` / `served_by_primary` / `served_by_colo` come from
   *     D1's own result `meta`. They are what distinguishes "the replica read
   *     landed on a real replica" from "the Sessions API silently served the
   *     primary" — a replica time equal to the primary time is meaningless
   *     without them.
   *   - A read that THROWS reports `ok: false` + a non-null `error`. It is NEVER
   *     reported as a fast number and never as a missing field.
   *   - `available: false` means the path could not be attempted at all
   *     (binding unbound / no Sessions API), which is DISTINCT from "attempted
   *     and failed" (`available: true, ok: false`).
   */
  private async probeD1Latency(includeColo: boolean): Promise<D1ProbeReport> {
    // Structural read of the binding: `Env.CONFIG_DB` is declared non-optional,
    // but a test double (or a stripped env) may simply not have it. Mirrors the
    // optional-binding pattern in index.ts (`env as unknown as { METADATA_KV?… }`).
    const binding = (this.env as unknown as { CONFIG_DB?: D1ProbeBinding }).CONFIG_DB;

    const doColo = includeColo ? await resolveDoColo() : { colo: null, error: null };

    if (binding === undefined || binding === null) {
      const unavailable = unavailablePath("CONFIG_DB binding is not bound");
      return {
        probe_version: D1_PROBE_VERSION,
        samples: D1_PROBE_SAMPLES,
        binding_bound: false,
        sessions_api_available: false,
        warmup_ms: null,
        warmup_error: "CONFIG_DB binding is not bound",
        primary: unavailable,
        replica: unavailable,
        do_colo: doColo.colo,
        do_colo_error: doColo.error,
      };
    }

    // Uncounted warm-up on the primary handle: the FIRST D1 call in a fresh
    // isolate pays connection setup, and charging that to `primary` would fake a
    // "the DO is far from ENAM" verdict. Reported separately, never dropped.
    const warmup = await timedD1Read(binding);

    const primary = await probeD1Path(binding);

    // Feature-detect the Sessions API EXACTLY as index.ts:1259-1260 does. A
    // runtime (or a test double) without it must degrade to primary-only, never
    // throw — the health probe outranks the instrument.
    const sessionsAvailable = typeof binding.withSession === "function";
    let replica: D1PathProbeResult;
    if (!sessionsAvailable) {
      replica = unavailablePath("Sessions API (withSession) not available on this binding");
    } else {
      try {
        // `first-unconstrained` = no bookmark constraint → nearest replica.
        const session = binding.withSession?.("first-unconstrained");
        replica =
          session === undefined || session === null
            ? unavailablePath("withSession returned no session handle")
            : await probeD1Path(session);
      } catch (err: unknown) {
        replica = { ...unavailablePath("withSession threw"), error: errText(err) };
      }
    }

    return {
      probe_version: D1_PROBE_VERSION,
      samples: D1_PROBE_SAMPLES,
      binding_bound: true,
      sessions_api_available: sessionsAvailable,
      warmup_ms: warmup.ms,
      warmup_error: warmup.error,
      primary,
      replica,
      do_colo: doColo.colo,
      do_colo_error: doColo.error,
    };
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
    // The alarm is the SOLE owner of both the health chain and the reaper, so
    // losing a link is losing the reaper — the immortal container re-entered
    // through a third door. Any throw below (a storage.put fault, the
    // PagerDuty POST, hashForLog) would otherwise end the chain permanently
    // once CF exhausts its bounded retries. Mirror the always-re-arm posture
    // of ReplicationCoordinatorDO.alarm(): re-arm in `finally` UNLESS this
    // tick deliberately ended the chain (`chainEnded`).
    let chainEnded = false;
    try {
      chainEnded = await this.alarmTick();
    } finally {
      if (!chainEnded) {
        await this.storage.setAlarm(Date.now() + HEALTH_CHECK_INTERVAL_MS);
      }
    }
  }

  /**
   * One alarm tick. Returns `true` when the chain was deliberately ENDED (the
   * container is dead or was just reaped) — the caller then does NOT re-arm,
   * which is what lets the DO hibernate instead of heartbeating a dead
   * container forever. Any other return (or a throw) leaves the chain armed.
   */
  private async alarmTick(): Promise<boolean> {
    const container = this.state.container;
    const status = this.lifecycleState.containerStatus;
    if (
      container === undefined ||
      !container.running ||
      (status !== "running" && status !== "degraded")
    ) {
      // Chain deliberately ENDS here (no reschedule): a dead container must
      // not keep the DO alive on a 30s alarm heartbeat — that is the other
      // half of the immortal-container bill. The next real request restarts
      // both the container and the alarm chain (ensureContainerRunning).
      // NB: "degraded" with a still-running container stays IN the chain —
      // it must remain subject to the idle reaper below, or a degraded
      // container becomes the one immortality path left.
      return true;
    }

    const requestId = "alarm-health-" + crypto.randomUUID().slice(0, 8);
    const now = Date.now();

    // ── Durable idle reaper ─────────────────────────────────────────────────
    // The reaper MUST live here, on the alarm (durable, storage-backed), not
    // in a setTimeout: the old in-memory idle timer evaporated on every
    // isolate eviction (deploy/recycle) while this alarm chain survived and
    // kept health-checking the container — so any once-started container
    // became IMMORTAL and was billed 24/7 (2026-07 invoice: ~13 GiB resident
    // memory around the clock, $84/mo, with $0.00 of real traffic).
    const lastActivity = this.lifecycleState.lastActivityMs;
    if (lastActivity === undefined) {
      // Pre-fix persisted state: start the idle clock now; reaps one idle
      // window later. Persisted immediately so an eviction can't reset it.
      await this.updateLifecycleState({ ...this.lifecycleState, lastActivityMs: now });
    } else if (now - lastActivity >= IDLE_TIMEOUT_MS) {
      const tenantHash = await hashForLog(this.lifecycleState.tenantId ?? "_unknown");
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
      // RE-CHECK before destroying. Every `await` above is a yield point where
      // a queued fetch() runs (the same concurrency rule the REV-S2
      // concurrent-start guard is built on), and emitLifecycleEvent is a real
      // outbound POST — seconds-scale. A request that arrived in that window
      // has already passed ensureContainerRunning and may be mid-proxy;
      // destroying now would kill it in flight. Abort and let the chain re-arm.
      const activityNow = this.lifecycleState.lastActivityMs ?? 0;
      if (Date.now() - activityNow < IDLE_TIMEOUT_MS) {
        return false; // raced with a live request — keep the container
      }
      await this.destroyContainer(requestId);
      return true; // chain ends — container dead, DO free to hibernate
    }

    if (status !== "running") {
      // Degraded-but-running: no health probe (preserved semantics), but the
      // chain stays alive so the reaper above still fires on idle expiry.
      // Persist the idle clock too — the degraded arm never wrote lifecycle,
      // so an eviction here would revert lastActivityMs to a stale value.
      await this.updateLifecycleState({ ...this.lifecycleState });
      return false;
    }

    // Re-arm the platform idle reaper. Reached only when the container is
    // running AND the reaper above did NOT find it idle, so this slides the
    // window for a container that is actually being used. Placed before the
    // health-probe dedupe below so a deduped double-fire still re-arms.
    // See armInactivityTimeout: the timer's reset semantics are unspecified,
    // and re-arming is what makes this correct under the absolute reading.
    await this.armInactivityTimeout(container, requestId);

    if (now - this.lifecycleState.lastHealthCheckMs < HEALTH_CHECK_INTERVAL_MS) {
      // Deduped double-fire: skip the probe but NEVER break the alarm chain —
      // a silent early-return here would orphan a running container with no
      // health checks AND no reaper. (Persist for the same reason as above.)
      await this.updateLifecycleState({ ...this.lifecycleState });
      return false;
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

    // Chain continues — alarm() re-arms in its `finally`.
    return false;
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
