import { describe, it, expect, vi, afterEach } from "vitest";
import {
  autoProvisionFromClerkEvent,
  defaultApiClient,
  tenantSlugFor,
  regionFromColo,
  verifySvixSignature,
  type AutoProvisionEnv,
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
      async issuePat(tenantId: string, scope: "read-write") {
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
      "issuePat(t_1,read-write)",
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
    // Default happy path: metadata write succeeded.
    expect(result.metadata_published).toBe(true);
  });

  it("tolerates Clerk metadata 4xx (dead-letter; keeps tenant+PAT)", async () => {
    const api = {
      async createTenant(_name: string, _userId: string) {
        return { id: "t_dead" };
      },
      async configureTenant() {},
      async issuePat() {
        return { id: "pat_dead", plaintext: "ct_dead" };
      },
      async publishUserMetadata() {
        throw new Error("clerk_metadata_update_failed_404");
      },
    };
    const analytics = { async emit() {} };
    const result = await autoProvisionFromClerkEvent({
      event: fakeUser(),
      colo: "ORD",
      svixId: "msg_dl",
      api,
      analytics,
    });
    expect(result.tenant_id).toBe("t_dead");
    expect(result.pat_plaintext).toBe("ct_dead");
    expect(result.metadata_published).toBe(false);
  });

  it("re-throws Clerk metadata 5xx (transient; let Svix retry)", async () => {
    const api = {
      async createTenant(_name: string, _userId: string) {
        return { id: "t_retry" };
      },
      async configureTenant() {},
      async issuePat() {
        return { id: "pat_retry", plaintext: "ct_retry" };
      },
      async publishUserMetadata() {
        throw new Error("clerk_metadata_update_failed_503");
      },
    };
    const analytics = { async emit() {} };
    await expect(
      autoProvisionFromClerkEvent({
        event: fakeUser(),
        colo: "ORD",
        svixId: "msg_retry",
        api,
        analytics,
      }),
    ).rejects.toThrow(/_503$/);
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
      // Pin server clock to the signed timestamp so the freshness check passes.
      nowSeconds: Number(svixTimestamp),
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
      // Pin the clock fresh so this test isolates the *signature* mismatch,
      // not the timestamp-freshness rejection (covered separately below).
      nowSeconds: 1700000000,
    });
    expect(ok).toBe(false);
  });

  it("rejects a stale timestamp (replay) even with a valid signature", async () => {
    // Build a genuinely-valid signature, then verify with a clock skewed
    // beyond the 300s window — the freshness (anti-replay) check must reject it.
    const secretRaw = "supersecret-raw-bytes-with-good-entropy";
    const secret = `whsec_${btoa(secretRaw)}`;
    const body = JSON.stringify({ type: "user.created" });
    const svixId = "msg_replay";
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

    // 301s in the future relative to the signed timestamp → outside ±300s.
    const stale = await verifySvixSignature({
      body,
      svixId,
      svixTimestamp,
      svixSignature: sig,
      secret,
      nowSeconds: 1700000000 + 301,
    });
    expect(stale).toBe(false);

    // Same signature, clock just inside the window → accepted.
    const fresh = await verifySvixSignature({
      body,
      svixId,
      svixTimestamp,
      svixSignature: sig,
      secret,
      nowSeconds: 1700000000 + 299,
    });
    expect(fresh).toBe(true);
  });

  it("rejects a missing or non-numeric timestamp", async () => {
    const secretRaw = "supersecret-raw-bytes-with-good-entropy";
    const secret = `whsec_${btoa(secretRaw)}`;
    const body = JSON.stringify({ type: "user.created" });
    const svixId = "msg_badts";
    const key = await crypto.subtle.importKey(
      "raw",
      new TextEncoder().encode(secretRaw),
      { name: "HMAC", hash: "SHA-256" },
      false,
      ["sign"],
    );

    async function sigFor(ts: string): Promise<string> {
      const sb = new Uint8Array(
        await crypto.subtle.sign(
          "HMAC",
          key,
          new TextEncoder().encode(`${svixId}.${ts}.${body}`),
        ),
      );
      return `v1,${btoa(String.fromCharCode(...sb))}`;
    }

    // Empty timestamp — rejected.
    expect(
      await verifySvixSignature({
        body,
        svixId,
        svixTimestamp: "",
        svixSignature: await sigFor(""),
        secret,
        nowSeconds: 1700000000,
      }),
    ).toBe(false);

    // Non-numeric timestamp — rejected.
    expect(
      await verifySvixSignature({
        body,
        svixId,
        svixTimestamp: "not-a-number",
        svixSignature: await sigFor("not-a-number"),
        secret,
        nowSeconds: 1700000000,
      }),
    ).toBe(false);
  });
});

describe("defaultApiClient.issuePat (H3: honors scope, no privilege-by-default)", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("mints + persists the requested scope (read-write), never admin", async () => {
    // Capture the D1 bind args for the `pat` INSERT.
    let patBindArgs: unknown[] = [];
    const fakeDb = {
      prepare(query: string) {
        const stmt = {
          _query: query,
          bind(...values: unknown[]) {
            if (query.includes("INSERT OR IGNORE INTO pat")) {
              patBindArgs = values;
            }
            return stmt;
          },
          async run() {
            return { success: true };
          },
          async all() {
            return { results: [] };
          },
          async first() {
            return null;
          },
        };
        return stmt;
      },
    };

    // Capture the mint request body.
    let mintBody: Record<string, unknown> = {};
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockImplementation(async (req: Request | string | URL) => {
        const r = req as Request;
        mintBody = JSON.parse(await r.text()) as Record<string, unknown>;
        return new Response(
          JSON.stringify({
            token_plaintext: "corelink_pat_REAL",
            pat_id: "pat_real",
            token_id: "tok_real",
            expires_ms: 9_999_999_999_999,
            hash: "deadbeefhash",
          }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      });

    const env = {
      CLERK_WEBHOOK_SECRET: "whsec_x",
      CORELINK_API_BASE: "https://api.example.test",
      CORELINK_INTERNAL_AUTH_KEY: "internal-key",
      CONFIG_DB: fakeDb,
    } as unknown as AutoProvisionEnv;

    const api = defaultApiClient(env);
    const pat = await api.issuePat("t_1", "read-write");

    expect(pat).toMatchObject({ id: "pat_real", plaintext: "corelink_pat_REAL" });

    // H3: the mint request asks for read-write, not admin.
    expect(mintBody["scopes"]).toBe("read-write");
    expect(mintBody["scopes"]).not.toBe("admin");

    // H3: the D1 `scope` column is bound to read-write (4th positional bind, ?4),
    // not the old hardcoded 'admin' literal.
    expect(patBindArgs[3]).toBe("read-write");
    expect(patBindArgs).not.toContain("admin");

    expect(fetchSpy).toHaveBeenCalledTimes(1);
  });
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
              // VALUES (?1=tenant_id, ?2=email_hash, ?3=clerk_user_id, ?4=...).
              boundClerkUserId = values[2] as string;
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
    const { id } = await api.createTenant("alice-codes-default", ownerUserId);

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
