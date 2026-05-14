/**
 * Pure SLA countdown helpers (WI-S16-004).
 *
 * Color-coding thresholds:
 *   green  > 14 days
 *   yellow 7..14 days (inclusive lower)
 *   red    < 7 days
 *
 * Spec note: `deadline` is ALWAYS sourced from the backend
 * (`sla_deadline` in the DSR receipt). We never compute it on the client
 * — see WI-S16-004 §"Privacy + security".
 */

export type CountdownColor = "green" | "yellow" | "red" | "overdue";

export interface CountdownState {
  totalMs: number;
  days: number;
  hours: number;
  color: CountdownColor;
  overdue: boolean;
}

export function computeCountdown(
  deadlineIso: string,
  now: Date = new Date(),
): CountdownState {
  const deadline = new Date(deadlineIso).getTime();
  const totalMs = deadline - now.getTime();
  if (Number.isNaN(deadline)) {
    return { totalMs: 0, days: 0, hours: 0, color: "red", overdue: false };
  }
  if (totalMs <= 0) {
    return {
      totalMs,
      days: 0,
      hours: 0,
      color: "overdue",
      overdue: true,
    };
  }
  const days = Math.floor(totalMs / 86_400_000);
  const hours = Math.floor((totalMs % 86_400_000) / 3_600_000);
  return {
    totalMs,
    days,
    hours,
    color: colorFor(days),
    overdue: false,
  };
}

export function colorFor(daysRemaining: number): CountdownColor {
  if (daysRemaining > 14) return "green";
  if (daysRemaining >= 7) return "yellow";
  return "red";
}
