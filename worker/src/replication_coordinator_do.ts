/**
 * ReplicationCoordinatorDO — the PRODUCTION multi-region replication coordinator
 * (WI-MULTI-REGION-V1 prod-wiring).
 *
 * `crates/corelink-replication-coordinator` ships the pure-logic decision tree
 * (region role state machine: which region is Primary, promotion on outage,
 * split-brain rejection, the 24 h hot-standby failback cool-down,
 * audit-emit-BEFORE-mutation fail-CLOSED) plus an `InMemoryReplicationCoordinator`
 * whose "singleton promotion lock" is a per-instance `std::sync::Mutex`. The crate
 * docs say verbatim: "in production this is satisfied by the Cloudflare Durable
 * Object singleton ID (deferred per charter)".
 *
 * This DO IS that production singleton. It ports the Rust decision tree
 * FAITHFULLY (same constants, same ordering, same invariants) and makes the two
 * deferred pieces real:
 *
 *   1. **Singleton promotion lock.** The Worker addresses this DO via a FIXED
 *      name (`idFromName(REPLICATION_COORDINATOR_SINGLETON)`), so exactly ONE
 *      instance exists globally and the DO runtime serializes every fetch/alarm
 *      against it. That single-writer guarantee IS the split-brain-safe lock the
 *      Rust `Mutex` only modelled (INV-FAILOVER-NO-SPLIT-BRAIN).
 *   2. **Durable state.** The role map + heartbeats live in DO storage (not an
 *      in-process HashMap), so a coordinator restart preserves which region is
 *      Primary and each hot-standby's cool-down clock.
 *
 * The scheduled driver is the DO **alarm** (`alarm()` re-arms itself every
 * `REPLICATION_TICK_INTERVAL_MS`): it periodically runs the evaluate→promote loop
 * so a region outage is acted on WITHOUT a human. The alarm self-arms the first
 * time the DO is woken (any register/heartbeat/arm request), so once the topology
 * is registered the driver perpetuates itself.
 *
 * Charter constraints (mirrored from event_log_do.ts / durable_object.ts):
 *   - INV-NO-PII-IN-LOGS: only region CODES + lag seconds are logged — never a
 *     tenant/user id (regions are not PII).
 *   - JSON error envelopes match the REAPI shape used across the Worker.
 *
 * OPERATOR RESIDUAL (not faked here): the HEARTBEAT FEED — each region measuring
 * its real R2/D1/KV replication lag + POSTing `/_internal/replication/heartbeat`
 * — and the R2 Cross-Region-Replication bindings are a Cloudflare-infra step. The
 * DO ingests heartbeats + drives promotion; with no heartbeats the tick is a
 * safe no-op (it NEVER auto-promotes into a partition).
 */

import type {
  DurableObject,
  DurableObjectState,
  DurableObjectStorage,
} from "@cloudflare/workers-types";
import type { Env } from "./index.js";

// ──────────────────────────────────────────────────────────────────────────────
// Constants — ported 1:1 from the Rust coordinator crate
// ──────────────────────────────────────────────────────────────────────────────

/**
 * Fixed DO instance name — the ONE global coordinator. The single-instance
 * guarantee of a name-addressed DO IS the singleton promotion lock.
 */
export const REPLICATION_COORDINATOR_SINGLETON = "replication-coordinator-singleton";

/** 24 h hot-standby cool-down (`state.rs::HOT_STANDBY_COOLDOWN_SECONDS`). */
export const HOT_STANDBY_COOLDOWN_SECONDS = 24 * 60 * 60;
/** Heartbeat staleness threshold in seconds (`heartbeat.rs::HEARTBEAT_STALE_SECONDS`). */
export const HEARTBEAT_STALE_SECONDS = 60;
/** Per-domain replication-lag SLO ceilings (`lag.rs`). Neon is informational. */
export const SLO_REPLICATION_LAG_R2_SECONDS = 60;
export const SLO_REPLICATION_LAG_D1_SECONDS = 60;
export const SLO_REPLICATION_LAG_KV_SECONDS = 60;

/** Deterministic region scan order (`Region::ALL`). */
export const REGIONS = ["wnam", "enam", "weur", "sam"] as const;
export type RegionCode = (typeof REGIONS)[number];

/** Canonical region roles (`state.rs::RegionRole`). */
export type RegionRole = "primary" | "hot_standby" | "replica";

