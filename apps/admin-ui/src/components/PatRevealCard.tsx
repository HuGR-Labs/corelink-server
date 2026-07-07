"use client";

import * as React from "react";
import { Callout } from "@/components/ui/linear";

/**
 * PatRevealCard — one-time PAT display widget (CTRL-CRED-001).
 *
 * Renders the PAT redacted by default (masked to the last 4 chars via the kit
 * `CopyField redactAs`). The user must explicitly click "Reveal" to see the
 * plaintext. After 60 s the redaction is reinstated automatically.
 *
 * Copy-to-clipboard works in either state — the kit `CopyField` always writes
 * the raw `value`, independent of what is displayed, so copy works whether the
 * token is revealed or masked.
 *
 * Security invariants (CTRL-CRED-001):
 *   - `patPlaintext` is NEVER written to localStorage or sessionStorage.
 *   - `patPlaintext` is NEVER logged (no console.log, no analytics call).
 *   - The prop value lives only in React render state for the component lifetime.
 *
 * Linear kit: warn `Callout` for the "shown once" notice + `CopyField`
 * (redaction-safe) for the token row + a kit ghost button for reveal/hide.
 * No inline styles, no hue decoration — the mask is the kit's own affordance.
 */
export interface PatRevealCardProps {
  /** The one-time PAT plaintext, passed from the Server Component render. */
  patPlaintext: string;
}

/** Auto-hide delay in milliseconds after the user reveals the token. */
const AUTO_HIDE_MS = 60_000;
/** "Copied!" feedback duration. */
const COPIED_FEEDBACK_MS = 1_500;

export function PatRevealCard({
  patPlaintext,
}: PatRevealCardProps): React.ReactElement {
  const [revealed, setRevealed] = React.useState(false);
  const [copied, setCopied] = React.useState(false);

  // Auto-hide + copied-feedback timers — cancelled on unmount.
  const timerRef = React.useRef<ReturnType<typeof setTimeout> | null>(null);
  const copiedTimerRef = React.useRef<ReturnType<typeof setTimeout> | null>(null);

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

  // Copy the RAW plaintext (independent of the masked display), then flash
  // "Copied!". The plaintext is read only from the render-state prop — never
  // stored, never logged (CTRL-CRED-001).
  async function copy(): Promise<void> {
    await navigator.clipboard.writeText(patPlaintext);
    setCopied(true);
    if (copiedTimerRef.current !== null) {
      clearTimeout(copiedTimerRef.current);
    }
    copiedTimerRef.current = setTimeout(() => {
      setCopied(false);
    }, COPIED_FEEDBACK_MS);
  }

  // Clean up timers when the component unmounts.
  React.useEffect(() => {
    return () => {
      if (timerRef.current !== null) {
        clearTimeout(timerRef.current);
      }
      if (copiedTimerRef.current !== null) {
        clearTimeout(copiedTimerRef.current);
      }
    };
  }, []);

  // When redacted, show only the last 4 chars behind a mask prefix.
  const maskedDisplay = `••••••••${patPlaintext.slice(-4)}`;

  return (
    <div className="lin-checklist" data-testid="pat-reveal-card">
      {/* Warning banner — kit warn Callout, semantic tone only. */}
      <div role="alert" data-testid="pat-reveal-warning">
        <Callout tone="warn">
          Your personal access token is shown <strong>once</strong>. Copy it now
          and store it in a secret manager — you cannot retrieve it again from
          this screen.
        </Callout>
      </div>

      {/* Token row — Linear `.lin-copy` styling. The display is masked until the
          user reveals; copy always writes the raw value regardless of display. */}
      <div className="lin-copy" data-testid="pat-reveal-token-row">
        <span className="lin-copy__val" data-testid="pat-reveal-token-display">
          {revealed ? patPlaintext : maskedDisplay}
        </span>
        <button
          type="button"
          className="lin-copy__btn"
          onClick={() => {
            void copy();
          }}
          data-testid="pat-reveal-copy"
          aria-label="Copy personal access token"
        >
          {copied ? "Copied!" : "Copy"}
        </button>
      </div>

      {/* Reveal / Hide toggle. */}
      <div>
        <button
          type="button"
          onClick={revealed ? hide : reveal}
          data-testid="pat-reveal-toggle"
          aria-label={
            revealed
              ? "Hide personal access token"
              : "Reveal personal access token"
          }
          className="lin-btn lin-btn--ghost lin-btn--sm"
        >
          {revealed ? "Hide" : "Reveal"}
        </button>
      </div>

      {/* Countdown hint — shown only while revealed. */}
      {revealed ? (
        <div className="lin-card__meta" data-testid="pat-reveal-countdown-hint">
          Token will be hidden automatically after 60 s.
        </div>
      ) : null}
    </div>
  );
}
