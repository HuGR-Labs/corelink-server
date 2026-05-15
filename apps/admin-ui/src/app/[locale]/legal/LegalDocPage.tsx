"use client";

import * as React from "react";
import { PageHeader } from "@/components/layout/PageHeader";
import { MarkdownView } from "@/components/content/MarkdownView";
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
    <article className="mx-auto max-w-3xl py-8">
      <PageHeader title={title} />
      <div
        role="note"
        className="mb-6 rounded border border-blue-200 bg-blue-50 p-3 text-sm text-blue-900"
      >
        {version && (
          <span className="mr-3">
            <strong>{VERSION_LABEL[locale]}:</strong> {version}
          </span>
        )}
        {acceptedAt && (
          <span>
            <strong>{ACCEPTED_LABEL[locale]}:</strong> {formatDate(acceptedAt, locale)}
          </span>
        )}
      </div>
      <MarkdownView content={content} />
    </article>
  );
}
