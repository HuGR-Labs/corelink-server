/**
 * Unit tests for the cf-multitenant WP4 provisioning primitive
 * (`handleInstallationProvision`). Pins the fail-closed gates + the idempotent
 * D1 writes to BOTH `tenant_gh_installation_map` (0084) and
 * `runner_repo_allowlist` (0085).
 *
 * The D1 mock captures every `prepare(sql).bind(...vals)` as a
 * `{ sql, vals }` record so the tests assert the exact statements + binds. Each
 * test uses a DISTINCT `installation_id` (no cross-test pollution).
 */
import { describe, it, expect } from "vitest";
import {
  handleInstallationProvision,
  type InstallationProvisionEnv,
} from "../src/webhooks/github_provision.js";

/**
 * Shared-key fixture: 64 hex chars — mirrors `openssl rand -hex 32` (the format
 * the main Worker's internal-auth gate and the container's
 * `build_state_from_env` floor both target). MUST be `>= 32` chars; the new
 * `resolveRunnerProvisionKey` rejects anything shorter (arm c on the shared
 * key, arm b on the dedicated key), so under-sized fixtures silently shift
 * tests from "auth path" to "503 unavailable" without an obvious cause.
 */
const AUTH = "a".repeat(64);

/**
 * Dedicated-key fixture for the runner-provisioning consumer. Same length
 * floor as `AUTH`; distinct value so a "did the right key get picked up?"
 * assertion in arm (a) is meaningful.
 */
const DEDICATED = "b".repeat(64);

interface Captured {
  sql: string;
  vals: unknown[];
}

/** A D1 mock that records prepared statements; `throwOnRun` forces a fault. */
function db(opts: { throwOnRun?: boolean } = {}): {
  binding: NonNullable<InstallationProvisionEnv["CONFIG_DB"]>;
  captured: Captured[];
} {
  const captured: Captured[] = [];
  const binding = {
    prepare: (sql: string) => ({
      bind: (...vals: unknown[]) => {
        const rec: Captured = { sql, vals };
        const stmt = {
          run: async () => {
            captured.push(rec);
            if (opts.throwOnRun) throw new Error("d1 fault");
            return { success: true };
          },
        };
        return stmt;
      },
    }),
    batch: async (statements: Array<{ run(): Promise<unknown> }>) => {
      // Mirror D1's transactional batch: run each (which records + may throw).
      const out: unknown[] = [];
      for (const st of statements) out.push(await st.run());
      return out;
    },
  } as unknown as NonNullable<InstallationProvisionEnv["CONFIG_DB"]>;
  return { binding, captured };
}

function req(
  body: unknown,
  opts: { method?: string; auth?: string | null } = {},
): Request {
  const headers: Record<string, string> = { "content-type": "application/json" };
  if (opts.auth !== null) {
    headers["authorization"] = `Bearer ${opts.auth ?? AUTH}`;
  }
  const method = opts.method ?? "POST";
  const hasBody = method !== "GET" && method !== "HEAD";
  return new Request("https://sw/internal/v1/runner/provision-installation", {
    method,
    headers,
    body: hasBody
      ? typeof body === "string"
        ? body
        : JSON.stringify(body)
      : undefined,
  });
}

