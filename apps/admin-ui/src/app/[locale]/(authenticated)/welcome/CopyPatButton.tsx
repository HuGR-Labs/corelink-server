"use client";

import * as React from "react";
import { Button } from "@/components/ui/linear";
import { clearPatPlaintext } from "./actions";

/** "Copied!" feedback duration. */
const COPIED_FEEDBACK_MS = 1_500;

/**
 * CopyPatButton — client component for the one-time PAT reveal panel.
 *
 * Responsibilities:
 *  1. Render the PAT in a copy-safe field (server-passed via props — never
 *     stored in localStorage, sessionStorage, or any persistent client state).
 *  2. Copy-to-clipboard for the PAT value (kit `CopyField`).
 *  3. "I've saved my token" action that calls the server action
 *     `clearPatPlaintext()` which removes `pat_plaintext` from Clerk metadata
 *     and redirects to /customer.
 *
 * CTRL-CRED-001: PAT is NEVER logged, NEVER persisted beyond this render.
 * The `pat` prop is held only in React render state for the duration of
 * this component's lifetime.
 *
 * Linear kit: `CopyField` for the token + kit `Button` (primary) for the
 * confirm action. Testids preserved.
 */
export interface CopyPatButtonProps {
  /** The one-time PAT plaintext, passed from the Server Component render. */
  pat: string;
}

export function CopyPatButton({ pat }: CopyPatButtonProps): React.ReactElement {
  const [pending, startTransition] = React.useTransition();
  const [copied, setCopied] = React.useState(false);
  const copiedTimerRef = React.useRef<ReturnType<typeof setTimeout> | null>(null);

  function handleSaved(): void {
    startTransition(async () => {
      await clearPatPlaintext();
    });
  }

  // Copy the raw PAT (render-state only — never stored/logged, CTRL-CRED-001),
  // then flash "Copied!".
  async function copy(): Promise<void> {
    await navigator.clipboard.writeText(pat);
    setCopied(true);
    if (copiedTimerRef.current !== null) {
      clearTimeout(copiedTimerRef.current);
    }
    copiedTimerRef.current = setTimeout(() => {
      setCopied(false);
    }, COPIED_FEEDBACK_MS);
  }

  React.useEffect(() => {
    return () => {
      if (copiedTimerRef.current !== null) {
        clearTimeout(copiedTimerRef.current);
      }
    };
  }, []);

  return (
    <div className="lin-checklist" data-testid="copy-pat-panel">
      {/* PAT display + copy — Linear `.lin-copy` styling, testid contract preserved. */}
      <div className="lin-copy" data-testid="pat-token-block">
        <span className="lin-copy__val" data-testid="pat-token-value">
          {pat}
        </span>
        <button
          type="button"
          className="lin-copy__btn"
          onClick={() => {
            void copy();
          }}
          data-testid="copy-pat-btn"
          aria-label="Copy personal access token"
        >
          {copied ? "Copied!" : "Copy"}
        </button>
      </div>

      {/* "I've saved my token" — calls server action, clears PAT, redirects */}
      <div data-testid="pat-saved-action">
        <Button
          onClick={handleSaved}
          disabled={pending}
          loading={pending}
          data-testid="pat-saved-btn"
          aria-label="Confirm token saved and go to dashboard"
        >
          {pending
            ? "Saving…"
            : "I've saved my token — Take me to the dashboard"}
        </Button>
      </div>
    </div>
  );
}
