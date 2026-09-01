/**
 * B-081 — prove the Worker→container boundary, not merely the presence of an
 * env name somewhere in the source tree.
 *
 * The Rust verifier already accepts current/PREV/NEW. The defect was that the
 * Durable Object omitted PREV/NEW from the actual `container.start({ env })`
 * artifact. These tests observe that call directly. Removing either mapping,
 * swapping its source, or replacing the empty default makes a test fail.
 */
import { describe, expect, it, vi } from "vitest";
import { readFile } from "node:fs/promises";
import { CoreLinkServer } from "../src/durable_object.js";
import type { Env } from "../src/index.js";

type StartOptions = {
  env?: Record<string, string>;
};

function makeState(start: (options: StartOptions) => void, doName = "pat-rotation-forward-test"): DurableObjectState {
  const storage = new Map<string, unknown>();
  let running = false;
  const container = {
    get running(): boolean {
      return running;
    },
    start: (options: StartOptions) => {
      start(options);
      running = true;
    },
    destroy: vi.fn(() => {
      running = false;
    }),
    getTcpPort: () => ({
      fetch: async () => new Response("ok", { status: 200 }),
    }),
    setInactivityTimeout: vi.fn(async () => {}),
    monitor: () => new Promise<void>(() => {}),
  };

  return {
    id: {
      toString: () => doName,
      name: doName,
      equals: () => false,
    } as DurableObjectId,
    storage: {
      get: async (key: string) => storage.get(key),
      put: async (key: string, value: unknown) => {
        storage.set(key, value);
      },
      delete: async (key: string) => storage.delete(key),
      list: async () => new Map(storage),
      getAlarm: async () => null,
      setAlarm: async () => {},
      deleteAlarm: async () => {},
      transaction: async (fn: (txn: DurableObjectTransaction) => Promise<void>) =>
        fn({} as DurableObjectTransaction),
      deleteAll: async () => storage.clear(),
    } as unknown as DurableObjectStorage,
    container,
    waitUntil: () => {},
    blockConcurrencyWhile: async <T>(fn: () => Promise<T>): Promise<T> => fn(),
  } as unknown as DurableObjectState;
}

async function capturedStartEnv(
  overrides: Partial<Env>,
  doName = "tenant-pat-rotation",
): Promise<Record<string, string>> {
  const start = vi.fn<(options: StartOptions) => void>();
  const state = makeState(start, doName);
  const env = {
    CORELINK_SERVER: {} as DurableObjectNamespace,
    ENVIRONMENT: "test",
    PAGERDUTY_ROUTING_KEY: "",
    ...overrides,
  } as Env;
  const server = new CoreLinkServer(state, env);
  (server as unknown as { lifecycleState: Record<string, unknown> }).lifecycleState = {
    containerStatus: "stopped",
    tenantId: "tenant-pat-rotation",
    coldStartCount: 0,
  };

  const result = await (
    server as unknown as {
      startContainer: (requestId: string) => Promise<{ ok: boolean }>;
    }
  ).startContainer("req-pat-rotation");

  expect(result.ok).toBe(true);
  expect(start).toHaveBeenCalledOnce();
  const options = start.mock.calls[0]?.[0];
  expect(options?.env).toBeDefined();
  return options?.env ?? {};
}

describe("B-081 PAT rotation siblings reach container.start", () => {
  it("forwards PREV and NEW byte-for-byte from the Worker env", async () => {
    const env = await capturedStartEnv({
      PAT_SIGNING_KEY_PREV: "11".repeat(32),
      PAT_SIGNING_KEY_NEW: "22".repeat(32),
    });

    expect(env.PAT_SIGNING_KEY_PREV).toBe("11".repeat(32));
    expect(env.PAT_SIGNING_KEY_NEW).toBe("22".repeat(32));
  });

  it("forwards absent optional siblings as EMPTY, never a non-empty placeholder", async () => {
    const env = await capturedStartEnv({});

    expect(env.PAT_SIGNING_KEY_PREV).toBe("");
    expect(env.PAT_SIGNING_KEY_NEW).toBe("");
  });

  it("makes the shared PAT-authenticating _oci container a mandatory rotation member", async () => {
    const runbook = await readFile(
      new URL("../../specs/_runbooks/RB-PAT-SIGNING-KEY-ROTATION.md", import.meta.url),
      "utf8",
    );
    const match = runbook.match(/```json rotation-population-contract\n([\s\S]*?)\n```/);
    expect(match?.[1]).toBeDefined();
    const contract = JSON.parse(match?.[1] ?? "{}") as {
      all_pat_authenticating_instance_classes?: Array<{
        name?: string;
        per_environment?: string;
        routes?: string[];
      }>;
      not_population_evidence?: string[];
    };

    expect(contract.all_pat_authenticating_instance_classes).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ name: "active-per-tenant", per_environment: "enumerated" }),
        expect.objectContaining({
          name: "_oci",
          per_environment: "exactly-one",
          routes: ["/token", "/v2/*"],
        }),
      ]),
    );
    expect(contract.not_population_evidence).toContain("_system");

    const env = await capturedStartEnv(
      { PAT_SIGNING_KEY_PREV: "33".repeat(32), PAT_SIGNING_KEY_NEW: "44".repeat(32) },
      "_oci",
    );
    expect(env.PAT_SIGNING_KEY_PREV).toBe("33".repeat(32));
    expect(env.PAT_SIGNING_KEY_NEW).toBe("44".repeat(32));
  });
});
