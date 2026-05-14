// The 6 CTRL-PRIV-CONSENT fields (per spec §17 + privacy_model.md §6).
// CTRL-PRIV-CONSENT-001..006 reflection.

import type { Locale } from "@/lib/i18n";

export type LegalBasis =
  | "consent"
  | "contract"
  | "legitimate-interest"
  | "legal-obligation"
  | "vital-interest"
  | "public-task";

export type RetentionPeriod = "30d" | "90d" | "1y" | "7y" | "indefinite-justified";

export type DataCategory =
  | "identifier"
  | "contact"
  | "billing"
  | "usage"
  | "technical"
  | "derived";

export interface ConsentSixFields {
  /** CTRL-PRIV-CONSENT-001 — what data + why (free text required). */
  purpose: string;
  /** CTRL-PRIV-CONSENT-002 — one of 6 canonical legal bases. */
  legal_basis: LegalBasis;
  /** CTRL-PRIV-CONSENT-003 — multi-select tag list. */
  data_categories: DataCategory[];
  /** CTRL-PRIV-CONSENT-004 — retention period choice. */
  retention_period: RetentionPeriod;
  /** CTRL-PRIV-CONSENT-005 — read-only sub-processor list (backend). */
  third_parties: string[];
  /** CTRL-PRIV-CONSENT-006 — auto-filled withdrawal pointer. */
  withdrawal_method: string;
}

export const WITHDRAWAL_METHOD_DEFAULT =
  "Withdraw via /consent/withdraw or email privacy@corelink.dev";

export interface ConsentSubmitPayload extends ConsentSixFields {
  locale: Locale;
  ui_capture_ts: number;
  wording_id: string;
  notice_text_hash: string;
  notice_version: string;
  screenshot_evidence_base64: string | null;
}

export interface ConsentRow {
  id: string;
  purpose: string;
  granted_at: string;
  status: "active" | "withdrawn";
}

export interface JwtReceiptClaims {
  tenant_id: string;
  consent_id: string;
  granted_at: string;
  locale: string;
  jti: string;
  exp: number;
}

export const LEGAL_BASIS_OPTIONS: LegalBasis[] = [
  "consent",
  "contract",
  "legitimate-interest",
  "legal-obligation",
  "vital-interest",
  "public-task",
];

export const RETENTION_OPTIONS: RetentionPeriod[] = [
  "30d",
  "90d",
  "1y",
  "7y",
  "indefinite-justified",
];

export const DATA_CATEGORY_OPTIONS: DataCategory[] = [
  "identifier",
  "contact",
  "billing",
  "usage",
  "technical",
  "derived",
];
