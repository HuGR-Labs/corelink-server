/**
 * S-09 audit-chain OFFSITE ARCHIVE sweep.
 *
 * Cloudflare Cron Trigger handler: POST the container's archive endpoint
 * (`POST /_internal/audit/archive`). The Rust handler copies rows the drain has
 * already sealed into immutable NDJSON chunks in the R2 audit bucket, which
 * carries a 7-year Object Lock. Until this ran, the sealed chain existed only
 * in D1 — a mutable store — so the S-09 "immutable offsite copy" control was
 * described in the specs and verified by a daily cron, but never produced.
 *
 * Why this is a SEPARATE sweep from `audit_drain_cron`: the drain's job is to
 * make the trail tamper-evident and it must not gain a new way to fail. If the
 * archive write lived inside the drain, an R2 outage would either abort the
 * seal (leaving rows plain and mutable — strictly worse than no archive) or be
 * swallowed. Split, R2 can be down for a week and the chain still seals; the
 * archive simply catches up on the next tick.
 *
 * The endpoint is IDEMPOTENT (a chunk already present with identical bytes is a
 * success, and only rows whose chunk is durable in R2 get marked archived), so
 * an hourly sweep is safe and a missed hour just archives a bigger batch next
 * time. A call that could not finish the backlog returns `incomplete: true`;
 * the next tick continues. That is also how the pre-existing sealed backlog is
 * archived — there is no separate backfill script to run and forget.
 *
 * Auth: the `/_internal/audit/*` surface reuses the erase/DSR consumer key
 * (erase-first, shared-fallback — see `resolveEraseAuthKey`); no new secret.
 * Inert (no-op-with-log) until that key is bound, exactly like the drain sweep,
 * so a cron run on an unprovisioned env is a clean skip rather than a 401 storm.
 */

import { resolveEraseAuthKey } from "../lib/erase-auth-key.js";

/** Minimal env surface the sweep needs. */
export interface AuditArchiveCronEnv {
  /** Base URL of the CoreLink API (container host). */
  CORELINK_API_BASE: string;
  /** Dedicated erase/DSR consumer secret; preferred over the shared key. */
  CORELINK_ERASE_AUTH_KEY?: string;
  /** Shared secret for the `x-corelink-internal-auth` header. */
  CORELINK_INTERNAL_AUTH_KEY?: string;
  /** Service binding to the main CoreLink Worker (bypasses CF edge error 1014). */
  CORELINK_API_SVC?: { fetch: typeof fetch };
}

/** What one archive sweep did. */
export interface AuditArchiveSweepResult {
  ok: boolean;
  status: number;
  rowsArchived: number;
  chunksCreated: number;
  partitionsFailed: number;
  /** The batch budget truncated the backlog; the next tick continues. */
  incomplete: boolean;
  /** Neither the dedicated nor the shared internal-auth key is bound. */
  skipped: boolean;
}

/**
 * Run the audit-archive sweep. POSTs the container archive endpoint once and
 * returns the outcome. NEVER throws (transport/parse errors are reported as
 * `ok:false`) so a cron failure cannot escape `scheduled()`.
 */
export async function runAuditArchiveSweep(
  env: AuditArchiveCronEnv,
  _nowMs: number,
): Promise<AuditArchiveSweepResult> {
  const eraseAuthKey = resolveEraseAuthKey(env);
  if (!eraseAuthKey) {
    return {
      ok: false,
      status: 0,
      rowsArchived: 0,
      chunksCreated: 0,
      partitionsFailed: 0,
      incomplete: false,
      skipped: true,
    };
  }

  const req = new Request(`${env.CORELINK_API_BASE}/_internal/audit/archive`, {
    method: "POST",
    headers: {
      "content-type": "application/json",
      "x-corelink-internal-auth": eraseAuthKey,
    },
    body: "{}",
  });

  try {
    const resp = env.CORELINK_API_SVC
      ? await env.CORELINK_API_SVC.fetch(req)
      : await fetch(req);
    let rowsArchived = 0;
    let chunksCreated = 0;
    let partitionsFailed = 0;
    let incomplete = false;
    try {
      const j = (await resp.json()) as {
        rows_archived?: number;
        chunks_created?: number;
        partitions_failed?: number;
        incomplete?: boolean;
      };
      rowsArchived = Number(j.rows_archived ?? 0);
      chunksCreated = Number(j.chunks_created ?? 0);
      partitionsFailed = Number(j.partitions_failed ?? 0);
      incomplete = Boolean(j.incomplete ?? false);
    } catch {
      // Non-JSON body — keep the transport status, report unknown counts. The
      // body is parsed even on a non-2xx because the handler reports
      // `partitions_failed` there, and that count is the useful diagnostic.
    }
    return {
      ok: resp.ok,
      status: resp.status,
      rowsArchived,
      chunksCreated,
      partitionsFailed,
      incomplete,
      skipped: false,
    };
  } catch (err) {
    console.error(
      `[audit-archive-cron] archive call threw: ${(err as Error).message.slice(0, 120)}`,
    );
    return {
      ok: false,
      status: 0,
      rowsArchived: 0,
      chunksCreated: 0,
      partitionsFailed: 0,
      incomplete: false,
      skipped: false,
    };
  }
}
