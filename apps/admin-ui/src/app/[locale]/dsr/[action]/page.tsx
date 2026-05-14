import { notFound } from "next/navigation";
import { isDsrAction } from "@/lib/dsr-types";
import { isLocale, tFor, type Locale } from "@/i18n";
import { DsrActionPageClient } from "./DsrActionPageClient";

interface PageProps {
  params: { locale: string; action: string };
}

/**
 * `/<locale>/dsr/<action>` — server component shell.
 *
 * Validates locale + action then delegates to the client island that
 * wires Clerk (`useUser`, `useSession`) and the ReAuthGate.
 */
export default function DsrActionPage({ params }: PageProps) {
  if (!isDsrAction(params.action)) {
    notFound();
  }
  const locale: Locale = isLocale(params.locale) ? params.locale : "en";
  const t = (k: string) => tFor(locale, k);

  return (
    <main aria-labelledby="dsr-action-title">
      <h1 id="dsr-action-title">
        {t(`dsr.rights.${params.action}.label`)}
      </h1>
      <p>{t(`dsr.rights.${params.action}.description`)}</p>
      <DsrActionPageClient locale={locale} action={params.action} />
    </main>
  );
}
