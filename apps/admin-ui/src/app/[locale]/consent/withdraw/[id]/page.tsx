// Consent withdrawal route — LGPD Art. 18 IX + GDPR Art. 7§3.
// MFA re-auth via Clerk before the backend accepts the withdrawal.

import { WithdrawForm } from "@/components/consent/WithdrawForm";

interface PageProps {
  params: { locale: string; id: string };
}

export default function WithdrawPage({ params }: PageProps) {
  return <WithdrawForm consentId={params.id} />;
}
