import { isLocale, type Locale } from "@/i18n";
import { DsrStatusDetailClient } from "./DsrStatusDetailClient";

interface PageProps {
  params: Promise<{ locale: string; id: string }>;
}

export default async function DsrStatusDetailPage({ params }: PageProps) {
  const { locale: rawLocale, id } = await params;
  const locale: Locale = isLocale(rawLocale) ? rawLocale : "en";
  return <DsrStatusDetailClient locale={locale} requestId={id} />;
}
