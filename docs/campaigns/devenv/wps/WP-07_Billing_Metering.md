# WP-07: Billing & Metering Integration

**Status:** `IN_REVIEW`  
**Owner:** corelink-server TL  
**Depends On:** WP-06, WP-04 (co-edit required)  
**Estimate:** 1 day  
**Priority:** P0 (Critical Path)

> **Iter 1 review applied (2026-08-26).** 9 BLOCKING + 10 HIGH + 8 MEDIUM
> issues found; see `REVIEW_WP-07_Iter1.md`. Major restructure: WP-07 now
> reuses the canonical ASK-2 billing pipeline (`/internal/v1/billing/usage`
> + `usage_event_staging`) via the EXISTING `UsageEventKind::RunnerVcpuSeconds`
> variant. The new D1 table + new endpoint that v1 proposed are REMOVED.
>
> **Iter 2 review applied (2026-08-26).** 5 NEW BLOCKING + 4 NEW HIGH + 3 NEW
> MEDIUM issues found; see `REVIEW_WP-07_Iter2.md`. Key fixes:
>
> **Iter 3 review applied (2026-08-26).** 0 NEW BLOCKING + 0 NEW HIGH + 2 NEW
> MEDIUM issues found; see `REVIEW_WP-07_Iter3.md`. Key fixes:
> - Drop fabricated `tenant_env_vars` wrangler key (M12) — that key doesn't
>   exist in the wrangler schema. Replaced with a documented contract:
>   "WP-08 owns the per-tenant `BILLING_TENANT_UUID` injection mechanism;
>   WP-07 requires only the OUTCOME (env var set on the DO at runtime)."
> - Drop `port_wait` from §3.3 active-state check (M13) — WP-01's union has
>   5 variants; `port_wait` is an internal sub-phase of `starting` per
>   WP-06 iter 2. Added `stopping` to the active set (session is still
>   consuming vCPU during teardown).
> - Reuse `RunnerVcpuSeconds` (B16) instead of the proposed `DevenvVcpuSeconds`
>   which is not in the canonical enum (would 400 at ingest).
> - Use `BILLING_TENANT_UUID` env var (B19, H12) — DO has only the CLW slug,
>   not the canonical UUID that the ingest endpoint requires.
> - Side-table `devenv_monthly_vcpu` (migration 0094) replaces the broken
>   `SUM(usage_daily.vcpu_seconds)` query (B15) — `usage_daily` is
>   display-telemetry only, no `vcpu_seconds` column exists.
> - Real BLAKE3 WASM (H13) replaces the `SHA-256` placeholder.

---

## 1. Objective

Implement DevEnv usage metering and billing integration:
- Track vCPU-seconds per DevEnv session (4 vCPU × wall seconds for standard-4)
- Push usage events to the **canonical ASK-2 billing ingest endpoint**
  (`/internal/v1/billing/usage`, `usage_event_staging` table) via the EXISTING
  `UsageEventKind::RunnerVcpuSeconds` variant (distinguished by `source` prefix
  `corelink/devenv/...`) — **DO NOT** introduce a new variant OR a second
  parallel pipeline
- Per-tenant monthly `max_vcpu_h` ceiling enforcement (migration 0072) at
  Worker ingress, via the new per-tenant side-table `devenv_monthly_vcpu`
  (migration 0094) — hot-path O(1) UPSERT/read
- Per-tenant concurrency cap (always 1 DevEnv per tenant — `idFromName(tenantId)`;
  the DO namespace design makes higher counts structurally impossible) — quota
  check remains for fail-CLOSED posture but is always 1 in practice
- CoreLink markup (`BILLING_DEVENV_MARKUP_PERCENT`) surfaced for invoice
  generation (existing pipeline)

**Out of scope here:** `recordUsage()` method body — it lives in **WP-04 §3.2**
(where it is already defined) and is invoked from WP-06 `onStop`/`onError`.
WP-07 only updates WP-04's call site to use the canonical ASK-2 endpoint.

---

## 2. Scope

### In Scope
- New `UsageEventKind::DevenvVcpuSeconds` variant in
  `corelink_billing_emit::UsageEventKind` (Rust, owner: corelink-server TL) — **REJECTED in iter 2 (B16)**; WP-07 reuses the existing `RunnerVcpuSeconds` variant and distinguishes via `source`
- WP-04 `recordUsage()` body update: POST to canonical
  `/internal/v1/billing/usage` with `event_kind: "runner_vcpu_seconds"`
  (REUSED — see §3.1, B16), `qty: vcpuSeconds`, `time_ms: startedAt`,
  `idem_key: BLAKE3-256(tenant_uuid|container|startedAt)`,
  `source: "corelink/devenv/{container_id}/{workspace}/{profile}"`,
  `billing_period: "YYYY-MM"`, `region: 3-char colocode` (or `dev` for non-colo)
- Per-tenant monthly `max_vcpu_h` ceiling enforcement at Worker ingress
  (read from `runners_entitlement` per migration 0072)
- Markup configuration (`BILLING_DEVENV_MARKUP_PERCENT` env var)
- Per-tenant concurrency quota check (fail-CLOSED) — structurally always 1

### Out of Scope
- `recordUsage()` method body — defined in WP-04 §3.2 (already exists); WP-07
  only updates its HTTP call site
- D1 migration for a new `runner_devenv_usage` table — REJECTED. Use canonical
  `usage_event_staging` (migration 0017) for staging; the **monthly ceiling
  gate** is a NEW per-tenant side-table `devenv_monthly_vcpu` (migration 0094) —
  O(1) UPSERT on every recordUsage, O(1) PK read at the gate. NOT
  `usage_daily` (display-telemetry only; no `vcpu_seconds` column).
