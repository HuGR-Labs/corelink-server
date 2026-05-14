import { CookiePolicyPage } from "./CookiePolicyPage";
import type { Locale } from "@/i18n/LocaleContext";

interface RouteParams {
  params: Promise<{ locale: Locale }>;
}

export default async function Page({ params }: RouteParams) {
  const { locale } = await params;
  return <CookiePolicyPage locale={locale} />;
}
