import { isLocale, type Locale } from "@/i18n";
import { DsrStatusListClient } from "./DsrStatusListClient";

interface PageProps {
  params: Promise<{ locale: string }>;
}

export default async function DsrStatusListPage({ params }: PageProps) {
  const { locale: rawLocale } = await params;
  const locale: Locale = isLocale(rawLocale) ? rawLocale : "en";
  return <DsrStatusListClient locale={locale} />;
}
