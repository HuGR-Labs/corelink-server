/**
 * DSR 24h verification sweep (WI-S11-008 increment 5).
 *
 * Cloudflare Cron Trigger handler: for every DSR whose erasure landed past its
 * 24h SLA deadline (within a bounded look-back window), POST the container's
 * verify endpoint (`POST /_internal/dsr/verify`). That re-fingerprints all 12
 * backends and emits the canonical `verification_passed/failed` + `completed`
 * audit arms. `verify_erasure` is idempotent, so re-sweeping a DSR is harmless
 * — the bounded window simply keeps the sweep cheap.
 *
 * Reads TWO candidate sources (deduped by `dsr_id`):
 *
 *   1. `dsr_requested` (G4) — the durable "requested at enqueue" anchor. This is
 *      the load-bearing source: a DSR that FAILS before any backend tombstone
 *      (audit-fail-closed) has NO `dsr_erasure_log` row, so without this anchor
 *      its SLA breach would go UNDETECTED. We enumerate `status='requested'` rows
 *      past the 24h deadline and verify them; a `VerifiedComplete` (ok response)
 *      flips the row to `status='verified'` so it stops being re-enumerated.
 *   2. `dsr_erasure_log` — DSRs that produced at least one tombstone (the legacy
 *      source; kept so a DSR with a log row but no requested row, e.g. enqueued
 *      before this migration, is still swept).
 *
 * Needs only the ids + the SLA-clock anchor (the per-DSR salt / raw subject are
 * NOT retained and the sweep does not use them). Inert until
 * `CORELINK_INTERNAL_AUTH_KEY` is bound (task #46).
 */

import { resolveEraseAuthKey } from "../lib/erase-auth-key.js";

/** Light verify wire shape — mirrors the container's `DsrVerifyV1`. */
export interface DsrVerifyV1 {
  dsr_id: string;
  tenant_id: string;
  queued_at_ms: number;
}

/** Minimal D1 surface (test-independent of workers-types; structurally
 * satisfied by `D1Database`). Non-generic so the row read stays cast-free. */
export interface D1Lite {
  prepare(sql: string): {
    bind(...vals: unknown[]): {
      all(): Promise<{ results?: Array<Record<string, unknown>> }>;
      run(): Promise<unknown>;
    };
  };
}

/** Minimal env surface the sweep needs. */
export interface DsrVerifyCronEnv {
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
  /** `corelink-prod-d1` binding (holds `dsr_erasure_log`). */
  CONFIG_DB: D1Lite;
}

/** 24h verification SLA deadline. */
const DEADLINE_MS = 24 * 60 * 60 * 1000;
/** Look-back window — bound the sweep so it stays cheap + idempotent. */
const WINDOW_MS = 7 * 24 * 60 * 60 * 1000;

/**
 * Format epoch-ms as the ledger's `"YYYY-MM-DDTHH:MM:SSZ"` second-precision ISO
 * (matches `dsr_erasure_log.started_at`, so the SQL string comparison is exact).
 */
export function isoFromMs(ms: number): string {
  return new Date(ms).toISOString().replace(/\.\d{3}Z$/, "Z");
}

/**
 * Parse an ISO-8601 instant to epoch-ms, or `null` on NULL/empty/unparseable
 * input.
 *
 * MUST NOT return 0 (epoch-1970) on bad input: a NULL/corrupt `started_at`
 * would otherwise anchor the DSR SLA clock at 1970 → the row is permanently
 * past the 24h deadline → a recurring FALSE `sla_breached` PagerDuty page on
 * every 24h cron tick. Returning `null` lets the call site SKIP the row instead
 * of anchoring it at the epoch.
 */
export function msFromIso(iso: string): number | null {
  if (!iso) return null;
  const t = Date.parse(iso);
  return Number.isNaN(t) ? null : t;
}

/**
 * POST one verify request to the container. NEVER throws (transport errors are
 * reported as `ok:false`). Returns `{ ok, status }`.
 */
export async function postVerify(
  env: DsrVerifyCronEnv,
  body: DsrVerifyV1,
): Promise<{ ok: boolean; status: number; decision: string }> {
  // rt-nuclear #23: erase-key-first, shared-fallback (mirrors the container +
  // main-Worker erase gate) so the verify call authenticates in the full-split
  // config rather than 401'ing the moment a dedicated erase key is provisioned.
  const eraseAuthKey = resolveEraseAuthKey(env);
  if (!eraseAuthKey) {
    return { ok: false, status: 0, decision: "" };
  }
  const req = new Request(`${env.CORELINK_API_BASE}/_internal/dsr/verify`, {
    method: "POST",
    headers: {
      "content-type": "application/json",
      "x-corelink-internal-auth": eraseAuthKey,
    },
    body: JSON.stringify(body),
  });
  try {
    const resp = env.CORELINK_API_SVC
      ? await env.CORELINK_API_SVC.fetch(req)
      : await fetch(req);
    let decision = "";
    if (resp.ok) {
      try {
        const j = (await resp.json()) as { decision?: string };
        decision = String(j.decision ?? "");
      } catch {
        // non-JSON 200 — treat as a successful transport with unknown decision.
      }
    }
    return { ok: resp.ok, status: resp.status, decision };
  } catch (err) {
    console.error(
      `[dsr-verify-cron] verify call threw dsr_id=${body.dsr_id}: ${(err as Error).message.slice(0, 120)}`,
    );
    return { ok: false, status: 0, decision: "" };
  }
}

/**
 * Run the verification sweep. Queries `dsr_erasure_log` for DSRs whose erasure
 * landed between `WINDOW_MS` and `DEADLINE_MS` ago (one verify per `dsr_id`),
 * and POSTs the container verify endpoint for each. Returns the counts.
 *
 * `skipped: true` (and zero counts) when `CORELINK_INTERNAL_AUTH_KEY` is unbound
 * — the cron is inert until provisioning (task #46).
 */
