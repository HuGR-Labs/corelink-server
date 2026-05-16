/**
 * Pilot landing page — `docs.corelink.humangr.com/pilot`.
 *
 * R-prep wave-29 stream-2 deliverable. Source-of-truth copy:
 *   marketing/launch/PILOT-LANDING-PAGE-COPY.md (wave-28 step-7, b0485bc).
 *
 * Pairs with the `signup.corelink.humangr.com` backend handler built in stream-1.
 * Submitted form (see `./apply.tsx`) POSTs to
 *   `https://signup.corelink.humangr.com/v1/signup/pilot/{token}`
 *
 * Honest pre-GA framing is mandatory (wave-28 pilot comms precedent):
 * the in-page banner makes the pilot-vs-GA boundary explicit so it is
 * never a procurement surprise.
 *
 * i18n: served in 4 locales (en-US / pt-BR / de / es-419) via the
 * Docusaurus `<Translate>` extraction pipeline. Non-default locales
 * are EN-baseline pending native-speaker review per
 * `apps/docs/i18n/TRANSLATION-WORKFLOW.md` SLA (≤ 14 d of EN change).
 */

import type { ReactElement } from "react";
import Layout from "@theme/Layout";
import Link from "@docusaurus/Link";
import Translate, { translate } from "@docusaurus/Translate";
import styles from "./pilot.module.css";

type Feature = {
  readonly id: string;
  readonly icon: string;
  readonly title: ReactElement;
  readonly body: ReactElement;
};

const FEATURES: readonly Feature[] = [
  {
    id: "cas",
    icon: "▢", // outline square — content addressing
    title: (
      <Translate
        id="pilot.feature.cas.title"
        description="Pilot landing — feature block 1 title (CAS / tenant isolation)"
      >
        Tenant-isolated CAS, modelled in TLA+
      </Translate>
    ),
    body: (
      <Translate
        id="pilot.feature.cas.body"
        description="Pilot landing — feature block 1 body (CAS / tenant isolation)"
      >
        {
          "BLAKE3 + SHA-256 content addressing with a per-tenant namespace. " +
          "Cross-tenant byte non-leakage is a TLA+ safety property checked in CI " +
          "on every commit — not a code-review hope."
        }
      </Translate>
    ),
  },
  {
    id: "audit",
    icon: "⚭", // chain — audit chain
    title: (
      <Translate
        id="pilot.feature.audit.title"
        description="Pilot landing — feature block 2 title (Merkle audit chain)"
      >
        Cryptographic audit chain
      </Translate>
    ),
    body: (
      <Translate
        id="pilot.feature.audit.body"
        description="Pilot landing — feature block 2 body (Merkle audit chain)"
      >
        {
          "Every CAS read and write appends to a per-tenant append-only Merkle " +
          "log. Each entry is Ed25519-signed. Replayable from any verified anchor."
        }
      </Translate>
    ),
  },
  {
    id: "multiregion",
    icon: "⌖", // crosshair — multi-region routing
    title: (
      <Translate
        id="pilot.feature.region.title"
        description="Pilot landing — feature block 3 title (multi-region)"
      >
        Four-region replication
      </Translate>
    ),
    body: (
      <Translate
        id="pilot.feature.region.body"
        description="Pilot landing — feature block 3 body (multi-region)"
      >
        {
          "Active in US-East, EU-West, AP-Southeast, AU-East on Cloudflare R2 " +
          "+ Workers + D1. Per-tenant residency policy: EU bytes stay in EU, AU " +
          "bytes stay in AU."
        }
      </Translate>
    ),
  },
];

type Faq = {
  readonly id: string;
  readonly q: ReactElement;
  readonly a: ReactElement;
};

