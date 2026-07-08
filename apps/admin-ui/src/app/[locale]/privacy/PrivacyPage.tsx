"use client";

import * as React from "react";
import Link from "next/link";
import { PublicShell } from "@/components/public/PublicShell";
import { PublicPageHeader } from "@/components/public/PublicPageHeader";
import { LegalProse } from "@/components/public/LegalProse";
import { Callout } from "@/components/ui/linear";
import type { Locale } from "@/i18n/LocaleContext";

// R-prep i18n-de — `de` joined as the fourth canonical locale.
const TITLE: Record<Locale, string> = {
  en: "Privacy notice",
  pt: "Aviso de privacidade",
  es: "Aviso de privacidad",
  de: "Datenschutzhinweis",
};

const LAST_UPDATED_LABEL: Record<Locale, string> = {
  en: "Last updated",
  pt: "Última atualização",
  es: "Última actualización",
  de: "Zuletzt aktualisiert",
};

const VERSION_LABEL: Record<Locale, string> = {
  en: "Version",
  pt: "Versão",
  es: "Versión",
  de: "Version",
};

const SUBPROC_LABEL: Record<Locale, string> = {
  en: "Sub-processors",
  pt: "Subprocessadores",
  es: "Subprocesadores",
  de: "Unterauftragsverarbeiter",
};

export interface PrivacyPageProps {
  locale: Locale;
  content: string;
  version: string | null;
  lastUpdated: string | null;
}

export function PrivacyPage({ locale, content, version, lastUpdated }: PrivacyPageProps) {
  return (
    // `<main id="main">` landmark is provided by PublicShell (satisfies axe
    // landmark-one-main + region, matching the landing page's pattern).
    <PublicShell width="prose">
      <PublicPageHeader title={TITLE[locale]} />
      {(version || lastUpdated) && (
        <div role="note" className="mb-6">
          <Callout tone="info">
            {version && (
              <span className="mr-3">
                <strong className="font-[560] text-[var(--t1)]">{VERSION_LABEL[locale]}:</strong>{" "}
                {version}
              </span>
            )}
            {lastUpdated && (
              <span>
                <strong className="font-[560] text-[var(--t1)]">{LAST_UPDATED_LABEL[locale]}:</strong>{" "}
                {lastUpdated}
              </span>
            )}
          </Callout>
        </div>
      )}
      <LegalProse content={content} />
      <p className="mt-8">
        <Link
          href={`/${locale}/privacy/sub-processors`}
          className="text-[var(--t1)] underline decoration-[var(--line-2)] underline-offset-2 hover:decoration-[var(--t2)]"
        >
          {SUBPROC_LABEL[locale]} →
        </Link>
      </p>
    </PublicShell>
  );
}
