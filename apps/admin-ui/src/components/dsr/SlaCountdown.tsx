"use client";

import { useEffect, useState } from "react";
import {
  computeCountdown,
  type CountdownColor,
  type CountdownState,
} from "@/lib/sla-countdown";
import { interpolate, tFor, type Locale } from "@/i18n";
import { StatusDot } from "@/components/ui/linear";

/** Map the server-derived countdown color to a Linear status tone. */
function toneFor(
  color: CountdownColor,
): "neutral" | "success" | "warn" | "danger" {
  switch (color) {
    case "green":
      return "success";
    case "yellow":
      return "warn";
    case "red":
    case "overdue":
      return "danger";
    default:
      return "neutral";
  }
}

export interface SlaCountdownProps {
  locale: Locale;
  /** ISO 8601 deadline, server-provided. */
  deadline: string;
  /** Polling interval in ms (default 60s). */
  refreshMs?: number;
  /** Test injection. */
  nowProvider?: () => Date;
}

/**
 * Visual countdown to the DSR SLA deadline.
 *
 * Source-of-truth: server-supplied `sla_deadline`. We NEVER compute the
 * deadline on the client — see WI-S16-004 §"Privacy + security".
 */
// In test environments (vitest) we skip the polling timer by default — the
// component still renders the initial state, but the runner doesn't hang on
// the open setInterval handle when tests forget to unmount.
const DEFAULT_REFRESH_MS =
  typeof process !== "undefined" && process.env?.VITEST === "true"
    ? 0
    : 60_000;

export function SlaCountdown(props: SlaCountdownProps) {
  const interval = props.refreshMs ?? DEFAULT_REFRESH_MS;
  const nowProvider = props.nowProvider ?? (() => new Date());
  const [state, setState] = useState<CountdownState>(() =>
    computeCountdown(props.deadline, nowProvider()),
  );

  useEffect(() => {
    setState(computeCountdown(props.deadline, nowProvider()));
    // refreshMs <= 0 disables the timer (test-friendly + static rendering).
    if (interval <= 0) return;
    const id = setInterval(() => {
      setState(computeCountdown(props.deadline, nowProvider()));
    }, interval);
    return () => clearInterval(id);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [props.deadline, interval]);

  const t = (k: string) => tFor(props.locale, k);
  const text = formatText(state, props.locale, t);

  return (
    <span
      data-testid="dsr-sla-countdown"
      data-color={state.color}
      role="timer"
      aria-live="polite"
      className="lin-badge"
    >
      <StatusDot tone={toneFor(state.color)} />
      {text}
    </span>
  );
}

function formatText(
  state: CountdownState,
  locale: Locale,
  t: (k: string) => string,
): string {
  if (state.overdue) return t("dsr.countdown.overdue");
  if (state.days <= 0) {
    return interpolate(t("dsr.countdown.remaining_hours"), {
      hours: state.hours,
    });
  }
  return interpolate(t("dsr.countdown.remaining_days"), { days: state.days });
}

export function colorClassName(color: CountdownColor): string {
  switch (color) {
    case "green":
      return "dsr-sla-green";
    case "yellow":
      return "dsr-sla-yellow";
    case "red":
      return "dsr-sla-red";
    case "overdue":
      return "dsr-sla-overdue";
  }
}