/** Alarm cadence — the scheduled-driver tick period (matches the ~30 s replica
 *  heartbeat cadence). Kept < HEARTBEAT_STALE so a missed heartbeat is caught
 *  within a couple of ticks. */
export const REPLICATION_TICK_INTERVAL_MS = 30_000;

// Storage keys.
const ROLES_KEY = "roles";
const HEARTBEATS_KEY = "heartbeats";

// ──────────────────────────────────────────────────────────────────────────────
// Types
// ──────────────────────────────────────────────────────────────────────────────

export interface LagBundle {
  readonly r2_s: number;
  readonly d1_s: number;
  readonly kv_s: number;
  /** Informational only — NOT part of the promotion-readiness SLO gate. */
  readonly neon_s: number;
}

export interface Heartbeat {
  readonly ts_ms: number;
  readonly lag: LagBundle;
}

export interface RoleState {
  readonly role: RegionRole;
  /** When (ms) the hot-standby cool-down started; null unless hot_standby. */
  readonly cooldown_started_ms: number | null;
}

export type RoleMap = Record<string, RoleState>;
export type HeartbeatMap = Record<string, Heartbeat>;

export type CoordErrorKind =
  | "unknown_region"
  | "no_eligible_replica"
  | "split_brain_rejected"
  | "primary_still_eligible"
  | "cooldown_not_elapsed"
  | "internal";

export interface CoordError {
  readonly kind: CoordErrorKind;
  readonly message: string;
}

export type PromotionDecision =
  | { kind: "keep_primary" }
  | { kind: "promote_replica"; replica: RegionCode }
  | { kind: "no_eligible_replica" };

export type AuditEventType =
  | "corelink.failover.region_promoted.v1"
  | "corelink.failover.region_demoted.v1"
  | "corelink.failover.failback_blocked.v1"
  | "corelink.failover.failback_committed.v1";

export interface AuditRecord {
  readonly event_type: AuditEventType;
  readonly region: string;
  readonly previous_primary: string;
  readonly timestamp_ms: number;
  readonly detail: string;
}

interface RegionStatus {
  readonly region: string;
  readonly role: RegionRole;
  readonly heartbeat_fresh: boolean;
  readonly within_slo: boolean;
  readonly cooldown_started_ms: number | null;
}

export interface ReplicationStatus {
  readonly regions: RegionStatus[];
  readonly healthy: boolean;
  readonly timestamp_ms: number;
}

// ──────────────────────────────────────────────────────────────────────────────
// Pure decision core — ported from coordinator.rs (exported for unit tests)
// ──────────────────────────────────────────────────────────────────────────────

export function isRegionCode(s: string): s is RegionCode {
  return (REGIONS as readonly string[]).includes(s);
}

export function isRegionRole(s: string): s is RegionRole {
  return s === "primary" || s === "hot_standby" || s === "replica";
}

/** `lag.rs::within_slo` — r2/d1/kv within ceiling; neon is informational. */
export function lagWithinSlo(lag: LagBundle): boolean {
  return (
    lag.r2_s <= SLO_REPLICATION_LAG_R2_SECONDS &&
    lag.d1_s <= SLO_REPLICATION_LAG_D1_SECONDS &&
    lag.kv_s <= SLO_REPLICATION_LAG_KV_SECONDS
  );
}

/** `heartbeat.rs::is_fresh` — future heartbeat is stale (clock-skew guard). */
export function heartbeatFresh(hb: Heartbeat, now_ms: number): boolean {
  if (now_ms < hb.ts_ms) return false;
  const age_ms = now_ms - hb.ts_ms;
  return Math.floor(age_ms / 1000) < HEARTBEAT_STALE_SECONDS;
}

/** (fresh, within_slo) for a region, defaulting to (false, false) if unseen. */
export function regionHealth(
  hbs: HeartbeatMap,
  region: string,
  now_ms: number,
): { fresh: boolean; within: boolean } {
  const hb = hbs[region];
  if (hb === undefined) return { fresh: false, within: false };
  return { fresh: heartbeatFresh(hb, now_ms), within: lagWithinSlo(hb.lag) };
}

/** First region (in `REGIONS` order) currently holding `primary`, else null. */
export function primaryIn(roles: RoleMap): RegionCode | null {
  for (const r of REGIONS) {
    const s = roles[r];
    if (s !== undefined && s.role === "primary") return r;
  }
  return null;
}

