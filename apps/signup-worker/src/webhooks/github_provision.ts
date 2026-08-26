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
 * Resolved per-call via {@link resolveRunnerProvisionKey}, which selects
 * between this consumer's dedicated key (`CORELINK_RUNNER_PROVISION_AUTH_KEY`)
 * and the shared `CORELINK_INTERNAL_AUTH_KEY` (PHASE 1 — the shared fallback
 * STAYS, mirroring `resolveConsumerKey`'s non-`DEDICATED_REQUIRED_CONSUMERS`
 * arms; promoting this consumer to "dedicated required" is a separate later
 * change). The dedicated/shared split arms are load-bearing:
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

export interface InstallationProvisionEnv {
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
   * `Authorization: Bearer <token>` on the provisioning call MUST equal this
   * (or, when this is UNSET, the shared `CORELINK_INTERNAL_AUTH_KEY` — see
   * {@link resolveRunnerProvisionKey}). Set via
   * `wrangler secret put CORELINK_RUNNER_PROVISION_AUTH_KEY`. Mirrors the
   * per-consumer key-split the main Worker enforces for the other
   * `/_internal/*` surfaces, so leaking this consumer's secret does not unlock
   * the rest of the internal auth family. PHASE 1: a set-but-sub-floor value
   * fails CLOSED (never widens to the shared key) — see the arm-b note in the
   * module header.
   */
  CORELINK_RUNNER_PROVISION_AUTH_KEY?: string;

  /**
   * Shared internal-auth secret. The fallback credential used when
   * `CORELINK_RUNNER_PROVISION_AUTH_KEY` is UNSET (PHASE 1; this is the
   * `resolveConsumerKey` arm-c pattern — no flag-day required to roll out the
   * dedicated key). Also must be `>= MIN_INTERNAL_AUTH_KEY_LEN` to qualify.
   * Bound via `wrangler secret put CORELINK_INTERNAL_AUTH_KEY`.
   */
  CORELINK_INTERNAL_AUTH_KEY?: string;
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
 *   - `CORELINK_RUNNER_PROVISION_AUTH_KEY` (dedicated, set per this consumer)
 *   - `CORELINK_INTERNAL_AUTH_KEY`        (shared fallback, PHASE 1)
 *
 * Selection (see the arm-by-arm commentary on each branch):
 *   a) dedicated key SET and `>= MIN_INTERNAL_AUTH_KEY_LEN` → use the dedicated
 *   b) dedicated key SET but `< MIN_INTERNAL_AUTH_KEY_LEN` → `null`, REFUSING
 *      the shared fallback (fail-CLOSED, logged)
 *   c) dedicated key UNSET, shared key SET and `>= MIN…`     → use the shared
 *   d) otherwise                                              → `null`
 *
 * Arm (b) is the subtle one. An operator who sets a dedicated key for this
 * consumer has DECLARED that consumer should be ISOLATED; silently serving it
 * the broad shared key on a typo would WIDEN the blast radius exactly when the
 * operator was trying to NARROW it. A sub-floor secret is a misconfiguration to
 * surface, not one to route around — same posture as the main Worker.
 *
 * PHASE 1: arm (c) is deliberate and remains. Promoting this consumer to
 * "dedicated required" (so an UNSET dedicated key also fails closed instead of
 * degrading to the shared key) is a separate later change. Do not add a
 * `DEDICATED_REQUIRED_CONSUMERS`-style check here without that follow-up.
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
  // No dedicated key configured for this consumer. PHASE 1: fall back to the
  // shared key when it is properly sized (arm c). This is deliberate and
  // matches `resolveConsumerKey` for a non-`DEDICATED_REQUIRED_CONSUMERS`
  // consumer — it lets the per-consumer split roll out without a flag-day.
  const shared = env.CORELINK_INTERNAL_AUTH_KEY;
  if (shared && shared.length >= MIN_INTERNAL_AUTH_KEY_LEN) {
    return shared;
  }
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
  opts: { installationId: string; tenantId: string; repos: string[]; nowMs: number },
): Promise<void> {
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
  if (typeof db.batch === "function") {
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

  const db = env.CONFIG_DB;
  if (!db) {
    // A provisioning call that cannot persist is an error, not a silent no-op.
    return json(500, { error: "config_db_unavailable" });
  }

  // 4. D1 writes (idempotent, transactional via the shared write path). One
  // `Date.now()` for all rows in this call.
  const nowMs = Date.now();
  try {
    await writeInstallationProvision(db, { installationId, tenantId, repos, nowMs });
  } catch {
    // 5. Fail-closed: never partially claim success on a D1 fault.
    return json(500, { error: "provision_failed" });
  }

  return json(200, { installation_id: installationId, repos_added: repos.length });
}
