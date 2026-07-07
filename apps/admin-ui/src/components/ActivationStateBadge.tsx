"use client";

import * as React from "react";
import { Badge } from "@/components/ui/linear";

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
 *
 * Linear kit: rendered with the frozen `Badge` (dot variant) so the status
 * colour comes only from the a11y-validated semantic dot tokens, never hue
 * decoration. The status wrapper keeps `role="status"`/`aria-live` + the
 * testids the welcome-flow e2e specs assert on.
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

const TONE: Record<ActivationState, "neutral" | "warn" | "success"> = {
  waiting: "neutral",
  "cli-authed": "warn",
  activated: "success",
};

export function ActivationStateBadge(
  props: ActivationStateBadgeProps,
): React.ReactElement {
  const label = props.labels?.[props.state] ?? DEFAULT_LABELS[props.state];
  return (
    <span
      data-testid="activation-state-badge"
      data-state={props.state}
      role="status"
      aria-live="polite"
    >
      <span data-testid="activation-state-label">
        <Badge tone={TONE[props.state]} dot>
          <span data-testid="activation-state-icon" data-state={props.state}>
            {label}
          </span>
        </Badge>
      </span>
    </span>
  );
}
