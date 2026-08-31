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
 * The call carries NO body that the handler requires — it sweeps all pending
 * partitions — but we send `{}` so the request is a well-formed JSON POST.
 *
 * Auth: the `/_internal/audit/*` surface reuses the erase/DSR consumer key
 * (erase-first, shared-fallback — see `resolveEraseAuthKey`); no new secret.
 * Inert (no-op-with-log) until that key is bound (task #46) — exactly like the
 * DSR verify cron, so a cron run on an unprovisioned env is a clean skip rather
 * than a 401 storm.
 */

import { resolveEraseAuthKey } from "../lib/erase-auth-key.js";

/**
 * Hard cap on drain calls per cron tick. At the prod default of 200 rows per
 * call this drains up to 2,000 rows/hour — an order of magnitude over the
 * measured 310/h mean arrival rate, with headroom over the 3,141/h peak burst
 * to catch a cold backlog up over a few ticks rather than never.
 */
export const MAX_DRAIN_CALLS = 10;

/**
 * Wall-clock budget for the whole sweep. One call is ~60s at the prod default
 * (200 rows x ~0.3s/row of D1-over-HTTP UPDATE), so this stops the loop before
 * it can crowd out the archive sweep that runs alongside it in the same
 * `scheduled()` tick.
 */
export const DRAIN_WALL_BUDGET_MS = 10 * 60 * 1000;

/** Minimal env surface the sweep needs. */
export interface AuditDrainCronEnv {
  /** Base URL of the CoreLink API (container host). */
  CORELINK_API_BASE: string;
  /**
   * Dedicated erase/DSR consumer secret (red-team #3 split). Preferred for the
   * `x-corelink-internal-auth` header so it matches the main-Worker / container
   * erase gate in the full-split config (rt-nuclear #23); shared-key fallback.
   */
  CORELINK_ERASE_AUTH_KEY?: string;
  /** Shared secret for the `x-corelink-internal-auth` header. */
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
   * True when the LAST successful call still reported `incomplete: true` — the
   * backlog outlived this tick's budget and the next hourly tick continues it.
   * A sweep that stopped on an error leaves this as the last value it saw.
   */
  incomplete: boolean;
  /** True when neither internal-auth key is bound (inert, not a failure to seal). */
  skipped: boolean;
}

/**
 * Run the audit-chain drain sweep. Re-POSTs the container drain endpoint while
 * it reports `incomplete: true`, bounded by `MAX_DRAIN_CALLS` and
 * `DRAIN_WALL_BUDGET_MS`, and returns the summed outcome. NEVER throws
 * (transport/parse errors are reported as `ok:false`) so a cron failure cannot
 * escape `scheduled()`.
 *
 * `skipped: true` (and zero counts) when NEITHER the dedicated erase key nor the
 * shared key is bound — the cron is inert until provisioning (task #46).
 */
export async function runAuditDrainSweep(
  env: AuditDrainCronEnv,
  _nowMs: number,
): Promise<AuditDrainSweepResult> {
  // Inert only when NEITHER the dedicated erase key nor the shared key is bound
  // (rt-nuclear #23: same erase-first, shared-fallback resolution as the DSR
  // verify cron).
  const eraseAuthKey = resolveEraseAuthKey(env);
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
        incomplete,
        skipped: false,
      };
    }

    status = resp.status;
    if (!resp.ok) {
      // A non-2xx is terminal for this tick: re-calling a failing endpoint nine
      // more times turns one bad hour into a request storm.
      ok = false;
      break;
    }

    // The keys are the handler's, verbatim: `rows_sealed`, `partitions_drained`,
    // `partitions_failed`, `incomplete`. Reading anything else is how this cron
    // logged `sealed=0` for its entire life.
    let body: {
      rows_sealed?: number;
      partitions_drained?: number;
      partitions_failed?: number;
      incomplete?: boolean;
    };
    try {
      body = (await resp.json()) as typeof body;
    } catch {
      // non-JSON 200 — a successful transport with unknown counts. We cannot
      // know whether work remains, so stop rather than loop blind.
      incomplete = false;
      break;
    }

    sealed += Number(body.rows_sealed ?? 0);
    partitions += Number(body.partitions_drained ?? 0);
    partitionsFailed += Number(body.partitions_failed ?? 0);
    incomplete = Boolean(body.incomplete ?? false);

    // Done: the handler says the backlog is drained.
    if (!incomplete) {
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