describe("handleInstallationProvision — gates", () => {
  it("non-POST → 405", async () => {
    const { binding } = db();
    const env: InstallationProvisionEnv = {
      CONFIG_DB: binding,
      CORELINK_RUNNER_PROVISION_AUTH_KEY: AUTH,
    };
    const r = await handleInstallationProvision(
      req({}, { method: "GET" }),
      env,
    );
    expect(r.status).toBe(405);
  });

  it("missing Authorization → 401 unauthorized (verdict about the CALLER)", async () => {
    const { binding } = db();
    const env: InstallationProvisionEnv = {
      CONFIG_DB: binding,
      CORELINK_RUNNER_PROVISION_AUTH_KEY: AUTH,
    };
    const r = await handleInstallationProvision(
      req(
        { installation_id: "i-401a", tenant_id: "t1", repositories: [] },
        { auth: null },
      ),
      env,
    );
    expect(r.status).toBe(401);
    expect(await r.json()).toEqual({ error: "unauthorized" });
  });

  it("mismatched bearer → 401 unauthorized (verdict about the CALLER)", async () => {
    const { binding } = db();
    const env: InstallationProvisionEnv = {
      CONFIG_DB: binding,
      CORELINK_RUNNER_PROVISION_AUTH_KEY: AUTH,
    };
    const r = await handleInstallationProvision(
      req(
        { installation_id: "i-401b", tenant_id: "t1", repositories: [] },
        { auth: "wrong-key" },
      ),
      env,
    );
    expect(r.status).toBe(401);
    expect(await r.json()).toEqual({ error: "unauthorized" });
  });

  it("neither key bound → 503 unavailable (statement about US, not 403)", async () => {
    const { binding } = db();
    const env: InstallationProvisionEnv = { CONFIG_DB: binding };
    const r = await handleInstallationProvision(
      req({ installation_id: "i-503a", tenant_id: "t1", repositories: [] }),
      env,
    );
    expect(r.status).toBe(503);
    expect(await r.json()).toEqual({ error: "unavailable" });
  });

  it("missing installation_id → 400", async () => {
    const { binding } = db();
    const env: InstallationProvisionEnv = {
      CONFIG_DB: binding,
      CORELINK_RUNNER_PROVISION_AUTH_KEY: AUTH,
    };
    const r = await handleInstallationProvision(
      req({ tenant_id: "t1", repositories: [] }),
      env,
    );
    expect(r.status).toBe(400);
  });

  it("missing tenant_id → 400", async () => {
    const { binding } = db();
    const env: InstallationProvisionEnv = {
      CONFIG_DB: binding,
      CORELINK_RUNNER_PROVISION_AUTH_KEY: AUTH,
    };
    const r = await handleInstallationProvision(
      req({ installation_id: "i-400b", repositories: [] }),
      env,
    );
    expect(r.status).toBe(400);
  });
});

