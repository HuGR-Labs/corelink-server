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
 * A partition whose chain is BROKEN (the 2026-08-14 seal fork left duplicated
 * sequence numbers in 8 of 360 partitions) no longer blocks its own healthy
 * rows: the handler archives the longest verifying prefix and QUARANTINES the
 * unarchivable remainder — sealed rows are evidence and are never re-sequenced.
 * Quarantined rows come back as `rows_quarantined` / `partitions_quarantined`
 * and are logged separately, because a row that is unarchivable forever must
 * never be a silent one.
 *
 * Auth: the archive route accepts only the dedicated erase/DSR consumer key
 * (see `resolveDedicatedEraseAuthKey`); the shared key is never a substitute.
 * Inert (no-op-with-log) until that key is bound, exactly like the drain sweep,
 * so a cron run on an unprovisioned env is a clean skip rather than a 401 storm.
 */

import { resolveDedicatedEraseAuthKey } from "../lib/erase-auth-key.js";

/** Minimal env surface the sweep needs. */
export interface AuditArchiveCronEnv {
  /** Base URL of the CoreLink API (container host). */
  CORELINK_API_BASE: string;
  /** Dedicated erase/DSR consumer secret required by the archive route. */
  CORELINK_ERASE_AUTH_KEY?: string;
  /** Present for other Worker duties; archive deliberately ignores it. */
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
  /** Fixed, container-supplied classes only; never a raw provider error. */
  failureCodes: readonly ArchiveFailureCode[];
  /** Rows ruled permanently unarchivable (a chain break; migration 0100). */
  rowsQuarantined: number;
  /** Partitions that contributed at least one quarantined row this sweep. */
  partitionsQuarantined: number;
  /** The batch budget truncated the backlog; the next tick continues. */
  incomplete: boolean;
  /** The dedicated internal-auth key is not safely bound. */
  skipped: boolean;
}

interface AuditArchiveResponse {
  rows_archived: number;
  chunks_created: number;
  chunks_already_present: number;
  partitions_archived: number;
  partitions_failed: number;
  failure_codes?: readonly ArchiveFailureCode[];
  rows_quarantined: number;
  partitions_quarantined: number;
  incomplete: boolean;
}

const ARCHIVE_FAILURE_CODES = [
  "immutable_key_conflict",
  "r2_conditional_put",
  "r2_read",
  "r2_consistency",
  "d1_mark",
  "d1_query",
  "row_decode",
  "epoch_evidence",
  "candidate_invalid",
  "unclassified",
] as const;

type ArchiveFailureCode = (typeof ARCHIVE_FAILURE_CODES)[number] | "unreported";

function parseFailureCodes(
  body: Record<string, unknown>,
  partitionsFailed: number,
): readonly ArchiveFailureCode[] | null {
  const raw = body.failure_codes;
  if (raw === undefined) {
    // An older container can still report a failure without this diagnostic.
    return partitionsFailed > 0 ? ["unreported"] : [];
  }
  if (
    !Array.isArray(raw) ||
    raw.length > ARCHIVE_FAILURE_CODES.length ||
    raw.length > partitionsFailed ||
    (partitionsFailed === 0 && raw.length !== 0) ||
    (partitionsFailed > 0 && raw.length === 0)
  ) {
    return null;
  }
  const allowed = new Set<string>(ARCHIVE_FAILURE_CODES);
  if (
    raw.some((code: unknown) => typeof code !== "string" || !allowed.has(code)) ||
    new Set(raw).size !== raw.length
  ) {
    return null;
  }
  return raw as ArchiveFailureCode[];
}

const ARCHIVE_COUNTER_FIELDS = [
  "rows_archived",
  "chunks_created",
  "chunks_already_present",
  "partitions_archived",
  "partitions_failed",
  "rows_quarantined",
  "partitions_quarantined",
] as const;

/** Decode the Rust handler's complete JSON contract; unknown is never success. */
function parseAuditArchiveResponse(value: unknown):
  | { value: AuditArchiveResponse }
  | { error: string } {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return { error: "body is not an object" };
  }

  const body = value as Record<string, unknown>;
  if (typeof body.incomplete !== "boolean") {
    return { error: "missing or non-boolean incomplete" };
  }
  for (const field of ARCHIVE_COUNTER_FIELDS) {
    const counter = body[field];
    if (
      typeof counter !== "number" ||
      !Number.isSafeInteger(counter) ||
      counter < 0
    ) {
      return { error: `missing or invalid ${field}` };
    }
  }

  const failureCodes = parseFailureCodes(body, body.partitions_failed as number);
  if (failureCodes === null) {
    return { error: "invalid failure_codes" };
  }

  return {
    value: {
      ...(body as unknown as AuditArchiveResponse),
      failure_codes: failureCodes,
    },
  };
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
  const eraseAuthKey = resolveDedicatedEraseAuthKey(env);
  if (!eraseAuthKey) {
    return {
      ok: false,
      status: 0,
      rowsArchived: 0,
      chunksCreated: 0,
      partitionsFailed: 0,
      failureCodes: [],
      rowsQuarantined: 0,
      partitionsQuarantined: 0,
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
    let failureCodes: readonly ArchiveFailureCode[] = [];
    let rowsQuarantined = 0;
    let partitionsQuarantined = 0;
    let incomplete = true;
    let ok = resp.ok;
    try {
      const parsed = parseAuditArchiveResponse(await resp.json());
      if ("error" in parsed) {
        ok = false;
        console.error(
          `[audit-archive-cron] archive call returned invalid JSON contract: ${parsed.error}`,
        );
      } else {
        const j = parsed.value;
        rowsArchived = j.rows_archived;
        chunksCreated = j.chunks_created;
        partitionsFailed = j.partitions_failed;
        failureCodes = j.failure_codes ?? [];
        rowsQuarantined = j.rows_quarantined;
        partitionsQuarantined = j.partitions_quarantined;
        incomplete = j.incomplete;
      }
    } catch {
      ok = false;
      console.error(
        "[audit-archive-cron] archive call returned malformed JSON; completion is unknown",
      );
    }
    if (!resp.ok) {
      // A non-2xx is terminal for this tick even when the handler includes a
      // diagnostic body; the archive completion state is not trustworthy.
      ok = false;
      incomplete = true;
    }
    if (partitionsFailed > 0) {
      // Older or inconsistent handlers may return 200 for partial failure.
      // The count is authoritative; a successful HTTP envelope cannot clear it.
      ok = false;
      incomplete = true;
    }
    return {
      ok,
      status: resp.status,
      rowsArchived,
      chunksCreated,
      partitionsFailed,
      failureCodes,
      rowsQuarantined,
      partitionsQuarantined,
      incomplete,
      skipped: false,
    };
  } catch (err) {
    // The thrown transport error is not an authenticated diagnostic and may
    // contain a URL or provider text. Never print it into Worker logs.
    void err;
    console.error("[audit-archive-cron] archive call threw: transport error");
    return {
      ok: false,
      status: 0,
      rowsArchived: 0,
      chunksCreated: 0,
      partitionsFailed: 0,
      failureCodes: [],
      rowsQuarantined: 0,
      partitionsQuarantined: 0,
      incomplete: false,
      skipped: false,
    };
  }
}