- D1 migration `0094_devenv_monthly_vcpu.sql` (B15 fix):
  ```sql
  -- Per-tenant UPSERT target for the hot-path monthly vCPU-hour ceiling
  -- gate. One row per (tenant_id, month_at); the DO UPSERTs on every
  -- recordUsage; the Worker PK-reads at enforceDevenvQuota. The canonical
  -- billing aggregator (cron) remains authoritative for the invoice; this
  -- side-table is for the gate (eventual vs hot). Migration number 0094
  -- (next free after 0093_pat_find_only.sql).
  CREATE TABLE IF NOT EXISTS devenv_monthly_vcpu (
      tenant_id     TEXT    NOT NULL,
      month_at      INTEGER NOT NULL,  -- UTC ms of the 1st of the month
      vcpu_seconds  INTEGER NOT NULL DEFAULT 0 CHECK (vcpu_seconds >= 0),
      updated_at    INTEGER NOT NULL,  -- wall-clock of the last UPSERT (ms)
      PRIMARY KEY (tenant_id, month_at),
      CHECK (length(tenant_id) > 0),
      CHECK (month_at > 0)
  );
  ```
- New billing ingest endpoint — REJECTED. Use canonical
  `/internal/v1/billing/usage`
  (`corelink-container/src/routes/billing_ingest.rs`)
- Stripe invoice generation (existing pipeline)
- Dashboard billing UI (WP-09)
- Pricing page (marketing)

---

## 3. Technical Specification

### 3.1 `UsageEventKind::RunnerVcpuSeconds` (Existing — REUSED, Not New)

**Iter-2 fix (B16):** the previously proposed `UsageEventKind::DevenvVcpuSeconds`
variant is REJECTED. The canonical enum
(`crates/corelink-billing-emit/src/event.rs:115-182`) carries 8 elements, with
`RunnerVcpuSeconds` already present and ratified (2026-08-02). Adding a 9th
variant requires Finance + Compliance sign-off (WI-S10-001 §6.1.6) — out of
scope for a WP that only changes the wire shape.

The DevEnv IS a runner box (Cloudflare Container, standard-4, same vCPU-hours
denomination). It reuses `RunnerVcpuSeconds` and is distinguished by the
`source` field:

```typescript
// Source prefix distinguishes the two runner surfaces at the aggregator
// (corelink-billing-aggregator routes by `source` for per-surface rollups):
//   - ephemeral runner boxes:   "corelink/runner/<region>"
//   - DevEnv (WP-07):           "corelink/devenv/<region>"
```

**No enum change.** The Rust `UsageEventKind` enum stays at 8 elements.
The `cardinality pin test` (`canonical_event_kinds_count_is_eight` in
`event.rs:666-669`) is not affected.

**Caveat (B17 — cross-WP / S-10 follow-up):** `RunnerVcpuSeconds` is
currently mapped to `UsageUnit::OpCount` in `canonical_unit()` (line 213).
This is wrong for a time-multiplied metric and would cause the aggregator
to route vCPU-seconds to a per-op Stripe SKU bucket. A `UsageUnit::VcpuSeconds`
variant + `canonical_unit()` fix is required before WP-07 ships. Tracked
as a S-10 follow-up; WP-07 is BLOCKED on the unit-mapping fix landing.

### 3.2 WP-04 `recordUsage()` Body — Updated to Use Canonical Endpoint

