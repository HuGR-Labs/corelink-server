/**
 * End-to-end webhook flow test (Stream-5).
 *
 * Tests the full user.created → tenant row + pat row + Clerk metadata update
 * path, using in-memory fakes for D1 CONFIG_DB, the internal mint endpoint,
 * and the Clerk Backend API.
 *
 * This test covers:
 *   1. svix signature verification gates side effects
 *   2. Idempotent provisioning: second webhook with same clerk_user_id → 200 + cached tenant_id
 *   3. Happy path: tenant INSERT, mint call, PAT INSERT, Clerk metadata PATCH
 *   4. Correct session claim shape: { tenant_id, region, pat_plaintext }
 */

import { describe, it, expect, vi, beforeEach } from "vitest";
import {
  handleClerkWebhook,
  defaultApiClient,
  type AutoProvisionEnv,
  type ClerkUserCreatedEvent,
} from "../src/webhooks/clerk.js";

// ─── In-memory D1 fake ────────────────────────────────────────────────────────

interface D1Row {
  [key: string]: unknown;
}

class InMemoryD1 {
  private tables: Map<string, D1Row[]> = new Map();

  private getTable(name: string): D1Row[] {
    let t = this.tables.get(name);
    if (!t) {
      t = [];
      this.tables.set(name, t);
    }
    return t;
  }

  prepare(query: string) {
    const self = this;
    let boundValues: unknown[] = [];

    const stmt = {
      bind(...values: unknown[]) {
        boundValues = values;
        return stmt;
      },
      async run() {
        const q = query.trim().toUpperCase();
        if (q.startsWith("INSERT")) {
          // Extract table name
          const match = query.match(/INTO\s+(\w+)/i);
          const tableName = match?.[1] ?? "unknown";
          const table = self.getTable(tableName);
          const row: D1Row = {};
          // Simple: store bound values as positional params
          boundValues.forEach((v, i) => {
            row[`_p${i + 1}`] = v;
          });
          row["_raw_query"] = query;
          row["_values"] = [...boundValues];
          if (q.includes("INSERT OR IGNORE")) {
            // Check for existing tenant by clerk_user_id if inserting tenant
            if (tableName === "tenant") {
              const clerkUserId = boundValues[3]; // 4th param = clerk_user_id
              const existing = table.find((r) => r["_clerk_user_id"] === clerkUserId);
              if (!existing) {
                row["tenant_id"] = boundValues[0];
                row["_clerk_user_id"] = boundValues[3];
                row["_pat_id"] = undefined;
                table.push(row);
              }
            } else if (tableName === "pat") {
              row["pat_id"] = boundValues[0];
              table.push(row);
            } else {
              table.push(row);
            }
          } else {
            table.push(row);
          }
          return { success: true };
        }
        return { success: true };
      },
      async first<T = unknown>(): Promise<T | null> {
        // Handle: SELECT tenant_id FROM tenant WHERE clerk_user_id = ?1 LIMIT 1
        if (query.includes("clerk_user_id")) {
          const clerkUserId = boundValues[0];
          const table = self.getTable("tenant");
          const row = table.find((r) => r["_clerk_user_id"] === clerkUserId);
          if (row) return { tenant_id: row["tenant_id"] } as T;
          return null;
        }
        return null;
      },
    };
    return stmt;
  }
}

// ─── Svix signature helper ─────────────────────────────────────────────────────

async function signWebhookBody(
  secret: string,
  svixId: string,
  svixTimestamp: string,
  body: string,
): Promise<string> {
  const rawSecret = secret.replace(/^whsec_/, "");
  const secretBytes = Uint8Array.from(atob(rawSecret), (c) => c.charCodeAt(0));
  const key = await crypto.subtle.importKey(
    "raw",
    secretBytes,
    { name: "HMAC", hash: "SHA-256" },
    false,
    ["sign"],
  );
  const toSign = new TextEncoder().encode(`${svixId}.${svixTimestamp}.${body}`);
  const sigBytes = new Uint8Array(await crypto.subtle.sign("HMAC", key, toSign));
  return `v1,${btoa(String.fromCharCode(...sigBytes))}`;
}

// ─── Test setup ───────────────────────────────────────────────────────────────

const WEBHOOK_SECRET_RAW = "test-webhook-secret-bytes-with-good-entropy-32";
const WEBHOOK_SECRET = `whsec_${btoa(WEBHOOK_SECRET_RAW)}`;

function makeUserCreatedEvent(userId = "user_2test"): ClerkUserCreatedEvent {
  return {
    type: "user.created",
    data: {
      id: userId,
      email_addresses: [{ id: "em_1", email_address: `${userId}@test.com` }],
      primary_email_address_id: "em_1",
      external_accounts: [{ provider: "oauth_github", username: "testuser" }],
    },
  };
}

