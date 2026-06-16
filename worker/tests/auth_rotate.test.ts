/**
 * Unit tests for the clw `auth rotate` seam:
 *   - POST /internal/v1/auth/rotate — atomic mint-NEW + revoke-OLD.
 *
 * Rotate is the runner mint/revoke pattern composed: it reads the OLD `pat` row
 * for its tenant + scope, mints an EQUIVALENT-scope new PAT for the SAME tenant
 * via the single mint authority (mintScopedPat → the container's
 * /_internal/pat/mint via the _system DO), then revokes the OLD pat_id via the
 * existing `UPDATE pat SET revoked_at_ms` surface — but ONLY after the mint
 * succeeds, so a mint failure never leaves the caller with zero valid PATs.
 *
 * The _system DO stub captures the forwarded mint so we can assert the inherited
 * tenant + scope + that no client trust headers reach the mint route. The
 * CONFIG_DB stub answers the OLD-pat SELECT, the per-principal throttle INSERT,
 * and records the revoke UPDATE binds.
 */

import { describe, it, expect } from "vitest";
import type { D1Database, DurableObjectNamespace } from "@cloudflare/workers-types";
import workerHandler from "../src/index.js";
import type { Env } from "../src/index.js";

const INTERNAL_KEY = "test-internal-auth-key-0123456789"; // ≥32 chars
const PAT_MINT_KEY = "test-pat-mint-auth-key-0123456789ab"; // ≥32 chars, distinct

const TENANT = "11111111-1111-1111-1111-111111111111";
const OLD_PAT_ID = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";

const CANNED_MINT = {
  token_plaintext: "corelink_pat_abcdef0123456789.rndsecret.hmacsig",
  pat_id: "22222222-3333-4444-5555-666666666666",
  token_id: "abcdef0123456789",
  expires_ms: 1893456000000,
  hash: "$argon2id$v=19$m=19456,t=2,p=1$c2FsdA$aGFzaA",
};

/** Shape the OLD-pat SELECT resolves to (null = no row). */
interface OldPatRow {
  tenant_id: string;
  scope: string;
  expires_ms: number;
  revoked_at_ms: number | null;
}

function makeCtx(): ExecutionContext {
  return {
    waitUntil: (_p: Promise<unknown>) => {},
    passThroughOnException: () => {},
  } as unknown as ExecutionContext;
}

/**
 * CONFIG_DB stub.
 *   - `oldRow`: what the `SELECT ... FROM pat WHERE pat_id` resolves to (or null).
 *   - `revokeCapture`: records the binds of the revoke UPDATE.
 *   - `throttleCount`: the throttle INSERT...RETURNING count (default 1, under cap).
 */
function makeConfigDb(opts: {
  oldRow: OldPatRow | null;
  revokeCapture?: { binds?: unknown[]; called?: boolean };
  throttleCount?: number;
  revokeThrows?: boolean;
}): D1Database {
  return {
    prepare: (sql: string) => ({
      bind: (...args: unknown[]) => ({
        first: async <T>() => {
          if (sql.includes("FROM pat WHERE pat_id")) {
            return opts.oldRow as T | null;
          }
          if (sql.includes("session_exchange_throttle")) {
            return { count: opts.throttleCount ?? 1 } as T;
          }
          return null as T | null;
        },
        run: async () => {
          if (sql.includes("UPDATE pat SET revoked_at_ms")) {
            if (opts.revokeThrows) {
              throw new Error("D1 revoke transport error");
            }
            if (opts.revokeCapture) {
              opts.revokeCapture.binds = args;
              opts.revokeCapture.called = true;
            }
          }
          return { success: true } as unknown as D1Result;
        },
      }),
    }),
  } as unknown as D1Database;
}

function makeMintNamespace(
  captured: { req?: Request },
  opts: { status?: number; body?: unknown } = {},
): DurableObjectNamespace {
  const stub = {
    fetch: async (req: Request): Promise<Response> => {
      captured.req = req;
      return new Response(JSON.stringify(opts.body ?? CANNED_MINT), {
        status: opts.status ?? 200,
        headers: { "Content-Type": "application/json" },
      });
    },
  };
  const namespace = {
    idFromName: (_n: string) => ({ toString: () => "stub-id" }),
    get: (_id: unknown) => stub,
    idFromString: (_s: string) => ({ toString: () => "stub-id" }),
    newUniqueId: () => ({ toString: () => "stub-unique" }),
    jurisdiction: (_j: string) => namespace,
  } as unknown as DurableObjectNamespace;
  return namespace;
}

