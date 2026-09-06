/**
 * S-09 audit-chain drain sweep (compliance artifact 2, WP-C2).
 *
 * Cloudflare Cron Trigger handler: once per hour, POST the container's audit
 * drain endpoint (`POST /_internal/audit/drain`). The Rust handler seals every
 * pending `audit_outbox` row into the tamper-evident hash chain — per
 * `(tenant_id, region)` partition it resumes the chain head, computes the
 * canonical JCS + chain hash for each unsealed row, and advances
 * `audit_chain_head`. The endpoint is IDEMPOTENT (already-sealed rows are
 * skipped), so an hourly sweep is safe and a missed hour simply seals a bigger
 * batch on the next run.
 *
 * B-064 — the two halves this file got wrong, and why they mattered:
 *
 * 1. WRONG KEYS. The handler answers
 *    `{ ok, partitions_drained, rows_sealed, partitions_drifted,
 *    partitions_failed, partitions_leased, heads_resigned, incomplete }`
 *    (`crates/corelink-container/src/routes/audit_drain.rs`, summary built at
 *    ~:1535). This caller read the response's `sealed` / `partitions` fields —
 *    names the handler has never emitted — so those keys were always absent, and
 *    `Number(undefined ?? 0)` made the log line read
 *    `sealed=0 partitions=0` on every run since the cron was wired, no matter
 *    how much work the drain actually did. A sweep that sealed 200 rows and a
 *    sweep that sealed none were byte-identical in the log.
 *
 * 2. NO LOOP. The handler is bounded per CALL by `AUDIT_DRAIN_BATCH_LIMIT`
 *    (`.unwrap_or(200)`, UNSET in prod ⇒ 200 rows across ALL partitions), and
 *    it says so by returning `incomplete: true`. Its contract is explicit that
 *    "a caller (the hourly cron, or a manual loop) re-calls until `incomplete`
 *    is false". This caller POSTed exactly once per hour, so the per-CALL
 *    budget became a per-HOUR ceiling of 200 rows for the whole platform.
 *    Measured against prod D1 on 2026-08-31: ten consecutive hours sealed
 *    exactly 200, 200, 200, 200, 200, 200, 200, 200, 200, 200 rows while
 *    arrivals averaged 310/h and peaked at 3,141/h — the unsealed backlog grew.
 *
 * The loop below is bounded twice over so it can never become the runaway the
 * single call was protecting against: at most `MAX_DRAIN_CALLS` iterations, and
 * it stops early once `DRAIN_WALL_BUDGET_MS` of wall clock has been spent. Both
 * bounds leave the remaining backlog for the next hourly tick — the endpoint is
 * idempotent, so stopping early is always safe.
 *
 * B-125 keeps this caller adaptive under contention: an `incomplete` response
 * that only reports lease skips or head drift is backpressure, not durable
 * progress, so it stops for this tick instead of hammering D1 while another
 * holder owns the partition. The container's explicit production batch budget
 * is 512 rows/call; its 32-row JSON1 write chunks preserve the same signed
 * chain values while avoiding one D1 HTTP round trip per row.
 *
 * The call carries NO body that the handler requires — it sweeps all pending
 * partitions — but we send `{}` so the request is a well-formed JSON POST.
 *
 * Auth: the `/_internal/audit/drain` route is an irreversible control and
 * accepts only the dedicated `CORELINK_ERASE_AUTH_KEY` (at least 32 non-blank
 * characters). `CORELINK_INTERNAL_AUTH_KEY` is never a fallback here. A
 * shared-only or malformed configuration is an explicit skipped/fail-closed
 * result, never a request that can exercise the route with a broad key.
 */

import { resolveDedicatedEraseAuthKey } from "../lib/erase-auth-key.js";

/**
 * Hard cap on drain calls per cron tick. Production declares a 512-row
 * container budget, so this permits at most 5,120 rows/tick — over the
 * measured 310/h mean arrival rate and with headroom over the 3,141/h peak
 * burst to catch a cold backlog up over a few ticks rather than never.
 */
export const MAX_DRAIN_CALLS = 10;

