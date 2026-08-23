/**
 * RequestMeterShardDO — the per-(tenant,region) edge-local balance holder in the
 * P3 edge-local metering design (ACCEPTED,
 * docs/design/2026-08-19-adr-edge-local-do-request-metering.md).
 *
 * ONE instance per (tenant, region) (`idFromName(tenant + ":" + region)`), so the
 * DO runtime serializes every debit against it and the pure core's local
 * over-serve=0 holds under concurrency. Each request debits the LOCAL balance
 * with no cross-DO hop; when the shell reports `needsRefill` the caller tops up
 * from the per-tenant `RequestMeterCoordinatorDO` and feeds the grant back via
 * `applyRefill`. The debit/refill/epoch arithmetic itself lives in
 * `lib/request_meter_shard.ts` (fully unit-tested); this shell only persists
 * `ShardState` and routes ops.
 *
 * LIVE: wired into the request path in worker/src/index.ts, and it does NOT
 * hold the coordinator binding — the coordinator hop + `EDGE_DO_METER` gate
 * are wired there too (`EDGE_DO_METER` = "serve" for prod environments).
 * The refill values (`spentDelta`, `reportedBalance`) are RETURNED to the caller
 * so this shell never has to hold the coordinator binding itself.
 *
 * Charter (mirrored from request_meter_coordinator_do.ts):
 *   - INV-NO-PII-IN-LOGS: tenant ids are NOT logged (the tenant+region is the DO
 *     name, never emitted here); only counts, which are not PII.
 *   - JSON error envelopes match the shape used across the Worker.
 */

import type {
  DurableObject,
  DurableObjectState,
  DurableObjectStorage,
} from "@cloudflare/workers-types";
import type { Env } from "./index.js";
import {
  emptyShard,
  debit,
  prepareRefill,
  applyRefill,
  type ShardState,
} from "./lib/request_meter_shard.js";

/** Storage key holding the single per-(tenant,region) ShardState. */
const STATE_KEY = "shard";

interface DebitOp {
  readonly op: "debit";
  readonly yearMonth: string;
  readonly lowWater: number;
}
interface ApplyRefillOp {
  readonly op: "applyRefill";
  readonly yearMonth: string;
  readonly newBalance: number;
}
interface ReadOp {
  readonly op: "read";
}
type ShardOp = DebitOp | ApplyRefillOp | ReadOp;

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

export class RequestMeterShardDO implements DurableObject {
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
    let op: ShardOp;
    try {
      op = (await request.json()) as ShardOp;
    } catch {
      return jsonResponse({ error: "bad_json" }, 400);
    }
    if (op === null || typeof op !== "object") {
      return jsonResponse({ error: "bad_op" }, 400);
    }

    switch (op.op) {
      case "debit":
        return this.handleDebit(op);
      case "applyRefill":
        return this.handleApplyRefill(op);
      case "read":
        return this.handleRead();
      default:
        return jsonResponse({ error: "unknown_op" }, 400);
    }
  }

  /** Load the persisted state, defaulting to a fresh month shard. */
  private async load(yearMonth: string): Promise<ShardState> {
    const stored = await this.storage.get<ShardState>(STATE_KEY);
    return stored ?? emptyShard(yearMonth);
  }

  private async handleDebit(op: DebitOp): Promise<Response> {
    if (!isNonEmptyString(op.yearMonth) || !isFiniteNumber(op.lowWater)) {
      return jsonResponse({ error: "bad_debit_args" }, 400);
    }
    // Serialize the read-modify-write against the single DO instance so the
    // core's local over-serve=0 holds under concurrency.
    const result = await this.state.blockConcurrencyWhile(async () => {
      const prev = await this.load(op.yearMonth);
      const r = debit(prev, op.yearMonth, op.lowWater);
      await this.storage.put(STATE_KEY, r.state);
      // Return the refill inputs so the caller can top up from the coordinator
      // (the shell holds no coordinator binding — see the INERT note above).
      const req = prepareRefill(r.state);
      return {
        served: r.served,
        needsRefill: r.needsRefill,
        balance: r.state.balance,
        yearMonth: r.state.yearMonth,
        refillReq: { spentDelta: req.spentDelta, reportedBalance: req.reportedBalance },
      };
    });
    return jsonResponse(result);
  }

  private async handleApplyRefill(op: ApplyRefillOp): Promise<Response> {
    if (!isNonEmptyString(op.yearMonth) || !isFiniteNumber(op.newBalance)) {
      return jsonResponse({ error: "bad_apply_refill_args" }, 400);
    }
    const next = await this.state.blockConcurrencyWhile(async () => {
      const prev = await this.load(op.yearMonth);
      const s = applyRefill(prev, op.yearMonth, op.newBalance);
      await this.storage.put(STATE_KEY, s);
      return s;
    });
    return jsonResponse({ balance: next.balance, yearMonth: next.yearMonth });
  }

  private async handleRead(): Promise<Response> {
    const stored = await this.storage.get<ShardState>(STATE_KEY);
    if (stored === undefined) {
      return jsonResponse({ present: false });
    }
    return jsonResponse({ present: true, state: stored });
  }
}
