"use client";

import { useCallback, useMemo, useRef, useState } from "react";
import { v7 as uuidv7 } from "uuid";
import { ConsentForm } from "@/components/consent/ConsentForm";
import { ScrollToBottomGuard } from "@/components/consent/ScrollToBottomGuard";
import { JwtReceiptDisplay } from "@/components/consent/JwtReceiptDisplay";
import { captureConsentScreenshot } from "@/lib/consent-screenshot";
import { sha256Hex } from "@/lib/sha256";
import { safeLog } from "@/lib/safe-log";
import { getActiveLocale } from "@/lib/i18n";
import type { ConsentApi, ConsentGrantResponse } from "@/lib/consent-api";
import { useConsentApi } from "@/lib/use-consent-api";
import {
  WITHDRAWAL_METHOD_DEFAULT,
  type ConsentSixFields,
  type ConsentSubmitPayload,
} from "@/lib/consent-types";
import { Button, Callout, Card } from "@/components/ui/linear";

export interface ConsentCaptureFlowProps {
  /** Active locale resolved by Next.js [locale] segment. */
  locale: string;
  /** Plain-text notice rendered to the user (hashed client-side). */
  noticeText: string;
  /** Semver of the notice MDX (frontmatter `notice_version`). */
  noticeVersion: string;
  /** Read-only sub-processor list (resolved server-side or via api). */
  thirdParties: string[];
  api?: ConsentApi;
}

type Step = "review" | "scroll" | "consent" | "done";

