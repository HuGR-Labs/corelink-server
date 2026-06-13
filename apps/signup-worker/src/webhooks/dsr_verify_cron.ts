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
 * Reads `dsr_erasure_log` (D1) for the candidate DSRs; needs only the ids + the
 * SLA-clock anchor (the per-DSR salt / raw subject are NOT retained and the
 * sweep does not use them). Inert until `CORELINK_INTERNAL_AUTH_KEY` is bound
 * (task #46).
 */

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
    };
  };
}

/** Minimal env surface the sweep needs. */
export interface DsrVerifyCronEnv {
  CORELINK_API_BASE: string;
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

/** Parse an ISO-8601 instant to epoch-ms (0 on malformed input). */
export function msFromIso(iso: string): number {
  const t = Date.parse(iso);
  return Number.isNaN(t) ? 0 : t;
}

/**
 * POST one verify request to the container. NEVER throws (transport errors are
 * reported as `ok:false`). Returns `{ ok, status }`.
 */
export async function postVerify(
  env: DsrVerifyCronEnv,
  body: DsrVerifyV1,
): Promise<{ ok: boolean; status: number }> {
  if (!env.CORELINK_INTERNAL_AUTH_KEY) {
    return { ok: false, status: 0 };
  }
  const req = new Request(`${env.CORELINK_API_BASE}/_internal/dsr/verify`, {
    method: "POST",
    headers: {
      "content-type": "application/json",
      "x-corelink-internal-auth": env.CORELINK_INTERNAL_AUTH_KEY,
    },
    body: JSON.stringify(body),
  });
  try {
    const resp = env.CORELINK_API_SVC
      ? await env.CORELINK_API_SVC.fetch(req)
      : await fetch(req);
    return { ok: resp.ok, status: resp.status };
  } catch (err) {
    console.error(
      `[dsr-verify-cron] verify call threw dsr_id=${body.dsr_id}: ${(err as Error).message.slice(0, 120)}`,
    );
    return { ok: false, status: 0 };
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
  if (!env.CORELINK_INTERNAL_AUTH_KEY) {
    return { swept: 0, failed: 0, skipped: true };
  }
  const olderThan = isoFromMs(nowMs - DEADLINE_MS);
  const newerThan = isoFromMs(nowMs - WINDOW_MS);
  const res = await env.CONFIG_DB.prepare(
    `SELECT dsr_id, tenant_id, MIN(started_at) AS queued_at
       FROM dsr_erasure_log
      WHERE started_at <= ?1 AND started_at >= ?2
      GROUP BY dsr_id, tenant_id`,
  )
    .bind(olderThan, newerThan)
    .all();

  let swept = 0;
  let failed = 0;
  for (const r of res.results ?? []) {
    const dsr_id = String(r.dsr_id ?? "");
    const tenant_id = String(r.tenant_id ?? "");
    const queued_at = String(r.queued_at ?? "");
    if (!dsr_id || !tenant_id) {
      continue;
    }
    const out = await postVerify(env, {
      dsr_id,
      tenant_id,
      queued_at_ms: msFromIso(queued_at),
    });
    if (out.ok) {
      swept += 1;
    } else {
      failed += 1;
    }
  }
  return { swept, failed, skipped: false };
}
