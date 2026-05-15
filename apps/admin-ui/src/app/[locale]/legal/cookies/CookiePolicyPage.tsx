"use client";

import * as React from "react";
import { PageHeader } from "@/components/layout/PageHeader";
import { Switch } from "@/components/ui/Switch";
import { Button } from "@/components/ui/Button";
import { postConsent, DEFAULT_CONSENT, type ConsentState } from "@/lib/consent";
import type { Locale } from "@/i18n/LocaleContext";

const STRINGS: Record<Locale, {
  title: string;
  intro: string;
  functional: string;
  functionalHint: string;
  analytics: string;
  analyticsHint: string;
  marketing: string;
  marketingHint: string;
  save: string;
  saved: string;
  error: string;
}> = {
  en: {
    title: "Cookie policy",
    intro: "Choose which optional cookies you allow. Strictly necessary cookies are always on; we cannot disable them.",
    functional: "Strictly necessary",
    functionalHint: "Required for authentication, security, and load balancing.",
    analytics: "Analytics",
    analyticsHint: "Helps us understand product usage in aggregate.",
    marketing: "Marketing",
    marketingHint: "Used to deliver personalized content and measure campaign effectiveness.",
    save: "Save consent",
    saved: "Your consent has been saved.",
    error: "We could not save your consent. Please try again.",
  },
  pt: {
    title: "Política de cookies",
    intro: "Escolha quais cookies opcionais você permite. Os estritamente necessários permanecem sempre ativos.",
    functional: "Estritamente necessários",
    functionalHint: "Necessários para autenticação, segurança e balanceamento de carga.",
    analytics: "Analítica",
    analyticsHint: "Ajuda-nos a entender o uso do produto de forma agregada.",
    marketing: "Marketing",
    marketingHint: "Utilizados para entregar conteúdo personalizado e medir campanhas.",
    save: "Salvar consentimento",
    saved: "Seu consentimento foi salvo.",
    error: "Não foi possível salvar. Tente novamente.",
  },
  es: {
    title: "Política de cookies",
    intro: "Elija qué cookies opcionales permite. Las estrictamente necesarias permanecen siempre activas.",
    functional: "Estrictamente necesarias",
    functionalHint: "Necesarias para autenticación, seguridad y balanceo de carga.",
    analytics: "Análisis",
    analyticsHint: "Nos ayuda a entender el uso del producto de manera agregada.",
    marketing: "Marketing",
    marketingHint: "Se utilizan para entregar contenido personalizado y medir campañas.",
    save: "Guardar consentimiento",
    saved: "Su consentimiento fue guardado.",
    error: "No se pudo guardar. Inténtelo de nuevo.",
  },
  // R-prep i18n-de — DACH market.
  de: {
    title: "Cookie-Richtlinie",
    intro: "Wählen Sie aus, welche optionalen Cookies Sie zulassen. Unbedingt erforderliche Cookies sind immer aktiv und können nicht deaktiviert werden.",
    functional: "Unbedingt erforderlich",
    functionalHint: "Erforderlich für Authentifizierung, Sicherheit und Lastverteilung.",
    analytics: "Analyse",
    analyticsHint: "Hilft uns, die Produktnutzung im Aggregat zu verstehen.",
    marketing: "Marketing",
    marketingHint: "Wird zur Bereitstellung personalisierter Inhalte und zur Messung von Kampagnen verwendet.",
    save: "Einwilligung speichern",
    saved: "Ihre Einwilligung wurde gespeichert.",
    error: "Ihre Einwilligung konnte nicht gespeichert werden. Bitte erneut versuchen.",
  },
};

export interface CookiePolicyPageProps {
  locale: Locale;
  initial?: ConsentState;
  /** Override fetch — useful for tests. */
  fetchImpl?: typeof fetch;
}

export function CookiePolicyPage({ locale, initial = DEFAULT_CONSENT, fetchImpl }: CookiePolicyPageProps) {
  const t = STRINGS[locale];
  const [state, setState] = React.useState<ConsentState>(initial);
  const [message, setMessage] = React.useState<{ kind: "ok" | "err"; text: string } | null>(null);
  const [submitting, setSubmitting] = React.useState(false);

  async function onSave() {
    setSubmitting(true);
    setMessage(null);
    try {
      await postConsent(state, fetchImpl);
      setMessage({ kind: "ok", text: t.saved });
    } catch {
      setMessage({ kind: "err", text: t.error });
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <article className="mx-auto max-w-3xl py-8">
      <PageHeader title={t.title} description={t.intro} />
      <fieldset className="flex flex-col gap-6">
        <legend className="sr-only">{t.title}</legend>
        <div>
          <Switch label={t.functional} checked disabled />
          <p className="ml-12 text-sm text-slate-600">{t.functionalHint}</p>
        </div>
        <div>
          <Switch
            label={t.analytics}
            checked={state.analytics}
            onCheckedChange={(v) => setState((s) => ({ ...s, analytics: v }))}
          />
          <p className="ml-12 text-sm text-slate-600">{t.analyticsHint}</p>
        </div>
        <div>
          <Switch
            label={t.marketing}
            checked={state.marketing}
            onCheckedChange={(v) => setState((s) => ({ ...s, marketing: v }))}
          />
          <p className="ml-12 text-sm text-slate-600">{t.marketingHint}</p>
        </div>
      </fieldset>
      <div className="mt-6 flex items-center gap-3">
        <Button onClick={onSave} disabled={submitting}>
          {t.save}
        </Button>
        {message && (
          <p
            role="status"
            aria-live="polite"
            className={message.kind === "ok" ? "text-green-700" : "text-red-700"}
          >
            {message.text}
          </p>
        )}
      </div>
    </article>
  );
}
