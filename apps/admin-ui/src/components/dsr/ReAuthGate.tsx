"use client";

import { useCallback, useState, type ReactNode } from "react";
import { tFor, type Locale } from "@/i18n";
import { isMfaFresh } from "@/lib/dsr-client";
import { Badge, Button, Callout, Card } from "@/components/ui/linear";

/**
 * Identity re-auth gate (CTRL-AUTH-010).
 *
 * Wraps a child form. Renders MFA challenge UI until the supplied
 * `verifier` resolves successfully. Once verified, the child is rendered
 * and the gate exposes a `verifiedAt` timestamp that ancestor forms can
 * use to gate `submit`.
 *
 * IMPORTANT: re-auth is required for EVERY DSR submit (no session-wide
 * bypass) — DSR ops are sensitive. The gate also locks itself if the
 * verifier reports an expired session.
 */

export interface ReAuthVerifier {
  /** Start the Clerk MFA challenge; resolves with the verification time (ms). */
  startVerification(): Promise<number>;
}

export interface ReAuthGateProps {
  locale: Locale;
  verifier: ReAuthVerifier;
  /**
   * Render-prop for the protected form. Receives the `verifiedAt`
   * timestamp so the child can pass it to its submit handler.
   */
  children: (ctx: { verifiedAt: number }) => ReactNode;
  /** Optional initial value (test-only). */
  initialVerifiedAt?: number | null;
}

export function ReAuthGate(props: ReAuthGateProps) {
  const [verifiedAt, setVerifiedAt] = useState<number | null>(
    props.initialVerifiedAt ?? null,
  );
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const t = (key: string) => tFor(props.locale, key);

  const startVerify = useCallback(async () => {
    setPending(true);
    setError(null);
    try {
      const at = await props.verifier.startVerification();
      setVerifiedAt(at);
    } catch {
      setError(tFor(props.locale, "dsr.reauth.failed"));
      setVerifiedAt(null);
    } finally {
      setPending(false);
    }
  }, [props.verifier, props.locale]);

  const fresh = isMfaFresh(verifiedAt);
  if (!fresh) {
    return (
      <section
        aria-labelledby="dsr-reauth-title"
        data-testid="dsr-reauth-gate"
        className="lin-checklist"
      >
        <Card>
          <h2 id="dsr-reauth-title" className="lin-card__title">
            {t("dsr.reauth.title")}
          </h2>
          <p className="lin-card__meta">{t("dsr.reauth.description")}</p>
          <Callout tone="warn">
            <span role="note">{t("dsr.reauth.blocked_notice")}</span>
          </Callout>
          <div>
            <Button
              onClick={startVerify}
              loading={pending}
              disabled={pending}
              data-testid="dsr-reauth-start"
            >
              {pending
                ? t("dsr.reauth.verifying")
                : t("dsr.reauth.start_button")}
            </Button>
          </div>
          {error ? (
            <div role="alert" data-testid="dsr-reauth-error">
              <Callout tone="danger">{error}</Callout>
            </div>
          ) : null}
        </Card>
      </section>
    );
  }

  return (
    <div data-testid="dsr-reauth-verified" className="lin-checklist">
      <p role="status">
        <Badge tone="success" dot>
          {t("dsr.reauth.verified")}
        </Badge>
      </p>
      {props.children({ verifiedAt: verifiedAt as number })}
    </div>
  );
}