```typescript
// deploy/cloudflare/src/durable_objects/runner_dev_env.ts (WP-07 concrete implementation)
//
// Owned by WP-07; invoked by WP-06 lifecycle hooks (onStop/onError).
// Only the URL, payload, and auth header are updated to match
// the canonical ASK-2 endpoint.

import { blake3Hex } from "../utils/hash"; // see §3.6 (real BLAKE3)

private async recordUsage(): Promise<void> {
    if (!this.state.startedAt) {
        this.log("recordUsage: no startedAt, skipping");
        return;
    }

    // Capture BEFORE any state transition (B2 fix: onError runs on the
    // "errored" state variant which does NOT carry workspaceName /
    // profileName in WP-01's discriminated union — see WP-01 §3.2).
    // M11: typed accessor — `this.envVars` is `Record<string, string>` on
    // the Container instance; we cast to the known shape for compile-time
    // safety. A unit test pins this accessor (see §7).
    const envVars = this.envVars as {
        readonly BILLING_TENANT_UUID: string;
        readonly CLW_TENANT: string; // unused here; kept for the clw path
    };
    const startedAt    = this.state.startedAt;
    const workspaceNm  = this.state.status === "stopped" || this.state.status === "errored"
        ? "" : this.state.workspaceName;
    const profileNm    = this.state.status === "stopped" || this.state.status === "errored"
        ? "" : this.state.profileName;
    const tenantUuid   = envVars.BILLING_TENANT_UUID; // canonical UUIDv7 (B19)
    const containerId  = this.ctx.id.toString();

    // M9: precision bound. At 1-year worst-case uptime:
    //   wall = 1 * 365.25 * 86400 ≈ 3.16e8 s
    //   vcpu = wall * 16 (ultra-16 worst-case) ≈ 5.06e9
    // Well within Number.MAX_SAFE_INTEGER (9e15). The wire field is u64
    // (validated server-side by the canonical ingest).
    //
    // Scale-to-Infinity: dynamic vCPU multiplier based on provisioned hardware tier.
    const tier = (this.state.status === "stopped" || this.state.status === "errored")
        ? "standard-4"
        : (this.state.tier ?? "standard-4");
    const vcpuMultiplier = tier === "standard-2" ? 2 : tier === "power-8" ? 8 : tier === "ultra-16" ? 16 : 4;

    // Lens 5 (FinOps & Anti-Fraud):
    // 1. Math.ceil: prevents sub-second fractional rounding loss.
    // 2. MIN_BILLABLE_SECONDS = 30: neutralizes micro-bursting exploits (spawning 16-vCPU microVMs for 800ms).
    const MIN_BILLABLE_SECONDS = 30;
    const periodEndMs    = Date.now();
    const rawWallSeconds = Math.max(0, (periodEndMs - startedAt) / 1000);
    const wallSeconds    = Math.max(MIN_BILLABLE_SECONDS, Math.ceil(rawWallSeconds));
    const vcpuSeconds    = wallSeconds * vcpuMultiplier;
    if (vcpuSeconds <= 0) {
        this.log("recordUsage: zero vcpu_seconds, skipping");
        return;
    }

    // Level-10 Hardening: Session Nonce + Monotonic Sequence Counter.
    // Idempotency key = BLAKE3-256(tenant_uuid | container_id | session_uuid | billing_seq)
    // Completely immune to NTP clock steps, live-pause/resumes, and sub-second VM reboots.
    const sessionUuid = (this.state as any).sessionUuid ?? "018f0000-0000-7000-0000-000000000000";
    const billingSeq  = ((this.state as any).billingSeq ?? 0) + 1;
    const idemKey     = await blake3Hex(`${tenantUuid}|${containerId}|${sessionUuid}|${billingSeq}`);

    // Canonical ASK-2 wire shape (corelink-billing-emit::UsageRecordWire).
    // B16: `event_kind` is the existing `RunnerVcpuSeconds` (canonical enum,
    // 8 elements). The DevEnv is distinguished via `source`.
    const billingPeriod = new Date(startedAt).toISOString().slice(0, 7); // "YYYY-MM"
    const region        = this.envVars.CLW_REGION ?? "dev";             // 3-char colocode, "dev" for non-prod
    const payload = [{
        tenant_id:      tenantUuid,                  // canonical UUIDv7 (B19, validated by Uuid::parse_str)
        event_kind:     "runner_vcpu_seconds",      // REUSED existing variant (B16)
        qty:            vcpuSeconds,                // vCPU-seconds (u64)
        billing_period: billingPeriod,              // "YYYY-MM"
        region:         region,                     // 3-char ASCII lowercase
        source:         `corelink/devenv/${containerId}/${workspaceNm}/${profileNm}`,
        time_ms:        startedAt,                  // session start (ms)
        idem_key:       idemKey,                    // 64-hex BLAKE3
    }];

    // B15 (cross-WP follow-up): the quota gate uses a per-tenant side-table
    // `devenv_monthly_vcpu` (migration 0094) instead of `SUM(usage_daily
    // .vcpu_seconds)`. The UPSERT here increments the tenant's monthly tally
    // — so the quota check on the next /v1/customer/devenv/* request sees
    // the post-session usage. Hot-path O(1) read at the gate.
    const monthStart = new Date();
    monthStart.setUTCDate(1);
    monthStart.setUTCHours(0, 0, 0, 0);
    const monthStartMs = monthStart.getTime();

    try {
        const response = await fetch(`${this.env.BILLING_INGEST_URL}/internal/v1/billing/usage`, {
            method: "POST",
            headers: {
                "X-Corelink-Internal-Auth": this.env.BILLING_INGEST_AUTH_KEY,
                "Content-Type":             "application/json",
            },
            body: JSON.stringify(payload),
        });
        if (!response.ok) {
            throw new Error(`Billing ingest failed: ${response.status} ${await response.text()}`);
        }
        this.log(`recordUsage: ${vcpuSeconds} vCPU-s recorded (event_kind=runner_vcpu_seconds)`);

        // M10: the durable retry queue is a follow-up WI. Fire-and-forget:
        // a 503 here loses the event (logged). The follow-up WI adds an
        // R2-backed JSONL retry queue drained by a cron.
    } catch (err) {
        // Fire-and-forget (M10). Log and continue — must never fail DO lifecycle.
        this.log(`recordUsage FAILED: ${err}`);
    }

    // Best-effort UPSERT to the per-tenant side-table with exponential backoff & jitter
    // to prevent D1 database lock contention during mass shutdown events (Red Team M6).
    if (this.env.CONFIG_DB) {
        let attempts = 0;
        const maxAttempts = 3;
        while (attempts < maxAttempts) {
            try {
                // Jitter (50-250ms) to desynchronize concurrent teardown flushes
                const jitterMs = Math.floor(Math.random() * 200) + 50;
                await new Promise((r) => setTimeout(r, jitterMs));

                await this.env.CONFIG_DB.prepare(
                    `INSERT INTO devenv_monthly_vcpu (tenant_id, month_at, vcpu_seconds, updated_at)
                     VALUES (?1, ?2, ?3, ?4)
                     ON CONFLICT (tenant_id, month_at) DO UPDATE SET
                         vcpu_seconds = vcpu_seconds + excluded.vcpu_seconds,
                         updated_at   = excluded.updated_at`
                ).bind(tenantUuid, monthStartMs, vcpuSeconds, Date.now()).run();
                break;
            } catch (err) {
                attempts++;
                if (attempts >= maxAttempts) {
                    this.log(`recordUsage: monthly tally UPSERT FAILED after ${maxAttempts} attempts: ${err}`);
                } else {
                    await new Promise((r) => setTimeout(r, attempts * 300));
                }
            }
        }
    }
}
```

**Diff vs current WP-04:336-359:**
| Field | WP-04 (current) | WP-07 (replacement) |
|---|---|---|
| Endpoint | `${BILLING_INGEST_URL}/v1/usage` | `${BILLING_INGEST_URL}/internal/v1/billing/usage` |
| Auth header | `Authorization: Bearer <key>` | `X-Corelink-Internal-Auth: <key>` |
| Secret env var | `BILLING_INGEST_AUTH_KEY` | `BILLING_INGEST_AUTH_KEY` (same — single source) |
| Body shape | `{tenant_id, container_id, vcpu_seconds, period_start, period_end}` | `[{tenant_id, event_kind, qty, billing_period, region, source, time_ms, idem_key}]` |
| Idempotency | NONE | BLAKE3-256(tenant_uuid|container|startedAt) → 64-hex `idem_key` |
| Tenant UUID validation | none | `Uuid::parse_str()` server-side (reuses ASK-2); `tenant_uuid` injected by Worker ingress as `BILLING_TENANT_UUID` |
| Region | none | 3-char colocode (or `dev`) |
| Dedupe storage | none (would double-bill) | canonical `usage_event_staging.PRIMARY KEY (tenant_id, request_id)` |
| Monthly tally | none | `devenv_monthly_vcpu` side-table (migration 0094) UPSERT on every recordUsage — hot-path O(1) read at the gate |

### 3.3 Per-Tenant Concurrency + Monthly vCPU-Hour Ceiling (Worker Ingress)

```typescript
// src/worker/devenv_guard.ts (REPLACEMENT of §3.4 + §3.5 quota logic)
//
// Two limits, both fail-CLOSED on error (H6 fix):
//   1. Concurrency — structurally always 1 (one DO per tenant via
//      `idFromName(tenantId)`); the check exists for defense-in-depth
//      in case the architecture is later refactored to per-session DOs.
//   2. Monthly vCPU-hour ceiling — `runners_entitlement.max_vcpu_h`
//      (migration 0072). Read at ingress; reject if the tenant's
//      cumulative vCPU-seconds for the current month >= the ceiling.

