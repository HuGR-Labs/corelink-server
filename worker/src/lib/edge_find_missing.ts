/**
 * Edge-native `findMissingBlobs` — pure derivation half (F0).
 *
 * WHY THIS EXISTS. Measured from inside the fabric, the container answers
 * `findMissingBlobs` at ~13 digests/second: n=100 costs 8.6-9.1 s, the curve is
 * linear, and sixteen SEPARATE single-digest requests (1.19 s wall) are no
 * faster than one sixteen-digest request (1.59-1.82 s) — so the ceiling is the
 * 0.25-vCPU instance and its public-S3 path to R2, not our fan-out. Reaching
 * the owner's 15 ms ceiling means probing R2 in-colo through the Worker's
 * native binding instead. See
 * `docs/design/2026-08-25-adr-edge-native-find-missing.md`.
 *
 * This module is DERIVATION ONLY: given a tenant and a digest, what R2 key does
 * the container use? Nothing here touches R2, D1, or auth. Serving is F2.
 *
 * The key format and the prefix derivation are pinned by
 * `worker/tests/vectors/tenant_prefix_vectors.json`, which the Rust source of
 * truth asserts too (`crates/tenant-path/tests/edge_parity_vectors.rs`). A
 * one-sided test would let the edge derive keys the container never wrote —
 * every blob would read as missing, which a cache client acts on by uploading
 * everything.
 */

/** `TENANT_PREFIX_LEN` (`crates/tenant-path/src/prefix.rs`). */
export const TENANT_PREFIX_LEN = 16;

/** Digest algorithms the CAS keyspace distinguishes (`DigestAlgo`). */
export type CasDigestAlgo = "blake3" | "sha256";

/**
 * Tenant ids that must never reach key derivation. These are sentinels the
 * Worker uses for "no tenant resolved yet"; deriving a prefix for one would
 * create a keyspace SHARED by every unresolved caller. Mirrors the container's
 * `TENANT_SENTINELS` (`routes/cas.rs`, `routes/ac.rs`, …).
 */
const TENANT_SENTINELS = new Set(["_anonymous", "_unknown", "_system", "_pending", "_public", ""]);

const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;

/** REAPI v2 digests are hex sha256. */
const SHA256_HEX_RE = /^[0-9a-f]{64}$/;

function hexToBytes(hex: string): Uint8Array {
  const out = new Uint8Array(hex.length / 2);
  for (let i = 0; i < out.length; i++) {
    out[i] = Number.parseInt(hex.slice(i * 2, i * 2 + 2), 16);
  }
  return out;
}

