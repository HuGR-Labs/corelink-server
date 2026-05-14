import { isLocale, type Locale } from "@/i18n";
import { DsrStatusListClient } from "./DsrStatusListClient";

interface PageProps {
  params: { locale: string };
}

export default function DsrStatusListPage({ params }: PageProps) {
  const locale: Locale = isLocale(params.locale) ? params.locale : "en";
  return <DsrStatusListClient locale={locale} />;
}
