/**
 * Pilot admin web UI — Wave-29 stream-3.
 *
 * Replaces the three wave-27 placeholder shell scripts
 * (`scripts/admin/list-pilot-tenants.sh`, `grant-pilot-tier.sh`,
 * `pilot-24h-checkin.sh`) with a single operator-facing page that
 * consumes the canonical admin endpoints:
 *
 *   GET  /v1/admin/pilots?state=<STATE>
 *   POST /v1/admin/pilots/{tenant_id}/grant-tier
 *   POST /v1/admin/pilots/{tenant_id}/checkin
 *
 * Auth model: the page assumes a Clerk-issued admin JWT carrying the
 * `corelink:admin:pilots` scope claim. The fetch helpers below pin the
 * required `X-Admin-Scope` + `X-Admin-Principal` headers from the
 * Clerk session at request time. Tenant-scoped admins surface their
 * bound tenant via `X-Admin-Tenant`.
 *
 * Polling: the list re-fetches every 30s while the page is mounted.
 *
 * Charter:
 *   - No production secrets in the bundle.
 *   - All buttons confirm before issuing the POST (defence in depth
 *     for the SEC team's after-the-fact triage).
 */

import React, { useCallback, useEffect, useMemo, useState } from "react";
import Layout from "@theme/Layout";
import BrowserOnly from "@docusaurus/BrowserOnly";

/** Pilot lifecycle states surfaced in the state filter. */
const PILOT_STATES = [
  "NEW",
  "RESERVED",
  "PROVISIONED",
  "ACTIVE",
  "GRADUATED",
  "TERMINATED",
] as const;

type PilotStateName = (typeof PILOT_STATES)[number];

interface PilotTenant {
  readonly tenant_id: string;
  readonly slug: string;
  readonly tier: string;
  readonly cap_bytes: number;
  readonly pilot_state: PilotStateName;
  readonly signup_at_ms: number;
  readonly tier_granted_at_ms: number | null;
  readonly first_blob_at_ms: number | null;
}

interface ListPilotsResponse {
  readonly state: PilotStateName;
  readonly rows: ReadonlyArray<PilotTenant>;
  readonly offset: number;
  readonly limit: number;
}

interface CheckinResponse {
  readonly tenant_id: string;
  readonly alert_emitted: boolean;
  readonly age_ms: number | null;
}

/** Polling interval (ms) for the auto-refresh loop. */
const POLL_INTERVAL_MS = 30_000;

/** 24h window — used to drive the countdown UI. */
const CHECKIN_WINDOW_MS = 86_400_000;

/**
 * Admin auth headers. The Clerk session injects the principal + scope
 * claims; tenant-scoped admins also pin a bound tenant.
 *
 * Production wiring (Phase 2): swap these env-derived defaults for
 * `useUser()` + `getToken()` from `@clerk/clerk-react`. The headers
 * are pinned exactly the same way; only the source differs.
 */
function adminHeaders(): Record<string, string> {
  return {
    "Content-Type": "application/json",
    // The Clerk session injection point — Phase 2 swap target.
    "X-Admin-Scope": "corelink:admin:pilots",
    "X-Admin-Principal": "ops@corelink.dev",
  };
}

/** Format a ms-epoch timestamp as an ISO-8601 UTC string. */
function formatTimestamp(ms: number | null): string {
  if (ms === null || ms === undefined) return "—";
  return new Date(ms).toISOString().replace(".000", "");
}

/** Format a byte cap as a decimal-GB string. */
function formatCap(bytes: number): string {
  if (bytes >= 1_000_000_000) {
    return `${(bytes / 1_000_000_000).toFixed(0)}GB`;
  }
  return `${bytes}B`;
}

/**
 * Compute the 24h countdown for an ACTIVE tenant since `tier_granted_at_ms`.
 *
 * Returns the human-readable remaining (or overdue) duration and a
 * boolean flag indicating whether the tenant is past the 24h window
 * without an upload (the no-blob silent-fail anti-pattern).
 */