/**
 * Wall-clock budget for the whole sweep. The bound leaves room for the
 * production 512-row call budget and its bounded 32-row container write
 * chunks, while stopping the loop before it can crowd out the archive sweep
 * that runs alongside it in the same `scheduled()` tick.
 */
export const DRAIN_WALL_BUDGET_MS = 10 * 60 * 1000;

/** Minimal env surface the sweep needs. */
export interface AuditDrainCronEnv {
  /** Base URL of the CoreLink API (container host). */
  CORELINK_API_BASE: string;
  /**
   * Dedicated erase key required by the audit-drain route. The key must be at
   * least 32 non-blank characters; shorter values fail closed before a request.
   */
  CORELINK_ERASE_AUTH_KEY?: string;
  /** Present for other Worker duties; audit drain deliberately ignores it. */
  CORELINK_INTERNAL_AUTH_KEY?: string;
  /** Service binding to the main CoreLink Worker (bypasses CF edge error 1014). */
  CORELINK_API_SVC?: { fetch: typeof fetch };
}

/** Outcome of one cron tick's drain sweep. */
export interface AuditDrainSweepResult {
  /** False if ANY call in the loop failed (transport or non-2xx). */
  ok: boolean;
  /** Status of the LAST call attempted — 0 when the transport threw. */
  status: number;
  /** Rows sealed across every call this tick (`rows_sealed`, summed). */
  sealed: number;
  /** Partitions drained across every call this tick (`partitions_drained`, summed). */
  partitions: number;
  /** Partitions the handler reported as failed, summed. */
  partitionsFailed: number;
  /** How many times the endpoint was called this tick. */
  calls: number;
  /**
   * True when the backlog was not verified complete: the handler returned
   * `incomplete`, a bounded tick ended early, or a response/transport failure
   * made completion unknowable. This is intentionally true on every failure so
   * the scheduled seam cannot make an uncertain sweep look ordinary.
   */
  incomplete: boolean;
  /** True when the required dedicated key is not safely bound (no request sent). */
  skipped: boolean;
}

interface AuditDrainResponse {
  ok: boolean;
  partitions_drained: number;
  rows_sealed: number;
  partitions_drifted: number;
  partitions_failed: number;
  partitions_leased: number;
  heads_resigned: number;
  incomplete: boolean;
}

const DRAIN_COUNTER_FIELDS = [
  "partitions_drained",
  "rows_sealed",
  "partitions_drifted",
  "partitions_failed",
  "partitions_leased",
  "heads_resigned",
] as const;

/** Decode the Rust handler's complete JSON contract; unknown is never success. */
function parseAuditDrainResponse(value: unknown):
  | { value: AuditDrainResponse }
  | { error: string } {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return { error: "body is not an object" };
  }

  const body = value as Record<string, unknown>;
  if (typeof body.ok !== "boolean") {
    return { error: "missing or non-boolean ok" };
  }
  if (typeof body.incomplete !== "boolean") {
    return { error: "missing or non-boolean incomplete" };
  }
  for (const field of DRAIN_COUNTER_FIELDS) {
    const counter = body[field];
    if (
      typeof counter !== "number" ||
      !Number.isSafeInteger(counter) ||
      counter < 0
    ) {
      return { error: `missing or invalid ${field}` };
    }
  }

  return { value: body as unknown as AuditDrainResponse };
}

/**
 * A true incomplete response without durable progress cannot converge. Lease
 * skips, drift, and an empty partition are observations/backpressure rather
 * than progress; retrying those immediately would create a D1 request storm
 * while the competing holder is still working. Only rows sealed or heads
 * re-signed justify another call in this tick.
 */
function madeDrainProgress(body: AuditDrainResponse): boolean {
  return body.rows_sealed > 0 || body.heads_resigned > 0;
}

/**
 * Run the audit-chain drain sweep. Re-POSTs the container drain endpoint while
 * it reports `incomplete: true`, bounded by `MAX_DRAIN_CALLS` and
 * `DRAIN_WALL_BUDGET_MS`, and returns the summed outcome. NEVER throws
 * (transport/parse errors are reported as `ok:false`) so a cron failure cannot
 * escape `scheduled()`.
 *
 * `skipped: true` (and zero counts) when the dedicated erase key is absent,
 * blank, or short. The shared key is not a substitute for this irreversible
 * control.
 */