describe("handleInstallationProvision — resolver arms (dedicated/shared)", () => {
  // These tests pin the four-arm selection in
  // `resolveRunnerProvisionKey` AND the new 503/401 status contract. The
  // arms mirror `worker/src/lib/internal_auth.ts::resolveConsumerKey`; the
  // load-bearing one is (b) — a set-but-sub-floor dedicated key MUST NOT
  // silently widen to the shared key, which is the regression this resolver
  // exists to prevent.

  it("arm (a) dedicated key (>= 32) accepted; shared key then REJECTED with 401", async () => {
    const { binding } = db();
    // The dedicated key is bound and properly sized. Any OTHER properly-sized
    // secret — the shared `CORELINK_INTERNAL_AUTH_KEY` being the one that
    // matters operationally — must NOT unlock this surface. `AUTH` stands in
    // for it here: as of PHASE 2 the shared key is not a member of
    // `InstallationProvisionEnv` at all, so a test cannot bind it even by
    // mistake, and what is left to prove is that presenting it gets a 401.
    const env: InstallationProvisionEnv = {
      CONFIG_DB: binding,
      CORELINK_RUNNER_PROVISION_AUTH_KEY: DEDICATED,
    };
    // 1. Presented Bearer = dedicated key → authorized.
    const ok = await handleInstallationProvision(
      req(
        {
          installation_id: "i-arm-a-ok",
          tenant_id: "t1",
          repositories: ["acme/web"],
        },
        { auth: DEDICATED },
      ),
      env,
    );
    expect(ok.status).toBe(200);

    // 2. Presented Bearer = the shared-key stand-in → REJECTED with 401, not
    //    200, not 503. The resolver picked the dedicated key (so the gate IS
    //    bound); the presented value simply did not match it.
    const denied = await handleInstallationProvision(
      req(
        {
          installation_id: "i-arm-a-deny",
          tenant_id: "t1",
          repositories: ["acme/web"],
        },
        { auth: AUTH },
      ),
      env,
    );
    expect(denied.status).toBe(401);
    expect(await denied.json()).toEqual({ error: "unauthorized" });
  });

  it("arm (b) dedicated key set but 10 chars → 503; shared key does NOT work (no widening)", async () => {
    const { binding } = db();
    // Dedicated set but sub-floor (10 chars, well below the 32-char
    // minimum) → the resolver returns null and the route answers 503 with a
    // logged refusal. We present a DIFFERENT properly-sized secret (the
    // stand-in for the shared key) rather than the sub-floor value itself:
    // presenting the sub-floor value would produce a 401 under a naive
    // implementation too, so it could not distinguish "refused to resolve"
    // from "compared and mismatched". A 503 here can only come from the
    // resolver declining to hand back ANY key.
    const shortDedicated = "short-key1"; // 10 chars, < 32 floor
    const env: InstallationProvisionEnv = {
      CONFIG_DB: binding,
      CORELINK_RUNNER_PROVISION_AUTH_KEY: shortDedicated,
    };
    const r = await handleInstallationProvision(
      req(
        {
          installation_id: "i-arm-b",
          tenant_id: "t1",
          repositories: ["acme/web"],
        },
        { auth: AUTH },
      ),
      env,
    );
    // The shared key is what was presented, the shared key is what is
    // bound AND properly sized, and the call is still 503 — proof that
    // arm (b) refused to widen. NOT 200, NOT 401.
    expect(r.status).toBe(503);
    expect(await r.json()).toEqual({ error: "unavailable" });
  });

  it("arm (c) no dedicated key → 503, and NO shared-key fallback remains", async () => {
    const { binding, captured } = db();
    // PHASE 2, and the behaviour change this test exists for. Under phase 1
    // this same call was a 200: the route fell back to the shared
    // `CORELINK_INTERNAL_AUTH_KEY` whenever the dedicated key was absent.
    //
    // Why the fallback had to go even though binding the dedicated key already
    // made it unreachable in production: while it existed, the isolation held
    // only for as long as a secret STAYED bound. UNBINDING
    // `CORELINK_RUNNER_PROVISION_AUTH_KEY` would silently re-widen this
    // endpoint back to the broad shared secret rather than fail — an operator
    // action quietly undoing a security property. Now the property lives in
    // the code: no dedicated key, no gate to evaluate, 503.
    //
    // The shared key IS bound here, through a cast. That is deliberate and it
    // is the whole point of the test. Removing the field from
    // `InstallationProvisionEnv` stops a TYPED read, but the live Worker `Env`
    // is an intersection that still carries `CORELINK_INTERNAL_AUTH_KEY`, so a
    // future edit could reach it with exactly this cast. A test that merely
    // OMITTED the key would stay GREEN if the fallback were restored — it
    // would be asserting nothing. Verified by reverting: with the fallback put
    // back and the key bound as below, this case fails `expected 200 to be
    // 503`.
    const env = {
      CONFIG_DB: binding,
      CORELINK_INTERNAL_AUTH_KEY: AUTH,
    } as InstallationProvisionEnv & { CORELINK_INTERNAL_AUTH_KEY: string };
    const r = await handleInstallationProvision(
      req(
        {
          installation_id: "i-arm-c",
          tenant_id: "t1",
          repositories: ["acme/web"],
        },
        { auth: AUTH },
      ),
      env,
    );
    expect(r.status).toBe(503);
    expect(await r.json()).toEqual({ error: "unavailable" });
    // Fail-CLOSED means nothing was written, not merely that the response
    // said no. A 503 that had already provisioned rows would be worse than
    // a 200.
    expect(captured).toHaveLength(0);
  });

  it("arm (d) neither key bound → 503 unavailable (never 403)", async () => {
    // Cross-reference to the matching gates-block test: explicitly named
    // for the arm-d coverage so the four-arm matrix is complete in one
    // glance. Same contract: no properly-sized key resolvable → 503
    // `{"error":"unavailable"}`, not 403.
    const { binding } = db();
    const env: InstallationProvisionEnv = { CONFIG_DB: binding };
    const r = await handleInstallationProvision(
      req(
        { installation_id: "i-arm-d", tenant_id: "t1", repositories: [] },
        { auth: AUTH },
      ),
      env,
    );
    expect(r.status).toBe(503);
    expect(await r.json()).toEqual({ error: "unavailable" });
  });

  it("status contract: wrong bearer against a bound key → 401 (never 403, never 503)", async () => {
    // The resolver returned a key (shared, properly sized, PHASE-1) but
    // the presented Bearer is wrong. That is a verdict about the CALLER:
    // 401 `{"error":"unauthorized"}`. NOT 403 (config-fault costume) and
    // NOT 503 (the resolver DID return a key, so the endpoint can
    // evaluate authz).
    const { binding } = db();
    const env: InstallationProvisionEnv = {
      CONFIG_DB: binding,
      CORELINK_RUNNER_PROVISION_AUTH_KEY: AUTH,
    };
    const r = await handleInstallationProvision(
      req(
        { installation_id: "i-status-401", tenant_id: "t1", repositories: [] },
        { auth: "definitely-not-the-key" },
      ),
      env,
    );
    expect(r.status).toBe(401);
    expect(await r.json()).toEqual({ error: "unauthorized" });
  });
});