function makeEnv(opts: {
  captured?: { req?: Request };
  oldRow?: OldPatRow | null;
  revokeCapture?: { binds?: unknown[]; called?: boolean };
  mintStatus?: number;
  withInternalKey?: boolean;
  withPatMintKey?: boolean;
  revokeThrows?: boolean;
}): Env {
  return {
    CORELINK_SERVER: makeMintNamespace(opts.captured ?? {}, { status: opts.mintStatus }),
    ENVIRONMENT: "test",
    CONFIG_DB: makeConfigDb({
      oldRow: opts.oldRow ?? null,
      revokeCapture: opts.revokeCapture,
      revokeThrows: opts.revokeThrows,
    }),
    CORELINK_INTERNAL_AUTH_KEY: opts.withInternalKey === false ? undefined : INTERNAL_KEY,
    CORELINK_PAT_MINT_AUTH_KEY: opts.withPatMintKey ? PAT_MINT_KEY : undefined,
  } as Env;
}

function activeRow(scope = "read-write"): OldPatRow {
  return { tenant_id: TENANT, scope, expires_ms: 1893456000000, revoked_at_ms: null };
}

function rotateFetch(
  env: Env,
  opts: { auth?: string; body?: unknown; method?: string } = {},
): Promise<Response> {
  const headers: Record<string, string> = { "Content-Type": "application/json" };
  if (opts.auth !== undefined) headers["x-corelink-internal-auth"] = opts.auth;
  const method = opts.method ?? "POST";
  const init: RequestInit = { method, headers };
  if (method === "POST") {
    init.body =
      opts.body !== undefined ? JSON.stringify(opts.body) : JSON.stringify({ pat_id: OLD_PAT_ID });
  }
  const req = new Request("http://localhost/internal/v1/auth/rotate", init);
  return workerHandler.fetch!(req, env, makeCtx());
}

