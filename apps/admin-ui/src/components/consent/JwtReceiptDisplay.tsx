"use client";

import { useMemo, useState } from "react";
import { decodeJwtReceipt } from "@/lib/jwt-decode";

export interface JwtReceiptDisplayProps {
  jwt: string;
}

export function JwtReceiptDisplay({ jwt }: JwtReceiptDisplayProps) {
  const [copied, setCopied] = useState(false);
  const decoded = useMemo(() => {
    try {
      return decodeJwtReceipt(jwt);
    } catch {
      return null;
    }
  }, [jwt]);

  if (!decoded) {
    return (
      <div role="alert" data-testid="jwt-decode-error">
        Receipt could not be decoded.
      </div>
    );
  }

  const { payload } = decoded;

  async function copyToClipboard() {
    try {
      await navigator.clipboard.writeText(jwt);
      setCopied(true);
    } catch {
      setCopied(false);
    }
  }

  return (
    <div data-testid="jwt-receipt" aria-label="JWT consent receipt">
      <dl>
        <div>
          <dt>tenant_id</dt>
          <dd data-testid="claim-tenant_id">{payload.tenant_id}</dd>
        </div>
        <div>
          <dt>consent_id</dt>
          <dd data-testid="claim-consent_id">{payload.consent_id}</dd>
        </div>
        <div>
          <dt>granted_at</dt>
          <dd data-testid="claim-granted_at">{payload.granted_at}</dd>
        </div>
        <div>
          <dt>locale</dt>
          <dd data-testid="claim-locale">{payload.locale}</dd>
        </div>
        <div>
          <dt>jti</dt>
          <dd data-testid="claim-jti">{payload.jti}</dd>
        </div>
        <div>
          <dt>exp</dt>
          <dd data-testid="claim-exp">{payload.exp}</dd>
        </div>
      </dl>
      <button
        type="button"
        onClick={copyToClipboard}
        aria-label="Copy receipt JWT to clipboard"
        data-testid="copy-receipt-btn"
      >
        {copied ? "Copied" : "Copy receipt"}
      </button>
    </div>
  );
}
