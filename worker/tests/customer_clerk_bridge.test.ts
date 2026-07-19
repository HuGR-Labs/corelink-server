/**
 * Unit tests for the /v1/customer/* Clerk session bridge (dashboard revival
 * WP-1) — the dual-auth dispatch in worker/src/index.ts plus the shared
 * verification helper worker/src/lib/clerk_auth.ts.
 *
 * Dispatch contract under test:
 *   - A bearer that parses as a canonical CoreLink PAT takes the EXISTING
 *     generic PAT gate, byte-identical (regression: valid PAT forwards;
 *     revoked PAT — migration 0063 — still 401s and NEVER enters the Clerk
 *     pipeline).
 *   - Anything else (Clerk JWT, garbage, missing header) takes the Clerk
 *     pipeline: verifyToken + M1 azp re-assert + M2 issuer check + tenant
 *     lookup by clerk_user_id; then a least-privilege forward to the
 *     per-tenant DO (NO authorization header, NO x-corelink-internal-auth).
 *
 * Mirrors onboarding.test.ts patterns (@clerk/backend mocked per test; the DO
 * stub captures the forwarded request) + index.test.ts PAT fixtures (canonical
 * 96-char PAT shape; D1 mock honoring the `revoked_at_ms IS NULL` predicate).
 */

import { describe, it, expect, vi, beforeEach } from "vitest";
import type { D1Database, DurableObjectNamespace } from "@cloudflare/workers-types";

// Mock the Clerk edge verifier (hoisted) — controlled per test.
vi.mock("@clerk/backend", () => ({ verifyToken: vi.fn() }));
import { verifyToken } from "@clerk/backend";
import workerHandler from "../src/index.js";
import type { Env } from "../src/index.js";
import { TEST_PAT_SIGNING_KEY, mintTestPat } from "./setup.js";

const mockVerifyToken = vi.mocked(verifyToken);

const CLERK_SECRET = "sk_test_clerk_secret";
const INTERNAL_KEY = "test-internal-auth-key-0123456789";

// Canonical test PAT — CRYPTOGRAPHICALLY VALID under TEST_PAT_SIGNING_KEY
// (honest auth harness, #345). The native plane (extractAuth) fails CLOSED
// with a 503 unless PAT_SIGNING_KEY is bound AND the PAT's HMAC verifies, so
// PAT-surface regression tests must mint a real signed PAT and bind the key
// (makeBridgeEnv does the latter). The earlier inline all-"A" fixture relied
// on the now-removed "PAT_SIGNING_KEY absent ⇒ skip HMAC" theater and 503'd.
const TEST_TOKEN_ID = "AAAAAAAAAAAAAAAA"; // 16 Crockford b32 chars
const TEST_PAT_TOKEN = await mintTestPat({ tokenId: TEST_TOKEN_ID });
if (TEST_PAT_TOKEN.length !== 96) {
  throw new Error(`TEST_PAT_TOKEN length ${TEST_PAT_TOKEN.length} !== 96`);
}
const REVOKED_TOKEN_ID = "DDDDDDDDDDDDDDDD";
const REVOKED_PAT_TOKEN = await mintTestPat({ tokenId: REVOKED_TOKEN_ID });

const TEST_TENANT_ID = "00000000-0000-0000-0000-000000000001";
const CLERK_TENANT_ID = "00000000-0000-0000-0000-00000000c1e7";
// Team-member fallback (WP-T4) fixtures: tenant A OWNS the second seat; tenant
// B is an UNRELATED tenant the member must never resolve to (cross-tenant).
const TENANT_A_ID = "00000000-0000-0000-0000-0000000000aa";
const TENANT_B_ID = "00000000-0000-0000-0000-0000000000bb";

function makeCtx(): ExecutionContext {
  return {
    waitUntil: (_p: Promise<unknown>) => {},
    passThroughOnException: () => {},
  } as unknown as ExecutionContext;
}

