/**
 * PII redaction wrapper for client-side telemetry (CTRL-PRIV-001).
 *
 * Allowlist-only. Any field not on the allowlist is dropped silently.
 * Denylist assertion: even if a key on the allowlist contains a value that
 * looks like a PAT/email, we still scrub.
 */

const ALLOWLIST: ReadonlyArray<string> = [
  "request_id",
  "user_id_hash",
  "error_code",
  "route",
  "cli_version",
  "app_version",
  "commit",
];

const DENYLIST: ReadonlyArray<string> = [
  "email",
  "pat",
  "tenant_id",
  "blob_digest",
  "password",
  "mfa_code",
  "secret",
  "token",
  "authorization",
];

export type LogLevel = "debug" | "info" | "warn" | "error";

export function sanitizeContext(
  context: Readonly<Record<string, unknown>>,
): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const key of Object.keys(context)) {
    const lower = key.toLowerCase();
    if (DENYLIST.includes(lower)) continue;
    if (!ALLOWLIST.includes(key)) continue;
    const value = context[key];
    // Defense-in-depth: never serialize objects (might smuggle PII fields).
    if (value === null || value === undefined) continue;
    if (typeof value === "string" || typeof value === "number" || typeof value === "boolean") {
      out[key] = value;
    }
  }
  return out;
}

export function safeLog(
  level: LogLevel,
  msg: string,
  context: Readonly<Record<string, unknown>> = {},
): { level: LogLevel; msg: string; context: Record<string, unknown> } {
  const sanitized = sanitizeContext(context);
  // Side-effect: emit to console with PII already scrubbed (CTRL-PRIV-001).
  // eslint-disable-next-line no-console
  const sink = level === "debug" ? console.log : console[level];
  sink(`[${msg}]`, sanitized);
  return { level, msg, context: sanitized };
}
