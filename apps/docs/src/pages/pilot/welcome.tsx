/**
 * Pilot welcome page — `humangr.com/corelink/docs/pilot/welcome`.
 *
 * R-prep wave-29 stream-2 deliverable. Reached after successful POST
 * to `corelink-api.humangr.com/v1/signup/pilot/{token}` from `./apply.tsx`.
 *
 * Confetti is intentionally tiny (8 emoji spans + a CSS animation in
 * `pilot.module.css`) — no third-party dependency, no canvas. The
 * burst is gated on the `?slot=reserved` query so direct visits to
 * /pilot/welcome don't display celebration without a real signup.
 *
 * The onboarding next-steps mirror the COPY.md post-submit promise:
 *   "We'll review within 2 business days. If accepted, we'll send
 *    activation details and the pilot Slack invite within 5 business
 *    days of acceptance."
 */

import type { ReactElement } from "react";
import { useEffect, useState } from "react";
import Layout from "@theme/Layout";
import Translate, { translate } from "@docusaurus/Translate";
import styles from "./pilot.module.css";

const CONFETTI_PIECES = ["✦", "✧", "✺", "✹", "✸", "✷", "✶", "✵"] as const;

function useReservedQuery(): boolean {
  const [reserved, setReserved] = useState(false);
  useEffect(() => {
    if (typeof window === "undefined") return;
    const params = new URLSearchParams(window.location.search);
    setReserved(params.get("slot") === "reserved");
  }, []);
  return reserved;
}

export default function PilotWelcome(): ReactElement {
  const reserved = useReservedQuery();

  return (
    <Layout
      title={translate({
        id: "pilot.welcome.meta.title",
        message: "Welcome to CoreLink Pilot",
        description: "Pilot welcome page — <title>",
      })}
      description={translate({
        id: "pilot.welcome.meta.description",
        message:
          "Thanks for applying. We'll review within 2 business days and send activation + pilot Slack invite within 5 business days of acceptance.",
        description: "Pilot welcome page — meta description",
      })}
    >
      <main className={styles.page}>
        <div className={styles.welcomeCard}>
          <span className={styles.welcomeIcon} aria-hidden="true">
            {reserved ? "🎉" : "✦"}
          </span>
          {reserved ? (
            <div aria-hidden="true">
              {CONFETTI_PIECES.map((piece, idx) => (
                <span
                  key={`${piece}-${idx}`}
                  style={{
                    display: "inline-block",
                    margin: "0 0.25rem",
                    fontSize: "1.25rem",
                    color: "var(--ifm-color-primary)",
                    animation: `pilot-pop 600ms ease-out ${idx * 40}ms both`,
                  }}
                >
                  {piece}
                </span>
              ))}
              <style>{`
                @keyframes pilot-pop {
                  0%   { opacity: 0; transform: translateY(8px) scale(0.85); }
                  60%  { opacity: 1; transform: translateY(-4px) scale(1.1); }
                  100% { opacity: 1; transform: translateY(0)     scale(1); }
                }
              `}</style>
            </div>
          ) : null}

          <h1>
            <Translate
              id="pilot.welcome.title"
              description="Pilot welcome — H1"
            >
              Thanks for applying
            </Translate>
          </h1>
          <p>
            <Translate
              id="pilot.welcome.body"
              description="Pilot welcome — body paragraph"
            >
              {
                "We'll review your application within 2 business days. " +
                "If accepted, we'll send activation details and the pilot " +
                "Slack invite within 5 business days of acceptance."
              }
            </Translate>
          </p>

          <h2>
            <Translate
              id="pilot.welcome.next"
              description="Pilot welcome — next-steps heading"
            >
              While you wait
            </Translate>
          </h2>
          <ol className={styles.stepList}>
            <li>
              <Translate
                id="pilot.welcome.step.quickstart"
                description="Pilot welcome — next-steps item 1"
              >
                Read the 10-minute quickstart at /tutorials/quickstart-10min.
              </Translate>
            </li>
            <li>
              <Translate
                id="pilot.welcome.step.architecture"
                description="Pilot welcome — next-steps item 2"
              >
                Skim the architecture explainer (/explanation/architecture) — TLA+ isolation + Merkle audit + 4-region replication.
              </Translate>
            </li>
            <li>
              <Translate
                id="pilot.welcome.step.security"
                description="Pilot welcome — next-steps item 3"
              >
                Review the security + pre-GA posture at /security.
              </Translate>
            </li>
            <li>
              <Translate
                id="pilot.welcome.step.contact"
                description="Pilot welcome — next-steps item 4"
              >
                Email pilot@humangr.com with any questions before activation.
              </Translate>
            </li>
          </ol>
        </div>
      </main>
    </Layout>
  );
}