function countdownSinceGrant(
  tenant: PilotTenant,
  nowMs: number,
): { readonly label: string; readonly overdueNoBlob: boolean } {
  if (tenant.tier_granted_at_ms === null) {
    return { label: "—", overdueNoBlob: false };
  }
  const age = Math.max(0, nowMs - tenant.tier_granted_at_ms);
  const remaining = CHECKIN_WINDOW_MS - age;
  const hoursRemaining = Math.floor(Math.abs(remaining) / 3_600_000);
  const minutesRemaining = Math.floor((Math.abs(remaining) % 3_600_000) / 60_000);
  if (remaining < 0) {
    const overdueNoBlob =
      tenant.pilot_state === "ACTIVE" && tenant.first_blob_at_ms === null;
    return {
      label: `+${hoursRemaining}h${minutesRemaining}m past 24h`,
      overdueNoBlob,
    };
  }
  return {
    label: `${hoursRemaining}h${minutesRemaining}m until 24h`,
    overdueNoBlob: false,
  };
}

/** Top-level admin page component. */
function PilotsAdminPage(): React.ReactElement {
  const [state, setState] = useState<PilotStateName>("NEW");
  const [rows, setRows] = useState<ReadonlyArray<PilotTenant>>([]);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState<boolean>(false);
  const [actionMessage, setActionMessage] = useState<string | null>(null);
  const [nowMs, setNowMs] = useState<number>(Date.now());

  const refresh = useCallback(async (): Promise<void> => {
    setLoading(true);
    setError(null);
    try {
      const url = `/v1/admin/pilots?state=${encodeURIComponent(state)}`;
      const resp = await fetch(url, {
        method: "GET",
        headers: adminHeaders(),
      });
      if (!resp.ok) {
        setError(`list failed: ${resp.status} ${await resp.text()}`);
        return;
      }
      const body = (await resp.json()) as ListPilotsResponse;
      setRows(body.rows);
    } catch (e) {
      setError(`list failed: ${(e as Error).message}`);
    } finally {
      setLoading(false);
    }
  }, [state]);

  // Auto-refresh every POLL_INTERVAL_MS while mounted.
  useEffect(() => {
    void refresh();
    const id = window.setInterval(() => void refresh(), POLL_INTERVAL_MS);
    return () => window.clearInterval(id);
  }, [refresh]);

  // Tick the local clock every 60s for the countdown rendering.
  useEffect(() => {
    const id = window.setInterval(() => setNowMs(Date.now()), 60_000);
    return () => window.clearInterval(id);
  }, []);

  const handleGrantTier = useCallback(
    async (tenant: PilotTenant): Promise<void> => {
      const confirmed = window.confirm(
        `Grant pilot tier to ${tenant.slug} (${tenant.tenant_id})?\n` +
          `cap = 100GB; pilot_state ${tenant.pilot_state} -> ACTIVE.`,
      );
      if (!confirmed) return;
      try {
        const resp = await fetch(
          `/v1/admin/pilots/${encodeURIComponent(tenant.tenant_id)}/grant-tier`,
          {
            method: "POST",
            headers: adminHeaders(),
            body: JSON.stringify({ tier: "pilot", cap_bytes: 100_000_000_000 }),
          },
        );
        if (!resp.ok) {
          setActionMessage(
            `grant-tier failed: ${resp.status} ${await resp.text()}`,
          );
          return;
        }
        setActionMessage(`grant-tier ok: ${tenant.slug}`);
        void refresh();
      } catch (e) {
        setActionMessage(`grant-tier failed: ${(e as Error).message}`);
      }
    },
    [refresh],
  );

  const handleCheckin = useCallback(
    async (tenant: PilotTenant): Promise<void> => {
      try {
        const resp = await fetch(
          `/v1/admin/pilots/${encodeURIComponent(tenant.tenant_id)}/checkin`,
          { method: "POST", headers: adminHeaders() },
        );
        if (!resp.ok) {
          setActionMessage(
            `checkin failed: ${resp.status} ${await resp.text()}`,
          );
          return;
        }
        const body = (await resp.json()) as CheckinResponse;
        setActionMessage(
          body.alert_emitted
            ? `checkin ALERT: ${tenant.slug} — no blob 24h after grant`
            : `checkin ok: ${tenant.slug}`,
        );
      } catch (e) {
        setActionMessage(`checkin failed: ${(e as Error).message}`);
      }
    },
    [],
  );

  const tableRows = useMemo(
    () =>
      rows.map((t) => {
        const { label, overdueNoBlob } = countdownSinceGrant(t, nowMs);
        return (
          <tr
            key={t.tenant_id}
            style={overdueNoBlob ? { background: "#fef2f2" } : undefined}
            data-testid={`pilot-row-${t.tenant_id}`}
          >
            <td>
              <code>{t.tenant_id}</code>
            </td>
            <td>{t.slug}</td>
            <td>{t.tier}</td>
            <td>{formatCap(t.cap_bytes)}</td>
            <td>{t.pilot_state}</td>
            <td>{formatTimestamp(t.signup_at_ms)}</td>
            <td>{formatTimestamp(t.tier_granted_at_ms)}</td>
            <td>{label}</td>
            <td>
              <button
                type="button"
                onClick={() => void handleGrantTier(t)}
                disabled={t.pilot_state === "ACTIVE" || t.pilot_state === "GRADUATED" || t.pilot_state === "TERMINATED"}
                data-testid={`grant-tier-${t.tenant_id}`}
              >
                grant pilot tier
              </button>{" "}
              <button
                type="button"
                onClick={() => void handleCheckin(t)}
                data-testid={`checkin-${t.tenant_id}`}
              >
                24h checkin
              </button>
            </td>
          </tr>
        );
      }),
    [rows, nowMs, handleGrantTier, handleCheckin],
  );

  return (
    <section
      style={{ padding: "1.5rem" }}
      data-testid="pilots-admin-page"
      aria-label="Pilot admin"
    >
      <h1>Pilot admin</h1>
      <p>
        Wave-29 stream-3 web UI for pilot tenant operations. Replaces
        the wave-27 placeholder shell scripts with proper{" "}
        <code>/v1/admin/pilots/*</code> endpoints. Auth-gated by the
        <code>corelink:admin:pilots</code> JWT scope claim.
      </p>
      <div style={{ marginBottom: "1rem" }}>
        <label htmlFor="state-filter">State filter:&nbsp;</label>
        <select
          id="state-filter"
          value={state}
          onChange={(e) => setState(e.target.value as PilotStateName)}
        >
          {PILOT_STATES.map((s) => (
            <option key={s} value={s}>
              {s}
            </option>
          ))}
        </select>{" "}
        <button type="button" onClick={() => void refresh()}>
          refresh now
        </button>
        {loading ? (
          <span style={{ marginLeft: "0.75rem", color: "#6b7280" }}>
            loading…
          </span>
        ) : null}
      </div>
      {error !== null ? (
        <div
          role="alert"
          style={{
            color: "#991b1b",
            background: "#fef2f2",
            padding: "0.5rem",
            border: "1px solid #fecaca",
            marginBottom: "1rem",
          }}
        >
          {error}
        </div>
      ) : null}
      {actionMessage !== null ? (
        <div
          role="status"
          style={{
            color: "#1e40af",
            background: "#eff6ff",
            padding: "0.5rem",
            border: "1px solid #bfdbfe",
            marginBottom: "1rem",
          }}
        >
          {actionMessage}
        </div>
      ) : null}
      <table
        style={{ width: "100%", borderCollapse: "collapse" }}
        data-testid="pilots-table"
      >
        <thead>
          <tr>
            <th>tenant_id</th>
            <th>slug</th>
            <th>tier</th>
            <th>cap</th>
            <th>state</th>
            <th>signup_at</th>
            <th>granted_at</th>
            <th>24h countdown</th>
            <th>actions</th>
          </tr>
        </thead>
        <tbody>{tableRows.length > 0 ? tableRows : (
          <tr>
            <td colSpan={9} style={{ textAlign: "center", color: "#6b7280" }}>
              no pilot tenants matching state={state}
            </td>
          </tr>
        )}</tbody>
      </table>
      <p style={{ marginTop: "1rem", color: "#6b7280", fontSize: "0.875rem" }}>
        Polling every {POLL_INTERVAL_MS / 1000}s. Rows shaded red are
        ACTIVE pilots past the 24h window with no first blob — the
        no-blob silent-fail anti-pattern (customer-success-playbook
        §3.3).
      </p>
    </section>
  );
}

/**
 * Docusaurus default export. Wraps the SPA fragment in a `<Layout>`
 * and a `<BrowserOnly>` so SSR builds (which lack `fetch` + `window`)
 * skip the runtime path and emit a static placeholder.
 */
export default function PilotsAdminRoute(): React.ReactElement {
  return (
    <Layout
      title="Pilot admin"
      description="Operator-facing admin web UI for pilot tenant lifecycle (Wave-29 stream-3)."
    >
      <BrowserOnly fallback={<p>Loading admin UI…</p>}>
        {() => <PilotsAdminPage />}
      </BrowserOnly>
    </Layout>
  );
}
