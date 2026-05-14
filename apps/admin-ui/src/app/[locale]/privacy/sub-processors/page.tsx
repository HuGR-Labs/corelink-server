import { SubProcessorsPage } from "./SubProcessorsPage";
import { loadSubProcessors } from "@/content/load";
import type { Locale } from "@/i18n/LocaleContext";

interface RouteParams {
  params: Promise<{ locale: Locale }>;
}

export default async function Page({ params }: RouteParams) {
  const { locale } = await params;
  // Try `/v1/subprocessors`; fall back to bundled JSON.
  let list = loadSubProcessors();
  try {
    const res = await fetch(`${process.env.CORELINK_API_BASE ?? ""}/v1/subprocessors`, {
      cache: "no-store",
    });
    if (res.ok) {
      list = (await res.json()) as typeof list;
    }
  } catch {
    // Static fallback used.
  }
  return <SubProcessorsPage locale={locale} version={list.version} items={list.items} />;
}
