/**
 * GitHub-installation → isolated-tenant PROVISIONING PRIMITIVE (cf-multitenant
 * PACKET, WP4). Internal-auth-gated write endpoint that populates the two
 * control-plane read models the runner-CI fabric consults:
 *   • `tenant_gh_installation_map` (migration 0084) — `installation_id → tenant_id`
 *   • `runner_repo_allowlist`      (migration 0085) — per-tenant `owner/repo` allowlist
 *
 * ## Trust model (DP3 — NOT lazy-provision)
 *
 * This endpoint is the identity-GATED provisioning primitive, exactly like the
 * container `/_internal/pat/mint` gate: it trusts (a) internal-auth AND (b) the
 * caller-supplied `tenant_id`. It NEVER auto-creates a tenant and NEVER derives a
 * tenant from the installation — the CALLER (the install-flow callback) is
 * responsible for having verified the authenticated identity → `tenant_id`
 * binding BEFORE invoking this. A missing/unmapped tenant is the caller's bug,
 * not something this primitive papers over by minting one.
 *
 * ## Fail-closed + idempotent
 *
 * Wrong method → 405. Malformed body → 400. Any D1 fault → 500 (never a partial
 * "success"). Both writes are `INSERT OR IGNORE`, so a Svix-style redelivery /
 * re-call is a harmless no-op.
 *
 * ## Internal-auth gate (mirrors `worker/src/lib/internal_auth.ts`)
 *
 * Resolved per-call via {@link resolveRunnerProvisionKey}. As of PHASE 2 this
 * consumer is DEDICATED-REQUIRED: the only credential accepted is
 * `CORELINK_RUNNER_PROVISION_AUTH_KEY`, at or above the length floor. The
 * shared `CORELINK_INTERNAL_AUTH_KEY` is NOT accepted here — it is deliberately
 * absent from {@link InstallationProvisionEnv} so a future edit cannot reach
 * for it by accident. The arms are load-bearing:
 *
 *   a) dedicated key set AND `>= MIN_INTERNAL_AUTH_KEY_LEN`           → dedicated
 *   b) dedicated key set BUT `< MIN_INTERNAL_AUTH_KEY_LEN`           → null
 *      (FAIL CLOSED; never silently widens to the shared key on a typo —
 *       an operator who set a dedicated key DECLARED this consumer should be
 *       ISOLATED, so silently widening on a typo is the opposite of the
 *       intent; the floor and the consumer name are logged)
 *   c) dedicated key NOT set, shared key set AND `>= MIN…`            → shared
 *   d) otherwise                                                     → null
 *
 * The status contract distinguishes two failures that used to share 403:
 *
 *   - resolver returns `null` (no properly-sized key bound)         → **503**
 *     `{"error":"unavailable"}` — a statement about US, not the caller.
 *     The endpoint cannot evaluate authz at all right now. Callers branch
 *     on this: a 5xx is retryable, a 403 is a hard deny. 503 also matches
 *     the frozen Rust contract (`routes/internal_pat.rs::build_state_from_env`)
 *     for the same condition. The TypeScript side was the half that drifted.
 *   - resolver returns a key, presented Bearer does not match         → **401**
 *     `{"error":"unauthorized"}` — a verdict ABOUT THE CALLER.
 *
 * Both still fail CLOSED — nothing is authorized either way.
 */

import {
  signupArtifactHandle,
  signupOwnershipContext,
  writeSignupArtifactBatch,
} from "../signup_writer_ownership.js";
import type { StagingOwnershipContext } from "../staging_load_test_ownership.js";

export interface InstallationProvisionEnv {
  /** Deployment environment and staging-only admission signing key. */
  ENVIRONMENT?: string;
  CORELINK_STAGING_LOAD_TEST_ADMISSION_KEY?: string;
  /**
   * D1 CONFIG_DB binding — holds `tenant_gh_installation_map` (0084) and
   * `runner_repo_allowlist` (0085). Same binding `clerk.ts` writes
   * `tenant_org_map` through. Absent (dev/CI without the binding) → the handler
   * fails closed with 500 (a provisioning call that cannot persist is an error,
   * not a silent no-op).
   */
  CONFIG_DB?: D1Database;