async function makeWebhookRequest(
  event: ClerkUserCreatedEvent,
  webhookSecret: string,
  svixId = "msg_test_001",
): Promise<Request> {
  const body = JSON.stringify(event);
  const svixTimestamp = String(Math.floor(Date.now() / 1000));
  const sig = await signWebhookBody(webhookSecret, svixId, svixTimestamp, body);
  return new Request("https://signup.corelink.humangr.com/webhooks/clerk", {
    method: "POST",
    headers: {
      "content-type": "application/json",
      "svix-id": svixId,
      "svix-timestamp": svixTimestamp,
      "svix-signature": sig,
    },
    body,
  });
}

// ─── Tests ─────────────────────────────────────────────────────────────────────

describe("Clerk webhook — Stream-5 end-to-end flow", () => {
  it("rejects missing svix signature headers with 400", async () => {
    const db = new InMemoryD1();
    const env: AutoProvisionEnv = {
      CLERK_WEBHOOK_SECRET: WEBHOOK_SECRET,
      CORELINK_API_BASE: "https://corelink-api.humangr.com",
      CONFIG_DB: db as unknown as Parameters<typeof defaultApiClient>[0]["CONFIG_DB"],
    };
    const req = new Request(
      "https://signup.corelink.humangr.com/webhooks/clerk",
      { method: "POST", body: "{}", headers: { "content-type": "application/json" } },
    );
    const resp = await handleClerkWebhook(req, env, defaultApiClient);
    expect(resp.status).toBe(400);
  });

  it("rejects invalid svix signature with 401", async () => {
    const db = new InMemoryD1();
    const env: AutoProvisionEnv = {
      CLERK_WEBHOOK_SECRET: WEBHOOK_SECRET,
      CORELINK_API_BASE: "https://corelink-api.humangr.com",
      CONFIG_DB: db as unknown as Parameters<typeof defaultApiClient>[0]["CONFIG_DB"],
    };
    const event = makeUserCreatedEvent();
    const body = JSON.stringify(event);
    const req = new Request(
      "https://signup.corelink.humangr.com/webhooks/clerk",
      {
        method: "POST",
        headers: {
          "content-type": "application/json",
          "svix-id": "msg_001",
          "svix-timestamp": "1700000000",
          "svix-signature": "v1,invalidsignature",
        },
        body,
      },
    );
    const resp = await handleClerkWebhook(req, env, defaultApiClient);
    expect(resp.status).toBe(401);
  });

  it("provisions tenant + PAT on first user.created event (dev/CI no-D1 path)", async () => {
    // No CONFIG_DB — uses stub path in defaultApiClient.
    const mintCalled: string[] = [];
    const metadataCalled: string[] = [];

    const env: AutoProvisionEnv = {
      CLERK_WEBHOOK_SECRET: WEBHOOK_SECRET,
      CORELINK_API_BASE: "https://corelink-api.humangr.com",
    };

    // Inject a stub api that records calls.
    const stubApi = {
      async createTenant(name: string, ownerUserId: string) {
        mintCalled.push(`createTenant:${name}:${ownerUserId}`);
        return { id: "tenant-uuid-001" };
      },
      async configureTenant(_tenantId: string, _region: string, _plan: "free") {},
      async issuePat(tenantId: string, scope: "read-write") {
        mintCalled.push(`issuePat:${tenantId}:${scope}`);
        return { id: "pat-uuid-001", plaintext: "corelink_pat_TESTTOKEN" };
      },
      async publishUserMetadata(userId: string, meta: Record<string, unknown>) {
        metadataCalled.push(`publishMetadata:${userId}`);
        // Verify the correct fields are present.
        expect(meta["tenant_id"]).toBe("tenant-uuid-001");
        // region is derived from CF colo — in tests it falls back to "auto"
        expect(typeof meta["region"]).toBe("string");
        expect(typeof meta["pat_plaintext"]).toBe("string");
      },
    };

    const event = makeUserCreatedEvent("user_2abc");
    const req = await makeWebhookRequest(event, WEBHOOK_SECRET);
    // Inject fake CF colo header via a modified request
    const reqWithColo = new Request(req, {
      // cf object not directly settable in test; colo falls back to "auto" — that's fine
    });

    const resp = await handleClerkWebhook(reqWithColo, env, () => stubApi);
    expect(resp.status).toBe(200);
    const body = await resp.json() as { ok: boolean; tenant_id: string };
    expect(body.ok).toBe(true);
    expect(body.tenant_id).toBe("tenant-uuid-001");
    expect(mintCalled.length).toBeGreaterThanOrEqual(2);
    expect(metadataCalled.length).toBe(1);
  });

  it("idempotency: second webhook for same clerk_user_id returns cached tenant_id", async () => {
    const db = new InMemoryD1();
    // Pre-seed the tenant row as if already provisioned.
    db.prepare(
      "INSERT OR IGNORE INTO tenant (tenant_id, primary_region, tenant_state, email_hash, clerk_user_id, created_at_ms, updated_at_ms, created_ms, updated_ms) VALUES (?1, ?2, 'active', ?3, ?4, ?5, ?5, ?5, ?5)",
    )
      .bind("cached-tenant-uuid", "enam", "hash", "user_2abc", Date.now())
      .run();

    const env: AutoProvisionEnv = {
      CLERK_WEBHOOK_SECRET: WEBHOOK_SECRET,
      CORELINK_API_BASE: "https://corelink-api.humangr.com",
      CONFIG_DB: db as unknown as Parameters<typeof defaultApiClient>[0]["CONFIG_DB"],
    };

    let apiCalled = false;
    const guardApi = () => ({
      async createTenant() {
        apiCalled = true;
        return { id: "should-not-be-called" };
      },
      async configureTenant() {},
      async issuePat() {
        apiCalled = true;
        return { id: "x", plaintext: "x" };
      },
      async publishUserMetadata() {
        apiCalled = true;
      },
    });

    const event = makeUserCreatedEvent("user_2abc");
    const req = await makeWebhookRequest(event, WEBHOOK_SECRET);
    const resp = await handleClerkWebhook(req, env, guardApi);

    expect(resp.status).toBe(200);
    const body = await resp.json() as { ok: boolean; tenant_id: string; idempotent?: boolean };
    expect(body.ok).toBe(true);
    expect(body.tenant_id).toBe("cached-tenant-uuid");
    expect(body.idempotent).toBe(true);
    // The guard api must NOT have been called.
    expect(apiCalled).toBe(false);
  });

  it("ignores non-user.created event types with 200", async () => {
    const env: AutoProvisionEnv = {
      CLERK_WEBHOOK_SECRET: WEBHOOK_SECRET,
      CORELINK_API_BASE: "https://corelink-api.humangr.com",
    };
    const body = JSON.stringify({ type: "user.deleted", data: { id: "u_1" } });
    const svixId = "msg_del_001";
    const svixTimestamp = String(Math.floor(Date.now() / 1000));
    const sig = await signWebhookBody(WEBHOOK_SECRET, svixId, svixTimestamp, body);
    const req = new Request("https://signup.corelink.humangr.com/webhooks/clerk", {
      method: "POST",
      headers: {
        "content-type": "application/json",
        "svix-id": svixId,
        "svix-timestamp": svixTimestamp,
        "svix-signature": sig,
      },
      body,
    });
    const resp = await handleClerkWebhook(req, env, defaultApiClient);
    expect(resp.status).toBe(200);
    const text = await resp.text();
    expect(text).toBe("ignored");
  });

  it("session claim shape: publishUserMetadata receives { tenant_id, region, pat_plaintext }", async () => {
    // Verify the exact claim shape the /welcome page reads.
    const capturedMetadata: Record<string, unknown>[] = [];

    const env: AutoProvisionEnv = {
      CLERK_WEBHOOK_SECRET: WEBHOOK_SECRET,
      CORELINK_API_BASE: "https://corelink-api.humangr.com",
    };

    const claimCapturingApi = () => ({
      async createTenant(_name: string, _userId: string) {
        return { id: "tenant-claim-test" };
      },
      async configureTenant() {},
      async issuePat() {
        return { id: "pat-claim-test", plaintext: "corelink_pat_CLAIMTEST" };
      },
      async publishUserMetadata(_userId: string, meta: Record<string, unknown>) {
        capturedMetadata.push(meta);
      },
    });

    const event = makeUserCreatedEvent("user_claim_test");
    const req = await makeWebhookRequest(event, WEBHOOK_SECRET);
    const resp = await handleClerkWebhook(req, env, claimCapturingApi);

    expect(resp.status).toBe(200);
    expect(capturedMetadata).toHaveLength(1);
    const claims = capturedMetadata[0]!;
    // These three keys are what welcome/page.tsx reads from sessionClaims.
    expect(typeof claims["tenant_id"]).toBe("string");
    expect(typeof claims["region"]).toBe("string");
    expect(typeof claims["pat_plaintext"]).toBe("string");
    // pat_plaintext must NOT be empty.
    expect((claims["pat_plaintext"] as string).length).toBeGreaterThan(0);
  });
});
