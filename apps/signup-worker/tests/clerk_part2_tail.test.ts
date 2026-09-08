import { describe, it, expect, vi, afterEach } from "vitest";
import {
  autoProvisionFromClerkEvent,
  defaultApiClient,
  handleClerkWebhook,
  tenantSlugFor,
  orgMapKeyFor,
  regionFromColo,
  isProvisionedMacro,
  PROVISIONED_MACROS,
  verifySvixSignature,
  isValidClerkUserId,
  type AutoProvisionEnv,
  type ClerkUserCreatedEvent,
} from "../src/webhooks/clerk.js";

const VALID_AUTH_KEY = "internal-auth-key-0123456789abcdef";
const VALID_DEDICATED_MINT_KEY = "dedicated-mint-key-0123456789abcdef";

function fakeUser(
  over: Partial<ClerkUserCreatedEvent["data"]> = {},
): ClerkUserCreatedEvent {
  return {
    type: "user.created",
    data: {
      id: "user_2abc",
      email_addresses: [
        {
          id: "em_1",
          email_address: "alice@acme.com",
          verification: { status: "verified" },
        },
      ],
      primary_email_address_id: "em_1",
      external_accounts: [
        { provider: "oauth_github", username: "alice-codes" },
      ],
      ...over,
    },
  };
}
describe("defaultApiClient.createTenant (concurrent-duplicate webhook → no orphan PAT)", () => {
  /**
   * Models the migration-0056 UNIQUE(clerk_user_id) index: a `tenant` table
   * that already holds a row for this Clerk user (a Svix retry / a second
   * concurrent isolate won the insert first). `INSERT OR IGNORE` must no-op on
   * the clerk_user_id conflict, and the read-back `SELECT ... WHERE
   * clerk_user_id = ?1` must surface the PRE-EXISTING tenant_id — never the
   * freshly-generated random one that has no row.
   */
  function fakeDbWithExistingTenant(existing: {
    clerkUserId: string;
    tenantId: string;
  }) {
    // The single durable row keyed by clerk_user_id (the UNIQUE index).
    const rows = new Map<string, string>([
      [existing.clerkUserId, existing.tenantId],
    ]);
    let insertAttempted = false;
    let insertInserted = false;
    let selectClerkUserId: string | undefined;

    const db = {
      prepare(query: string) {
        let boundClerkUserId: string | undefined;
        const stmt = {
          _query: query,
          bind(...values: unknown[]) {
            if (query.includes("INSERT OR IGNORE INTO tenant")) {
              // backlog #29: createTenant now uses the parameterized insertTenant
              // helper, whose bind order is
              // (?1=tenant_id, ?2=primary_region, ?3=email_hash, ?4=clerk_user_id, ?5=...).
              boundClerkUserId = values[3] as string;
            } else if (
              query.includes("SELECT tenant_id FROM tenant WHERE clerk_user_id")
            ) {
              boundClerkUserId = values[0] as string;
            }
            return stmt;
          },
          async run() {
            if (query.includes("INSERT OR IGNORE INTO tenant")) {
              insertAttempted = true;
              const cuid = boundClerkUserId as string;
              if (rows.has(cuid)) {
                // UNIQUE(clerk_user_id) conflict → OR IGNORE silently skips.
                insertInserted = false;
              } else {
                // No conflict in this scenario, but model the happy path too.
                // (tenant_id isn't captured here — irrelevant to this test.)
                insertInserted = true;
              }
            }
            return { success: true };
          },
          async all() {
            return { results: [] };
          },
          async first() {
            if (query.includes("SELECT tenant_id FROM tenant WHERE clerk_user_id")) {
              selectClerkUserId = boundClerkUserId;
              const tid = rows.get(boundClerkUserId as string);
              return tid ? { tenant_id: tid } : null;
            }
            return null;
          },
        };
        return stmt;
      },
    };

    return {
      db,
      state: () => ({ insertAttempted, insertInserted, selectClerkUserId }),
    };
  }

  it("returns the EXISTING tenant_id (read-back wins), not the freshly-generated one", async () => {
    const ownerUserId = "user_2abc";
    const existingTenantId = "t_pre_existing_winner";
    const { db, state } = fakeDbWithExistingTenant({
      clerkUserId: ownerUserId,
      tenantId: existingTenantId,
    });

    const env = {
      CLERK_WEBHOOK_SECRET: "whsec_x",
      CORELINK_API_BASE: "https://api.example.test",
      CONFIG_DB: db,
    } as unknown as AutoProvisionEnv;

    const api = defaultApiClient(env);
    const { id } = await api.createTenant("alice-codes-default", ownerUserId, "enam");

    // The fix: adopt the durable winner. The OLD code returned its own random
    // crypto.randomUUID() here (an orphan id with no tenant row) — this assert
    // fails against that behavior.
    expect(id).toBe(existingTenantId);

    const s = state();
    // INSERT OR IGNORE was attempted but no-op'd on the UNIQUE conflict.
    expect(s.insertAttempted).toBe(true);
    expect(s.insertInserted).toBe(false);
    // The read-back queried by the owner's clerk_user_id.
    expect(s.selectClerkUserId).toBe(ownerUserId);
  });
});
describe("orgMapKeyFor (A1 — tenant_org_map key: org_id else sub)", () => {
  it("prefers the Clerk organization_id when the event carries one", () => {
    const u = fakeUser({ organization_id: "org_live123" }).data;
    expect(orgMapKeyFor(u)).toBe("org_live123");
  });
  it("accepts the alternate org_id field name", () => {
    const u = fakeUser({ org_id: "org_alt456" }).data;
    expect(orgMapKeyFor(u)).toBe("org_alt456");
  });
  it("falls back to the user sub (id) for an individual (no org)", () => {
    // The common self-serve pilot case — no org → the principal IS the user.
    expect(orgMapKeyFor(fakeUser().data)).toBe("user_2abc");
  });
  it("ignores an empty-string org and falls back to sub", () => {
    const u = fakeUser({ organization_id: "" }).data;
    expect(orgMapKeyFor(u)).toBe("user_2abc");
  });
});

