"use client";

import * as React from "react";
import { Button, Callout } from "@/components/ui/linear";
import {
  clearPlaintextPat,
  getPlaintextPat,
} from "@/lib/onboarding-state";

/**
 * PatModal — one-time minted-PAT reveal dialog (copy + shown-once + confirm).
 *
 * UI migrated to the Linear design language (frozen kit + globals.css tokens),
 * matching the customer dashboard it is embedded in (KeysClient). Behaviour +
 * contract are preserved exactly: the PAT is read from the in-memory holder on
 * each render (never React state, never storage), the finish button stays
 * disabled until the token is both copied AND the "saved" box is checked, and
 * the injectable `copyImpl` is honoured for tests. All testids are unchanged.
 */
export interface PatModalProps {
  open: boolean;
  /** Called once user confirms they've stored the PAT safely. */
  onConfirm: () => void;
  labels: {
    title: string;
    warning: string;
    copy: string;
    confirmSaved: string;
    finish: string;
  };
  /** Override for tests; defaults to navigator.clipboard.writeText. */
  copyImpl?: (text: string) => Promise<void>;
}

export function PatModal(props: PatModalProps): React.ReactElement | null {
  const [confirmed, setConfirmed] = React.useState(false);
  const [copied, setCopied] = React.useState(false);

  // Read the PAT from in-memory holder each render. We deliberately do
  // NOT store it in React state so it cannot leak via devtools history.
  const pat = props.open ? getPlaintextPat() : null;

  const handleCopy = React.useCallback(async () => {
    if (!pat) return;
    const impl =
      props.copyImpl ??
      ((text: string) => navigator.clipboard.writeText(text));
    try {
      await impl(pat);
    } catch {
      // The clipboard write can be blocked — WebKit/Firefox reject the
      // chromium-only clipboard permission, and any engine can deny the write
      // outside a trusted gesture. The token is shown on screen for manual
      // copy, so a blocked write must NOT trap the user in the shown-once
      // modal: the click of "Copy" is itself the acknowledgement that (with the
      // saved-checkbox) gates "Done". Best-effort copy, guaranteed progress.
    }
    setCopied(true);
  }, [pat, props.copyImpl]);

  if (!props.open) return null;

  return (
    <div className="lin-scrim">
      <div
        className="lin-modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="pat-modal-title"
      >
        <h2 id="pat-modal-title" className="lin-modal__title">
          {props.labels.title}
        </h2>

        <div className="lin-modal__body lin-checklist">
          <div data-testid="pat-warning">
            <Callout tone="warn">{props.labels.warning}</Callout>
          </div>

          <div className="lin-copy">
            <span
              className="lin-copy__val"
              data-testid="pat-value"
              aria-label="personal access token"
            >
              {pat ?? ""}
            </span>
            <button
              type="button"
              className="lin-copy__btn"
              onClick={handleCopy}
              data-testid="pat-copy"
            >
              {props.labels.copy}
            </button>
          </div>

          <label className="lin-label">
            <input
              type="checkbox"
              checked={confirmed}
              onChange={(e) => setConfirmed(e.target.checked)}
              data-testid="pat-confirm-checkbox"
            />{" "}
            {props.labels.confirmSaved}
          </label>
        </div>

        <div className="lin-modal__actions">
          <Button
            disabled={!confirmed || !copied}
            onClick={() => {
              // Zero the in-memory PAT before navigating away.
              clearPlaintextPat();
              props.onConfirm();
            }}
            data-testid="pat-finish"
          >
            {props.labels.finish}
          </Button>
        </div>
      </div>
    </div>
  );
}
