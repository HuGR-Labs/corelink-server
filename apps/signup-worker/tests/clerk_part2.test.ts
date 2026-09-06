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
      JSON.stringify({ type: "user.created", data: { id: "user_locked", email_addresses: [] } }),
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
