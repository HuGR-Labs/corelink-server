import { isLocale, type Locale } from "@/i18n";
import { DsrStatusDetailClient } from "./DsrStatusDetailClient";

interface PageProps {
  params: { locale: string; id: string };
}

export default function DsrStatusDetailPage({ params }: PageProps) {
  const locale: Locale = isLocale(params.locale) ? params.locale : "en";
  return <DsrStatusDetailClient locale={locale} requestId={params.id} />;
}
