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

describe("handleClerkWebhook (B4: fail-loud BEFORE any tenant write)", () => {
  // Build a Svix-signed user.created request with a CURRENT timestamp so the
  // handler's ±300s replay-freshness check passes against the real clock.
  async function signedUserCreated(secretRaw: string, body: string): Promise<Request> {
    const svixId = "msg_b4";
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

  it("acknowledges an unverified primary email without provisioning", async () => {
    const secretRaw = "supersecret-raw-bytes-with-good-entropy";
    const secret = `whsec_${btoa(secretRaw)}`;
    const body = JSON.stringify({
      type: "user.created",
      data: {
        id: "user_unverified",
        email_addresses: [
          {
            id: "em_unverified",
            email_address: "not-yet@acme.com",
            verification: { status: "unverified" },
          },
        ],
        primary_email_address_id: "em_unverified",
      },
    });
    const req = await signedUserCreated(secretRaw, body);
    let createTenantCalled = false;
    const apiFactory = (() => ({
      async createTenant() {
        createTenantCalled = true;
        return { id: "must-not-be-created" };
      },
      async configureTenant() {},
      async issuePat() {
        return { id: "must-not-be-issued", plaintext: "must-not-be-issued" };
      },
      async publishUserMetadata() {},
    })) as unknown as Parameters<typeof handleClerkWebhook>[2];

    const res = await handleClerkWebhook(
      req,
      { CLERK_WEBHOOK_SECRET: secret, CORELINK_API_BASE: "https://api.test" },
      apiFactory,
    );
    expect(res.status).toBe(202);
    expect(await res.json()).toEqual({ ok: false, reason: "email_not_verified" });
    expect(createTenantCalled).toBe(false);
  });

  it("rejects a missing or mismatched primary_email_address_id without array fallback", async () => {
    const secretRaw = "supersecret-raw-bytes-with-good-entropy";
    const secret = `whsec_${btoa(secretRaw)}`;
    const apiFactory = (() => ({
      async createTenant() {
        throw new Error("must not provision");
      },
      async configureTenant() {},
      async issuePat() {
        throw new Error("must not mint");
      },
      async publishUserMetadata() {},
    })) as unknown as Parameters<typeof handleClerkWebhook>[2];

    for (const primaryId of [undefined, "em_not_in_payload"]) {
      const data: Record<string, unknown> = {
        id: `user_primary_${primaryId ?? "missing"}`,
        email_addresses: [
          {
            id: "em_verified",
            email_address: "verified@acme.com",
            verification: { status: "verified" },
          },
        ],
      };
      if (primaryId !== undefined) data.primary_email_address_id = primaryId;
      const req = await signedUserCreated(
        secretRaw,
        JSON.stringify({ type: "user.created", data }),
      );
      const res = await handleClerkWebhook(
        req,
        { CLERK_WEBHOOK_SECRET: secret, CORELINK_API_BASE: "https://api.test" },
        apiFactory,
      );
      expect(res.status).toBe(202);
      expect(await res.json()).toEqual({ ok: false, reason: "email_not_verified" });
    }
  });

  it("rejects a known lifecycle event with a missing or malformed Clerk id before D1", async () => {
    const secretRaw = "supersecret-raw-bytes-with-good-entropy";
    const secret = `whsec_${btoa(secretRaw)}`;
    let prepared = false;
    const req = await signedUserCreated(
      secretRaw,
      JSON.stringify({ type: "user.created", data: { id: "../tenant", email_addresses: [] } }),
    );
    const env = {
      CLERK_WEBHOOK_SECRET: secret,
      CONFIG_DB: { prepare() { prepared = true; throw new Error("must not query"); } },
    } as unknown as AutoProvisionEnv;
    const apiFactory = (() => { throw new Error("must not provision"); }) as unknown as Parameters<typeof handleClerkWebhook>[2];
    const res = await handleClerkWebhook(req, env, apiFactory);
    expect(res.status).toBe(400);
    expect(await res.text()).toBe("invalid_clerk_user_id");
    expect(prepared).toBe(false);
  });

  it("returns 409 for a concurrent delivery holding the durable mint lease", async () => {
    const secretRaw = "supersecret-raw-bytes-with-good-entropy";
    const secret = `whsec_${btoa(secretRaw)}`;
    const req = await signedUserCreated(
      secretRaw,
      JSON.stringify({
        type: "user.created",
        data: {
          id: "user_locked",
          email_addresses: [
            { id: "em_locked", email_address: "locked@acme.com", verification: { status: "verified" } },
          ],
          primary_email_address_id: "em_locked",
        },
      }),
    );
    const env = {
      CLERK_WEBHOOK_SECRET: secret,
      CORELINK_INTERNAL_AUTH_KEY: VALID_AUTH_KEY,
      CONFIG_DB: {
        prepare(query: string) {
          const stmt = {
            bind() { return stmt; },
            async run() {
              return query.includes("clerk_provisioning_lock")
                ? { success: true, meta: { changes: 0 } }
                : { success: true };
            },
            async first() { return null; },
          };
          return stmt;
        },
      },
    } as unknown as AutoProvisionEnv;
    const apiFactory = (() => { throw new Error("must not mint"); }) as unknown as Parameters<typeof handleClerkWebhook>[2];
    const res = await handleClerkWebhook(req, env, apiFactory);
    expect(res.status).toBe(409);
    expect(await res.text()).toBe("provisioning_in_progress");
  });

  it("CORELINK_INTERNAL_AUTH_KEY absent → 500 and NEVER creates a tenant (no PAT-less orphan)", async () => {
    const secretRaw = "supersecret-raw-bytes-with-good-entropy";
    const secret = `whsec_${btoa(secretRaw)}`;
    const body = JSON.stringify({
      type: "user.created",
      data: {
        id: "user_b4",
        email_addresses: [
          { id: "em_b4", email_address: "b4@acme.com", verification: { status: "verified" } },
        ],
        primary_email_address_id: "em_b4",
      },
    });
    const req = await signedUserCreated(secretRaw, body);

    // CONFIG_DB present (declarative binding) with NO existing tenant — but the
    // internal auth key (a `wrangler secret`) is absent. This is the launch
    // landmine: the OLD code would createTenant, then issuePat throws, leaving a
    // PAT-less orphan that the idempotency check blocks from ever re-provisioning.
    const configDb = {
      prepare() {
        return {
          bind() {
            return this;
          },
          async first() {
            return null; // no existing tenant
          },
          async run() {
            return { success: true };
          },
          async all() {
            return { results: [] };
          },
        };
      },
    };
    const env = {
      CLERK_WEBHOOK_SECRET: secret,
      CONFIG_DB: configDb,
      // CORELINK_INTERNAL_AUTH_KEY intentionally UNSET.
    } as unknown as AutoProvisionEnv;

    // apiFactory whose createTenant flips a flag if it is (wrongly) reached.
    let createTenantCalled = false;
    const apiFactory = (() => ({
      async createTenant() {
        createTenantCalled = true;
        return { id: "t_should_not_exist" };
      },
      async configureTenant() {},
      async issuePat() {
        return { id: "x", plaintext: "y" };
      },
      async publishUserMetadata() {},
    })) as unknown as Parameters<typeof handleClerkWebhook>[2];

    const res = await handleClerkWebhook(req, env, apiFactory);

    expect(res.status).toBe(500);
    // The load-bearing assertion: we bailed BEFORE provisioning, so no
    // PAT-less orphan tenant was committed — Svix redelivery re-runs the whole
    // flow cleanly once the secret is set.
    expect(createTenantCalled).toBe(false);
  });

  it(
    "retry after a mid-provision failure (tenant row exists but NO live PAT) " +
      "RE-ISSUES the PAT instead of acking idempotent (finding #18)",
    async () => {
      const secretRaw = "supersecret-raw-bytes-with-good-entropy";
      const secret = `whsec_${btoa(secretRaw)}`;
      const body = JSON.stringify({
        type: "user.created",
        data: {
          id: "user_orphan",
          email_addresses: [
            {
              id: "em_1",
              email_address: "alice@acme.com",
              verification: { status: "verified" },
            },
          ],
          primary_email_address_id: "em_1",
          external_accounts: [{ provider: "oauth_github", username: "alice-codes" }],
        },
      });
      const req = await signedUserCreated(secretRaw, body);

      // CONFIG_DB models a PRIOR attempt that committed the tenant row but died
      // before minting the PAT: the tenant SELECT returns a row, the live-PAT
      // SELECT returns null. The OLD (tenant-existence-only) idempotency check
      // would short-circuit here and ack `idempotent: true`, leaving the user
      // permanently PAT-less. The fix must FALL THROUGH and re-provision.
      const configDb = {
        prepare(query: string) {
          return {
            bind() {
              return this;
            },
            async first() {
              if (query.includes("FROM tenant WHERE clerk_user_id")) {
                return { tenant_id: "t_orphan" }; // tenant row exists
              }
              if (query.includes("FROM pat WHERE tenant_id")) {
                return null; // NO live PAT — provisioning never completed
              }
              return null;
            },
            async run() {
              return { success: true };
            },
            async all() {
              return { results: [] };
            },
          };
        },
      };
      const env = {
        CLERK_WEBHOOK_SECRET: secret,
        CONFIG_DB: configDb,
        CORELINK_INTERNAL_AUTH_KEY: VALID_AUTH_KEY, // secret now present on retry
      } as unknown as AutoProvisionEnv;

      let issuePatCalled = false;
      let createTenantCalled = false;
      const apiFactory = (() => ({
        async createTenant() {
          createTenantCalled = true;
          return { id: "t_orphan" };
        },
        async configureTenant() {},
        async issuePat() {
          issuePatCalled = true;
          return { id: "pat_reissued", plaintext: "ct_reissued" };
        },
        async publishUserMetadata() {},
      })) as unknown as Parameters<typeof handleClerkWebhook>[2];

      const res = await handleClerkWebhook(req, env, apiFactory);
      const json = (await res.json()) as Record<string, unknown>;

      expect(res.status).toBe(200);
      // The load-bearing assertions: we did NOT short-circuit on tenant
      // existence — the PAT was re-issued and metadata re-published.
      expect(issuePatCalled).toBe(true);
      expect(createTenantCalled).toBe(true);
      expect(json["idempotent"]).toBeUndefined();
      expect(json["tenant_id"]).toBe("t_orphan");
    },
  );

  it(
    "fully-provisioned retry (tenant row + live PAT) short-circuits idempotent, " +
      "NO re-provision (finding #18 regression guard)",
    async () => {
      const secretRaw = "supersecret-raw-bytes-with-good-entropy";
      const secret = `whsec_${btoa(secretRaw)}`;
      const body = JSON.stringify({
        type: "user.created",
        data: {
          id: "user_done",
          email_addresses: [
            { id: "em_done", email_address: "done@acme.com", verification: { status: "verified" } },
          ],
          primary_email_address_id: "em_done",
        },
      });
      const req = await signedUserCreated(secretRaw, body);

      const configDb = {
        prepare(query: string) {
          return {
            bind() {
              return this;
            },
            async first() {
              if (query.includes("FROM tenant WHERE clerk_user_id")) {
                return { tenant_id: "t_done" };
              }
              if (query.includes("FROM pat WHERE tenant_id")) {
                return { pat_id: "pat_live" }; // live PAT exists → fully provisioned
              }
              return null;
            },
            async run() {
              return { success: true };
            },
            async all() {
              return { results: [] };
            },
          };
        },
      };
      const env = {
        CLERK_WEBHOOK_SECRET: secret,
        CONFIG_DB: configDb,
      CORELINK_INTERNAL_AUTH_KEY: VALID_AUTH_KEY,
      } as unknown as AutoProvisionEnv;

      let issuePatCalled = false;
      const apiFactory = (() => ({
        async createTenant() {
          return { id: "t_done" };
        },
        async configureTenant() {},
        async issuePat() {
          issuePatCalled = true;
          return { id: "x", plaintext: "y" };
        },
        async publishUserMetadata() {},
      })) as unknown as Parameters<typeof handleClerkWebhook>[2];

      const res = await handleClerkWebhook(req, env, apiFactory);
      const json = (await res.json()) as Record<string, unknown>;

      expect(res.status).toBe(200);
      expect(json["idempotent"]).toBe(true);
      expect(json["tenant_id"]).toBe("t_done");
      // No re-provision when already complete.
      expect(issuePatCalled).toBe(false);
    },
  );
});
