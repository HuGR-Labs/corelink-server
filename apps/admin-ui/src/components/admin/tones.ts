// Shared Linear-tone mappings for admin surfaces. Keeps severity/status → Badge
// tone in one place so the audit table and tenant deep-dive stay visually
// consistent with the frozen kit (no ad-hoc color).

import type { Severity } from "@/lib/types";

export type Tone = "neutral" | "success" | "warn" | "danger";

/** Audit-event severity → Badge/StatusDot tone. */
export function severityTone(severity: Severity | string): Tone {
  switch (severity) {
    case "critical":
      return "danger";
    case "warn":
      return "warn";
    case "info":
    default:
      return "neutral";
  }
}

/** BYOK status → Badge tone. */
export function byokTone(status: string): Tone {
  switch (status) {
    case "active":
      return "success";
    case "pending":
      return "warn";
    case "revoked":
    case "error":
      return "danger";
    default:
      return "neutral";
  }
}
