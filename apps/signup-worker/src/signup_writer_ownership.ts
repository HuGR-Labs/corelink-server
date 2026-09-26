/** Signup-family request binding and atomic ownership registration. */

import {
  ownershipInsertStatement,
  verifyStagingOwnershipEnvelope,
} from "./staging_load_test_ownership.js";
import type { StagingOwnershipContext } from "./staging_load_test_ownership.js";
import {
  runAtomicD1Batch,
} from "./lib/d1.js";
import type {
  D1Database as SignupD1Database,
  D1PreparedStatement as SignupPreparedStatement,
} from "./lib/d1.js";

const OWNERSHIP_ENVELOPE_HEADER = "x-corelink-staging-ownership";
const OWNERSHIP_REQUEST_ID_HEADER = "x-corelink-staging-request-id";
const INVALID_OWNERSHIP = "staging ownership envelope is invalid";
const SIGNUP_ARTIFACT_HANDLE_DOMAIN = "corelink/signup-artifact-handle/v1\0";

function bytesToHex(value: ArrayBuffer): string {
  return Array.from(new Uint8Array(value), (byte) => byte.toString(16).padStart(2, "0")).join("");
}

/**
 * Derive a stable, redacted ledger key from the domain row's identity.
 *
 * A request ID authenticates one request but is deliberately fresh on a
 * retry. Using it as the ownership handle would let two valid requests claim
 * two ledger rows after an `INSERT OR IGNORE` domain no-op. The kind keeps
 * independent signup artifacts in separate namespaces; the digest avoids
 * recording Clerk, tenant, or GitHub identifiers in the ownership ledger.
 */
export async function signupArtifactHandle(kind: string, identity: string): Promise<string> {
  const encoder = new TextEncoder();
  const parts = [kind, identity].map((part) => encoder.encode(part));
  const domain = encoder.encode(SIGNUP_ARTIFACT_HANDLE_DOMAIN);
  const size = domain.byteLength + parts.reduce((total, part) => total + 8 + part.byteLength, 0);
  const input = new Uint8Array(size);
  let offset = 0;
  input.set(domain, offset);
  offset += domain.byteLength;
  for (const part of parts) {
    new DataView(input.buffer).setBigUint64(offset, BigInt(part.byteLength), false);
    offset += 8;
    input.set(part, offset);
    offset += part.byteLength;
  }
  return `signup:v1:${bytesToHex(await crypto.subtle.digest("SHA-256", input))}`;
}

/**
 * `ownershipInsertStatement` deliberately tolerates an exact replay with
 * `ON CONFLICT DO NOTHING`. This follow-up statement turns that no-op into a
 * constraint failure inside the same D1 transaction, so earlier domain
 * statements in the batch roll back on a concurrent replay.
 */
function requireFreshOwnershipInsert(
  db: D1Database,
  opaqueHandle: string,
): D1PreparedStatement {
  return db
    .prepare(
      "INSERT INTO staging_load_test_resources " +
        "(run_id, scenario, resource_class, receipt_ref, opaque_handle, disposition, state, registered_at_ms) " +
        "SELECT run_id, scenario, resource_class, receipt_ref, opaque_handle, disposition, state, registered_at_ms " +
        "FROM staging_load_test_resources " +
        "WHERE resource_class = 'signup_artifact' AND opaque_handle = ?1 " +
        "AND changes() = 0 LIMIT 1",
    )
    .bind(opaqueHandle);
}

/** Verify the optional worker envelope before any signup-owned write. */
export async function signupOwnershipContext(
  request: Request,
  environment: string | undefined,
  signingKey: string | undefined,
  nowMs = Date.now(),
): Promise<StagingOwnershipContext | null> {
  const envelope = request.headers.get(OWNERSHIP_ENVELOPE_HEADER);
  const requestId = request.headers.get(OWNERSHIP_REQUEST_ID_HEADER);
  if (envelope === null && requestId === null) return null;
  if (
    envelope === null ||
    requestId === null ||
    environment !== "staging" ||
    !signingKey
  ) {
    throw new Error(INVALID_OWNERSHIP);
  }
  return verifyStagingOwnershipEnvelope(envelope, "signup", requestId, nowMs, signingKey);
}

/** Apply signup D1 writes and their one resource registration as one batch. */
export async function writeSignupArtifactBatch(
  db: D1Database,
  context: StagingOwnershipContext | null,
  opaqueHandle: string,
  statements: SignupPreparedStatement[],
  registeredAtMs: number,
): Promise<void> {
  if (context === null) {
    for (const statement of statements) await statement.run();
    return;
  }

  const prior = await db
    .prepare(
      "SELECT 1 AS present FROM staging_load_test_resources " +
        "WHERE resource_class = 'signup_artifact' AND opaque_handle = ?1 LIMIT 1",
    )
    .bind(opaqueHandle)
    .first<{ present: number }>();
  if (prior !== null) throw new Error("staging signup artifact is already registered");

  const ownership = await ownershipInsertStatement(
    db,
    context,
    "signup_artifact",
    "disposable",
    opaqueHandle,
    registeredAtMs,
  );
  await runAtomicD1Batch(
    db as unknown as SignupD1Database,
    [
      ...statements,
      ownership as unknown as SignupPreparedStatement,
      requireFreshOwnershipInsert(db, opaqueHandle) as unknown as SignupPreparedStatement,
    ],
  );
}
