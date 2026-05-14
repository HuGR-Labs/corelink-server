// WI-S16-005 — shared types for admin ops UI.

export type EventType =
  | "auth.login"
  | "auth.mfa_step_up"
  | "tenant.created"
  | "tenant.deleted"
  | "byok.cmk_rotated"
  | "dsr.request"
  | "admin.op_submitted"
  | "admin.op_approved"
  | "admin.op_rejected";

export type Severity = "info" | "warn" | "critical";

export interface AuditEventSummary {
  event_id: string;
  ts: string; // ISO-8601
  tenant_id: string;
  event_type: EventType | string;
  severity: Severity;
  actor: string; // user_id hash or "system"
  summary: string;
  correlation_id: string;
}

export interface MerkleProof {
  leaf_hash: string; // hex
  siblings: Array<{ hash: string; position: "left" | "right" }>;
  expected_root: string; // hex
  algorithm: "sha256" | "blake3";
}

export interface AuditEventDetail extends AuditEventSummary {
  cloudevent: {
    specversion: string;
    id: string;
    source: string;
    type: string;
    time: string;
    datacontenttype: string;
    subject?: string;
  };
  payload: Record<string, unknown>;
  merkle_proof: MerkleProof;
  r2_url: string;
}

export interface AuditFilter {
  tenant_id?: string;
  event_types?: string[];
  since?: string;
  until?: string;
  severity?: Severity[];
  correlation_id?: string;
  cursor?: string | null;
}

export interface AuditPage {
  rows: AuditEventSummary[];
  next_cursor: string | null;
}

export type OpType =
  | "tenant_data_export"
  | "byok_cmk_rotation"
  | "tenant_account_deletion"
  | "data_residency_change";

export type OpStatus = "awaiting_approval" | "approved" | "executed" | "rejected";

export interface DualApproval {
  approver: string;
  approved_at: string;
  reason: string;
}

export interface AdminOp {
  op_id: string;
  op_type: OpType;
  requestor: string;
  requested_at: string;
  status: OpStatus;
  payload: Record<string, unknown>;
  impact_summary: string;
  tenant_scope: string[];
  approvals: DualApproval[];
  rejection?: { rejector: string; rejected_at: string; reason: string };
}

export interface Tenant {
  tenant_id: string;
  name: string;
  plan: "free" | "starter" | "team" | "enterprise";
  region: "us-east" | "us-west" | "eu-west" | "ap-south";
  byok_status: "none" | "active" | "rotation_pending";
  created_at: string;
}

export type ClerkOrgRole = "corelink-admin" | "corelink-viewer" | "corelink-member" | null;

export interface ExportResponse {
  signed_url: string;
  expires_at: string;
  format: "csv" | "json";
}