/** `coordinator.rs::route_write`. */
export function routeWriteCheck(
  roles: RoleMap,
  hbs: HeartbeatMap,
  region: string,
  now_ms: number,
): CoordError | null {
  const st = roles[region];
  if (st === undefined) return { kind: "unknown_region", message: region };
  if (st.role !== "primary") {
    return { kind: "internal", message: `region ${region} role ${st.role} (not primary)` };
  }
  const h = regionHealth(hbs, region, now_ms);
  if (!h.fresh) return { kind: "internal", message: `primary ${region} heartbeat stale` };
  if (!h.within) return { kind: "internal", message: `primary ${region} lag breach` };
  return null;
}

/** `coordinator.rs::evaluate` — no state mutation. */
export function evaluateDecision(
  roles: RoleMap,
  hbs: HeartbeatMap,
  primary: string,
  now_ms: number,
): PromotionDecision | CoordError {
  if (roles[primary] === undefined) return { kind: "unknown_region", message: primary };
  const ph = regionHealth(hbs, primary, now_ms);
  if (ph.fresh && ph.within) return { kind: "keep_primary" };
  for (const replica of REGIONS) {
    if (replica === primary) continue;
    if (roles[replica] === undefined) continue;
    const rh = regionHealth(hbs, replica, now_ms);
    if (rh.fresh && rh.within) return { kind: "promote_replica", replica };
  }
  return { kind: "no_eligible_replica" };
}

export type PromotePlan =
  | { ok: true; audits: AuditRecord[]; roles: RoleMap }
  | { ok: false; error: CoordError };

/** `coordinator.rs::promote` — computes the audits + post-mutation role map. */
export function planPromote(
  roles: RoleMap,
  hbs: HeartbeatMap,
  primary: string,
  replica: string,
  now_ms: number,
): PromotePlan {
  if (primary === replica) {
    return { ok: false, error: { kind: "internal", message: "primary and replica identical" } };
  }
  if (roles[primary] === undefined) return { ok: false, error: { kind: "unknown_region", message: primary } };
  if (roles[replica] === undefined) return { ok: false, error: { kind: "unknown_region", message: replica } };

  // INV-FAILOVER-NO-SPLIT-BRAIN: no region OTHER than `primary` may hold primary.
  // Scan ALL regions, not just the first (`primaryIn`) — a 2nd concurrent primary
  // is split-brain even when `primary` itself is (one of) the current primary(ies).
  for (const r of REGIONS) {
    if (r !== primary && roles[r]?.role === "primary") {
      return {
        ok: false,
        error: { kind: "split_brain_rejected", message: `existing primary ${r}` },
      };
    }
  }

  // Anti-flap: refuse if the old primary is still eligible.
  const ph = regionHealth(hbs, primary, now_ms);
  if (ph.fresh && ph.within) {
    return { ok: false, error: { kind: "primary_still_eligible", message: primary } };
  }

  // Replica must be eligible (fresh + within SLO).
  const rh = regionHealth(hbs, replica, now_ms);
  if (!rh.fresh || !rh.within) {
    return { ok: false, error: { kind: "no_eligible_replica", message: primary } };
  }

  const audits: AuditRecord[] = [
    {
      event_type: "corelink.failover.region_demoted.v1",
      region: primary,
      previous_primary: primary,
      timestamp_ms: now_ms,
      detail: `demoted_to=hot_standby heartbeat_fresh=${ph.fresh} within_slo=${ph.within}`,
    },
    {
      event_type: "corelink.failover.region_promoted.v1",
      region: replica,
      previous_primary: primary,
      timestamp_ms: now_ms,
      detail: "promoted_from=replica",
    },
  ];

  const newRoles: RoleMap = { ...roles };
  newRoles[primary] = { role: "hot_standby", cooldown_started_ms: now_ms };
  newRoles[replica] = { role: "primary", cooldown_started_ms: null };
  return { ok: true, audits, roles: newRoles };
}

export type FailbackPlan =
  | { ok: true; audits: AuditRecord[]; roles: RoleMap }
  | { ok: false; error: CoordError; blockedAudit?: AuditRecord };

