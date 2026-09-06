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
  it("provisions EXACTLY {wnam, enam, weur, apac}", () => {
    expect([...PROVISIONED_MACROS].sort()).toEqual(["apac", "enam", "weur", "wnam"]);
    expect(isProvisionedMacro("wnam")).toBe(true);
    expect(isProvisionedMacro("enam")).toBe(true);
    expect(isProvisionedMacro("weur")).toBe(true);
    // apac provisioned by WP4 (APAC-located bucket corelink-cas-apac, Tokyo/nrt).
    expect(isProvisionedMacro("apac")).toBe(true);
  });

  it("does NOT provision sam — the LGPD cross-border trap (PROD_SAM = US bucket)", () => {
    // sam is a valid, geo-derivable macro (regionFromColo("GRU") === "sam") but
    // must be REJECTED at signup — Cloudflare has no SAM region (platform limit),
    // so PROD_SAM = US bucket and the tenant's data would mis-land in US R2 under
    // a false label.
    expect(regionFromColo("GRU")).toBe("sam");
    expect(isProvisionedMacro("sam")).toBe(false);
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
  it("maps Asia-Pacific + Oceania colos to apac (provisioned by WP4 → Tokyo/nrt)", () => {
    expect(regionFromColo("NRT")).toBe("apac");
    expect(regionFromColo("SYD")).toBe("apac");
    expect(regionFromColo("SIN")).toBe("apac");
  });
  it("maps African colos to afr so signup rejects before any tenant write", () => {
    expect(regionFromColo("JNB")).toBe("afr");
    expect(regionFromColo("CPT")).toBe("afr");
    expect(regionFromColo("LOS")).toBe("afr");
    expect(regionFromColo("NBO")).toBe("afr");
    expect(isProvisionedMacro(regionFromColo("JNB"))).toBe(false);
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
  it("REJECTS an African macro before any tenant write (residency fail-closed)", async () => {
    let createTenantCalled = false;
    const api = {
      async createTenant() {
        createTenantCalled = true;
        return { id: "must-not-exist" };
      },
      async configureTenant() {},
      async issuePat() { return { id: "must-not-exist", plaintext: "never" }; },
      async publishUserMetadata() {},
    };
    await expect(
      autoProvisionFromClerkEvent({
        event: fakeUser(), colo: "JNB", svixId: "msg_afr", api,
        analytics: { async emit() {} },
      }),
    ).rejects.toThrow(/not provisioned/);
    expect(createTenantCalled).toBe(false);
  });

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

  it("REJECTS an unprovisioned macro region (sam) before any tenant write (backlog #29)", async () => {
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
    // GRU → sam (valid macro, NOT provisioned — no CF SAM region) → must reject.
    await expect(
      autoProvisionFromClerkEvent({
        event: fakeUser(),
        colo: "GRU",
        svixId: "msg_sam",
        api,
        analytics,
      }),
    ).rejects.toThrow(/not provisioned/);
    // No tenant row was written — the throw is BEFORE createTenant.
    expect(createTenantCalled).toBe(false);
  });

  it("PROVISIONS an apac tenant (NRT → apac) — WP4 opened the region", async () => {
    let createdRegion: string | undefined;
    const api = {
      async createTenant(_n: string, _u: string, r: string) {
        createdRegion = r;
        return { id: "t_apac" };
      },
      async configureTenant() {},
      async issuePat() {
        return { id: "pat_apac", plaintext: "ct_apac" };
      },
      async publishUserMetadata() {},
    };
    const analytics = { async emit() {} };
    // NRT → apac is now PROVISIONED (WP4: corelink-cas-apac, Tokyo) → tenant created.
    const result = await autoProvisionFromClerkEvent({
      event: fakeUser(),
      colo: "NRT",
      svixId: "msg_apac_ok",
      api,
      analytics,
    });
    expect(createdRegion).toBe("apac");
    expect(result.tenant_id).toBe("t_apac");
  });

  it("seeds the free-tier entitlement row-family AFTER configureTenant and BEFORE issuePat (fully-provisioned tenant; converge with githugr_provision)", async () => {
    // The launch-critical gap this closes: the Clerk-signup path previously
    // seeded only tenant+org_map+pat, leaving tier_selections/tenant_quota/
    // runners_entitlement empty → billing `inactive` + empty quota/runner gates.
    // seedEntitlements must fire (with the created tenant id) and be ordered so
    // the tenant+live-PAT idempotency guard implies the rows exist: after
    // configureTenant, before issuePat.
    const calls: string[] = [];
    const seededFor: string[] = [];
    const api = {
      async createTenant(_name: string, _userId: string, _region: string) {
        calls.push("createTenant");
        return { id: "t_seed" };
      },
      async configureTenant() {
        calls.push("configureTenant");
      },
      async issuePat() {
        calls.push("issuePat");
        return { id: "pat_seed", plaintext: "ct_seed" };
      },
      async publishUserMetadata() {
        calls.push("publishUserMetadata");
      },
    };
    const analytics = { async emit() {} };
    const result = await autoProvisionFromClerkEvent({
      event: fakeUser(),
      colo: "ORD",
      svixId: "msg_seed",
      api,
      analytics,
      async seedEntitlements(tenantId: string) {
        calls.push("seedEntitlements");
        seededFor.push(tenantId);
      },
    });
    expect(result.tenant_id).toBe("t_seed");
    // Fired exactly once, keyed to the created tenant.
    expect(seededFor).toEqual(["t_seed"]);
    // Ordering: after configureTenant, before issuePat.
    expect(calls).toEqual([
      "createTenant",
      "configureTenant",
      "seedEntitlements",
      "issuePat",
      "publishUserMetadata",
    ]);
  });

  it("propagates a seedEntitlements failure (fail-CLOSED → webhook 500 → Svix retry) BEFORE the PAT is minted", async () => {
    let issuePatCalled = false;
    const api = {
      async createTenant() {
        return { id: "t_fail" };
      },
      async configureTenant() {},
      async issuePat() {
        issuePatCalled = true;
        return { id: "pat_fail", plaintext: "ct_fail" };
      },
      async publishUserMetadata() {},
    };
    const analytics = { async emit() {} };
    await expect(
      autoProvisionFromClerkEvent({
        event: fakeUser(),
        colo: "ORD",
        svixId: "msg_seed_fail",
        api,
        analytics,
        async seedEntitlements() {
          throw new Error("d1_seed_entitlements_failed");
        },
      }),
    ).rejects.toThrow(/d1_seed_entitlements_failed/);
    // The throw is BEFORE issuePat, so no orphan PAT is minted; on Svix retry the
    // tenant+live-PAT idempotency guard re-drives the whole (idempotent) flow.
    expect(issuePatCalled).toBe(false);
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

describe("Clerk user id wire contract", () => {
  it("accepts opaque provider ids only with the user_ prefix", () => {
    expect(isValidClerkUserId("user_2abc")).toBe(true);
    expect(isValidClerkUserId("user_live-opaque_123")).toBe(true);
  });
  it("rejects missing, non-user, whitespace, and oversized ids", () => {
    expect(isValidClerkUserId(undefined)).toBe(false);
    expect(isValidClerkUserId("org_2abc")).toBe(false);
    expect(isValidClerkUserId("user_ bad")).toBe(false);
    expect(isValidClerkUserId(`user_${"x".repeat(129)}`)).toBe(false);
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
      CORELINK_INTERNAL_AUTH_KEY: VALID_AUTH_KEY,
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

  // Minimal D1 fake that satisfies the pat INSERT + reads the issuePat path needs.
  function mintFakeDb() {
    const stmt = {
      bind() {
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
    return { prepare: () => stmt };
  }

  // Capture the x-corelink-internal-auth header the mint request presents.
  function spyMintHeader() {
    let mintAuth: string | null = null;
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockImplementation(async (req: Request | string | URL) => {
        mintAuth = (req as Request).headers.get("x-corelink-internal-auth");
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
    return { fetchSpy, getMintAuth: () => mintAuth };
  }

  it("presents the DEDICATED CORELINK_PAT_MINT_AUTH_KEY on x-corelink-internal-auth (dedicated wins over shared)", async () => {
    const { getMintAuth } = spyMintHeader();
    const env = {
      CLERK_WEBHOOK_SECRET: "whsec_x",
      CORELINK_API_BASE: "https://api.example.test",
      CORELINK_PAT_MINT_AUTH_KEY: VALID_DEDICATED_MINT_KEY,
      CORELINK_INTERNAL_AUTH_KEY: VALID_AUTH_KEY,
      CONFIG_DB: mintFakeDb(),
    } as unknown as AutoProvisionEnv;

    await defaultApiClient(env).issuePat("t_1", "read-write");
    expect(getMintAuth()).toBe("dedicated-mint-key");
  });

  it("falls back to the shared CORELINK_INTERNAL_AUTH_KEY when the dedicated key is unset (additive)", async () => {
    const { getMintAuth } = spyMintHeader();
    const env = {
      CLERK_WEBHOOK_SECRET: "whsec_x",
      CORELINK_API_BASE: "https://api.example.test",
      // CORELINK_PAT_MINT_AUTH_KEY intentionally UNSET.
      CORELINK_INTERNAL_AUTH_KEY: VALID_AUTH_KEY,
      CONFIG_DB: mintFakeDb(),
    } as unknown as AutoProvisionEnv;

    await defaultApiClient(env).issuePat("t_1", "read-write");
    expect(getMintAuth()).toBe("shared-key");
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

  it("throws (fail-closed) when the resolved mint key is below the 32-char floor", async () => {
    const fetchSpy = vi.spyOn(globalThis, "fetch");
    const env = {
      CLERK_WEBHOOK_SECRET: "whsec_x",
      CORELINK_API_BASE: "https://api.example.test",
      CORELINK_PAT_MINT_AUTH_KEY: "short-dedicated-key",
      CORELINK_INTERNAL_AUTH_KEY: "shared-key-that-is-long-enough-0123456789",
    } as unknown as AutoProvisionEnv;
    await expect(defaultApiClient(env).issuePat("t_1", "read-write")).rejects.toThrow(
      /CORELINK_INTERNAL_AUTH_KEY/,
    );
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});
// B-126 M3 split population: clerk_part2.test.ts
