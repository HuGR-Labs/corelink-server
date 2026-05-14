"use client";

import * as React from "react";
import { Wizard } from "@/components/onboarding/Wizard";
import { PatModal } from "@/components/onboarding/PatModal";
import { t, type Locale } from "@/i18n/messages";
import {
  PAT_EXPIRY_OPTIONS,
  PAT_SCOPES,
  type PatExpiryDays,
  type PatScope,
  validatePatInput,
} from "@/lib/validators";
import { createPatAction } from "../actions";
import {
  setPlaintextPat,
  saveState,
  loadState,
} from "@/lib/onboarding-state";

export function PatStep({ locale }: { locale: Locale }): React.ReactElement {
  const [label, setLabel] = React.useState("CLI");
  const [scope, setScope] = React.useState<PatScope>("read-only");
  const [expiry, setExpiry] = React.useState<PatExpiryDays>(30);
  const [modalOpen, setModalOpen] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  const [submitting, setSubmitting] = React.useState(false);

  const v = validatePatInput({ label, scope, expiryDays: expiry });
  const canSubmit = v.ok && !submitting;

  async function onSubmit(e: React.FormEvent<HTMLFormElement>): Promise<void> {
    e.preventDefault();
    if (!v.ok) {
      setError(v.reason);
      return;
    }
    setSubmitting(true);
    try {
      const pat = await createPatAction({
        label,
        scope,
        expiryDays: expiry,
      });
      // Hold plaintext in memory only; modal reads from there.
      setPlaintextPat(pat.plaintext);
      setModalOpen(true);
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setSubmitting(false);
    }
  }

  function onModalConfirm(): void {
    const persisted = loadState();
    saveState({ step: "done", tenantId: persisted.tenantId });
    setModalOpen(false);
    window.location.assign(`/${locale}/onboarding/done`);
  }

  return (
    <Wizard current="pat">
      <h1>{t(locale, "onboarding.pat.title")}</h1>
      <form onSubmit={onSubmit}>
        <label>
          {t(locale, "onboarding.pat.label_label")}
          <input
            value={label}
            onChange={(e) => setLabel(e.target.value)}
            data-testid="pat-label"
            required
          />
        </label>
        <fieldset>
          <legend>{t(locale, "onboarding.pat.scope_label")}</legend>
          {PAT_SCOPES.map((s) => (
            <label key={s}>
              <input
                type="radio"
                name="scope"
                value={s}
                checked={scope === s}
                onChange={() => setScope(s)}
                data-testid={`pat-scope-${s}`}
              />
              {s}
            </label>
          ))}
        </fieldset>
        <label>
          {t(locale, "onboarding.pat.expiry_label")}
          <select
            value={expiry}
            onChange={(e) =>
              setExpiry(Number(e.target.value) as PatExpiryDays)
            }
            data-testid="pat-expiry"
          >
            {PAT_EXPIRY_OPTIONS.map((d) => (
              <option key={d} value={d}>
                {d} days
              </option>
            ))}
          </select>
        </label>
        {error ? (
          <p role="alert" data-testid="pat-error">
            {error}
          </p>
        ) : null}
        <button type="submit" disabled={!canSubmit} data-testid="pat-submit">
          {t(locale, "onboarding.pat.submit")}
        </button>
      </form>

      <PatModal
        open={modalOpen}
        onConfirm={onModalConfirm}
        labels={{
          title: t(locale, "onboarding.pat.modal_title"),
          warning: t(locale, "onboarding.pat.modal_warning"),
          copy: t(locale, "onboarding.pat.copy"),
          confirmSaved: t(locale, "onboarding.pat.confirm_saved"),
          finish: t(locale, "onboarding.pat.finish"),
        }}
      />
    </Wizard>
  );
}
