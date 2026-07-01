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

describe("PROVISIONED_MACROS (backlog #29 / H1 — canonical provisionable set)", () => {
  // This set is the SINGLE SOURCE OF TRUTH shared with worker/src/region-map.ts
  // and crates/corelink-container/src/storage/region_map.rs. The 3-way cross-file
  // diff lives in worker/tests/region-map.test.ts; this asserts the clerk copy in
  // isolation so a drift here fails the signup-worker suite directly.
  it("provisions EXACTLY {wnam, enam, weur}", () => {
    expect([...PROVISIONED_MACROS].sort()).toEqual(["enam", "weur", "wnam"]);
    expect(isProvisionedMacro("wnam")).toBe(true);
    expect(isProvisionedMacro("enam")).toBe(true);
    expect(isProvisionedMacro("weur")).toBe(true);
  });

  it("does NOT provision sam — the LGPD cross-border trap (PROD_SAM = US bucket)", () => {
    // sam is a valid, geo-derivable macro (regionFromColo("GRU") === "sam") but
    // must be REJECTED at signup until PROD_SAM has a real SAM-jurisdiction
    // bucket, else the tenant's data mis-lands in US R2 under a false label.
    expect(regionFromColo("GRU")).toBe("sam");
    expect(isProvisionedMacro("sam")).toBe(false);
    expect(isProvisionedMacro("apac")).toBe(false);
    expect(isProvisionedMacro("afr")).toBe(false);
  });
});

describe("regionFromColo (backlog #29 — colo → MACRO residency region)", () => {
  it("maps EU colos to weur (closes the Schrems II leak)", () => {
    expect(regionFromColo("LHR")).toBe("weur");
    expect(regionFromColo("FRA")).toBe("weur");
    expect(regionFromColo("CDG")).toBe("weur");
    expect(regionFromColo("AMS")).toBe("weur");
  });
  it("maps South-American colos to sam", () => {
    expect(regionFromColo("GRU")).toBe("sam");
    expect(regionFromColo("EZE")).toBe("sam");
  });
  it("maps Asia-Pacific colos to apac (valid macro, unprovisioned)", () => {
    expect(regionFromColo("NRT")).toBe("apac");
    expect(regionFromColo("SYD")).toBe("apac");
    expect(regionFromColo("SIN")).toBe("apac");
  });
  it("maps North-American + unknown + absent colos to enam (US-east default)", () => {
    expect(regionFromColo("ORD")).toBe("enam");
    expect(regionFromColo("IAD")).toBe("enam");
    expect(regionFromColo("ZZZ")).toBe("enam");
    expect(regionFromColo(null)).toBe("enam");
    expect(regionFromColo(undefined)).toBe("enam");
    expect(regionFromColo("")).toBe("enam");
  });
});

