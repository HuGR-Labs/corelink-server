"use client";

import * as React from "react";
import { PublicShell } from "@/components/public/PublicShell";
import { PublicPageHeader } from "@/components/public/PublicPageHeader";
import { LegalProse } from "@/components/public/LegalProse";
import { Callout } from "@/components/ui/linear";
import { formatDate } from "@/i18n/format";
import type { Locale } from "@/i18n/LocaleContext";

// R-prep i18n-de — `de` joined as the fourth canonical locale.
const ACCEPTED_LABEL: Record<Locale, string> = {
  en: "Accepted on",
  pt: "Aceito em",
  es: "Aceptado el",
  de: "Akzeptiert am",
};

const VERSION_LABEL: Record<Locale, string> = {
  en: "Version",
  pt: "Versão",
  es: "Versión",
  de: "Version",
};

export interface LegalDocPageProps {
  locale: Locale;
  title: string;
  content: string;
  version: string | null;
  /** ISO date of acceptance if the current user accepted this version. */
  acceptedAt?: string | null;
}

export function LegalDocPage({ locale, title, content, version, acceptedAt }: LegalDocPageProps) {
  return (
    <PublicShell width="prose">
      <PublicPageHeader title={title} />
      {(version || acceptedAt) && (
        <div role="note" className="mb-6">
          <Callout tone="info">
            {version && (
              <span className="mr-3">
                <strong className="font-[560] text-[var(--t1)]">{VERSION_LABEL[locale]}:</strong>{" "}
                {version}
              </span>
            )}
            {acceptedAt && (
              <span>
                <strong className="font-[560] text-[var(--t1)]">{ACCEPTED_LABEL[locale]}:</strong>{" "}
                {formatDate(acceptedAt, locale)}
              </span>
            )}
          </Callout>
        </div>
      )}
      <LegalProse content={content} />
    </PublicShell>
  );
}
