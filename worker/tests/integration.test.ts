/**
 * Integration-layer tests — end-to-end pipeline tests using the worker handler
 * directly with a mock DO that simulates full request → response round trips.
 *
 * These tests verify the full pipeline from HTTP entry → auth → route → DO →
 * response, using a configurable mock DO stub that can simulate different
 * upstream scenarios (success, 404, 502, etc.).
 *
 * For live wrangler dev smoke tests, see scripts/test-wrangler-smoke.sh which
 * runs `wrangler dev --local` and hits `curl http://localhost:8787/health`.
 */

import { describe, it, expect } from "vitest";
import type { D1Database } from "@cloudflare/workers-types";
import workerHandler from "../src/index.js";
import type { Env } from "../src/index.js";
import { TEST_PAT_SIGNING_KEY, mintTestPat } from "./setup.js";

// ──────────────────────────────────────────────────────────────────────────────
// Helpers
// ──────────────────────────────────────────────────────────────────────────────

// Canonical test PAT — a CRYPTOGRAPHICALLY VALID CoreLink PAT: its hmac_sig
// verifies under TEST_PAT_SIGNING_KEY (bound in makeEnvWithStub below), and the
// D1 mock recognises its token_id (AAAAAAAAAAAAAAAA) and returns a non-expired
// row. So "reaches DO" tests clear parse + HMAC + D1 lookup and hit the stub —
// rather than short-circuiting on a missing signing key (503) the way an
// all-placeholder sig used to. mintTestPat() defaults reproduce this token_id.
// Format: corelink_pat_<16-char-Crockford-b32>.<43-char-base64url>.<22-char-base64url>
const TEST_TOKEN_ID = "AAAAAAAAAAAAAAAA";
const TEST_TENANT_ID = "00000000-0000-0000-0000-000000000001";
const VALID_TOKEN = await mintTestPat({ tokenId: TEST_TOKEN_ID }); // 96 chars, valid sig

/**
 * Build a minimal D1 mock that recognises the test token_id and returns a
 * non-expired tenant row. All other token_ids return null (→ 401).
 */
