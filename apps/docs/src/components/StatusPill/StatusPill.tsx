/**
 * StatusPill — small operational-status badge for the docs site navbar.
 *
 * Per the channels audit: showing a live "all systems operational" pill
 * signals operational legitimacy to docs visitors and gives a low-friction
 * path to the public status page when something is wrong.
 *
 * Data source: the BetterStack public status page's JSON endpoint,
 * `<page-url>/index.json`. NOTE: `<page-url>/badge.json` returns HTML, not
 * JSON, on this BetterStack tenant — do not use it. The legacy
 * Atlassian-compatible `<page-url>/api/v2/status.json` shape is also parsed
 * (see `classify()`) so an operator override that points at a real
 * Atlassian Statuspage still works. The endpoint is overridable via the
 * `statusBadgeUrl` prop / site-config customField so operators can rewire to
 * a different status page (e.g. white-label) without touching code.
 *
 * Cache: 30 minutes. The pill stores the last successful fetch in
 * `sessionStorage` keyed by URL, plus a stale-while-revalidate fetch on
 * mount when the cache is older than 30 minutes. The site-wide pill is
 * NOT a real-time monitor — for that, visitors follow the link to the
 * full BetterStack status page (which has its own auto-refresh).
 *
 * Fetch-failure handling — history + current rationale:
 *   This component used to render a green "All systems operational" pill
 *   on ANY fetch failure ("assume good when we can't prove otherwise").
 *   That shipped a false claim for an extended period: the default badge
 *   endpoint's hostname failed its TLS handshake outright (never
 *   provisioned at the status-page host), so every single page load hit
 *   the failure path and rendered "All systems operational" in green — even
 *   while the real page's `aggregate_state` was `"downtime"`. A pill that
 *   asserts good status it has never actually observed is worse than no
 *   pill: it is a trust signal the component cannot back up. So the rule
 *   now is evidence-gated: the pill only ever shows "ok" after a fetch that
 *   actually returned an operational payload (this run, or a still-fresh
 *   cache entry from a prior successful run). Cold render or fetch
 *   failure with nothing cached renders a neutral "unknown" state instead —
 *   grey dot, "Status unavailable" label — which is an honest statement
 *   ("we don't know") rather than an unverified claim of health.
 *
 * State machine (post-fetch):
 *   - status === "operational" / aggregate_state === "operational" → ok (green)
 *   - status === "degraded" / "maintenance" / indicator === "minor" /
 *     aggregate_state === "degraded" / "maintenance" → degraded (yellow)
 *   - status === "down" / "outage" / indicator === "major" / "critical" /
 *     aggregate_state === "downtime" / "outage" → down (red)
 *   - any other / unrecognised payload → unknown (grey, "Status
 *     unavailable") — we do not guess a severity we cannot back with data.
 *   - fetch failure / no cache → unknown (grey, "Status unavailable").
 *
 * Privacy: the component does not send any user data to BetterStack;
 * it just GETs a static JSON URL. No cookies, no fingerprint.
 */

import { useEffect, useState, type ReactElement } from "react";
import styles from "./StatusPill.module.css";
import {
  classify,
  type BetterStackBadgePayload,
  type Severity,
} from "./classify";

export type { BetterStackBadgePayload, Severity };
export { classify };

const CACHE_TTL_MS = 30 * 60 * 1000;
const STORAGE_PREFIX = "corelink:statuspill:v1:";
const DEFAULT_LABEL_OK = "All systems operational";
const DEFAULT_LABEL_DEGRADED = "Degraded performance";
const DEFAULT_LABEL_DOWN = "Active incident";
const DEFAULT_LABEL_UNKNOWN = "Status unavailable";

export interface StatusPillProps {
  /**
   * Public status-page URL (where the pill links to). This is the URL a
   * visitor sees; it should be the human-facing status page, not the
   * JSON badge endpoint.
   */
  readonly statuspageUrl: string;
  /**
   * BetterStack JSON status endpoint. Defaults to
   * `<statuspageUrl>/index.json`. Do NOT default to `/badge.json` — on
   * this BetterStack tenant it returns HTML, not JSON.
   */
  readonly statusBadgeUrl?: string;
  /** Optional translated labels per severity. */
  readonly labelOk?: string;
  readonly labelDegraded?: string;
  readonly labelDown?: string;
  readonly labelUnknown?: string;
}

