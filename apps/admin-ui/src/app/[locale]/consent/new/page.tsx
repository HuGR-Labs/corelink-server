// Consent capture route (the 6-field CTRL-PRIV-CONSENT-001..006 flow).
//
// PRIVACY GUARD: the rendered consent form root carries the
// `.privacy-no-capture` CSS class (see ConsentForm.tsx). Analytics +
// session-replay tools MUST honour this class. No analytics SDK is wired
// in this WI; the screenshot evidence PNG is the only payload leaving the
// browser and it is sent only to the trusted backend `/v1/consent/grant`.

import { ConsentCaptureFlow } from "@/components/consent/ConsentCaptureFlow";

interface PageProps {
  params: { locale: string };
}

// Stub notice + sub-processors so the scaffold compiles. WI-S16-006 will
// swap these for the real MDX render + `/v1/subprocessors` fetch.
const STUB_NOTICE_TEXT =
  "CoreLink privacy notice — by granting consent you agree to processing per the disclosed purpose, legal basis, data categories, retention period, sub-processors, and withdrawal method.";
const STUB_NOTICE_VERSION = "1.0.0";
const STUB_SUBPROCESSORS = ["Cloudflare", "Stripe", "Clerk"];

export default function ConsentNewPage({ params }: PageProps) {
  return (
    <ConsentCaptureFlow
      locale={params.locale}
      noticeText={STUB_NOTICE_TEXT}
      noticeVersion={STUB_NOTICE_VERSION}
      thirdParties={STUB_SUBPROCESSORS}
    />
  );
}
