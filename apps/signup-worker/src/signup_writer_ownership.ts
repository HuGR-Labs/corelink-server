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