/** `coordinator.rs::failback` — 24 h cool-down gate (no heartbeat gate). */
export function planFailback(roles: RoleMap, region: string, now_ms: number): FailbackPlan {
  const st = roles[region];
  if (st === undefined) return { ok: false, error: { kind: "unknown_region", message: region } };
  if (st.role !== "hot_standby") {
    return { ok: false, error: { kind: "internal", message: `region ${region} not hot_standby` } };
  }
  if (st.cooldown_started_ms === null) {
    return { ok: false, error: { kind: "internal", message: `region ${region} has no cooldown clock` } };
  }
  const elapsed_ms = Math.max(0, now_ms - st.cooldown_started_ms);
  const elapsed_s = Math.floor(elapsed_ms / 1000);
  if (elapsed_s < HOT_STANDBY_COOLDOWN_SECONDS) {
    const remaining = HOT_STANDBY_COOLDOWN_SECONDS - elapsed_s;
    return {
      ok: false,
      error: {
        kind: "cooldown_not_elapsed",
        message: `elapsed_s=${elapsed_s} remaining_s=${remaining}`,
      },
      blockedAudit: {
        event_type: "corelink.failover.failback_blocked.v1",
        region,
        previous_primary: region,
        timestamp_ms: now_ms,
        detail: `cooldown_elapsed_s=${elapsed_s} cooldown_remaining_s=${remaining}`,
      },
    };
  }

  const current = primaryIn(roles);
  const audits: AuditRecord[] = [
    {
      event_type: "corelink.failover.failback_committed.v1",
      region,
      previous_primary: current ?? "",
      timestamp_ms: now_ms,
      detail: `cooldown_elapsed_s=${elapsed_s} (>= ${HOT_STANDBY_COOLDOWN_SECONDS})`,
    },
  ];
  const newRoles: RoleMap = { ...roles };
  if (current !== null) {
    newRoles[current] = { role: "replica", cooldown_started_ms: null };
  }
  newRoles[region] = { role: "primary", cooldown_started_ms: null };
  return { ok: true, audits, roles: newRoles };
}

/** `coordinator.rs::replication_status`. */
export function buildStatus(roles: RoleMap, hbs: HeartbeatMap, now_ms: number): ReplicationStatus {
  const regions: RegionStatus[] = [];
  let primaryCount = 0;
  let anyUnhealthyPrimary = false;
  for (const region of REGIONS) {
    const st = roles[region];
    if (st === undefined) continue;
    const h = regionHealth(hbs, region, now_ms);
    if (st.role === "primary") {
      primaryCount += 1;
      if (!h.fresh || !h.within) anyUnhealthyPrimary = true;
    }
    regions.push({
      region,
      role: st.role,
      heartbeat_fresh: h.fresh,
      within_slo: h.within,
      cooldown_started_ms: st.cooldown_started_ms,
    });
  }
  return { regions, healthy: primaryCount === 1 && !anyUnhealthyPrimary, timestamp_ms: now_ms };
}

// ──────────────────────────────────────────────────────────────────────────────
// HTTP helpers
// ──────────────────────────────────────────────────────────────────────────────

function coordErrorStatus(kind: CoordErrorKind): number {
  switch (kind) {
    case "unknown_region":
      return 404;
    case "split_brain_rejected":
    case "primary_still_eligible":
    case "no_eligible_replica":
    case "cooldown_not_elapsed":
      return 409;
    case "internal":
      return 500;
    default:
      return 500;
  }
}

function jsonResponse(body: unknown, status: number, requestId: string): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json", "X-Request-Id": requestId },
  });
}

function errorResponse(error: string, message: string, status: number, requestId: string): Response {
  return jsonResponse({ error, message, request_id: requestId }, status, requestId);
}

// ──────────────────────────────────────────────────────────────────────────────
// Durable Object
// ──────────────────────────────────────────────────────────────────────────────

export class ReplicationCoordinatorDO implements DurableObject {
  private readonly state: DurableObjectState;
  private readonly storage: DurableObjectStorage;
  private readonly env: Env;

  private roles: RoleMap = {};
  private heartbeats: HeartbeatMap = {};
  private readonly ready: Promise<void>;

