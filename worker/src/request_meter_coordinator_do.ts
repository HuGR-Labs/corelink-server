/**
 * RequestMeterCoordinatorDO — the per-tenant authority for the monthly
 * request-count cap in the P3 edge-local metering design (ACCEPTED,
 * docs/design/2026-08-19-adr-edge-local-do-request-metering.md).
 *
 * ONE instance per tenant (`idFromName(tenant_id)`), so the DO runtime serializes
 * every fetch against it — that single-writer guarantee is what makes the pure
 * core's **over-serve = 0** invariant hold under real concurrency (the core is
 * pure arithmetic; this shell is the serialization + durability the core assumes).
 * The lease accounting itself lives in `lib/request_meter_lease.ts` (WP-1a, fully
 * unit-tested); this shell only persists `CoordinatorState` and routes ops.
 *
 * INERT: not yet wired to any request path (that is the `EDGE_DO_METER`-gated
 * worker change, a later WP). Bound + exported so it can be shadow-tested.
 *
 * Charter (mirrored from event_log_do.ts):
 *   - INV-NO-PII-IN-LOGS: tenant ids are NOT logged (the tenant id is the DO name,
 *     never emitted here); only region codes + counts, which are not PII.
 *   - JSON error envelopes match the REAPI shape used across the Worker.
 */

import type {
  DurableObject,
  DurableObjectState,
  DurableObjectStorage,
} from "@cloudflare/workers-types";
import type { Env } from "./index.js";
import {
  emptyState,
  refill,
  reclaimIdle,
  remaining,
  type CoordinatorState,
} from "./lib/request_meter_lease.js";

/** Storage key holding the single per-tenant CoordinatorState. */
const STATE_KEY = "coord";

interface RefillOp {
  readonly op: "refill";
  readonly cap: number;
  readonly region: string;
  readonly yearMonth: string;
  readonly spentDelta: number;
  readonly reportedBalance: number;
  readonly block: number;
}
interface ReclaimOp {
  readonly op: "reclaim";
  readonly region: string;
  readonly yearMonth: string;
  readonly keep: number;
}
interface ReadOp {
  readonly op: "read";
}
type MeterOp = RefillOp | ReclaimOp | ReadOp;

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

function isFiniteNumber(v: unknown): v is number {
  return typeof v === "number" && Number.isFinite(v);
}
function isNonEmptyString(v: unknown): v is string {
  return typeof v === "string" && v.length > 0;
}

export class RequestMeterCoordinatorDO implements DurableObject {
  private readonly state: DurableObjectState;
  private readonly storage: DurableObjectStorage;

  constructor(state: DurableObjectState, _env: Env) {
    this.state = state;
    this.storage = state.storage;
  }

  async fetch(request: Request): Promise<Response> {
    if (request.method !== "POST") {
      return jsonResponse({ error: "method_not_allowed" }, 405);
    }
    let op: MeterOp;
    try {
      op = (await request.json()) as MeterOp;
    } catch {
      return jsonResponse({ error: "bad_json" }, 400);
    }
    if (op === null || typeof op !== "object") {
      return jsonResponse({ error: "bad_op" }, 400);
    }

    switch (op.op) {
      case "refill":
        return this.handleRefill(op);
      case "reclaim":
        return this.handleReclaim(op);
      case "read":
        return this.handleRead();
      default:
        return jsonResponse({ error: "unknown_op" }, 400);
    }
  }

  /** Load the persisted state, defaulting to a fresh month state. */
  private async load(yearMonth: string): Promise<CoordinatorState> {
    const stored = await this.storage.get<CoordinatorState>(STATE_KEY);
    return stored ?? emptyState(yearMonth);
  }

  private async handleRefill(op: RefillOp): Promise<Response> {
    if (
      !isFiniteNumber(op.cap) ||
      !isNonEmptyString(op.region) ||
      !isNonEmptyString(op.yearMonth) ||
      !isFiniteNumber(op.spentDelta) ||
      !isFiniteNumber(op.reportedBalance) ||
      !isFiniteNumber(op.block)
    ) {
      return jsonResponse({ error: "bad_refill_args" }, 400);
    }
    // Serialize the read-modify-write against the single DO instance so the
    // core's `consumed + Σoutstanding ≤ cap` invariant holds under concurrency.
    const result = await this.state.blockConcurrencyWhile(async () => {
      const prev = await this.load(op.yearMonth);
      const r = refill(
        prev,
        op.cap,
        op.region,
        op.yearMonth,
        op.spentDelta,
        op.reportedBalance,
        op.block,
      );
      await this.storage.put(STATE_KEY, r.state);
      return {
        granted: r.granted,
        newBalance: r.newBalance,
        remaining: remaining(r.state, op.cap),
        consumed: r.state.consumed,
        yearMonth: r.state.yearMonth,
      };
    });
    return jsonResponse(result);
  }

  private async handleReclaim(op: ReclaimOp): Promise<Response> {
    if (
      !isNonEmptyString(op.region) ||
      !isNonEmptyString(op.yearMonth) ||
      !isFiniteNumber(op.keep)
    ) {
      return jsonResponse({ error: "bad_reclaim_args" }, 400);
    }
    const next = await this.state.blockConcurrencyWhile(async () => {
      const prev = await this.load(op.yearMonth);
      const s = reclaimIdle(prev, op.region, op.yearMonth, op.keep);
      await this.storage.put(STATE_KEY, s);
      return s;
    });
    return jsonResponse({ yearMonth: next.yearMonth, consumed: next.consumed });
  }

  private async handleRead(): Promise<Response> {
    const stored = await this.storage.get<CoordinatorState>(STATE_KEY);
    if (stored === undefined) {
      return jsonResponse({ present: false });
    }
    return jsonResponse({ present: true, state: stored });
  }
}