/**
 * Combined D1 mock — dispatches on the SQL string:
 *   - `FROM pat` lookups resolve by token_id, honoring the soft-revocation
 *     predicate `revoked_at_ms IS NULL` (migration 0063);
 *   - `FROM tenant WHERE clerk_user_id` lookups resolve the Clerk bridge's
 *     OWNER tenant mapping (migration 0056);
 *   - `FROM team_member WHERE user_id` lookups resolve the ADDITIVE
 *     team-member fallback (migration 0074), honoring the MANDATORY
 *     `status = 'active'` predicate (an 'invited'/'removed' seat → null);
 *   - everything else (tier/quota/region probes on the PAT path) → null.
 */
function makeDualD1(opts: {
  pats?: Map<string, { tenant_id: string; expires_ms: number; revoked_at_ms?: number | null }>;
  clerkUserToTenant?: Map<string, string>;
  teamMembers?: Map<string, { tenant_id: string; status: string; role?: string }>;
}): D1Database {
  return {
    prepare: (sql: string) => ({
      bind: (...args: unknown[]) => ({
        first: async <T>() => {
          if (sql.includes("FROM pat")) {
            const row = opts.pats?.get(args[0] as string);
            if (sql.includes("revoked_at_ms IS NULL") && row?.revoked_at_ms != null) {
              return null as T | null;
            }
            return (row ?? null) as T | null;
          }
          if (sql.includes("FROM tenant WHERE clerk_user_id")) {
            const tenantId = opts.clerkUserToTenant?.get(args[0] as string);
            return (tenantId ? { tenant_id: tenantId } : null) as T | null;
          }
          if (sql.includes("FROM team_member WHERE user_id")) {
            const member = opts.teamMembers?.get(args[0] as string);
            // Honor the MANDATORY `status = 'active'` filter: a non-active
            // (invited/removed) seat must NOT resolve.
            if (
              !member ||
              (sql.includes("status = 'active'") && member.status !== "active")
            ) {
              return null as T | null;
            }
            return { tenant_id: member.tenant_id, role: member.role } as T | null;
          }
          return null as T | null;
        },
        all: async <T>() => ({ success: true as const, meta: {} as never, results: [] as T[] }),
        run: async <T>() => ({ success: true as const, meta: {} as never, results: [] as T[] }),
        raw: async <T>() => [] as T[],
      }),
      first: async <T>() => null as T | null,
      all: async <T>() => ({ success: true as const, meta: {} as never, results: [] as T[] }),
      run: async <T>() => ({ success: true as const, meta: {} as never, results: [] as T[] }),
      raw: async <T>() => [] as T[],
    }),
    batch: async () => [],
    exec: async () => ({ count: 0, duration: 0 }),
    withSession() { return this; },
    dump: async () => new ArrayBuffer(0),
  } as unknown as D1Database;
}

/** CORELINK_SERVER DO namespace that CAPTURES the forwarded request + DO name. */
function makeCaptureNamespace(captured: { req?: Request; doName?: string }): DurableObjectNamespace {
  const stub = {
    fetch: async (req: Request): Promise<Response> => {
      captured.req = req;
      return new Response(JSON.stringify({ ok: true }), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      });
    },
  };
  const namespace = {
    idFromName: (n: string) => {
      captured.doName = n;
      return { toString: () => `stub-${n}` };
    },
    get: (_id: unknown) => stub,
    idFromString: (_s: string) => ({ toString: () => "stub-id" }),
    newUniqueId: () => ({ toString: () => "stub-unique" }),
    jurisdiction: (_j: string) => namespace,
  } as unknown as DurableObjectNamespace;
  return namespace;
}

