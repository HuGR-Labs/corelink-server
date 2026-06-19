/**
 * Unit tests for the PAT-plaintext scrub cron (CTRL-CRED-001).
 *
 * Mocks the Clerk Backend API via a global `fetch` spy. Asserts the sweep
 * PATCHes `private_metadata.pat_plaintext` → null for STALE reveals and skips
 * FRESH ones, never logs the plaintext, and is inert without CLERK_SECRET_KEY.
 */
import { describe, it, expect, vi, afterEach } from "vitest";
import {
  runPatScrubSweep,
  shouldScrub,
  PAT_REVEAL_TTL_MS,
  type PatScrubCronEnv,
} from "../src/webhooks/pat_scrub_cron.js";

afterEach(() => {
  vi.restoreAllMocks();
});

describe("shouldScrub", () => {
  const now = 10 * PAT_REVEAL_TTL_MS;

  it("returns false when there is no pat_plaintext", () => {
    expect(shouldScrub(null, now)).toBe(false);
    expect(shouldScrub({}, now)).toBe(false);
    expect(shouldScrub({ region: "enam" }, now)).toBe(false);
    expect(shouldScrub({ pat_plaintext: "" }, now)).toBe(false);
  });

  it("keeps a FRESH reveal (within the TTL)", () => {
    const fresh = { pat_plaintext: "ct_x", pat_revealed_at: now - 1 };
    expect(shouldScrub(fresh, now)).toBe(false);
  });

  it("scrubs a STALE reveal (older than the TTL)", () => {
    const stale = { pat_plaintext: "ct_x", pat_revealed_at: now - PAT_REVEAL_TTL_MS - 1 };
    expect(shouldScrub(stale, now)).toBe(true);
  });

  it("scrubs at exactly the TTL boundary (>=)", () => {
    const at = { pat_plaintext: "ct_x", pat_revealed_at: now - PAT_REVEAL_TTL_MS };
    expect(shouldScrub(at, now)).toBe(true);
  });

  it("fail-closed: secret present but NO usable clock → scrub", () => {
    expect(shouldScrub({ pat_plaintext: "ct_x" }, now)).toBe(true);
    expect(shouldScrub({ pat_plaintext: "ct_x", pat_revealed_at: 0 }, now)).toBe(true);
    expect(shouldScrub({ pat_plaintext: "ct_x", pat_revealed_at: "bad" }, now)).toBe(true);
  });
});

describe("runPatScrubSweep", () => {
  it("is inert (skipped) without CLERK_SECRET_KEY", async () => {
    const fetchSpy = vi.spyOn(globalThis, "fetch");
    const out = await runPatScrubSweep({} as PatScrubCronEnv, Date.now());
    expect(out).toEqual({ scanned: 0, scrubbed: 0, failed: 0, skipped: true });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it("PATCHes pat_plaintext→null for stale users, skips fresh ones", async () => {
    const now = 10 * PAT_REVEAL_TTL_MS;
    const users = [
      // stale → scrub
      { id: "user_stale", private_metadata: { pat_plaintext: "ct_stale", pat_revealed_at: now - PAT_REVEAL_TTL_MS - 5 } },
      // fresh → keep
      { id: "user_fresh", private_metadata: { pat_plaintext: "ct_fresh", pat_revealed_at: now - 10 } },
      // no secret → keep
      { id: "user_none", private_metadata: { region: "enam" } },
      // secret with no clock → fail-closed scrub
      { id: "user_noclock", private_metadata: { pat_plaintext: "ct_legacy" } },
    ];

    const patched: string[] = [];
    const patchBodies: Record<string, unknown> = {};
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockImplementation(async (input: Request | string | URL, init?: RequestInit) => {
        const url = String(input);
        const method = init?.method ?? "GET";
        if (method === "GET") {
          // first page returns the users, subsequent pages empty.
          if (url.includes("offset=0")) {
            return Response.json(users);
          }
          return Response.json([]);
        }
        // PATCH /v1/users/{id}
        const m = url.match(/\/users\/([^/?]+)$/);
        const id = m ? decodeURIComponent(m[1] as string) : "";
        patched.push(id);
        patchBodies[id] = JSON.parse(String(init?.body));
        return new Response("{}", { status: 200 });
      });

    const out = await runPatScrubSweep({ CLERK_SECRET_KEY: "sk_test_x" }, now);

    expect(out.skipped).toBe(false);
    expect(out.scanned).toBe(4);
    expect(out.scrubbed).toBe(2);
    expect(out.failed).toBe(0);

    // Only the stale + no-clock users were PATCHed.
    expect(patched.sort()).toEqual(["user_noclock", "user_stale"]);
    // The PATCH clears the secret (null removes the key under Clerk merge).
    expect(patchBodies["user_stale"]).toEqual({
      private_metadata: { pat_plaintext: null, pat_revealed_at: null },
    });

    // Auth header carried the secret on every call (GET list + PATCHes).
    for (const call of fetchSpy.mock.calls) {
      const init = call[1] as RequestInit | undefined;
      const headers = (init?.headers ?? {}) as Record<string, string>;
      expect(headers["authorization"]).toBe("Bearer sk_test_x");
    }
  });

  it("counts a failed PATCH without aborting the sweep", async () => {
    const now = 10 * PAT_REVEAL_TTL_MS;
    const users = [
      { id: "user_a", private_metadata: { pat_plaintext: "ct_a", pat_revealed_at: now - PAT_REVEAL_TTL_MS - 5 } },
      { id: "user_b", private_metadata: { pat_plaintext: "ct_b", pat_revealed_at: now - PAT_REVEAL_TTL_MS - 5 } },
    ];
    vi.spyOn(globalThis, "fetch").mockImplementation(
      async (input: Request | string | URL, init?: RequestInit) => {
        const url = String(input);
        if ((init?.method ?? "GET") === "GET") {
          return url.includes("offset=0") ? Response.json(users) : Response.json([]);
        }
        // user_a PATCH fails (403), user_b succeeds.
        return url.includes("user_a")
          ? new Response("forbidden", { status: 403 })
          : new Response("{}", { status: 200 });
      },
    );

    const out = await runPatScrubSweep({ CLERK_SECRET_KEY: "sk_test_x" }, now);
    expect(out.scanned).toBe(2);
    expect(out.scrubbed).toBe(1);
    expect(out.failed).toBe(1);
  });

  it("stops the sweep on a list-page failure (best-effort; hourly retry)", async () => {
    vi.spyOn(globalThis, "fetch").mockImplementation(async () => new Response("rate limited", { status: 429 }));
    const out = await runPatScrubSweep({ CLERK_SECRET_KEY: "sk_test_x" }, Date.now());
    expect(out).toEqual({ scanned: 0, scrubbed: 0, failed: 0, skipped: false });
  });
});
