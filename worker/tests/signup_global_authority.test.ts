import type { DurableObjectNamespace } from "@cloudflare/workers-types";
import { describe, expect, it } from "vitest";
import { baseHandler } from "../src/index_fetch.js";
import type { Env } from "../src/index.js";
import { CoreLinkServer, makeEnv, makeMockState } from "./durable_object_part2_test_helpers.js";

function ctx(): ExecutionContext {
  return {
    waitUntil: () => {},
    passThroughOnException: () => {},
  } as unknown as ExecutionContext;
}

function namespace(fetcher: (request: Request) => Promise<Response>): DurableObjectNamespace {
  const stub = { fetch: fetcher };
  return {
    idFromName: (_name: string) => ({ toString: () => "anonymous" }),
    get: (_id: unknown) => stub,
    idFromString: (_id: string) => ({ toString: () => "anonymous" }),
    newUniqueId: () => ({ toString: () => "anonymous" }),
  } as unknown as DurableObjectNamespace;
}

describe("pilot signup global authority", () => {
  it("meters one IP across IAD plus every public regional entry point", async () => {
    let nowMs = 1_700_000_000_000;
    const authority = new CoreLinkServer(makeMockState("global-signup"), makeEnv(), () => nowMs);
    await new Promise<void>((resolve) => setTimeout(resolve, 5));
    const authorityCalls: string[] = [];
    const globalBinding = {
      fetch: async (request: Request) => {
        authorityCalls.push(new URL(request.url).pathname);
        return authority.fetch(request);
      },
    };
    const localFallback = namespace(async () =>
      new Response(JSON.stringify({ error: "regional-local-do-used" }), { status: 500 }),
    );
    const globalNamespace = namespace((request) => authority.fetch(request));
    const regions = ["iad", "sam", "lhr", "nrt", "syd"];

    const send = async (region: string, path: string) => {
      const env = {
        ...makeEnv(),
        R2_CAS_REGION: region,
        CORELINK_SERVER: region === "iad" ? globalNamespace : localFallback,
        ...(region === "iad" ? {} : { PROD_IAD: globalBinding }),
      } as Env;
      return baseHandler.fetch!(
        new Request(`https://${region}.example${path}`, {
          method: "POST",
          headers: { "cf-connecting-ip": "203.0.113.55" },
        }),
        env,
        ctx(),
      );
    };

    // One request through each of the five public regional entry points shares
    // the IAD authority. A per-region namespace mutant would admit all six.
    for (const region of regions) {
      expect((await send(region, `/v1/signup/pilot/token-${region}`)).status).toBe(503);
    }
    expect((await send("lhr", "/v1/signup/pilot/token-six"))).toMatchObject({ status: 429 });
    expect(authorityCalls).toHaveLength(5);

    // The unrelated signup flow remains local and does not spend the pilot
    // bucket or cross the global binding.
    expect((await send("lhr", "/v1/signup/other"))).toMatchObject({ status: 500 });
    expect(authorityCalls).toHaveLength(5);

    nowMs += 60 * 60 * 1000;
    expect((await send("syd", "/v1/signup/pilot/after-window"))).toMatchObject({ status: 503 });
  });

  it("fails closed when a regional pilot authority binding is absent", async () => {
    const local = namespace(async () =>
      new Response(JSON.stringify({ error: "regional-local-do-used" }), { status: 500 }),
    );
    const response = await baseHandler.fetch!(
      new Request("https://lhr.example/v1/signup/pilot/token", {
        method: "POST",
        headers: { "cf-connecting-ip": "198.51.100.7" },
      }),
      { ...makeEnv(), R2_CAS_REGION: "lhr", CORELINK_SERVER: local } as Env,
      ctx(),
    );
    expect(response.status).toBe(503);
    expect(await response.json()).toMatchObject({ error: "SERVICE_UNAVAILABLE" });
  });
});
