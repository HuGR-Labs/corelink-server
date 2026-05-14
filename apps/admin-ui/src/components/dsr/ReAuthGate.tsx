"use client";

import { useCallback, useState, type ReactNode } from "react";
import { tFor, type Locale } from "@/i18n";
import { isMfaFresh } from "@/lib/dsr-client";

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
      <section aria-labelledby="dsr-reauth-title" data-testid="dsr-reauth-gate">
        <h2 id="dsr-reauth-title">{t("dsr.reauth.title")}</h2>
        <p>{t("dsr.reauth.description")}</p>
        <p role="note">{t("dsr.reauth.blocked_notice")}</p>
        <button
          type="button"
          onClick={startVerify}
          disabled={pending}
          data-testid="dsr-reauth-start"
        >
          {pending ? t("dsr.reauth.verifying") : t("dsr.reauth.start_button")}
        </button>
        {error ? (
          <p role="alert" data-testid="dsr-reauth-error">
            {error}
          </p>
        ) : null}
      </section>
    );
  }

  return (
    <div data-testid="dsr-reauth-verified">
      <p role="status">{t("dsr.reauth.verified")}</p>
      {props.children({ verifiedAt: verifiedAt as number })}
    </div>
  );
}