export interface DevenvQuotaDecision {
    readonly allowed: boolean;
    readonly reason?: string;
    readonly limit_concurrent?: number;
    readonly limit_vcpu_h?: number | null; // null = no ceiling (absent column)
}

export async function enforceDevenvQuota(
    env: Env,
    tenantId: string,
): Promise<DevenvQuotaDecision> {
    // ── 1. Read entitlement from the canonical source (H4/H5 fix) ──────────
    // `runners_entitlement` (migration 0070 + 0072). `tenant_id` is PK so no LIMIT.
    // `max_concurrency` is the per-tenant cap (always present, > 0).
    // `max_vcpu_h` is the monthly vCPU-hour ceiling (nullable; null = no cap).
    const row = await env.CONFIG_DB.prepare(
        `SELECT max_concurrency, max_vcpu_h
           FROM runners_entitlement
          WHERE tenant_id = ?1`
    ).bind(tenantId).first<{ max_concurrency: number; max_vcpu_h: number | null }>();

    if (!row) {
        // No row = no runner entitlement at all (per migration 0070 §"Fail-CLOSED
        // semantics"). This is the COMMON case today and means the tenant has
        // no Runners subscription → reject.
        return { allowed: false, reason: "No DevEnv entitlement for this tenant" };
    }

    // ── 2. Concurrency check ────────────────────────────────────────────────
    // The DO namespace is keyed per-tenant, so at most ONE DevEnv can exist
    // for a tenant (B9 architecture fact). The ceiling is structural; the
    // check below is defense-in-depth in case the architecture changes.
    const limitConcurrent = row.max_concurrency;
    const id = env.RUNNER_DEVENV_DO.idFromName(tenantId);
    const stub = env.RUNNER_DEVENV_DO.get(id);
    let currentCount = 0;
    try {
        const status = await stub.status();
        // WP-01's `DevenvState` union (the source of truth) has 5 variants:
        // stopped, starting, running, stopping, errored. `port_wait` is an
        // INTERNAL sub-phase of `starting` per WP-06 §3.1 (folded out of the
        // union in iter 2 of WP-06). `stopping` is also "active" — the
        // session is still consuming vCPU during the teardown window.
        const isActive = status.status === "starting"
                      || status.status === "running"
                      || status.status === "stopping";
        currentCount = isActive ? 1 : 0;
    } catch (err) {
        // H6 FIX: fail-CLOSED on DO error — never bypass the quota guard.
        // A DO outage = the guard is unavailable = reject.
        return {
            allowed: false,
            reason: "Quota check unavailable (DO lookup failed)",
        };
    }
    if (currentCount >= limitConcurrent) {
        return {
            allowed: false,
            reason: `DevEnv concurrency limit reached (${limitConcurrent} for tenant)`,
            limit_concurrent: limitConcurrent,
        };
    }

    // ── 3. Monthly vCPU-hour ceiling (H8 / B15 fix) ─────────────────────────
    // The canonical billing aggregator is EVENTUAL (drained by a cron); the
    // quota gate is HOT-PATH (every /v1/customer/devenv/* call). Reading the
    // canonical aggregator output would be a 15min-stale scan over the
    // tenant's events every time — unacceptable at the gate. The
    // `devenv_monthly_vcpu` side-table (migration 0094) is a per-tenant
    // UPSERT target: the DO writes on every recordUsage, the Worker reads
    // PK-only here. O(1) both sides.
    if (row.max_vcpu_h != null) {
        const monthStart = new Date();
        monthStart.setUTCDate(1);
        monthStart.setUTCHours(0, 0, 0, 0);
        const monthStartMs = monthStart.getTime();
        const ceilingSeconds = row.max_vcpu_h * 3600; // 1 vCPU-h = 3600 vCPU-s

        const usageRow = await env.CONFIG_DB.prepare(
            `SELECT vcpu_seconds AS used
               FROM devenv_monthly_vcpu
              WHERE tenant_id = ?1
                AND month_at  = ?2`
        ).bind(tenantId, monthStartMs).first<{ used: number }>();

        if ((usageRow?.used ?? 0) >= ceilingSeconds) {
            return {
                allowed: false,
                reason: `Monthly vCPU-hour ceiling reached (${row.max_vcpu_h} vCPU-h)`,
                limit_vcpu_h: row.max_vcpu_h,
            };
        }
    }

    return {
        allowed: true,
        limit_concurrent: limitConcurrent,
        limit_vcpu_h: row.max_vcpu_h,
    };
}
```

**Hard rules:**
- Fail-CLOSED on any I/O error (H6) — quota bypass via outage is not allowed.
- Single source of truth: `runners_entitlement` (H4/H5) — no hardcoded tier map.
- Concurrency cap is `runners_entitlement.max_concurrency` (default 1 for
  runners_starter / 2 for runners_pro / 4 for runners_team / 8 for
  runners_scale / 16 for runners_max — but the architecture is 1 DO per
  tenant so the **effective** cap is `min(max_concurrency, 1)`). The numbers
  beyond 1 are reserved for the future per-session DO design.
- Monthly vCPU-hour ceiling is `runners_entitlement.max_vcpu_h` (nullable;
  null = no cap = wall-off per migration 0072).
- Markup is read at invoice time (existing pipeline), not enforced here.

### 3.4 Worker Integration

```typescript
// src/worker/index.ts (additions — REPLACES old §3.5)

