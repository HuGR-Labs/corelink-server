// Consent withdrawal route — LGPD Art. 18 IX + GDPR Art. 7§3.
// MFA re-auth via Clerk before the backend accepts the withdrawal.

// ⛔ RETIRED — this page answers 404. Beyond the missing
// `/v1/consent/{id}/withdraw` handler, this route could never have completed a
// withdrawal even against a working backend: it renders `WithdrawForm` with no
// `clerkClient` prop, so the MFA step fails immediately and submit stays
// disabled. Rationale + one-line reversal: `../../retired.ts`.

import { WithdrawForm } from "@/components/consent/WithdrawForm";
import { assertConsentUiEnabled } from "../../retired";

interface PageProps {
  params: Promise<{ locale: string; id: string }>;
}

export default async function WithdrawPage({ params }: PageProps) {
  assertConsentUiEnabled();
  const { id } = await params;
  return <WithdrawForm consentId={id} />;
}
