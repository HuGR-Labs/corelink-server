"use client";

import * as React from "react";

/**
 * ActivationStateBadge — three-state progress chip that the `/welcome` SSE
 * stream drives in real-time as the user advances through the activation
 * funnel defined in PLG framework §3.2 + §7.1.
 *
 * States:
 *   "waiting"    — server has not yet seen first authenticated `/v1/ping`
 *                  (no `first_cli_authed` event in `analytics_events`).
 *   "cli-authed" — server saw `first_cli_authed`, awaiting `first_cache_hit`.
 *   "activated"  — `first_cache_hit` landed — funnel complete.
 *
 * The badge is purely presentational; the parent `WelcomeStream` owns the
 * SSE subscription and re-renders this component with each state change.
 */

export type ActivationState = "waiting" | "cli-authed" | "activated";

export interface ActivationStateBadgeProps {
  state: ActivationState;
  /** Optional override for screen-reader / test text. */
  labels?: Partial<Record<ActivationState, string>>;
}

const DEFAULT_LABELS: Record<ActivationState, string> = {
  waiting: "Waiting for first CLI call",
  "cli-authed": "CLI authenticated — run a build twice for first cache hit",
  activated: "Activated — first cache hit recorded",
};

export function ActivationStateBadge(
  props: ActivationStateBadgeProps,
): React.ReactElement {
  const label = props.labels?.[props.state] ?? DEFAULT_LABELS[props.state];
  return (
    <div
      className={`activation-badge activation-badge--${props.state}`}
      data-testid="activation-state-badge"
      data-state={props.state}
      role="status"
      aria-live="polite"
    >
      <span
        aria-hidden="true"
        data-testid="activation-state-icon"
        data-state={props.state}
      >
        {props.state === "activated" ? "✓" : props.state === "cli-authed" ? "•" : "…"}
      </span>
      <span data-testid="activation-state-label">{label}</span>
    </div>
  );
}
