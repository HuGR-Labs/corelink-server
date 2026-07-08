// Customer-side audit viewer — tenant-scoped, no operator Merkle tab.
//
// W5 (customer-dashboard build wave). Kit-only: every surface is a
// `@/components/ui/linear` primitive. The [live] flat audit list is rendered as
// a kit table with HUMAN event labels (raw `event_type` codes are machine noise
// — the audit found raw codes were dumped at users), a severity StatusDot/Badge,
// and a legend HelpPopover mapping labels back to their raw codes. Filters wire
// the backend's CANONICAL query params `from` / `to` / `kind` (the pre-2026-07
// UI sent `since`/`event_types`, which the backend silently DROPPED) plus a
// client-side severity filter. The API returns a bounded set, so pagination
// is client-side. States are proper: Skeleton while loading, InlineError with a
// retry on failure (the old page had NONE), and a teaching EmptyState for no
// results. The cryptographic chain verifier (Merkle/Ed25519) routes are
// mock-only in prod, so instead of a fake verifier we surface an honest Callout.

"use client";

import React from "react";
import { useAuth } from "@clerk/nextjs";
import { CustomerClient } from "@/lib/customer-client";
import type { CustomerAuditEvent } from "@/lib/customer-types";
import {
  Badge,
  Button,
  Callout,
  Card,
  EmptyState,
  Field,
  HelpPopover,
  InlineError,
  Input,
  Select,
  Skeleton,
} from "@/components/ui/linear";

type Severity = CustomerAuditEvent["severity"];

const PAGE_SIZE = 25;

/** Raw `event_type` codes the backend emits (customer_d1 writes `pat.created` +
 *  `team.invited` today; the rest are emitted/planned) mapped to a plain-language
 *  label. Unknown codes fall back to `humanizeCode` so a new backend event is
 *  never a bare machine string. The legend HelpPopover surfaces this mapping. */
const EVENT_LABELS: Record<string, string> = {
  "auth.login": "Signed in",
  "pat.created": "Token created",
  "pat.revoked": "Token revoked",
  "team.invited": "Teammate invited",
  "team.removed": "Teammate removed",
  "byok.cmk_rotated": "Encryption key rotated",
  "billing.portal_opened": "Billing portal opened",
  "billing.checkout_started": "Checkout started",
  "dsr.request": "Data request filed",
  "account.delete_requested": "Account deletion requested",
};

/** Turn an unmapped `namespace.action.code` into readable Title-case words so an
 *  unrecognized event still reads as English, never as a raw machine token. */
function humanizeCode(code: string): string {
  const words = code
    .replace(/[._-]+/g, " ")
    .trim()
    .split(/\s+/)
    .filter(Boolean);
  if (words.length === 0) return code;
  return words
    .map((w) => w.charAt(0).toUpperCase() + w.slice(1))
    .join(" ");
}

function labelFor(code: string): string {
  return EVENT_LABELS[code] ?? humanizeCode(code);
}

const SEVERITY_TONE: Record<Severity, "neutral" | "warn" | "danger"> = {
  info: "neutral",
  warn: "warn",
  critical: "danger",
};

const SEVERITY_LABEL: Record<Severity, string> = {
  info: "Info",
  warn: "Warning",
  critical: "Critical",
};

const SEVERITIES: Severity[] = ["info", "warn", "critical"];

