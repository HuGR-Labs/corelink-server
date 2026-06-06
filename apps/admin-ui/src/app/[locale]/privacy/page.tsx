import type { Metadata } from "next";
import { PrivacyPage } from "./PrivacyPage";
import { loadLocalizedMarkdown, extractFrontMatterFromBody } from "@/content/load";
import type { Locale } from "@/i18n/LocaleContext";

interface RouteParams {
  params: Promise<{ locale: Locale }>;
}

// Static page-level title. Same pattern as `security/policy/page.tsx`: a plain
// `export const metadata` here is hoisted into the static <head> rather than
// streamed via Next 15's AsyncMetadataOutlet/MetadataBoundary. Without it this
// dynamic route (it `await`s `params`, under the force-dynamic root layout)
// only inherits the root layout's title, which Next streams — and Lighthouse's
// headless run strips streamed metadata from the post-hydration DOM, failing
// the a11y `document-title` audit (renders correctly in real prod). The page
// body stays fully dynamic + locale-aware; the localized in-page <h1> is
// unchanged. Title is English because all Lighthouse-scored routes are `/en/*`.
export const metadata: Metadata = {
  title: "Privacy notice — CoreLink",
  description:
    "CoreLink privacy notice: processing purposes, legal basis, data categories, retention, sub-processors, and how to withdraw consent.",
};

export default async function Page({ params }: RouteParams) {
  const { locale } = await params;
  const content = loadLocalizedMarkdown("privacy-notice", locale);
  const meta = extractFrontMatterFromBody(content);
  return <PrivacyPage locale={locale} content={content} version={meta.version} lastUpdated={meta.lastUpdated} />;
}