import { enforceDevenvQuota } from "./devenv_guard";

export default {
    async fetch(request, env, ctx): Promise<Response> {
        const url = new URL(request.url);

        // DevEnv API routes
        if (url.pathname.startsWith("/v1/customer/devenv")) {
            // Auth: extract tenant from headers (Worker-injected)
            const tenantId = request.headers.get("x-corelink-tenant-id");
            if (!tenantId) {
                return new Response(JSON.stringify({ error: "Unauthorized" }), {
                    status: 401,
                    headers: { "Content-Type": "application/json" }
                });
            }

            // Quota enforcement (fail-CLOSED)
            const quota = await enforceDevenvQuota(env, tenantId);
            if (!quota.allowed) {
                // 503 if the guard itself was unavailable (H6), 403 otherwise
                const isGuardDown = quota.reason?.startsWith("Quota check unavailable") ?? false;
                return new Response(JSON.stringify({ error: quota.reason }), {
                    status: isGuardDown ? 503 : 403,
                    headers: { "Content-Type": "application/json" }
                });
            }

            // Route to DO
            const id = env.RUNNER_DEVENV_DO.idFromName(tenantId);
            const stub = env.RUNNER_DEVENV_DO.get(id);
            return stub.fetch(request);
        }

        // Billing ingest route lives in corelink-server (Rust) at
        // /internal/v1/billing/usage — NOT mounted in the Worker.
        // (Removed: was billingDevenvIngestRouter; canonical endpoint is
        //  in corelink-container/src/routes/billing_ingest.rs.)

        // ... existing routes
    }
};
```

### 3.5 Environment Variables (wrangler.jsonc)

```jsonc
// wrangler.jsonc (additions)
{
  "vars": {
    // Billing (existing canonical names — DO NOT introduce a parallel secret)
    "BILLING_INGEST_URL": "https://corelink-api.humangr.com",
    "BILLING_DEVENV_MARKUP_PERCENT": "50",  // 50% markup on CF Container cost

    // DevEnv defaults
    "DEVENV_DEFAULT_IDLE_TIMEOUT_MS": "1800000",  // 30 minutes
    "DEVENV_MAX_SESSION_HOURS": "24",

    // Region colocode for usage events (3-char ASCII lowercase; e.g. "iad")
    "CLW_REGION": "dev"
  },
  // H12 (iter 2 fix) — Per-Tenant UUID Injection Mechanism:
  // The DO contract is: `this.envVars.BILLING_TENANT_UUID` is a canonical
  // UUIDv7 at runtime. The DO has only the CLW slug (`CLW_TENANT`); the
  // ingest endpoint's `Uuid::parse_str()` validation requires the canonical
  // UUID. The slug→UUID mapping lives in the Worker auth context.
  //
  // The wrangler mechanism for per-tenant env-var injection is NOT a
  // top-level `tenant_env_vars` key (that key does not exist in the
  // wrangler schema — adding it would break `wrangler deploy`). The actual
  // surface is the Workers for Platforms dispatch-namespace binding OR
  // the WP-08 Worker ingress calling `super.start({ envVars: {...} })`
  // with the resolved UUID on every session-start. The WP-08 owner picks
  // the mechanism; WP-07 only requires the OUTCOME (the env var is set
  // on the DO at runtime). Cross-WP ownership: WP-08.

  "secrets": [
    "BILLING_INGEST_AUTH_KEY"   // canonical ASK-2 secret — REUSED, not duplicated
  ]
}
```

### 3.6 BLAKE3 Helper (Real WASM, Not Placeholder)

```typescript
// src/utils/hash.ts
//
// BLAKE3-256, lowercase hex, 64 chars. The canonical `idem_key` shape for
// the existing `usage_event_staging` table (migration 0017:
// length(event_payload_hash) = 64). MUST be stable BLAKE3 (not BLAKE2, not
// SHA-256) so the same input produces the same 64-hex output across
// workers AND matches the Rust `corelink_hash::Digest::compute` (which the
// server-side `event_payload_hash` also uses — `billing_ingest.rs:228`).
//
// H13 (iter 2 fix): NO `crypto.subtle.digest("SHA-256")` placeholder. The
// Cloudflare Workers SubtleCrypto API exposes SHA-256 + SHA-1 + SHA-512
// only — not BLAKE3. A vendored WASM BLAKE3 is required. Acceptable
// implementations (in order of preference):
//   1. `@blaze/blake3` — pure-JS, no WASM dependency, ~5KB gzipped.
//   2. `blake3-wasm` — WASM build, ~20KB gzipped, slightly faster.
// Either MUST be deterministic across workers (no Math.random, no time
// dependence); the determinism test below pins this contract.
import { hash as blake3Wasm } from "@blaze/blake3"; // vendored BLAKE3-256

