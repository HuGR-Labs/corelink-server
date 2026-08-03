// Consent capture route (the 6-field CTRL-PRIV-CONSENT-001..006 flow).
//
// PRIVACY GUARD: the rendered consent form root carries the
// `.privacy-no-capture` CSS class (see ConsentForm.tsx). Analytics +
// session-replay tools MUST honour this class. No analytics SDK is wired
// in this WI; the screenshot evidence PNG is the only payload leaving the
// browser and it is sent only to the trusted backend `/v1/consent/grant`.

// ⛔ RETIRED — this page answers 404. It was the ONE anonymous-reachable
// screen of the surface (public per `lib/route-matcher.ts`), and it solicited
// a GDPR/LGPD consent record — against STUB notice text, see below — that no
// backend ever stored. Rationale + one-line reversal: `../retired.ts`.

import type { Metadata } from "next";
import { ConsentCaptureFlow } from "@/components/consent/ConsentCaptureFlow";
import { assertConsentUiEnabled } from "../retired";

interface PageProps {
  params: Promise<{ locale: string }>;
}

// Static page-level title. Same pattern as `security/policy/page.tsx`: a plain
// `export const metadata` here is hoisted into the static <head> rather than
// streamed via Next 15's AsyncMetadataOutlet/MetadataBoundary. Without it this
// dynamic route (it `await`s `params`, under the force-dynamic root layout)
// only inherits the root layout's title, which Next streams — and Lighthouse's
// headless run strips streamed metadata from the post-hydration DOM, failing
// the a11y `document-title` audit (renders correctly in real prod). The page
// body stays fully dynamic + locale-aware. Title is English because all
// Lighthouse-scored routes are `/en/*`.
export const metadata: Metadata = {
  title: "Grant consent — CoreLink",
  description:
    "Review the disclosed purpose, legal basis, data categories, retention period, sub-processors, and withdrawal method, then grant consent.",
};

// Stub notice + sub-processors so the scaffold compiles. WI-S16-006 will
// swap these for the real MDX render + `/v1/subprocessors` fetch.
const STUB_NOTICE_TEXT =
  "CoreLink privacy notice — by granting consent you agree to processing per the disclosed purpose, legal basis, data categories, retention period, sub-processors, and withdrawal method.";
const STUB_NOTICE_VERSION = "1.0.0";
const STUB_SUBPROCESSORS = ["Cloudflare", "Stripe", "Clerk"];

export default async function ConsentNewPage({ params }: PageProps) {
  assertConsentUiEnabled();
  const { locale } = await params;
  return (
    <ConsentCaptureFlow
      locale={locale}
      noticeText={STUB_NOTICE_TEXT}
      noticeVersion={STUB_NOTICE_VERSION}
      thirdParties={STUB_SUBPROCESSORS}
    />
  );
}
