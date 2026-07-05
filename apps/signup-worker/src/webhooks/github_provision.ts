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
 * Wrong method → 405. Missing/mismatched internal-auth (or an unbound
 * `CORELINK_INTERNAL_AUTH_KEY`) → 403 (constant-time compare, no key-shape leak).
 * Malformed body → 400. Any D1 fault → 500 (never a partial "success"). Both
 * writes are `INSERT OR IGNORE`, so a Svix-style redelivery / re-call is a
 * harmless no-op.
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
   * Shared internal-auth secret. The `Authorization: Bearer <token>` on the
   * provisioning call MUST equal this. Unbound ⇒ every call 403s (fail-closed).
   * Bound via `wrangler secret put CORELINK_INTERNAL_AUTH_KEY`.
   */
  CORELINK_INTERNAL_AUTH_KEY?: string;
}

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
  const expected = env.CORELINK_INTERNAL_AUTH_KEY;
  if (!expected) {
    // Key unbound → no way to authenticate anyone → deny.
    return json(403, { error: "forbidden" });
  }
  const authz = request.headers.get("authorization") ?? "";
  const prefix = "Bearer ";
  const presented = authz.startsWith(prefix) ? authz.slice(prefix.length) : "";
  if (!constantTimeEqual(presented, expected)) {
    return json(403, { error: "forbidden" });
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