  /**
   * DEDICATED internal-auth secret for the runner-provisioning consumer. The
   * `Authorization: Bearer <token>` on the provisioning call MUST equal this.
   * Set via `wrangler secret put CORELINK_RUNNER_PROVISION_AUTH_KEY`. Mirrors
   * the per-consumer key-split the main Worker enforces for the other
   * `/_internal/*` surfaces, so leaking this consumer's secret does not unlock
   * the rest of the internal auth family.
   *
   * PHASE 2: this is the ONLY credential this route accepts. Unset, or set
   * below `MIN_INTERNAL_AUTH_KEY_LEN`, both fail CLOSED with a 503 — the route
   * does NOT fall back to the shared `CORELINK_INTERNAL_AUTH_KEY`, which is
   * why that secret is not a member of this interface at all. Making it
   * unreachable by TYPE, not only by control flow, is the point: a later edit
   * cannot re-widen this consumer without first re-declaring the dependency.
   */
  CORELINK_RUNNER_PROVISION_AUTH_KEY?: string;
}

/**
 * Minimum length (chars) of the internal-auth secret. Mirrors the container's
 * `build_state_from_env` floor (`openssl rand -hex 32` → 64 chars; the floor is
 * 32) AND `worker/src/lib/internal_auth.ts::MIN_INTERNAL_AUTH_KEY_LEN`. A
 * shorter/absent secret fails the endpoint CLOSED (arm b for the dedicated
 * key, the shared-key check in arm c, and the arm-d fallback all apply this
 * floor).
 */
const MIN_INTERNAL_AUTH_KEY_LEN = 32;

interface ProvisionBody {
  installation_id: string;
  tenant_id: string;
  repositories: string[];
}

interface DeprovisionBody {
  request_id: string;
  installation_id: string;
  tenant_id: string;
  repositories: string[];
  remove_installation: boolean;
}

const DEPROVISION_EVENT = "corelink.runner.installation_deprovision_requested";
const DEPROVISION_BODY_KEYS = [
  "installation_id",
  "remove_installation",
  "repositories",
  "request_id",
  "tenant_id",
].sort();
const DEPROVISION_DATA_KEYS = [
  "installation_id",
  "remove_installation",
  "repositories_sha256",
  "repository_count",
].sort();

interface DeprovisionData {
  installation_id: string;
  repositories_sha256: string;
  repository_count: number;
  remove_installation: boolean;
}

interface AuditCloudEvent {
  specversion: "1.0";
  id: string;
  source: "corelink-signup-worker";
  type: typeof DEPROVISION_EVENT;
  subject: string;
  time: string;
  data: DeprovisionData;
}

/**
 * Constant-time string equality over the UTF-8 bytes. Guards the internal-auth
 * compare against a timing side-channel that could otherwise let an attacker
 * recover the key byte-by-byte. Length is folded into the accumulator (not an
 * early `return`) so mismatched-length inputs are indistinguishable in time from
 * same-length mismatches.
 */
export function constantTimeEqual(a: string, b: string): boolean {
  const enc = new TextEncoder();
  const ab = enc.encode(a);
  const bb = enc.encode(b);
  let diff = ab.length ^ bb.length;
  const n = Math.max(ab.length, bb.length);
  for (let i = 0; i < n; i++) {
    // Reads past the shorter array yield `undefined`; `?? 0` normalizes to a
    // byte value so the XOR still runs for the full `n` iterations.
    diff |= (ab[i] ?? 0) ^ (bb[i] ?? 0);
  }
  return diff === 0;
}

