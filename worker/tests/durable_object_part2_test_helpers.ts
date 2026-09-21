/**
 * Unit tests for worker/src/durable_object.ts — DO state machine, lifecycle
 * events, health probe, constant-time comparison, per-tenant pinning.
 *
 * Tests run in Node.js environment (no cloudflare:test / workerd runtime).
 * We test the DO class directly by constructing it with mock DurableObjectState.
 *
 * Charter invariants verified:
 *   - Telemetry fires BEFORE container action (audit-before-mutation)
 *   - Body bytes NEVER in error response (INV-NO-BODY-IN-LOGS)
 *   - Constant-time comparison (timingSafeEqual) correctness
 *   - Per-tenant isolation (DO IDs derived from tenant path)
 */

import { CoreLinkServer, timingSafeEqual } from "../src/durable_object.js";
import { vi } from "vitest";
import type { Env } from "../src/index.js";

// ──────────────────────────────────────────────────────────────────────────────
// Mock helpers
// ──────────────────────────────────────────────────────────────────────────────

/** Create a mock DurableObjectState. Container is always undefined (no CF runtime). */
function makeMockState(idStr = "test-do-id"): DurableObjectState {
  const storage = new Map<string, unknown>();
  const alarmTime: number | null = null;

  return {
    id: {
      toString: () => idStr,
      name: idStr,
      equals: (other: DurableObjectId) => other.toString() === idStr,
    } as DurableObjectId,
    storage: {
      get: async (key: string) => storage.get(key),
      put: async (key: string, val: unknown) => { storage.set(key, val); },
      delete: async (key: string) => storage.delete(key),
      list: async () => new Map(storage),
      getAlarm: async () => alarmTime,
      setAlarm: async (_time: number) => {},
      deleteAlarm: async () => {},
      transaction: async (fn: (txn: DurableObjectTransaction) => Promise<void>) => fn({} as DurableObjectTransaction),
      deleteAll: async () => { storage.clear(); },
    } as unknown as DurableObjectStorage,
    container: undefined, // No container in Node.js test environment
    waitUntil: (_p: Promise<unknown>) => {},
    blockConcurrencyWhile: async <T>(fn: () => Promise<T>): Promise<T> => fn(),
    acceptWebSocket: () => {},
    getWebSockets: () => [],
    setWebSocketAutoResponse: () => {},
    getWebSocketAutoResponse: () => null,
    getWebSocketAutoResponseTimestamp: () => null,
    setHibernatableWebSocketEventTimeout: () => {},
    getHibernatableWebSocketEventTimeout: () => null,
    getTags: () => [],
    abort: () => {},
    props: {},
    facets: {} as DurableObjectFacets,
  } as unknown as DurableObjectState;
}

function makeEnv(): Env {
  return {
    CORELINK_SERVER: {} as DurableObjectNamespace,
    ENVIRONMENT: "test",
    PAGERDUTY_ROUTING_KEY: "",
  };
}

// ──────────────────────────────────────────────────────────────────────────────
// Idempotent-start guard (multi-region cold-start thrash fix, 2026-08-18)
// ──────────────────────────────────────────────────────────────────────────────


interface ReaperHarness {
  state: DurableObjectState;
  destroy: ReturnType<typeof vi.fn>;
  portFetch: ReturnType<typeof vi.fn>;
  setInactivityTimeout: ReturnType<typeof vi.fn>;
  alarms: number[];
  storageMap: Map<string, unknown>;
}

/**
 * Mock state WITH a running container + setAlarm capture.
 *
 * `withInactivityApi=false` models an older workerd (or any runtime predating
 * `Container.setInactivityTimeout`): the DO must still start and serve, just
 * without the platform reaper.
 */
function makeReaperState(lifecycle: Record<string, unknown>, currentAlarm: number | null = 1, withInactivityApi = true): ReaperHarness {
  const storageMap = new Map<string, unknown>();
  storageMap.set("lifecycle", lifecycle);
  const alarms: number[] = [];
  // Model the real binding: destroy() actually stops the container, and the
  // alarm is a single scheduled time that reads back (a mock that always
  // returns null for getAlarm would make the hot-path self-heal untestable).
  const containerRef = { running: true };
  const destroy = vi.fn(async () => { containerRef.running = false; });
  const portFetch = vi.fn(async () => new Response(null, { status: 200 }));
  const setInactivityTimeout = vi.fn(async (_ms: number) => {});
  let pendingAlarm: number | null = currentAlarm;

  const state = {
    id: {
      toString: () => "reaper-do-id",
      name: "reaper-do-id",
      equals: (other: DurableObjectId) => other.toString() === "reaper-do-id",
    } as DurableObjectId,
    storage: {
      get: async (key: string) => storageMap.get(key),
      put: async (key: string, val: unknown) => { storageMap.set(key, val); },
      delete: async (key: string) => storageMap.delete(key),
      list: async (options?: { prefix?: string; limit?: number }) => {
        const entries = [...storageMap.entries()]
          .filter(([key]) => options?.prefix === undefined || key.startsWith(options.prefix))
          .slice(0, options?.limit ?? Number.POSITIVE_INFINITY);
        return new Map(entries);
      },
      getAlarm: async () => pendingAlarm,
      setAlarm: async (time: number) => { alarms.push(time); pendingAlarm = time; },
      deleteAlarm: async () => { pendingAlarm = null; },
      deleteAll: async () => { storageMap.clear(); },
    } as unknown as DurableObjectStorage,
    container: {
      get running() { return containerRef.running; },
      destroy,
      getTcpPort: () => ({ fetch: portFetch }),
      start: () => {},
      monitor: () => new Promise<void>(() => {}),
      ...(withInactivityApi ? { setInactivityTimeout } : {}),
    },
    waitUntil: (_p: Promise<unknown>) => {},
    blockConcurrencyWhile: async <T>(fn: () => Promise<T>): Promise<T> => fn(),
  } as unknown as DurableObjectState;

  return { state, destroy, portFetch, setInactivityTimeout, alarms, storageMap };
}

async function makeReaperDO(lifecycle: Record<string, unknown>, currentAlarm: number | null = 1, withInactivityApi = true): Promise<{ h: ReaperHarness; do_: CoreLinkServer }> {
  const h = makeReaperState(lifecycle, currentAlarm, withInactivityApi);
  const do_ = new CoreLinkServer(h.state, makeEnv());
  await new Promise<void>((r) => setTimeout(r, 5));
  return { h, do_ };
}

const IDLE_MS = 30 * 60 * 1000;


export { CoreLinkServer, timingSafeEqual, makeMockState, makeEnv, makeReaperState, makeReaperDO, IDLE_MS };
export type { ReaperHarness };
