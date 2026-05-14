"use client";

import { useState } from "react";
import type { ConsentApi } from "@/lib/consent-api";
import { defaultConsentApi } from "@/lib/consent-api";
import { JwtReceiptDisplay } from "@/components/consent/JwtReceiptDisplay";

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
      <section aria-label="Withdrawal receipt" data-testid="withdraw-success">
        <h1>Consent withdrawn</h1>
        <p>
          Withdrawn at <time>{receipt.withdrawn_at}</time>
        </p>
        <JwtReceiptDisplay jwt={receipt.jwt_receipt} />
      </section>
    );
  }

  return (
    <form onSubmit={submit} aria-label="Withdraw consent" data-testid="withdraw-form">
      <h1>Withdraw consent {consentId}</h1>
      <p>
        <button type="button" onClick={runMfa} data-testid="mfa-button" disabled={mfaVerified}>
          {mfaVerified ? "MFA verified" : "Re-authenticate (MFA)"}
        </button>
        {mfaError && (
          <span role="alert" data-testid="mfa-error">
            {mfaError}
          </span>
        )}
      </p>
      <div>
        <label htmlFor="withdraw-reason">Reason (optional)</label>
        <textarea
          id="withdraw-reason"
          data-testid="withdraw-reason"
          value={reason}
          onChange={(e) => setReason(e.target.value)}
        />
      </div>
      {submitError && (
        <p role="alert" data-testid="withdraw-error">
          {submitError}
        </p>
      )}
      <button
        type="submit"
        disabled={!mfaVerified || submitting}
        aria-disabled={!mfaVerified || submitting}
        data-testid="withdraw-submit"
      >
        Withdraw
      </button>
    </form>
  );
}
