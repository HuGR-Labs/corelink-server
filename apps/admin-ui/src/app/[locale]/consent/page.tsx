// CTRL-PRIV-CONSENT dashboard route. Renders active consent rows from
// `/v1/consent/active`. Each row exposes view + withdraw actions. The active
// `[locale]` segment is threaded into the client island so every nav link is
// locale-prefixed (otherwise the links drop the segment and 404).

// ⛔ RETIRED — this page answers 404. `/v1/consent/active` has no handler, so
// the dashboard could only ever render its error state. Rationale + one-line
// reversal: `./retired.ts`.

import { ConsentDashboard } from "@/components/consent/ConsentDashboard";
import { assertConsentUiEnabled } from "./retired";

interface PageProps {
  params: Promise<{ locale: string }>;
}

export default async function ConsentDashboardPage({ params }: PageProps) {
  assertConsentUiEnabled();
  const { locale } = await params;
  return <ConsentDashboard locale={locale} />;
}
