// Consent withdrawal route — LGPD Art. 18 IX + GDPR Art. 7§3.
// MFA re-auth via Clerk before the backend accepts the withdrawal.

import { WithdrawForm } from "@/components/consent/WithdrawForm";

interface PageProps {
  params: Promise<{ locale: string; id: string }>;
}

export default async function WithdrawPage({ params }: PageProps) {
  const { id } = await params;
  return <WithdrawForm consentId={id} />;
}
