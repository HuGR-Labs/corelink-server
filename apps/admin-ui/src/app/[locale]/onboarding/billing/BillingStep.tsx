"use client";

import * as React from "react";
import { Wizard } from "@/components/onboarding/Wizard";
import { t, type Locale } from "@/i18n/messages";
import {
  createSetupIntentAction,
  recordBillingSetupAction,
} from "../actions";
import { loadState, saveState } from "@/lib/onboarding-state";

export function BillingStep({ locale }: { locale: Locale }): React.ReactElement {
  const [clientSecret, setClientSecret] = React.useState<string | null>(null);
  const [paymentMethodId, setPaymentMethodId] = React.useState("");
  const [error, setError] = React.useState<string | null>(null);
  const [submitting, setSubmitting] = React.useState(false);

  React.useEffect(() => {
    let cancelled = false;
    createSetupIntentAction()
      .then((intent) => {
        if (!cancelled) setClientSecret(intent.client_secret);
      })
      .catch((err: Error) => {
        if (!cancelled) setError(err.message);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  async function onSubmit(e: React.FormEvent<HTMLFormElement>): Promise<void> {
    e.preventDefault();
    setSubmitting(true);
    const persisted = loadState();
    if (!persisted.tenantId) {
      setError("Missing tenant_id");
      setSubmitting(false);
      return;
    }
    try {
      await recordBillingSetupAction({
        tenantId: persisted.tenantId,
        paymentMethodId,
      });
      saveState({ step: "pat", tenantId: persisted.tenantId });
      window.location.assign(`/${locale}/onboarding/pat`);
    } catch (err) {
      setError((err as Error).message);
      setSubmitting(false);
    }
  }

  return (
    <Wizard current="billing">
      <h1>{t(locale, "onboarding.billing.title")}</h1>
      {/* In production this is replaced with Stripe Elements bound to the
          setup intent client_secret. For the scaffold we accept the
          payment method ID directly so the flow is testable. */}
      <form onSubmit={onSubmit}>
        <label>
          payment_method_id
          <input
            value={paymentMethodId}
            onChange={(e) => setPaymentMethodId(e.target.value)}
            data-testid="billing-payment-method"
            required
          />
        </label>
        {clientSecret ? (
          <p data-testid="billing-client-secret-loaded">
            setup_intent ready
          </p>
        ) : null}
        {error ? (
          <p role="alert" data-testid="billing-error">
            {error}
          </p>
        ) : null}
        <button type="submit" disabled={submitting} data-testid="billing-submit">
          {t(locale, "onboarding.billing.submit")}
        </button>
      </form>
    </Wizard>
  );
}
