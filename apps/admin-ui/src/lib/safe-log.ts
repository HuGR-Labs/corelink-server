// CTRL-PRIV-001: zero PII in client-side logs. This wrapper redacts emails,
// tenant_ids, PATs, and any field whose key matches a PII allow-deny list.

const PII_KEY_PATTERN = /(email|tenant_id|pat|token|secret|password|user_id)/i;
const EMAIL_PATTERN = /[a-z0-9._%+-]+@[a-z0-9.-]+\.[a-z]{2,}/gi;
const TENANT_PATTERN = /tenant_[a-z0-9-]+/gi;

export type SafeLogLevel = "debug" | "info" | "warn" | "error";

export function safeLog(
  level: SafeLogLevel,
  event: string,
  fields: Record<string, unknown> = {},
): void {
  const redacted = redactFields(fields);
  // eslint-disable-next-line no-console
  console[level === "debug" ? "log" : level](`[${event}]`, redacted);
}

function redactFields(input: Record<string, unknown>): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(input)) {
    if (PII_KEY_PATTERN.test(k)) {
      out[k] = "[REDACTED]";
      continue;
    }
    if (typeof v === "string") {
      out[k] = v.replace(EMAIL_PATTERN, "[REDACTED-EMAIL]").replace(TENANT_PATTERN, "[REDACTED-TENANT]");
    } else if (v && typeof v === "object" && !Array.isArray(v)) {
      out[k] = redactFields(v as Record<string, unknown>);
    } else {
      out[k] = v;
    }
  }
  return out;
}