const FAQ: readonly Faq[] = [
  {
    id: "ga",
    q: (
      <Translate id="pilot.faq.ga.q" description="Pilot landing — FAQ 1 question">
        Is CoreLink GA?
      </Translate>
    ),
    a: (
      <Translate id="pilot.faq.ga.a" description="Pilot landing — FAQ 1 answer">
        {
          "No. The pilot is pre-GA. GA is gated on the criteria in " +
          "GA-GATE-CRITERIA.md, including a minimum of three ACTIVE pilots — " +
          "which is part of why we are running this program. We say so here " +
          "so it never becomes a procurement surprise."
        }
      </Translate>
    ),
  },
  {
    id: "convert",
    q: (
      <Translate id="pilot.faq.convert.q" description="Pilot landing — FAQ 2 question">
        What happens at GA?
      </Translate>
    ),
    a: (
      <Translate id="pilot.faq.convert.a" description="Pilot landing — FAQ 2 answer">
        {
          "Your pilot auto-converts to the STANDARD paid tier unless you opt " +
          "out. There is no card on file, so opt-out is the default if you do " +
          "nothing. STANDARD-tier pricing is published before any pilot's " +
          "30-day window expires."
        }
      </Translate>
    ),
  },
  {
    id: "prod",
    q: (
      <Translate id="pilot.faq.prod.q" description="Pilot landing — FAQ 3 question">
        Can I use CoreLink for production workloads during the pilot?
      </Translate>
    ),
    a: (
      <Translate id="pilot.faq.prod.a" description="Pilot landing — FAQ 3 answer">
        {
          "You can, and several pilot fits will be production-shaped. We will " +
          "not offer a contractual SLA during the pilot — we publish target " +
          "SLOs and run best-effort. If your production posture requires a " +
          "contractual SLA today, wait for GA."
        }
      </Translate>
    ),
  },
  {
    id: "access",
    q: (
      <Translate id="pilot.faq.access.q" description="Pilot landing — FAQ 4 question">
        Who can see my data?
      </Translate>
    ),
    a: (
      <Translate id="pilot.faq.access.a" description="Pilot landing — FAQ 4 answer">
        {
          "Only you and a tightly-scoped on-call rotation under documented " +
          "break-glass procedures. Cross-tenant byte access is mechanically " +
          "prevented by the TLA+-verified isolation property. BYOK " +
          "(cryptographic non-access by us) ships at GA."
        }
      </Translate>
    ),
  },
  {
    id: "leave",
    q: (
      <Translate id="pilot.faq.leave.q" description="Pilot landing — FAQ 5 question">
        What if I want to leave?
      </Translate>
    ),
    a: (
      <Translate id="pilot.faq.leave.a" description="Pilot landing — FAQ 5 answer">
        {
          "Terminate at any point during the pilot. You get 14 days to export " +
          "under tenant-scoped credentials, and we deliver a signed Ed25519 " +
          "erasure attestation referencing the audit-chain anchor of the " +
          "deletion. No clawbacks. No retention."
        }
      </Translate>
    ),
  },
];