function base64UrlNoPad(bytes: Uint8Array): string {
  let bin = "";
  for (const b of bytes) bin += String.fromCharCode(b);
  return btoa(bin).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

/**
 * Canonical UUID → the 16 raw bytes, big-endian — `Uuid::as_bytes()`, which is
 * what `derive_prefix` HMACs. Returns null for anything that is not a canonical
 * lowercase UUID, because the container's `tenant_prefix` FAILS CLOSED on a
 * non-UUID tenant rather than inventing a prefix, and the edge must refuse
 * exactly where the container refuses.
 */
export function uuidToBytes(tenantId: string): Uint8Array | null {
  if (!UUID_RE.test(tenantId)) return null;
  return hexToBytes(tenantId.replace(/-/g, ""));
}

/**
 * Port of `derive_prefix(tdk, tenant_uuid)` (`crates/tenant-path/src/prefix.rs`):
 * HMAC-SHA256(key = 32-byte TDK, msg = 16-byte UUID) → base64url-no-pad → first
 * 16 chars. Generalises `edge_public_read.ts::derivePublicPrefix`, which is the
 * same primitive fixed to the `_public` namespace UUID.
 *
 * Returns null — never a guess — when the tenant is a sentinel or not a
 * canonical UUID. A caller that gets null must fall back to the container.
 */
export async function deriveTenantPrefix(
  tdkHex: string,
  tenantId: string,
): Promise<string | null> {
  if (TENANT_SENTINELS.has(tenantId)) return null;
  const uuidBytes = uuidToBytes(tenantId);
  if (!uuidBytes) return null;

  const tdk = hexToBytes(tdkHex);
  if (tdk.length !== 32) throw new Error("R2_TDK_HEX must be 32 bytes (64 hex chars)");

  const key = await crypto.subtle.importKey(
    "raw",
    tdk,
    { name: "HMAC", hash: "SHA-256" },
    false,
    ["sign"],
  );
  const mac = new Uint8Array(await crypto.subtle.sign("HMAC", key, uuidBytes));
  return base64UrlNoPad(mac).slice(0, TENANT_PREFIX_LEN);
}

/**
 * Port of `R2S3Client::blob_key`. REAPI digests are sha256 and live under a
 * SEPARATE `bazel/sha256/` keyspace from native BLAKE3 blobs — probing the
 * native form for a REAPI digest would answer "missing" for blobs that exist.
 */
export function casBlobKey(
  region: string,
  tenantPrefix: string,
  digest: string,
  algo: CasDigestAlgo,
): string {
  return algo === "sha256"
    ? `${region}/${tenantPrefix}/bazel/sha256/${digest}`
    : `${region}/${tenantPrefix}/${digest}`;
}

/**
 * The R2 key a `findMissingBlobs` digest is probed at, or null when the edge
 * must not answer: sentinel/non-UUID tenant, or a digest that is not canonical
 * lowercase hex sha256. Malformed digests are the container's to reject, with
 * its own error taxonomy — the edge declining is not the edge accepting.
 */
export async function findMissingProbeKey(
  tdkHex: string,
  region: string,
  tenantId: string,
  digestHex: string,
): Promise<string | null> {
  if (!SHA256_HEX_RE.test(digestHex)) return null;
  const prefix = await deriveTenantPrefix(tdkHex, tenantId);
  if (!prefix) return null;
  return casBlobKey(region, prefix, digestHex, "sha256");
}

// ─────────────────────────────────────────────────────────────────────────────
// F1 SHADOW — compute the edge answer alongside the container's, compare, and
// measure. Serves nothing. Every uncertainty resolves to a SKIP verdict, never
// to a claim of parity: a shadow that quietly counts "we could not check" as
// "match" is worse than no shadow, because it manufactures the confidence the
// F2 flip is supposed to earn.
// ─────────────────────────────────────────────────────────────────────────────

/** Bindings the shadow needs. Optional ones mirror how `index.ts` reads them. */
export interface EdgeFindMissingEnv {
  CONFIG_DB: D1Database;
  CAS_BUCKET: R2Bucket;
  R2_TDK_HEX?: string;
  R2_CAS_REGION?: string;
}

/**
 * The most digests the edge path will probe in one request. Each `head()` is a
 * subrequest, and a Worker's per-request subrequest budget is finite, while
 * `FIND_MISSING_BLOB_CAP` (the container's limit) is 4096 — far above it. Over
 * this, the edge declines and the container answers, which is the behaviour
 * today anyway.
 */
export const EDGE_FIND_MISSING_MAX_DIGESTS = 256;

/** Verdict strings are logged verbatim; keep them greppable and unambiguous. */
export type ShadowVerdict = string;

/** Digests out of a `findMissingBlobs` request body, or null if it is not one. */
export function parseRequestDigests(bodyText: string): string[] | null {
  let doc: unknown;
  try {
    doc = JSON.parse(bodyText);
  } catch {
    return null;
  }
  const list = (doc as { blobDigests?: unknown })?.blobDigests;
  if (!Array.isArray(list)) return null;
  const out: string[] = [];
  for (const item of list) {
    const hash = (item as { hash?: unknown })?.hash;
    if (typeof hash !== "string") return null;
    out.push(hash);
  }
  return out;
}

/** The `missingBlobDigests` hashes out of a container response body. */
export function parseMissingDigests(bodyText: string): string[] | null {
  let doc: unknown;
  try {
    doc = JSON.parse(bodyText);
  } catch {
    return null;
  }
  const list = (doc as { missingBlobDigests?: unknown })?.missingBlobDigests;
  if (!Array.isArray(list)) return null;
  const out: string[] = [];
  for (const item of list) {
    const hash = (item as { hash?: unknown })?.hash;
    if (typeof hash !== "string") return null;
    out.push(hash);
  }
  return out;
}

/**
 * True when this tenant has ANY row in `tenant_byok_secret`.
 *
 * The edge must not answer for a BYOK tenant: the container probes a PHYSICAL
 * key that is an HMAC of the logical digest, resolved through the TCS, which
 * the edge does not have. Probing the plaintext key would report every blob
 * missing. The check is deliberately coarse — a row at all, wrapped or not —
 * and any query error also disqualifies the tenant, because "we do not know"
 * and "not BYOK" must not collapse into the same branch.
 */
export async function tenantMayUseEdgePath(
  env: EdgeFindMissingEnv,
  tenantId: string,
): Promise<boolean> {
  try {
    const row = await env.CONFIG_DB.prepare(
      "SELECT 1 AS present FROM tenant_byok_secret WHERE tenant_id = ?1 LIMIT 1",
    )
      .bind(tenantId)
      .first<{ present: number }>();
    return row === null;
  } catch {
    return false;
  }
}

/**
 * Probe every digest through the native R2 binding, concurrently, and return
 * the ones that are ABSENT — the same set `findMissingBlobs` returns.
 */
export async function probeMissingAtEdge(
  env: EdgeFindMissingEnv,
  tenantId: string,
  digests: string[],
): Promise<string[] | null> {
  const tdk = env.R2_TDK_HEX;
  const region = env.R2_CAS_REGION;
  if (!tdk || !region) return null;

  const prefix = await deriveTenantPrefix(tdk, tenantId);
  if (!prefix) return null;

  const keys: string[] = [];
  for (const d of digests) {
    if (!SHA256_HEX_RE.test(d)) return null;
    keys.push(casBlobKey(region, prefix, d, "sha256"));
  }

  // One `head()` per digest, all in flight at once. This is the whole point of
  // the move: in-colo, no TLS handshake, no SigV4 — the container's ~85 ms per
  // digest is not a property of the work, it is a property of where the work runs.
  const heads = await Promise.all(keys.map((k) => env.CAS_BUCKET.head(k)));
  const missing: string[] = [];
  for (let i = 0; i < heads.length; i++) {
    // `heads[i]` is null exactly when the object is absent. Index over the
    // ORIGINAL digest array: `Promise.all` preserves input order, and the
    // response must not depend on which R2 call finished first.
    const digest = digests[i];
    if (heads[i] === null && digest !== undefined) missing.push(digest);
  }
  return missing;
}

/**
 * Compare the edge answer against the container's on ONE request. Returns a
 * verdict string for the log. Never throws to the caller's path: the caller
 * runs it inside `ctx.waitUntil`, and a shadow must not be able to affect the
 * response it is shadowing.
 */
export async function shadowCompareEdgeFindMissing(
  env: EdgeFindMissingEnv,
  tenantId: string,
  requestBodyText: string,
  containerResponse: Response,
  now: () => number = () => Date.now(),
): Promise<ShadowVerdict> {
  const requested = parseRequestDigests(requestBodyText);
  if (requested === null) return "skip:unparseable-request";
  if (requested.length === 0) return "skip:empty";
  if (requested.length > EDGE_FIND_MISSING_MAX_DIGESTS) {
    return `skip:over-cap n=${requested.length}`;
  }

  if (!(await tenantMayUseEdgePath(env, tenantId))) return "skip:byok-or-unknown";

  const containerText = await containerResponse.text();
  const containerMissing = parseMissingDigests(containerText);
  if (containerMissing === null) return "skip:unparseable-container-response";

  const started = now();
  let edgeMissing: string[] | null;
  try {
    edgeMissing = await probeMissingAtEdge(env, tenantId, requested);
  } catch (e) {
    return `skip:edge-error ${String(e).slice(0, 60)}`;
  }
  const edgeMs = now() - started;
  if (edgeMissing === null) return "skip:underivable";

  const a = new Set(edgeMissing);
  const b = new Set(containerMissing);
  const onlyEdge = [...a].filter((h) => !b.has(h)).length;
  const onlyContainer = [...b].filter((h) => !a.has(h)).length;
  const shape = `n=${requested.length} edge_ms=${edgeMs}`;
  if (onlyEdge === 0 && onlyContainer === 0) return `match ${shape}`;
  // Counts only — digests are tenant data and this line goes to logs
  // (INV-NO-BODY-IN-LOGS).
  return `DIVERGENT ${shape} only_edge=${onlyEdge} only_container=${onlyContainer}`;
}

// ─────────────────────────────────────────────────────────────────────────────
// F2 SERVE — answer at the edge, but only once the audit rows are DURABLE.
//
// F1 proved the edge computes the same SET (43/43 parity, 8.65 s -> 2.10 s at
// n=100). That is not permission to serve. `exists_batch`
// (`crates/corelink-container/src/storage/r2_s3.rs`) writes one `ReadAttempted`
// row per digest and DISCARDS probe results it is already holding when that
// write fails — `findMissingBlobs` is a REAPI read surface and those rows are
// the evidence the S-09 chain drains. An edge that answered without them would
// keep answering while the audit sink was down.
//
// So the edge probes R2 in-colo and then AWAITS one call to the container's
// `POST /_internal/audit/cas-attempted`, which emits the rows through the same
// `D1AuditOutboxSink`. The container stays the single author of the row shape.
// See `docs/design/2026-08-26-adr-edge-find-missing-audit-seam.md`.
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Emits the `ReadAttempted` rows for a probe the edge performed. Injected so the
 * decision logic is testable without a DO stub.
 *
 * MUST resolve `true` ONLY on a 204 from the container. Every other outcome —
 * any other status, a timeout, a transport error — is `false`, and `false` means
 * the caller falls through to the container. "We could not audit" must never
 * become "we answered".
 */
export type EmitAttemptedAudit = (batch: {
  tenant: string;
  principal: string;
  caller_tenant: string;
  at_unix_ms: number;
  digests: string[];
}) => Promise<boolean>;

/** What the caller must do next. `null` ⇒ fall through to the container. */
export type EdgeServeOutcome = { missing: string[]; edgeMs: number } | null;

/**
 * Decide whether the edge may answer this `findMissingBlobs`, and produce the
 * answer if so. Returns `null` for every reason to defer — malformed body, over
 * cap, BYOK/unknown tenant, underivable key, probe error, and above all an audit
 * that did not commit.
 *
 * Deferring is always SAFE: the container answers exactly as it does today,
 * writing its own audit rows and failing closed in its own taxonomy. That is why
 * every branch here resolves to `null` rather than to a served response.
 */
export async function serveEdgeFindMissing(
  env: EdgeFindMissingEnv,
  tenantId: string,
  principal: string,
  requestBodyText: string,
  emitAudit: EmitAttemptedAudit,
  now: () => number = () => Date.now(),
): Promise<EdgeServeOutcome> {
  const requested = parseRequestDigests(requestBodyText);
  if (requested === null) return null;
  // An empty batch is answerable without probing OR auditing — the container
  // writes no row and issues no request for it either (`exists_batch` returns
  // early on an empty slice). Answering it here is not an unaudited answer,
  // because there is nothing to audit.
  if (requested.length === 0) return { missing: [], edgeMs: 0 };
  if (requested.length > EDGE_FIND_MISSING_MAX_DIGESTS) return null;
  if (!(await tenantMayUseEdgePath(env, tenantId))) return null;

  const started = now();
  let missing: string[] | null;
  try {
    missing = await probeMissingAtEdge(env, tenantId, requested);
  } catch {
    return null;
  }
  if (missing === null) return null;

  // AUDIT GATES THE RESPONSE. The probe result is already in hand and is thrown
  // away if the rows did not commit — deliberately mirroring the container,
  // which does exactly this with its own results rather than serve unaudited.
  const at = now();
  let audited = false;
  try {
    audited = await emitAudit({
      tenant: tenantId,
      principal,
      caller_tenant: tenantId,
      at_unix_ms: at,
      digests: requested,
    });
  } catch {
    return null;
  }
  if (!audited) return null;

  return { missing, edgeMs: now() - started };
}

/** The REAPI `findMissingBlobs` response body for a set of missing digests. */
export function findMissingResponseBody(missing: string[]): string {
  // `sizeBytes` is required by the proto and unknown to a HEAD probe; the
  // container answers the same shape, echoing only the hash.
  return JSON.stringify({ missingBlobDigests: missing.map((hash) => ({ hash })) });
}
