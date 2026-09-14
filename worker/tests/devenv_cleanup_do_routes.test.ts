import { afterEach, describe, expect, it, vi } from "vitest";

const { drainDevenv, drainRunner } = vi.hoisted(() => ({
  drainDevenv: vi.fn(),
  drainRunner: vi.fn(),
}));

vi.mock("../src/lib/devenv_cleanup.js", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../src/lib/devenv_cleanup.js")>();
  return { ...actual, drainDevenvOperations: drainDevenv };
});
vi.mock("../src/lib/runner_credential_obligation.js", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../src/lib/runner_credential_obligation.js")>();
  return { ...actual, drainRunnerOperations: drainRunner };
});

import { CoreLinkServer } from "../src/durable_object.js";
import type { Env } from "../src/index.js";
import { IDLE_MS, makeReaperDO } from "./durable_object_part2_test_helpers.js";

const AUTH_KEY = "r".repeat(40);
const OPERATION_ID = "00000000-0000-4000-8000-000000000001";
const TENANT_ID = "00000000-0000-4000-8000-000000000002";

interface Harness {
  do_: CoreLinkServer;
  saved: Map<string, unknown>;
  alarms: number[];
  schemaReads: ReturnType<typeof vi.fn>;
  inserts: ReturnType<typeof vi.fn>;
}

function makeHarness({ system = true, schemaFails = false }: { system?: boolean; schemaFails?: boolean } = {}): Harness {
  const saved = new Map<string, unknown>();
  const alarms: number[] = [];
  const schemaReads = vi.fn(async () => {
    if (schemaFails) throw new Error("D1 unavailable");
    return { results: [] };
  });
  const inserts = vi.fn(async () => ({ success: true, meta: { changes: 1 } }));
  const storage = {
    get: async (key: string) => saved.get(key),
    put: async (key: string, value: unknown) => { saved.set(key, value); },
    delete: async (key: string) => { saved.delete(key); },
    list: async <T>({ prefix = "", limit = Number.MAX_SAFE_INTEGER }: { prefix?: string; limit?: number } = {}) =>
      new Map([...saved].filter(([key]) => key.startsWith(prefix)).slice(0, limit)) as Map<string, T>,
    getAlarm: async () => null,
    setAlarm: async (at: number) => { alarms.push(at); },
    transaction: async (fn: (txn: typeof storage) => Promise<void>) => fn(storage),
  };
  const systemId = "_system";
  const stateId = system ? systemId : "tenant-do";
  const state = {
    id: {
      toString: () => stateId,
      equals: (other: DurableObjectId) => other.toString() === stateId,
    },
    storage,
    container: undefined,
    blockConcurrencyWhile: async <T>(fn: () => Promise<T>) => fn(),
  } as unknown as DurableObjectState;
  const db = {
    prepare: (sql: string) => ({
      all: schemaReads,
      bind: () => ({ run: inserts }),
    }),
  } as unknown as D1Database;
  const env = {
    CORELINK_SERVER: { idFromName: () => ({ toString: () => systemId }) },
    CORELINK_RUNNER_MINT_AUTH_KEY: AUTH_KEY,
    CONFIG_DB: db,
    ENVIRONMENT: "test",
    PAGERDUTY_ROUTING_KEY: "",
  } as unknown as Env;
  return { do_: new CoreLinkServer(state, env), saved, alarms, schemaReads, inserts };
}

function prepareRequest(body: unknown, options: { auth?: string; method?: string } = {}): Request {
  return new Request("https://do.test/_do/devenv-cleanup/prepare", {
    method: options.method ?? "POST",
    headers: {
      "content-type": "application/json",
      ...(options.auth === undefined ? {} : { "x-corelink-internal-auth": options.auth }),
    },
    body: JSON.stringify(body),
  });
}

afterEach(() => {
  drainDevenv.mockReset();
  drainRunner.mockReset();
});

