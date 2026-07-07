"use client";

import { useEffect, useId, useState, type FormEvent } from "react";
import {
  REASON_REQUIRED_ACTIONS,
  type DataCategory,
  type DsrAction,
  type DsrSubmitRequest,
  type DsrSubmitResponse,
  type MeProfile,
} from "@/lib/dsr-types";
import { tFor, type Locale } from "@/i18n";
import { Button, Callout, Field, Input, Textarea } from "@/components/ui/linear";

export interface DsrActionFormProps {
  locale: Locale;
  action: DsrAction;
  profile: MeProfile;
  categories: DataCategory[];
  /**
   * Submit handler — must always be wired to a DSR client that uses a fresh
   * MFA token. The host page is responsible for wrapping this form in a
   * `<ReAuthGate>` (see `dsr/[action]/page.tsx`).
   */
  onSubmit: (req: DsrSubmitRequest) => Promise<DsrSubmitResponse>;
  /** Set by ReAuthGate; required before submit can fire. */
  mfaVerifiedAt: number;
}

export function DsrActionForm(props: DsrActionFormProps) {
  const t = (k: string) => tFor(props.locale, k);
  const reasonRequired = REASON_REQUIRED_ACTIONS.has(props.action);
  const uid = useId();

  // --- shared
  const [reason, setReason] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // --- rectification
  const [recName, setRecName] = useState(props.profile.name ?? "");
  const [recEmail, setRecEmail] = useState(props.profile.email);
  const [recLanguage, setRecLanguage] = useState(
    props.profile.language ?? props.locale,
  );

  // --- erasure
  const [erasureScope, setErasureScope] = useState<"all" | "categories">(
    "all",
  );
  const [erasureCats, setErasureCats] = useState<string[]>([]);

  // --- portability
  const [portabilityFormat, setPortabilityFormat] = useState<
    "json" | "csv" | "both"
  >("json");

  // --- objection
  const [objectionPurpose, setObjectionPurpose] = useState("");

  // Refresh rectification fields if profile changes (test-friendly).
  useEffect(() => {
    setRecName(props.profile.name ?? "");
    setRecEmail(props.profile.email);
    setRecLanguage(props.profile.language ?? props.locale);
  }, [props.profile, props.locale]);

  const handleSubmit = async (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault();
    setError(null);

    if (!props.mfaVerifiedAt) {
      // Defensive: ReAuthGate should already block this path.
      setError(t("dsr.form.submit_error_mfa"));
      return;
    }
    if (reasonRequired && reason.trim().length === 0) {
      setError(t("dsr.form.reason_required_error"));
      return;
    }

    const req: DsrSubmitRequest = { action: props.action };
    if (reason.trim().length > 0) req.reason = reason.trim();

    switch (props.action) {
      case "rectification":
        req.rectification = {
          name: recName,
          email: recEmail,
          language: recLanguage,
        };
        break;
      case "erasure":
        req.erasure =
          erasureScope === "all"
            ? { scope: "all" }
            : { scope: "categories", categories: erasureCats };
        break;
      case "portability":
        req.portability = { format: portabilityFormat };
        break;
      case "objection":
        req.objection = { purpose: objectionPurpose };
        break;
      default:
        break;
    }

    setSubmitting(true);
    try {
      await props.onSubmit(req);
    } catch (err) {
      const key =
        err && typeof err === "object" && "translationKey" in err
          ? String((err as { translationKey: unknown }).translationKey)
          : "dsr.form.submit_error_generic";
      setError(t(key));
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <form
      onSubmit={handleSubmit}
      data-testid={`dsr-form-${props.action}`}
      noValidate
      className="lin-checklist"
    >
      <fieldset className="lin-card lin-card--pad">
        <legend className="lin-card__title">
          {t("dsr.form.subject_section")}
        </legend>
        <Field
          label={t("dsr.form.subject_email_label")}
          htmlFor={`${uid}-subject-email`}
        >
          <Input
            id={`${uid}-subject-email`}
            type="email"
            value={props.profile.email}
            readOnly
            data-testid="dsr-subject-email"
          />
        </Field>
      </fieldset>

      {props.action === "rectification" ? (
        <fieldset
          className="lin-card lin-card--pad"
          data-testid="dsr-rectification-fields"
        >
          <legend className="lin-card__title">
            {t("dsr.form.rectification_fields_title")}
          </legend>
          <div className="lin-checklist">
            <Field label={t("dsr.form.field_name")} htmlFor={`${uid}-rec-name`}>
              <Input
                id={`${uid}-rec-name`}
                type="text"
                value={recName}
                onChange={(e) => setRecName(e.target.value)}
                data-testid="dsr-field-name"
              />
            </Field>
            <Field
              label={t("dsr.form.field_email")}
              htmlFor={`${uid}-rec-email`}
            >
              <Input
                id={`${uid}-rec-email`}
                type="email"
                value={recEmail}
                onChange={(e) => setRecEmail(e.target.value)}
                data-testid="dsr-field-email"
              />
            </Field>
            <Field
              label={t("dsr.form.field_language")}
              htmlFor={`${uid}-rec-language`}
            >
              <Input
                id={`${uid}-rec-language`}
                type="text"
                value={recLanguage}
                onChange={(e) => setRecLanguage(e.target.value)}
                data-testid="dsr-field-language"
              />
            </Field>
          </div>
        </fieldset>
      ) : null}

      {props.action === "erasure" ? (
        <fieldset
          className="lin-card lin-card--pad"
          data-testid="dsr-erasure-fields"
        >
          <legend className="lin-card__title">
            {t("dsr.form.erasure_scope_title")}
          </legend>
          <Callout tone="danger">
            <span role="note">{t("dsr.form.erasure_legal_hold_warning")}</span>
          </Callout>
          <ul>
            <li>
              <label>
                <input
                  type="radio"
                  name="erasure-scope"
                  value="all"
                  checked={erasureScope === "all"}
                  onChange={() => setErasureScope("all")}
                  data-testid="dsr-erasure-scope-all"
                />{" "}
                {t("dsr.form.erasure_scope_all")}
              </label>
            </li>
            <li>
              <label>
                <input
                  type="radio"
                  name="erasure-scope"
                  value="categories"
                  checked={erasureScope === "categories"}
                  onChange={() => setErasureScope("categories")}
                  data-testid="dsr-erasure-scope-categories"
                />{" "}
                {t("dsr.form.erasure_scope_categories")}
              </label>
            </li>
          </ul>
          {erasureScope === "categories" ? (
            <ul data-testid="dsr-erasure-categories">
              {props.categories.map((cat) => (
                <li key={cat.id}>
                  <label>
                    <input
                      type="checkbox"
                      checked={erasureCats.includes(cat.id)}
                      onChange={(e) => {
                        setErasureCats((prev) =>
                          e.target.checked
                            ? [...prev, cat.id]
                            : prev.filter((c) => c !== cat.id),
                        );
                      }}
                      data-testid={`dsr-erasure-cat-${cat.id}`}
                    />{" "}
                    {cat.label}
                  </label>
                </li>
              ))}
            </ul>
          ) : null}
        </fieldset>
      ) : null}

      {props.action === "portability" ? (
        <fieldset
          className="lin-card lin-card--pad"
          data-testid="dsr-portability-fields"
        >
          <legend className="lin-card__title">
            {t("dsr.form.portability_format_title")}
          </legend>
          <ul>
            {(["json", "csv", "both"] as const).map((fmt) => (
              <li key={fmt}>
                <label>
                  <input
                    type="radio"
                    name="portability-format"
                    value={fmt}
                    checked={portabilityFormat === fmt}
                    onChange={() => setPortabilityFormat(fmt)}
                    data-testid={`dsr-portability-format-${fmt}`}
                  />{" "}
                  {t(`dsr.form.portability_format_${fmt}`)}
                </label>
              </li>
            ))}
          </ul>
          <p>{t("dsr.form.portability_notice")}</p>
        </fieldset>
      ) : null}

      {props.action === "objection" ? (
        <fieldset
          className="lin-card lin-card--pad"
          data-testid="dsr-objection-fields"
        >
          <Field
            label={t("dsr.form.objection_purpose_label")}
            htmlFor={`${uid}-objection-purpose`}
          >
            <Input
              id={`${uid}-objection-purpose`}
              type="text"
              value={objectionPurpose}
              onChange={(e) => setObjectionPurpose(e.target.value)}
              data-testid="dsr-objection-purpose"
            />
          </Field>
        </fieldset>
      ) : null}

      <div className="lin-card lin-card--pad">
        <Field
          label={`${t("dsr.form.reason_label")}${reasonRequired ? " *" : ""}`}
          htmlFor={`${uid}-reason`}
        >
          <Textarea
            id={`${uid}-reason`}
            value={reason}
            onChange={(e) => setReason(e.target.value)}
            placeholder={t("dsr.form.reason_placeholder")}
            required={reasonRequired}
            data-testid="dsr-reason"
          />
        </Field>
      </div>

      {error ? (
        <div role="alert" data-testid="dsr-form-error">
          <Callout tone="danger">{error}</Callout>
        </div>
      ) : null}

      <div>
        <Button type="submit" loading={submitting} data-testid="dsr-submit">
          {submitting ? t("dsr.form.submitting") : t("dsr.form.submit")}
        </Button>
      </div>
    </form>
  );
}
