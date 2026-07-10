/**
 * `@corelink/client` — the official CoreLink JavaScript/TypeScript SDK.
 *
 * A thin, dependency-light client over the wired CoreLink cache surface:
 * content-addressable storage (CAS) and the Action Cache (AC), authenticated
 * with a PAT bearer token. BLAKE3 digests are computed and verified in pure JS
 * via `@noble/hashes`.
 *
 * @packageDocumentation
 */

export { CoreLinkClient, ActionCacheApi } from "./client.js";
export { blake3Hex, isCanonicalDigest, DIGEST_RE } from "./digest.js";
export type { BlobDigest } from "./digest.js";
export type {
  ClientConfig,
  GetOptions,
  PutOptions,
  RetryConfig,
  StatResult,
  TenantId,
} from "./types.js";
export {
  CoreLinkError,
  AuthError,
  QuotaError,
  ForbiddenError,
  NotFoundError,
  ActionCacheMiss,
  ConflictError,
  GoneError,
  DigestMismatchError,
  RateLimitError,
  ServerError,
  ConnectError,
} from "./errors.js";