describe("DevEnv cleanup DO boundary", () => {
  it("rejects malformed, unauthenticated, and non-system prepare requests before creating an obligation", async () => {
    const unconfigured = makeHarness();
    const unconfiguredEnv = (unconfigured.do_ as unknown as { env: Env }).env;
    unconfiguredEnv.CORELINK_RUNNER_MINT_AUTH_KEY = undefined;
    expect((await unconfigured.do_.fetch(prepareRequest({}, { auth: AUTH_KEY }))).status).toBe(503);

    const malformed = makeHarness();
    for (const body of [
      null,
      [],
      {},
      { operationId: OPERATION_ID },
      { operationId: OPERATION_ID, tenantId: TENANT_ID },
      { operationId: 1, tenantId: TENANT_ID, lifecycleGeneration: "0" },
      { operationId: OPERATION_ID, tenantId: 1, lifecycleGeneration: "0" },
      { operationId: OPERATION_ID, tenantId: TENANT_ID, lifecycleGeneration: 0 },
    ]) {
      expect((await malformed.do_.fetch(prepareRequest(body, { auth: AUTH_KEY }))).status).toBe(400);
    }
    expect(malformed.schemaReads).not.toHaveBeenCalled();

    const unauthenticated = makeHarness();
    expect((await unauthenticated.do_.fetch(prepareRequest({ operationId: OPERATION_ID, tenantId: TENANT_ID, lifecycleGeneration: "0" }))).status).toBe(401);
    expect(unauthenticated.schemaReads).not.toHaveBeenCalled();

    const tenantDo = makeHarness({ system: false });
    expect((await tenantDo.do_.fetch(prepareRequest({ operationId: OPERATION_ID, tenantId: TENANT_ID, lifecycleGeneration: "0" }, { auth: AUTH_KEY }))).status).toBe(403);
    expect(tenantDo.schemaReads).not.toHaveBeenCalled();
  });

  it("prepares a system-only obligation and turns dependency failure into a retryable response", async () => {
    const successful = makeHarness();
    const input = { operationId: OPERATION_ID, tenantId: TENANT_ID, lifecycleGeneration: "0" };
    expect((await successful.do_.fetch(prepareRequest(input, { auth: AUTH_KEY }))).status).toBe(204);
    expect(successful.schemaReads).toHaveBeenCalledTimes(1);
    expect(successful.inserts).toHaveBeenCalledTimes(1);
    expect(successful.saved.get(`devenv-cleanup/${OPERATION_ID}`)).toMatchObject({ attempts: 0, tenantId: TENANT_ID, lifecycleGeneration: "0" });
    expect(successful.alarms).toHaveLength(1);

    const rejectedByPrepare = makeHarness();
    expect((await rejectedByPrepare.do_.fetch(prepareRequest({ ...input, operationId: "not-a-uuid" }, { auth: AUTH_KEY }))).status).toBe(503);
    expect(rejectedByPrepare.schemaReads).not.toHaveBeenCalled();

    const failed = makeHarness({ schemaFails: true });
    expect((await failed.do_.fetch(prepareRequest(input, { auth: AUTH_KEY }))).status).toBe(503);
    expect(failed.saved).toHaveLength(0);
  });

  it("keeps a stopped DO alarmed when a cleanup drain is pending or rejected, then permits hibernation after both drains finish", async () => {
    const pending = makeHarness();
    drainDevenv.mockResolvedValueOnce(true);
    drainRunner.mockResolvedValueOnce(false);
    await pending.do_.alarm();
    expect(pending.alarms).toHaveLength(1);

    const rejected = makeHarness();
    drainDevenv.mockRejectedValueOnce(new Error("D1 unavailable"));
    drainRunner.mockResolvedValueOnce(false);
    await rejected.do_.alarm();
    expect(rejected.alarms).toHaveLength(1);

    const drained = makeHarness();
    drainDevenv.mockResolvedValueOnce(false);
    drainRunner.mockResolvedValueOnce(false);
    await drained.do_.alarm();
    expect(drained.alarms).toHaveLength(0);

    // A legacy lifecycle record can lack a tenant ID. The idle reaper must
    // still terminate it and must not let telemetry prevent that cleanup.
    const now = Date.now();
    const { h, do_ } = await makeReaperDO({
      containerStatus: "running",
      lastHealthCheckMs: now - 60_000,
      coldStartCount: 1,
      tenantId: null,
      lastActivityMs: now - IDLE_MS - 1,
    });
    drainDevenv.mockResolvedValueOnce(false);
    drainRunner.mockResolvedValueOnce(false);
    await do_.alarm();
    expect(h.destroy).toHaveBeenCalledTimes(1);
  });
});
