import { describe, it, expect } from "vitest";
import {
  autoProvisionFromClerkEvent,
  tenantSlugFor,
  regionFromColo,
  verifySvixSignature,
  type ClerkUserCreatedEvent,
} from "../src/webhooks/clerk.js";

function fakeUser(
  over: Partial<ClerkUserCreatedEvent["data"]> = {},
): ClerkUserCreatedEvent {
  return {
    type: "user.created",
    data: {
      id: "user_2abc",
      email_addresses: [{ id: "em_1", email_address: "alice@acme.com" }],
      primary_email_address_id: "em_1",
      external_accounts: [
        { provider: "oauth_github", username: "alice-codes" },
      ],
      ...over,
    },
  };
}

describe("tenantSlugFor", () => {
  it("prefers GitHub username", () => {
    expect(tenantSlugFor(fakeUser().data)).toBe("alice-codes-default");
  });
  it("falls back to email local part when no GitHub", () => {
    const u = fakeUser({ external_accounts: [] }).data;
    expect(tenantSlugFor(u)).toBe("alice-default");
  });
  it("strips invalid characters", () => {
    const u = fakeUser({
      external_accounts: [
        { provider: "oauth_github", username: "Alice O'Connor!" },
      ],
    }).data;
    expect(tenantSlugFor(u)).toBe("alice-o-connor-default");
  });
  it("synthesises a name when seed is too short", () => {
    const u = fakeUser({
      external_accounts: [],
      email_addresses: [{ id: "em_1", email_address: "x@a.io" }],
      primary_email_address_id: "em_1",
    }).data;
    // 'x' is 1 char — falls back to `user-${id.slice(0,12)}-default`.
    expect(tenantSlugFor(u)).toBe("user-user_2abc-default");
  });
});

describe("regionFromColo", () => {
  it("lowercases CF colo codes", () => {
    expect(regionFromColo("ORD")).toBe("ord");
    expect(regionFromColo("GRU")).toBe("gru");
  });
  it("falls back to auto on missing colo", () => {
    expect(regionFromColo(null)).toBe("auto");
    expect(regionFromColo(undefined)).toBe("auto");
    expect(regionFromColo("")).toBe("auto");
  });
});

describe("autoProvisionFromClerkEvent", () => {
  it("provisions tenant + region + plan + PAT and emits the 4 funnel events", async () => {
    const calls: string[] = [];
    const emits: Array<{ name: string; tenantId: string | null; props: Record<string, unknown> }> = [];
    const api = {
      async createTenant(name: string, ownerUserId: string) {
        calls.push(`createTenant(${name},${ownerUserId})`);
        return { id: "t_1" };
      },
      async configureTenant(tenantId: string, region: string, plan: "free") {
        calls.push(`configureTenant(${tenantId},${region},${plan})`);
      },
      async issuePat(tenantId: string, scope: "cas:rw") {
        calls.push(`issuePat(${tenantId},${scope})`);
        return { id: "pat_1", plaintext: "ct_test_secret_xyz" };
      },
      async publishUserMetadata(userId: string, metadata: Record<string, unknown>) {
        calls.push(`publishUserMetadata(${userId},${JSON.stringify(metadata)})`);
      },
    };
    const analytics = {
      async emit(
        eventName: string,
        tenantId: string | null,
        _userId: string | null,
        properties: Record<string, unknown>,
      ): Promise<void> {
        emits.push({ name: eventName, tenantId, props: properties });
      },
    };

    const result = await autoProvisionFromClerkEvent({
      event: fakeUser(),
      colo: "ORD",
      svixId: "msg_xyz",
      api,
      analytics,
    });

    expect(result).toMatchObject({
      tenant_id: "t_1",
      region: "ord",
      plan: "free",
      pat_id: "pat_1",
      pat_plaintext: "ct_test_secret_xyz",
    });
    expect(calls).toEqual([
      "createTenant(alice-codes-default,user_2abc)",
      "configureTenant(t_1,ord,free)",
      "issuePat(t_1,cas:rw)",
      `publishUserMetadata(user_2abc,${JSON.stringify({
        tenant_id: "t_1",
        region: "ord",
        pat_plaintext: "ct_test_secret_xyz",
      })})`,
    ]);
    expect(emits.map((e) => e.name)).toEqual([
      "signup_completed",
      "tenant_created",
      "region_assigned",
      "pat_issued",
    ]);
    // svix_id propagates for idempotency.
    expect(emits.every((e) => e.props["svix_id"] === "msg_xyz")).toBe(true);
  });
});

describe("verifySvixSignature", () => {
  it("rejects when secret is not whsec_-prefixed", async () => {
    const ok = await verifySvixSignature({
      body: "{}",
      svixId: "msg_1",
      svixTimestamp: "1700000000",
      svixSignature: "v1,deadbeef",
      secret: "not_a_svix_secret",
    });
    expect(ok).toBe(false);
  });

  it("accepts a correctly signed payload", async () => {
    // Build a known-good signature using the same algorithm.
    const secretRaw = "supersecret-raw-bytes-with-good-entropy"; // 39 chars
    const secretB64 = btoa(secretRaw);
    const secret = `whsec_${secretB64}`;
    const body = JSON.stringify({ type: "user.created" });
    const svixId = "msg_test";
    const svixTimestamp = "1700000000";

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

    const ok = await verifySvixSignature({
      body,
      svixId,
      svixTimestamp,
      svixSignature: sig,
      secret,
    });
    expect(ok).toBe(true);
  });

  it("rejects tampered payloads", async () => {
    const secretRaw = "supersecret-raw-bytes-with-good-entropy";
    const secret = `whsec_${btoa(secretRaw)}`;
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
        new TextEncoder().encode("msg.1700000000.{}"),
      ),
    );
    const sig = `v1,${btoa(String.fromCharCode(...sigBytes))}`;
    const ok = await verifySvixSignature({
      body: '{"tampered":true}',
      svixId: "msg",
      svixTimestamp: "1700000000",
      svixSignature: sig,
      secret,
    });
    expect(ok).toBe(false);
  });
});
