/**
 * EventLogDO — a thin, generic, append-only event-log Durable Object primitive.
 *
 * Ratified by ADR-0065 (hugit-P2 seam D). This is deliberately MINIMAL: a
 * per-tenant, append-only, totally-ordered log. It provides ordering +
 * durability and NOTHING else.
 *
 * Surface (ADR-0065 "Decision"):
 *   - append(tenant_id, payload) -> { seq, ts_ms }
 *   - read(tenant_id, from_seq, limit) -> entries[]
 *
 * NOT chain-aware (ADR-0065): no hashing, no Merkle tree, no signatures, no
 * hash-linking. The consumer (hugit) layers its integrity chain ON TOP of this
 * raw ordered log (see ADR-0066). The crypto risk lives in the consumer, never
 * in this shared primitive.
 *
 * Ordering guarantee (frozen contract — consumers depend on it):
 *   - One DO instance per tenant: the Worker derives the DO id from the
 *     PAT-resolved tenant (idFromName(tenant_id)), so a single DO is the sole
 *     writer for a tenant's log. `seq` is a strictly-monotonic, gap-free,
 *     1-based counter assigned under the DO's single-writer serialization.
 *   - `ts_ms` is the wall-clock (Date.now()) at append time. It is advisory
 *     metadata; `seq` — NOT `ts_ms` — defines total order.
 *
 * Auth / tenant authority (matches the existing DO idioms):
 *   - The Worker is the SOLE authority for the tenant id. It strips any
 *     client-supplied `x-corelink-tenant-id` and re-sets it from the trusted
 *     D1-resolved PAT before forwarding (see CLIENT_TRUST_HEADERS in index.ts).
 *     This DO reads `x-corelink-tenant-id` as a server-trusted value and pins
 *     it to the DO instance the first time it is seen; subsequent requests
 *     carrying a DIFFERENT tenant id are rejected (cross-tenant guard), since a
 *     tenant-pinned DO must never serve another tenant's log.
 *
 * Retention (ADR-0065 "bounded by retention + size caps"):
 *   - The DO storage budget is finite. This primitive does not itself archive
 *     to R2; rolloff/archive is a documented policy layered by the operator.
 *     The single bound enforced here is the per-append payload size cap, which
 *     keeps a single entry from exhausting the per-key storage limit. Whole-log
 *     retention/rolloff is intentionally OUT of this thin primitive's scope
 *     (ADR-0065: "do not over-build").
 *
 * Charter constraints (mirrored from durable_object.ts):
 *   - INV-NO-BODY-IN-LOGS: payload bytes are NEVER logged.
 *   - INV-NO-PII-IN-LOGS: tenant ids are NOT logged.
 *   - JSON error envelopes match the REAPI shape used across the Worker.
 *
 * Status: deployed and bound (EVENT_LOG_DO, migration tag v3, prod
 * environments), but no handler in worker/src currently calls
 * `env.EVENT_LOG_DO`. It carries no traffic.
 */

import type {
  DurableObject,
  DurableObjectState,
  DurableObjectStorage,
} from "@cloudflare/workers-types";
import type { Env } from "./index.js";

// ──────────────────────────────────────────────────────────────────────────────
// Constants
// ──────────────────────────────────────────────────────────────────────────────

/** Storage key holding the per-tenant monotonic head (last assigned seq). */
const HEAD_KEY = "head";
/** Storage key holding this DO's pinned tenant id. */
const TENANT_KEY = "tenant";
/** Prefix for entry rows: `entry:<zero-padded-seq>`. */
const ENTRY_PREFIX = "entry:";
/**
 * Zero-pad width for the seq embedded in the entry storage key. Lexicographic
 * order of the padded key MUST equal numeric order of seq so `storage.list`
 * with a start key returns entries in seq order. 18 digits covers the full
 * range of a JS safe integer (Number.MAX_SAFE_INTEGER ≈ 9.0e15, 16 digits).
 */
const SEQ_PAD_WIDTH = 18;

/** Default `read` page size when `limit` is absent. */
const DEFAULT_READ_LIMIT = 100;
/** Hard cap on `read` page size — bounds a single response. */
const MAX_READ_LIMIT = 1000;

/**
 * Max payload bytes per appended entry. Keeps one entry well under the DO
 * per-value storage limit (128 KiB) with headroom for the envelope. A payload
 * over this cap is rejected with 413 rather than silently truncated.
 */
