import Link from "next/link";
import { DSR_ACTIONS } from "@/lib/dsr-types";
import { isLocale, tFor, type Locale } from "@/i18n";

interface PageProps {
  params: { locale: string };
}

/**
 * DSR landing page (`/<locale>/dsr`).
 *
 * Lists the 6 rights with legal references and renders one large action
 * button per right. Pure server component — no Clerk session needed at
 * this level; identity re-auth is enforced inside each action route.
 */
export default function DsrLandingPage({ params }: PageProps) {
  const locale: Locale = isLocale(params.locale) ? params.locale : "en";
  const t = (k: string) => tFor(locale, k);

  return (
    <main aria-labelledby="dsr-landing-title">
      <h1 id="dsr-landing-title">{t("dsr.landing.title")}</h1>
      <p>{t("dsr.landing.intro")}</p>

      <section aria-labelledby="dsr-rights-title">
        <h2 id="dsr-rights-title">{t("dsr.landing.rights_section_title")}</h2>
        <ul data-testid="dsr-rights-list">
          {DSR_ACTIONS.map((action) => (
            <li key={action}>
              <Link
                href={`/${locale}/dsr/${action}`}
                data-testid={`dsr-action-button-${action}`}
              >
                <strong>{t(`dsr.rights.${action}.label`)}</strong>
                <span>{t(`dsr.rights.${action}.description`)}</span>
                <small>{t(`dsr.rights.${action}.legal_ref`)}</small>
              </Link>
            </li>
          ))}
        </ul>
      </section>

      <p>
        <Link href={`/${locale}/dsr/status`} data-testid="dsr-status-link">
          {t("dsr.landing.status_link")}
        </Link>
      </p>
    </main>
  );
}

export function generateStaticParams() {
  return [{ locale: "en" }, { locale: "pt" }, { locale: "es" }];
}
