"use client";

import { useState } from "react";
import type { ConsentApi } from "@/lib/consent-api";
import { defaultConsentApi } from "@/lib/consent-api";
import { JwtReceiptDisplay } from "@/components/consent/JwtReceiptDisplay";
import {
  Badge,
  Button,
  Callout,
  Card,
  Field,
  Textarea,
} from "@/components/ui/linear";

export interface ClerkMfaClient {
  session: {
    startVerification: (opts: { strategy: "totp" | "phone_code" }) => Promise<{ verified: boolean }>;
  };
}

export interface WithdrawFormProps {
  consentId: string;
  api?: ConsentApi;
  /** Injectable Clerk client so the MFA call is stubbable in unit tests. */
  clerkClient?: ClerkMfaClient;
}

export function WithdrawForm({ consentId, api = defaultConsentApi, clerkClient }: WithdrawFormProps) {
  const [reason, setReason] = useState("");
  const [mfaVerified, setMfaVerified] = useState(false);
  const [mfaError, setMfaError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [receipt, setReceipt] = useState<{ jwt_receipt: string; withdrawn_at: string } | null>(null);
  const [submitError, setSubmitError] = useState<string | null>(null);

  async function runMfa() {
    setMfaError(null);
    if (!clerkClient) {
      setMfaError("clerk_unavailable");
      return;
    }
    try {
      const out = await clerkClient.session.startVerification({ strategy: "totp" });
      if (!out.verified) {
        setMfaError("mfa_not_verified");
        return;
      }
      setMfaVerified(true);
    } catch (e: unknown) {
      setMfaError(e instanceof Error ? e.message : "mfa_failed");
    }
  }

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    if (!mfaVerified) {
      setSubmitError("mfa_required");
      return;
    }
    setSubmitting(true);
    setSubmitError(null);
    try {
      const out = await api.withdraw(consentId, reason);
      setReceipt(out);
    } catch (err: unknown) {
      setSubmitError(err instanceof Error ? err.message : "withdraw_failed");
    } finally {
      setSubmitting(false);
    }
  }

  if (receipt) {
    return (
      <div className="cx-shell lin">
        <div className="cx-main">
          <main>
            <h1>Consent withdrawn</h1>
            <section
              aria-label="Withdrawal receipt"
              data-testid="withdraw-success"
            >
              <p>
                Withdrawn at <time>{receipt.withdrawn_at}</time>
              </p>
              <JwtReceiptDisplay jwt={receipt.jwt_receipt} />
            </section>
          </main>
        </div>
      </div>
    );
  }

  return (
    <div className="cx-shell lin">
      <div className="cx-main">
        <main>
          <h1>Withdraw consent</h1>
          <p>
            <code>{consentId}</code>
          </p>
          <form
            onSubmit={submit}
            aria-label="Withdraw consent"
            data-testid="withdraw-form"
            className="lin-checklist"
          >
            <Card
              title="Re-authenticate"
              meta="Withdrawing consent requires a fresh MFA check (LGPD Art. 18 IX · GDPR Art. 7§3)."
            >
              <div>
                <Button
                  variant="ghost"
                  onClick={runMfa}
                  data-testid="mfa-button"
                  disabled={mfaVerified}
                >
                  {mfaVerified ? "MFA verified" : "Re-authenticate (MFA)"}
                </Button>{" "}
                {mfaVerified ? (
                  <Badge tone="success" dot>
                    Verified
                  </Badge>
                ) : null}
              </div>
              {mfaError && (
                <div role="alert" data-testid="mfa-error">
                  <Callout tone="danger">{mfaError}</Callout>
                </div>
              )}
            </Card>

            <div className="lin-card lin-card--pad">
              <Field label="Reason (optional)" htmlFor="withdraw-reason">
                <Textarea
                  id="withdraw-reason"
                  data-testid="withdraw-reason"
                  value={reason}
                  onChange={(e) => setReason(e.target.value)}
                />
              </Field>
            </div>

            {submitError && (
              <div role="alert" data-testid="withdraw-error">
                <Callout tone="danger">{submitError}</Callout>
              </div>
            )}
            <div>
              <Button
                type="submit"
                variant="danger"
                loading={submitting}
                disabled={!mfaVerified || submitting}
                aria-disabled={!mfaVerified || submitting}
                data-testid="withdraw-submit"
              >
                Withdraw
              </Button>
            </div>
          </form>
        </main>
      </div>
    </div>
  );
}
