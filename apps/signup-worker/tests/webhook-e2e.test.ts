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
 *   4. Metadata split: public_metadata { tenant_id, region } (session claims,
 *      no secret) + private_metadata { pat_plaintext, pat_revealed_at }
 *      (backend-only) per CTRL-CRED-001.
 */

import { describe, it, expect, vi, beforeEach } from "vitest";
import {
  handleClerkWebhook,
  defaultApiClient,
  buildErasureQueueMessage,
  deterministicDsrId,
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

  /** Number of rows recorded for a table (0 if the table was never written). */
  rowCount(name: string): number {
    return this.tables.get(name)?.length ?? 0;
  }

  /** The first recorded row for a table, or undefined. */
  firstRow(name: string): D1Row | undefined {
    return this.tables.get(name)?.[0];
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
              // Store named columns so the WP-1 pat-liveness SELECT (tenant_id +
              // expires_ms) can match. Seed INSERTs use the column order
              // (pat_id, tenant_id, expires_ms).
              row["pat_id"] = boundValues[0];
              row["tenant_id"] = boundValues[1];
              row["expires_ms"] = boundValues[2];
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
        // Handle WP-1: SELECT pat_id FROM pat WHERE tenant_id = ?1 AND expires_ms > ?2
        if (query.includes("FROM pat") && query.includes("expires_ms")) {
          const tenantId = boundValues[0];
          const minExpires = Number(boundValues[1]);
          const row = self
            .getTable("pat")
            .find((r) => r["tenant_id"] === tenantId && Number(r["expires_ms"]) > minExpires);
          return row ? ({ pat_id: row["pat_id"] } as T) : null;
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
      // #195 fail-loud: provisioning 500s BEFORE any tenant write when the
      // PAT-mint key is absent — the e2e env must carry it like prod does.
      CORELINK_INTERNAL_AUTH_KEY: "test-internal-auth-key-e2e",
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
      async publishUserMetadata(
        userId: string,
        publicMeta: Record<string, unknown>,
        privateMeta: Record<string, unknown>,
      ) {
        metadataCalled.push(`publishMetadata:${userId}`);
        // public_metadata: legit session claims only — NO secret.
        expect(publicMeta["tenant_id"]).toBe("tenant-uuid-001");
        // region is derived from CF colo — in tests it falls back to "auto"
        expect(typeof publicMeta["region"]).toBe("string");
        expect(publicMeta).not.toHaveProperty("pat_plaintext");
        // private_metadata (backend-only) carries the one-time secret.
        expect(typeof privateMeta["pat_plaintext"]).toBe("string");
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

  it("provisions to enam even when Svix delivers from a European PoP (cf.colo is Svix's, not the user's)", async () => {
    // REGRESSION: a Clerk webhook is delivered by Svix (server-to-server), so
    // request.cf.colo is Svix's sender PoP, NOT the end-user's. Before the fix,
    // a European Svix PoP (e.g. "FRA") was geo-mapped to `weur` and then REJECTED
    // by PROVISIONED_MACROS (US-only at launch) with a 422 — a legitimate signup
    // lost to Svix routing. The handler now ignores the webhook colo and defaults
    // to the launch-served region (enam), so the signup MUST succeed.
    const regions: string[] = [];
    const env: AutoProvisionEnv = {
      CLERK_WEBHOOK_SECRET: WEBHOOK_SECRET,
      CORELINK_API_BASE: "https://corelink-api.humangr.com",
      CORELINK_INTERNAL_AUTH_KEY: "test-internal-auth-key-e2e",
    };
    const stubApi = () => ({
      async createTenant(_name: string, _ownerUserId: string, region: string) {
        regions.push(region);
        return { id: "eu-tenant-uuid" };
      },
      async configureTenant(_t: string, region: string, _p: "free") {
        regions.push(region);
      },
      async issuePat() {
        return { id: "pat-eu", plaintext: "corelink_pat_EUTOKEN" };
      },
      async publishUserMetadata() {},
    });

    const event = makeUserCreatedEvent("user_eu_signup");
    const req = await makeWebhookRequest(event, WEBHOOK_SECRET);
    // Attach a European Svix sender PoP — pre-fix this forced weur → 422.
    Object.defineProperty(req, "cf", { value: { colo: "FRA" }, configurable: true });

    const resp = await handleClerkWebhook(req, env, stubApi);

    expect(resp.status).toBe(200); // NOT 422 region_not_provisioned
    // Every region the handler assigned is the launch default enam — never weur.
    expect(regions.length).toBeGreaterThan(0);
    for (const r of regions) {
      expect(r).toBe("enam");
    }
  });

  it("provisions the 4-row-family on first user.created: tenant, tenant_org_map, pat, tier_selections('free','active'), tenant_quota — and grants NO runners_entitlement", async () => {
    // LAUNCH-CRITICAL GAP: the Clerk-signup path previously seeded only
    // tenant+tenant_org_map+pat, leaving tier_selections/tenant_quota EMPTY — so a
    // Clerk-signup tenant reported billing `inactive` and read an empty quota gate
    // (degraded dashboard), while a githugr-LOGIN tenant
    // (worker/src/lib/githugr_provision.ts) was seeded. This drives the REAL
    // defaultApiClient end-to-end (mint mocked) and asserts each row-family lands.
    //
    // ⚠️ `runners_entitlement` is the DELIBERATE exception and this test pins it
    // NEGATIVELY (2026-08-02). It used to be seeded here as ('free', 1) "so the
    // runner cap gate reads a real entitlement" — but that row IS the entitlement
    // the mint gate checks, so seeding it granted paid Runners capacity to every
    // free signup, silently reverting owner-ratified Option B (Runners = separate
    // PAID axis) and migration 0070's "empty table = no cap = reject" contract.
    // A free signup that then installs the public GitHub App (which seeds the
    // installation map + repo allowlist with NO entitlement check) would clear all
    // four mint gates and spawn boxes on our Cloudflare account.
    //
    // So: if this assertion ever fails because a row appeared, the leak is back.
    // Do NOT "fix" it by relaxing the expectation.
    const db = new InMemoryD1();
    const env: AutoProvisionEnv = {
      CLERK_WEBHOOK_SECRET: WEBHOOK_SECRET,
      CORELINK_API_BASE: "https://corelink-api.humangr.com",
      CONFIG_DB: db as unknown as Parameters<typeof defaultApiClient>[0]["CONFIG_DB"],
      CORELINK_INTERNAL_AUTH_KEY: "test-internal-auth-key-e2e",
      // No CLERK_SECRET_KEY → publishUserMetadata is a no-op (dev path).
    };

    // Mock the container /_internal/pat/mint call so issuePat lands a pat row.
    const fetchMock = vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(
        JSON.stringify({
          token_plaintext: "corelink_pat_FULLROWS",
          pat_id: "pat-full-1",
          token_id: "tok-full-1",
          expires_ms: Date.now() + 365 * 24 * 60 * 60 * 1000,
          hash: "deadbeefhash",
        }),
        { status: 200, headers: { "content-type": "application/json" } },
      ),
    );

    try {
      const event = makeUserCreatedEvent("user_full_rows");
      const req = await makeWebhookRequest(event, WEBHOOK_SECRET, "msg_full_rows_001");
      const resp = await handleClerkWebhook(req, env, defaultApiClient);
      expect(resp.status).toBe(200);

      // ── The FOUR row-families a signup gets (the convergence assertion) ────
      expect(db.rowCount("tenant")).toBeGreaterThanOrEqual(1);
      expect(db.rowCount("tenant_org_map")).toBe(1);
      expect(db.rowCount("pat")).toBe(1);
      // The two rows the gap left empty:
      expect(db.rowCount("tier_selections")).toBe(1);
      expect(db.rowCount("tenant_quota")).toBe(1);

      // ── The NEGATIVE pin: a signup buys CACHE, never RUNNERS ───────────────
      // Absence of the row IS the zero (migration 0070). The schema's
      // `CHECK(max_concurrency > 0)` makes "entitled to zero" inexpressible, so
      // there is no such thing as a harmless placeholder row here — any row is a
      // grant of real, billable compute capacity.
      expect(db.rowCount("runners_entitlement")).toBe(0);

      // tier_selections is seeded free/ACTIVE (billing reads `active`, not the
      // degraded `inactive`), matching githugr_provision's shape.
      const tierRaw = String(db.firstRow("tier_selections")?.["_raw_query"] ?? "");
      expect(tierRaw).toContain("INTO tier_selections");
      expect(tierRaw).toContain("'free', 'active'");
    } finally {
      fetchMock.mockRestore();
    }
  });

  it("idempotency: second webhook for same clerk_user_id returns cached tenant_id", async () => {
    const db = new InMemoryD1();
    // Pre-seed the tenant row as if already provisioned.
    db.prepare(
      "INSERT OR IGNORE INTO tenant (tenant_id, primary_region, tenant_state, email_hash, clerk_user_id, created_at_ms, updated_at_ms, created_ms, updated_ms) VALUES (?1, ?2, 'active', ?3, ?4, ?5, ?5, ?5, ?5)",
    )
      .bind("cached-tenant-uuid", "enam", "hash", "user_2abc", Date.now())
      .run();
    // WP-1 (#269): idempotency requires tenant AND a LIVE pat — a tenant row
    // alone is not "complete provisioning". Seed a non-expired pat so this models
    // a fully-provisioned tenant (column order: pat_id, tenant_id, expires_ms).
    db.prepare("INSERT OR IGNORE INTO pat (pat_id, tenant_id, expires_ms) VALUES (?1, ?2, ?3)")
      .bind("cached-pat-id", "cached-tenant-uuid", Date.now() + 3_600_000)
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

  it("ignores unhandled event types with 200", async () => {
    const env: AutoProvisionEnv = {
      CLERK_WEBHOOK_SECRET: WEBHOOK_SECRET,
      CORELINK_API_BASE: "https://corelink-api.humangr.com",
    };
    // session.created is a real Clerk event the signup-worker does not handle.
    const body = JSON.stringify({ type: "session.created", data: { id: "sess_1" } });
    const svixId = "msg_sess_001";
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

  // ── Clerk user.deleted → GDPR right-to-erasure enqueue (WI-S11-008) ──────────
  describe("user.deleted → DSR erasure enqueue", () => {
    type ConfigDb = Parameters<typeof defaultApiClient>[0]["CONFIG_DB"];

    async function deletedReq(userId: string): Promise<Request> {
      const body = JSON.stringify({ type: "user.deleted", data: { id: userId, deleted: true } });
      const svixId = `msg_del_${userId}`;
      const ts = String(Math.floor(Date.now() / 1000));
      const sig = await signWebhookBody(WEBHOOK_SECRET, svixId, ts, body);
      return new Request("https://signup.corelink.humangr.com/webhooks/clerk", {
        method: "POST",
        headers: {
          "content-type": "application/json",
          "svix-id": svixId,
          "svix-timestamp": ts,
          "svix-signature": sig,
        },
        body,
      });
    }

    async function seedTenant(db: InMemoryD1, tenantId: string, clerkUserId: string): Promise<void> {
      await db
        .prepare(
          "INSERT OR IGNORE INTO tenant (tenant_id, primary_region, tenant_state, email_hash, clerk_user_id, created_at_ms, updated_at_ms, created_ms, updated_ms) VALUES (?1, ?2, 'active', ?3, ?4, ?5, ?5, ?5, ?5)",
        )
        .bind(tenantId, "enam", "hash", clerkUserId, 1)
        .run();
    }

    function envWith(db: InMemoryD1, sent: unknown[] | null): AutoProvisionEnv {
      return {
        CLERK_WEBHOOK_SECRET: WEBHOOK_SECRET,
        CORELINK_API_BASE: "https://corelink-api.humangr.com",
        CONFIG_DB: db as unknown as ConfigDb,
        ...(sent
          ? { DSR_QUEUE: { send: async (m: unknown) => { sent.push(m); } } }
          : {}),
        ERASURE_SALT_KEY: "test-erasure-salt-key",
      };
    }

    it("enqueues a dsr.queued.v1 erasure request when a tenant exists", async () => {
      const db = new InMemoryD1();
      await seedTenant(db, "tenant-uuid-1", "user_del_1");
      const sent: unknown[] = [];
      const resp = await handleClerkWebhook(await deletedReq("user_del_1"), envWith(db, sent), defaultApiClient);
      expect(resp.status).toBe(200);
      const json = (await resp.json()) as { erasure_enqueued: boolean; tenant_id: string; dsr_id: string };
      expect(json.erasure_enqueued).toBe(true);
      expect(json.tenant_id).toBe("tenant-uuid-1");
      expect(sent).toHaveLength(1);
      const msg = sent[0] as Record<string, unknown>;
      expect(msg.schema).toBe("dev.hugr.corelink.dsr.queued.v1");
      expect(msg.tenant_id).toBe("tenant-uuid-1");
      expect(msg.subject_id).toBe("tenant-uuid-1");
      expect(msg.legal_hold).toBe(false);
      expect(msg.source).toBe("clerk.user.deleted");
      expect((msg.erasure_salt_hex as string)).toHaveLength(64);
      expect(msg.dsr_id).toBe(json.dsr_id);
    });

    it("a tenant UNDER LEGAL HOLD → erasure enqueued with legal_hold=true (preserve, not destroy) (REV-O1)", async () => {
      // REV-O1: the hold flag was hardcoded false, so the CTRL-PRIV-033
      // preservation branch was unreachable from the only live erasure trigger.
      // The handler now consults a `tenant_legal_hold` source-of-truth. This
      // stub answers the tenant lookup AND reports an ACTIVE hold for the tenant.
      const sent: unknown[] = [];
      const heldDb = {
        prepare(query: string) {
          let bound: unknown[] = [];
          const stmt = {
            bind(...v: unknown[]) {
              bound = v;
              return stmt;
            },
            async run() {
              return { success: true };
            },
            async first<T = unknown>(): Promise<T | null> {
              if (query.includes("clerk_user_id")) {
                return { tenant_id: "tenant-held-1" } as T;
              }
              if (query.includes("tenant_legal_hold")) {
                // bound[0] is the tenant_id; report an active hold for it.
                return bound[0] === "tenant-held-1" ? ({ held: 1 } as T) : null;
              }
              return null;
            },
          };
          return stmt;
        },
      };
      const env: AutoProvisionEnv = {
        CLERK_WEBHOOK_SECRET: WEBHOOK_SECRET,
        CORELINK_API_BASE: "https://corelink-api.humangr.com",
        CONFIG_DB: heldDb as unknown as ConfigDb,
        DSR_QUEUE: { send: async (m: unknown) => { sent.push(m); } },
        ERASURE_SALT_KEY: "test-erasure-salt-key",
      };
      const resp = await handleClerkWebhook(await deletedReq("user_held_1"), env, defaultApiClient);
      expect(resp.status).toBe(200);
      expect(sent).toHaveLength(1);
      const msg = sent[0] as Record<string, unknown>;
      // The crux: a held tenant's deletion carries legal_hold=true so the
      // downstream adapters PRESERVE the data instead of erasing it.
      expect(msg.legal_hold).toBe(true);
      expect(msg.tenant_id).toBe("tenant-held-1");
    });

    it("is idempotent: a redelivered user.deleted yields the same dsr_id", async () => {
      const db = new InMemoryD1();
      await seedTenant(db, "tenant-uuid-2", "user_del_2");
      const sent: unknown[] = [];
      const env = envWith(db, sent);
      const r1 = (await (await handleClerkWebhook(await deletedReq("user_del_2"), env, defaultApiClient)).json()) as { dsr_id: string };
      const r2 = (await (await handleClerkWebhook(await deletedReq("user_del_2"), env, defaultApiClient)).json()) as { dsr_id: string };
      expect(r1.dsr_id).toBe(r2.dsr_id);
      expect((sent[0] as Record<string, unknown>).dsr_id).toBe((sent[1] as Record<string, unknown>).dsr_id);
    });

    it("no-ops (200, erasure_enqueued=false) when the deleted user has no tenant", async () => {
      const db = new InMemoryD1(); // empty — no tenant for this user
      const sent: unknown[] = [];
      const resp = await handleClerkWebhook(await deletedReq("user_nobody"), envWith(db, sent), defaultApiClient);
      expect(resp.status).toBe(200);
      const json = (await resp.json()) as { erasure_enqueued: boolean; reason: string };
      expect(json.erasure_enqueued).toBe(false);
      expect(json.reason).toBe("no_tenant");
      expect(sent).toHaveLength(0);
    });

    it("FAILS LOUD (500) when a tenant exists but DSR_QUEUE is unbound", async () => {
      const db = new InMemoryD1();
      await seedTenant(db, "tenant-uuid-3", "user_del_3");
      const resp = await handleClerkWebhook(await deletedReq("user_del_3"), envWith(db, null), defaultApiClient);
      expect(resp.status).toBe(500);
      const json = (await resp.json()) as { error: string };
      expect(json.error).toBe("dsr_queue_unconfigured");
    });
  });

  describe("erasure message helpers", () => {
    it("deterministicDsrId is stable, distinct per user, and valid-UUID-shaped", async () => {
      const a = await deterministicDsrId("user_x");
      const b = await deterministicDsrId("user_x");
      const c = await deterministicDsrId("user_y");
      expect(a).toBe(b);
      expect(a).not.toBe(c);
      expect(a).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-5[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/);
    });

    it("buildErasureQueueMessage: 32-byte salt; secret-keyed salt differs from the fallback", async () => {
      const withKey = await buildErasureQueueMessage({ clerkUserId: "u", tenantId: "t", nowMs: 1, saltKey: "k" });
      const noKey = await buildErasureQueueMessage({ clerkUserId: "u", tenantId: "t", nowMs: 1, saltKey: undefined });
      expect(withKey.erasure_salt_hex).toHaveLength(64);
      expect(noKey.erasure_salt_hex).toHaveLength(64);
      expect(withKey.erasure_salt_hex).not.toBe(noKey.erasure_salt_hex);
      expect(withKey.dsr_id).toBe(noKey.dsr_id); // dsr_id is independent of the salt key
    });

    it("buildErasureQueueMessage: legal_hold flows through (default false; true PRESERVES per CTRL-PRIV-033) (REV-O1)", async () => {
      const unheld = await buildErasureQueueMessage({ clerkUserId: "u", tenantId: "t", nowMs: 1, saltKey: "k" });
      expect(unheld.legal_hold).toBe(false); // default — no hold
      const held = await buildErasureQueueMessage({
        clerkUserId: "u",
        tenantId: "t",
        nowMs: 1,
        saltKey: "k",
        legalHold: true,
      });
      // The hold flag is no longer hardcoded — a held tenant carries legal_hold=true,
      // reaching the adapter preservation branch (adapter_d1.rs:202 et al).
      expect(held.legal_hold).toBe(true);
    });
  });

  it("metadata split: tenant_id/region → public_metadata, pat_plaintext → private_metadata (CTRL-CRED-001)", async () => {
    // Verify the secret never lands on the JWT/client surface (public_metadata)
    // and the session claims the /welcome page reads carry no secret.
    const capturedPublic: Record<string, unknown>[] = [];
    const capturedPrivate: Record<string, unknown>[] = [];

    const env: AutoProvisionEnv = {
      CLERK_WEBHOOK_SECRET: WEBHOOK_SECRET,
      CORELINK_API_BASE: "https://corelink-api.humangr.com",
      // #195 fail-loud: the PAT-mint key gate runs before provisioning.
      CORELINK_INTERNAL_AUTH_KEY: "test-internal-auth-key-e2e",
    };

    const claimCapturingApi = () => ({
      async createTenant(_name: string, _userId: string) {
        return { id: "tenant-claim-test" };
      },
      async configureTenant() {},
      async issuePat() {
        return { id: "pat-claim-test", plaintext: "corelink_pat_CLAIMTEST" };
      },
      async publishUserMetadata(
        _userId: string,
        publicMeta: Record<string, unknown>,
        privateMeta: Record<string, unknown>,
      ) {
        capturedPublic.push(publicMeta);
        capturedPrivate.push(privateMeta);
      },
    });

    const event = makeUserCreatedEvent("user_claim_test");
    const req = await makeWebhookRequest(event, WEBHOOK_SECRET);
    const resp = await handleClerkWebhook(req, env, claimCapturingApi);

    expect(resp.status).toBe(200);
    expect(capturedPublic).toHaveLength(1);
    expect(capturedPrivate).toHaveLength(1);

    const pub = capturedPublic[0]!;
    // public_metadata = the legit session claims welcome/page.tsx reads.
    expect(typeof pub["tenant_id"]).toBe("string");
    expect(typeof pub["region"]).toBe("string");
    // The secret must NOT ride public_metadata (JWT/client-readable).
    expect(pub).not.toHaveProperty("pat_plaintext");

    const priv = capturedPrivate[0]!;
    // private_metadata (backend-only) carries the one-time secret + clock.
    expect(typeof priv["pat_plaintext"]).toBe("string");
    expect((priv["pat_plaintext"] as string).length).toBeGreaterThan(0);
    expect(typeof priv["pat_revealed_at"]).toBe("number");
  });
});
