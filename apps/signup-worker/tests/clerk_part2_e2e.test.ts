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
describe("handleClerkWebhook — A1 tenant_org_map end-to-end", () => {
  async function signedUserCreated(secretRaw: string, body: string): Promise<Request> {
    const svixId = "msg_a1";
    const svixTimestamp = String(Math.floor(Date.now() / 1000));
    const key = await crypto.subtle.importKey(
      "raw",
      new TextEncoder().encode(secretRaw),
      { name: "HMAC", hash: "SHA-256" },
      false,
      ["sign"],
    );
    const sigBytes = new Uint8Array(
      await crypto.subtle.sign(
        "HMAC",
        key,
        new TextEncoder().encode(`${svixId}.${svixTimestamp}.${body}`),
      ),
    );
    const sig = `v1,${btoa(String.fromCharCode(...sigBytes))}`;
    return new Request("https://signup.test/webhooks/clerk", {
      method: "POST",
      headers: {
        "svix-id": svixId,
        "svix-timestamp": svixTimestamp,
        "svix-signature": sig,
        "content-type": "application/json",
      },
      body,
    });
  }

  /**
   * A fake CONFIG_DB that records every tenant_org_map INSERT and, optionally,
   * throws on the tenant_org_map write to model a D1 failure. No existing tenant
   * (fresh signup), so provisioning runs the full flow.
   */
  function fakeConfigDb(opts: { orgMapThrows?: boolean } = {}) {
    const orgMapInserts: Array<unknown[]> = [];
    let orgMapRunCount = 0;
    const db = {
      prepare(query: string) {
        let bound: unknown[] = [];
        const stmt = {
          bind(...values: unknown[]) {
            bound = values;
            return stmt;
          },
          async run() {
            if (query.includes("INSERT OR IGNORE INTO tenant_org_map")) {
              orgMapRunCount += 1;
              if (opts.orgMapThrows) {
                throw new Error("d1_org_map_write_failed");
              }
              orgMapInserts.push(bound);
            }
            return { success: true };
          },
          async all() {
            return { results: [] };
          },
          async first() {
            return null; // no existing tenant / no live PAT
          },
        };
        return stmt;
      },
    };
    return { db, orgMapInserts: () => orgMapInserts, orgMapRunCount: () => orgMapRunCount };
  }

  const apiFactory = (() => ({
    async createTenant() {
      return { id: "t_e2e" };
    },
    async configureTenant() {},
    async issuePat() {
      return { id: "pat_e2e", plaintext: "ct_e2e" };
    },
    async publishUserMetadata() {},
  })) as unknown as Parameters<typeof handleClerkWebhook>[2];

  it("writes BOTH the tenant AND a tenant_org_map row (keyed on sub for an individual)", async () => {
    const secretRaw = "supersecret-raw-bytes-with-good-entropy";
    const secret = `whsec_${btoa(secretRaw)}`;
    const body = JSON.stringify({
      type: "user.created",
      data: {
        id: "user_a1",
        email_addresses: [
          { id: "em_1", email_address: "a1@acme.com", verification: { status: "verified" } },
        ],
        primary_email_address_id: "em_1",
      },
    });
    const req = await signedUserCreated(secretRaw, body);
    const { db, orgMapInserts } = fakeConfigDb();
    const env = {
      CLERK_WEBHOOK_SECRET: secret,
      CONFIG_DB: db,
      CORELINK_INTERNAL_AUTH_KEY: VALID_AUTH_KEY,
    } as unknown as AutoProvisionEnv;

    const res = await handleClerkWebhook(req, env, apiFactory);
    expect(res.status).toBe(200);

    const inserts = orgMapInserts();
    expect(inserts.length).toBe(1);
    // (?1=clerk_org_id, ?2=tenant_id, ?3=created_at_ms) — no org → sub fallback.
    expect(inserts[0]?.[0]).toBe("user_a1");
    expect(inserts[0]?.[1]).toBe("t_e2e");
    expect(typeof inserts[0]?.[2]).toBe("number");
  });

  it("writes the tenant_org_map row keyed on org_id when the event carries one", async () => {
    const secretRaw = "supersecret-raw-bytes-with-good-entropy";
    const secret = `whsec_${btoa(secretRaw)}`;
    const body = JSON.stringify({
      type: "user.created",
      data: {
        id: "user_a1org",
        organization_id: "org_A1",
        email_addresses: [
          { id: "em_1", email_address: "a1org@acme.com", verification: { status: "verified" } },
        ],
        primary_email_address_id: "em_1",
      },
    });
    const req = await signedUserCreated(secretRaw, body);
    const { db, orgMapInserts } = fakeConfigDb();
    const env = {
      CLERK_WEBHOOK_SECRET: secret,
      CONFIG_DB: db,
      CORELINK_INTERNAL_AUTH_KEY: VALID_AUTH_KEY,
    } as unknown as AutoProvisionEnv;

    const res = await handleClerkWebhook(req, env, apiFactory);
    expect(res.status).toBe(200);
    const inserts = orgMapInserts();
    expect(inserts.length).toBe(1);
    expect(inserts[0]?.[0]).toBe("org_A1"); // org id wins over sub
    expect(inserts[0]?.[1]).toBe("t_e2e");
  });

  it("a map-write FAILURE fails the webhook (500 → Svix retries)", async () => {
    const secretRaw = "supersecret-raw-bytes-with-good-entropy";
    const secret = `whsec_${btoa(secretRaw)}`;
    const body = JSON.stringify({
      type: "user.created",
      data: {
        id: "user_a1fail",
        email_addresses: [
          { id: "em_1", email_address: "a1fail@acme.com", verification: { status: "verified" } },
        ],
        primary_email_address_id: "em_1",
      },
    });
    const req = await signedUserCreated(secretRaw, body);
    const { db, orgMapRunCount } = fakeConfigDb({ orgMapThrows: true });
    const env = {
      CLERK_WEBHOOK_SECRET: secret,
      CONFIG_DB: db,
        CORELINK_INTERNAL_AUTH_KEY: VALID_AUTH_KEY,
    } as unknown as AutoProvisionEnv;

    const res = await handleClerkWebhook(req, env, apiFactory);
    // Non-2xx so Svix retries — a tenant without its map row is the lockout.
    expect(res.status).toBe(500);
    // The map write was attempted (and threw), not silently skipped.
    expect(orgMapRunCount()).toBe(1);
  });
});
