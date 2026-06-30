/**
 * Sentry event PII/secret scrubber — shared posture across every CoreLink
 * Sentry init.
 *
 * WHY: `sendDefaultPii: false` + the old header-key filter only ever scrubbed
 * request *header keys*. Secrets and PII routinely land in the *bodies* Sentry
 * captures by default — `event.message`, `exception.values[].value`,
 * breadcrumb messages, and `extra`/`contexts` *values* — and those shipped to
 * the error backend unscrubbed (enterprise-DD MED). This module closes that:
 * it is registered in `beforeSend` (and `beforeSendTransaction`) so NO PII /
 * secret reaches Sentry.
 *
 * Two layers:
 *   1. Default-DENY by key — any object key matching a known-sensitive name
 *      (authorization, cookie, token, secret, password, email, the internal-
 *      auth key, …) has its value replaced wholesale with `[REDACTED]`. This
 *      covers opaque secrets we cannot pattern-match in free text.
 *   2. Substring redaction in free text — message bodies, exception values,
 *      breadcrumb messages and remaining string values are scanned for
 *      secret/PII shapes (CoreLink PATs, bearer/basic auth, Stripe sk_/pk_/rk_
 *      keys, `whsec_` webhook secrets, emails) and each match replaced with
 *      `[REDACTED]`.
 *
 * Conservative + allocation-light by design (it runs on the error path):
 * mutates the event in place, short-circuits on non-strings, and rebuilds only
 * the nested objects it actually scrubs. Pure + dependency-free so it unit-
 * tests under any runner and needs no shared build target.
 */

export const REDACTED = "[REDACTED]";

/**
 * Layer 1 — keys whose VALUE must never reach the error backend regardless of
 * content. Matched case-insensitively against the *exact* key name.
 */
const SENSITIVE_KEY =
  /^(authorization|proxy-authorization|cookie|set-cookie|x-api-key|x-corelink-internal-auth|svix-signature|stripe-signature|token|access[_-]?token|refresh[_-]?token|id[_-]?token|session[_-]?token|secret|client[_-]?secret|signing[_-]?key|private[_-]?key|api[_-]?key|password|passwd|pwd|email|e[_-]?mail)$/i;

/**
 * Layer 2 — secret/PII shapes redacted anywhere they appear in free text.
 * Each is global so every occurrence in a string is replaced.
 */
const PATTERNS: readonly RegExp[] = [
  /corelink_pat_[A-Za-z0-9._-]+/gi, // CoreLink PAT tokens
  /\b(?:bearer|basic)\s+[A-Za-z0-9._~+/=-]+/gi, // Authorization header values
  /\b(?:sk|pk|rk)_(?:live|test)_[A-Za-z0-9]{8,}/g, // Stripe secret/publishable/restricted keys
  /\bwhsec_[A-Za-z0-9]{8,}/g, // Stripe webhook signing secret
  /[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}/g, // email addresses
];

/** Redact every secret/PII pattern occurrence in a free-text string. */
export function scrubText(input: string): string {
  let out = input;
  for (const re of PATTERNS) {
    // `replace` resets the global regex `lastIndex` each call, so reuse is safe.
    out = out.replace(re, REDACTED);
  }
  return out;
}

function scrubValue(value: unknown, keyIsSensitive: boolean): unknown {
  if (keyIsSensitive) return REDACTED;
  if (typeof value === "string") return scrubText(value);
  if (Array.isArray(value)) return value.map((v) => scrubValue(v, false));
  if (value && typeof value === "object") {
    return scrubObject(value as Record<string, unknown>);
  }
  return value;
}

/**
 * Recursively scrub an object: default-DENY by sensitive key, otherwise
 * substring-redact string values and recurse into nested objects/arrays.
 */
export function scrubObject(obj: Record<string, unknown>): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(obj)) {
    out[k] = scrubValue(v, SENSITIVE_KEY.test(k));
  }
  return out;
}