export async function blake3Hex(input: string): Promise<string> {
    const bytes = new TextEncoder().encode(input);
    // `@blaze/blake3` returns a Uint8Array of length 32 (BLAKE3-256).
    const hashBytes = await blake3Wasm(bytes, 32);
    return Array.from(hashBytes)
        .map(b => b.toString(16).padStart(2, "0"))
        .join("");
}
```

> **Determinism test** (do not skip — pins H13):
> ```typescript
> import { describe, it, expect } from "vitest";
> import { blake3Hex } from "../src/utils/hash";
>
> describe("blake3Hex", () => {
>     it("is deterministic: same input → same output", async () => {
>         const a = await blake3Hex("test-input");
>         const b = await blake3Hex("test-input");
>         expect(a).toBe(b);
>     });
>     it("returns 64 lowercase hex chars (BLAKE3-256 canonical form)", async () => {
>         const h = await blake3Hex("anything");
>         expect(h).toMatch(/^[0-9a-f]{64}$/);
>     });
>     it("matches the Rust golden vector from corelink-hash test suite", async () => {
>         // Golden: BLAKE3-256("") =
>         //   af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262
>         // (Per RFC 7693 + BLAKE3 spec test vectors.)
>         const h = await blake3Hex("");
>         expect(h).toBe("af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262");
>     });
> });
> ```

### 3.7 Pricing Reference (Out-of-Scope for Implementation)

The Stripe invoice generation already exists (out of scope per §2). The
env var `BILLING_DEVENV_MARKUP_PERCENT` (default `50`) is read at invoice
time. WP-07 does NOT change invoice code; it only documents the contract.

Reference values for operator audit (2026 Cloudflare Container standard-4
rates — verify before going to production):
- vCPU-second: $0.000024/sec ($0.0864/hour) for standard-4 (4 vCPU)
- Memory (8 GiB): included
- Storage (20 GiB NVMe): included
- Egress: free (Cloudflare network)

---

## 4. Acceptance Criteria (DoD)

| # | Criterion | Verification Method |
|---|-----------|---------------------|
| 1 | No new `UsageEventKind` variant added — reuses `RunnerVcpuSeconds` (B16) | `canonical_event_kinds_count_is_eight` test still passes; `grep devenv_vcpu_seconds` returns no Rust matches |
| 2 | `recordUsage()` posts to canonical `/internal/v1/billing/usage` with `event_kind=runner_vcpu_seconds` | DO logs show "recordUsage: X vCPU-s recorded (event_kind=runner_vcpu_seconds)" |
| 3 | Idempotency: same session → 1 row in `usage_event_staging` | Two `recordUsage` calls (forced) → 1 row, second returns `deduped: 1` |
| 4 | Tenant UUID (`BILLING_TENANT_UUID`) validated by ASK-2 (`Uuid::parse_str`) | Malformed UUID → 400 `bad_tenant_id` |
| 5 | `enforceDevenvQuota` enforces concurrency + vCPU-hour ceiling via `devenv_monthly_vcpu` side-table | Tenant with `max_vcpu_h=0` ceiling reached → 403 |
| 6 | Markup env var declared (`BILLING_DEVENV_MARKUP_PERCENT=50`) | `wrangler secret list` shows the var |
| 7 | `onError` records usage BEFORE transitioning to `errored` state (B2 fix) | Crash test → usage row created even on crash |
| 8 | Quota guard fails CLOSED on DO error (H6 fix) | Inject DO stub error → 503 (not bypass) |
| 9 | Auth uses `X-Corelink-Internal-Auth` + `BILLING_INGEST_AUTH_KEY` (canonical) | Curl with `Authorization: Bearer …` → 401 |
| 10 | No new D1 migration for `runner_devenv_usage` (uses canonical `usage_event_staging`) | `ls migrations/d1/ \| grep runner_devenv` returns empty |
| 11 | **B15 fix:** NEW migration `0094_devenv_monthly_vcpu.sql` creates the per-tenant side-table | `grep "CREATE TABLE devenv_monthly_vcpu" migrations/d1/0094_devenv_monthly_vcpu.sql` |
| 12 | **B19 / H12 fix:** Worker injects `BILLING_TENANT_UUID` as a tenant-scoped env var on `start()` | WP-08 ingress code review: `BILLING_TENANT_UUID` present in the `envVars` set on `super.start()` |
| 13 | **H13 fix:** `blake3Hex` uses a real BLAKE3 WASM (not SHA-256 placeholder) | Determinism test passes; matches Rust golden vector |
| 14 | **B17 prerequisite:** S-10 follow-up adds `UsageUnit::VcpuSeconds` variant + `canonical_unit()` fix for `RunnerVcpuSeconds` | Tracked in S-10 backlog; gates WP-07 ship |

---

## 5. Invariants

| Invariant | Description | Enforced at |
|-----------|-------------|-------------|
| **I1** | `vcpu_seconds = wall_seconds × 4` (standard-4 = 4 vCPU) | Worker (TS) |
| **I2** | `idem_key = BLAKE3-256(tenant_uuid\|container\|startedAt)` is unique per session | Worker (TS) — real BLAKE3 WASM (H13) |
| **I3** | Duplicate `idem_key` → single `usage_event_staging` row | DB (PK `(tenant_id, request_id)`) |
| **I4** | `qty >= 0` always | DB (CHECK) + TS validation |
| **I5** | `time_ms` is Unix epoch ms (canonical) | TS (single source) |
| **I6** | Per-tenant active DevEnvs ≤ `runners_entitlement.max_concurrency` | Worker ingress (`enforceDevenvQuota`) |
| **I7** | Monthly vCPU-seconds ≤ `max_vcpu_h × 3600` (read from `devenv_monthly_vcpu`) | Worker ingress (`enforceDevenvQuota`) |
| **I8** | `BILLING_INGEST_AUTH_KEY` ≥ 32 chars, route mounted only if present (canonical ASK-2 fail-CLOSED) | Rust boot (`build_state_from_env`) |
| **I9** | Usage recording never fails DO lifecycle (fire-and-forget) | Worker (TS) `try/catch` |
| **I10** | Auth compare is constant-time (reuses `internal_auth_ok`) | Rust route |
| **I11** | `event_kind` is `RunnerVcpuSeconds` (canonical, reused — not a new variant) | `UsageEventKind` enum (8 elements) |
| **I12** | `tenant_id` on the wire is the canonical UUIDv7, not the CLW slug | `BILLING_TENANT_UUID` env var (H12, injected by Worker ingress) |

---

## 6. Quality Standards (SOTA)

| Standard | Requirement |
|----------|-------------|
| **Idempotency** | DB-level PRIMARY KEY `(tenant_id, request_id)` (no SELECT-then-INSERT race) |
| **Audit Trail** | Every usage event flows through canonical `usage_event_staging` → R2 Object Lock 7y archive |
| **Fire-and-Forget** | `recordUsage` never throws; logs error and continues |
| **Per-Tenant Isolation** | All D1 queries always `WHERE tenant_id = ?` |
| **Tier Enforcement** | Quota checked at Worker ingress (before DO), read from `runners_entitlement` (no hardcoded map) |
| **Markup Transparency** | `BILLING_DEVENV_MARKUP_PERCENT` env var; visible on invoice (existing pipeline) |
| **Reuse, don't reinvent** | No parallel billing pipeline; reuses canonical ASK-2 endpoint + table + auth |
| **Fail-CLOSED** | Quota guard rejects on any I/O error; route not mounted if secret absent/short |

---

## 7. Completeness Checklist

- [ ] `UsageEventKind::RunnerVcpuSeconds` REUSED (no new variant — B16)
- [ ] WP-04 `recordUsage()` updated to use canonical ASK-2 endpoint
- [ ] `enforceDevenvQuota` reads `max_concurrency` + `max_vcpu_h` from `runners_entitlement`; ceiling from `devenv_monthly_vcpu`
- [ ] Migration `0094_devenv_monthly_vcpu.sql` applied (B15)
- [ ] Worker ingress injects `BILLING_TENANT_UUID` on `start()` (H12)
- [ ] Fail-CLOSED posture on DO error (H6) + on auth error (canonical ASK-2)
- [ ] Quota guard wired into Worker ingress for `/v1/customer/devenv/*`
- [ ] `BILLING_INGEST_AUTH_KEY` declared as Worker secret
- [ ] `BILLING_DEVENV_MARKUP_PERCENT=50` declared as Worker var
- [ ] BLAKE3 helper in `src/utils/hash.ts` (real WASM, deterministic, 64-hex, matches Rust `corelink_hash::Digest`) + golden-vector test (H13)
- [ ] Unit tests: `recordUsage` BLAKE3 determinism, `BILLING_TENANT_UUID` env accessor (M11), quota guard (allow / concurrency-reject / vCPU-ceiling-reject / fail-CLOSED-on-DO-error)
- [ ] Integration test: full session → 1 row in `usage_event_staging`, idempotent re-fire → `deduped: 1`; monthly UPSERT → next /v1/customer/devenv/* 403
- [ ] Code review completed by corelink-server TL
- [ ] **Pre-ship gate:** S-10 follow-up lands `UsageUnit::VcpuSeconds` + `canonical_unit()` fix for `RunnerVcpuSeconds` (B17)

---

## 8. Self-Check Points (Agent Evaluation)

### Self-Check 1: Idempotency Correctness
> **Question:** Can the same usage event be submitted twice without double-billing?
>
> **Verification:**
> - [ ] `idem_key = BLAKE3-256(tenant|container|startedAt)` — unique per session
> - [ ] ASK-2 endpoint uses `INSERT … ON CONFLICT (tenant_id, request_id) DO NOTHING`
> - [ ] Duplicate key → 202 with `deduped: 1`
> - [ ] `onStop` and `onError` both use the same key (capture in local before transition)
> - [ ] Retry-safe: network timeout → retry → same key → no duplicate row

### Self-Check 2: vCPU-Second Calculation Accuracy
> **Question:** Is the vCPU-second calculation correct for Cloudflare Container standard-4?
>
> **Verification:**
> - [ ] standard-4 = 4 vCPU (Cloudflare docs)
> - [ ] `vcpu_seconds = max(0, wall_seconds) × 4`
> - [ ] `wall_seconds = floor((period_end_ms - period_start_ms) / 1000)`
> - [ ] 1 hour session = 3600s × 4 = 14,400 vCPU-seconds
> - [ ] Minimum billable: 1 second = 4 vCPU-seconds
> - [ ] Zero-second session is skipped (logged, not billed)

### Self-Check 3: Quota Guard Correctness
> **Question:** Are the concurrency + monthly vCPU-hour limits correctly enforced?
>
> **Verification:**
> - [ ] Reads `max_concurrency` + `max_vcpu_h` from `runners_entitlement` (single source)
> - [ ] No hardcoded tier map in Worker code
> - [ ] Fail-CLOSED on DO error (503, not 200-bypass)
> - [ ] Fail-CLOSED on DB error (no `catch { allowed: true }`)
> - [ ] `max_vcpu_h` null = no ceiling (wall-off, per migration 0072)
> - [ ] No row in `runners_entitlement` = no entitlement = reject 403
> - [ ] Concurrency check uses WP-06 state union (starting/running/port_wait = active)

---

## 9. Risk Register

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| ASK-2 endpoint unavailable | Medium | Low | Fire-and-forget; log error; don't fail DO lifecycle (I9) |
| BLAKE3 TS impl drifts from Rust | Low | High | Determinism test (same input → same output) + golden vector from `corelink_hash` test suite |
| Tier limit race (concurrent requests) | Low | Medium | DO is single-threaded per tenant; ceiling checked at ingress under Worker lock |
| Markup config drift | Low | Low | Env var; reviewed at deploy |
| `max_vcpu_h` query hits unindexed `usage_daily` | Low | Medium | Use existing `usage_daily` index on `(tenant_id, day_at)` (verify in migration 0089) |
| `recordUsage` runs on `errored` state with no `workspaceName` | Low | Medium | Capture locals BEFORE state transition (B2 fix in §3.2) |

---

## 10. Cross-WP Coordination

| WP | Coordination Needed |
|----|---------------------|
| **WP-04** | Update `recordUsage()` body per §3.2 (URL, auth, payload, idempotency, BLAKE3, `BILLING_TENANT_UUID`). Signature unchanged. WP-04 §3.2's `computeUsageEvent` is now superseded by WP-07 §3.2 — remove it. (B18) |
| **WP-06** | No change. `onStop` (line 356) and `onError` (line 420) already call `this.recordUsage()`. Ensure `onError` captures `workspaceName`/`profileName` into locals BEFORE `transitionState("errored", ...)` (B2 fix). The "BILLING_DO" reference in WP-06 §3.2.1 is incorrect — drop it; the method is on the same DO. |
| **WP-08** | **NEW (H12):** Worker ingress must inject the canonical `BILLING_TENANT_UUID` (UUIDv7) as a tenant-scoped env var on every `super.start({ envVars: {...} })` call. The DO has only the CLW slug; the ingest endpoint requires the UUID. The slug→UUID mapping lives in the Worker auth context. |
| **WP-01** | No change. Discriminated union is the source of truth; WP-07 captures fields in locals before `errored` transition. The `envVars` instance field is read by `recordUsage` (M11 typed accessor). |
| **WP-09** | No change. Dashboard reads from `usage_daily` (already exists, migration 0089) and from the new `devenv_monthly_vcpu` for the per-tenant ceiling display. |
| **`corelink-billing-emit`** | **B17 (cross-sprint, S-10 follow-up):** add `UsageUnit::VcpuSeconds` variant + map `RunnerVcpuSeconds → UsageUnit::VcpuSeconds` in `canonical_unit()`. Current `OpCount` mapping is wrong (time-multiplied metric lands in per-op Stripe bucket). Required before WP-07 ships. |
| **`corelink-container/src/routes/billing_ingest.rs`** | No code change. The endpoint already accepts `event_kind: "runner_vcpu_seconds"` (verified — line 240 of `event.rs`). The aggregator downstream routes by `source` to distinguish the DevEnv surface from the ephemeral runner surface. |
| **Migration 0094 (new, B15)** | New D1 migration: `0094_devenv_monthly_vcpu.sql` — per-tenant UPSERT target for the hot-path ceiling gate. NOT `usage_daily` (display-telemetry, no `vcpu_seconds` column). The canonical aggregator (cron) is EVENTUAL; the gate is HOT. UPSERT on every `recordUsage`; PK lookup at `enforceDevenvQuota`. |

---

## 11. Sign-Off

| Role | Name | Signature | Date |
|------|------|-----------|------|
| Author | | | |
| Reviewer (corelink-server TL) | | | |
| Approver (TechLead) | | | |

---

**END OF WP-07 (Iter 1 + Iter 2 applied — pending: S-10 follow-up B17, cross-WP signal to WP-04/WP-08)**
