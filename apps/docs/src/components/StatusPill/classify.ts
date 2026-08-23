/**
 * StatusPill classification logic — split out from StatusPill.tsx (which is
 * JSX and cannot be imported from a plain-node vitest run against this
 * repo's `tsconfig.json` `jsx: "preserve"` setting) so the mapping is
 * unit-testable without a React/JSX toolchain in the test runner.
 *
 * See StatusPill.tsx's module doc-comment for the full incident writeup
 * this logic exists to fix: the pill used to render green "All systems
 * operational" on any unrecognised or failed fetch. `classify()` here
 * never returns "ok" unless the payload positively says so.
 */

export type Severity = "ok" | "degraded" | "down" | "unknown";

export interface BetterStackBadgePayload {
  /**
   * The historical badge shape's `status` string: "up" | "down" |
   * "degraded" | "maintenance" | "validating" | "paused". Kept for
   * operator overrides that point at a page still serving this shape.
   */
  status?: string;
  /**
   * Some BetterStack accounts / Atlassian-compatible pages return
   * `status.indicator` instead ("none" | "minor" | "major" | "critical").
   * We map both.
   */
  indicator?: string;
  /**
   * The REAL shape returned by `<statuspageUrl>/index.json` on this
   * BetterStack tenant (verified 2026-08-22):
   *   { "data": { "attributes": { "aggregate_state": "downtime", ... } } }
   * `aggregate_state` values observed: "operational" | "degraded" |
   * "maintenance" | "downtime" | "outage".
   */
  data?: {
    attributes?: {
      aggregate_state?: string;
    };
  };
}

/**
 * Classify a payload into a severity. Returns "unknown" — never "ok" — for
 * anything this function cannot positively identify as a recognised
 * operational state; see StatusPill.tsx's module doc-comment for why an
 * unrecognised payload must not default to "ok".
 */
export function classify(payload: BetterStackBadgePayload): Severity {
  const status = (payload.status ?? "").toLowerCase();
  const indicator = (payload.indicator ?? "").toLowerCase();
  const aggregateState = (
    payload.data?.attributes?.aggregate_state ?? ""
  ).toLowerCase();

  if (
    status === "down" ||
    status === "outage" ||
    indicator === "major" ||
    indicator === "critical" ||
    aggregateState === "downtime" ||
    aggregateState === "outage"
  ) {
    return "down";
  }
  if (
    status === "degraded" ||
    status === "maintenance" ||
    indicator === "minor" ||
    aggregateState === "degraded" ||
    aggregateState === "maintenance"
  ) {
    return "degraded";
  }
  if (status === "up" || indicator === "none" || aggregateState === "operational") {
    return "ok";
  }
  return "unknown";
}
