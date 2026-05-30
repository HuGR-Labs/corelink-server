"use client";

import * as React from "react";

/**
 * PatRevealCard — one-time PAT display widget (CTRL-CRED-001).
 *
 * Renders the PAT blurred by default. The user must explicitly click "Reveal"
 * to see the plaintext. After 60 s the blur is reinstated automatically.
 *
 * Copy-to-clipboard works in either state (blurred or revealed) — the button
 * writes the raw value without requiring the blur to be lifted first.
 *
 * Security invariants (CTRL-CRED-001):
 *   - `patPlaintext` is NEVER written to localStorage or sessionStorage.
 *   - `patPlaintext` is NEVER logged (no console.log, no analytics call).
 *   - The prop value lives only in React render state for the component lifetime.
 */
export interface PatRevealCardProps {
  /** The one-time PAT plaintext, passed from the Server Component render. */
  patPlaintext: string;
}

/** Auto-hide delay in milliseconds after the user reveals the token. */
const AUTO_HIDE_MS = 60_000;

export function PatRevealCard({
  patPlaintext,
}: PatRevealCardProps): React.ReactElement {
  const [revealed, setRevealed] = React.useState(false);
  const [copied, setCopied] = React.useState(false);

  // Auto-hide timer — cancelled if the component unmounts.
  const timerRef = React.useRef<ReturnType<typeof setTimeout> | null>(null);

  function reveal(): void {
    setRevealed(true);
    if (timerRef.current !== null) {
      clearTimeout(timerRef.current);
    }
    timerRef.current = setTimeout(() => {
      setRevealed(false);
    }, AUTO_HIDE_MS);
  }

  function hide(): void {
    setRevealed(false);
    if (timerRef.current !== null) {
      clearTimeout(timerRef.current);
      timerRef.current = null;
    }
  }

  // Clean up the auto-hide timer when the component unmounts.
  React.useEffect(() => {
    return () => {
      if (timerRef.current !== null) {
        clearTimeout(timerRef.current);
      }
    };
  }, []);

  async function copyPat(): Promise<void> {
    try {
      await navigator.clipboard.writeText(patPlaintext);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      // Clipboard API can fail on non-secure origins or permission denial.
      // The user can manually select and copy from the revealed text.
      setCopied(false);
    }
  }

  // When blurred, show only the last 4 chars with a mask prefix.
  const maskedDisplay = `••••••••${patPlaintext.slice(-4)}`;

  return (
    <div
      className="rounded border border-gray-200 bg-white p-4 shadow-sm"
      data-testid="pat-reveal-card"
    >
      {/* Warning banner */}
      <div
        className="mb-3 rounded border border-amber-300 bg-amber-50 px-3 py-2 text-sm"
        role="alert"
        data-testid="pat-reveal-warning"
      >
        Your personal access token is shown <strong>once</strong>. Copy it now
        and store it in a secret manager — you cannot retrieve it again from
        this screen.
      </div>

      {/* Token display */}
      <div
        className="relative flex items-center gap-2 rounded bg-gray-50 px-3 py-2 font-mono text-sm"
        data-testid="pat-reveal-token-row"
      >
        <span
          aria-label="Personal access token"
          data-testid="pat-reveal-token-display"
          style={revealed ? undefined : { filter: "blur(4px)", userSelect: "none" }}
        >
          {revealed ? patPlaintext : maskedDisplay}
        </span>

        <div className="ml-auto flex items-center gap-2">
          {/* Reveal / Hide toggle */}
          <button
            type="button"
            onClick={revealed ? hide : reveal}
            data-testid="pat-reveal-toggle"
            aria-label={revealed ? "Hide personal access token" : "Reveal personal access token"}
            className="rounded px-2 py-1 text-xs text-blue-600 underline hover:text-blue-800"
          >
            {revealed ? "Hide" : "Reveal"}
          </button>

          {/* Copy button — works regardless of reveal state */}
          <button
            type="button"
            onClick={copyPat}
            data-testid="pat-reveal-copy"
            aria-label="Copy personal access token to clipboard"
            className="rounded bg-gray-200 px-2 py-1 text-xs hover:bg-gray-300"
          >
            {copied ? "Copied!" : "Copy"}
          </button>
        </div>
      </div>

      {/* Countdown hint — shown only while revealed */}
      {revealed ? (
        <p
          className="mt-2 text-xs text-gray-500"
          data-testid="pat-reveal-countdown-hint"
        >
          Token will be hidden automatically after 60 s.
        </p>
      ) : null}
    </div>
  );
}