describe("autoProvisionFromClerkEvent — A1 tenant_org_map write (LOAD-BEARING)", () => {
  const baseApi = {
    async createTenant() {
      return { id: "t_map" };
    },
    async configureTenant() {},
    async issuePat() {
      return { id: "pat_map", plaintext: "ct_map" };
    },
    async publishUserMetadata() {},
  };
  const noopAnalytics = { async emit() {} };

  it("writes the map row keyed on org_id when the event carries one", async () => {
    const seen: Array<{ key: string; tenant: string }> = [];
    await autoProvisionFromClerkEvent({
      event: fakeUser({ organization_id: "org_ACME" }),
      colo: "ORD",
      svixId: "msg_map1",
      api: baseApi,
      analytics: noopAnalytics,
      writeOrgMap: async (key, tenant) => {
        seen.push({ key, tenant });
      },
    });
    // Keyed on the org id, bound to the created tenant.
    expect(seen).toEqual([{ key: "org_ACME", tenant: "t_map" }]);
  });

  it("writes the map row keyed on sub (user id) when there is no org", async () => {
    const seen: Array<{ key: string; tenant: string }> = [];
    await autoProvisionFromClerkEvent({
      event: fakeUser(), // no org → fall back to sub
      colo: "ORD",
      svixId: "msg_map2",
      api: baseApi,
      analytics: noopAnalytics,
      writeOrgMap: async (key, tenant) => {
        seen.push({ key, tenant });
      },
    });
    expect(seen).toEqual([{ key: "user_2abc", tenant: "t_map" }]);
  });

  it("FAILS the provision when the map write throws (fail-closed → Svix retry)", async () => {
    // A tenant WITHOUT its map row is the exact org_not_mapped lockout A1 fixes,
    // so a map-write failure must NOT be swallowed — it must propagate so the
    // webhook returns non-2xx and Svix retries.
    await expect(
      autoProvisionFromClerkEvent({
        event: fakeUser({ organization_id: "org_boom" }),
        colo: "ORD",
        svixId: "msg_map_fail",
        api: baseApi,
        analytics: noopAnalytics,
        writeOrgMap: async () => {
          throw new Error("d1_org_map_write_failed");
        },
      }),
    ).rejects.toThrow(/d1_org_map_write_failed/);
  });
});
