// CTRL-PRIV-CONSENT dashboard route. Renders active consent rows from
// `/v1/consent/active`. Each row exposes view + withdraw actions. The active
// `[locale]` segment is threaded into the client island so every nav link is
// locale-prefixed (otherwise the links drop the segment and 404).

import { ConsentDashboard } from "@/components/consent/ConsentDashboard";

interface PageProps {
  params: Promise<{ locale: string }>;
}

export default async function ConsentDashboardPage({ params }: PageProps) {
  const { locale } = await params;
  return <ConsentDashboard locale={locale} />;
}
