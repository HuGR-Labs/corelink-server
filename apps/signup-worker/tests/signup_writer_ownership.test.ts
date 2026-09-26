import { describe, expect, it } from "vitest";
import { defaultApiClient } from "../src/webhooks/clerk.js";
import { writeInstallationProvision } from "../src/webhooks/github_provision.js";
import {
  signupOwnershipContext,
  signupArtifactHandle,
  writeSignupArtifactBatch,
} from "../src/signup_writer_ownership.js";

const KEY = "01234567890123456789012345678901";
const SHA = "a".repeat(40);
const REQUEST_ID = "b".repeat(32);
const NOW = 1_000_000;
const DOMAIN = "corelink/staging-ownership-envelope/v1\0";

function bytesToHex(bytes: ArrayBuffer): string {
  return Array.from(new Uint8Array(bytes), (byte) => byte.toString(16).padStart(2, "0")).join("");
}

async function ownershipEnvelope(requestId = REQUEST_ID): Promise<string> {
  const payload = `v1.123.signup.staging.${SHA}.${NOW}.${NOW + 60_000}.${requestId}`;
  const key = await crypto.subtle.importKey(
    "raw",
    new TextEncoder().encode(KEY),
    { name: "HMAC", hash: "SHA-256" },
    false,
    ["sign"],
  );
  const tag = await crypto.subtle.sign(
    "HMAC",
    key,
    new TextEncoder().encode(DOMAIN + payload),
  );
  return `${payload}.${bytesToHex(tag)}`;
}

async function requestWithOwnership(requestId = REQUEST_ID): Promise<Request> {
  return new Request("https://signup-worker.test/write", {
    headers: {
      "x-corelink-staging-ownership": await ownershipEnvelope(requestId),
      "x-corelink-staging-request-id": requestId,
    },
  });
}

type Captured = { sql: string; values: unknown[] };

function fakeDb(): {
  db: D1Database;
  batches: Captured[][];
  domainRows: Set<string>;
  synchronizeOwnershipPreflightReads(): void;
} {
  const batches: Captured[][] = [];
  const registered = new Set<string>();
  const domainRows = new Set<string>();
  const tenantByClerkId = new Map<string, string>();
  let transactionTail: Promise<void> = Promise.resolve();
  let ownershipReadCount = 0;
  let releaseOwnershipReads: (() => void) | undefined;
  const ownershipReadsBarrier = new Promise<void>((resolve) => {
    releaseOwnershipReads = resolve;
  });
  let synchronizeOwnershipReads = false;
  const prepare = (sql: string) => {
    const captured: Captured = { sql, values: [] };
    const statement = {
      sql,
      values: [] as unknown[],
      bind(...values: unknown[]) {
        captured.values = values;
        this.values = values;
        return this;
      },
      async run() {
        return { success: true, meta: { changes: 1 } };
      },
      async first<T>() {
        if (sql.includes("FROM staging_load_test_resources")) {
          if (synchronizeOwnershipReads) {
            ownershipReadCount += 1;
            if (ownershipReadCount === 2) releaseOwnershipReads?.();
            await ownershipReadsBarrier;
          }
          return (registered.has(String(captured.values[0])) ? { present: 1 } : null) as T | null;
        }
        if (sql.includes("FROM tenant WHERE clerk_user_id")) {
          const tenantId = tenantByClerkId.get(String(captured.values[0])) ?? null;
          return (tenantId === null ? null : { tenant_id: tenantId }) as T | null;
        }
        return null;
      },
    };
    return statement;
  };
  const db = {
    prepare,
    async batch(statements: D1PreparedStatement[]) {
      const execute = async () => {
        const capturedBatch = statements.map((statement) => {
          const candidate = statement as unknown as { sql?: string; values?: unknown[] };
          return { sql: candidate.sql ?? "", values: candidate.values ?? [] };
        });
        batches.push(capturedBatch);

        const registeredBefore = new Set(registered);
        const domainRowsBefore = new Set(domainRows);
        const tenantByClerkIdBefore = new Map(tenantByClerkId);
        let lastChanges = 0;
        try {
          for (const statement of statements) {
            const candidate = statement as unknown as { sql?: string; values?: unknown[] };
            const sql = candidate.sql ?? "";
            const values = candidate.values ?? [];
            if (sql.startsWith("INSERT INTO staging_load_test_resources ") && !sql.includes("SELECT run_id")) {
              const handle = String(values[4]);
              if (registered.has(handle)) {
                lastChanges = 0;
              } else {
                registered.add(handle);
                lastChanges = 1;
              }
            } else if (sql.includes("AND changes() = 0")) {
              if (lastChanges === 0) {
                throw new Error("UNIQUE constraint failed: staging_load_test_resources.opaque_handle");
              }
              lastChanges = 0;
            } else if (sql.startsWith("INSERT OR IGNORE INTO tenant ")) {
              const clerkId = String(values[3]);
              if (tenantByClerkId.has(clerkId)) {
                lastChanges = 0;
              } else {
                const tenantId = String(values[0]);
                tenantByClerkId.set(clerkId, tenantId);
                domainRows.add(`tenant:${clerkId}`);
                lastChanges = 1;
              }
            } else if (sql.startsWith("INSERT OR IGNORE INTO ")) {
              const key = `${sql}:${values.map(String).join(":")}`;
              if (domainRows.has(key)) {
                lastChanges = 0;
              } else {
                domainRows.add(key);
                lastChanges = 1;
              }
            }
          }
        } catch (error) {
          registered.clear();
          for (const value of registeredBefore) registered.add(value);
          domainRows.clear();
          for (const value of domainRowsBefore) domainRows.add(value);
          tenantByClerkId.clear();
          for (const [key, value] of tenantByClerkIdBefore) tenantByClerkId.set(key, value);
          throw error;
        }
        return statements.map(() => ({ success: true, meta: { changes: 1 } }));
      };
      const pending = transactionTail.then(execute);
      transactionTail = pending.then(() => undefined, () => undefined);
      return pending;
    },
  } as unknown as D1Database;
  return {
    db,
    batches,
    domainRows,
    synchronizeOwnershipPreflightReads() {
      synchronizeOwnershipReads = true;
    },
  };
}