describe("autoProvisionFromClerkEvent", () => {
  it("provisions tenant + region + plan + PAT and emits the 4 funnel events", async () => {
    const calls: string[] = [];
    const emits: Array<{ name: string; tenantId: string | null; props: Record<string, unknown> }> = [];
    // Capture the two metadata objects so we can assert the secret split:
    // tenant_id/region → public, pat_plaintext → private (NEVER public).
    let capturedPublic: Record<string, unknown> | null = null;
    let capturedPrivate: Record<string, unknown> | null = null;
    const api = {
      async createTenant(name: string, ownerUserId: string, region: string) {
        calls.push(`createTenant(${name},${ownerUserId},${region})`);
        return { id: "t_1" };
      },
      async configureTenant(tenantId: string, region: string, plan: "free") {
        calls.push(`configureTenant(${tenantId},${region},${plan})`);
      },
      async issuePat(tenantId: string, scope: "read-write") {
        calls.push(`issuePat(${tenantId},${scope})`);
        return { id: "pat_1", plaintext: "ct_test_secret_xyz" };
      },
      async publishUserMetadata(
        userId: string,
        publicMetadata: Record<string, unknown>,
        privateMetadata: Record<string, unknown>,
      ) {
        calls.push(`publishUserMetadata(${userId})`);
        capturedPublic = publicMetadata;
        capturedPrivate = privateMetadata;
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
      region: "enam",
      plan: "free",
      pat_id: "pat_1",
      pat_plaintext: "ct_test_secret_xyz",
    });
    expect(calls).toEqual([
      "createTenant(alice-codes-default,user_2abc,enam)",
      "configureTenant(t_1,enam,free)",
      "issuePat(t_1,read-write)",
      "publishUserMetadata(user_2abc)",
    ]);

    // ── CTRL-CRED-001: PAT plaintext goes to PRIVATE metadata ONLY ──────────
    // public_metadata carries ONLY the legit session claims, no secret.
    expect(capturedPublic).toEqual({ tenant_id: "t_1", region: "enam" });
    expect(capturedPublic).not.toHaveProperty("pat_plaintext");
    // private_metadata carries the one-time secret + its reveal-age clock.
    expect(capturedPrivate).not.toBeNull();
    const priv = capturedPrivate as unknown as Record<string, unknown>;
    expect(priv).toMatchObject({ pat_plaintext: "ct_test_secret_xyz" });
    expect(typeof priv["pat_revealed_at"]).toBe("number");
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

  it("REJECTS an unprovisioned macro region (apac) before any tenant write (backlog #29)", async () => {
    let createTenantCalled = false;
    const api = {
      async createTenant(_n: string, _u: string, _r: string) {
        createTenantCalled = true;
        return { id: "t_never" };
      },
      async configureTenant() {},
      async issuePat() {
        return { id: "pat_never", plaintext: "ct_never" };
      },
      async publishUserMetadata() {},
    };
    const analytics = { async emit() {} };
    // NRT → apac (valid macro, NOT provisioned in Phase 1) → must reject.
    await expect(
      autoProvisionFromClerkEvent({
        event: fakeUser(),
        colo: "NRT",
        svixId: "msg_apac",
        api,
        analytics,
      }),
    ).rejects.toThrow(/not provisioned/);
    // No tenant row was written — the throw is BEFORE createTenant.
    expect(createTenantCalled).toBe(false);
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

  it("throws (fail-loud) when CORELINK_INTERNAL_AUTH_KEY is absent — never returns a stub PAT", async () => {
    const fetchSpy = vi.spyOn(globalThis, "fetch");
    const env = {
      CLERK_WEBHOOK_SECRET: "whsec_x",
      CORELINK_API_BASE: "https://api.example.test",
      // CORELINK_INTERNAL_AUTH_KEY intentionally UNSET.
    } as unknown as AutoProvisionEnv;

    const api = defaultApiClient(env);

    // Must reject (so the webhook handler returns 500 and Svix retries),
    // NOT return a fake "corelink_pat_DEVSTUB", and NOT hit the mint endpoint.
    await expect(api.issuePat("t_1", "read-write")).rejects.toThrow(
      /CORELINK_INTERNAL_AUTH_KEY/,
    );
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});

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

  it("CORELINK_INTERNAL_AUTH_KEY absent → 500 and NEVER creates a tenant (no PAT-less orphan)", async () => {
    const secretRaw = "supersecret-raw-bytes-with-good-entropy";
    const secret = `whsec_${btoa(secretRaw)}`;
    const body = JSON.stringify({
      type: "user.created",
      data: { id: "user_b4", email_addresses: [] },
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
          email_addresses: [{ id: "em_1", email_address: "alice@acme.com" }],
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
        CORELINK_INTERNAL_AUTH_KEY: "internal-key", // secret now present on retry
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
        data: { id: "user_done", email_addresses: [] },
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
        CORELINK_INTERNAL_AUTH_KEY: "internal-key",
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
        email_addresses: [{ id: "em_1", email_address: "a1@acme.com" }],
        primary_email_address_id: "em_1",
      },
    });
    const req = await signedUserCreated(secretRaw, body);
    const { db, orgMapInserts } = fakeConfigDb();
    const env = {
      CLERK_WEBHOOK_SECRET: secret,
      CONFIG_DB: db,
      CORELINK_INTERNAL_AUTH_KEY: "internal-key",
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
        email_addresses: [{ id: "em_1", email_address: "a1org@acme.com" }],
        primary_email_address_id: "em_1",
      },
    });
    const req = await signedUserCreated(secretRaw, body);
    const { db, orgMapInserts } = fakeConfigDb();
    const env = {
      CLERK_WEBHOOK_SECRET: secret,
      CONFIG_DB: db,
      CORELINK_INTERNAL_AUTH_KEY: "internal-key",
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
        email_addresses: [{ id: "em_1", email_address: "a1fail@acme.com" }],
        primary_email_address_id: "em_1",
      },
    });
    const req = await signedUserCreated(secretRaw, body);
    const { db, orgMapRunCount } = fakeConfigDb({ orgMapThrows: true });
    const env = {
      CLERK_WEBHOOK_SECRET: secret,
      CONFIG_DB: db,
      CORELINK_INTERNAL_AUTH_KEY: "internal-key",
    } as unknown as AutoProvisionEnv;

    const res = await handleClerkWebhook(req, env, apiFactory);
    // Non-2xx so Svix retries — a tenant without its map row is the lockout.
    expect(res.status).toBe(500);
    // The map write was attempted (and threw), not silently skipped.
    expect(orgMapRunCount()).toBe(1);
  });
});