const MAX_PAYLOAD_BYTES = 96 * 1024;

// ──────────────────────────────────────────────────────────────────────────────
// Types
// ──────────────────────────────────────────────────────────────────────────────

/** A single persisted log entry. */
interface LogEntry {
  /** Strictly-monotonic, gap-free, 1-based sequence number. */
  readonly seq: number;
  /** Wall-clock append time (ms since epoch). Advisory; `seq` defines order. */
  readonly ts_ms: number;
  /** The opaque consumer payload — stored verbatim, never interpreted. */
  readonly payload: unknown;
}

/** Result of an `append`. */
interface AppendResult {
  readonly seq: number;
  readonly ts_ms: number;
}

// ──────────────────────────────────────────────────────────────────────────────
// Helpers
// ──────────────────────────────────────────────────────────────────────────────

function entryKey(seq: number): string {
  return ENTRY_PREFIX + String(seq).padStart(SEQ_PAD_WIDTH, "0");
}

function jsonResponse(body: unknown, status: number, requestId: string): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: {
      "Content-Type": "application/json",
      "X-Request-Id": requestId,
    },
  });
}

function errorResponse(
  error: string,
  message: string,
  status: number,
  requestId: string,
): Response {
  return jsonResponse({ error, message, request_id: requestId }, status, requestId);
}

// ──────────────────────────────────────────────────────────────────────────────
// Durable Object class
// ──────────────────────────────────────────────────────────────────────────────

export class EventLogDO implements DurableObject {
  private readonly state: DurableObjectState;
  private readonly storage: DurableObjectStorage;
  private readonly env: Env;

  /** In-memory cache of the persisted head; restored on wake. */
  private head = 0;
  /** In-memory cache of the pinned tenant id; restored on wake. */
  private pinnedTenant: string | null = null;
  /** Resolves once the persisted head + tenant pin are restored. */
  private readonly ready: Promise<void>;

  constructor(state: DurableObjectState, env: Env) {
    this.state = state;
    this.storage = state.storage;
    this.env = env;

    // Restore persisted head + tenant pin on DO wakeup. The CF runtime's
    // blockConcurrencyWhile already serializes this ahead of any fetch; we also
    // retain the promise and await it at the top of `fetch` so the read-modify
    // path never observes a pre-restore (head=0 / unpinned) state — correct in
    // both the real runtime and any harness that doesn't block on the ctor.
    this.ready = this.state.blockConcurrencyWhile(async () => {
      const storedHead = await this.storage.get<number>(HEAD_KEY);
      if (storedHead !== undefined) {
        this.head = storedHead;
      }
      const storedTenant = await this.storage.get<string>(TENANT_KEY);
      if (storedTenant !== undefined) {
        this.pinnedTenant = storedTenant;
      }
    });
  }

  // ──────────────────────────────────────────────────────────────────────────
  // fetch — DO entry point
  // ──────────────────────────────────────────────────────────────────────────
  //
  // Routes (forwarded by the Worker, which sets the trusted x-corelink-tenant-id):
  //   GET  /_do/health          → liveness probe (no tenant required)
  //   POST /_eventlog/append    → append(payload); body = raw payload JSON
  //   GET  /_eventlog/read?from_seq=&limit=  → read(from_seq, limit)

  async fetch(request: Request): Promise<Response> {
    // Ensure the persisted head + tenant pin are restored before any branch
    // reads them (no-op once resolved).
    await this.ready;

    const requestId = request.headers.get("x-request-id") ?? crypto.randomUUID();
    const url = new URL(request.url);

    if (url.pathname === "/_do/health") {
      return jsonResponse(
        { status: "ok", class: "EventLogDO", request_id: requestId },
        200,
        requestId,
      );
    }

    // Tenant pin / cross-tenant guard. The Worker is the sole setter of
    // x-corelink-tenant-id (stripped-then-set from the trusted PAT). The DO is
    // tenant-pinned: it adopts the first tenant it sees and refuses any other.
    const incomingTenant = request.headers.get("x-corelink-tenant-id");
    if (incomingTenant === null || incomingTenant.length === 0) {
      return errorResponse(
        "TENANT_REQUIRED",
        "missing x-corelink-tenant-id",
        400,
        requestId,
      );
    }
    if (this.pinnedTenant === null) {
      this.pinnedTenant = incomingTenant;
      await this.storage.put(TENANT_KEY, incomingTenant);
    } else if (this.pinnedTenant !== incomingTenant) {
      // A tenant-pinned DO must never serve a different tenant's log.
      return errorResponse(
        "TENANT_MISMATCH",
        "event-log instance is pinned to a different tenant",
        403,
        requestId,
      );
    }

    if (request.method === "POST" && url.pathname === "/_eventlog/append") {
      return this.handleAppend(request, requestId);
    }
    if (request.method === "GET" && url.pathname === "/_eventlog/read") {
      return this.handleRead(url, requestId);
    }

    return errorResponse(
      "NOT_FOUND",
      "unknown event-log route",
      404,
      requestId,
    );
  }

