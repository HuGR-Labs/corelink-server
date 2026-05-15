import { LegalDocPage } from "../LegalDocPage";
import { loadLocalizedMarkdown, extractFrontMatterFromBody } from "@/content/load";
import type { Locale } from "@/i18n/LocaleContext";

const TITLE: Record<Locale, string> = {
  en: "Terms of Service",
  pt: "Termos de Uso",
  es: "Términos de servicio",
  // R-prep i18n-de — DACH market.
  de: "Nutzungsbedingungen",
};

interface RouteParams {
  params: Promise<{ locale: Locale }>;
}

export default async function Page({ params }: RouteParams) {
  const { locale } = await params;
  const content = loadLocalizedMarkdown("tos", locale);
  const meta = extractFrontMatterFromBody(content);
  return (
    <LegalDocPage
      locale={locale}
      title={TITLE[locale]}
      content={content}
      version={meta.version}
    />
  );
}