function makeTestD1(): D1Database {
  return {
    prepare: (_sql: string) => ({
      bind: (...args: unknown[]) => ({
        first: async <T>() => {
          const tokenId = args[0] as string;
          if (tokenId === TEST_TOKEN_ID) {
            return { tenant_id: TEST_TENANT_ID, expires_ms: Date.now() + 3_600_000 } as T;
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
    withSession: () => null as never,
    dump: async () => new ArrayBuffer(0),
  } as unknown as D1Database;
}

function makeCtx(): ExecutionContext {
  return {
    waitUntil: () => {},
    passThroughOnException: () => {},
  } as unknown as ExecutionContext;
}

/** Create an env with a configurable DO stub. */
function makeEnvWithStub(
  statusCode: number,
  body: unknown,
  extraHeaders?: Record<string, string>,
): Env {
  const stub = {
    fetch: async (req: Request): Promise<Response> => {
      const respHeaders: Record<string, string> = {
        "Content-Type": "application/json",
        "X-Request-Id": req.headers.get("x-request-id") ?? "stub",
        ...extraHeaders,
      };
      return new Response(JSON.stringify(body), {
        status: statusCode,
        headers: respHeaders,
      });
    },
  };

  return {
    CORELINK_SERVER: {
      idFromName: (_n: string) => ({ toString: () => "id" }),
      get: (_id: unknown) => stub,
      idFromString: (_s: string) => ({ toString: () => "id" }),
      newUniqueId: () => ({ toString: () => "unique-id" }),
      jurisdiction: (_j: string) => ({}) as DurableObjectNamespace,
    } as unknown as DurableObjectNamespace,
    ENVIRONMENT: "test",
    CONFIG_DB: makeTestD1(),
    // Bind the shared valid test signing key so the native-plane PAT gate
    // (extractAuth) reaches real HMAC + D1 auth instead of failing CLOSED with
    // pat_signing_key_absent (503) — the harness-honesty fix (House #2).
    PAT_SIGNING_KEY: TEST_PAT_SIGNING_KEY,
  };
}

async function fetch_(
  url: string,
  init?: RequestInit,
  envOverride?: Partial<Env>,
): Promise<Response> {
  const req = new Request(url, init);
  const env = { ...makeEnvWithStub(200, { ok: true }), ...envOverride };
  return workerHandler.fetch!(req, env, makeCtx());
}

// ──────────────────────────────────────────────────────────────────────────────
// Full pipeline smoke
// ──────────────────────────────────────────────────────────────────────────────

describe("full pipeline smoke", () => {
  it("GET /health returns 200 with env field", async () => {
    const resp = await fetch_("http://localhost/health");
    expect(resp.status).toBe(200);
    const body = await resp.json() as { status: string; env: string };
    expect(body.status).toBe("ok");
    expect(typeof body.env).toBe("string");
  });

  it("GET /health does NOT require auth", async () => {
    const resp = await fetch_("http://localhost/health");
    expect(resp.status).toBe(200);
  });

  it("OCI v2 pass-through — Worker forwards to the _oci DO and returns its response verbatim", async () => {
    // PR #169: /v2/* is a pure pass-through. The Worker does NOT auth-gate it;
    // it forwards RAW to the _oci DO and returns whatever the DO returns. Here
    // the stub returns 200 + a DO-supplied header, both of which pass through.
    const env = makeEnvWithStub(200, { ok: true }, { "x-corelink-from-do": "1" });
    const resp = await workerHandler.fetch!(
      // No Authorization needed at the Worker — OCI auth happens in the container.
      new Request("http://localhost/v2/"),
      env,
      makeCtx(),
    );
    expect(resp.status).toBe(200);
    // The DO's response (status + headers) is returned verbatim.
    expect(resp.headers.get("x-corelink-from-do")).toBe("1");
  });

  it("authenticated REAPI v2 — pipeline reaches DO", async () => {
    const env = makeEnvWithStub(200, { blobs: [] });
    const resp = await workerHandler.fetch!(
      new Request(`http://localhost/api/v2/${TEST_TENANT_ID}/blobs`, {
        headers: { Authorization: `Bearer ${VALID_TOKEN}` },
      }),
      env,
      makeCtx(),
    );
    expect(resp.status).toBe(200);
  });

  it("authenticated npm — pipeline reaches DO", async () => {
    const env = makeEnvWithStub(200, { name: "my-package" });
    const resp = await workerHandler.fetch!(
      new Request(`http://localhost/npm/${TEST_TENANT_ID}/my-package`, {
        headers: { Authorization: `Bearer ${VALID_TOKEN}` },
      }),
      env,
      makeCtx(),
    );
    expect(resp.status).toBe(200);
  });

  it("authenticated cargo — pipeline reaches DO", async () => {
    const env = makeEnvWithStub(200, { crates: [] });
    const resp = await workerHandler.fetch!(
      new Request(`http://localhost/cargo/${TEST_TENANT_ID}/api/v1/crates/my-crate`, {
        headers: { Authorization: `Bearer ${VALID_TOKEN}` },
      }),
      env,
      makeCtx(),
    );
    expect(resp.status).toBe(200);
  });

  it("brew path reaches DO", async () => {
    const env = makeEnvWithStub(200, { formulae: [] });
    const resp = await workerHandler.fetch!(
      new Request(`http://localhost/brew/${TEST_TENANT_ID}/api/formula`, {
        headers: { Authorization: `Bearer ${VALID_TOKEN}` },
      }),
      env,
      makeCtx(),
    );
    expect(resp.status).toBe(200);
  });

  it("pip path reaches DO", async () => {
    const env = makeEnvWithStub(200, { packages: [] });
    const resp = await workerHandler.fetch!(
      new Request(`http://localhost/pip/${TEST_TENANT_ID}/simple/my-pkg`, {
        headers: { Authorization: `Bearer ${VALID_TOKEN}` },
      }),
      env,
      makeCtx(),
    );
    expect(resp.status).toBe(200);
  });
});

// ──────────────────────────────────────────────────────────────────────────────
// OCI Distribution Spec conformance — owned by the CONTAINER (pass-through)
//
// PR #169: the Worker no longer performs OCI auth or builds OCI envelopes. The
// container emits the Docker-Distribution-Api-Version header, the 401 +
// Www-Authenticate challenge, and the error envelopes. These tests therefore
// assert the WORKER's job: route /v2/* + /token to the _oci DO and forward the
// container's response VERBATIM (here a configurable stub stands in for it).
// ──────────────────────────────────────────────────────────────────────────────

describe("OCI spec conformance — pass-through to the container", () => {
  it("forwards the container's Docker-Distribution-Api-Version header verbatim", async () => {
    // The container sets the OCI version header; the Worker forwards it unchanged.
    const env = makeEnvWithStub(401, { errors: [{ code: "UNAUTHORIZED", message: "auth required" }] }, {
      "Docker-Distribution-Api-Version": "registry/2.0",
      "WWW-Authenticate": 'Bearer realm="https://corelink.test/token"',
    });
    const resp = await workerHandler.fetch!(new Request("http://localhost/v2/"), env, makeCtx());
    expect(resp.headers.get("docker-distribution-api-version")).toBe("registry/2.0");
  });

  it("forwards the container's 401 + errors array + Www-Authenticate challenge verbatim", async () => {
    const env = makeEnvWithStub(401, { errors: [{ code: "UNAUTHORIZED", message: "auth required" }] }, {
      "Docker-Distribution-Api-Version": "registry/2.0",
      "WWW-Authenticate": 'Bearer realm="https://corelink.test/token"',
    });
    const resp = await workerHandler.fetch!(new Request("http://localhost/v2/"), env, makeCtx());
    expect(resp.status).toBe(401);
    expect(resp.headers.get("www-authenticate")).toContain("Bearer realm=");
    const body = await resp.json() as { errors: Array<{ code: string; message: string }> };
    expect(Array.isArray(body.errors)).toBe(true);
    expect(body.errors[0]?.code).toBe("UNAUTHORIZED");
  });

  it("the Worker itself does NOT 401 an unauthenticated /v2/ — it forwards to the _oci DO", async () => {
    // With a 200-returning stub (no container-issued challenge), the Worker must
    // pass the 200 straight through: proof it did not inject its own auth gate.
    const resp = await fetch_("http://localhost/v2/");
    expect(resp.status).toBe(200);
  });

  it("GET /v2 (no trailing slash) also resolves to the pass-through (not 404)", async () => {
    const resp = await fetch_("http://localhost/v2");
    expect(resp.status).not.toBe(404);
    // Default stub returns 200 — the route matched the OCI pass-through, not the
    // not_found arm, and the Worker added no OCI envelope of its own.
    expect(resp.status).toBe(200);
    const body = await resp.json() as Record<string, unknown>;
    expect(body["errors"]).toBeUndefined();
  });

  it("GET /token resolves to the pass-through (OCI second leg), not the PAT gate", async () => {
    // /token carries OCI Basic auth; it must reach the _oci DO, never the PAT
    // gate (which would 401 a non-PAT credential). Default stub → 200.
    const resp = await fetch_("http://localhost/token", {
      headers: { Authorization: "Basic dXNlcjpwYXNz" },
    });
    expect(resp.status).toBe(200);
  });
});

// ──────────────────────────────────────────────────────────────────────────────
// Stripe billing webhook pass-through (LAUNCH-BLOCKER fix)
//
// POST /v1/billing/stripe-webhook authenticates via the `Stripe-Signature` HMAC
// header, NOT a Bearer PAT. The Worker forwards it to the shared _system DO with
// NO PAT gate (pre-fix it 401'd in the reapi_v1 bucket), preserving the raw body
// + Stripe-Signature so the container verifies the HMAC over the signed bytes.
// ──────────────────────────────────────────────────────────────────────────────

describe("Stripe billing webhook pass-through to the container", () => {
  const RAW_BODY = JSON.stringify({ id: "evt_1", type: "checkout.session.completed" });
  const SIG = "t=1700000000,v1=abc123abc123abc123abc123abc123abc123abc123abc123abc123abc123abcd";

  it("forwards the webhook to the _system DO and returns its response verbatim (no 401)", async () => {
    // The DO stub returns 200 — proof the Worker did NOT inject a PAT auth gate.
    const env = makeEnvWithStub(200, { received: true }, { "x-corelink-from-do": "1" });
    const resp = await workerHandler.fetch!(
      new Request("http://localhost/v1/billing/stripe-webhook", {
        method: "POST",
        headers: { "Stripe-Signature": SIG },
        body: RAW_BODY,
      }),
      env,
      makeCtx(),
    );
    expect(resp.status).toBe(200);
    expect(resp.headers.get("x-corelink-from-do")).toBe("1");
  });

  it("preserves the RAW body bytes + Stripe-Signature on the forwarded request", async () => {
    // Capture what the Worker forwards: the container re-hashes these exact bytes.
    let captured: { body: string; sig: string | null } | null = null;
    const capturingEnv: Partial<Env> = {
      CORELINK_SERVER: {
        idFromName: (_n: string) => ({ toString: () => "id" }),
        get: () => ({
          fetch: async (req: Request): Promise<Response> => {
            captured = { body: await req.text(), sig: req.headers.get("stripe-signature") };
            return new Response(JSON.stringify({ ok: true }), {
              status: 200,
              headers: { "Content-Type": "application/json" },
            });
          },
        }),
        idFromString: (_s: string) => ({ toString: () => "id" }),
        newUniqueId: () => ({ toString: () => "unique-id" }),
        jurisdiction: (_j: string) => ({}) as DurableObjectNamespace,
      } as unknown as DurableObjectNamespace,
    };
    const resp = await fetch_(
      "http://localhost/v1/billing/stripe-webhook",
      { method: "POST", headers: { "Stripe-Signature": SIG }, body: RAW_BODY },
      capturingEnv,
    );
    expect(resp.status).toBe(200);
    expect(captured).not.toBeNull();
    expect(captured!.body).toBe(RAW_BODY);
    expect(captured!.sig).toBe(SIG);
  });

  it("a DO 500 on the webhook path is forwarded verbatim (Worker does not reshape it)", async () => {
    const env = makeEnvWithStub(500, { error: "transient backend error" });
    const resp = await workerHandler.fetch!(
      new Request("http://localhost/v1/billing/stripe-webhook", {
        method: "POST",
        headers: { "Stripe-Signature": SIG },
        body: RAW_BODY,
      }),
      env,
      makeCtx(),
    );
    expect(resp.status).toBe(500);
  });
});

// ──────────────────────────────────────────────────────────────────────────────
// 404 timing-pad path
// ──────────────────────────────────────────────────────────────────────────────

describe("timing-pad on 404", () => {
  it("404 response completes in reasonable time (< 2s ceiling)", async () => {
    // The timing pad targets ~80ms. In tests we don't want to wait the full
    // pad time — but we verify the function completes (doesn't hang forever).
    // We use a short timeout: the pad uses Date.now() comparison, so in the
    // test it will see requestStart as ~0ms ago and compute a positive sleep.
    // We just verify it resolves without hanging.
    const start = Date.now();
    const resp = await fetch_("http://localhost/completely-unknown");
    const elapsed = Date.now() - start;
    expect(resp.status).toBe(404);
    // Must complete within 2s (the pad target is ~80ms; we allow 2s margin)
    expect(elapsed).toBeLessThan(2000);
  });
});

// ──────────────────────────────────────────────────────────────────────────────
// DO 404 response — timing-pad applied
// ──────────────────────────────────────────────────────────────────────────────

describe("OCI DO 404 pass-through", () => {
  it("container 404 (BLOB_UNKNOWN) on an OCI route is passed through with x-request-id", async () => {
    // OCI is pass-through: the Worker forwards the container's 404 envelope as-is.
    // (No Worker auth gate, and no timing-pad on the OCI path — that pad only
    // applies to the PAT-gated DO forward.) No Authorization needed at the edge.
    const env = makeEnvWithStub(404, { errors: [{ code: "BLOB_UNKNOWN", message: "not found", detail: null }] });
    const resp = await workerHandler.fetch!(
      new Request(`http://localhost/v2/myrepo/blobs/sha256:deadbeef`),
      env,
      makeCtx(),
    );
    expect(resp.status).toBe(404);
    expect(resp.headers.get("x-request-id")).not.toBeNull();
    const body = await resp.json() as { errors: Array<{ code: string }> };
    expect(body.errors[0]?.code).toBe("BLOB_UNKNOWN");
  });
});

// ──────────────────────────────────────────────────────────────────────────────
// Security headers on all responses
// ──────────────────────────────────────────────────────────────────────────────

describe("security headers", () => {
  it("X-Request-Id present on every response", async () => {
    const paths = [
      "http://localhost/health",
      "http://localhost/v2/",
      "http://localhost/api/v2/t/p",
      "http://localhost/unknown-xyz",
    ];
    for (const path of paths) {
      const resp = await fetch_(path);
      expect(resp.headers.get("x-request-id"), `missing x-request-id for ${path}`).not.toBeNull();
    }
  });

  it("Authorization header value NOT reflected in any response body", async () => {
    const secretToken = "SECRETTOKEN456" + "x".repeat(50);
    const resp = await fetch_("http://localhost/api/v2/t/path", {
      headers: { Authorization: `Bearer ${secretToken}` },
    });
    const text = await resp.text();
    expect(text).not.toContain(secretToken);
    expect(text).not.toContain("SECRETTOKEN456");
  });
});

// ──────────────────────────────────────────────────────────────────────────────
// Request method coverage
// ──────────────────────────────────────────────────────────────────────────────

describe("HTTP method handling", () => {
  // POST/PUT/DELETE on an authenticated PAT route reach the DO. (OCI /v2/* is a
  // pass-through with no Worker auth gate — covered separately above — so the
  // "authenticated method reaches DO" contract is exercised on a REAPI route.)
  it("POST to an authenticated REAPI route reaches DO", async () => {
    const env = makeEnvWithStub(201, { upload_url: "/v2/repo/blobs/uploads/uuid" });
    const resp = await workerHandler.fetch!(
      new Request(`http://localhost/api/v2/${TEST_TENANT_ID}/blobs/uploads/`, {
        method: "POST",
        headers: { Authorization: `Bearer ${VALID_TOKEN}` },
      }),
      env,
      makeCtx(),
    );
    expect(resp.status).toBe(201);
  });

  it("PUT to an authenticated REAPI route reaches DO", async () => {
    const env = makeEnvWithStub(200, { digest: "sha256:abc" });
    const resp = await workerHandler.fetch!(
      new Request(`http://localhost/api/v2/${TEST_TENANT_ID}/manifests/latest`, {
        method: "PUT",
        headers: {
          Authorization: `Bearer ${VALID_TOKEN}`,
          "Content-Type": "application/vnd.oci.image.manifest.v1+json",
        },
        body: JSON.stringify({ schemaVersion: 2 }),
      }),
      env,
      makeCtx(),
    );
    expect(resp.status).toBe(200);
  });

  it("OCI methods (PUT/DELETE) are forwarded to the _oci DO verbatim, not auth-gated by the Worker", async () => {
    // The Worker forwards the request method + path to the container, which does
    // its own method/auth handling. Capture the forwarded method to prove the
    // pass-through (no Worker 401 for an unauthenticated OCI DELETE).
    let capturedMethod: string | null = null;
    const capturingEnv: Partial<Env> = {
      CORELINK_SERVER: {
        idFromName: (name: string) => ({ toString: () => `do-${name}` }),
        get: () => ({
          fetch: async (req: Request): Promise<Response> => {
            capturedMethod = req.method;
            return new Response(JSON.stringify({ ok: true }), {
              status: 202,
              headers: { "Content-Type": "application/json" },
            });
          },
        }),
        idFromString: (_s: string) => ({ toString: () => "id" }),
        newUniqueId: () => ({ toString: () => "unique-id" }),
        jurisdiction: (_j: string) => capturingEnv.CORELINK_SERVER,
      } as unknown as DurableObjectNamespace,
    };
    // No Authorization header — the Worker must NOT 401; it forwards verbatim.
    const resp = await fetch_("http://localhost/v2/repo/manifests/latest", { method: "DELETE" }, capturingEnv);
    expect(capturedMethod).toBe("DELETE");
    // The container's status is returned verbatim (no Worker-synthesized 401).
    expect(resp.status).toBe(202);
  });
});
