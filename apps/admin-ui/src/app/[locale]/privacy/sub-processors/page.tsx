import { SubProcessorsPage } from "./SubProcessorsPage";
import { loadSubProcessors } from "@/content/load";
import { resolveApiBaseUrl } from "@/lib/api-base-url";
import type { Locale } from "@/i18n/LocaleContext";

interface RouteParams {
  params: Promise<{ locale: Locale }>;
}

export default async function Page({ params }: RouteParams) {
  const { locale } = await params;
  // Try `/v1/subprocessors`; fall back to bundled JSON.
  //
  // `CORELINK_API_BASE` is defined NOWHERE in this repo, so this read used to
  // resolve to a bare `/v1/subprocessors` — a relative URL, which Node's
  // `fetch` rejects outright, so the `try` block has never once reached the
  // network and the bundled JSON has always been served. Route through the
  // same resolver the browser clients use instead of a fourth env-var
  // convention. The static fallback stays: `/v1/subprocessors` still has no
  // handler anywhere (see `lib/consent-api.ts`), so the bundled list remains
  // the real source until a backend lands.
  let list = loadSubProcessors();
  try {
    const res = await fetch(`${resolveApiBaseUrl()}/v1/subprocessors`, {
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
