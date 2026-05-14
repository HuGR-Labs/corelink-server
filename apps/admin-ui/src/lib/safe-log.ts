/**
 * Safe logging wrapper (CTRL-PRIV-001 + CTRL-CRED-001).
 *
 * - Strips bearer-token shaped strings before forwarding to the underlying
 *   console / telemetry sink.
 * - Never serialises raw `Authorization` headers.
 * - Used by all DSR-surface helpers; production wires this to Sentry with
 *   PII scrubbing on top.
 */

const TOKEN_PATTERNS: RegExp[] = [
  /Bearer\s+[A-Za-z0-9._\-]+/g,
  /eyJ[A-Za-z0-9_\-]+\.[A-Za-z0-9_\-]+\.[A-Za-z0-9_\-]+/g, // JWT-ish
  /corelink_(?:prod|test)_[A-Za-z0-9_-]+/g,
  /sk_(?:live|test)_[A-Za-z0-9]+/g,
];

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

export function redactString(s: string): string {
  let out = s;
  for (const re of TOKEN_PATTERNS) {
    out = out.replace(re, "[REDACTED]");
  }
  return out;
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
