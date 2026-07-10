import type { BlobDigest } from "./digest.js";

export type { BlobDigest };

/** A CoreLink tenant identifier (the isolation key for every request). */
export type TenantId = string;

/** Retry policy for transient failures (429 / 503). */
export interface RetryConfig {
  /** Total attempts including the first. Default `3`. `1` disables retry. */
  maxAttempts: number;
  /** Base backoff in ms for the first retry. Default `200`. */
  baseDelayMs: number;
  /** Cap on any single backoff, in ms. Default `10_000`. */
  maxDelayMs: number;
}

/** Constructor configuration for {@link CoreLinkClient}. */
export interface ClientConfig {
  /**
   * Personal Access Token. Falls back to the `CORELINK_PAT` environment
   * variable when omitted. The client refuses to construct without one.
   */
  pat?: string;
  /** Tenant scope. Required — it is the sole isolation key server-side. */
  tenantId: TenantId;
  /**
   * API base URL. Defaults to `https://corelink-api.humangr.com`. Override for
   * staging / local dev (trailing slashes are trimmed).
   */
  baseUrl?: string;
  /**
   * Verify the BLAKE3 digest of downloaded bytes after `get()`. Default
   * `true`. Disabling logs a warning; the bytes are then unverified.
   */
  clientVerify?: boolean;
  /** Per-request timeout in ms. Default `30_000`. */
  timeoutMs?: number;
  /** Retry policy for 429/503. Partial overrides merge over the defaults. */
  retry?: Partial<RetryConfig>;
  /**
   * `fetch` implementation to use. Defaults to the global `fetch` (Node 18+,
   * browsers, Cloudflare Workers). Inject one for tests or a custom runtime.
   */
  fetch?: typeof fetch;
}

/** Options for {@link CoreLinkClient.put}. */
export interface PutOptions {
  /**
   * If provided, the SDK asserts the locally computed BLAKE3 digest equals
   * this value before uploading and throws {@link DigestMismatchError} on
   * disagreement — a cheap guard against a mislabeled buffer.
   */
  expectedDigest?: BlobDigest;
}

/** Options for {@link CoreLinkClient.get}. */
export interface GetOptions {
  /** Override the client-verify default for this one call. */
  verify?: boolean;
}

/** Result of {@link CoreLinkClient.stat}. */
export interface StatResult {
  /** The digest that was queried. */
  digest: BlobDigest;
  /** Whether a blob is stored at this digest for the tenant. */
  exists: boolean;
  /** Exact byte length of the stored blob, or `0` when absent. */
  sizeBytes: number;
}
