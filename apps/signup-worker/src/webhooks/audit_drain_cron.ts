/**
 * S-09 audit-chain drain sweep (compliance artifact 2, WP-C2).
 *
 * Cloudflare Cron Trigger handler: once per hour, POST the container's audit
 * drain endpoint (`POST /_internal/audit/drain`). The Rust handler (built by a
 * separate WP) seals every pending `audit_outbox` row into the tamper-evident
 * hash chain — per `(tenant_id, region)` partition it resumes the chain head,
 * computes the canonical JCS + chain hash for each unsealed row, and advances
 * `audit_chain_head`. The endpoint is IDEMPOTENT (already-sealed rows are
 * skipped), so an hourly sweep is safe and a missed hour simply seals a bigger
 * batch on the next run.
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

/**
 * Run the audit-chain drain sweep. POSTs the container drain endpoint once and
 * returns the outcome. NEVER throws (transport/parse errors are reported as
 * `ok:false`) so a cron failure cannot escape `scheduled()`.
 *
 * `skipped: true` (and zero counts) when NEITHER the dedicated erase key nor the
 * shared key is bound — the cron is inert until provisioning (task #46).
 */
export async function runAuditDrainSweep(
  env: AuditDrainCronEnv,
  _nowMs: number,
): Promise<{
  ok: boolean;
  status: number;
  sealed: number;
  partitions: number;
  skipped: boolean;
}> {
  // Inert only when NEITHER the dedicated erase key nor the shared key is bound
  // (rt-nuclear #23: same erase-first, shared-fallback resolution as the DSR
  // verify cron).
  const eraseAuthKey = resolveEraseAuthKey(env);
  if (!eraseAuthKey) {
    return { ok: false, status: 0, sealed: 0, partitions: 0, skipped: true };
  }

  const req = new Request(`${env.CORELINK_API_BASE}/_internal/audit/drain`, {
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
    let sealed = 0;
    let partitions = 0;
    if (resp.ok) {
      try {
        const j = (await resp.json()) as {
          sealed?: number;
          partitions?: number;
        };
        sealed = Number(j.sealed ?? 0);
        partitions = Number(j.partitions ?? 0);
      } catch {
        // non-JSON 200 — treat as a successful transport with unknown counts.
      }
    }
    return { ok: resp.ok, status: resp.status, sealed, partitions, skipped: false };
  } catch (err) {
    console.error(
      `[audit-drain-cron] drain call threw: ${(err as Error).message.slice(0, 120)}`,
    );
    return { ok: false, status: 0, sealed: 0, partitions: 0, skipped: false };
  }
}
