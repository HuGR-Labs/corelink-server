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
import { defaultConsentApi } from "@/lib/consent-api";
import {
  WITHDRAWAL_METHOD_DEFAULT,
  type ConsentSixFields,
  type ConsentSubmitPayload,
} from "@/lib/consent-types";

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
  api = defaultConsentApi,
}: ConsentCaptureFlowProps) {
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
      <section aria-label="Consent receipt" data-testid="consent-success">
        <h1>Consent recorded</h1>
        <p>
          Audit event: <code data-testid="audit-event-id">{result.audit_event_id}</code>
        </p>
        <JwtReceiptDisplay jwt={result.jwt_receipt} />
      </section>
    );
  }

  return (
    <section aria-label="Capture consent" data-testid="consent-capture">
      <ol aria-label="Steps">
        <li aria-current={step === "review" ? "step" : undefined}>1. Review</li>
        <li aria-current={step === "scroll" ? "step" : undefined}>2. Read in full</li>
        <li aria-current={step === "consent" ? "step" : undefined}>3. Consent</li>
      </ol>

      {step === "review" && (
        <div data-testid="step-review">
          <h2>What we are requesting</h2>
          <ConsentForm
            ref={formRootRef}
            value={fields}
            onChange={setFields}
            locale={locale}
            capturedAtMs={capturedAtMs}
          />
          <button
            type="button"
            onClick={() => setStep("scroll")}
            data-testid="to-scroll-step"
            disabled={fields.purpose.trim().length === 0 || fields.data_categories.length === 0}
          >
            Continue
          </button>
        </div>
      )}

      {step === "scroll" && (
        <div data-testid="step-scroll">
          <h2>Please read the notice</h2>
          <ScrollToBottomGuard onReachBottom={onReachBottom}>
            <pre data-testid="notice-body" style={{ whiteSpace: "pre-wrap" }}>
              {noticeText}
            </pre>
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
            <p role="alert" data-testid="submit-error">
              {submitError}
            </p>
          )}
          <button
            type="button"
            onClick={handleSubmit}
            disabled={!canSubmit}
            aria-disabled={!canSubmit}
            data-testid="consent-submit"
          >
            I consent
          </button>
        </div>
      )}
    </section>
  );
}
