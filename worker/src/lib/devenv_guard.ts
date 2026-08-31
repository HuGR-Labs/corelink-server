// worker/src/lib/devenv_guard.ts
//
// DevEnv per-tenant entitlement guard (WP-08 / WP-07).

import type { Env } from "../index.js";

export interface DevenvQuotaResult {
  readonly allowed: boolean;
  readonly reason?: string;
}

/**
 * The columns this guard reads from `runners_entitlement`.
 *
 * This list is NOT free-form: it is pinned by
 * `worker/tests/devenv_guard.test.ts`, which parses the actual DDL out of
 * `migrations/d1/0070_runners_entitlement.sql` + `0072_..._max_vcpu_h.sql` and
 * fails if any name here is not a real column. Adding a name that no migration
 * creates turns EVERY call into a D1 `no such column` throw — see the header of
 * `checkDevenvQuota` for what that cost.
 */
export const DEVENV_ENTITLEMENT_COLUMNS = ["max_concurrency", "max_vcpu_h"] as const;

interface EntitlementRow {
  max_concurrency: number | null;
  max_vcpu_h: number | null;
}

/**
 * Check whether the given tenant is entitled to launch or use a DevEnv session.
 *
 * ## The predicate is the migration's, not this file's
 *
 * `migrations/d1/0072_runners_entitlement_max_vcpu_h.sql` states the ratified
 * semantics in its own words: an absent `max_concurrency` means no Runners
 * entitlement and is a REJECT, while an absent `max_vcpu_h` is a "wall-off" and
 * proceeds. `0070` adds the other half — `CHECK (max_concurrency > 0)` forbids a
 * zero-valued row, so a cap of zero is expressed by the ABSENCE of a row, which
 * makes the presence of a row with a positive cap the thing that expresses a
 * positive entitlement. This guard transcribes that and nothing more:
 *
 *   - no `tenant_id` (or the anonymous sentinel)   → deny
 *   - `CONFIG_DB` unbound                          → deny
 *   - the D1 read throws (outage, timeout, …)      → deny
 *   - NO `runners_entitlement` row for the tenant  → deny
 *   - row present, `max_concurrency` not > 0       → deny
 *   - row present with a positive `max_concurrency` → ALLOW (absent
 *     `max_vcpu_h` walls off and proceeds, per 0072)
 *
 * ## What this guard does NOT do, stated rather than implied
 *
 * It does not enforce a monthly vCPU ceiling. `max_vcpu_h` is SELECTed and typed
 * on `EntitlementRow`, and that is ALL it does: its value never reaches a
 * predicate, so no value of it — including a negative or zero one — can change
 * the outcome. `max_concurrency` is the only field this guard decides on. That
 * is not an oversight to be closed by adding a `max_vcpu_h > 0` check: 0072
 * ratifies that an absent `max_vcpu_h` walls off and PROCEEDS, and all 8
 * `runners_entitlement` rows in prod carry `max_vcpu_h = NULL`, so the wall-off
 * branch is the only live one and refusing on it would change declared
 * semantics, not enforce them. Enforcement would need a CONSUMPTION side, and
 * there is none: `devenv_monthly_vcpu` (migration 0106) is referenced only by
 * its own migration and the DSR erase set — nothing writes it and nothing reads
 * it — and `customer_runners.rs` reports `consumed_vcpu_h` as a literal `0`
 * marked `[stub]`. A ceiling checked against a table that is never written would
 * either be a no-op or deny everyone; both are worse than saying so here.
 * `docs/campaigns/remediation/devenv-manifest.tsv:170` tracks the real metering
 * guard as prerequisite work.
 *
 * ## Why every non-entitlement is a denial (B-075)
 *
 * DevEnv is the most expensive per-request compute surface in the product, so
 * every way of NOT obtaining a positive entitlement denies, and there is exactly
 * one `allowed: true` exit, reachable only past all of them.
 *
 * Two rounds of history are worth keeping, because the second was caused by
 * fixing the first carelessly:
 *
 *  1. The guard denied only on `install_status === "suspended"`, and both the
 *     no-row path and the `catch` fell through to `allowed: true`.
 *  2. `install_status` IS NOT A COLUMN. It never was. It is synthesised into the
 *     API response by `crates/corelink-container/src/routes/customer_runners.rs:285`,
 *     which hardcodes `"installed"` whenever a row exists, and the published type
 *     union is `"installed" | "not_installed"` — nothing anywhere ever produces
 *     `"suspended"`. Selecting it made D1 throw `no such column: install_status`
 *     on EVERY call, so the empty `catch` was not an edge case: it was the only
 *     path, and the guard authorised 100% of requests unconditionally. Making
 *     that same `catch` deny — without removing the phantom column — would have
 *     converted always-allow into always-deny and taken the whole DevEnv surface
 *     to 403, including the 8 tenants that hold real entitlement rows in prod.
 *
 * That is why `DEVENV_ENTITLEMENT_COLUMNS` is pinned against the migrations by a
 * test: the schema is the contract, and a mock that invents columns cannot check
 * it.
 *
 * The `CONFIG_DB`-unbound denial matches the call site in `worker/src/index.ts`,
 * which already 503s when `RUNNER_DEVENV_DO` is unbound — a missing binding is a
 * service fault, never an authorisation.
 */
export async function checkDevenvQuota(
  env: Env,
  tenantId: string,
): Promise<DevenvQuotaResult> {
  if (!tenantId || tenantId === "_anonymous") {
    return { allowed: false, reason: "tenant_id required" };
  }

  // CONFIG_DB is declared non-optional on `Env` and is bound in EVERY wrangler
  // environment (top-level + prod/staging/prod-sam/prod-lhr/prod-nrt/prod-syd).
  // An absent binding is therefore a misconfiguration, not a supported mode:
  // deny rather than authorise a tenant we cannot check.
  if (!env.CONFIG_DB) {
    return {
      allowed: false,
      reason: "DevEnv entitlement unavailable (CONFIG_DB unbound)",
    };
  }

  let row: EntitlementRow | null;

  try {
    row = await env.CONFIG_DB.prepare(
      `SELECT ${DEVENV_ENTITLEMENT_COLUMNS.join(", ")}
         FROM runners_entitlement
         WHERE tenant_id = ?1`
    ).bind(tenantId).first<EntitlementRow>();
  } catch {
    // Fail CLOSED: an unreadable entitlement is not an entitlement. Authorising
    // through a D1 outage would hand billable DevEnv compute to every tenant for
    // the duration of the outage.
    return {
      allowed: false,
      reason: "DevEnv entitlement check unavailable",
    };
  }

  if (!row) {
    return {
      allowed: false,
      reason: "No DevEnv runners entitlement for tenant",
    };
  }

  // 0070: `CHECK (max_concurrency > 0)` — a zero cap is expressed by the absence
  // of a row, so a row that somehow carries a non-positive or missing cap is not
  // an entitlement either. 0072: an absent `max_vcpu_h` walls off and proceeds,
  // so it is deliberately NOT part of this condition.
  if (typeof row.max_concurrency !== "number" || !(row.max_concurrency > 0)) {
    return {
      allowed: false,
      reason: "DevEnv runners entitlement carries no positive concurrency cap",
    };
  }

  return { allowed: true };
}

export const enforceDevenvQuota = checkDevenvQuota;
