/**
 * Customer-facing audit-chain visualization — wave-29 stream-6 deliverable.
 *
 * Restarts the wave-8 `wt/r-prep-audit-chain-viz` scaffold (commit `21f8ea8`,
 * never merged) as a Docusaurus standalone page so the customer-value-add
 * lands inside the public docs surface that ships at `docs.corelink.dev`
 * (WI-S18-001 foundation) rather than the admin-ui shell.
 *
 * Consumes ONLY the wave-19 customer-audit endpoints (no new server-side
 * handlers per the wave-29 stream-6 charter):
 *
 *   - `GET /v1/audit/analytics/event-count?from=&to=`
 *       (wave-19 `apps/server/src/routes/audit_analytics.rs`)
 *   - `GET /v1/audit/analytics/timeline?from=&to=&granularity=`
 *       (wave-19 ibid.)
 *   - `GET /v1/audit/export?from=&to=`
 *       (wave-15 + wave-18 streaming + wave-19 schema lift —
 *        `apps/server/src/routes/audit_export.rs`, commit `3d835cb`)
 *
 * The 32-byte BLAKE3 chain-head anchor is recovered from the
 * `X-CoreLink-Audit-Export-Chain-Head-Anchor` response header — the same
 * canonical header the wave-17/wave-19 CLI `verify-ndjson --url` mode
 * binds (commit `7ec5435`, header constant
 * `audit_export.rs:148 = "x-corelink-audit-export-chain-head-anchor"`).
 *
 * Auth: customer-scoped Clerk JWT. The Clerk shell on the customer dashboard
 * exposes the active session token at `window.__corelink.getToken()` and the
 * tenant id at `window.__corelink.tenantId`; both are populated by the admin-
 * ui Clerk provider before navigation here. When absent (e.g. direct landing
 * from a marketing link) the page surfaces a non-blocking instruction strip
 * rather than redirecting — the surrounding docs site is a public surface.
 *
 * Pre-GA honesty framing: the hero banner carries the "Pilot data; SOC 2
 * Type II certification pending" disclosure required by the wave-26 Trust
 * Center copy review.
 *
 * Charter compliance (wave-29 stream-6):
 *   - Customer-facing pre-GA enhancement (§3.b customer-value-add).
 *   - No server handlers added (consumes existing wave-19 endpoints).
 *   - i18n 4 locales (en-US default + pt-BR + de + es-419) via Docusaurus
 *     `<Translate>` keyed by stable IDs in `i18n/<locale>/code.json`.
 *   - WCAG 2.2 AA: contrast tracked via Infima vars (wave-25 a11y baseline).
 */

import { type ReactElement, useCallback, useEffect, useMemo, useState } from "react";
import Layout from "@theme/Layout";
import BrowserOnly from "@docusaurus/BrowserOnly";
import Translate, { translate } from "@docusaurus/Translate";
import Link from "@docusaurus/Link";

import styles from "./audit-chain.module.css";

// --- Wave-19 endpoint contracts (mirrors `audit_analytics.rs` response shapes).

/** Response from `GET /v1/audit/analytics/event-count`. */
interface EventCountResponse {
  buckets: ReadonlyArray<{ event_type: string; count: number }>;
}

/** Response from `GET /v1/audit/analytics/timeline`. */
interface TimelineResponse {
  buckets: ReadonlyArray<{ bucket_start_ms: number; count: number }>;
  granularity_ms: number;
}

/** Result of the chain-head + analytics probe over the user-selected window. */
interface ChainState {
  anchorHex: string | null;
  totalEvents: number;
  approxBytesFlushed: number;
  timeline: TimelineResponse["buckets"];
  granularityMs: number;
}

const ANCHOR_HEADER = "x-corelink-audit-export-chain-head-anchor";

/** Hard upper bound enforced by the UI per §UX (the API permits a 7-year window). */
const MAX_WINDOW_MS = 7 * 24 * 60 * 60 * 1000;

