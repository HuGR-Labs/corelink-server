"use client";

import * as React from "react";
import { Select } from "@/components/ui/Select";
import { LOCALE_COOKIE, SUPPORTED_LOCALES, useLocale, type Locale } from "@/i18n/LocaleContext";

const labels: Record<Locale, string> = {
  en: "English",
  pt: "Português",
  es: "Español",
};

export interface LocaleSwitcherProps {
  /** Override label (defaults to "Language"). */
  label?: string;
  /** Override reload behavior on change — used for tests. */
  onChange?: (locale: Locale) => void;
}

/**
 * Persists locale selection to a cookie and (by default) reloads the page so
 * server-rendered translations refresh.
 */
export function LocaleSwitcher({ label = "Language", onChange }: LocaleSwitcherProps) {
  const { locale, setLocale } = useLocale();

  function persist(next: Locale) {
    if (typeof document !== "undefined") {
      // 1 year cookie. Path=/ so all routes honor it. SameSite=Lax for CSRF safety.
      document.cookie = `${LOCALE_COOKIE}=${next}; Path=/; Max-Age=31536000; SameSite=Lax`;
    }
    setLocale(next);
    if (onChange) {
      onChange(next);
      return;
    }
    if (typeof window !== "undefined") {
      window.location.reload();
    }
  }

  return (
    <Select
      label={label}
      value={locale}
      onValueChange={(v) => persist(v as Locale)}
      options={SUPPORTED_LOCALES.map((l) => ({ value: l, label: labels[l] }))}
    />
  );
}