export function ConsentCaptureFlow({
  locale,
  noticeText,
  noticeVersion,
  thirdParties,
  api: apiProp,
}: ConsentCaptureFlowProps) {
  const sessionApi = useConsentApi();
  const api = apiProp ?? sessionApi;
  const [step, setStep] = useState<Step>("review");
  const [scrolled, setScrolled] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [submitError, setSubmitError] = useState<string | null>(null);
  const [result, setResult] = useState<ConsentGrantResponse | null>(null);
  const formRootRef = useRef<HTMLDivElement | null>(null);

  const capturedAtMs = useMemo(() => Date.now(), []);
  const wording_id = useMemo(() => uuidv7(), []);

  const [fields, setFields] = useState<ConsentSixFields>(() => ({
    purpose: "",
    legal_basis: "consent",
    data_categories: [],
    retention_period: "1y",
    third_parties: thirdParties,
    withdrawal_method: WITHDRAWAL_METHOD_DEFAULT,
  }));

  const canSubmit =
    step === "consent" &&
    scrolled &&
    !submitting &&
    fields.purpose.trim().length > 0 &&
    fields.data_categories.length > 0;

  const onReachBottom = useCallback(() => {
    setScrolled(true);
    if (step === "scroll") setStep("consent");
  }, [step]);

  const handleSubmit = useCallback(async () => {
    setSubmitting(true);
    setSubmitError(null);
    try {
      // CTRL-PRIV-CONSENT-005 locale-match assertion (client-side guard).
      // The active rendered locale (cookie/<html lang>) MUST equal the
      // locale prop wired by the [locale] segment. We deliberately do NOT
      // consult navigator.language here (see lib/i18n.ts).
      const active = getActiveLocale();
      if (active !== locale) {
        throw new Error("locale_mismatch_client_guard");
      }

      const screenshot = await captureConsentScreenshot(formRootRef.current);
      const notice_text_hash = await sha256Hex(noticeText);

      const payload: ConsentSubmitPayload = {
        ...fields,
        locale: active,
        ui_capture_ts: capturedAtMs,
        wording_id,
        notice_text_hash,
        notice_version: noticeVersion,
        screenshot_evidence_base64: screenshot.data_url,
      };

      // CTRL-PRIV-001: no PII in client logs — safeLog redacts.
      safeLog("info", "consent_submit", {
        notice_version: payload.notice_version,
        locale: payload.locale,
        wording_id: payload.wording_id,
        screenshot_ok: screenshot.ok,
      });

      const res = await api.grant(payload);
      setResult(res);
      setStep("done");
    } catch (e: unknown) {
      setSubmitError(e instanceof Error ? e.message : "submit_failed");
    } finally {
      setSubmitting(false);
    }
  }, [api, capturedAtMs, fields, locale, noticeText, noticeVersion, wording_id]);

  if (step === "done" && result) {
    return (
      <div className="cx-shell lin">
        <div className="cx-main">
          <main id="main">
            <h1>Consent recorded</h1>
            <section aria-label="Consent receipt" data-testid="consent-success">
              <Callout tone="info">
                Audit event:{" "}
                <code data-testid="audit-event-id">
                  {result.audit_event_id}
                </code>
              </Callout>
              <JwtReceiptDisplay jwt={result.jwt_receipt} />
            </section>
          </main>
        </div>
      </div>
    );
  }

  return (
    // `<main id="main">` + a top-level `<h1>`: the flow previously rendered
    // only `<h2>` step headings inside a bare `<section>`, so the page had no
    // main landmark (axe `landmark-one-main`) and no level-1 heading
    // (`page-has-heading-one`) — both dragged the Lighthouse a11y category
    // below the 1.0 gate. The frozen customer-dashboard shell
    // (`cx-shell lin` + `cx-main`) supplies the a11y-validated dark tokens and
    // their validated backdrop; the single `<main id="main">` + `<h1>` are
    // preserved.
    <div className="cx-shell lin">
      <div className="cx-main">
        <main id="main">
          <h1>Grant consent</h1>
          <section
            aria-label="Capture consent"
            data-testid="consent-capture"
            className="lin-card lin-card--pad mt-6"
          >
            <ol
              aria-label="Steps"
              className="mb-6 flex list-none flex-wrap gap-x-5 gap-y-1 p-0 text-[12.5px] text-[color:var(--t3)]"
            >
              <li
                aria-current={step === "review" ? "step" : undefined}
                className="aria-[current=step]:font-[560] aria-[current=step]:text-[color:var(--t1)]"
              >
                1. Review
              </li>
              <li
                aria-current={step === "scroll" ? "step" : undefined}
                className="aria-[current=step]:font-[560] aria-[current=step]:text-[color:var(--t1)]"
              >
                2. Read in full
              </li>
              <li
                aria-current={step === "consent" ? "step" : undefined}
                className="aria-[current=step]:font-[560] aria-[current=step]:text-[color:var(--t1)]"
              >
                3. Consent
              </li>
            </ol>

            {step === "review" && (
              <div data-testid="step-review">
                <h2 className="mb-4 text-[16px] font-[560] text-[color:var(--t1)]">
                  What we are requesting
                </h2>
                <ConsentForm
                  ref={formRootRef}
                  value={fields}
                  onChange={setFields}
                  locale={locale}
                  capturedAtMs={capturedAtMs}
                />
                <Button
                  onClick={() => setStep("scroll")}
                  data-testid="to-scroll-step"
                  disabled={
                    fields.purpose.trim().length === 0 ||
                    fields.data_categories.length === 0
                  }
                >
                  Continue
                </Button>
              </div>
            )}

            {step === "scroll" && (
              <div data-testid="step-scroll">
                <h2>Please read the notice</h2>
                <ScrollToBottomGuard onReachBottom={onReachBottom}>
                  <Card>
                    <div data-testid="notice-body">{noticeText}</div>
                  </Card>
                </ScrollToBottomGuard>
              </div>
            )}

            {step === "consent" && (
              <div data-testid="step-consent">
                <h2>Confirm</h2>
                <ConsentForm
                  ref={formRootRef}
                  value={fields}
                  onChange={setFields}
                  locale={locale}
                  capturedAtMs={capturedAtMs}
                  readOnly
                />
                {submitError && (
                  <div role="alert" data-testid="submit-error">
                    <Callout tone="danger">{submitError}</Callout>
                  </div>
                )}
                <Button
                  onClick={handleSubmit}
                  disabled={!canSubmit}
                  aria-disabled={!canSubmit}
                  data-testid="consent-submit"
                >
                  I consent
                </Button>
              </div>
            )}
          </section>
        </main>
      </div>
    </div>
  );
}