function makeBridgeEnv(opts: {
  captured: { req?: Request; doName?: string };
  pats?: Map<string, { tenant_id: string; expires_ms: number; revoked_at_ms?: number | null }>;
  clerkUserToTenant?: Map<string, string>;
  teamMembers?: Map<string, { tenant_id: string; status: string; role?: string }>;
  withClerkSecret?: boolean;
}): Env {
  return {
    CORELINK_SERVER: makeCaptureNamespace(opts.captured),
    ENVIRONMENT: "test",
    CONFIG_DB: makeDualD1({ pats: opts.pats, clerkUserToTenant: opts.clerkUserToTenant, teamMembers: opts.teamMembers }),
    // Honest auth harness (#345): bind the valid test signing key so a minted
    // PAT's HMAC verifies and the PAT surface reaches the real auth/route logic
    // (instead of fail-closing to 503 on an unset key).
    PAT_SIGNING_KEY: TEST_PAT_SIGNING_KEY,
    CLERK_SECRET_KEY: opts.withClerkSecret === false ? undefined : CLERK_SECRET,
    // Bound ON PURPOSE: the least-privilege assertion below must prove the
    // customer_v1 Clerk forward does NOT inject internal-auth even when the
    // key IS available to the Worker (unlike onboarding, which injects it).
    CORELINK_INTERNAL_AUTH_KEY: INTERNAL_KEY,
  } as Env;
}

async function customerFetch(env: Env, headers: Record<string, string>): Promise<Response> {
  const req = new Request("http://localhost/v1/customer/overview", {
    method: "GET",
    headers,
  });
  return workerHandler.fetch!(req, env, makeCtx());
}

// DSR destructive-arm (erasure) fetch on the privacy plane — the surface where
// the Worker stamps the container's fail-CLOSED `x-corelink-mfa-verified: 1`
// step-up marker. Distinct path from customerFetch on purpose (the marker is
// ONLY set for /v1/privacy/*).
async function privacyEraseFetch(env: Env, headers: Record<string, string>): Promise<Response> {
  const req = new Request("http://localhost/v1/privacy/dsr/erasure", {
    method: "POST",
    headers,
  });
  return workerHandler.fetch!(req, env, makeCtx());
}