describe("POST /internal/v1/auth/rotate — clw auth rotate", () => {
  // ── (a) internal-auth gate ────────────────────────────────────────────────
  it("(a) 401 when the internal-auth header is missing (before any work)", async () => {
    const captured: { req?: Request } = {};
    const env = makeEnv({ captured, oldRow: activeRow() });
    const resp = await rotateFetch(env, {}); // no auth header
    expect(resp.status).toBe(401);
    expect(captured.req).toBeUndefined(); // never minted
  });

  it("(a) 401 when the internal-auth header is blank", async () => {
    const captured: { req?: Request } = {};
    const env = makeEnv({ captured, oldRow: activeRow() });
    const resp = await rotateFetch(env, { auth: "" });
    expect(resp.status).toBe(401);
    expect(captured.req).toBeUndefined();
  });

  it("(a) 401 when the internal-auth header is wrong", async () => {
    const captured: { req?: Request } = {};
    const env = makeEnv({ captured, oldRow: activeRow() });
    const resp = await rotateFetch(env, { auth: "wrong-but-long-enough-key-0123456789" });
    expect(resp.status).toBe(401);
    expect(captured.req).toBeUndefined();
  });

  it("(a) 403 when NO internal-auth key is bound (fail-CLOSED)", async () => {
    const captured: { req?: Request } = {};
    const env = makeEnv({ captured, oldRow: activeRow(), withInternalKey: false });
    const resp = await rotateFetch(env, { auth: INTERNAL_KEY });
    expect(resp.status).toBe(403);
    expect(captured.req).toBeUndefined();
  });

  it("(a) the dedicated pat_mint key is accepted; the shared key is then rejected", async () => {
    const c1: { req?: Request } = {};
    const env1 = makeEnv({ captured: c1, oldRow: activeRow(), withPatMintKey: true });
    const ok = await rotateFetch(env1, { auth: PAT_MINT_KEY });
    expect(ok.status).toBe(200);
    expect(c1.req).toBeDefined();

    const c2: { req?: Request } = {};
    const env2 = makeEnv({ captured: c2, oldRow: activeRow(), withPatMintKey: true });
    const bad = await rotateFetch(env2, { auth: INTERNAL_KEY });
    expect(bad.status).toBe(401);
    expect(c2.req).toBeUndefined();
  });

  // ── (b) happy path: mint new (same tenant+scope) + revoke old ─────────────
  it("(b) 200 + new token, old revoked, same tenant + scope (read-write)", async () => {
    const captured: { req?: Request } = {};
    const revokeCapture: { binds?: unknown[]; called?: boolean } = {};
    const env = makeEnv({ captured, oldRow: activeRow("read-write"), revokeCapture });
    const resp = await rotateFetch(env, { auth: INTERNAL_KEY, body: { pat_id: OLD_PAT_ID } });

    expect(resp.status).toBe(200);
    const out = (await resp.json()) as Record<string, unknown>;
    expect(out["token_plaintext"]).toBe(CANNED_MINT.token_plaintext);
    expect(out["pat_id"]).toBe(CANNED_MINT.pat_id); // NEW pat id
    expect(out["token_id"]).toBe(CANNED_MINT.token_id);
    expect(out["expires_ms"]).toBe(CANNED_MINT.expires_ms);
    expect(out["rotated_from"]).toBe(OLD_PAT_ID);
    expect(out["hash"]).toBeUndefined(); // never leak the Argon2id hash

    // The new mint inherited the OLD PAT's tenant + scope.
    const mintBody = (await captured.req!.json()) as Record<string, unknown>;
    expect(mintBody["tenant_id"]).toBe(TENANT);
    expect(mintBody["scopes"]).toBe("read-write");
    expect(typeof mintBody["ttl_seconds"]).toBe("number");
    expect(mintBody["ttl_seconds"] as number).toBeGreaterThan(0); // never 0 (no-expiry clamp)
    // principal_id is the SHA-256-derived UUID of the OLD pat_id (NOT the raw id).
    expect(mintBody["principal_id"]).not.toBe(OLD_PAT_ID);
    expect(mintBody["principal_id"]).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-/);

    // The OLD pat was soft-revoked: UPDATE bound with (now_ms, OLD_PAT_ID).
    expect(revokeCapture.called).toBe(true);
    expect(revokeCapture.binds![1]).toBe(OLD_PAT_ID);
    expect(typeof revokeCapture.binds![0]).toBe("number");
  });

  it("(b) preserves an admin-scoped PAT's scope verbatim", async () => {
    const captured: { req?: Request } = {};
    const env = makeEnv({ captured, oldRow: activeRow("admin") });
    const resp = await rotateFetch(env, { auth: INTERNAL_KEY });
    expect(resp.status).toBe(200);
    const mintBody = (await captured.req!.json()) as Record<string, unknown>;
    expect(mintBody["scopes"]).toBe("admin");
  });

  it("(b) NEVER lets a client trust header reach the mint route", async () => {
    const captured: { req?: Request } = {};
    const env = makeEnv({ captured, oldRow: activeRow() });
    await rotateFetch(env, { auth: INTERNAL_KEY });
    expect(captured.req!.headers.get("x-corelink-route-kind")).toBe("internal");
    expect(captured.req!.headers.get("authorization")).toBeNull();
  });

  // ── (c) unknown pat → 404 ─────────────────────────────────────────────────
  it("(c) 404 when the pat_id does not exist (never silently mints)", async () => {
    const captured: { req?: Request } = {};
    const revokeCapture: { binds?: unknown[]; called?: boolean } = {};
    const env = makeEnv({ captured, oldRow: null, revokeCapture });
    const resp = await rotateFetch(env, { auth: INTERNAL_KEY });
    expect(resp.status).toBe(404);
    expect(captured.req).toBeUndefined(); // never minted
    expect(revokeCapture.called).toBeUndefined(); // never revoked
  });

  // ── (d) already-revoked pat → 404 ─────────────────────────────────────────
  it("(d) 404 when the pat is already revoked", async () => {
    const captured: { req?: Request } = {};
    const revokeCapture: { binds?: unknown[]; called?: boolean } = {};
    const env = makeEnv({
      captured,
      oldRow: { tenant_id: TENANT, scope: "read-write", expires_ms: 1893456000000, revoked_at_ms: 1234 },
      revokeCapture,
    });
    const resp = await rotateFetch(env, { auth: INTERNAL_KEY });
    expect(resp.status).toBe(404);
    expect(captured.req).toBeUndefined();
    expect(revokeCapture.called).toBeUndefined();
  });

  // ── (e) mint fails → old NOT revoked (no zero-valid-PAT window) ───────────
  it("(e) propagates a mint failure and does NOT revoke the old pat", async () => {
    const captured: { req?: Request } = {};
    const revokeCapture: { binds?: unknown[]; called?: boolean } = {};
    // Container mint returns non-200 → mintScopedPat collapses to a 500.
    const env = makeEnv({ captured, oldRow: activeRow(), revokeCapture, mintStatus: 401 });
    const resp = await rotateFetch(env, { auth: INTERNAL_KEY });
    expect(resp.status).toBe(500); // mint error propagated
    expect(captured.req).toBeDefined(); // mint WAS attempted
    expect(revokeCapture.called).toBeUndefined(); // old PAT left INTACT
  });

  // ── (f) mint OK + revoke fails → 200 + new token + revoke_pending ─────────
  it("(f) revoke failure after a successful mint returns 200 + new token + revoke_pending:true", async () => {
    const captured: { req?: Request } = {};
    // Mint succeeds (200), but the old-pat revoke UPDATE throws (D1 transient).
    const env = makeEnv({ captured, oldRow: activeRow("read-write"), revokeThrows: true });
    // Unique pat_id ⇒ a fresh per-principal mint-throttle counter (the in-memory
    // burst backstop is module-scoped across this file).
    const uniqueId = "ffffffff-1111-2222-3333-444444444444";
    const resp = await rotateFetch(env, { auth: INTERNAL_KEY, body: { pat_id: uniqueId } });
    expect(resp.status).toBe(200); // the new PAT is returned, NOT thrown away
    const body = (await resp.json()) as {
      token_plaintext: string;
      rotated_from: string;
      revoke_pending: boolean;
    };
    expect(body.token_plaintext).toBe(CANNED_MINT.token_plaintext); // caller HAS the new credential
    expect(body.rotated_from).toBe(uniqueId);
    expect(body.revoke_pending).toBe(true); // old key may still be live → retry the idempotent revoke
    expect(captured.req).toBeDefined(); // mint WAS attempted
  });

  // ── (b) happy path sets revoke_pending:false ──────────────────────────────
  it("(b) a successful rotate sets revoke_pending:false", async () => {
    const env = makeEnv({ captured: {}, oldRow: activeRow("read-write") });
    const uniqueId = "bbbbbbbb-9999-8888-7777-666666666666";
    const resp = await rotateFetch(env, { auth: INTERNAL_KEY, body: { pat_id: uniqueId } });
    expect(resp.status).toBe(200);
    const body = (await resp.json()) as { revoke_pending: boolean };
    expect(body.revoke_pending).toBe(false);
  });

  // ── unmappable scope → 422 (preserve scope exactly; never escalate) ───────
  it("422 when the old scope is read-only (single mint authority cannot reproduce it)", async () => {
    const captured: { req?: Request } = {};
    const revokeCapture: { binds?: unknown[]; called?: boolean } = {};
    const env = makeEnv({ captured, oldRow: activeRow("read-only"), revokeCapture });
    const resp = await rotateFetch(env, { auth: INTERNAL_KEY });
    expect(resp.status).toBe(422);
    expect(captured.req).toBeUndefined(); // never minted (no escalation)
    expect(revokeCapture.called).toBeUndefined();
  });

  // ── body / method gates ────────────────────────────────────────────────────
  it("400 when pat_id is missing", async () => {
    const env = makeEnv({ oldRow: activeRow() });
    const resp = await rotateFetch(env, { auth: INTERNAL_KEY, body: {} });
    expect(resp.status).toBe(400);
  });

  it("400 when pat_id is empty", async () => {
    const env = makeEnv({ oldRow: activeRow() });
    const resp = await rotateFetch(env, { auth: INTERNAL_KEY, body: { pat_id: "" } });
    expect(resp.status).toBe(400);
  });

  it("405 on a non-POST method", async () => {
    const captured: { req?: Request } = {};
    const env = makeEnv({ captured, oldRow: activeRow() });
    const resp = await rotateFetch(env, { auth: INTERNAL_KEY, method: "GET" });
    expect(resp.status).toBe(405);
    expect(captured.req).toBeUndefined();
  });
});