describe("handleInstallationProvision — writes", () => {
  it("happy path → 200 and BOTH tables written with correct binds", async () => {
    const { binding, captured } = db();
    const env: InstallationProvisionEnv = {
      CONFIG_DB: binding,
      CORELINK_RUNNER_PROVISION_AUTH_KEY: AUTH,
    };
    const r = await handleInstallationProvision(
      req({
        installation_id: "i-happy",
        tenant_id: "tenant-xyz",
        repositories: ["acme/web", "acme/api"],
      }),
      env,
    );
    expect(r.status).toBe(200);
    expect(await r.json()).toEqual({
      installation_id: "i-happy",
      repos_added: 2,
    });

    const map = captured.find((c) =>
      c.sql.includes("tenant_gh_installation_map"),
    );
    expect(map).toBeDefined();
    expect(map!.sql).toContain("INSERT OR IGNORE INTO tenant_gh_installation_map");
    // (installation_id, tenant_id, created_at_ms)
    expect(map!.vals[0]).toBe("i-happy");
    expect(map!.vals[1]).toBe("tenant-xyz");
    expect(typeof map!.vals[2]).toBe("number");

    const allow = captured.filter((c) =>
      c.sql.includes("runner_repo_allowlist"),
    );
    expect(allow).toHaveLength(2);
    for (const a of allow) {
      expect(a.sql).toContain("INSERT OR IGNORE INTO runner_repo_allowlist");
      // (tenant_id, repo_full_name, created_at_ms)
      expect(a.vals[0]).toBe("tenant-xyz");
      expect(typeof a.vals[2]).toBe("number");
    }
    expect(allow.map((a) => a.vals[1]).sort()).toEqual(["acme/api", "acme/web"]);

    // One Date.now() for all rows in the call.
    const stamps = new Set(captured.map((c) => c.vals[2]));
    expect(stamps.size).toBe(1);
  });

  it("empty repositories → 200 repos_added:0 and only the map written", async () => {
    const { binding, captured } = db();
    const env: InstallationProvisionEnv = {
      CONFIG_DB: binding,
      CORELINK_RUNNER_PROVISION_AUTH_KEY: AUTH,
    };
    const r = await handleInstallationProvision(
      req({ installation_id: "i-empty", tenant_id: "t-empty", repositories: [] }),
      env,
    );
    expect(r.status).toBe(200);
    expect(await r.json()).toEqual({
      installation_id: "i-empty",
      repos_added: 0,
    });
    expect(
      captured.filter((c) => c.sql.includes("runner_repo_allowlist")),
    ).toHaveLength(0);
    expect(
      captured.filter((c) => c.sql.includes("tenant_gh_installation_map")),
    ).toHaveLength(1);
  });

  it("idempotent re-call → no error (200 both times)", async () => {
    const { binding } = db();
    const env: InstallationProvisionEnv = {
      CONFIG_DB: binding,
      CORELINK_RUNNER_PROVISION_AUTH_KEY: AUTH,
    };
    const payload = {
      installation_id: "i-idem",
      tenant_id: "t-idem",
      repositories: ["acme/repo"],
    };
    const first = await handleInstallationProvision(req(payload), env);
    const second = await handleInstallationProvision(req(payload), env);
    expect(first.status).toBe(200);
    expect(second.status).toBe(200);
  });

  it("D1 fault → 500 (fail-closed, no partial success claim)", async () => {
    const { binding } = db({ throwOnRun: true });
    const env: InstallationProvisionEnv = {
      CONFIG_DB: binding,
      CORELINK_RUNNER_PROVISION_AUTH_KEY: AUTH,
    };
    const r = await handleInstallationProvision(
      req({ installation_id: "i-fault", tenant_id: "t1", repositories: ["a/b"] }),
      env,
    );
    expect(r.status).toBe(500);
  });

  it("CONFIG_DB unbound → 500 (cannot persist)", async () => {
    const env: InstallationProvisionEnv = { CORELINK_RUNNER_PROVISION_AUTH_KEY: AUTH };
    const r = await handleInstallationProvision(
      req({ installation_id: "i-nodb", tenant_id: "t1", repositories: [] }),
      env,
    );
    expect(r.status).toBe(500);
  });
});
