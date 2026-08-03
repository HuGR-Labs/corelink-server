"use client";

import * as React from "react";
import { Callout, CodeBlock } from "@/components/ui/linear";
import {
  ActivationStateBadge,
  type ActivationState,
} from "@/components/ActivationStateBadge";
import { withAppBasePath } from "@/lib/route-matcher";

/**
 * WelcomeStream — client component that subscribes to the
 * `/corelink/api/welcome/stream` SSE endpoint (the basePath is re-attached via
 * `withAppBasePath`) and projects the inbound `analytics_events` into a single
 * `ActivationState` value.
 *
 * Event mapping (per PLG framework §7.1):
 *   - `welcome_view`        → no-op (just confirms session is wired)
 *   - `first_cli_authed`    → "cli-authed"
 *   - `first_cache_hit`     → "activated"
 *
 * While waiting for CLI auth, renders install instructions and the
 * `corelink whoami` verify step so the user knows what to do next.
 *
 * The stream is filtered server-side by the caller's `tenant_id` (resolved
 * from the Clerk session in the route handler), so this component does not
 * need to do any access control.
 *
 * Linear kit: the status chip is `ActivationStateBadge`, terminal blocks are
 * the frozen `CodeBlock`, and the live status lines use kit `Callout`s — text
 * uses `--t1`/`--t2` tokens only.
 */
export function WelcomeStream(props: {
  streamUrl?: string;
}): React.ReactElement {
  // Re-attach the surface's `/corelink` basePath. `EventSource` resolves its
  // URL against the document origin with NO framework involvement, so a bare
  // `/api/welcome/stream` connects to the apex `humangr.com` — the hugr-site
  // MARKETING app. That surface answers `200 text/html`, so the EventSource
  // opens successfully against HTML and then never delivers an event: the
  // activation pane waits forever and merely looks empty. Silent, not loud.
  const url = props.streamUrl ?? withAppBasePath("/api/welcome/stream");
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
    <div data-testid="welcome-stream" className="lin-checklist">
      <ActivationStateBadge state={state} />

      {state === "waiting" ? (
        <div data-testid="welcome-stream-instructions" className="lin-checklist">
          <p>Run this in your terminal to connect:</p>
          <CodeBlock code="corelink whoami" lang="bash" />
          <p className="lin-card__meta">
            Waiting for the CLI to authenticate… this updates live.
          </p>
        </div>
      ) : null}

      {state === "cli-authed" ? (
        <div data-testid="welcome-stream-cli-authed" className="lin-checklist">
          <Callout tone="info">
            <strong>CLI authenticated.</strong> Now run your first build.
          </Callout>
          <CodeBlock code="bazel build //..." lang="bash" />
        </div>
      ) : null}

      {state === "activated" ? (
        <div data-testid="welcome-stream-activated">
          <Callout tone="info">
            <strong>Cache is active</strong> — your first cache hit was recorded.
          </Callout>
        </div>
      ) : null}

      {error ? (
        <p
          role="status"
          data-testid="welcome-stream-error"
          className="lin-card__meta"
        >
          {error}
        </p>
      ) : null}
    </div>
  );
}