  // ──────────────────────────────────────────────────────────────────────────
  // append(payload) -> { seq, ts_ms }
  // ──────────────────────────────────────────────────────────────────────────

  private async handleAppend(request: Request, requestId: string): Promise<Response> {
    // Read the raw payload. INV-NO-BODY-IN-LOGS: the bytes are NEVER logged.
    const raw = await request.text();
    const payloadBytes = new TextEncoder().encode(raw).length;
    if (payloadBytes > MAX_PAYLOAD_BYTES) {
      return errorResponse(
        "PAYLOAD_TOO_LARGE",
        `payload exceeds ${MAX_PAYLOAD_BYTES} bytes`,
        413,
        requestId,
      );
    }

    let payload: unknown;
    if (raw.length === 0) {
      payload = null;
    } else {
      try {
        payload = JSON.parse(raw);
      } catch {
        return errorResponse("INVALID_PAYLOAD", "payload must be valid JSON", 400, requestId);
      }
    }

    // Single-writer ordering: the DO serializes fetches, so head++ is a safe
    // monotonic assignment. blockConcurrencyWhile makes the read-modify-write of
    // head + the two storage puts atomic against any interleaving append.
    const result = await this.state.blockConcurrencyWhile<AppendResult>(async () => {
      const seq = this.head + 1;
      const ts_ms = Date.now();
      const entry: LogEntry = { seq, ts_ms, payload };
      // Persist the entry, then advance the head. Order matters: if the second
      // put never lands, the entry is orphaned (unreachable via head) rather
      // than the head pointing at a missing entry — read stays gap-free.
      await this.storage.put(entryKey(seq), entry);
      await this.storage.put(HEAD_KEY, seq);
      this.head = seq;
      return { seq, ts_ms };
    });

    return jsonResponse(result, 200, requestId);
  }

  // ──────────────────────────────────────────────────────────────────────────
  // read(from_seq, limit) -> entries[]
  // ──────────────────────────────────────────────────────────────────────────

  private async handleRead(url: URL, requestId: string): Promise<Response> {
    const fromSeq = parsePositiveInt(url.searchParams.get("from_seq"), 1);
    if (fromSeq === null || fromSeq < 1) {
      return errorResponse("INVALID_FROM_SEQ", "from_seq must be an integer >= 1", 400, requestId);
    }

    let limit = parsePositiveInt(url.searchParams.get("limit"), DEFAULT_READ_LIMIT);
    if (limit === null || limit < 1) {
      return errorResponse("INVALID_LIMIT", "limit must be a positive integer", 400, requestId);
    }
    if (limit > MAX_READ_LIMIT) {
      limit = MAX_READ_LIMIT;
    }

    // Lexicographic key order == numeric seq order (zero-padded keys), so a
    // prefixed list starting at entry:<from_seq> yields entries in seq order.
    const stored = await this.storage.list<LogEntry>({
      prefix: ENTRY_PREFIX,
      start: entryKey(fromSeq),
      limit,
    });

    const entries: LogEntry[] = [];
    for (const entry of stored.values()) {
      entries.push(entry);
    }

    return jsonResponse(
      { entries, head: this.head, next_seq: this.head + 1 },
      200,
      requestId,
    );
  }
}

/**
 * Parse a non-negative integer query param. Returns `fallback` when the raw
 * value is null/empty, or null when present-but-malformed (caller maps to 400).
 */
function parsePositiveInt(raw: string | null, fallback: number): number | null {
  if (raw === null || raw.length === 0) {
    return fallback;
  }
  if (!/^[0-9]+$/.test(raw)) {
    return null;
  }
  const n = Number(raw);
  if (!Number.isSafeInteger(n)) {
    return null;
  }
  return n;
}
