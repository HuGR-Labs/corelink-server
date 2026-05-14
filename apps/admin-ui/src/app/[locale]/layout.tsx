import * as React from "react";
import { LocaleProvider } from "@/i18n/LocaleProvider";
import type { Locale } from "@/i18n/LocaleContext";

interface LayoutProps {
  children: React.ReactNode;
  params: Promise<{ locale: Locale }>;
}

const BCP47: Record<Locale, string> = {
  en: "en-US",
  pt: "pt-BR",
  es: "es-419",
};

export default async function LocaleLayout({ children, params }: LayoutProps) {
  const { locale } = await params;
  return (
    <html lang={BCP47[locale] ?? "en-US"}>
      <body>
        <LocaleProvider initialLocale={locale}>{children}</LocaleProvider>
      </body>
    </html>
  );
}