function json(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

/**
 * Resolve the internal-auth key to verify against for the runner-provisioning
 * consumer. Mirrors `worker/src/lib/internal_auth.ts::resolveConsumerKey`'s
 * four-arm selection for a NEW consumer key:
 *   - `CORELINK_RUNNER_PROVISION_AUTH_KEY` (dedicated; the ONLY key accepted)
 *
 * Selection:
 *   a) dedicated key SET and `>= MIN_INTERNAL_AUTH_KEY_LEN` → use the dedicated
 *   b) dedicated key SET but `< MIN_INTERNAL_AUTH_KEY_LEN` → `null` (fail-CLOSED, logged)
 *   c) dedicated key UNSET                                  → `null` (fail-CLOSED, logged)
 *
 * PHASE 2 (this change): the shared `CORELINK_INTERNAL_AUTH_KEY` is NO LONGER
 * accepted here, in either direction. Phase 1 kept it as a fallback so the
 * per-consumer split could roll out without a flag-day; that migration is
 * DONE — the dedicated key is bound on `corelink-signup-worker` and was proven
 * live end to end (dedicated ⇒ auth passes; shared ⇒ 401; absent/wrong ⇒ 401).
 *
 * Why removing arm (c) matters even though binding the dedicated key already
 * makes it unreachable: while the fallback exists, UNBINDING the dedicated key
 * silently re-widens this endpoint back to the broad shared secret instead of
 * failing. The isolation would then depend on a secret staying bound — an
 * operator action — rather than on the code. It now depends on the code: with
 * no dedicated key there is no gate to evaluate, and the route answers 503.
 *
 * Arm (b) is the subtle one and is unchanged. An operator who sets a dedicated
 * key for this consumer has DECLARED that consumer should be ISOLATED; silently
 * serving it the broad shared key on a typo would WIDEN the blast radius
 * exactly when the operator was trying to NARROW it. A sub-floor secret is a
 * misconfiguration to surface, not one to route around.
 *
 * ⚠️ OPERATIONAL CONSEQUENCE: unbinding or shortening
 * `CORELINK_RUNNER_PROVISION_AUTH_KEY` takes this endpoint down (503) rather
 * than degrading it. That is the intent — a 503 says "we cannot evaluate
 * authz", which is retryable and visible, where a silent widening is neither.
 *
 * @returns the chosen key string, or `null` when neither qualifies (fail-CLOSED;
 *   the caller MUST then return 503 — no properly sized gate is bound).
 */
function resolveRunnerProvisionKey(env: InstallationProvisionEnv): string | null {
  const specific = env.CORELINK_RUNNER_PROVISION_AUTH_KEY;
  if (specific && specific.length > 0) {
    // A dedicated key was EXPLICITLY provided for this consumer.
    if (specific.length >= MIN_INTERNAL_AUTH_KEY_LEN) {
      return specific;
    }
    // Set but below the floor: a misconfiguration. Do NOT silently fall back to
    // the broad shared key — that would give this consumer a WIDER blast radius
    // than the operator intended (the whole point of a dedicated key is to
    // ISOLATE it). Fail LOUD + fail-CLOSED: this consumer's gate rejects
    // everything until the key is fixed or unset. Mirrors
    // `resolveConsumerKey` arm (b) in `worker/src/lib/internal_auth.ts`.
    console.error(
      `[runner-provision-auth] dedicated key for consumer "runner_provision" is ` +
        `set but < ${MIN_INTERNAL_AUTH_KEY_LEN} chars — REFUSING to fall back ` +
        `to the shared CORELINK_INTERNAL_AUTH_KEY (that would silently widen the ` +
        `blast radius). Fix the dedicated key to >= ${MIN_INTERNAL_AUTH_KEY_LEN} ` +
        `chars, or unset it to intentionally use the shared key.`,
    );
    return null;
  }
  // Arm (c). No dedicated key configured. PHASE 2: there is NO shared-key
  // fallback any more — this consumer is `DEDICATED_REQUIRED`. Falling back
  // would mean the isolation holds only while a secret stays bound, so that
  // unbinding it silently re-widens the endpoint to the broad shared key
  // instead of failing. Fail-CLOSED and LOUD: the caller returns 503, which
  // says "we cannot evaluate authz" (retryable, visible) rather than pretending
  // to be a verdict about the caller.
  console.error(
    `[runner-provision-auth] no dedicated key bound for consumer ` +
      `"runner_provision" — CORELINK_RUNNER_PROVISION_AUTH_KEY is REQUIRED ` +
      `(>= ${MIN_INTERNAL_AUTH_KEY_LEN} chars) and the shared ` +
      `CORELINK_INTERNAL_AUTH_KEY is NOT accepted here. The endpoint answers ` +
      `503 until the dedicated key is bound.`,
  );
  return null;
}

/**
 * Persist the installation→tenant map + repo allowlist (idempotent, transactional
 * where the binding supports `.batch`). Shared by the internal-auth provisioning
 * endpoint ([`handleInstallationProvision`]) AND the identity-gated install
 * callback ([`../webhooks/github_install_callback`]), so the exact same write
 * path serves both the Option-A (fabric-called) and Option-B (self-owned
 * callback) provisioning triggers. Throws on a D1 fault (the callers translate
 * to a fail-closed 500 — never a partial-success claim).
 */
export async function writeInstallationProvision(
  db: D1Database,
  opts: {
    installationId: string;
    tenantId: string;
    repos: string[];
    nowMs: number;
    ownershipContext?: StagingOwnershipContext | null;
  },
): Promise<void> {
  if (opts.ownershipContext) {
    const existingMap = await db
      .prepare(
        "SELECT 1 AS present FROM tenant_gh_installation_map " +
          "WHERE installation_id = ?1 LIMIT 1",
      )
      .bind(opts.installationId)
      .first<{ present: number }>();
    const existingAllowlist = await db
      .prepare(
        "SELECT 1 AS present FROM runner_repo_allowlist WHERE tenant_id = ?1 " +
          "AND repo_full_name IN (SELECT CAST(value AS TEXT) FROM json_each(?2)) LIMIT 1",
      )
      .bind(opts.tenantId, JSON.stringify(opts.repos))
      .first<{ present: number }>();
    if (existingMap !== null || existingAllowlist !== null) {
      throw new Error("staging GitHub signup artifact already exists");
    }
  }

  const statements = [
    db
      .prepare(
        "INSERT OR IGNORE INTO tenant_gh_installation_map " +
          "(installation_id, tenant_id, created_at_ms) VALUES (?1, ?2, ?3)",
      )
      .bind(opts.installationId, opts.tenantId, opts.nowMs),
    ...opts.repos.map((repo) =>
      db
        .prepare(
          "INSERT OR IGNORE INTO runner_repo_allowlist " +
            "(tenant_id, repo_full_name, created_at_ms) VALUES (?1, ?2, ?3)",
        )
        .bind(opts.tenantId, repo, opts.nowMs),
    ),
  ];
  if (opts.ownershipContext) {
    await writeSignupArtifactBatch(
      db,
      opts.ownershipContext,
      await signupArtifactHandle("github-installation", opts.installationId),
      statements,
      opts.nowMs,
    );
  } else if (typeof db.batch === "function") {
    await db.batch(statements);
  } else {
    for (const st of statements) {
      await st.run();
    }
  }
}

/**
 * `POST /internal/v1/runner/provision-installation`.
 *
 * Body: `{ installation_id: string, tenant_id: string, repositories: string[] }`.
 * On success → 200 `{ installation_id, repos_added }`.
 */
export async function handleInstallationProvision(
  request: Request,
  env: InstallationProvisionEnv,
): Promise<Response> {
  // 1. Method gate.
  if (request.method !== "POST") {
    return json(405, { error: "method_not_allowed" });
  }

  // 2. Internal-auth gate (fail-closed, constant-time).
  //    Resolver follows the dedicated/shared arms documented on
  //    `resolveRunnerProvisionKey`; the STATUS distinction is load-bearing
  //    (see the module header) and matches the main Worker's
  //    `requireConsumerAuth` / `requireInternalAuth`:
  //      - resolver `null` (no properly-sized key bound) → 503 "unavailable"
  //        (statement about US; endpoint cannot evaluate authz; a 5xx is
  //        retryable, where the prior 403 was a hard deny)
  //      - resolver returned a key, presented Bearer does not match → 401
  //        "unauthorized" (verdict about the CALLER)
  const expected = resolveRunnerProvisionKey(env);
  if (expected === null) {
    return json(503, { error: "unavailable" });
  }
  const authz = request.headers.get("authorization") ?? "";
  const prefix = "Bearer ";
  const presented = authz.startsWith(prefix) ? authz.slice(prefix.length) : "";
  if (!constantTimeEqual(presented, expected)) {
    return json(401, { error: "unauthorized" });
  }

  // 3. Parse + validate body.
  let body: Partial<ProvisionBody>;
  try {
    body = (await request.json()) as Partial<ProvisionBody>;
  } catch {
    return json(400, { error: "invalid_json" });
  }
  const installationId = body.installation_id;
  const tenantId = body.tenant_id;
  if (
    typeof installationId !== "string" ||
    installationId.length === 0 ||
    typeof tenantId !== "string" ||
    tenantId.length === 0
  ) {
    return json(400, { error: "installation_id and tenant_id are required" });
  }
  const repositories = Array.isArray(body.repositories) ? body.repositories : [];
  // Only well-formed non-empty `owner/repo` strings are allowlisted; anything
  // else is dropped rather than written as a junk allowlist row.
  const repos = repositories.filter(
    (r): r is string => typeof r === "string" && r.length > 0,
  );

  let ownershipContext: StagingOwnershipContext | null;
  try {
    ownershipContext = await signupOwnershipContext(
      request,
      env.ENVIRONMENT,
      env.CORELINK_STAGING_LOAD_TEST_ADMISSION_KEY,
    );
  } catch {
    return json(403, { error: "invalid_staging_ownership" });
  }

  const db = env.CONFIG_DB;
  if (!db) {
    // A provisioning call that cannot persist is an error, not a silent no-op.
    return json(500, { error: "config_db_unavailable" });
  }

  // 4. D1 writes (idempotent, transactional via the shared write path). One
  // `Date.now()` for all rows in this call.
  const nowMs = Date.now();
  try {
    await writeInstallationProvision(db, {
      installationId,
      tenantId,
      repos,
      nowMs,
      ownershipContext,
    });
  } catch {
    // 5. Fail-closed: never partially claim success on a D1 fault.
    return json(500, { error: "provision_failed" });
  }

  return json(200, { installation_id: installationId, repos_added: repos.length });
}

function hasExactKeys(value: Record<string, unknown>, keys: string[]): boolean {
  const actual = Object.keys(value).sort();
  return actual.length === keys.length && actual.every((key, index) => key === keys[index]);
}

function canonicalRepository(value: unknown): value is string {
  if (typeof value !== "string") return false;
  const parts = value.split("/");
  return parts.length === 2 && parts.every((part) => /^[A-Za-z0-9_.-]+$/.test(part));
}

function validIdentifier(value: unknown): value is string {
  return typeof value === "string" && value.trim().length > 0 && !/[\u0000-\u001f\u007f]/.test(value);
}

function validRequestId(value: unknown): value is string {
  return typeof value === "string" && /^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$/.test(value);
}

async function deprovisionData(body: DeprovisionBody): Promise<DeprovisionData> {
  const repositories = [...body.repositories].sort();
  const digest = await crypto.subtle.digest(
    "SHA-256",
    new TextEncoder().encode(JSON.stringify(repositories)),
  );
  const repositoriesSha256 = [...new Uint8Array(digest)]
    .map((byte) => byte.toString(16).padStart(2, "0"))
    .join("");
  return {
    installation_id: body.installation_id,
    repositories_sha256: repositoriesSha256,
    repository_count: repositories.length,
    remove_installation: body.remove_installation,
  };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isCanonicalReplay(
  payloadJson: unknown,
  rowTenantId: unknown,
  body: DeprovisionBody,
  expectedData: DeprovisionData,
): boolean {
  if (typeof payloadJson !== "string" || rowTenantId !== body.tenant_id) return false;
  let parsed: unknown;
  try {
    parsed = JSON.parse(payloadJson);
  } catch {
    return false;
  }
  if (!isRecord(parsed)) return false;
  const envelopeKeys = ["data", "id", "source", "specversion", "subject", "time", "type"].sort();
  if (!hasExactKeys(parsed, envelopeKeys)) return false;
  if (
    parsed.specversion !== "1.0" ||
    parsed.source !== "corelink-signup-worker" ||
    parsed.type !== DEPROVISION_EVENT ||
    typeof parsed.id !== "string" ||
    parsed.id.length === 0 ||
    parsed.subject !== `installation/${body.installation_id}` ||
    typeof parsed.time !== "string" ||
    !Number.isFinite(Date.parse(parsed.time))
  ) return false;
  const time = new Date(parsed.time as string).toISOString();
  if (time !== parsed.time || !isRecord(parsed.data) || !hasExactKeys(parsed.data, DEPROVISION_DATA_KEYS)) {
    return false;
  }
  const data = parsed.data;
  return (
    data.installation_id === expectedData.installation_id &&
    data.repositories_sha256 === expectedData.repositories_sha256 &&
    data.repository_count === expectedData.repository_count &&
    data.remove_installation === expectedData.remove_installation
  );
}

async function deprovisionStateMatches(
  db: D1Database,
  body: DeprovisionBody,
): Promise<boolean> {
  const requestedRemaining = await db
    .prepare(
      "SELECT repo_full_name FROM runner_repo_allowlist " +
        "WHERE tenant_id = ?1 AND repo_full_name IN " +
        "(SELECT CAST(value AS TEXT) FROM json_each(?2))",
    )
    .bind(body.tenant_id, JSON.stringify(body.repositories))
    .all<{ repo_full_name: string }>();
  if ((requestedRemaining.results ?? []).length !== 0) return false;

  const map = await db
    .prepare(
      "SELECT installation_id, tenant_id FROM tenant_gh_installation_map " +
        "WHERE installation_id = ?1",
    )
    .bind(body.installation_id)
    .first<{ installation_id: string; tenant_id: string }>();
  if (body.remove_installation ? map !== null : map?.tenant_id !== body.tenant_id) {
    return false;
  }

  if (body.remove_installation) {
    const allowlist = await db
      .prepare("SELECT repo_full_name FROM runner_repo_allowlist WHERE tenant_id = ?1")
      .bind(body.tenant_id)
      .all<{ repo_full_name: string }>();
    if ((allowlist.results ?? []).length !== 0) return false;
  }
  return true;
}

/**
 * `DELETE /internal/v1/runner/provision-installation`.
 * Deletes only explicitly named allowlist entries and optionally the exact
 * installation mapping, with an atomic audit-outbox record.
 */
export async function handleInstallationDeprovision(
  request: Request,
  env: InstallationProvisionEnv,
): Promise<Response> {
  if (request.method !== "DELETE") return json(405, { error: "method_not_allowed" });

  const expected = resolveRunnerProvisionKey(env);
  if (expected === null) return json(503, { error: "unavailable" });
  const authz = request.headers.get("authorization") ?? "";
  const prefix = "Bearer ";
  const presented = authz.startsWith(prefix) ? authz.slice(prefix.length) : "";
  if (!constantTimeEqual(presented, expected)) return json(401, { error: "unauthorized" });

  let input: unknown;
  try {
    input = await request.json();
  } catch {
    return json(400, { error: "invalid_json" });
  }
  if (!isRecord(input) || !hasExactKeys(input, DEPROVISION_BODY_KEYS)) {
    return json(400, { error: "invalid_body" });
  }
  const body = input as unknown as DeprovisionBody;
  if (
    !validRequestId(body.request_id) ||
    !validIdentifier(body.installation_id) ||
    !validIdentifier(body.tenant_id) ||
    !Array.isArray(body.repositories) ||
    typeof body.remove_installation !== "boolean" ||
    body.repositories.some((repo) => !canonicalRepository(repo)) ||
    new Set(body.repositories).size !== body.repositories.length ||
    (body.repositories.length === 0 && body.remove_installation !== true)
  ) {
    return json(400, { error: "invalid_body" });
  }

  const db = env.CONFIG_DB;
  if (!db) return json(500, { error: "config_db_unavailable" });
  if (typeof db.batch !== "function") return json(500, { error: "deprovision_failed" });

  const repos = [...body.repositories].sort();
  let data: DeprovisionData;
  try {
    data = await deprovisionData(body);
    const existing = await db
      .prepare(
        "SELECT tenant_id, request_id, event_type, payload_json FROM audit_outbox " +
          "WHERE request_id = ?1 AND event_type = ?2",
      )
      .bind(body.request_id, DEPROVISION_EVENT)
      .first<{ tenant_id: string; request_id: string; event_type: string; payload_json: string }>();
    if (existing) {
      if (
        existing.request_id !== body.request_id ||
        existing.event_type !== DEPROVISION_EVENT ||
        !isCanonicalReplay(existing.payload_json, existing.tenant_id, body, data)
      ) {
        return json(409, { error: "request_id_conflict" });
      }
      if (!(await deprovisionStateMatches(db, body))) {
        return json(500, { error: "deprovision_verify_failed" });
      }
      return json(200, {
        installation_id: body.installation_id,
        repos_removed: data.repository_count,
        installation_removed: body.remove_installation,
        replayed: true,
      });
    }

    const installation = await db
      .prepare(
        "SELECT installation_id, tenant_id FROM tenant_gh_installation_map WHERE installation_id = ?1",
      )
      .bind(body.installation_id)
      .first<{ installation_id: string; tenant_id: string }>();
    if (!installation) return json(404, { error: "installation_not_found" });
    if (installation.tenant_id !== body.tenant_id) return json(409, { error: "tenant_mismatch" });

    const tenant = await db
      .prepare("SELECT tenant_id, primary_region FROM tenant WHERE tenant_id = ?1")
      .bind(body.tenant_id)
      .first<{ tenant_id: string; primary_region: string }>();
    if (!tenant || typeof tenant.primary_region !== "string" || tenant.primary_region.length === 0) {
      return json(500, { error: "tenant_region_unavailable" });
    }

    const allowlist = await db
      .prepare("SELECT repo_full_name FROM runner_repo_allowlist WHERE tenant_id = ?1")
      .bind(body.tenant_id)
      .all<{ repo_full_name: string }>();
    const currentRepos = (allowlist.results ?? []).map((row) => row.repo_full_name);
    const currentSet = new Set(currentRepos);
    if (repos.some((repo) => !currentSet.has(repo))) return json(404, { error: "repository_not_found" });
    if (body.remove_installation && currentRepos.some((repo) => !repos.includes(repo))) {
      return json(409, { error: "installation_repositories_remain" });
    }

    const event: AuditCloudEvent = {
      specversion: "1.0",
      id: crypto.randomUUID(),
      source: "corelink-signup-worker",
      type: DEPROVISION_EVENT,
      subject: `installation/${body.installation_id}`,
      time: new Date().toISOString(),
      data,
    };
    const statements = [];
    if (body.remove_installation) {
      statements.push(
        db
          .prepare(
            "DELETE FROM tenant_gh_installation_map WHERE installation_id = ?1 AND tenant_id = ?2 " +
              "AND NOT EXISTS (SELECT 1 FROM runner_repo_allowlist WHERE tenant_id = ?2 " +
              "AND repo_full_name NOT IN (SELECT CAST(value AS TEXT) FROM json_each(?3)))",
          )
          .bind(body.installation_id, body.tenant_id, JSON.stringify(repos)),
      );
    }
    statements.push(
      ...repos.map((repo) =>
        db
          .prepare(
            "DELETE FROM runner_repo_allowlist WHERE tenant_id = ?1 AND repo_full_name = ?2 " +
              (body.remove_installation
                ? "AND NOT EXISTS (SELECT 1 FROM tenant_gh_installation_map WHERE installation_id = ?3 AND tenant_id = ?1)"
                : "AND EXISTS (SELECT 1 FROM tenant_gh_installation_map WHERE installation_id = ?3 AND tenant_id = ?1)"),
          )
          .bind(body.tenant_id, repo, body.installation_id),
      ),
    );

    const repoAbsenceGuard =
      "NOT EXISTS (SELECT 1 FROM runner_repo_allowlist WHERE tenant_id = ?2 " +
      "AND repo_full_name IN (SELECT CAST(value AS TEXT) FROM json_each(?10)))";
    const expectedMapGuard = body.remove_installation
      ? "NOT EXISTS (SELECT 1 FROM tenant_gh_installation_map WHERE installation_id = ?9 AND tenant_id = ?2) AND NOT EXISTS (SELECT 1 FROM runner_repo_allowlist WHERE tenant_id = ?2)"
      : "EXISTS (SELECT 1 FROM tenant_gh_installation_map WHERE installation_id = ?9 AND tenant_id = ?2)";
    statements.push(
      db
        .prepare(
          "INSERT INTO audit_outbox " +
            "(id, tenant_id, region, digest, request_id, event_type, payload_json, enqueued_at) " +
            "SELECT ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8 WHERE " +
            repoAbsenceGuard + " AND " + expectedMapGuard,
        )
        .bind(
          event.id,
          body.tenant_id,
          tenant.primary_region,
          null,
          body.request_id,
          DEPROVISION_EVENT,
          JSON.stringify(event),
          Date.now(),
          body.installation_id,
          JSON.stringify(repos),
        ),
    );
    await db.batch(statements);
    if (!(await deprovisionStateMatches(db, body))) {
      return json(500, { error: "deprovision_verify_failed" });
    }
    return json(200, {
      installation_id: body.installation_id,
      repos_removed: data.repository_count,
      installation_removed: body.remove_installation,
      replayed: false,
    });
  } catch {
    return json(500, { error: "deprovision_failed" });
  }
}
