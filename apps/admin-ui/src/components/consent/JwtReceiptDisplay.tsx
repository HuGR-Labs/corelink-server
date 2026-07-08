"use client";

import { useMemo, useState } from "react";
import { decodeJwtReceipt } from "@/lib/jwt-decode";
import { Button, Callout, Card } from "@/components/ui/linear";

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
        <Callout tone="danger">Receipt could not be decoded.</Callout>
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
    <Card title="Signed receipt" meta="Cryptographic proof of this consent record.">
      <div
        data-testid="jwt-receipt"
        aria-label="JWT consent receipt"
        className="lin-checklist"
      >
        <dl>
          <div>
            <dt>tenant_id</dt>
            <dd data-testid="claim-tenant_id">
              <code>{payload.tenant_id}</code>
            </dd>
          </div>
          <div>
            <dt>consent_id</dt>
            <dd data-testid="claim-consent_id">
              <code>{payload.consent_id}</code>
            </dd>
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
            <dd data-testid="claim-jti">
              <code>{payload.jti}</code>
            </dd>
          </div>
          <div>
            <dt>exp</dt>
            <dd data-testid="claim-exp">{payload.exp}</dd>
          </div>
        </dl>
        <div>
          <Button
            variant="ghost"
            onClick={copyToClipboard}
            aria-label="Copy receipt JWT to clipboard"
            data-testid="copy-receipt-btn"
          >
            {copied ? "Copied" : "Copy receipt"}
          </Button>
        </div>
      </div>
    </Card>
  );
}