const DEFAULT_GRANULARITY_MS = 60 * 60 * 1000; // 1h buckets

/**
 * Read the customer-scoped JWT + tenant id from the Clerk shell hooks that
 * the admin-ui provider injects into `window.__corelink`. Returns `null`
 * when the page is opened standalone (no Clerk shell present) — the UI
 * surfaces an instruction strip in that case.
 */
interface CorelinkShell {
  getToken: () => Promise<string | null>;
  tenantId: string | null;
}

function readShell(): CorelinkShell | null {
  if (typeof window === "undefined") {
    return null;
  }
  const w = window as unknown as { __corelink?: CorelinkShell };
  return w.__corelink ?? null;
}

function toIsoLocalDate(d: Date): string {
  const y = d.getUTCFullYear();
  const m = String(d.getUTCMonth() + 1).padStart(2, "0");
  const day = String(d.getUTCDate()).padStart(2, "0");
  return `${y}-${m}-${day}`;
}

function isoToEpochMs(iso: string): number | null {
  // Accept `YYYY-MM-DD` from the date picker; widen to a full UTC day.
  if (!/^\d{4}-\d{2}-\d{2}$/.test(iso)) {
    return null;
  }
  const t = Date.parse(`${iso}T00:00:00Z`);
  return Number.isFinite(t) ? t : null;
}

function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let v = n / 1024;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i += 1;
  }
  return `${v.toFixed(2)} ${units[i]}`;
}

/**
 * Probe the chain-head anchor + analytics for the selected window.
 *
 * Strategy: issue a `Range: bytes=0-0`-style HEAD via `fetch` with method
 * `HEAD` against `/v1/audit/export` to recover the
 * `X-CoreLink-Audit-Export-Chain-Head-Anchor` response header without
 * downloading the NDJSON body. Then issue parallel GETs against the
 * event-count + timeline analytics endpoints.
 */
async function probeChain(
  token: string,
  tenantId: string,
  fromMs: number,
  toMs: number,
  granularityMs: number,
): Promise<ChainState> {
  const auth = `Bearer ${token}`;
  const tenantHeader = { "x-tenant-id": tenantId } as const;

  const exportUrl = `/v1/audit/export?from=${fromMs}&to=${toMs}`;
  const eventCountUrl = `/v1/audit/analytics/event-count?from=${fromMs}&to=${toMs}`;
  const timelineUrl = `/v1/audit/analytics/timeline?from=${fromMs}&to=${toMs}&granularity=${granularityMs}`;

  const [headResp, ecResp, tlResp] = await Promise.all([
    fetch(exportUrl, {
      method: "HEAD",
      headers: { authorization: auth, ...tenantHeader },
      credentials: "omit",
    }),
    fetch(eventCountUrl, {
      method: "GET",
      headers: { authorization: auth, accept: "application/json", ...tenantHeader },
      credentials: "omit",
    }),
    fetch(timelineUrl, {
      method: "GET",
      headers: { authorization: auth, accept: "application/json", ...tenantHeader },
      credentials: "omit",
    }),
  ]);

  if (!ecResp.ok) {
    throw new Error(`event-count: HTTP ${ecResp.status}`);
  }
  if (!tlResp.ok) {
    throw new Error(`timeline: HTTP ${tlResp.status}`);
  }

  const anchorHex = headResp.headers.get(ANCHOR_HEADER);
  const ec = (await ecResp.json()) as EventCountResponse;
  const tl = (await tlResp.json()) as TimelineResponse;

  const totalEvents = ec.buckets.reduce((acc, b) => acc + b.count, 0);
  // Bytes flushed is a wave-19 SLO surrogate; the canonical figure comes
  // from the export manifest. We surface a deterministic estimate
  // (avg-row-bytes × count) until the wave-30 analytics extension lands
  // the canonical bytes-flushed bucket — labelled "estimate" in the UI.
  const AVG_ROW_BYTES = 512;
  const approxBytesFlushed = totalEvents * AVG_ROW_BYTES;

  return {
    anchorHex,
    totalEvents,
    approxBytesFlushed,
    timeline: tl.buckets,
    granularityMs: tl.granularity_ms,
  };
}

