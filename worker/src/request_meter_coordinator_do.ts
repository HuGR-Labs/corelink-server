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
  D1Database,
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

/** The narrow D1 surface the seed/reconcile need (CONFIG_DB). */
type D1Like = Pick<D1Database, "prepare">;

/**
 * SEED: read the tenant's authoritative D1 request count for this month so the
 * coordinator's `consumed` starts from the real ledger value, not 0. Called once
 * per (tenant, month) — on the first refill of a fresh month. A missing row (new
 * month) reads as 0. This is what makes a shadow run track D1 from the correct
 * baseline (so its verdict is comparable) AND what lets a `serve` flip start from
 * the true count instead of silently granting a whole extra month's worth.
 */
async function seedConsumedFromD1(
  db: D1Like,
  tenantId: string,
  yearMonth: string,
): Promise<number> {
  const row = await db
    .prepare(
      "SELECT request_count FROM monthly_request_counts WHERE tenant_id = ?1 AND year_month = ?2 LIMIT 1",
    )
    .bind(tenantId, yearMonth)
    .first<{ request_count: number }>();
  const count = row?.request_count ?? 0;
  return Number.isFinite(count) && count > 0 ? count : 0;
}

/**
 * RECONCILE (serve-only): write the coordinator's `consumed` back to the D1
 * ledger so `monthly_request_counts` stays the billing source of truth even when
 * `serve` mode skips the per-request D1 UPSERT. `MAX(existing, consumed)` so the
 * write is idempotent and never DECREASES the ledger — a stale coordinator write
 * (or a shadow→serve transition where D1 briefly ran ahead) can only be a no-op,
 * never an under-count. MUST NOT run in shadow mode: there D1 is authoritative
 * and the coordinator is behind, so writing would corrupt the ledger.
 */
async function reconcileConsumedToD1(
  db: D1Like,
  tenantId: string,
  yearMonth: string,
  consumed: number,
  nowMs: number,
): Promise<void> {
  await db
    .prepare(
      "INSERT INTO monthly_request_counts (tenant_id, year_month, request_count, updated_at_ms) " +
        "VALUES (?1, ?2, ?3, ?4) " +
        "ON CONFLICT(tenant_id, year_month) " +
        "DO UPDATE SET request_count = MAX(request_count, ?3), updated_at_ms = ?4",
    )
    .bind(tenantId, yearMonth, Math.max(0, Math.floor(consumed)), nowMs)
    .run();
}

interface RefillOp {
  readonly op: "refill";
  readonly cap: number;
  readonly region: string;
  readonly yearMonth: string;
  readonly spentDelta: number;
  readonly reportedBalance: number;
  readonly block: number;
  /**
   * Tenant id (the DO's identity) — needed to seed/reconcile the D1 ledger.
   * OPTIONAL: when absent (or CONFIG_DB is unbound) the coordinator runs as a
   * pure in-memory lease with no D1 seam, exactly as before WP-4.
   */
  readonly tenantId?: string;
  /**
   * SERVE-only: write `consumed` back to the D1 ledger after this refill. MUST be
   * false/absent in shadow (D1 is authoritative there). Requires `tenantId`.
   */
  readonly reconcileToD1?: boolean;
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
  private readonly env: Env;

  constructor(state: DurableObjectState, env: Env) {
    this.state = state;
    this.storage = state.storage;
    this.env = env;
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
    const db = this.env.CONFIG_DB as D1Like | undefined;
    const canD1 = isNonEmptyString(op.tenantId) && db !== undefined;
    // Serialize the read-modify-write against the single DO instance so the
    // core's `consumed + Σoutstanding ≤ cap` invariant holds under concurrency.
    // The D1 seed/reconcile run INSIDE the serialized block so two concurrent
    // first-of-month refills cannot double-seed (the second sees stored state).
    const result = await this.state.blockConcurrencyWhile(async () => {
      const stored = await this.storage.get<CoordinatorState>(STATE_KEY);
      // SEED on the first refill of a fresh month (no stored state, or the month
      // just rolled): initialise `consumed` from the authoritative D1 ledger so
      // the coordinator tracks from the real baseline instead of 0.
      let base: CoordinatorState;
      if (stored === undefined || stored.yearMonth !== op.yearMonth) {
        const seeded =
          canD1 && db !== undefined
            ? await seedConsumedFromD1(db, op.tenantId as string, op.yearMonth)
            : 0;
        base = { yearMonth: op.yearMonth, consumed: seeded, outstanding: {} };
      } else {
        base = stored;
      }
      const r = refill(
        base,
        op.cap,
        op.region,
        op.yearMonth,
        op.spentDelta,
        op.reportedBalance,
        op.block,
      );
      await this.storage.put(STATE_KEY, r.state);
      // RECONCILE (serve-only): write `consumed` back to the D1 ledger. Gated so
      // it NEVER runs in shadow (where D1 is authoritative and would be corrupted).
      if (op.reconcileToD1 === true && canD1 && db !== undefined) {
        await reconcileConsumedToD1(
          db,
          op.tenantId as string,
          op.yearMonth,
          r.state.consumed,
          Date.now(),
        );
      }
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