export default function PilotLanding(): ReactElement {
  const title = translate({
    id: "pilot.meta.title",
    message: "CoreLink Pilot — Shared Content-Addressable Cache",
    description: "Pilot landing page — <title> tag",
  });
  const description = translate({
    id: "pilot.meta.description",
    message:
      "CoreLink Pilot — free 30-day pilot of a shared, tenant-isolated, content-addressable cache for builds, Docker, packages, and ML. Pre-GA. 10 slots.",
    description: "Pilot landing page — meta description",
  });

  return (
    <Layout title={title} description={description}>
      <main className={styles.page}>
        <div
          className={styles.banner}
          role="note"
          aria-label="Pre-GA pilot terms"
        >
          <strong>
            <Translate
              id="pilot.banner.label"
              description="Pilot landing — honest pre-GA banner label"
            >
              Pilot tier:
            </Translate>
          </strong>{" "}
          <Translate
            id="pilot.banner.body"
            description="Pilot landing — honest pre-GA banner body (quota + auto-convert)"
          >
            {
              "free 100 GB CAS + 10k audit events / month during eval " +
              "(30-day); auto-convert to STANDARD at GA."
            }
          </Translate>
        </div>

        <section className={styles.hero} aria-labelledby="pilot-hero-title">
          <h1 id="pilot-hero-title" className={styles.heroTitle}>
            <Translate
              id="pilot.hero.title"
              description="Pilot landing — hero H1"
            >
              Shared content-addressable cache for builds, packages, Docker, ML
            </Translate>
          </h1>
          <p className={styles.heroSub}>
            <Translate
              id="pilot.hero.sub"
              description="Pilot landing — hero subhead"
            >
              {
                "CoreLink deduplicates and audits the blobs your CI, your " +
                "registries, and your model store re-upload a hundred times " +
                "a day — across regions, across workloads, across teams — " +
                "with cryptographic tenant isolation and an append-only " +
                "Merkle audit chain. Pre-GA pilot. $0 for 30 days. 10 slots."
              }
            </Translate>
          </p>
          <Link
            to="/pilot/apply"
            className={styles.ctaPrimary}
            aria-label={translate({
              id: "pilot.hero.cta.aria",
              message: "Apply for a CoreLink pilot slot",
              description: "Pilot landing — hero CTA aria-label",
            })}
          >
            <Translate
              id="pilot.hero.cta"
              description="Pilot landing — hero CTA button label"
            >
              Apply for Pilot →
            </Translate>
          </Link>
          <small className={styles.ctaMeta}>
            <Translate
              id="pilot.hero.ctaMeta"
              description="Pilot landing — hero CTA sub-meta"
            >
              Token-gated · 2-business-day review · No card required
            </Translate>
          </small>
        </section>

        <section className={styles.section} aria-labelledby="pilot-features-title">
          <h2 id="pilot-features-title" className={styles.sectionTitle}>
            <Translate
              id="pilot.features.title"
              description="Pilot landing — features section heading"
            >
              What you get in the pilot
            </Translate>
          </h2>
          <div className={styles.featureGrid}>
            {FEATURES.map((feature) => (
              <article
                key={feature.id}
                className={styles.feature}
                aria-labelledby={`pilot-feature-${feature.id}-title`}
              >
                <span className={styles.featureIcon} aria-hidden="true">
                  {feature.icon}
                </span>
                <h3
                  id={`pilot-feature-${feature.id}-title`}
                  className={styles.featureTitle}
                >
                  {feature.title}
                </h3>
                <p className={styles.featureBody}>{feature.body}</p>
              </article>
            ))}
          </div>
        </section>

        <section className={styles.section} aria-labelledby="pilot-faq-title">
          <h2 id="pilot-faq-title" className={styles.sectionTitle}>
            <Translate
              id="pilot.faq.title"
              description="Pilot landing — FAQ section heading"
            >
              Frequently asked questions
            </Translate>
          </h2>
          <ul className={styles.faqList}>
            {FAQ.map((item) => (
              <li key={item.id} className={styles.faqItem}>
                <details>
                  <summary className={styles.faqSummary}>{item.q}</summary>
                  <p className={styles.faqBody}>{item.a}</p>
                </details>
              </li>
            ))}
          </ul>
        </section>

        <section className={styles.section} aria-labelledby="pilot-cta-footer">
          <h2 id="pilot-cta-footer" className={styles.sectionTitle}>
            <Translate
              id="pilot.footer.cta.title"
              description="Pilot landing — footer CTA heading"
            >
              Ready to apply?
            </Translate>
          </h2>
          <p>
            <Link to="/pilot/apply" className={styles.ctaPrimary}>
              <Translate
                id="pilot.footer.cta"
                description="Pilot landing — footer CTA button"
              >
                Apply for Pilot →
              </Translate>
            </Link>
          </p>
          <p className={styles.ctaMeta}>
            <Translate
              id="pilot.footer.contact"
              description="Pilot landing — footer contact line"
            >
              Questions before applying? pilot@humangr.com
            </Translate>
          </p>
        </section>
      </main>
    </Layout>
  );
}
