"use client";

import * as React from "react";
import type {
  ConsentSixFields,
  DataCategory,
  LegalBasis,
  RetentionPeriod,
} from "@/lib/consent-types";
import {
  DATA_CATEGORY_OPTIONS,
  LEGAL_BASIS_OPTIONS,
  RETENTION_OPTIONS,
} from "@/lib/consent-types";

export interface ConsentFormProps {
  value: ConsentSixFields;
  onChange: (next: ConsentSixFields) => void;
  /** Locale string for label translation. */
  locale: string;
  /** Captured timestamp displayed in the screenshot evidence. */
  capturedAtMs: number;
  /** Read-only mode (review step). */
  readOnly?: boolean;
  /**
   * React 19 ref-as-prop (replaces forwardRef pattern).
   * Typed as a plain mutable-ref shape to avoid dual @types/react conflicts
   * between the admin-ui (v19) and docs-site (v18) workspace packages.
   */
  ref?: { current: HTMLDivElement | null } | null;
}

// English fallback labels. Real i18n strings ship via WI-S16-006.
const LABELS: Record<string, Record<string, string>> = {
  "en-US": {
    purpose: "1. Purpose (what data + why)",
    legal_basis: "2. Legal basis",
    data_categories: "3. Data categories",
    retention_period: "4. Retention period",
    third_parties: "5. Sub-processors (read-only)",
    withdrawal_method: "6. Withdrawal method (read-only)",
    captured_at: "Captured at",
  },
  "pt-BR": {
    purpose: "1. Finalidade (quais dados + porque)",
    legal_basis: "2. Base legal",
    data_categories: "3. Categorias de dados",
    retention_period: "4. Periodo de retencao",
    third_parties: "5. Sub-processadores (somente leitura)",
    withdrawal_method: "6. Metodo de revogacao (somente leitura)",
    captured_at: "Capturado em",
  },
  "es-419": {
    purpose: "1. Finalidad (que datos + por que)",
    legal_basis: "2. Base legal",
    data_categories: "3. Categorias de datos",
    retention_period: "4. Periodo de retencion",
    third_parties: "5. Sub-procesadores (solo lectura)",
    withdrawal_method: "6. Metodo de revocacion (solo lectura)",
    captured_at: "Capturado a las",
  },
};

function labelsFor(locale: string): Record<string, string> {
  return LABELS[locale] ?? LABELS["en-US"]!;
}

export function ConsentForm({ value, onChange, locale, capturedAtMs, readOnly, ref }: ConsentFormProps) {
    const t = labelsFor(locale);

    function patch<K extends keyof ConsentSixFields>(key: K, v: ConsentSixFields[K]) {
      onChange({ ...value, [key]: v });
    }

    function toggleCategory(cat: DataCategory) {
      const next = value.data_categories.includes(cat)
        ? value.data_categories.filter((c) => c !== cat)
        : [...value.data_categories, cat];
      patch("data_categories", next);
    }

    return (
      <div
        ref={ref as React.RefObject<HTMLDivElement> | null | undefined}
        // CTRL-PRIV-001 / EVT-012 privacy guard: analytics + session-replay
        // tools MUST treat .privacy-no-capture as opt-out.
        className="privacy-no-capture consent-form"
        data-testid="consent-form"
        role="form"
        aria-label="Consent six-field capture form"
      >
        <div>
          <label htmlFor="consent-purpose">{t.purpose}</label>
          <textarea
            id="consent-purpose"
            data-testid="field-purpose"
            value={value.purpose}
            onChange={(e) => patch("purpose", e.target.value)}
            readOnly={readOnly}
            required
            aria-required="true"
          />
        </div>

        <div>
          <label htmlFor="consent-legal-basis">{t.legal_basis}</label>
          <select
            id="consent-legal-basis"
            data-testid="field-legal-basis"
            value={value.legal_basis}
            onChange={(e) => patch("legal_basis", e.target.value as LegalBasis)}
            disabled={readOnly}
            aria-required="true"
          >
            {LEGAL_BASIS_OPTIONS.map((o) => (
              <option key={o} value={o}>
                {o}
              </option>
            ))}
          </select>
        </div>

        <fieldset>
          <legend id="consent-categories-legend">{t.data_categories}</legend>
          <div role="group" aria-labelledby="consent-categories-legend" data-testid="field-data-categories">
            {DATA_CATEGORY_OPTIONS.map((cat) => {
              const id = `consent-cat-${cat}`;
              return (
                <label key={cat} htmlFor={id}>
                  <input
                    id={id}
                    type="checkbox"
                    data-testid={`cat-${cat}`}
                    checked={value.data_categories.includes(cat)}
                    onChange={() => toggleCategory(cat)}
                    disabled={readOnly}
                  />
                  {cat}
                </label>
              );
            })}
          </div>
        </fieldset>

        <div>
          <label htmlFor="consent-retention">{t.retention_period}</label>
          <select
            id="consent-retention"
            data-testid="field-retention"
            value={value.retention_period}
            onChange={(e) => patch("retention_period", e.target.value as RetentionPeriod)}
            disabled={readOnly}
            aria-required="true"
          >
            {RETENTION_OPTIONS.map((o) => (
              <option key={o} value={o}>
                {o}
              </option>
            ))}
          </select>
        </div>

        <div>
          <span id="consent-third-parties-label">{t.third_parties}</span>
          <ul
            aria-labelledby="consent-third-parties-label"
            data-testid="field-third-parties"
          >
            {value.third_parties.length === 0 ? (
              <li data-testid="third-parties-empty">(none)</li>
            ) : (
              value.third_parties.map((p) => <li key={p}>{p}</li>)
            )}
          </ul>
        </div>

        <div>
          <span id="consent-withdrawal-label">{t.withdrawal_method}</span>
          <p
            aria-labelledby="consent-withdrawal-label"
            data-testid="field-withdrawal-method"
            aria-readonly="true"
          >
            {value.withdrawal_method}
          </p>
        </div>

        <p data-testid="captured-at" aria-label="captured-at">
          {t.captured_at}: <time dateTime={new Date(capturedAtMs).toISOString()}>{new Date(capturedAtMs).toISOString()}</time>
        </p>
      </div>
    );
}
