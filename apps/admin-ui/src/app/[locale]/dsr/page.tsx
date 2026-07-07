import Link from "next/link";
import { DSR_ACTIONS } from "@/lib/dsr-types";
import { isLocale, tFor, type Locale } from "@/i18n";

interface PageProps {
  params: Promise<{ locale: string }>;
}

/**
 * DSR landing page (`/<locale>/dsr`).
 *
 * Lists the 6 rights with legal references and renders one large action
 * button per right. Pure server component — no Clerk session needed at
 * this level; identity re-auth is enforced inside each action route.
 *
 * Linear-doctrine surface: reuses the frozen customer-dashboard shell
 * (`cx-shell lin` + `cx-main`) so the a11y-validated dark tokens have their
 * validated backdrop; each right is a glass Card.
 */
export default async function DsrLandingPage({ params }: PageProps) {
  const { locale: rawLocale } = await params;
  const locale: Locale = isLocale(rawLocale) ? rawLocale : "en";
  const t = (k: string) => tFor(locale, k);

  return (
    <div className="cx-shell lin">
      <div className="cx-main">
        <main aria-labelledby="dsr-landing-title">
          <h1 id="dsr-landing-title">{t("dsr.landing.title")}</h1>
          <p>{t("dsr.landing.intro")}</p>

          <section aria-labelledby="dsr-rights-title">
            <h2 id="dsr-rights-title">
              {t("dsr.landing.rights_section_title")}
            </h2>
            <ul data-testid="dsr-rights-list">
              {DSR_ACTIONS.map((action) => (
                <li key={action}>
                  <Link
                    href={`/${locale}/dsr/${action}`}
                    data-testid={`dsr-action-button-${action}`}
                  >
                    <strong>{t(`dsr.rights.${action}.label`)}</strong>
                    <span>{t(`dsr.rights.${action}.description`)}</span>{" "}
                    <small>{t(`dsr.rights.${action}.legal_ref`)}</small>
                  </Link>
                </li>
              ))}
            </ul>
          </section>

          <p>
            <Link
              href={`/${locale}/dsr/status`}
              data-testid="dsr-status-link"
              className="lin-btn lin-btn--primary"
            >
              {t("dsr.landing.status_link")}
            </Link>
          </p>
        </main>
      </div>
    </div>
  );
}

export function generateStaticParams() {
  // R-prep i18n-de — `de` joined as the fourth canonical locale.
  return [
    { locale: "en" },
    { locale: "pt" },
    { locale: "es" },
    { locale: "de" },
  ];
}
