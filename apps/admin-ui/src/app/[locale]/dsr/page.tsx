import Link from "next/link";
import { DSR_ACTIONS } from "@/lib/dsr-types";
import { isLocale, tFor, type Locale } from "@/i18n";
import { Callout } from "@/components/ui/linear";

/**
 * Data Protection Officer contact — the same privacy inbox the consent
 * surface uses (see `lib/consent-types.ts`). A mailto, not a fabricated
 * endpoint.
 */
const DPO_CONTACT_EMAIL = "privacy@humangr.com";

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

          <Callout tone="info">
            Identity verification is required when you start a request. The
            statutory response window is 30 days under LGPD and GDPR.
          </Callout>

          <section aria-labelledby="dsr-rights-title" className="lin-mt-lg">
            <h2
              id="dsr-rights-title"
              className="mb-3 text-[15px] font-[560] text-[color:var(--t1)]"
            >
              {t("dsr.landing.rights_section_title")}
            </h2>
            <ul
              data-testid="dsr-rights-list"
              className="grid list-none grid-cols-1 gap-3 p-0 sm:grid-cols-2"
            >
              {DSR_ACTIONS.map((action) => (
                <li key={action}>
                  <Link
                    href={`/${locale}/dsr/${action}`}
                    data-testid={`dsr-action-button-${action}`}
                    className="lin-card lin-card--pad lin-card--hover block h-full"
                  >
                    <span className="lin-card__title block">
                      {t(`dsr.rights.${action}.label`)}
                    </span>
                    <span className="mt-1 block text-[13px] leading-relaxed text-[color:var(--t2)]">
                      {t(`dsr.rights.${action}.description`)}
                    </span>
                    <span className="mt-2 block text-[12px] text-[color:var(--t3)]">
                      {t(`dsr.rights.${action}.legal_ref`)}
                    </span>
                  </Link>
                </li>
              ))}
            </ul>
          </section>

          <p className="lin-mt-lg">
            <Link
              href={`/${locale}/dsr/status`}
              data-testid="dsr-status-link"
              className="lin-btn lin-btn--primary"
            >
              {t("dsr.landing.status_link")}
            </Link>
          </p>

          <section
            aria-labelledby="dsr-contact-title"
            className="lin-mt-lg"
          >
            <h2
              id="dsr-contact-title"
              className="mb-3 text-[15px] font-[560] text-[color:var(--t1)]"
            >
              Questions or help
            </h2>
            <p className="text-[13px] leading-relaxed text-[color:var(--t2)]">
              Read our{" "}
              <Link href={`/${locale}/privacy`}>privacy notice</Link> to see
              how we handle your personal data, or reach our Data Protection
              Officer at{" "}
              <a href={`mailto:${DPO_CONTACT_EMAIL}`}>{DPO_CONTACT_EMAIL}</a>.
            </p>
          </section>
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