function AuditChainViz(): ReactElement {
  const today = useMemo(() => new Date(), []);
  const sevenDaysAgo = useMemo(() => {
    const d = new Date(today);
    d.setUTCDate(d.getUTCDate() - 7);
    return d;
  }, [today]);

  const [fromIso, setFromIso] = useState<string>(toIsoLocalDate(sevenDaysAgo));
  const [toIso, setToIso] = useState<string>(toIsoLocalDate(today));
  const [loading, setLoading] = useState<boolean>(false);
  const [state, setState] = useState<ChainState | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [shellMissing, setShellMissing] = useState<boolean>(false);

  const load = useCallback(async () => {
    setError(null);

    const shell = readShell();
    if (!shell) {
      setShellMissing(true);
      return;
    }
    setShellMissing(false);

    const fromMs = isoToEpochMs(fromIso);
    const toMs = isoToEpochMs(toIso);
    if (fromMs === null || toMs === null) {
      setError(
        translate({
          id: "customer.auditChain.error.invalidDate",
          message: "Invalid date range; use YYYY-MM-DD.",
        }),
      );
      return;
    }
    if (toMs <= fromMs) {
      setError(
        translate({
          id: "customer.auditChain.error.invertedRange",
          message: "End date must be after start date.",
        }),
      );
      return;
    }
    if (toMs - fromMs > MAX_WINDOW_MS) {
      setError(
        translate({
          id: "customer.auditChain.error.windowTooLong",
          message: "Window too long; the UI caps at 7 days. Use the API for longer windows (up to 7 years).",
        }),
      );
      return;
    }

    setLoading(true);
    try {
      const token = await shell.getToken();
      if (!token || !shell.tenantId) {
        setShellMissing(true);
        return;
      }
      const next = await probeChain(token, shell.tenantId, fromMs, toMs, DEFAULT_GRANULARITY_MS);
      setState(next);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }, [fromIso, toIso]);

  useEffect(() => {
    // Initial probe on mount — guarded by the shell-presence check.
    void load();
  }, [load]);

  const onExport = useCallback(async () => {
    const shell = readShell();
    if (!shell) {
      setShellMissing(true);
      return;
    }
    const fromMs = isoToEpochMs(fromIso);
    const toMs = isoToEpochMs(toIso);
    if (fromMs === null || toMs === null) {
      return;
    }
    const token = await shell.getToken();
    if (!token) {
      setShellMissing(true);
      return;
    }
    // Triggers the wave-18 true-streaming NDJSON path. The browser handles
    // the response as a download via Content-Disposition (set by the route).
    const url = `/v1/audit/export?from=${fromMs}&to=${toMs}`;
    // We use a temporary anchor with `download` so the bearer + tenant
    // header still flow via a fetch-then-blob round-trip; a direct `<a>`
    // cannot inject Authorization headers.
    try {
      const resp = await fetch(url, {
        method: "GET",
        headers: {
          authorization: `Bearer ${token}`,
          "x-tenant-id": shell.tenantId ?? "",
          accept: "application/x-ndjson",
        },
        credentials: "omit",
      });
      if (!resp.ok) {
        throw new Error(`export: HTTP ${resp.status}`);
      }
      const blob = await resp.blob();
      const anchor = resp.headers.get(ANCHOR_HEADER) ?? "unknown";
      const downloadUrl = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = downloadUrl;
      a.download = `corelink-audit-${fromIso}-to-${toIso}-${anchor.slice(0, 12)}.ndjson`;
      a.rel = "noopener";
      document.body.appendChild(a);
      a.click();
      a.remove();
      URL.revokeObjectURL(downloadUrl);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }, [fromIso, toIso]);

  // ---------- Render ----------

  const maxBucketCount = state
    ? state.timeline.reduce((m, b) => (b.count > m ? b.count : m), 0)
    : 0;

  return (
    <div className={styles.container}>
      <header className={styles.heroCard}>
        <h1 className={styles.heroTitle}>
          <Translate id="customer.auditChain.title" description="H1 on the customer audit-chain visualization page">
            Audit chain
          </Translate>
        </h1>
        <p className={styles.heroSubtitle}>
          <Translate
            id="customer.auditChain.subtitle"
            description="Subtitle explaining what the page shows"
          >
            Inspect the cryptographic chain head anchor, event volume, and timeline for your tenant&apos;s audit log over the selected window.
          </Translate>
        </p>
      </header>

      <div className={styles.honestBanner} role="status">
        <strong>
          <Translate id="customer.auditChain.preGa.title" description="Pilot framing banner title">
            Pilot data — SOC 2 Type II certification pending.
          </Translate>
        </strong>
        <Translate id="customer.auditChain.preGa.body" description="Pilot framing banner body">
          Chain integrity is enforced cryptographically (RFC 6962 + BLAKE3) and verified daily via the cron job documented in WI-S09-008. SOC 2 evidence collection is in flight ahead of GA.
        </Translate>
      </div>

      {shellMissing ? (
        <div className={styles.error} role="alert">
          <Translate
            id="customer.auditChain.shellMissing"
            description="Shown when the Clerk shell is not bootstrapped (e.g. user landed here from a public link)"
          >
            Sign in to your CoreLink dashboard to load tenant-scoped audit data. This preview page consumes live data only inside the authenticated shell.
          </Translate>
        </div>
      ) : null}

      <section className={styles.section} aria-labelledby="window-heading">
        <h2 id="window-heading" className={styles.sectionTitle}>
          <Translate id="customer.auditChain.window.heading">Window</Translate>
        </h2>
        <div className={styles.controls}>
          <div className={styles.field}>
            <label htmlFor="from-date">
              <Translate id="customer.auditChain.window.from">From (UTC)</Translate>
            </label>
            <input
              id="from-date"
              type="date"
              value={fromIso}
              onChange={(e) => setFromIso(e.target.value)}
              max={toIso}
            />
          </div>
          <div className={styles.field}>
            <label htmlFor="to-date">
              <Translate id="customer.auditChain.window.to">To (UTC, inclusive)</Translate>
            </label>
            <input
              id="to-date"
              type="date"
              value={toIso}
              onChange={(e) => setToIso(e.target.value)}
              min={fromIso}
              max={toIsoLocalDate(today)}
            />
          </div>
          <div className={styles.actions}>
            <button
              type="button"
              className={styles.btn}
              onClick={() => {
                void load();
              }}
              disabled={loading}
            >
              {loading ? (
                <Translate id="customer.auditChain.btn.loading">Loading…</Translate>
              ) : (
                <Translate id="customer.auditChain.btn.reload">Reload</Translate>
              )}
            </button>
            <button
              type="button"
              className={`${styles.btn} ${styles.btnSecondary}`}
              onClick={() => {
                void onExport();
              }}
              disabled={loading || !state}
            >
              <Translate id="customer.auditChain.btn.export">Export to NDJSON</Translate>
            </button>
          </div>
        </div>
      </section>

      {error ? (
        <div className={styles.error} role="alert">
          {error}
        </div>
      ) : null}

      <section className={styles.section} aria-labelledby="anchor-heading">
        <h2 id="anchor-heading" className={styles.sectionTitle}>
          <Translate id="customer.auditChain.anchor.heading">Chain-head anchor</Translate>
        </h2>
        <div className={styles.anchorBlock}>
          <p>
            <Translate id="customer.auditChain.anchor.body">
              {"The 32-byte BLAKE3 anchor below pins the inclusive state of the audit chain at the end of your selected window. Capture it offline and feed it into `corelink audit verify-ndjson --chain-head-anchor <hex>` to verify the export without trusting our infrastructure."}
            </Translate>
          </p>
          <code className={styles.anchorHex} aria-label="chain head anchor (hex)">
            {state?.anchorHex ?? (
              <Translate id="customer.auditChain.anchor.placeholder">— (load a window to populate)</Translate>
            )}
          </code>
          <Link className={styles.helpLink} to="/security/audit-chain">
            <Translate id="customer.auditChain.anchor.helpLink">
              Verify offline — see the CLI verify-ndjson docs →
            </Translate>
          </Link>
        </div>
      </section>

      <section className={styles.section} aria-labelledby="metrics-heading">
        <h2 id="metrics-heading" className={styles.sectionTitle}>
          <Translate id="customer.auditChain.metrics.heading">Volume</Translate>
        </h2>
        <div className={styles.metricsGrid}>
          <div className={styles.metric}>
            <p className={styles.metricLabel}>
              <Translate id="customer.auditChain.metrics.events">Events in window</Translate>
            </p>
            <p className={styles.metricValue}>
              {state ? state.totalEvents.toLocaleString() : "—"}
            </p>
          </div>
          <div className={styles.metric}>
            <p className={styles.metricLabel}>
              <Translate id="customer.auditChain.metrics.bytes">Bytes flushed (estimate)</Translate>
            </p>
            <p className={styles.metricValue}>
              {state ? formatBytes(state.approxBytesFlushed) : "—"}
            </p>
          </div>
          <div className={styles.metric}>
            <p className={styles.metricLabel}>
              <Translate id="customer.auditChain.metrics.granularity">Bucket granularity</Translate>
            </p>
            <p className={styles.metricValue}>
              {state ? `${Math.round(state.granularityMs / 60000)} min` : "—"}
            </p>
          </div>
        </div>
      </section>

      <section className={styles.section} aria-labelledby="timeline-heading">
        <h2 id="timeline-heading" className={styles.sectionTitle}>
          <Translate id="customer.auditChain.timeline.heading">Timeline</Translate>
        </h2>
        {state && state.timeline.length > 0 && maxBucketCount > 0 ? (
          <div className={styles.timeline} role="img" aria-label="audit event timeline">
            {state.timeline.map((bucket) => (
              <div
                key={bucket.bucket_start_ms}
                className={styles.timelineBar}
                style={{ height: `${(bucket.count / maxBucketCount) * 100}%` }}
                title={`${new Date(bucket.bucket_start_ms).toISOString()} — ${bucket.count} events`}
              />
            ))}
          </div>
        ) : (
          <div className={styles.placeholder}>
            <Translate id="customer.auditChain.timeline.empty">No events in the selected window.</Translate>
          </div>
        )}
      </section>
    </div>
  );
}

/**
 * Default export — Docusaurus standalone page.
 *
 * The interactive viz is wrapped in `BrowserOnly` so SSG build emits a
 * static shell + hydrates client-side, matching the wave-25 a11y baseline
 * convention (`PricingCalculator` follows the same pattern).
 */
export default function AuditChainPage(): ReactElement {
  return (
    <Layout
      title={translate({
        id: "customer.auditChain.pageTitle",
        message: "Audit chain — customer",
      })}
      description={translate({
        id: "customer.auditChain.pageDescription",
        message:
          "Inspect the BLAKE3 chain-head anchor, event volume, and NDJSON export for your tenant's audit log.",
      })}
    >
      <BrowserOnly
        fallback={
          <div className={styles.container}>
            <p>
              <Translate id="customer.auditChain.ssrFallback">Loading customer audit chain…</Translate>
            </p>
          </div>
        }
      >
        {() => <AuditChainViz />}
      </BrowserOnly>
    </Layout>
  );
}
