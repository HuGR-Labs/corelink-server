// WP-4 — CustomerClient auth-token wiring:
//   - `Authorization: Bearer <token>` is attached when `getToken` is provided
//     AND resolves to a non-null string.
//   - The header is ABSENT when `getToken` is omitted (E2E mock mode must keep
//     working byte-identical) or when it resolves to null.

import { describe, it, expect, vi } from "vitest";
import { CustomerClient, CustomerClientError } from "@/lib/customer-client";

interface Captured {
  url: string;
  method: string;
  headers: Record<string, string>;
}

function makeFetch(captured: Captured[], body: unknown = {}) {
  return vi.fn(async (url: string, init?: RequestInit) => {
    const h: Record<string, string> = {};
    new Headers(init?.headers).forEach((v, k) => {
      h[k.toLowerCase()] = v;
    });
    captured.push({ url, method: init?.method ?? "GET", headers: h });
    return new Response(JSON.stringify(body), {
      status: 200,
      headers: { "content-type": "application/json" },
    });
  }) as unknown as typeof fetch;
}

describe("CustomerClient auth token", () => {
  it("attaches Authorization: Bearer <token> when getToken resolves non-null", async () => {
    const captured: Captured[] = [];
    const getToken = vi.fn(async () => "sess_jwt_abc123");
    const client = new CustomerClient({
      baseUrl: "https://api.test",
      fetchImpl: makeFetch(captured),
      getToken,
    });
    await client.getOverview();
    expect(getToken).toHaveBeenCalledTimes(1);
    expect(captured).toHaveLength(1);
    expect(captured[0]!.url).toBe("https://api.test/v1/customer/overview");
    expect(captured[0]!.headers["authorization"]).toBe("Bearer sess_jwt_abc123");
  });

  it("attaches the token on POST mutations too", async () => {
    const captured: Captured[] = [];
    const client = new CustomerClient({
      baseUrl: "https://api.test",
      fetchImpl: makeFetch(captured, {
        pat: {
          pat_id: "pat_post",
          name: "ci",
          scopes: ["cache:r"],
          created_at: "2026-08-01T00:00:00Z",
        },
        token: "crl_pat_post_secret",
      }),
      getToken: async () => "sess_jwt_post",
    });
    await client.createPat({ name: "ci", scopes: ["cache:r"] });
    expect(captured[0]!.method).toBe("POST");
    expect(captured[0]!.headers["authorization"]).toBe("Bearer sess_jwt_post");
    // content-type still set alongside the auth header
    expect(captured[0]!.headers["content-type"]).toBe("application/json");
  });

  it("fetches a FRESH token per request (no caching across calls)", async () => {
    const captured: Captured[] = [];
    const getToken = vi
      .fn<() => Promise<string | null>>()
      .mockResolvedValueOnce("tok_1")
      .mockResolvedValueOnce("tok_2");
    const client = new CustomerClient({
      baseUrl: "https://api.test",
      fetchImpl: makeFetch(captured),
      getToken,
    });
    await client.getUsage();
    await client.getBilling();
    expect(getToken).toHaveBeenCalledTimes(2);
    expect(captured[0]!.headers["authorization"]).toBe("Bearer tok_1");
    expect(captured[1]!.headers["authorization"]).toBe("Bearer tok_2");
  });

  it("omits Authorization when getToken is not provided (E2E mock mode untouched)", async () => {
    const captured: Captured[] = [];
    const client = new CustomerClient({
      baseUrl: "https://api.test",
      fetchImpl: makeFetch(captured),
    });
    await client.getOverview();
    expect(captured[0]!.headers).not.toHaveProperty("authorization");
    // Byte-identical request headers vs. the pre-WP-4 client: content-type only.
    expect(captured[0]!.headers).toEqual({ "content-type": "application/json" });
  });

  it("omits Authorization when getToken resolves null (signed-out)", async () => {
    const captured: Captured[] = [];
    const getToken = vi.fn(async () => null);
    const client = new CustomerClient({
      baseUrl: "https://api.test",
      fetchImpl: makeFetch(captured),
      getToken,
    });
    await client.getOverview();
    expect(getToken).toHaveBeenCalledTimes(1);
    expect(captured[0]!.headers).not.toHaveProperty("authorization");
    expect(captured[0]!.headers).toEqual({ "content-type": "application/json" });
  });

  it("still raises CustomerClientError on non-2xx with the token attached", async () => {
    const fetchImpl = vi.fn(async () => new Response("nope", { status: 401 })) as unknown as typeof fetch;
    const client = new CustomerClient({
      baseUrl: "https://api.test",
      fetchImpl,
      getToken: async () => "expired_tok",
    });
    await expect(client.getOverview()).rejects.toBeInstanceOf(CustomerClientError);
    await expect(client.getOverview()).rejects.toMatchObject({ status: 401 });
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// WIRE-SHAPE contract. `request<T>()` does `(await res.json()) as T` — a CAST,
// not a parse. When the declared type and the real wire body disagree,
// TypeScript is satisfied and the caller silently receives `undefined` fields at
// runtime.
//
// These tests feed the client the EXACT body the Rust handler emits (copied from
// `crates/corelink-container/src/routes/customer.rs`, where that crate's own
// tests pin the same shape) — never the convenient flat shape. A fixture that
// returns a shape the server never sends is a gate that cannot fail.
// ─────────────────────────────────────────────────────────────────────────────
describe("CustomerClient wire-shape contract", () => {
  function respondWith(body: unknown, status = 200) {
    return vi.fn(
      async () =>
        new Response(JSON.stringify(body), {
          status,
          headers: { "content-type": "application/json" },
        }),
    ) as unknown as typeof fetch;
  }

  // `POST /v1/customer/team/invite` → 201 `{ "member": { … }, "invitation_token": "…" }`
  // (routes/customer.rs:961-969; shape asserted server-side at :2062).
  // Regression: the client bare-cast this envelope to `CustomerTeamMember`, so
  // `.email` was `undefined` and the team screen rendered "Invite sent to
  // undefined" in production (the copy shipped at the time) — while every mock returned a flat member, so no
  // gate could catch it.
  it("inviteTeam unwraps the { member } envelope the container actually sends", async () => {
    const client = new CustomerClient({
      baseUrl: "https://api.test",
      fetchImpl: respondWith(
        {
          member: {
            user_id: "0198f0e3-0000-7000-8000-000000000001",
            email: "alice@example.com",
            role: "member",
            joined_at: "",
            status: "invited",
          },
          invitation_token: "a".repeat(64),
        },
        201,
      ),
    });

    const member = await client.inviteTeam({ email: "alice@example.com", role: "member" });

    expect(member.member.email).toBe("alice@example.com");
    expect(member.member.user_id).toBe("0198f0e3-0000-7000-8000-000000000001");
    expect(member.member.status).toBe("invited");
    expect(member.invitation_token).toHaveLength(64);
  });

  // `POST /v1/customer/keys` → 201 `{ "pat": { … }, "token": "…" }`
  // (routes/customer/part-01.rs, the current container response envelope).
  // Regression: the client declared `CustomerPat & { token? }` and bare-cast the
  // envelope. PAT metadata became undefined even though the token survived.
  // The optional alias must also reach the shown-once reveal if received.
  it("createPat unwraps the live token envelope for the reveal shape", async () => {
    const client = new CustomerClient({
      baseUrl: "https://api.test",
      fetchImpl: respondWith(
        {
          pat: {
            pat_id: "0198f0e3-0000-7000-8000-0000000000aa",
            name: "ci-github",
            scopes: ["cache:r", "cache:w"],
            created_at: "2026-08-01T00:00:00Z",
            last_used_at: null,
            revoked_at: null,
          },
          token: "crl_pat_shown_once_secret",
        },
        201,
      ),
    });

    const pat = await client.createPat({ name: "ci-github", scopes: ["cache:r", "cache:w"] });

    expect(pat.pat_id).toBe("0198f0e3-0000-7000-8000-0000000000aa");
    expect(pat.name).toBe("ci-github");
    expect(pat.scopes).toEqual(["cache:r", "cache:w"]);
    expect(pat.created_at).toBe("2026-08-01T00:00:00Z");
    // The shown-once secret rides ALONGSIDE the row, not inside it.
    expect(pat.token).toBe("crl_pat_shown_once_secret");
    // The envelope key must not leak through — callers spread the row.
    expect(pat).not.toHaveProperty("pat");
  });

  it.each([
    ["missing PAT row", undefined],
    ["empty PAT row", {}],
    ["array PAT row", []],
    ["blank PAT ID", { pat_id: " ", name: "ci", scopes: ["cache:r"], created_at: "2026-08-01T00:00:00Z" }],
    ["missing name", { pat_id: "pat_1", scopes: ["cache:r"], created_at: "2026-08-01T00:00:00Z" }],
    ["missing created_at", { pat_id: "pat_1", name: "ci", scopes: ["cache:r"] }],
    ["non-array scopes", { pat_id: "pat_1", name: "ci", scopes: "cache:r", created_at: "2026-08-01T00:00:00Z" }],
    ["non-string scope", { pat_id: "pat_1", name: "ci", scopes: [7], created_at: "2026-08-01T00:00:00Z" }],
  ])("createPat rejects %s despite a valid plaintext token", async (_caseName, pat) => {
    const client = new CustomerClient({
      baseUrl: "https://api.test",
      fetchImpl: respondWith({ pat, token: "crl_pat_shown_once_secret" }, 201),
    });

    await expect(client.createPat({ name: "ci", scopes: ["cache:r"] }))
      .rejects.toMatchObject({ status: 502, message: "customer api response has invalid PAT metadata" });
  });

  it.each([
    ["missing", { pat: { pat_id: "pat_missing", name: "ci", scopes: ["cache:r"], created_at: "2026-08-01T00:00:00Z" } }],
    ["blank live token", { pat: { pat_id: "pat_blank", name: "ci", scopes: ["cache:r"], created_at: "2026-08-01T00:00:00Z" }, token: "" }],
    ["blank compatibility alias", { pat: { pat_id: "pat_blank_alias", name: "ci", scopes: ["cache:r"], created_at: "2026-08-01T00:00:00Z" }, token_plaintext: "   " }],
  ])("createPat fails closed when PAT plaintext is %s", async (_caseName, body) => {
    const client = new CustomerClient({
      baseUrl: "https://api.test",
      fetchImpl: respondWith(body, 201),
    });

    await expect(client.createPat({ name: "ci-github", scopes: ["cache:r"] }))
      .rejects.toMatchObject({
        status: 502,
        message: "customer api response missing nonblank token or token_plaintext",
      });
  });

  it("createPat accepts the optional token_plaintext compatibility alias", async () => {
    const client = new CustomerClient({
      baseUrl: "https://api.test",
      fetchImpl: respondWith({
        pat: {
          pat_id: "0198f0e3-0000-7000-8000-0000000000ad",
          name: "ci-github",
          scopes: ["cache:r"],
          created_at: "2026-08-01T00:00:00Z",
        },
        token_plaintext: "crl_pat_alias_secret",
      }, 201),
    });

    await expect(client.createPat({ name: "ci-github", scopes: ["cache:r"] }))
      .resolves.toMatchObject({ token: "crl_pat_alias_secret" });
  });

  it("createPat accepts both token fields when they agree", async () => {
    const client = new CustomerClient({
      baseUrl: "https://api.test",
      fetchImpl: respondWith({
        pat: { pat_id: "pat_agree", name: "ci-github", scopes: ["cache:r"], created_at: "2026-08-01T00:00:00Z" },
        token: "crl_pat_same_secret",
        token_plaintext: "crl_pat_same_secret",
      }, 201),
    });

    await expect(client.createPat({ name: "ci-github", scopes: ["cache:r"] }))
      .resolves.toMatchObject({ pat_id: "pat_agree", token: "crl_pat_same_secret" });
  });

  it("createPat fails closed when both token fields conflict", async () => {
    const client = new CustomerClient({
      baseUrl: "https://api.test",
      fetchImpl: respondWith({
        pat: { pat_id: "pat_conflict", name: "ci-github", scopes: ["cache:r"], created_at: "2026-08-01T00:00:00Z" },
        token: "crl_pat_live",
        token_plaintext: "crl_pat_conflicting_alias",
      }, 201),
    });

    await expect(client.createPat({ name: "ci-github", scopes: ["cache:r"] }))
      .rejects.toMatchObject({
        status: 502,
        message: "customer api response has conflicting token_plaintext and token fields",
      });
  });

  // `POST /v1/customer/keys/:id/revoke` → 200 `{ "pat": { … } }`
  // (routes/customer.rs:860-870). Latent today (both call sites discard the
  // return value) — pinned so the unwrap cannot be dropped again.
  it("revokePat unwraps the { pat } envelope the container actually sends", async () => {
    const client = new CustomerClient({
      baseUrl: "https://api.test",
      fetchImpl: respondWith({
        pat: {
          pat_id: "0198f0e3-0000-7000-8000-0000000000bb",
          name: "ci-github",
          scopes: ["cache:r"],
          created_at: "2026-08-01T00:00:00Z",
          last_used_at: null,
          revoked_at: "2026-08-02T00:00:00Z",
        },
      }),
    });

    const pat = await client.revokePat("0198f0e3-0000-7000-8000-0000000000bb");

    expect(pat.pat_id).toBe("0198f0e3-0000-7000-8000-0000000000bb");
    expect(pat.name).toBe("ci-github");
    expect(pat.revoked_at).toBe("2026-08-02T00:00:00Z");
    expect(pat).not.toHaveProperty("pat");
  });

  // `POST /v1/customer/account/delete` → 202 `{ "ok": true, "status": "…" }`
  // (routes/customer.rs:1082-1093). There is NO `request_id` on this wire — the
  // settings screen used to render "Erasure requested (undefined)". `status` is
  // `erasure_requested` (erasure queued) or `no_account` (idempotent no-op).
  it("deleteAccount returns the { ok, status } body — there is no request_id", async () => {
    const client = new CustomerClient({
      baseUrl: "https://api.test",
      fetchImpl: respondWith({ ok: true, status: "erasure_requested" }, 202),
    });

    const r = await client.deleteAccount();

    expect(r.ok).toBe(true);
    expect(r.status).toBe("erasure_requested");
    expect(r).not.toHaveProperty("request_id");
  });

  it("deleteAccount surfaces the idempotent no_account status", async () => {
    const client = new CustomerClient({
      baseUrl: "https://api.test",
      fetchImpl: respondWith({ ok: true, status: "no_account" }, 202),
    });

    const r = await client.deleteAccount();

    expect(r.status).toBe("no_account");
  });

  // `GET /v1/customer/team` → `{ "members": [ … ] }` (routes/customer.rs:895-903).
  // Already correct — pinned so it cannot regress into the envelope bug above.
  it("listTeam reads the { members } envelope", async () => {
    const client = new CustomerClient({
      baseUrl: "https://api.test",
      fetchImpl: respondWith({
        members: [
          {
            user_id: "user_owner",
            email: "—",
            role: "owner",
            joined_at: "2026-05-01T00:00:00Z",
            status: "active",
          },
        ],
      }),
    });

    const { members } = await client.listTeam();
    expect(members).toHaveLength(1);
    expect(members[0]!.role).toBe("owner");
  });
});
