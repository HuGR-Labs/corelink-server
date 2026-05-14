// WI-S16-005 — audit filter bar. Filters: tenant_id (search), event_type
// (multi-select), date range, severity, correlation_id.

"use client";

import React from "react";
import type { AuditFilter, EventType, Severity } from "@/lib/types";

const EVENT_TYPES: EventType[] = [
  "auth.login",
  "auth.mfa_step_up",
  "tenant.created",
  "tenant.deleted",
  "byok.cmk_rotated",
  "dsr.request",
  "admin.op_submitted",
  "admin.op_approved",
  "admin.op_rejected",
];
const SEVERITIES: Severity[] = ["info", "warn", "critical"];

export interface AuditFilterBarProps {
  filter: AuditFilter;
  onChange: (next: AuditFilter) => void;
}

export function AuditFilterBar({ filter, onChange }: AuditFilterBarProps): React.ReactElement {
  const toggleEventType = (et: string) => {
    const current = new Set(filter.event_types ?? []);
    if (current.has(et)) current.delete(et);
    else current.add(et);
    onChange({ ...filter, event_types: Array.from(current), cursor: null });
  };

  const toggleSeverity = (s: Severity) => {
    const current = new Set(filter.severity ?? []);
    if (current.has(s)) current.delete(s);
    else current.add(s);
    onChange({ ...filter, severity: Array.from(current), cursor: null });
  };

  return (
    <form
      data-testid="audit-filter-bar"
      onSubmit={(e) => e.preventDefault()}
      aria-label="Audit log filters"
    >
      <label>
        Tenant ID
        <input
          type="search"
          data-testid="filter-tenant"
          value={filter.tenant_id ?? ""}
          onChange={(e) => onChange({ ...filter, tenant_id: e.target.value, cursor: null })}
        />
      </label>

      <fieldset>
        <legend>Event type</legend>
        {EVENT_TYPES.map((et) => (
          <label key={et}>
            <input
              type="checkbox"
              data-testid={`filter-event-${et}`}
              checked={(filter.event_types ?? []).includes(et)}
              onChange={() => toggleEventType(et)}
            />
            {et}
          </label>
        ))}
      </fieldset>

      <label>
        Since
        <input
          type="date"
          data-testid="filter-since"
          value={filter.since ?? ""}
          onChange={(e) => onChange({ ...filter, since: e.target.value, cursor: null })}
        />
      </label>
      <label>
        Until
        <input
          type="date"
          data-testid="filter-until"
          value={filter.until ?? ""}
          onChange={(e) => onChange({ ...filter, until: e.target.value, cursor: null })}
        />
      </label>

      <fieldset>
        <legend>Severity</legend>
        {SEVERITIES.map((s) => (
          <label key={s}>
            <input
              type="checkbox"
              data-testid={`filter-sev-${s}`}
              checked={(filter.severity ?? []).includes(s)}
              onChange={() => toggleSeverity(s)}
            />
            {s}
          </label>
        ))}
      </fieldset>

      <label>
        Correlation ID
        <input
          type="search"
          data-testid="filter-correlation"
          value={filter.correlation_id ?? ""}
          onChange={(e) =>
            onChange({ ...filter, correlation_id: e.target.value, cursor: null })
          }
        />
      </label>
    </form>
  );
}

export default AuditFilterBar;