export async function runAuditDrainSweep(
  env: AuditDrainCronEnv,
  _nowMs: number,
): Promise<AuditDrainSweepResult> {
  const eraseAuthKey = resolveDedicatedEraseAuthKey(env);
  if (!eraseAuthKey) {
    return {
      ok: false,
      status: 0,
      sealed: 0,
      partitions: 0,
      partitionsFailed: 0,
      calls: 0,
      incomplete: false,
      skipped: true,
    };
  }

  // The deadline MUST be derived from the same clock the loop reads. `nowMs` is
  // the tick's logical timestamp and callers (and tests) legitimately pass 0 for
  // it; deriving the deadline from it and then comparing against `Date.now()`
  // would compare two different epochs and abort the loop after one call.
  const deadlineAt = Date.now() + DRAIN_WALL_BUDGET_MS;

  let sealed = 0;
  let partitions = 0;
  let partitionsFailed = 0;
  let calls = 0;
  let status = 0;
  let ok = true;
  let incomplete = false;

  for (let i = 0; i < MAX_DRAIN_CALLS; i++) {
    const req = new Request(`${env.CORELINK_API_BASE}/_internal/audit/drain`, {
      method: "POST",
      headers: {
        "content-type": "application/json",
        "x-corelink-internal-auth": eraseAuthKey,
      },
      body: "{}",
    });

    calls++;
    let resp: Response;
    try {
      resp = env.CORELINK_API_SVC
        ? await env.CORELINK_API_SVC.fetch(req)
        : await fetch(req);
    } catch (err) {
      console.error(
        `[audit-drain-cron] drain call ${calls} threw: ${(err as Error).message.slice(0, 120)}`,
      );
      return {
        ok: false,
        status: 0,
        sealed,
        partitions,
        partitionsFailed,
        calls,
        incomplete: true,
        skipped: false,
      };
    }

    status = resp.status;
    if (!resp.ok) {
      // A non-2xx is terminal for this tick: re-calling a failing endpoint nine
      // more times turns one bad hour into a request storm.
      ok = false;
      incomplete = true;
      break;
    }

    // This boundary deliberately accepts only the handler's complete, typed
    // contract. A 200 with malformed/missing counters is not proof of completion
    // and must never trigger another blind retry.
    let rawBody: unknown;
    try {
      rawBody = await resp.json();
    } catch {
      console.error(
        `[audit-drain-cron] drain call ${calls} returned malformed 2xx JSON; completion is unknown`,
      );
      ok = false;
      incomplete = true;
      break;
    }

    const parsed = parseAuditDrainResponse(rawBody);
    if ("error" in parsed) {
      console.error(
        `[audit-drain-cron] drain call ${calls} returned invalid 2xx contract: ${parsed.error}`,
      );
      ok = false;
      incomplete = true;
      break;
    }
    const body = parsed.value;

    sealed += body.rows_sealed;
    partitions += body.partitions_drained;
    partitionsFailed += body.partitions_failed;
    incomplete = body.incomplete;

    // The server explicitly sets ok=false when a partition failed. Treat either
    // half of that invariant as terminal: retrying an acknowledged failure in a
    // tight loop hides it and can turn one failed partition into a request storm.
    if (!body.ok || body.partitions_failed > 0) {
      console.error(
        `[audit-drain-cron] drain call ${calls} was non-complete: ok=${body.ok} partitions_failed=${body.partitions_failed}`,
      );
      ok = false;
      incomplete = true;
      break;
    }

    // Done: the handler says the backlog is drained.
    if (!incomplete) {
      break;
    }
    if (!madeDrainProgress(body)) {
      console.error(
        `[audit-drain-cron] drain call ${calls} reported incomplete with no counter progress; refusing blind retry`,
      );
      ok = false;
      incomplete = true;
      break;
    }
    // Budget spent: leave the rest for the next hourly tick. The endpoint is
    // idempotent, so an early stop costs latency, never correctness.
    if (Date.now() >= deadlineAt) {
      break;
    }
  }

  return {
    ok,
    status,
    sealed,
    partitions,
    partitionsFailed,
    calls,
    incomplete,
    skipped: false,
  };
}
