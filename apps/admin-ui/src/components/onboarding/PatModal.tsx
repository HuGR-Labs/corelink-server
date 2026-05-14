"use client";

import * as React from "react";
import {
  clearPlaintextPat,
  getPlaintextPat,
} from "@/lib/onboarding-state";

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
    await impl(pat);
    setCopied(true);
  }, [pat, props.copyImpl]);

  if (!props.open) return null;

  return (
    <div role="dialog" aria-modal="true" aria-labelledby="pat-modal-title">
      <h2 id="pat-modal-title">{props.labels.title}</h2>
      <p data-testid="pat-warning">{props.labels.warning}</p>
      <pre data-testid="pat-value" aria-label="personal access token">
        {pat ?? ""}
      </pre>
      <button type="button" onClick={handleCopy} data-testid="pat-copy">
        {props.labels.copy}
      </button>
      <label>
        <input
          type="checkbox"
          checked={confirmed}
          onChange={(e) => setConfirmed(e.target.checked)}
          data-testid="pat-confirm-checkbox"
        />
        {props.labels.confirmSaved}
      </label>
      <button
        type="button"
        disabled={!confirmed || !copied}
        onClick={() => {
          // Zero the in-memory PAT before navigating away.
          clearPlaintextPat();
          props.onConfirm();
        }}
        data-testid="pat-finish"
      >
        {props.labels.finish}
      </button>
    </div>
  );
}
