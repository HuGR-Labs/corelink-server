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

const AUTH = "internal-secret-key";

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
      CORELINK_INTERNAL_AUTH_KEY: AUTH,
    };
    const r = await handleInstallationProvision(
      req({}, { method: "GET" }),
      env,
    );
    expect(r.status).toBe(405);
  });

  it("missing Authorization → 403", async () => {
    const { binding } = db();
    const env: InstallationProvisionEnv = {
      CONFIG_DB: binding,
      CORELINK_INTERNAL_AUTH_KEY: AUTH,
    };
    const r = await handleInstallationProvision(
      req(
        { installation_id: "i-403a", tenant_id: "t1", repositories: [] },
        { auth: null },
      ),
      env,
    );
    expect(r.status).toBe(403);
  });

  it("mismatched bearer → 403", async () => {
    const { binding } = db();
    const env: InstallationProvisionEnv = {
      CONFIG_DB: binding,
      CORELINK_INTERNAL_AUTH_KEY: AUTH,
    };
    const r = await handleInstallationProvision(
      req(
        { installation_id: "i-403b", tenant_id: "t1", repositories: [] },
        { auth: "wrong-key" },
      ),
      env,
    );
    expect(r.status).toBe(403);
  });

  it("unbound CORELINK_INTERNAL_AUTH_KEY → 403 (fail-closed)", async () => {
    const { binding } = db();
    const env: InstallationProvisionEnv = { CONFIG_DB: binding };
    const r = await handleInstallationProvision(
      req({ installation_id: "i-403c", tenant_id: "t1", repositories: [] }),
      env,
    );
    expect(r.status).toBe(403);
  });

  it("missing installation_id → 400", async () => {
    const { binding } = db();
    const env: InstallationProvisionEnv = {
      CONFIG_DB: binding,
      CORELINK_INTERNAL_AUTH_KEY: AUTH,
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
      CORELINK_INTERNAL_AUTH_KEY: AUTH,
    };
    const r = await handleInstallationProvision(
      req({ installation_id: "i-400b", repositories: [] }),
      env,
    );
    expect(r.status).toBe(400);
  });
});

describe("handleInstallationProvision — writes", () => {
  it("happy path → 200 and BOTH tables written with correct binds", async () => {
    const { binding, captured } = db();
    const env: InstallationProvisionEnv = {
      CONFIG_DB: binding,
      CORELINK_INTERNAL_AUTH_KEY: AUTH,
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
      CORELINK_INTERNAL_AUTH_KEY: AUTH,
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
      CORELINK_INTERNAL_AUTH_KEY: AUTH,
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
      CORELINK_INTERNAL_AUTH_KEY: AUTH,
    };
    const r = await handleInstallationProvision(
      req({ installation_id: "i-fault", tenant_id: "t1", repositories: ["a/b"] }),
      env,
    );
    expect(r.status).toBe(500);
  });

  it("CONFIG_DB unbound → 500 (cannot persist)", async () => {
    const env: InstallationProvisionEnv = { CORELINK_INTERNAL_AUTH_KEY: AUTH };
    const r = await handleInstallationProvision(
      req({ installation_id: "i-nodb", tenant_id: "t1", repositories: [] }),
      env,
    );
    expect(r.status).toBe(500);
  });
});
