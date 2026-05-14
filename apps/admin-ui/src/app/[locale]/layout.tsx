import * as React from "react";
import { LocaleProvider } from "@/i18n/LocaleProvider";
import type { Locale } from "@/i18n/LocaleContext";

interface LayoutProps {
  children: React.ReactNode;
  params: Promise<{ locale: Locale }>;
}

// HF-S17-001: root layout (`app/layout.tsx`) owns the single `<html lang>`
// element via `getLocale()` (next-intl) which honours the `corelink_locale`
// cookie + middleware header. Emitting another `<html>`/`<body>` here would
// produce invalid HTML and shadow the root lang attribute.
export default async function LocaleLayout({ children, params }: LayoutProps) {
  const { locale } = await params;
  return <LocaleProvider initialLocale={locale}>{children}</LocaleProvider>;
}
