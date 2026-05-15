"use client";

import * as React from "react";
import Link from "next/link";
import { PageHeader } from "@/components/layout/PageHeader";
import { MarkdownView } from "@/components/content/MarkdownView";
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
    <article className="mx-auto max-w-3xl py-8">
      <PageHeader title={TITLE[locale]} />
      <div
        role="note"
        className="mb-6 rounded border border-blue-200 bg-blue-50 p-3 text-sm text-blue-900"
      >
        {version && (
          <span className="mr-3">
            <strong>{VERSION_LABEL[locale]}:</strong> {version}
          </span>
        )}
        {lastUpdated && (
          <span>
            <strong>{LAST_UPDATED_LABEL[locale]}:</strong> {lastUpdated}
          </span>
        )}
      </div>
      <MarkdownView content={content} />
      <p className="mt-6">
        <Link href={`/${locale}/privacy/sub-processors`} className="underline">
          {SUBPROC_LABEL[locale]} →
        </Link>
      </p>
    </article>
  );
}