interface CachedStatus {
  severity: Severity;
  fetchedAt: number;
}

function readCache(key: string): CachedStatus | null {
  if (typeof window === "undefined") return null;
  try {
    const raw = window.sessionStorage.getItem(STORAGE_PREFIX + key);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as Partial<CachedStatus>;
    if (
      parsed.severity === "ok" ||
      parsed.severity === "degraded" ||
      parsed.severity === "down" ||
      parsed.severity === "unknown"
    ) {
      if (typeof parsed.fetchedAt === "number") {
        return { severity: parsed.severity, fetchedAt: parsed.fetchedAt };
      }
    }
    return null;
  } catch {
    return null;
  }
}

function writeCache(key: string, value: CachedStatus): void {
  if (typeof window === "undefined") return;
  try {
    window.sessionStorage.setItem(STORAGE_PREFIX + key, JSON.stringify(value));
  } catch {
    /* sessionStorage may be disabled (Safari private mode) — ignore. */
  }
}

export default function StatusPill(props: StatusPillProps): ReactElement {
  const {
    statuspageUrl,
    statusBadgeUrl = `${statuspageUrl.replace(/\/+$/, "")}/index.json`,
    labelOk = DEFAULT_LABEL_OK,
    labelDegraded = DEFAULT_LABEL_DEGRADED,
    labelDown = DEFAULT_LABEL_DOWN,
    labelUnknown = DEFAULT_LABEL_UNKNOWN,
  } = props;

  // Hydrate from cache synchronously so the first paint matches the cached
  // severity (avoids a green→red flash if there's an ongoing incident the
  // visitor just saw a moment ago). With nothing cached yet, we render the
  // neutral "unknown" state — NOT "ok" — because we have not observed any
  // status at all; see the module doc-comment.
  const [severity, setSeverity] = useState<Severity>(() => {
    const cached = readCache(statusBadgeUrl);
    return cached?.severity ?? "unknown";
  });

  useEffect(() => {
    let cancelled = false;
    const cached = readCache(statusBadgeUrl);
    const fresh = cached && Date.now() - cached.fetchedAt < CACHE_TTL_MS;
    if (fresh) {
      // Cache hit within TTL — no fetch, no API hammering. The site-wide
      // pill is informational; visitors who need real-time data follow
      // the link to the full status page.
      if (cached) setSeverity(cached.severity);
      return;
    }
    (async (): Promise<void> => {
      try {
        const resp = await fetch(statusBadgeUrl, {
          method: "GET",
          headers: { accept: "application/json" },
          credentials: "omit",
          // Cap the fetch — if BetterStack is slow we'd rather render
          // the default than block.
          signal: AbortSignal.timeout(4_000),
        });
        if (!resp.ok) return;
        const payload = (await resp.json()) as BetterStackBadgePayload;
        const next = classify(payload);
        if (cancelled) return;
        setSeverity(next);
        writeCache(statusBadgeUrl, { severity: next, fetchedAt: Date.now() });
      } catch {
        /* Network or CORS error — keep current state (which defaults to
         * "unknown", not "ok" — see the module doc-comment). We do not
         * flip to "down" on a network error, because that would ALSO be
         * an unverified claim; nor do we flip to "ok". */
      }
    })();
    return (): void => {
      cancelled = true;
    };
  }, [statusBadgeUrl]);

  const label =
    severity === "down"
      ? labelDown
      : severity === "degraded"
        ? labelDegraded
        : severity === "ok"
          ? labelOk
          : labelUnknown;

  const severityClass =
    severity === "down"
      ? styles.down
      : severity === "degraded"
        ? styles.degraded
        : severity === "ok"
          ? styles.ok
          : styles.unknown;

  return (
    <a
      className={`${styles.pill} ${severityClass}`}
      href={statuspageUrl}
      target="_blank"
      rel="noopener noreferrer"
      aria-label={`CoreLink status: ${label}. Opens the public status page in a new tab.`}
      title={label}
    >
      <span className={styles.dot} aria-hidden="true" />
      <span className={styles.pillLabel}>{label}</span>
    </a>
  );
}