export async function runDsrVerifySweep(
  env: DsrVerifyCronEnv,
  nowMs: number,
): Promise<{ swept: number; failed: number; skipped: boolean }> {
  // Inert only when NEITHER the dedicated erase key nor the shared key is bound
  // (rt-nuclear #23: same erase-first, shared-fallback resolution as postVerify).
  if (!resolveEraseAuthKey(env)) {
    return { swept: 0, failed: 0, skipped: true };
  }
  const deadlineMs = nowMs - DEADLINE_MS;
  const windowMs = nowMs - WINDOW_MS;
  const olderThan = isoFromMs(deadlineMs);
  const newerThan = isoFromMs(windowMs);

  // Source 1 (G4, load-bearing): the durable "requested" anchor. Catches DSRs
  // that failed before ANY tombstone (no dsr_erasure_log row). requested_at is
  // epoch-ms (INTEGER), so compare in ms directly.
  //
  // NO lower (WINDOW_MS) bound here, deliberately: a DSR stuck at
  // status='requested' is EXACTLY the breach this anchor exists to surface, and
  // it self-expires from a 7-day window after one week — silencing the alert
  // precisely for the permanently-stuck case. The set is self-limiting (a
  // completed DSR flips to 'verified' and drops out), so enumerating every
  // past-deadline 'requested' row is bounded and cheap. The WINDOW_MS lower
  // bound stays on the dsr_erasure_log source below (cost cap on the large
  // tombstone table, where a row's absence is not itself a breach signal).
  let requested: Array<Record<string, unknown>> = [];
  try {
    const reqRes = await env.CONFIG_DB.prepare(
      `SELECT dsr_id, tenant_id, requested_at
         FROM dsr_requested
        WHERE status = 'requested' AND requested_at <= ?1`,
    )
      .bind(deadlineMs)
      .all();
    requested = reqRes.results ?? [];
  } catch (err) {
    // dsr_requested may not exist on an env that hasn't applied 0069 yet —
    // degrade to the dsr_erasure_log source rather than failing the sweep.
    console.error(
      `[dsr-verify-cron] dsr_requested query failed (degrading to log source): ${(err as Error).message.slice(0, 120)}`,
    );
  }

  // Source 2 (legacy): DSRs that produced at least one tombstone.
  const logRes = await env.CONFIG_DB.prepare(
    `SELECT dsr_id, tenant_id, MIN(started_at) AS queued_at
       FROM dsr_erasure_log
      WHERE started_at <= ?1 AND started_at >= ?2
      GROUP BY dsr_id, tenant_id`,
  )
    .bind(olderThan, newerThan)
    .all();

  // Merge + dedupe by dsr_id (requested anchor wins — its queued_at is the true
  // SLA-clock anchor; the log MIN(started_at) is a backend-write proxy).
  interface Candidate {
    dsr_id: string;
    tenant_id: string;
    queued_at_ms: number;
    fromRequested: boolean;
  }
  const byId = new Map<string, Candidate>();
  for (const r of requested) {
    const dsr_id = String(r.dsr_id ?? "");
    const tenant_id = String(r.tenant_id ?? "");
    if (!dsr_id || !tenant_id) continue;
    byId.set(dsr_id, {
      dsr_id,
      tenant_id,
      queued_at_ms: Number(r.requested_at ?? 0),
      fromRequested: true,
    });
  }
  for (const r of logRes.results ?? []) {
    const dsr_id = String(r.dsr_id ?? "");
    const tenant_id = String(r.tenant_id ?? "");
    if (!dsr_id || !tenant_id || byId.has(dsr_id)) continue;
    // SLA-clock anchor: a NULL/malformed started_at must NEVER anchor at
    // epoch-1970 — that would make the row permanently past the 24h deadline
    // and fire a recurring FALSE sla_breached page every cron tick. Skip the
    // row (with a structured warn) when the timestamp is unparseable.
    const queued_at_ms = msFromIso(String(r.queued_at ?? ""));
    if (queued_at_ms === null) {
      console.warn(
        `[dsr-verify-cron] skipping row with invalid started_at dsr_id=${dsr_id} tenant_id=${tenant_id} raw_started_at=${JSON.stringify(r.queued_at ?? null)}`,
      );
      continue;
    }
    byId.set(dsr_id, {
      dsr_id,
      tenant_id,
      queued_at_ms,
      fromRequested: false,
    });
  }

  let swept = 0;
  let failed = 0;
  for (const c of byId.values()) {
    const out = await postVerify(env, {
      dsr_id: c.dsr_id,
      tenant_id: c.tenant_id,
      queued_at_ms: c.queued_at_ms,
    });
    if (out.ok) {
      swept += 1;
      // Flip the requested anchor to 'verified' ONLY on verified_complete — a
      // verified_partial / sla_breached still returns HTTP 200 but the erasure
      // is NOT done, so it must stay enumerable for the next sweep. Best-effort
      // (re-sweep is idempotent if the flip is dropped).
      if (c.fromRequested && out.decision === "verified_complete") {
        try {
          await env.CONFIG_DB.prepare(
            "UPDATE dsr_requested SET status = 'verified' WHERE dsr_id = ?1",
          )
            .bind(c.dsr_id)
            .run();
        } catch (err) {
          console.error(
            `[dsr-verify-cron] status flip failed dsr_id=${c.dsr_id}: ${(err as Error).message.slice(0, 120)}`,
          );
        }
      }
    } else {
      failed += 1;
    }
  }
  return { swept, failed, skipped: false };
}
