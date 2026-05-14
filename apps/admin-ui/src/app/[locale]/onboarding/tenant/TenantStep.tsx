"use client";

import * as React from "react";
import { Wizard } from "@/components/onboarding/Wizard";
import { t, type Locale } from "@/i18n/messages";
import {
  validateTenantName,
  type ValidationResult,
} from "@/lib/validators";
import { createTenantAction } from "../actions";
import { saveState } from "@/lib/onboarding-state";

export function TenantStep({ locale }: { locale: Locale }): React.ReactElement {
  const [name, setName] = React.useState("");
  const [legalName, setLegalName] = React.useState("");
  const [taxId, setTaxId] = React.useState("");
  const [error, setError] = React.useState<string | null>(null);
  const [submitting, setSubmitting] = React.useState(false);

  const validation: ValidationResult = validateTenantName(name);
  const canSubmit = validation.ok && legalName.length > 0 && !submitting;

  async function onSubmit(e: React.FormEvent<HTMLFormElement>): Promise<void> {
    e.preventDefault();
    if (!validation.ok) {
      setError(t(locale, `onboarding.tenant.errors.${validation.reason}`));
      return;
    }
    setSubmitting(true);
    try {
      const tenant = await createTenantAction({
        name,
        legalName,
        taxId: taxId || undefined,
      });
      saveState({ step: "dpa", tenantId: tenant.id });
      // Hard nav to next step page.
      window.location.assign(`/${locale}/onboarding/dpa`);
    } catch (err) {
      setError((err as Error).message);
      setSubmitting(false);
    }
  }

  return (
    <Wizard current="tenant">
      <h1>{t(locale, "onboarding.tenant.title")}</h1>
      <form onSubmit={onSubmit} noValidate>
        <label>
          {t(locale, "onboarding.tenant.name_label")}
          <input
            name="name"
            value={name}
            onChange={(e) => setName(e.target.value)}
            data-testid="tenant-name"
            required
            minLength={3}
            maxLength={64}
          />
        </label>
        <p>{t(locale, "onboarding.tenant.name_hint")}</p>
        <label>
          {t(locale, "onboarding.tenant.legal_label")}
          <input
            name="legalName"
            value={legalName}
            onChange={(e) => setLegalName(e.target.value)}
            data-testid="tenant-legal"
            required
          />
        </label>
        <label>
          {t(locale, "onboarding.tenant.tax_label")}
          <input
            name="taxId"
            value={taxId}
            onChange={(e) => setTaxId(e.target.value)}
            data-testid="tenant-tax"
          />
        </label>
        {error ? (
          <p role="alert" data-testid="tenant-error">
            {error}
          </p>
        ) : null}
        <button type="submit" disabled={!canSubmit} data-testid="tenant-submit">
          {t(locale, "onboarding.tenant.submit")}
        </button>
      </form>
    </Wizard>
  );
}