describe("signup writer ownership", () => {
  it("accepts absence and rejects unbound, expired, or mismatched claims", async () => {
    const ordinary = new Request("https://signup-worker.test/write");
    await expect(signupOwnershipContext(ordinary, "prod", undefined, NOW)).resolves.toBeNull();

    const valid = await requestWithOwnership();
    await expect(signupOwnershipContext(valid, "staging", KEY, NOW + 1)).resolves.toMatchObject({
      runId: "123",
      scenario: "signup",
      targetDeploymentSha: SHA,
      requestId: REQUEST_ID,
    });
    await expect(signupOwnershipContext(valid, "prod", KEY, NOW + 1)).rejects.toThrow(
      "staging ownership envelope is invalid",
    );
    const mismatchedRequestId = "c".repeat(32);
    const mismatched = new Request("https://signup-worker.test/write", {
      headers: {
        "x-corelink-staging-ownership": await ownershipEnvelope(mismatchedRequestId),
        "x-corelink-staging-request-id": REQUEST_ID,
      },
    });
    await expect(signupOwnershipContext(mismatched, "staging", KEY, NOW + 1)).rejects.toThrow(
      "staging ownership envelope is invalid",
    );
  });

  it("batches Clerk tenant creation and signup registration together", async () => {
    const { db, batches } = fakeDb();
    const context = await signupOwnershipContext(await requestWithOwnership(), "staging", KEY, NOW + 1);
    if (context === null) throw new Error("verified context missing");
    const api = defaultApiClient(
      {
        CONFIG_DB: db,
        CORELINK_API_BASE: "https://api.test",
        CLERK_WEBHOOK_SECRET: "test-secret",
      },
      context,
    );
    await api.createTenant("test-tenant", "user_test", "enam");

    expect(batches).toHaveLength(1);
    expect(batches[0]).toHaveLength(3);
    expect(batches[0]?.[0]?.sql).toContain("INSERT OR IGNORE INTO tenant");
    expect(batches[0]?.[1]?.sql).toContain("INSERT INTO staging_load_test_resources");
    expect(batches[0]?.[1]?.values[2]).toBe("signup_artifact");
    expect(batches[0]?.[1]?.values[4]).toBe(await signupArtifactHandle("clerk-tenant", "user_test"));
    expect(batches[0]?.[2]?.sql).toContain("AND changes() = 0");
    expect(JSON.stringify(batches[0]?.[1]?.values)).not.toContain("user_test");
    expect(JSON.stringify(batches[0]?.[1]?.values)).not.toContain(KEY);
  });

  it("batches GitHub installation provisioning and registration, then rejects replay before a second batch", async () => {
    const { db, batches } = fakeDb();
    const context = await signupOwnershipContext(await requestWithOwnership(), "staging", KEY, NOW + 1);
    if (context === null) throw new Error("verified context missing");
    await writeInstallationProvision(db, {
      installationId: "731",
      tenantId: "tenant-opaque-id",
      repos: ["org/private-repo"],
      nowMs: NOW + 2,
      ownershipContext: context,
    });

    expect(batches).toHaveLength(1);
    expect(batches[0]).toHaveLength(4);
    expect(batches[0]?.[0]?.sql).toContain("tenant_gh_installation_map");
    expect(batches[0]?.[1]?.sql).toContain("runner_repo_allowlist");
    expect(batches[0]?.[2]?.sql).toContain("staging_load_test_resources");
    expect(batches[0]?.[2]?.values[4]).toBe(await signupArtifactHandle("github-installation", "731"));
    expect(batches[0]?.[3]?.sql).toContain("AND changes() = 0");
    expect(JSON.stringify(batches[0]?.[2]?.values)).not.toContain("org/private-repo");

    await expect(
      writeInstallationProvision(db, {
        installationId: "731",
        tenantId: "tenant-opaque-id",
        repos: ["org/private-repo"],
        nowMs: NOW + 3,
        ownershipContext: context,
      }),
    ).rejects.toThrow("already registered");
    expect(batches).toHaveLength(1);
  });

  it("rolls back the entire batch when concurrent valid requests claim one installation", async () => {
    const { db, batches, domainRows, synchronizeOwnershipPreflightReads } = fakeDb();
    const firstContext = await signupOwnershipContext(await requestWithOwnership(), "staging", KEY, NOW + 1);
    const secondRequestId = "c".repeat(32);
    const secondContext = await signupOwnershipContext(
      await requestWithOwnership(secondRequestId),
      "staging",
      KEY,
      NOW + 1,
    );
    if (firstContext === null || secondContext === null) throw new Error("verified context missing");
    synchronizeOwnershipPreflightReads();

    const write = (ownershipContext: NonNullable<typeof firstContext>) =>
      writeInstallationProvision(db, {
        installationId: "731",
        tenantId: "tenant-opaque-id",
        repos: ["org/private-repo"],
        nowMs: NOW + 2,
        ownershipContext,
      });
    const results = await Promise.allSettled([write(firstContext), write(secondContext)]);

    expect(results.filter((result) => result.status === "fulfilled")).toHaveLength(1);
    expect(results.filter((result) => result.status === "rejected")).toHaveLength(1);
    expect(batches).toHaveLength(2);
    expect(domainRows.size).toBe(2);
    expect(batches[1]?.[2]?.sql).toContain("ON CONFLICT (run_id, scenario, resource_class, receipt_ref) DO NOTHING");
    expect(batches[1]?.[3]?.sql).toContain("AND changes() = 0");
    expect(batches[0]?.[2]?.values[4]).toBe(batches[1]?.[2]?.values[4]);
    expect(batches[0]?.[2]?.values[4]).not.toContain(REQUEST_ID);
    expect(batches[0]?.[2]?.values[4]).not.toContain(secondRequestId);
  });

  it("fails closed before domain writes when atomic D1 batches are unavailable", async () => {
    const context = await signupOwnershipContext(await requestWithOwnership(), "staging", KEY, NOW + 1);
    if (context === null) throw new Error("verified context missing");
    let writes = 0;
    const db = {
      prepare: () => ({
        bind: () => ({ first: async () => null }),
      }),
    } as unknown as D1Database;
    const domainStatement = {
      async run() {
        writes += 1;
        return { success: true };
      },
    } as unknown as D1PreparedStatement;
    await expect(
      writeSignupArtifactBatch(db, context, "opaque:handle", [domainStatement], NOW + 2),
    ).rejects.toThrow("D1 batch unavailable");
    expect(writes).toBe(0);
  });
});
