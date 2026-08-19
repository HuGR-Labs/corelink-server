// Worker-side orchestration for P3 edge-local request metering (ACCEPTED,
// docs/design/2026-08-19-adr-edge-local-do-request-metering.md).
//
// One request's metering verdict via the two Durable Objects:
//   - RequestMeterShardDO (1/(tenant,region)) — debit one token from the local
//     lease balance (no cap authority; edge-local, cheap).
//   - RequestMeterCoordinatorDO (1/tenant) — the monthly-cap authority; hands the
//     shard a fresh lease block when the shard runs low or empty.
//
// This module is the ONLY place that sequences shard.debit → (coordinator.refill →
// shard.applyRefill → retry) so both the `shadow` (compare-only) and, later, the
// `serve` caller share one code path. It performs NO enforcement itself — it
// returns a verdict; the caller decides what to do with it. Pure orchestration
// over the DO fetch contract, so it is unit-testable with mock namespaces.
//
// INV-NO-PII-IN-LOGS: this module logs nothing; the caller owns telemetry and
// must not emit the tenant id.

import type { DurableObjectNamespace } from "@cloudflare/workers-types";

/** The two DO namespaces this metering path needs (from `Env`). */
export interface DOMeterNamespaces {
  readonly shard: DurableObjectNamespace;
  readonly coordinator: DurableObjectNamespace;
}

export interface DOMeterParams {
  readonly tenantId: string;
  /** This worker's region code (the shard is per tenant+region). */
  readonly region: string;
  /** UTC calendar month `YYYY-MM` (quota.ts month key). */
  readonly yearMonth: string;
  /** The tenant tier's monthly request cap. */
  readonly cap: number;
  /** Lease block size `L` — tokens the coordinator grants per refill. */
  readonly block: number;
  /** Refill-eagerly threshold; the shard flags a refill at/below this balance. */
  readonly lowWater: number;
  /**
   * SERVE-only: have the coordinator write `consumed` back to the D1 ledger on
   * refill. MUST be false/absent in shadow (D1 is authoritative there). Defaults
   * to false, so a shadow caller never touches the ledger.
   */
  readonly reconcileToD1?: boolean;
}

export interface DOMeterVerdict {
  /** True iff this request is within the monthly cap (a token was available). */
  readonly withinCap: boolean;
  /** The shard's local lease balance after this request. */
  readonly balance: number;
  /** True iff a coordinator refill round-trip was performed. */
  readonly refilled: boolean;
}

interface DebitResponse {
  served: boolean;
  needsRefill: boolean;
  balance: number;
  yearMonth: string;
  refillReq: { spentDelta: number; reportedBalance: number };
}
interface RefillResponse {
  granted: number;
  newBalance: number;
}
interface ApplyRefillResponse {
  balance: number;
}

