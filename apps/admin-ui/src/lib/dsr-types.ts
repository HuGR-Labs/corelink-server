/**
 * Canonical DSR types shared between client + tests.
 *
 * The 6 actions match WI-S11-001 backend's `DsrRequestKind`:
 *   Access | Portability | Rectification | Erasure | Restriction | Objection
 *
 * Backend endpoints (per WI-S11-001 §1):
 *   POST /v1/privacy/dsr/{action}
 *   GET  /v1/privacy/dsr/{request_id}/status
 *   POST /v1/privacy/dsr/{request_id}/verify-mfa
 *   GET  /v1/privacy/dsr/verify?token=<jwt>
 */

export const DSR_ACTIONS = [
  "access",
  "rectification",
  "erasure",
  "portability",
  "restriction",
  "objection",
] as const;

export type DsrAction = (typeof DSR_ACTIONS)[number];

export function isDsrAction(value: string): value is DsrAction {
  return (DSR_ACTIONS as readonly string[]).includes(value);
}

/** Rights for which a `reason` field is mandatory. */
export const REASON_REQUIRED_ACTIONS: ReadonlySet<DsrAction> = new Set([
  "erasure",
  "restriction",
  "objection",
]);

export type DsrJurisdiction = "lgpd" | "gdpr" | "ccpa";

export type DsrStatus = "pending" | "in_progress" | "completed" | "rejected";

export interface DsrReceiptPayload {
  request_id: string;
  action: DsrAction;
  jurisdiction: DsrJurisdiction;
  sla_deadline: string; // ISO 8601
  jti: string;
  iat: number; // unix seconds
  exp: number; // unix seconds
  /** Hashed tenant identifier — never raw tenant_id (CTRL-PRIV-014). */
  tenant_id_hash?: string;
}

export interface DsrSubmitResponse {
  request_id: string;
  action: DsrAction;
  jurisdiction: DsrJurisdiction;
  sla_deadline: string;
  jwt_receipt: string;
}

export interface DsrSubmitRequest {
  action: DsrAction;
  reason?: string;
  // Rectification: which fields the subject wants to update.
  rectification?: {
    name?: string;
    email?: string;
    language?: string;
  };
  // Erasure: either "all" or a list of category IDs.
  erasure?:
    | { scope: "all" }
    | { scope: "categories"; categories: string[] };
  // Portability: requested export format(s).
  portability?: { format: "json" | "csv" | "both" };
  // Objection: which processing purpose to object to.
  objection?: { purpose: string };
}

export interface DsrRequestSummary {
  request_id: string;
  action: DsrAction;
  status: DsrStatus;
  submitted_at: string;
  sla_deadline: string;
  jurisdiction: DsrJurisdiction;
}

export interface DsrTimelineEvent {
  at: string;
  from?: DsrStatus;
  to: DsrStatus;
  note?: string;
}

export interface DsrRequestDetail extends DsrRequestSummary {
  timeline: DsrTimelineEvent[];
  /** Present once an Access/Portability export is ready. */
  data_download_url?: string;
}

export interface MeProfile {
  email: string;
  name?: string;
  language?: string;
}

export interface DataCategory {
  id: string;
  label: string;
}