function formatTs(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleString(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

/** Local date `YYYY-MM-DD` → an ISO instant at the START of that day, for the
 *  server-side `from` (lower-bound) param. */
function startOfDayIso(dateStr: string): string | undefined {
  if (!dateStr) return undefined;
  const d = new Date(`${dateStr}T00:00:00`);
  if (Number.isNaN(d.getTime())) return undefined;
  return d.toISOString();
}

/** Local date `YYYY-MM-DD` → an ISO instant at the END of that day, for the
 *  server-side `to` (upper-bound) param. */
function endOfDayIso(dateStr: string): string | undefined {
  if (!dateStr) return undefined;
  const d = new Date(`${dateStr}T23:59:59.999`);
  if (Number.isNaN(d.getTime())) return undefined;
  return d.toISOString();
}

/** Local date `YYYY-MM-DD` → epoch millis at the END of that day, for the
 *  client-side `to` upper bound (the backend accepts `to` but does not yet
 *  filter by it, so the client narrows the bounded set locally). */
function endOfDayMs(dateStr: string): number | undefined {
  if (!dateStr) return undefined;
  const d = new Date(`${dateStr}T23:59:59.999`);
  if (Number.isNaN(d.getTime())) return undefined;
  return d.getTime();
}

export function AuditClient(): React.ReactElement {
  const { getToken } = useAuth();
  const client = React.useMemo(() => new CustomerClient({ getToken }), [getToken]);

  const [rows, setRows] = React.useState<CustomerAuditEvent[]>([]);
  const [loading, setLoading] = React.useState(true);
  const [err, setErr] = React.useState<unknown | null>(null);

  // Filters. `from`/`to`/`eventType` are sent to the server as the canonical
  // `from` / `to` / `kind` params; `to` (server-ignored today) and `severity`
  // are also applied client-side over the bounded result set.
  const [from, setFrom] = React.useState("");
  const [to, setTo] = React.useState("");
  const [eventType, setEventType] = React.useState<string>("");
  const [severity, setSeverity] = React.useState<"" | Severity>("");
  const [page, setPage] = React.useState(0);

  const reload = React.useCallback(async () => {
    setLoading(true);
    setErr(null);
    try {
      const fromIso = startOfDayIso(from);
      const toIso = endOfDayIso(to);
      const res = await client.listAudit({
        ...(fromIso ? { from: fromIso } : {}),
        ...(toIso ? { to: toIso } : {}),
        ...(eventType ? { kind: [eventType] } : {}),
      });
      setRows(res.rows);
    } catch (e) {
      setErr(e);
    } finally {
      setLoading(false);
    }
  }, [client, from, to, eventType]);

  React.useEffect(() => {
    void reload();
  }, [reload]);

  // The event-type <Select> options come from the codes actually present in the
  // fetched set plus any known label keys, so the dropdown is never empty and
  // always offers what the tenant can see.
  const eventTypeOptions = React.useMemo(() => {
    const set = new Set<string>(rows.map((r) => r.event_type));
    for (const k of Object.keys(EVENT_LABELS)) set.add(k);
    return Array.from(set).sort((a, b) => labelFor(a).localeCompare(labelFor(b)));
  }, [rows]);

  // `to` (upper date bound) and severity are client-side over the bounded set.
  const filtered = React.useMemo(() => {
    const toMs = endOfDayMs(to);
    return rows.filter((r) => {
      if (severity && r.severity !== severity) return false;
      if (toMs != null) {
        const t = new Date(r.ts).getTime();
        if (!Number.isNaN(t) && t > toMs) return false;
      }
      return true;
    });
  }, [rows, to, severity]);

  // Reset to the first page whenever the effective result set changes.
  React.useEffect(() => {
    setPage(0);
  }, [to, severity, rows]);

  const pageCount = Math.max(1, Math.ceil(filtered.length / PAGE_SIZE));
  const clampedPage = Math.min(page, pageCount - 1);
  const pageRows = filtered.slice(clampedPage * PAGE_SIZE, clampedPage * PAGE_SIZE + PAGE_SIZE);

  const legend = (
    <HelpPopover label="What the event types mean">
      <div>
        Each row shows a plain-language label. The raw machine codes behind them:
        <ul>
          {Object.entries(EVENT_LABELS).map(([code, label]) => (
            <li key={code}>
              <code>{code}</code> — {label}
            </li>
          ))}
        </ul>
      </div>
    </HelpPopover>
  );

  return (
    <div data-testid="customer-audit-shell">
      <Callout tone="info">
        Cryptographic chain verification is coming — you&apos;ll be able to export the audit
        log and verify its Merkle/signature chain offline, proving no event was altered or
        removed.
      </Callout>

      <Card title="Filters" className="lin-mt">
        <div data-testid="customer-audit-filters">
          <Field label="From" htmlFor="customer-audit-since">
            <Input
              id="customer-audit-since"
              type="date"
              data-testid="customer-audit-since"
              value={from}
              max={to || undefined}
              onChange={(e) => setFrom(e.target.value)}
            />
          </Field>
          <Field label="To" htmlFor="customer-audit-to">
            <Input
              id="customer-audit-to"
              type="date"
              data-testid="customer-audit-to"
              value={to}
              min={from || undefined}
              onChange={(e) => setTo(e.target.value)}
            />
          </Field>
          <Field
            label="Event type"
            htmlFor="customer-audit-event-type"
            help="Filter to a single kind of event, e.g. token creation or sign-ins."
          >
            <Select
              id="customer-audit-event-type"
              data-testid="customer-audit-event-type"
              value={eventType}
              onChange={(e) => setEventType(e.target.value)}
            >
              <option value="">All events</option>
              {eventTypeOptions.map((code) => (
                <option key={code} value={code}>
                  {labelFor(code)}
                </option>
              ))}
            </Select>
          </Field>
          <Field
            label="Severity"
            htmlFor="customer-audit-severity"
            help="Info is routine, Warning needs a look, Critical is security-relevant."
          >
            <Select
              id="customer-audit-severity"
              data-testid="customer-audit-severity"
              value={severity}
              onChange={(e) => setSeverity(e.target.value as "" | Severity)}
            >
              <option value="">All severities</option>
              {SEVERITIES.map((s) => (
                <option key={s} value={s}>
                  {SEVERITY_LABEL[s]}
                </option>
              ))}
            </Select>
          </Field>
        </div>
      </Card>

      <Card title="Events" actions={legend} className="lin-mt">
        {loading ? (
          <div data-testid="customer-audit-loading" aria-busy="true" aria-label="Loading audit events">
            <Skeleton rows={5} />
          </div>
        ) : err != null ? (
          <div data-testid="customer-audit-error">
            <InlineError error={err} onRetry={() => void reload()} />
          </div>
        ) : filtered.length === 0 ? (
          <EmptyState
            title="No audit events yet"
            body="Token, team and config changes will appear here — create a token or invite a teammate to see your first event."
          />
        ) : (
          <>
            <table className="lin-table" data-testid="customer-audit-table">
              <thead>
                <tr>
                  <th>Time</th>
                  <th>Event</th>
                  <th>Severity</th>
                  <th>Actor</th>
                  <th>Summary</th>
                </tr>
              </thead>
              <tbody>
                {pageRows.map((r) => (
                  <tr key={r.event_id} data-testid={`customer-audit-row-${r.event_id}`}>
                    <td>{formatTs(r.ts)}</td>
                    <td>{labelFor(r.event_type)}</td>
                    <td>
                      <Badge tone={SEVERITY_TONE[r.severity]} dot>
                        {SEVERITY_LABEL[r.severity]}
                      </Badge>
                    </td>
                    <td>{r.actor}</td>
                    <td>{r.summary}</td>
                  </tr>
                ))}
              </tbody>
            </table>

            {pageCount > 1 ? (
              <div className="lin-mt" data-testid="customer-audit-pager">
                <Button
                  variant="ghost"
                  size="sm"
                  data-testid="customer-audit-prev"
                  disabled={clampedPage === 0}
                  onClick={() => setPage((p) => Math.max(0, p - 1))}
                >
                  Previous
                </Button>{" "}
                <span data-testid="customer-audit-page-label">
                  Page {clampedPage + 1} of {pageCount}
                </span>{" "}
                <Button
                  variant="ghost"
                  size="sm"
                  data-testid="customer-audit-next"
                  disabled={clampedPage >= pageCount - 1}
                  onClick={() => setPage((p) => Math.min(pageCount - 1, p + 1))}
                >
                  Next
                </Button>
              </div>
            ) : null}
          </>
        )}
      </Card>
    </div>
  );
}

export default AuditClient;