  constructor(state: DurableObjectState, env: Env) {
    this.state = state;
    this.storage = state.storage;
    this.env = env;
    this.ready = this.state.blockConcurrencyWhile(async () => {
      const storedRoles = await this.storage.get<RoleMap>(ROLES_KEY);
      if (storedRoles !== undefined) this.roles = storedRoles;
      const storedHbs = await this.storage.get<HeartbeatMap>(HEARTBEATS_KEY);
      if (storedHbs !== undefined) this.heartbeats = storedHbs;
      // Self-arm the scheduled driver on wake so the evaluate→promote loop
      // perpetuates once the DO has ever been touched.
      await this.armAlarm();
    });
  }

  // Emit an audit record BEFORE mutating state (fail-CLOSED: a throw here aborts
  // the mutation). Overridable in tests to exercise the fail-closed path.
  // INV-NO-PII-IN-LOGS: only region codes + lag numbers cross this boundary.
  protected emitAudit(rec: AuditRecord): void {
    console.log(JSON.stringify({ kind: "replication_audit", ...rec }));
  }

  private async persistRoles(roles: RoleMap): Promise<void> {
    this.roles = roles;
    await this.storage.put(ROLES_KEY, roles);
  }

  private async persistHeartbeats(hbs: HeartbeatMap): Promise<void> {
    this.heartbeats = hbs;
    await this.storage.put(HEARTBEATS_KEY, hbs);
  }

  private async armAlarm(): Promise<void> {
    const existing = await this.storage.getAlarm();
    if (existing === null) {
      await this.storage.setAlarm(Date.now() + REPLICATION_TICK_INTERVAL_MS);
    }
  }

  /** DO alarm — the SCHEDULED DRIVER. Runs the tick, then re-arms itself. */
  async alarm(): Promise<void> {
    await this.ready;
    try {
      await this.runTick(Date.now());
    } finally {
      // Always re-arm so the driver keeps running even if a tick throws.
      await this.storage.setAlarm(Date.now() + REPLICATION_TICK_INTERVAL_MS);
    }
  }

  /** evaluate(current primary) → promote if a replica is eligible. */
  async runTick(now_ms: number): Promise<PromotionDecision | null> {
    const primary = primaryIn(this.roles);
    if (primary === null) {
      // No primary registered yet — nothing to drive (safe no-op).
      return null;
    }
    const decision = evaluateDecision(this.roles, this.heartbeats, primary, now_ms);
    if ("kind" in decision && decision.kind === "promote_replica") {
      const plan = planPromote(this.roles, this.heartbeats, primary, decision.replica, now_ms);
      if (plan.ok) {
        for (const rec of plan.audits) this.emitAudit(rec); // BEFORE mutation
        await this.persistRoles(plan.roles);
      }
      return decision;
    }
    if ("kind" in decision && decision.kind === "no_eligible_replica") {
      // Escalate to runbook — do NOT auto-promote into a partition.
      console.warn(
        JSON.stringify({ kind: "replication_no_eligible_replica", primary, now_ms }),
      );
      return decision;
    }
    return decision as PromotionDecision;
  }

  private async applyPromote(primary: string, replica: string, now_ms: number, requestId: string): Promise<Response> {
    const plan = planPromote(this.roles, this.heartbeats, primary, replica, now_ms);
    if (!plan.ok) {
      return errorResponse(plan.error.kind, plan.error.message, coordErrorStatus(plan.error.kind), requestId);
    }
    for (const rec of plan.audits) this.emitAudit(rec); // audit-BEFORE-mutation
    await this.persistRoles(plan.roles);
    return jsonResponse({ ok: true, roles: plan.roles }, 200, requestId);
  }

  private async applyFailback(region: string, now_ms: number, requestId: string): Promise<Response> {
    const plan = planFailback(this.roles, region, now_ms);
    if (!plan.ok) {
      if (plan.blockedAudit) this.emitAudit(plan.blockedAudit);
      return errorResponse(plan.error.kind, plan.error.message, coordErrorStatus(plan.error.kind), requestId);
    }
    for (const rec of plan.audits) this.emitAudit(rec); // audit-BEFORE-mutation
    await this.persistRoles(plan.roles);
    return jsonResponse({ ok: true, roles: plan.roles }, 200, requestId);
  }

