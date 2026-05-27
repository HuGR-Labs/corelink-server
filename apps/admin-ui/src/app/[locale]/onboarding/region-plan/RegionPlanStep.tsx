"use client";

import * as React from "react";
import { Wizard } from "@/components/onboarding/Wizard";
import { t, type Locale } from "@/i18n/messages";
import {
  SUPPORTED_PLANS,
  SUPPORTED_REGIONS,
  type Plan,
  type Region,
} from "@/lib/validators";
import { configureTenantAction } from "../actions";
import { loadState, saveState, nextStepFor } from "@/lib/onboarding-state";

export function RegionPlanStep({
  locale,
}: {
  locale: Locale;
}): React.ReactElement {
  const [region, setRegion] = React.useState<Region>("us-east");
  const [plan, setPlan] = React.useState<Plan>("starter");
  const [error, setError] = React.useState<string | null>(null);
  const [submitting, setSubmitting] = React.useState(false);

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
      await configureTenantAction({
        tenantId: persisted.tenantId,
        region,
        plan,
      });
      // Phase 0.C: wizard no longer collects a payment method. All
      // plans (free + paid) advance straight to PAT. The money question
      // happens post-signup via Stripe-hosted Checkout Session.
      const next = nextStepFor("region-plan", { plan });
      saveState({ step: next, tenantId: persisted.tenantId });
      window.location.assign(`/${locale}/onboarding/${next}`);
    } catch (err) {
      setError((err as Error).message);
      setSubmitting(false);
    }
  }

  return (
    <Wizard current="region-plan">
      <h1>{t(locale, "onboarding.region_plan.title")}</h1>
      <form onSubmit={onSubmit}>
        <fieldset>
          <legend>{t(locale, "onboarding.region_plan.region_label")}</legend>
          {SUPPORTED_REGIONS.map((r) => (
            <label key={r}>
              <input
                type="radio"
                name="region"
                value={r}
                checked={region === r}
                onChange={() => setRegion(r)}
                data-testid={`region-${r}`}
              />
              {r}
            </label>
          ))}
        </fieldset>
        <fieldset>
          <legend>{t(locale, "onboarding.region_plan.plan_label")}</legend>
          {SUPPORTED_PLANS.map((p) => (
            <label key={p}>
              <input
                type="radio"
                name="plan"
                value={p}
                checked={plan === p}
                onChange={() => setPlan(p)}
                data-testid={`plan-${p}`}
              />
              {p}
            </label>
          ))}
        </fieldset>
        {error ? (
          <p role="alert" data-testid="region-plan-error">
            {error}
          </p>
        ) : null}
        <button type="submit" disabled={submitting} data-testid="region-plan-submit">
          {t(locale, "onboarding.region_plan.submit")}
        </button>
      </form>
    </Wizard>
  );
}