describe("/v1/customer/* — Clerk session bridge (dashboard revival WP-1)", () => {
  beforeEach(() => {
    mockVerifyToken.mockReset();
  });

  // ── PAT surface regression (byte-identical dispatch) ───────────────────────

  it("PAT path regression: a valid canonical PAT still forwards via the PAT gate", async () => {
    const captured: { req?: Request; doName?: string } = {};
    const env = makeBridgeEnv({
      captured,
      pats: new Map([[TEST_TOKEN_ID, { tenant_id: TEST_TENANT_ID, expires_ms: Date.now() + 3_600_000 }]]),
    });

    const resp = await customerFetch(env, { Authorization: `Bearer ${TEST_PAT_TOKEN}` });

    expect(resp.status).toBe(200);
    const h = captured.req!.headers;
    expect(h.get("x-corelink-route-kind")).toBe("customer_v1");
    expect(h.get("x-corelink-tenant-id")).toBe(TEST_TENANT_ID);
    // The PAT path FORWARDS Authorization (DO performs the Argon2id verify) —
    // proof the request took the PAT gate, not the Clerk arm (which drops it).
    expect(h.get("authorization")).toBe(`Bearer ${TEST_PAT_TOKEN}`);
    expect(h.get("x-corelink-token-prefix")).not.toBe("clerk");
    // The Clerk verifier must never run on the PAT surface.
    expect(mockVerifyToken).not.toHaveBeenCalled();
  });

  it("revoked-PAT regression (0063): a soft-revoked PAT still 401s on the PAT surface", async () => {
    // A revoked PAT still PARSES as a canonical PAT, so the dispatch guard
    // must route it to the PAT gate (where `revoked_at_ms IS NULL` filters it
    // out → 401) — NEVER into the Clerk pipeline.
    const captured: { req?: Request; doName?: string } = {};
    const env = makeBridgeEnv({
      captured,
      pats: new Map([
        [REVOKED_TOKEN_ID, { tenant_id: TEST_TENANT_ID, expires_ms: Date.now() + 3_600_000, revoked_at_ms: Date.now() - 1000 }],
      ]),
    });

    const resp = await customerFetch(env, { Authorization: `Bearer ${REVOKED_PAT_TOKEN}` });

    expect(resp.status).toBe(401);
    const body = await resp.json() as { error: string };
    expect(body.error).toBe("UNAUTHORIZED");
    expect(captured.req).toBeUndefined();
    expect(mockVerifyToken).not.toHaveBeenCalled();
  });

  // ── Clerk pipeline: rejection paths ─────────────────────────────────────────

  it("returns 401 on a bad-signature / expired Clerk JWT (never forwards)", async () => {
    mockVerifyToken.mockRejectedValue(new Error("signature verification failed"));
    const captured: { req?: Request; doName?: string } = {};
    const env = makeBridgeEnv({ captured });

    const resp = await customerFetch(env, { Authorization: "Bearer eyJ.bad.jwt" });

    expect(resp.status).toBe(401);
    const body = await resp.json() as { error: string };
    expect(body.error).toBe("UNAUTHORIZED");
    expect(captured.req).toBeUndefined();
  });

  it("returns 401 when no Authorization header is present (Clerk arm, UNAUTHORIZED envelope)", async () => {
    const captured: { req?: Request; doName?: string } = {};
    const env = makeBridgeEnv({ captured });

    const resp = await customerFetch(env, {});

    expect(resp.status).toBe(401);
    const body = await resp.json() as { error: string };
    expect(body.error).toBe("UNAUTHORIZED");
    expect(captured.req).toBeUndefined();
    expect(mockVerifyToken).not.toHaveBeenCalled();
  });

  it("returns 403 when the verified session has no CoreLink tenant", async () => {
    mockVerifyToken.mockResolvedValue({
      sub: "user_no_tenant",
      azp: "https://humangr.com",
      iss: "https://clerk.humangr.com",
    } as never);
    const captured: { req?: Request; doName?: string } = {};
    const env = makeBridgeEnv({ captured, clerkUserToTenant: new Map() }); // no mapping

    const resp = await customerFetch(env, { Authorization: "Bearer clerk.jwt" });

    expect(resp.status).toBe(403);
    expect(captured.req).toBeUndefined();
  });

  it("M1: returns 401 when azp is not in the allowlist (never forwards)", async () => {
    mockVerifyToken.mockResolvedValue({
      sub: "user_abc",
      azp: "https://attacker-app.example.com",
      iss: "https://clerk.humangr.com",
    } as never);
    const captured: { req?: Request; doName?: string } = {};
    const env = makeBridgeEnv({
      captured,
      clerkUserToTenant: new Map([["user_abc", CLERK_TENANT_ID]]),
    });

    const resp = await customerFetch(env, { Authorization: "Bearer wrong-azp.jwt" });

    expect(resp.status).toBe(401);
    expect(captured.req).toBeUndefined();
  });

  it("M1: returns 401 when azp is absent (library skips the check; our re-assert catches it)", async () => {
    mockVerifyToken.mockResolvedValue({
      sub: "user_abc",
      // azp intentionally absent
      iss: "https://clerk.humangr.com",
    } as never);
    const captured: { req?: Request; doName?: string } = {};
    const env = makeBridgeEnv({
      captured,
      clerkUserToTenant: new Map([["user_abc", CLERK_TENANT_ID]]),
    });

    const resp = await customerFetch(env, { Authorization: "Bearer no-azp.jwt" });

    expect(resp.status).toBe(401);
    expect(captured.req).toBeUndefined();
  });

  it("accepts azp from humangr.com (the dashboard host)", async () => {
    mockVerifyToken.mockResolvedValue({
      sub: "user_app",
      azp: "https://humangr.com",
      iss: "https://clerk.humangr.com",
    } as never);
    const captured: { req?: Request; doName?: string } = {};
    const env = makeBridgeEnv({
      captured,
      clerkUserToTenant: new Map([["user_app", CLERK_TENANT_ID]]),
    });

    const resp = await customerFetch(env, { Authorization: "Bearer clerk.jwt" });

    expect(resp.status).toBe(200);
    expect(captured.req!.headers.get("x-corelink-tenant-id")).toBe(CLERK_TENANT_ID);
  });

  it("fail-CLOSED: 403 when CLERK_SECRET_KEY is unbound (Clerk arm unavailable)", async () => {
    const captured: { req?: Request; doName?: string } = {};
    const env = makeBridgeEnv({ captured, withClerkSecret: false });

    const resp = await customerFetch(env, { Authorization: "Bearer clerk.jwt" });

    expect(resp.status).toBe(403);
    expect(captured.req).toBeUndefined();
    expect(mockVerifyToken).not.toHaveBeenCalled();
  });

  // ── Clerk pipeline: least-privilege forward ─────────────────────────────────

  it("valid Clerk session: forwards to the PER-TENANT DO with the exact server-trust headers", async () => {
    mockVerifyToken.mockResolvedValue({
      sub: "user_abc",
      azp: "https://humangr.com",
      iss: "https://clerk.humangr.com",
    } as never);
    const captured: { req?: Request; doName?: string } = {};
    const env = makeBridgeEnv({
      captured,
      clerkUserToTenant: new Map([["user_abc", CLERK_TENANT_ID]]),
    });

    const resp = await customerFetch(env, { Authorization: "Bearer valid.clerk.jwt" });

    expect(resp.status).toBe(200);
    expect(captured.req).toBeDefined();
    // Per-tenant DO — idFromName(resolved tenant), never a shared sentinel.
    expect(captured.doName).toBe(CLERK_TENANT_ID);
    const h = captured.req!.headers;
    expect(h.get("x-corelink-tenant-id")).toBe(CLERK_TENANT_ID);
    expect(h.get("x-corelink-route-kind")).toBe("customer_v1");
    expect(h.get("x-corelink-token-prefix")).toBe("clerk");
    expect(h.get("x-corelink-scope")).toBe("read-write billing");
    // Least privilege: the Clerk JWT is dropped at the edge…
    expect(h.get("authorization")).toBeNull();
    // …and the internal-auth key is NOT injected (it IS bound in this env —
    // proof the forward deliberately withholds it, unlike onboarding).
    expect(h.get("x-corelink-internal-auth")).toBeNull();
  });

  it("STRIPS client-supplied trust headers before forwarding (spoof defence)", async () => {
    mockVerifyToken.mockResolvedValue({
      sub: "user_abc",
      azp: "https://humangr.com",
      iss: "https://clerk.humangr.com",
    } as never);
    const captured: { req?: Request; doName?: string } = {};
    const env = makeBridgeEnv({
      captured,
      clerkUserToTenant: new Map([["user_abc", CLERK_TENANT_ID]]),
    });

    await customerFetch(env, {
      Authorization: "Bearer valid.clerk.jwt",
      "x-corelink-internal-auth": "SPOOFED-KEY",
      "x-corelink-tenant-id": "victim-tenant",
      "x-corelink-scope": "admin",
    });

    const h = captured.req!.headers;
    expect(h.get("x-corelink-internal-auth")).toBeNull();
    expect(h.get("x-corelink-tenant-id")).toBe(CLERK_TENANT_ID);
    expect(h.get("x-corelink-scope")).toBe("read-write billing");
  });

  // ── Team-member fallback (C-RESOLVE / WP-T4) ────────────────────────────────

  it("team_member fallback: an ACTIVE second-seat resolves to the OWNING tenant", async () => {
    // This Clerk user did NOT provision a tenant (no `tenant.clerk_user_id`
    // row) but is an ACTIVE member of tenant A. The additive OR-branch must
    // resolve the session to tenant A.
    mockVerifyToken.mockResolvedValue({
      sub: "user_member_a",
      azp: "https://humangr.com",
      iss: "https://clerk.humangr.com",
    } as never);
    const captured: { req?: Request; doName?: string } = {};
    const env = makeBridgeEnv({
      captured,
      clerkUserToTenant: new Map(), // NO owner row → forces the fallback
      teamMembers: new Map([["user_member_a", { tenant_id: TENANT_A_ID, status: "active", role: "member" }]]),
    });

    const resp = await customerFetch(env, { Authorization: "Bearer member.clerk.jwt" });

    expect(resp.status).toBe(200);
    expect(captured.doName).toBe(TENANT_A_ID);
    expect(captured.req!.headers.get("x-corelink-tenant-id")).toBe(TENANT_A_ID);
  });

  it("team_member RBAC: a VIEWER seat gets read-only scope (not read-write)", async () => {
    mockVerifyToken.mockResolvedValue({
      sub: "user_viewer",
      azp: "https://humangr.com",
      iss: "https://clerk.humangr.com",
    } as never);
    const captured: { req?: Request; doName?: string } = {};
    const env = makeBridgeEnv({
      captured,
      clerkUserToTenant: new Map(), // NO owner row → team_member fallback
      teamMembers: new Map([["user_viewer", { tenant_id: TENANT_A_ID, status: "active", role: "viewer" }]]),
    });

    const resp = await customerFetch(env, { Authorization: "Bearer viewer.clerk.jwt" });

    expect(resp.status).toBe(200);
    // The core of the HIGH finding: a viewer must NOT receive read-write.
    expect(captured.req!.headers.get("x-corelink-scope")).toBe("read-only");
  });

  it("team_member RBAC: an ADMIN seat gets read-write scope", async () => {
    mockVerifyToken.mockResolvedValue({
      sub: "user_admin",
      azp: "https://humangr.com",
      iss: "https://clerk.humangr.com",
    } as never);
    const captured: { req?: Request; doName?: string } = {};
    const env = makeBridgeEnv({
      captured,
      clerkUserToTenant: new Map(),
      teamMembers: new Map([["user_admin", { tenant_id: TENANT_A_ID, status: "active", role: "admin" }]]),
    });

    const resp = await customerFetch(env, { Authorization: "Bearer admin.clerk.jwt" });

    expect(resp.status).toBe(200);
    expect(captured.req!.headers.get("x-corelink-scope")).toBe("read-write billing");
  });

  it("team_member RBAC: a missing/unknown role fails SAFE to read-only", async () => {
    mockVerifyToken.mockResolvedValue({
      sub: "user_norole",
      azp: "https://humangr.com",
      iss: "https://clerk.humangr.com",
    } as never);
    const captured: { req?: Request; doName?: string } = {};
    const env = makeBridgeEnv({
      captured,
      clerkUserToTenant: new Map(),
      // role intentionally absent (simulates a corrupt/legacy row) → least privilege.
      teamMembers: new Map([["user_norole", { tenant_id: TENANT_A_ID, status: "active" }]]),
    });

    const resp = await customerFetch(env, { Authorization: "Bearer norole.clerk.jwt" });

    expect(resp.status).toBe(200);
    expect(captured.req!.headers.get("x-corelink-scope")).toBe("read-only");
  });

  it("team_member fallback: a member of tenant A does NOT resolve to tenant B (cross-tenant)", async () => {
    // The member belongs ONLY to tenant A. An unrelated tenant B exists. The
    // user_id-keyed lookup must yield tenant A — never tenant B.
    mockVerifyToken.mockResolvedValue({
      sub: "user_member_a",
      azp: "https://humangr.com",
      iss: "https://clerk.humangr.com",
    } as never);
    const captured: { req?: Request; doName?: string } = {};
    const env = makeBridgeEnv({
      captured,
      clerkUserToTenant: new Map(),
      // Only an A-membership exists; nothing maps this user to tenant B.
      teamMembers: new Map([["user_member_a", { tenant_id: TENANT_A_ID, status: "active", role: "member" }]]),
    });

    const resp = await customerFetch(env, { Authorization: "Bearer member.clerk.jwt" });

    expect(resp.status).toBe(200);
    expect(captured.doName).toBe(TENANT_A_ID);
    expect(captured.doName).not.toBe(TENANT_B_ID);
    expect(captured.req!.headers.get("x-corelink-tenant-id")).toBe(TENANT_A_ID);
    expect(captured.req!.headers.get("x-corelink-tenant-id")).not.toBe(TENANT_B_ID);
  });

  it("team_member fallback: a REMOVED member is DENIED (403, never forwards)", async () => {
    // The mandatory `status = 'active'` filter must exclude a removed seat —
    // no owner row + no active membership ⇒ 403, no DO forward.
    mockVerifyToken.mockResolvedValue({
      sub: "user_removed",
      azp: "https://humangr.com",
      iss: "https://clerk.humangr.com",
    } as never);
    const captured: { req?: Request; doName?: string } = {};
    const env = makeBridgeEnv({
      captured,
      clerkUserToTenant: new Map(),
      teamMembers: new Map([["user_removed", { tenant_id: TENANT_A_ID, status: "removed" }]]),
    });

    const resp = await customerFetch(env, { Authorization: "Bearer removed.clerk.jwt" });

    expect(resp.status).toBe(403);
    const body = await resp.json() as { error: string };
    expect(body.error).toBe("FORBIDDEN");
    expect(captured.req).toBeUndefined();
  });

  // ── DSR erasure MFA step-up freshness (Finding H2) ──────────────────────────
  // The container erasure gate (routes/dsr/portal.rs) is fail-CLOSED on the
  // Worker-set `x-corelink-mfa-verified: 1`. The Worker must stamp it ONLY when
  // the Clerk session's factor-verification age (`fva[0]`, minutes) is FRESH —
  // otherwise a stolen/XSS long-lived dashboard session could trigger
  // irreversible cross-region erasure with no re-auth.

  it("erasure WITH a fresh factor-verification age stamps x-corelink-mfa-verified", async () => {
    mockVerifyToken.mockResolvedValue({
      sub: "user_dsr_fresh",
      azp: "https://humangr.com",
      iss: "https://clerk.humangr.com",
      // fva[0] = 0 ⇒ the user reauthed within the last minute (FRESH).
      fva: [0, 0],
    } as never);
    const captured: { req?: Request; doName?: string } = {};
    const env = makeBridgeEnv({
      captured,
      clerkUserToTenant: new Map([["user_dsr_fresh", CLERK_TENANT_ID]]),
    });

    const resp = await privacyEraseFetch(env, { Authorization: "Bearer fresh.clerk.jwt" });

    expect(resp.status).toBe(200);
    expect(captured.req).toBeDefined();
    // The load-bearing assertion: the fresh session is stamped mfa-verified so
    // the container gate ALLOWS the destructive erase.
    expect(captured.req!.headers.get("x-corelink-mfa-verified")).toBe("1");
    expect(captured.doName).toBe(CLERK_TENANT_ID);
  });

  it("erasure at the freshness boundary (fva == threshold) still stamps mfa-verified", async () => {
    mockVerifyToken.mockResolvedValue({
      sub: "user_dsr_boundary",
      azp: "https://humangr.com",
      iss: "https://clerk.humangr.com",
      // fva[0] = 5 = MFA_FVA_FRESH_MAX_MINUTES ⇒ still fresh (inclusive bound).
      fva: [5, 5],
    } as never);
    const captured: { req?: Request; doName?: string } = {};
    const env = makeBridgeEnv({
      captured,
      clerkUserToTenant: new Map([["user_dsr_boundary", CLERK_TENANT_ID]]),
    });

    const resp = await privacyEraseFetch(env, { Authorization: "Bearer boundary.clerk.jwt" });

    expect(resp.status).toBe(200);
    expect(captured.req!.headers.get("x-corelink-mfa-verified")).toBe("1");
  });

  it("erasure WITHOUT a fresh factor age (no fva claim) does NOT stamp mfa-verified (fail-CLOSED)", async () => {
    mockVerifyToken.mockResolvedValue({
      sub: "user_dsr_nofva",
      azp: "https://humangr.com",
      iss: "https://clerk.humangr.com",
      // fva intentionally absent ⇒ NOT fresh (undefined, never treated as 0).
    } as never);
    const captured: { req?: Request; doName?: string } = {};
    const env = makeBridgeEnv({
      captured,
      clerkUserToTenant: new Map([["user_dsr_nofva", CLERK_TENANT_ID]]),
    });

    const resp = await privacyEraseFetch(env, { Authorization: "Bearer nofva.clerk.jwt" });

    // The session is still authenticated (forwards to the DO), but the marker
    // is WITHHELD, so the container gate fails closed → step-up required.
    expect(resp.status).toBe(200);
    expect(captured.req).toBeDefined();
    expect(captured.req!.headers.get("x-corelink-mfa-verified")).toBeNull();
  });

  it("erasure with a STALE factor age (fva beyond threshold) does NOT stamp mfa-verified", async () => {
    mockVerifyToken.mockResolvedValue({
      sub: "user_dsr_stale",
      azp: "https://humangr.com",
      iss: "https://clerk.humangr.com",
      // fva[0] = 60 min ⇒ well past the 5-min step-up window (a stolen long-lived session).
      fva: [60, 60],
    } as never);
    const captured: { req?: Request; doName?: string } = {};
    const env = makeBridgeEnv({
      captured,
      clerkUserToTenant: new Map([["user_dsr_stale", CLERK_TENANT_ID]]),
    });

    const resp = await privacyEraseFetch(env, { Authorization: "Bearer stale.clerk.jwt" });

    expect(resp.status).toBe(200);
    expect(captured.req!.headers.get("x-corelink-mfa-verified")).toBeNull();
  });

  it("STRIPS a client-forged x-corelink-mfa-verified even on a stale erasure session (no smuggle)", async () => {
    mockVerifyToken.mockResolvedValue({
      sub: "user_dsr_forge",
      azp: "https://humangr.com",
      iss: "https://clerk.humangr.com",
      fva: [60, 60], // stale ⇒ the Worker must NOT re-stamp
    } as never);
    const captured: { req?: Request; doName?: string } = {};
    const env = makeBridgeEnv({
      captured,
      clerkUserToTenant: new Map([["user_dsr_forge", CLERK_TENANT_ID]]),
    });

    const resp = await privacyEraseFetch(env, {
      Authorization: "Bearer forge.clerk.jwt",
      // Attacker tries to smuggle the freshness marker directly.
      "x-corelink-mfa-verified": "1",
    });

    expect(resp.status).toBe(200);
    // Stripped as a client-trust header AND not re-stamped (stale) ⇒ null.
    expect(captured.req!.headers.get("x-corelink-mfa-verified")).toBeNull();
  });

  it("M2: rejects (401) when CLERK_ISSUER_URL is pinned and iss does not match", async () => {
    mockVerifyToken.mockResolvedValue({
      sub: "user_abc",
      azp: "https://humangr.com",
      iss: "https://attacker.clerk.accounts.dev",
    } as never);
    const captured: { req?: Request; doName?: string } = {};
    const env: Env = {
      ...makeBridgeEnv({ captured, clerkUserToTenant: new Map([["user_abc", CLERK_TENANT_ID]]) }),
      CLERK_ISSUER_URL: "https://clerk.humangr.com",
    };

    const resp = await customerFetch(env, { Authorization: "Bearer wrong-issuer.jwt" });

    expect(resp.status).toBe(401);
    expect(captured.req).toBeUndefined();
  });
});