async function postJson<T>(
  ns: DurableObjectNamespace,
  name: string,
  body: unknown,
): Promise<T> {
  const stub = ns.get(ns.idFromName(name));
  const res = await stub.fetch("https://do/meter", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
  if (!res.ok) {
    throw new Error(`do_meter_op_failed status=${res.status}`);
  }
  return (await res.json()) as T;
}

/**
 * Meter ONE request through the shard (+ coordinator on a low/empty balance) and
 * return the within-cap verdict. Sequence:
 *   1. shard.debit — serve from the local balance if any.
 *   2. if the shard asks to refill (empty, or at low-water): coordinator.refill
 *      (reconciles the shard's spend into `consumed`, grants ≤ block without ever
 *      exceeding the cap), then shard.applyRefill adopts the new balance.
 *   3. if step 1 DENIED (empty) but the refill granted tokens, retry the debit
 *      once so a shard that merely ran out this instant still serves.
 *
 * The over-serve=0 guarantee is the coordinator's (it never grants past the cap);
 * this function only sequences the hops. Throws if any DO op returns non-2xx —
 * the caller (shadow: swallow into telemetry; serve: fail-open per its own rule)
 * decides how to treat an error, exactly as the D1 path treats a `d1Error`.
 */
export async function meterViaDO(
  ns: DOMeterNamespaces,
  params: DOMeterParams,
): Promise<DOMeterVerdict> {
  const shardName = `${params.tenantId}:${params.region}`;
  const debit = await postJson<DebitResponse>(ns.shard, shardName, {
    op: "debit",
    yearMonth: params.yearMonth,
    lowWater: params.lowWater,
  });

  if (!debit.needsRefill) {
    return { withinCap: debit.served, balance: debit.balance, refilled: false };
  }

  // Low or empty — reconcile spend + fetch a fresh lease from the coordinator.
  const refill = await postJson<RefillResponse>(ns.coordinator, params.tenantId, {
    op: "refill",
    cap: params.cap,
    region: params.region,
    yearMonth: params.yearMonth,
    spentDelta: debit.refillReq.spentDelta,
    reportedBalance: debit.refillReq.reportedBalance,
    block: params.block,
    tenantId: params.tenantId,
    reconcileToD1: params.reconcileToD1 === true,
  });
  const applied = await postJson<ApplyRefillResponse>(ns.shard, shardName, {
    op: "applyRefill",
    yearMonth: params.yearMonth,
    newBalance: refill.newBalance,
  });

  // Already served in step 1 (was at low-water, not empty) — done.
  if (debit.served) {
    return { withinCap: true, balance: applied.balance, refilled: true };
  }
  // Was empty; the coordinator granted nothing more ⇒ genuinely over cap.
  if (refill.granted <= 0) {
    return { withinCap: false, balance: applied.balance, refilled: true };
  }
  // Was empty but the refill granted tokens — retry the debit once.
  const retry = await postJson<DebitResponse>(ns.shard, shardName, {
    op: "debit",
    yearMonth: params.yearMonth,
    lowWater: params.lowWater,
  });
  return { withinCap: retry.served, balance: retry.balance, refilled: true };
}

/**
 * SERVE path: meter one request through the DO as the AUTHORITATIVE monthly
 * request-count cap, reconciling the coordinator's `consumed` back into the D1
 * ledger (`reconcileToD1: true`). This is the awaited, enforcing counterpart of
 * the `shadow` caller — the caller drops the per-request D1 counter UPSERT and
 * treats `withinCap` as the request-cap verdict.
 *
 * Thin over {@link meterViaDO}: it only pins `reconcileToD1` true and narrows the
 * return to the one field the serve call-site enforces on, so the sequencing
 * (and the over-serve=0 guarantee) stays in the single tested code path. Throws
 * exactly when meterViaDO throws — the serve caller catches and fails OPEN to the
 * D1 count path, so a DO outage can never break or fail-closed a request.
 */
export async function serveViaDO(
  ns: DOMeterNamespaces,
  params: DOMeterParams,
): Promise<{ withinCap: boolean }> {
  const verdict = await meterViaDO(ns, { ...params, reconcileToD1: true });
  return { withinCap: verdict.withinCap };
}

/** Inputs to the {@link serveGateActive} decision (pure; no env/DO access). */
export interface ServeGateInputs {
  /** The `EDGE_DO_METER` flag value (`"serve"` | `"shadow"` | undefined | …). */
  readonly mode: string | undefined;
  /** True only for a genuinely-counted request (a fan-out sub-request is false). */
  readonly meter: boolean;
  /** Whether the shard DO namespace is bound in this env. */
  readonly hasShardNs: boolean;
  /** Whether the coordinator DO namespace is bound in this env. */
  readonly hasCoordNs: boolean;
  /** This worker's serving region code (`R2_CAS_REGION`), or undefined. */
  readonly region: string | undefined;
  /** The tenant tier's monthly request cap (`Number.MAX_SAFE_INTEGER` = uncapped). */
  readonly cap: number;
}

/**
 * Pure decision: should the edge-local DO be the AUTHORITATIVE request-count
 * meter for this request? True iff EVERY condition holds:
 *   - `EDGE_DO_METER === "serve"` (only `serve` enforces; off/shadow never do);
 *   - `meter === true` (a genuinely-counted request — a fan-out sub-request,
 *     which has `meter=false`, NEVER hits the DO, so it is never double-counted);
 *   - both DO namespaces are bound AND `R2_CAS_REGION` is a non-empty string
 *     (the shard is per tenant+region — no region ⇒ no shard identity);
 *   - the tier is CAPPED (`cap !== MAX_SAFE_INTEGER`) — an uncapped tier
 *     (team/enterprise) can never deny, so there is nothing to serve.
 *
 * Factored out of the call-site so its truth table is unit-testable without env.
 */
export function serveGateActive(inputs: ServeGateInputs): boolean {
  return (
    inputs.mode === "serve" &&
    inputs.meter === true &&
    inputs.hasShardNs &&
    inputs.hasCoordNs &&
    typeof inputs.region === "string" &&
    inputs.region.length > 0 &&
    inputs.cap !== Number.MAX_SAFE_INTEGER
  );
}
