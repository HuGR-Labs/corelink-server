"use client";

import * as React from "react";
import {
  ActivationStateBadge,
  type ActivationState,
} from "@/components/ActivationStateBadge";

/**
 * WelcomeStream — client component that subscribes to the `/api/welcome/stream`
 * SSE endpoint and projects the inbound `analytics_events` into a single
 * `ActivationState` value.
 *
 * Event mapping (per PLG framework §7.1):
 *   - `welcome_view`        → no-op (just confirms session is wired)
 *   - `first_cli_authed`    → "cli-authed"
 *   - `first_cache_hit`     → "activated"
 *
 * The stream is filtered server-side by the caller's `tenant_id` (resolved
 * from the Clerk session in the route handler), so this component does not
 * need to do any access control.
 */
export function WelcomeStream(props: {
  streamUrl?: string;
}): React.ReactElement {
  const url = props.streamUrl ?? "/api/welcome/stream";
  const [state, setState] = React.useState<ActivationState>("waiting");
  const [error, setError] = React.useState<string | null>(null);

  React.useEffect(() => {
    let source: EventSource | null = null;
    try {
      source = new EventSource(url, { withCredentials: true });
    } catch (err) {
      setError((err as Error).message);
      return undefined;
    }

    function handle(evt: MessageEvent): void {
      try {
        const parsed = JSON.parse(evt.data) as { event_name?: string };
        if (parsed.event_name === "first_cache_hit") {
          setState("activated");
        } else if (parsed.event_name === "first_cli_authed") {
          setState((prev) => (prev === "activated" ? prev : "cli-authed"));
        }
      } catch {
        // Ignore non-JSON keepalive messages — server may send `: ping`.
      }
    }

    source.onmessage = handle;
    source.addEventListener("first_cli_authed", handle as EventListener);
    source.addEventListener("first_cache_hit", handle as EventListener);
    source.onerror = () => {
      // Browser auto-retries SSE; surface only persistent failures via state.
      setError("Reconnecting…");
    };

    return () => {
      source?.close();
    };
  }, [url]);

  return (
    <div data-testid="welcome-stream">
      <ActivationStateBadge state={state} />
      {error ? (
        <p role="status" data-testid="welcome-stream-error">
          {error}
        </p>
      ) : null}
    </div>
  );
}
