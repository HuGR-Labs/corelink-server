/**
 * Error hierarchy for `@corelink/client`.
 *
 * Every thrown error inherits from {@link CoreLinkError} so a caller can catch
 * one base type. Each subclass maps to a wired HTTP status of the CoreLink CAS
 * / AC surface (see `crates/corelink-container/src/routes/{cas,ac}.rs`):
 *
 * | status | class                     |
 * |--------|---------------------------|
 * | 401    | {@link AuthError}         |
 * | 402    | {@link QuotaError}        |
 * | 403    | {@link ForbiddenError}    |
 * | 404    | {@link NotFoundError} / {@link ActionCacheMiss} |
 * | 409    | {@link ConflictError}     |
 * | 410    | {@link GoneError}         |
 * | 422    | {@link DigestMismatchError} |
 * | 429    | {@link RateLimitError}    |
 * | 5xx    | {@link ServerError}       |
 * | (fetch throw) | {@link ConnectError} |
 */
export class CoreLinkError extends Error {
  /** HTTP status code, when the error originated from a server response. */
  readonly status?: number;
  /** Stable machine-readable code, when present (short server body text). */
  readonly code?: string;
  /** Underlying cause, when the error wraps a lower-level failure. */
  override readonly cause?: unknown;

  constructor(
    message: string,
    opts: { status?: number; code?: string; cause?: unknown } = {},
  ) {
    super(message);
    this.name = new.target.name;
    this.status = opts.status;
    this.code = opts.code;
    this.cause = opts.cause;
    Object.setPrototypeOf(this, new.target.prototype);
  }
}

/** 401 — the PAT is missing, malformed, revoked, or names another tenant. */
export class AuthError extends CoreLinkError {}

/** 402 — the tenant is over its storage cap or monthly spend ceiling. */
export class QuotaError extends CoreLinkError {}

/** 403 — cross-tenant attempt or the PAT lacks the required cache scope. */
export class ForbiddenError extends CoreLinkError {}

/** 404 — the digest is not present in this tenant's CAS. */
export class NotFoundError extends CoreLinkError {}

/**
 * 404 on an Action Cache lookup — no `ActionResult` stored for the action
 * digest. A subclass of {@link NotFoundError} so `catch (NotFoundError)` still
 * matches, while build tools can branch on the AC-specific miss.
 */
export class ActionCacheMiss extends NotFoundError {}

/** 409 — the write conflicts with an existing immutable entry. */
export class ConflictError extends CoreLinkError {}

/** 410 — the artifact was erased (GDPR/DSR) and must not be resurrected. */
export class GoneError extends CoreLinkError {}

/**
 * The bytes did not match the expected BLAKE3 digest. Raised for a server 422
 * (`content hash mismatch` on write) and for a failed client-side verify after
 * `get()` — in both cases the bytes are corrupt and must be discarded.
 */
export class DigestMismatchError extends CoreLinkError {}

/** 429 — per-PAT concurrency or rate limit exceeded. Retried by default. */
export class RateLimitError extends CoreLinkError {}

/** 5xx — the server (or its storage/audit backend) is unavailable. */
export class ServerError extends CoreLinkError {}

/** A transport-level failure: the request never produced an HTTP response. */
export class ConnectError extends CoreLinkError {}

/**
 * Map an HTTP status + short server body to the matching {@link CoreLinkError}
 * subclass. `context` (e.g. `"ac"`) lets a 404 become an {@link ActionCacheMiss}.
 */
export function errorForStatus(
  status: number,
  body: string,
  context?: "cas" | "ac",
): CoreLinkError {
  const msg = body.trim() || `HTTP ${status}`;
  const opts = { status, code: msg };
  switch (status) {
    case 401:
      return new AuthError(msg, opts);
    case 402:
      return new QuotaError(msg, opts);
    case 403:
      return new ForbiddenError(msg, opts);
    case 404:
      return context === "ac"
        ? new ActionCacheMiss(msg, opts)
        : new NotFoundError(msg, opts);
    case 409:
      return new ConflictError(msg, opts);
    case 410:
      return new GoneError(msg, opts);
    case 413:
      return new CoreLinkError(msg, opts);
    case 415:
      return new CoreLinkError(msg, opts);
    case 422:
      return new DigestMismatchError(msg, opts);
    case 429:
      return new RateLimitError(msg, opts);
    default:
      if (status >= 500) return new ServerError(msg, opts);
      return new CoreLinkError(msg, opts);
  }
}
