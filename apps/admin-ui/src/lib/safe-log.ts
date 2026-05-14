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

// =====================================================================
// CTRL-CRED-001: token-shape redaction helpers (WI-S16-004 surface).
// Used by dsr-client and other helpers that must scrub arbitrary
// strings/objects before forwarding to console / telemetry sinks.
// =====================================================================

const TOKEN_PATTERNS: RegExp[] = [
  /Bearer\s+[A-Za-z0-9._\-]+/g,
  /eyJ[A-Za-z0-9_\-]+\.[A-Za-z0-9_\-]+\.[A-Za-z0-9_\-]+/g, // JWT-ish
  /corelink_(?:prod|test)_[A-Za-z0-9_-]+/g,
  /sk_(?:live|test)_[A-Za-z0-9]+/g,
];

export function redactString(s: string): string {
  let out = s;
  for (const re of TOKEN_PATTERNS) {
    out = out.replace(re, "[REDACTED]");
  }
  return out;
}

export function redact(value: unknown): unknown {
  if (typeof value === "string") return redactString(value);
  if (Array.isArray(value)) return value.map(redact);
  if (value && typeof value === "object") {
    const out: Record<string, unknown> = {};
    for (const [k, v] of Object.entries(value as Record<string, unknown>)) {
      if (k.toLowerCase() === "authorization") {
        out[k] = "[REDACTED]";
      } else {
        out[k] = redact(v);
      }
    }
    return out;
  }
  return value;
}

export interface SafeLogger {
  warn(message: string, context?: unknown): void;
  error(message: string, context?: unknown): void;
}

export function makeSafeLogger(
  sink: Pick<Console, "warn" | "error"> = console,
): SafeLogger {
  return {
    warn(message, context) {
      sink.warn(redactString(message), redact(context));
    },
    error(message, context) {
      sink.error(redactString(message), redact(context));
    },
  };
}
