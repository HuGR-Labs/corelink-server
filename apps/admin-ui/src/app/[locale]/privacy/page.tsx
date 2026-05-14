import { PrivacyPage } from "./PrivacyPage";
import { loadLocalizedMarkdown, extractFrontMatterFromBody } from "@/content/load";
import type { Locale } from "@/i18n/LocaleContext";

interface RouteParams {
  params: Promise<{ locale: Locale }>;
}

export default async function Page({ params }: RouteParams) {
  const { locale } = await params;
  const content = loadLocalizedMarkdown("privacy-notice", locale);
  const meta = extractFrontMatterFromBody(content);
  return <PrivacyPage locale={locale} content={content} version={meta.version} lastUpdated={meta.lastUpdated} />;
}