  async fetch(request: Request): Promise<Response> {
    await this.ready;
    const requestId = request.headers.get("x-request-id") ?? crypto.randomUUID();
    const url = new URL(request.url);
    const path = url.pathname;

    if (path === "/_do/health") {
      return jsonResponse({ status: "ok", class: "ReplicationCoordinatorDO", request_id: requestId }, 200, requestId);
    }

    const now_ms = Date.now();

    if (request.method === "POST" && path === "/_repl/arm") {
      await this.armAlarm();
      return jsonResponse({ ok: true, armed: true }, 200, requestId);
    }

    if (request.method === "GET" && path === "/_repl/status") {
      return jsonResponse(buildStatus(this.roles, this.heartbeats, now_ms), 200, requestId);
    }

    if (request.method === "POST" && path === "/_repl/tick") {
      const decision = await this.runTick(now_ms);
      return jsonResponse(
        { decision, status: buildStatus(this.roles, this.heartbeats, now_ms) },
        200,
        requestId,
      );
    }

    if (request.method === "POST" && path === "/_repl/register") {
      const body = await this.parseJson(request);
      if (body === null) return errorResponse("INVALID_BODY", "body must be JSON", 400, requestId);
      const region = String((body as Record<string, unknown>).region ?? "");
      const role = String((body as Record<string, unknown>).role ?? "");
      if (!isRegionCode(region)) return errorResponse("INVALID_REGION", `unknown region ${region}`, 400, requestId);
      if (!isRegionRole(role)) return errorResponse("INVALID_ROLE", `unknown role ${role}`, 400, requestId);
      // Split-brain guard at registration time (mirrors coordinator.rs::register).
      if (role === "primary") {
        const existing = primaryIn(this.roles);
        if (existing !== null && existing !== region) {
          return errorResponse("split_brain_rejected", `existing primary ${existing}`, 409, requestId);
        }
      }
      const next: RoleMap = { ...this.roles, [region]: { role, cooldown_started_ms: null } };
      await this.persistRoles(next);
      await this.armAlarm();
      return jsonResponse({ ok: true, region, role }, 200, requestId);
    }

    if (request.method === "POST" && path === "/_repl/heartbeat") {
      const body = await this.parseJson(request);
      if (body === null) return errorResponse("INVALID_BODY", "body must be JSON", 400, requestId);
      const b = body as Record<string, unknown>;
      const region = String(b.region ?? "");
      if (!isRegionCode(region)) return errorResponse("INVALID_REGION", `unknown region ${region}`, 400, requestId);
      const ts_ms = Number(b.ts_ms ?? now_ms);
      const lagRaw = (b.lag ?? {}) as Record<string, unknown>;
      const lag: LagBundle = {
        r2_s: Number(lagRaw.r2_s ?? 0),
        d1_s: Number(lagRaw.d1_s ?? 0),
        kv_s: Number(lagRaw.kv_s ?? 0),
        neon_s: Number(lagRaw.neon_s ?? 0),
      };
      if (!Number.isFinite(ts_ms) || !Number.isFinite(lag.r2_s) || !Number.isFinite(lag.d1_s) || !Number.isFinite(lag.kv_s)) {
        return errorResponse("INVALID_HEARTBEAT", "ts_ms/lag must be finite numbers", 400, requestId);
      }
      const next: HeartbeatMap = { ...this.heartbeats, [region]: { ts_ms, lag } };
      await this.persistHeartbeats(next);
      await this.armAlarm();
      return jsonResponse({ ok: true, region }, 200, requestId);
    }

    if (request.method === "POST" && path === "/_repl/promote") {
      const body = await this.parseJson(request);
      if (body === null) return errorResponse("INVALID_BODY", "body must be JSON", 400, requestId);
      const b = body as Record<string, unknown>;
      const primary = String(b.primary ?? "");
      const replica = String(b.replica ?? "");
      return this.applyPromote(primary, replica, now_ms, requestId);
    }

    if (request.method === "POST" && path === "/_repl/failback") {
      const body = await this.parseJson(request);
      if (body === null) return errorResponse("INVALID_BODY", "body must be JSON", 400, requestId);
      const region = String((body as Record<string, unknown>).region ?? "");
      return this.applyFailback(region, now_ms, requestId);
    }

    return errorResponse("NOT_FOUND", "unknown replication route", 404, requestId);
  }

  private async parseJson(request: Request): Promise<unknown | null> {
    const raw = await request.text();
    if (raw.length === 0) return {};
    try {
      return JSON.parse(raw);
    } catch {
      return null;
    }
  }
}
