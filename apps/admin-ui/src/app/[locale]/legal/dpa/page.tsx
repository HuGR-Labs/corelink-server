import { LegalDocPage } from "../LegalDocPage";
import { loadLocalizedMarkdown, extractFrontMatterFromBody } from "@/content/load";
import type { Locale } from "@/i18n/LocaleContext";

const TITLE: Record<Locale, string> = {
  en: "Data Processing Agreement",
  pt: "Acordo de Processamento de Dados",
  es: "Acuerdo de Procesamiento de Datos",
  // R-prep i18n-de — DACH market.
  de: "Auftragsverarbeitungsvertrag",
};

interface RouteParams {
  params: Promise<{ locale: Locale }>;
}

export default async function Page({ params }: RouteParams) {
  const { locale } = await params;
  const content = loadLocalizedMarkdown("dpa", locale);
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
