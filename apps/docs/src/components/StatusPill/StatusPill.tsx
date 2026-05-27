/**
 * StatusPill — small operational-status badge for the docs site navbar.
 *
 * Per the channels audit: showing a live "all systems operational" pill
 * signals operational legitimacy to docs visitors and gives a low-friction
 * path to the public status page when something is wrong.
 *
 * Data source: BetterStack public status JSON. BetterStack exposes each
 * public status page as JSON at `<page-url>/badge.json` (and as the legacy
 * Atlassian-compatible `<page-url>/api/v2/status.json`). We default to
 * the `badge.json` shape because it is the canonical BetterStack endpoint
 * for this purpose. The URL is overridable via the
 * `statusBadgeUrl` site-config customField so operators can rewire to a
 * different status page (e.g. white-label) without touching code.
 *
 * Cache: 30 minutes. The pill stores the last successful fetch in
 * `sessionStorage` keyed by URL, plus a stale-while-revalidate fetch on
 * mount when the cache is older than 30 minutes. The site-wide pill is
 * NOT a real-time monitor — for that, visitors follow the link to the
 * full BetterStack status page (which has its own auto-refresh).
 *
 * Graceful degradation:
 *   - Cold render (no cache, no network response yet) → "operational"
 *     pill, neutral grey dot. This avoids a layout flicker and is
 *     consistent with the BetterStack badge's own "assume good" default.
 *   - Fetch failure → keep the neutral default. We do NOT show "down"
 *     on a CORS/network error, because that would be a false positive
 *     and undermine the trust signal.
 *
 * State machine (post-fetch):
 *   - status === "operational"  → ok (green)
 *   - status === "degraded" / "maintenance" → degraded (yellow)
 *   - status === "downtime" / "outage" / "major_outage" → down (red)
 *   - any other / unknown / fetch failure → ok (green, "All systems
 *     operational" label) — assume good when we can't prove otherwise.
 *
 * Privacy: the component does not send any user data to BetterStack;
 * it just GETs a static JSON URL. No cookies, no fingerprint.
 */

import { useEffect, useState, type ReactElement } from "react";
import styles from "./StatusPill.module.css";

const CACHE_TTL_MS = 30 * 60 * 1000;
const STORAGE_PREFIX = "corelink:statuspill:v1:";
const DEFAULT_LABEL_OK = "All systems operational";
const DEFAULT_LABEL_DEGRADED = "Degraded performance";
const DEFAULT_LABEL_DOWN = "Active incident";

type Severity = "ok" | "degraded" | "down";

export interface StatusPillProps {
  /**
   * Public status-page URL (where the pill links to). This is the URL a
   * visitor sees; it should be the human-facing status page, not the
   * JSON badge endpoint.
   */
  readonly statuspageUrl: string;
  /**
   * BetterStack JSON badge endpoint. Defaults to
   * `<statuspageUrl>/badge.json`.
   */
  readonly statusBadgeUrl?: string;
  /** Optional translated labels per severity. */
  readonly labelOk?: string;
  readonly labelDegraded?: string;
  readonly labelDown?: string;
}

interface CachedStatus {
  severity: Severity;
  fetchedAt: number;
}

interface BetterStackBadgePayload {
  /**
   * BetterStack's `badge.json` returns a `status` string. The historical
   * values observed are: "up" | "down" | "degraded" | "maintenance" |
   * "validating" | "paused". We treat unknown values as "ok" — see the
   * module-level rationale.
   */
  status?: string;
  /**
   * Some BetterStack accounts return the older Atlassian-compatible
   * `status.indicator` field instead ("none" | "minor" | "major" |
   * "critical"). We map both.
   */
  indicator?: string;
}

function classify(payload: BetterStackBadgePayload): Severity {
  const status = (payload.status ?? "").toLowerCase();
  const indicator = (payload.indicator ?? "").toLowerCase();
  if (
    status === "down" ||
    status === "outage" ||
    indicator === "major" ||
    indicator === "critical"
  ) {
    return "down";
  }
  if (
    status === "degraded" ||
    status === "maintenance" ||
    indicator === "minor"
  ) {
    return "degraded";
  }
  return "ok";
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
      parsed.severity === "down"
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
    statusBadgeUrl = `${statuspageUrl.replace(/\/+$/, "")}/badge.json`,
    labelOk = DEFAULT_LABEL_OK,
    labelDegraded = DEFAULT_LABEL_DEGRADED,
    labelDown = DEFAULT_LABEL_DOWN,
  } = props;

  // Hydrate from cache synchronously so the first paint matches the cached
  // severity (avoids a green→red flash if there's an ongoing incident the
  // visitor just saw a moment ago).
  const [severity, setSeverity] = useState<Severity>(() => {
    const cached = readCache(statusBadgeUrl);
    return cached?.severity ?? "ok";
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
        /* Network or CORS error — keep current state, do not flip to "down". */
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
        : labelOk;

  const severityClass =
    severity === "down"
      ? styles.down
      : severity === "degraded"
        ? styles.degraded
        : styles.ok;

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
