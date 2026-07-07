// WI-S16-005 — audit filter bar. Filters: tenant_id (search), event_type
// (multi-select), date range, severity, correlation_id.
//
// Linear kit: Field + Input for text/date, `lin-check` rows for the multi-selects.

"use client";

import React from "react";
import type { AuditFilter, EventType, Severity } from "@/lib/types";
import { Field, Input } from "@/components/ui/linear";

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

const SEVERITY_LABEL: Record<Severity, string> = {
  info: "Info",
  warn: "Warning",
  critical: "Critical",
};

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
      <Field label="Tenant ID" htmlFor="filter-tenant">
        <Input
          id="filter-tenant"
          type="search"
          data-testid="filter-tenant"
          value={filter.tenant_id ?? ""}
          onChange={(e) => onChange({ ...filter, tenant_id: e.target.value, cursor: null })}
        />
      </Field>

      <fieldset className="lin-mt">
        <legend className="lin-label">Event type</legend>
        <div className="lin-checklist">
          {EVENT_TYPES.map((et) => (
            <label key={et} className="lin-check">
              <input
                type="checkbox"
                data-testid={`filter-event-${et}`}
                checked={(filter.event_types ?? []).includes(et)}
                onChange={() => toggleEventType(et)}
              />
              <span className="lin-check__label">{et}</span>
            </label>
          ))}
        </div>
      </fieldset>

      <div className="lin-mt">
        <Field label="Since" htmlFor="filter-since">
          <Input
            id="filter-since"
            type="date"
            data-testid="filter-since"
            value={filter.since ?? ""}
            onChange={(e) => onChange({ ...filter, since: e.target.value, cursor: null })}
          />
        </Field>
      </div>
      <div className="lin-mt">
        <Field label="Until" htmlFor="filter-until">
          <Input
            id="filter-until"
            type="date"
            data-testid="filter-until"
            value={filter.until ?? ""}
            onChange={(e) => onChange({ ...filter, until: e.target.value, cursor: null })}
          />
        </Field>
      </div>

      <fieldset className="lin-mt">
        <legend className="lin-label">Severity</legend>
        <div className="lin-checklist">
          {SEVERITIES.map((s) => (
            <label key={s} className="lin-check">
              <input
                type="checkbox"
                data-testid={`filter-sev-${s}`}
                checked={(filter.severity ?? []).includes(s)}
                onChange={() => toggleSeverity(s)}
              />
              <span className="lin-check__label">{SEVERITY_LABEL[s]}</span>
            </label>
          ))}
        </div>
      </fieldset>

      <div className="lin-mt">
        <Field label="Correlation ID" htmlFor="filter-correlation">
          <Input
            id="filter-correlation"
            type="search"
            data-testid="filter-correlation"
            value={filter.correlation_id ?? ""}
            onChange={(e) =>
              onChange({ ...filter, correlation_id: e.target.value, cursor: null })
            }
          />
        </Field>
      </div>
    </form>
  );
}

export default AuditFilterBar;
