"use client";

import * as React from "react";
import { clearPatPlaintext } from "./actions";

/**
 * CopyPatButton — client component for the one-time PAT reveal panel.
 *
 * Responsibilities:
 *  1. Render the PAT in a code block (server-passed via props — never stored
 *     in localStorage, sessionStorage, or any persistent client state).
 *  2. Copy-to-clipboard button for the PAT value.
 *  3. "I've saved my token" form action that calls the server action
 *     `clearPatPlaintext()` which removes `pat_plaintext` from Clerk metadata
 *     and redirects to /customer.
 *
 * CTRL-CRED-001: PAT is NEVER logged, NEVER persisted beyond this render.
 * The `pat` prop is held only in React render state for the duration of
 * this component's lifetime. No useState needed — it is a static prop.
 */
export interface CopyPatButtonProps {
  /** The one-time PAT plaintext, passed from the Server Component render. */
  pat: string;
}

export function CopyPatButton({ pat }: CopyPatButtonProps): React.ReactElement {
  const [patCopied, setPatCopied] = React.useState(false);
  const [pending, startTransition] = React.useTransition();

  async function copyPat(): Promise<void> {
    try {
      await navigator.clipboard.writeText(pat);
      setPatCopied(true);
      window.setTimeout(() => setPatCopied(false), 2000);
    } catch {
      // Clipboard API can fail on non-secure origins or permission denial.
      // User can manually select and copy the text.
      setPatCopied(false);
    }
  }

  function handleSaved(): void {
    startTransition(async () => {
      await clearPatPlaintext();
    });
  }

  return (
    <div className="copy-pat-panel" data-testid="copy-pat-panel">
      {/* PAT display block */}
      <div className="pat-token-block" data-testid="pat-token-block">
        <pre aria-label="Personal access token" data-testid="pat-token-value">
          <code>{pat}</code>
        </pre>
        <button
          type="button"
          onClick={copyPat}
          data-testid="copy-pat-btn"
          aria-label="Copy personal access token to clipboard"
        >
          {patCopied ? "Copied!" : "Copy"}
        </button>
      </div>

      {/* "I've saved my token" — calls server action, clears PAT, redirects */}
      <div className="pat-saved-action" data-testid="pat-saved-action">
        <button
          type="button"
          onClick={handleSaved}
          disabled={pending}
          data-testid="pat-saved-btn"
          aria-label="Confirm token saved and go to dashboard"
        >
          {pending ? "Saving…" : "I've saved my token — Take me to the dashboard"}
        </button>
      </div>
    </div>
  );
}