/** Minimal structural view of the Sentry event fields we touch. */
interface SentryEventShape {
  message?: unknown;
  logentry?: unknown;
  transaction?: unknown;
  exception?: { values?: Array<{ value?: unknown; type?: unknown } | null> } | null;
  breadcrumbs?: Array<{ message?: unknown; data?: unknown } | null> | null;
  request?: unknown;
  extra?: unknown;
  contexts?: unknown;
  tags?: unknown;
  user?: unknown;
}

/**
 * Scrub a Sentry event in place before it is sent. Generic over the concrete
 * SDK event type so every package can reuse it without importing Sentry types.
 */
export function scrubSentryEvent<T>(event: T): T {
  if (!event || typeof event !== "object") return event;
  const e = event as unknown as SentryEventShape;

  // event.message: string | { message?, formatted?, params? }
  if (typeof e.message === "string") {
    e.message = scrubText(e.message);
  } else if (e.message && typeof e.message === "object") {
    const m = e.message as Record<string, unknown>;
    if (typeof m["message"] === "string") m["message"] = scrubText(m["message"]);
    if (typeof m["formatted"] === "string") m["formatted"] = scrubText(m["formatted"]);
    if (Array.isArray(m["params"])) {
      m["params"] = (m["params"] as unknown[]).map((p) =>
        typeof p === "string" ? scrubText(p) : p,
      );
    }
  }

  // logentry — alternate structured-message channel.
  if (e.logentry && typeof e.logentry === "object") {
    const le = e.logentry as Record<string, unknown>;
    if (typeof le["message"] === "string") le["message"] = scrubText(le["message"]);
    if (typeof le["formatted"] === "string") le["formatted"] = scrubText(le["formatted"]);
  }

  // exception bodies — the primary leak vector.
  const exVals = e.exception?.values;
  if (Array.isArray(exVals)) {
    for (const v of exVals) {
      if (v && typeof v.value === "string") v.value = scrubText(v.value);
      if (v && typeof v.type === "string") v.type = scrubText(v.type);
    }
  }

  // breadcrumbs.
  if (Array.isArray(e.breadcrumbs)) {
    for (const b of e.breadcrumbs) {
      if (b && typeof b.message === "string") b.message = scrubText(b.message);
      if (b && b.data && typeof b.data === "object") {
        b.data = scrubObject(b.data as Record<string, unknown>);
      }
    }
  }

  // request — headers/cookies/data/query string.
  if (e.request && typeof e.request === "object") {
    const r = e.request as Record<string, unknown>;
    if (r["headers"] && typeof r["headers"] === "object") {
      r["headers"] = scrubObject(r["headers"] as Record<string, unknown>);
    }
    if (typeof r["cookies"] === "string") {
      r["cookies"] = REDACTED;
    } else if (r["cookies"] && typeof r["cookies"] === "object") {
      r["cookies"] = scrubObject(r["cookies"] as Record<string, unknown>);
    }
    if (r["data"] !== undefined) r["data"] = scrubValue(r["data"], false);
    if (typeof r["query_string"] === "string") r["query_string"] = scrubText(r["query_string"]);
  }

  // extra / contexts.
  if (e.extra && typeof e.extra === "object") {
    e.extra = scrubObject(e.extra as Record<string, unknown>);
  }
  if (e.contexts && typeof e.contexts === "object") {
    e.contexts = scrubObject(e.contexts as Record<string, unknown>);
  }

  // tags — scrub string values (a raw tenant_id hash passes through untouched).
  if (e.tags && typeof e.tags === "object") {
    const t = e.tags as Record<string, unknown>;
    for (const k of Object.keys(t)) {
      if (typeof t[k] === "string") t[k] = scrubText(t[k] as string);
    }
  }

  // user — drop direct identifiers (defence-in-depth alongside sendDefaultPii).
  if (e.user && typeof e.user === "object") {
    const u = e.user as Record<string, unknown>;
    if (typeof u["email"] === "string") u["email"] = REDACTED;
    if (typeof u["ip_address"] === "string") u["ip_address"] = REDACTED;
    if (typeof u["username"] === "string") u["username"] = scrubText(u["username"] as string);
  }

  // transaction name (also the beforeSendTransaction entry point).
  if (typeof e.transaction === "string") e.transaction = scrubText(e.transaction);

  return event;
}
