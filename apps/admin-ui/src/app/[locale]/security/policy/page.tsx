import type { Locale } from "@/i18n/LocaleContext";
import { SecurityPolicyPage } from "./SecurityPolicyPage";

interface RouteParams {
  params: Promise<{ locale: Locale }>;
}

export const metadata = {
  title: "Security Policy — CoreLink",
  description:
    "How to report a vulnerability in CoreLink and what response you can expect. RFC 9116 contact card, safe-harbor language, CVSS-mapped SLA.",
};

export default async function Page({ params }: RouteParams) {
  const { locale } = await params;
  return <SecurityPolicyPage locale={locale} />;
}
