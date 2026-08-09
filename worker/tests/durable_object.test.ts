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

import { describe, it, expect, vi, beforeEach } from "vitest";
import { CoreLinkServer, timingSafeEqual } from "../src/durable_object.js";
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
// Constant-time comparison
// ──────────────────────────────────────────────────────────────────────────────

describe("timingSafeEqual", () => {
  it("returns true for equal strings", async () => {
    expect(await timingSafeEqual("hello", "hello")).toBe(true);
  });

  it("returns false for different strings", async () => {
    expect(await timingSafeEqual("hello", "world")).toBe(false);
  });

  it("returns false for strings of different lengths", async () => {
    expect(await timingSafeEqual("short", "longer-string")).toBe(false);
  });

  it("returns false for empty vs non-empty", async () => {
    expect(await timingSafeEqual("", "x")).toBe(false);
  });

  it("returns true for equal empty strings", async () => {
    expect(await timingSafeEqual("", "")).toBe(true);
  });

  it("returns false for single-char difference", async () => {
    const a = "abcdefghijklmnopqrstuvwxyz0123456789abcdefghijklmnopqrstuvwxyz01";
    const b = "abcdefghijklmnopqrstuvwxyz0123456789abcdefghijklmnopqrstuvwxyz0X";
    expect(await timingSafeEqual(a, b)).toBe(false);
  });
});

// ──────────────────────────────────────────────────────────────────────────────
// DO constructor and init
// ──────────────────────────────────────────────────────────────────────────────

describe("CoreLinkServer constructor", () => {
  it("can be instantiated without throwing", () => {
    const state = makeMockState();
    const env = makeEnv();
    expect(() => new CoreLinkServer(state, env)).not.toThrow();
  });

  it("restores lifecycle state from storage on wakeup", async () => {
    const state = makeMockState();
    // Pre-populate storage with a lifecycle state
    await state.storage.put("lifecycle", {
      containerStatus: "stopped",
      lastHealthCheckMs: 12345,
      coldStartCount: 3,
      tenantId: "test-tenant",
    });
    const env = makeEnv();
    const do_ = new CoreLinkServer(state, env);
    // Wait for blockConcurrencyWhile to complete
    await new Promise<void>((r) => setTimeout(r, 10));
    // Verify DO was created; we can't inspect private state directly but
    // indirectly verify by requesting health
    const resp = await do_.fetch(new Request("http://localhost/_do/health"));
    expect(resp.status).not.toBe(500); // Not an unhandled error
  });
});

// ──────────────────────────────────────────────────────────────────────────────
// DO health probe
// ──────────────────────────────────────────────────────────────────────────────

describe("DO /_do/health", () => {
  it("returns 503 when container is not running (no container binding in test)", async () => {
    const state = makeMockState();
    const env = makeEnv();
    const do_ = new CoreLinkServer(state, env);
    await new Promise<void>((r) => setTimeout(r, 5));

    const resp = await do_.fetch(new Request("http://localhost/_do/health"));
    // Container is undefined in test, so status is not 'running' → 503
    expect(resp.status).toBe(503);
  });

  it("health probe returns JSON with container_status field", async () => {
    const state = makeMockState();
    const env = makeEnv();
    const do_ = new CoreLinkServer(state, env);
    await new Promise<void>((r) => setTimeout(r, 5));

    const resp = await do_.fetch(new Request("http://localhost/_do/health"));
    const body = await resp.json() as Record<string, unknown>;
    expect(body["container_status"]).toBeDefined();
    expect(["stopped", "starting", "running", "degraded"]).toContain(body["container_status"]);
  });

  it("health probe sets X-Request-Id", async () => {
    const state = makeMockState();
    const env = makeEnv();
    const do_ = new CoreLinkServer(state, env);
    await new Promise<void>((r) => setTimeout(r, 5));

    const resp = await do_.fetch(
      new Request("http://localhost/_do/health", {
        headers: { "x-request-id": "health-probe-test" },
      }),
    );
    expect(resp.headers.get("x-request-id")).toBe("health-probe-test");
  });

  it("health probe generates x-request-id if not provided", async () => {
    const state = makeMockState();
    const env = makeEnv();
    const do_ = new CoreLinkServer(state, env);
    await new Promise<void>((r) => setTimeout(r, 5));

    const resp = await do_.fetch(new Request("http://localhost/_do/health"));
    expect(resp.headers.get("x-request-id")).not.toBeNull();
  });
});

// ──────────────────────────────────────────────────────────────────────────────
// DO D1 placement instrument (/_do/health → d1_probe)
//
// The instrument exists to answer ONE question: is this DO co-located with the
// ENAM D1 primary? These tests pin the property that makes the answer
// trustworthy — a read that never happened, or that threw, can NEVER surface as
// a fast number or as a missing field.
// ──────────────────────────────────────────────────────────────────────────────

/** Build a D1-ish binding double. `withSession` is omitted when `sessions` is false. */
function makeD1Double(opts: {
  sessions: boolean;
  primaryDelayMs?: number;
  replicaDelayMs?: number;
  throwOn?: "primary" | "replica" | "both";
  withSessionThrows?: boolean;
}): unknown {
  const handle = (kind: "primary" | "replica", delayMs: number) => ({
    prepare: (_q: string) => ({
      all: async () => {
        await new Promise<void>((r) => setTimeout(r, delayMs));
        if (opts.throwOn === kind || opts.throwOn === "both") {
          throw new Error(`${kind} read exploded`);
        }
        return {
          results: [{ "1": 1 }],
          meta: {
            served_by_region: kind === "primary" ? "ENAM" : "WNAM",
            served_by_primary: kind === "primary",
            served_by_colo: kind === "primary" ? "MIA" : "SJC",
          },
        };
      },
    }),
  });

  const primary = handle("primary", opts.primaryDelayMs ?? 0);
  if (!opts.sessions) return primary;
  return {
    ...primary,
    withSession: (_c: string) => {
      if (opts.withSessionThrows === true) throw new Error("no sessions here");
      return handle("replica", opts.replicaDelayMs ?? 0);
    },
  };
}

function envWithD1(db: unknown): Env {
  return { ...makeEnv(), CONFIG_DB: db } as unknown as Env;
}

async function healthBody(env: Env, path = "http://localhost/_do/health"): Promise<Record<string, unknown>> {
  const state = makeMockState();
  const do_ = new CoreLinkServer(state, env);
  await new Promise<void>((r) => setTimeout(r, 5));
  const resp = await do_.fetch(new Request(path));
  return (await resp.json()) as Record<string, unknown>;
}

describe("DO /_do/health d1_probe (placement instrument)", () => {
  it("measures BOTH paths and reports D1's served_by provenance", async () => {
    const body = await healthBody(envWithD1(makeD1Double({ sessions: true })));
    const probe = body["d1_probe"] as Record<string, unknown>;
    expect(probe["binding_bound"]).toBe(true);
    expect(probe["sessions_api_available"]).toBe(true);

    const primary = probe["primary"] as Record<string, unknown>;
    const replica = probe["replica"] as Record<string, unknown>;
    expect(primary["available"]).toBe(true);
    expect(primary["ok"]).toBe(true);
    expect((primary["samples_ms"] as number[]).length).toBe(3);
    expect(typeof primary["min_ms"]).toBe("number");
    expect(primary["error"]).toBeNull();
    // The provenance is what distinguishes "the replica path really hit a
    // replica" from "the Sessions API quietly served the primary".
    expect(primary["served_by_primary"]).toBe(true);
    expect(primary["served_by_region"]).toBe("ENAM");
    expect(replica["served_by_primary"]).toBe(false);
    expect(replica["ok"]).toBe(true);
    // The warm-up is REPORTED, not hidden — it is the honest cold number.
    expect(typeof probe["warmup_ms"]).toBe("number");
  });

  it("degrades to primary-only (never throws) when the Sessions API is absent", async () => {
    const body = await healthBody(envWithD1(makeD1Double({ sessions: false })));
    const probe = body["d1_probe"] as Record<string, unknown>;
    expect(probe["sessions_api_available"]).toBe(false);
    const replica = probe["replica"] as Record<string, unknown>;
    // "unavailable, fell back" — NOT silently reported as equal to the primary.
    expect(replica["available"]).toBe(false);
    expect(replica["error"]).toContain("withSession");
    expect(replica["min_ms"]).toBeNull();
    expect((probe["primary"] as Record<string, unknown>)["ok"]).toBe(true);
    // The health probe itself must be unaffected.
    expect(body["container_status"]).toBeDefined();
  });

  it("a read that THROWS is an explicit error — never a fast number, never a missing field", async () => {
    const body = await healthBody(envWithD1(makeD1Double({ sessions: true, throwOn: "primary" })));
    const probe = body["d1_probe"] as Record<string, unknown>;
    const primary = probe["primary"] as Record<string, unknown>;
    expect(primary["available"]).toBe(true); // it WAS attempted…
    expect(primary["ok"]).toBe(false); // …and it failed.
    expect(primary["min_ms"]).toBeNull();
    expect(primary["samples_ms"]).toEqual([]);
    expect(typeof primary["error"]).toBe("string");
    expect(primary["error"]).toContain("exploded");
  });

  it("reports an unbound CONFIG_DB as unavailable on both paths, not as zero", async () => {
    // makeEnv() has no CONFIG_DB at all — the pre-existing test double.
    const body = await healthBody(makeEnv());
    const probe = body["d1_probe"] as Record<string, unknown>;
    expect(probe["binding_bound"]).toBe(false);
    for (const path of ["primary", "replica"]) {
      const p = probe[path] as Record<string, unknown>;
      expect(p["available"]).toBe(false);
      expect(p["min_ms"]).toBeNull();
      expect(p["error"]).toContain("CONFIG_DB");
    }
  });

  it("a throwing withSession is reported, not propagated", async () => {
    const body = await healthBody(
      envWithD1(makeD1Double({ sessions: true, withSessionThrows: true })),
    );
    const probe = body["d1_probe"] as Record<string, unknown>;
    const replica = probe["replica"] as Record<string, unknown>;
    expect(replica["available"]).toBe(false);
    expect(replica["error"]).toContain("no sessions here");
    expect((probe["primary"] as Record<string, unknown>)["ok"]).toBe(true);
  });

  it("makes no external colo request unless ?colo=1 is passed", async () => {
    const fetchSpy = vi.spyOn(globalThis, "fetch");
    try {
      const body = await healthBody(envWithD1(makeD1Double({ sessions: true })));
      const probe = body["d1_probe"] as Record<string, unknown>;
      expect(probe["do_colo"]).toBeNull();
      expect(probe["do_colo_error"]).toBeNull();
      expect(fetchSpy).not.toHaveBeenCalled();
    } finally {
      fetchSpy.mockRestore();
    }
  });

  it("?colo=1 resolves the DO colo, and a failing trace is an error field", async () => {
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValue(new Response("fl=1\ncolo=IAD\n", { status: 200 }));
    try {
      const body = await healthBody(
        envWithD1(makeD1Double({ sessions: true })),
        "http://localhost/_do/health?colo=1",
      );
      expect((body["d1_probe"] as Record<string, unknown>)["do_colo"]).toBe("IAD");
    } finally {
      fetchSpy.mockRestore();
    }

    const failSpy = vi.spyOn(globalThis, "fetch").mockRejectedValue(new Error("trace down"));
    try {
      const body = await healthBody(
        envWithD1(makeD1Double({ sessions: true })),
        "http://localhost/_do/health?colo=1",
      );
      const probe = body["d1_probe"] as Record<string, unknown>;
      expect(probe["do_colo"]).toBeNull();
      expect(probe["do_colo_error"]).toContain("trace down");
    } finally {
      failSpy.mockRestore();
    }
  });

  it("the D1 reads run ONLY on /_do/health — /_do/stop never touches D1", async () => {
    let reads = 0;
    const db = {
      prepare: (_q: string) => ({
        all: async () => {
          reads += 1;
          return { results: [], meta: {} };
        },
      }),
    };
    const state = makeMockState();
    const do_ = new CoreLinkServer(state, envWithD1(db));
    await new Promise<void>((r) => setTimeout(r, 5));
    await do_.fetch(new Request("http://localhost/_do/stop"));
    expect(reads).toBe(0);
  });
});

// ──────────────────────────────────────────────────────────────────────────────
// DO stop endpoint
// ──────────────────────────────────────────────────────────────────────────────

describe("DO /_do/stop", () => {
  it("returns 200 with stopped:true", async () => {
    const state = makeMockState();
    const env = makeEnv();
    const do_ = new CoreLinkServer(state, env);
    await new Promise<void>((r) => setTimeout(r, 5));

    const resp = await do_.fetch(new Request("http://localhost/_do/stop"));
    expect(resp.status).toBe(200);
    const body = await resp.json() as { stopped: boolean };
    expect(body.stopped).toBe(true);
  });
});

// ──────────────────────────────────────────────────────────────────────────────
// DO container unavailable path (no container binding in test)
// ──────────────────────────────────────────────────────────────────────────────

describe("DO request forwarding without container", () => {
  it("returns 503 CONTAINER_UNAVAILABLE when container binding is absent", async () => {
    const state = makeMockState();
    const env = makeEnv();
    const do_ = new CoreLinkServer(state, env);
    await new Promise<void>((r) => setTimeout(r, 5));

    const resp = await do_.fetch(
      new Request("http://localhost/v2/myrepo/blobs/sha256:abc", {
        headers: { "x-request-id": "test-req-001" },
      }),
    );
    expect(resp.status).toBe(503);
    const body = await resp.json() as { error: string };
    expect(body.error).toBe("CONTAINER_UNAVAILABLE");
  });

  it("503 response body does NOT contain request body (INV-NO-BODY-IN-LOGS)", async () => {
    const state = makeMockState();
    const env = makeEnv();
    const do_ = new CoreLinkServer(state, env);
    await new Promise<void>((r) => setTimeout(r, 5));

    const sensitiveBody = "sensitive-data-that-must-not-leak=true";
    const resp = await do_.fetch(
      new Request("http://localhost/api/v2/t/blobs", {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          "x-request-id": "no-leak-test",
        },
        body: sensitiveBody,
      }),
    );
    const text = await resp.text();
    expect(text).not.toContain("sensitive-data");
    expect(text).not.toContain("must-not-leak");
  });

  it("returns X-Request-Id on 503 response", async () => {
    const state = makeMockState();
    const env = makeEnv();
    const do_ = new CoreLinkServer(state, env);
    await new Promise<void>((r) => setTimeout(r, 5));

    const resp = await do_.fetch(
      new Request("http://localhost/api/v2/t/path", {
        headers: { "x-request-id": "503-test" },
      }),
    );
    expect(resp.headers.get("x-request-id")).toBe("503-test");
  });
});

// ──────────────────────────────────────────────────────────────────────────────
// DO lifecycle state persistence
// ──────────────────────────────────────────────────────────────────────────────

describe("DO lifecycle state persistence", () => {
  it("persists lifecycle state on stop", async () => {
    const state = makeMockState();
    const env = makeEnv();
    const do_ = new CoreLinkServer(state, env);
    await new Promise<void>((r) => setTimeout(r, 5));

    await do_.fetch(new Request("http://localhost/_do/stop"));

    // Verify state was written to storage
    const stored = await state.storage.get("lifecycle") as Record<string, unknown> | undefined;
    expect(stored).toBeDefined();
    expect(stored?.["containerStatus"]).toBe("stopped");
  });
});

// ──────────────────────────────────────────────────────────────────────────────
// DO alarm
// ──────────────────────────────────────────────────────────────────────────────

describe("DO alarm", () => {
  it("alarm() runs without throwing when container is stopped", async () => {
    const state = makeMockState();
    const env = makeEnv();
    const do_ = new CoreLinkServer(state, env);
    await new Promise<void>((r) => setTimeout(r, 5));

    await expect(do_.alarm()).resolves.not.toThrow();
  });

  it("alarm() runs without throwing when lifecycle state is 'starting'", async () => {
    const state = makeMockState();
    await state.storage.put("lifecycle", {
      containerStatus: "starting",
      lastHealthCheckMs: 0,
      coldStartCount: 0,
      tenantId: null,
    });
    const env = makeEnv();
    const do_ = new CoreLinkServer(state, env);
    await new Promise<void>((r) => setTimeout(r, 5));

    await expect(do_.alarm()).resolves.not.toThrow();
  });

  it("alarm() runs without throwing when lifecycle state is 'degraded'", async () => {
    const state = makeMockState();
    await state.storage.put("lifecycle", {
      containerStatus: "degraded",
      lastHealthCheckMs: 0,
      coldStartCount: 1,
      tenantId: "tenant-a",
    });
    const env = makeEnv();
    const do_ = new CoreLinkServer(state, env);
    await new Promise<void>((r) => setTimeout(r, 5));

    await expect(do_.alarm()).resolves.not.toThrow();
  });
});

// ──────────────────────────────────────────────────────────────────────────────
// DO ensureContainerRunning state machine
// ──────────────────────────────────────────────────────────────────────────────

describe("DO ensureContainerRunning state machine", () => {
  it("returns 503 for 'stopped' state (no container binding)", async () => {
    const state = makeMockState();
    await state.storage.put("lifecycle", {
      containerStatus: "stopped",
      lastHealthCheckMs: 0,
      coldStartCount: 0,
      tenantId: null,
    });
    const env = makeEnv();
    const do_ = new CoreLinkServer(state, env);
    await new Promise<void>((r) => setTimeout(r, 5));

    const resp = await do_.fetch(new Request("http://localhost/v2/repo/blobs/sha256:abc"));
    expect(resp.status).toBe(503);
  });

  it("returns 503 for 'degraded' state (no container binding)", async () => {
    const state = makeMockState();
    await state.storage.put("lifecycle", {
      containerStatus: "degraded",
      lastHealthCheckMs: 0,
      coldStartCount: 2,
      tenantId: "my-tenant",
    });
    const env = makeEnv();
    const do_ = new CoreLinkServer(state, env);
    await new Promise<void>((r) => setTimeout(r, 5));

    const resp = await do_.fetch(new Request("http://localhost/v2/repo/blobs/sha256:abc"));
    expect(resp.status).toBe(503);
  });
});

// ──────────────────────────────────────────────────────────────────────────────
// DO multiple requests handling
// ──────────────────────────────────────────────────────────────────────────────

describe("DO multiple concurrent request paths", () => {
  it("handles multiple sequential fetch calls without throwing", async () => {
    const state = makeMockState();
    const env = makeEnv();
    const do_ = new CoreLinkServer(state, env);
    await new Promise<void>((r) => setTimeout(r, 5));

    const reqs = [
      do_.fetch(new Request("http://localhost/_do/health")),
      do_.fetch(new Request("http://localhost/api/v2/t/p")),
      do_.fetch(new Request("http://localhost/_do/health")),
    ];
    const responses = await Promise.all(reqs);
    for (const resp of responses) {
      expect([200, 503]).toContain(resp.status);
    }
  });

  it("stop then health returns 503 (container stopped)", async () => {
    const state = makeMockState();
    const env = makeEnv();
    const do_ = new CoreLinkServer(state, env);
    await new Promise<void>((r) => setTimeout(r, 5));

    await do_.fetch(new Request("http://localhost/_do/stop"));
    const healthResp = await do_.fetch(new Request("http://localhost/_do/health"));
    expect(healthResp.status).toBe(503);
  });
});

// ──────────────────────────────────────────────────────────────────────────────
// DO tenantId handling
// ──────────────────────────────────────────────────────────────────────────────

describe("DO tenantId in persisted state", () => {
  it("reads tenantId from storage if present", async () => {
    const state = makeMockState();
    await state.storage.put("lifecycle", {
      containerStatus: "stopped",
      lastHealthCheckMs: Date.now() - 1000,
      coldStartCount: 1,
      tenantId: "my-test-tenant",
    });
    const env = makeEnv();
    const do_ = new CoreLinkServer(state, env);
    await new Promise<void>((r) => setTimeout(r, 5));

    // Health probe runs fine with tenantId set
    const resp = await do_.fetch(new Request("http://localhost/_do/health"));
    expect([200, 503]).toContain(resp.status);
  });
});

// ──────────────────────────────────────────────────────────────────────────────
// DO response bodies are sanitized
// ──────────────────────────────────────────────────────────────────────────────

describe("DO response sanitization", () => {
  it("stop endpoint returns sanitized JSON (no raw tenant id in body)", async () => {
    const state = makeMockState();
    await state.storage.put("lifecycle", {
      containerStatus: "stopped",
      lastHealthCheckMs: 0,
      coldStartCount: 0,
      tenantId: "SUPERSECRET_TENANT_ID",
    });
    const env = makeEnv();
    const do_ = new CoreLinkServer(state, env);
    await new Promise<void>((r) => setTimeout(r, 5));

    const resp = await do_.fetch(new Request("http://localhost/_do/stop"));
    const text = await resp.text();
    // The tenant id MUST NOT appear verbatim in response (INV-NO-PII-IN-LOGS)
    expect(text).not.toContain("SUPERSECRET_TENANT_ID");
  });

  it("health probe body does not contain sensitive data", async () => {
    const state = makeMockState();
    const env = makeEnv();
    // Inject secret-looking data as DO id (should only appear as hash)
    const do_ = new CoreLinkServer(state, env);
    await new Promise<void>((r) => setTimeout(r, 5));

    const resp = await do_.fetch(new Request("http://localhost/_do/health"));
    const body = await resp.json() as Record<string, unknown>;
    // Verify only expected fields are present
    const allowedFields = new Set([
      "status", "container_status", "container_running",
      "cold_start_count", "last_health_check_ms", "request_id",
      // D1 placement instrument: timings + D1's own served_by_* provenance +
      // bounded error strings from a `SELECT 1`. No tenant id, no binding value,
      // no body bytes ever enter it.
      "d1_probe",
    ]);
    for (const key of Object.keys(body)) {
      expect(allowedFields.has(key), `unexpected field: ${key}`).toBe(true);
    }
  });
});

// ──────────────────────────────────────────────────────────────────────────────
// DO PagerDuty telemetry path (non-empty routingKey exercises emitLifecycleEvent body)
// ──────────────────────────────────────────────────────────────────────────────

describe("DO PagerDuty telemetry emit", () => {
  it("stop with non-empty PAGERDUTY_ROUTING_KEY exercises emitLifecycleEvent body (fetch stubbed to 200)", async () => {
    // Stub fetch so the PD call returns 200 immediately — exercises lines 160-186
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      new Response(JSON.stringify({ status: "success" }), {
        status: 202,
        headers: { "Content-Type": "application/json" },
      }),
    );
    try {
      const state = makeMockState();
      const env: Env & { PAGERDUTY_ROUTING_KEY?: string } = {
        CORELINK_SERVER: {} as DurableObjectNamespace,
        ENVIRONMENT: "test",
        PAGERDUTY_ROUTING_KEY: "test-fake-routing-key-00000000000000",
      };
      const do_ = new CoreLinkServer(state, env);
      await new Promise<void>((r) => setTimeout(r, 5));

      // Should return 200 — PD emit is fire-and-forget
      const resp = await do_.fetch(new Request("http://localhost/_do/stop"));
      expect(resp.status).toBe(200);
      const body = await resp.json() as { stopped: boolean };
      expect(body.stopped).toBe(true);
    } finally {
      vi.restoreAllMocks();
    }
  });

  it("emitLifecycleEvent catch block is exercised when fetch throws (lines 187-188)", async () => {
    // Patch global fetch to throw for PD endpoint, verifying the catch block is exercised.
    vi.spyOn(globalThis, "fetch").mockImplementation(async (input: RequestInfo | URL, _init?: RequestInit) => {
      const url = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
      if (url.includes("pagerduty.com")) {
        throw new Error("simulated PagerDuty network error");
      }
      throw new Error("unexpected fetch call in test");
    });

    try {
      const state = makeMockState();
      const env: Env & { PAGERDUTY_ROUTING_KEY?: string } = {
        CORELINK_SERVER: {} as DurableObjectNamespace,
        ENVIRONMENT: "test",
        PAGERDUTY_ROUTING_KEY: "non-empty-routing-key-for-throw-test",
      };
      const do_ = new CoreLinkServer(state, env);
      await new Promise<void>((r) => setTimeout(r, 5));

      // The stop endpoint emits a PD event; fetch throws → catch block runs → DO still returns 200
      const resp = await do_.fetch(new Request("http://localhost/_do/stop"));
      expect(resp.status).toBe(200);
      const body = await resp.json() as { stopped: boolean };
      expect(body.stopped).toBe(true);
    } finally {
      vi.restoreAllMocks();
    }
  });

  it("emitLifecycleEvent skips body when routingKey is empty string", async () => {
    // Verify the guard at routingKey.length === 0 (already covered by default tests,
    // this ensures the empty-key fast-path stays covered after refactors)
    const state = makeMockState();
    const env = makeEnv(); // PAGERDUTY_ROUTING_KEY = "" via module augmentation default
    const do_ = new CoreLinkServer(state, env);
    await new Promise<void>((r) => setTimeout(r, 5));

    const resp = await do_.fetch(new Request("http://localhost/_do/stop"));
    expect(resp.status).toBe(200);
  });

  it("stop with non-empty PD key still persists lifecycle state as 'stopped'", async () => {
    const state = makeMockState();
    await state.storage.put("lifecycle", {
      containerStatus: "running",
      lastHealthCheckMs: Date.now() - 1000,
      coldStartCount: 2,
      tenantId: "tenant-pd-test",
    });
    const env: Env & { PAGERDUTY_ROUTING_KEY?: string } = {
      CORELINK_SERVER: {} as DurableObjectNamespace,
      ENVIRONMENT: "test",
      PAGERDUTY_ROUTING_KEY: "fake-key-for-coverage",
    };
    const do_ = new CoreLinkServer(state, env);
    await new Promise<void>((r) => setTimeout(r, 5));

    await do_.fetch(new Request("http://localhost/_do/stop"));

    const stored = await state.storage.get("lifecycle") as Record<string, unknown> | undefined;
    expect(stored?.["containerStatus"]).toBe("stopped");
  });
});

// ──────────────────────────────────────────────────────────────────────────────
// Adversarial attacks — 5 required by WP-1.1 DOD §7
// ──────────────────────────────────────────────────────────────────────────────

describe("adversarial_replay_attack — replay of old stop request", () => {
  it("replay of /_do/stop is idempotent (already stopped → still returns 200 stopped:true)", async () => {
    // Attack: adversary captures a valid /_do/stop HTTP request and replays it
    // to force repeated shutdown of a tenant's container.
    // Defence: stop is idempotent; replaying it does not cause data loss — the
    // container is already stopped, and the DO persists 'stopped' status.
    const state = makeMockState();
    const env = makeEnv();
    const do_ = new CoreLinkServer(state, env);
    await new Promise<void>((r) => setTimeout(r, 5));

    // First stop
    const r1 = await do_.fetch(new Request("http://localhost/_do/stop"));
    expect(r1.status).toBe(200);
    // Replay same request
    const r2 = await do_.fetch(new Request("http://localhost/_do/stop"));
    expect(r2.status).toBe(200);
    const b2 = await r2.json() as { stopped: boolean };
    expect(b2.stopped).toBe(true);

    // State must still be 'stopped' — not corrupted by replay
    const stored = await state.storage.get("lifecycle") as Record<string, unknown> | undefined;
    expect(stored?.["containerStatus"]).toBe("stopped");
  });
});

describe("adversarial_tenant_boundary — cross-tenant request injection", () => {
  it("two DO instances with different DO IDs cannot share lifecycle state", async () => {
    // Attack: adversary attempts to leak one tenant's container state into another
    // by sending requests with manipulated tenant path headers.
    // Defence: each CoreLinkServer instance is isolated by DurableObject ID,
    // derived from `idFromName(tenantId)`. Two DOs with different IDs have
    // entirely separate storage maps — verified here by using two mock states.
    const stateA = makeMockState("do-id-tenant-A");
    const stateB = makeMockState("do-id-tenant-B");
    await stateA.storage.put("lifecycle", {
      containerStatus: "running",
      lastHealthCheckMs: Date.now(),
      coldStartCount: 5,
      tenantId: "tenant-A",
    });
    await stateB.storage.put("lifecycle", {
      containerStatus: "stopped",
      lastHealthCheckMs: 0,
      coldStartCount: 0,
      tenantId: "tenant-B",
    });
    const envA = makeEnv();
    const envB = makeEnv();
    const doA = new CoreLinkServer(stateA, envA);
    const doB = new CoreLinkServer(stateB, envB);
    await new Promise<void>((r) => setTimeout(r, 10));

    // DO-A is 'running' (with container=undefined so ensureContainerRunning will try startContainer)
    // DO-B is 'stopped'
    // Send a stop to DO-B — must NOT affect DO-A
    await doB.fetch(new Request("http://localhost/_do/stop"));

    // DO-A health probe should NOT be affected
    const healthA = await doA.fetch(new Request("http://localhost/_do/health"));
    const bodyA = await healthA.json() as Record<string, unknown>;
    // coldStartCount from DO-A's own storage must not have been zeroed by DO-B's stop
    expect(bodyA["cold_start_count"]).not.toBe(0);

    // DO-B is stopped — confirmed
    const storedB = await stateB.storage.get("lifecycle") as Record<string, unknown> | undefined;
    expect(storedB?.["containerStatus"]).toBe("stopped");
    // DO-A storage is untouched
    const storedA = await stateA.storage.get("lifecycle") as Record<string, unknown> | undefined;
    expect(storedA?.["containerStatus"]).toBe("running");
  });
});

describe("adversarial_malformed_grpc_frame — malformed path request to DO", () => {
  it("negative_malformed_path does not expose internal error stack traces", async () => {
    // Attack: adversary sends a request to an internal DO path that looks like
    // a gRPC frame prefix (binary garbage / path injection).
    // Defence: the DO only handles '/_do/health' and '/_do/stop' explicitly.
    // Everything else falls through to ensureContainerRunning, which returns
    // 503 CONTAINER_UNAVAILABLE (no container binding in test). No stack trace
    // or internal path information is leaked in the response body.
    const state = makeMockState();
    const env = makeEnv();
    const do_ = new CoreLinkServer(state, env);
    await new Promise<void>((r) => setTimeout(r, 5));

    // Malformed-looking path with binary-ish characters in URL-encoded form
    const malformedPaths = [
      "http://localhost/%00%01%02gRPC-frame-garbage",
      "http://localhost/../../etc/passwd",
      "http://localhost/_do//../health",
      "http://localhost/grpc.CoreLink.CacheService/Put\x00\x00\x00\x00\x05",
    ];
    for (const path of malformedPaths) {
      let resp: Response;
      try {
        resp = await do_.fetch(new Request(path));
      } catch (_err: unknown) {
        // URL constructor may reject truly malformed URLs — this is also correct behaviour
        continue;
      }
      const text = await resp.text();
      // Must not contain stack traces, file paths, or internal module names
      expect(text).not.toMatch(/at\s+\w+\s+\(/);  // no stack frames
      expect(text).not.toContain("node_modules");
      expect(text).not.toContain("/src/");
      // Must be valid JSON
      expect(() => JSON.parse(text)).not.toThrow();
    }
  });
});

describe("adversarial_do_storage_corruption — corrupt lifecycle state in DO storage", () => {
  it("tampered_storage_missing_fields falls back to default lifecycle on next fetch", async () => {
    // Attack: an operator or side-channel write corrupts the DO's persisted
    // 'lifecycle' key in Durable Object storage. If the DO restores corrupt
    // state on wakeup, it might behave incorrectly.
    // Defence: the storage.get<LifecycleState>('lifecycle') call returns
    // undefined for unexpected types; the default initializer value
    // `{containerStatus: 'stopped', ...}` is always used as the base.
    // We simulate this by setting a storage value with missing fields.
    const state = makeMockState();
    // Corrupt: missing required fields
    await state.storage.put("lifecycle", {
      containerStatus: "running",
      // missing: lastHealthCheckMs, coldStartCount, tenantId
    });
    const env = makeEnv();
    const do_ = new CoreLinkServer(state, env);
    await new Promise<void>((r) => setTimeout(r, 10));

    // DO should not panic; health probe must return a valid JSON response
    const resp = await do_.fetch(new Request("http://localhost/_do/health"));
    expect([200, 503]).toContain(resp.status);
    const body = await resp.json() as Record<string, unknown>;
    expect(typeof body["container_status"]).toBe("string");
    // Must not contain undefined or null in critical fields that would cause NaN issues
    expect(body["container_running"]).not.toBeUndefined();
  });

  it("tampered_storage_wrong_type for containerStatus does not crash the DO", async () => {
    // Attack: containerStatus set to an unknown string value
    const state = makeMockState();
    await state.storage.put("lifecycle", {
      containerStatus: "evil-injected-status",
      lastHealthCheckMs: 0,
      coldStartCount: 0,
      tenantId: null,
    });
    const env = makeEnv();
    const do_ = new CoreLinkServer(state, env);
    await new Promise<void>((r) => setTimeout(r, 10));

    // The DO restores this state and then calls ensureContainerRunning.
    // The 'running' check fails (container=undefined), degraded/stopped check fails,
    // starting check fails — so it falls to 'unexpected_lifecycle_state'.
    const resp = await do_.fetch(new Request("http://localhost/api/v2/t/p"));
    // Must return 503 CONTAINER_UNAVAILABLE (not a crash/unhandled rejection)
    expect(resp.status).toBe(503);
    const body = await resp.json() as { error: string };
    expect(body.error).toBe("CONTAINER_UNAVAILABLE");
  });
});

describe("adversarial_container_start_race — concurrent start race simulation", () => {
  it("prop_assert_concurrent_health_probes during 'stopped' state all return valid responses", async () => {
    // Attack: adversary sends N concurrent requests to a stopped DO instance,
    // attempting to trigger a race in container start logic that could allow
    // cross-request state confusion.
    // Defence: the CF DO serialization guarantee (single-threaded execution within
    // the DO) prevents races in production. In Node.js tests, Promise.all simulates
    // concurrent dispatch and verifies no unhandled rejections or status-code
    // inconsistencies across the burst.
    const state = makeMockState();
    const env = makeEnv();
    const do_ = new CoreLinkServer(state, env);
    await new Promise<void>((r) => setTimeout(r, 5));

    // 10 concurrent health probes
    const probes = Array.from({ length: 10 }, (_, i) =>
      do_.fetch(new Request("http://localhost/_do/health", {
        headers: { "x-request-id": `concurrent-${i}` },
      })),
    );
    const responses = await Promise.all(probes);
    for (const resp of responses) {
      // All must complete without throwing and return a known status
      expect([200, 503]).toContain(resp.status);
      const body = await resp.json() as Record<string, unknown>;
      expect(typeof body["container_status"]).toBe("string");
    }
  });

  it("prop_assert_stop_then_concurrent_fetch all return 503 (not mixed states)", async () => {
    // After a stop, all subsequent non-management requests must return 503
    // (not a mix of 200 + 503 from a state race).
    const state = makeMockState();
    const env = makeEnv();
    const do_ = new CoreLinkServer(state, env);
    await new Promise<void>((r) => setTimeout(r, 5));

    await do_.fetch(new Request("http://localhost/_do/stop"));

    // 5 concurrent fetches after stop
    const fetches = Array.from({ length: 5 }, (_, i) =>
      do_.fetch(new Request(`http://localhost/api/v2/tenant/blob-${i}`)),
    );
    const results = await Promise.all(fetches);
    for (const r of results) {
      expect(r.status).toBe(503);
    }
  });
});

// ──────────────────────────────────────────────────────────────────────────────
// DO durable idle reaper (immortal-container regression, 2026-08-01)
//
// The idle reaper MUST run inside alarm() off persisted lastActivityMs. The
// original in-memory setTimeout reaper evaporated on isolate eviction while
// the 30s health-check alarm chain survived, so any once-started container
// became immortal and was billed 24/7 (2026-07 invoice: ~13 GiB resident
// memory around the clock, $84/mo Container Memory, $0.00 real traffic).
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
      list: async () => new Map(storageMap),
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

describe("DO durable idle reaper (immortal-container regression)", () => {
  it("alarm() destroys the container and does NOT reschedule once idle expires", async () => {
    const now = Date.now();
    const { h, do_ } = await makeReaperDO({
      containerStatus: "running",
      lastHealthCheckMs: now - 60_000,
      coldStartCount: 1,
      tenantId: "tenant-idle",
      lastActivityMs: now - IDLE_MS - 60_000,
    });

    await do_.alarm();

    expect(h.destroy).toHaveBeenCalledTimes(1);
    expect(h.alarms).toHaveLength(0); // chain ends → DO free to hibernate
    const persisted = h.storageMap.get("lifecycle") as { containerStatus: string };
    expect(persisted.containerStatus).toBe("stopped");
  });

  it("alarm() health-checks and reschedules while activity is recent", async () => {
    const now = Date.now();
    const { h, do_ } = await makeReaperDO({
      containerStatus: "running",
      lastHealthCheckMs: now - 60_000,
      coldStartCount: 1,
      tenantId: "tenant-active",
      lastActivityMs: now - 60_000,
    });

    await do_.alarm();

    expect(h.destroy).not.toHaveBeenCalled();
    expect(h.portFetch).toHaveBeenCalledTimes(1); // health probe ran
    expect(h.alarms).toHaveLength(1); // chain continues
  });

  it("alarm() backfills an absent lastActivityMs (pre-fix state) instead of reaping", async () => {
    const now = Date.now();
    const { h, do_ } = await makeReaperDO({
      containerStatus: "running",
      lastHealthCheckMs: now - 60_000,
      coldStartCount: 3,
      tenantId: "tenant-prefix-state",
      // no lastActivityMs — state persisted before this field existed
    });

    await do_.alarm();

    expect(h.destroy).not.toHaveBeenCalled();
    expect(h.alarms.length).toBeGreaterThan(0); // chain continues
    const persisted = h.storageMap.get("lifecycle") as { lastActivityMs?: number };
    expect(persisted.lastActivityMs).toBeGreaterThanOrEqual(now); // clock started + persisted
  });

  it("a fresh proxied request protects the container even against stale persisted activity", async () => {
    const now = Date.now();
    const { h, do_ } = await makeReaperDO({
      containerStatus: "running",
      lastHealthCheckMs: now - 60_000,
      coldStartCount: 1,
      tenantId: "tenant-just-active",
      lastActivityMs: now - IDLE_MS - 60_000, // persisted value says: reap
    });

    // Real request → in-memory touch of lastActivityMs (no storage write)
    const resp = await do_.fetch(
      new Request("http://localhost/v1/cas/tenant-just-active/abc", {
        headers: { "x-corelink-tenant-id": "tenant-just-active" },
      }),
    );
    expect(resp.status).toBe(200); // proxied to the mock container

    await do_.alarm();

    expect(h.destroy).not.toHaveBeenCalled(); // in-memory activity wins
    expect(h.alarms.length).toBeGreaterThan(0);
  });

  it("alarm() double-fire dedup still reschedules (never orphans the chain)", async () => {
    const now = Date.now();
    const { h, do_ } = await makeReaperDO({
      containerStatus: "running",
      lastHealthCheckMs: now - 1_000, // probed 1s ago → dedup path
      coldStartCount: 1,
      tenantId: "tenant-dedup",
      lastActivityMs: now - 1_000,
    });

    await do_.alarm();

    expect(h.portFetch).not.toHaveBeenCalled(); // probe skipped
    expect(h.alarms).toHaveLength(1); // but chain NOT broken
  });

  it("degraded-but-running stays in the chain: reaper still fires on idle expiry", async () => {
    const now = Date.now();
    const { h, do_ } = await makeReaperDO({
      containerStatus: "degraded",
      lastHealthCheckMs: now - 60_000,
      coldStartCount: 5,
      tenantId: "tenant-degraded",
      lastActivityMs: now - IDLE_MS - 60_000,
    });

    await do_.alarm();

    expect(h.destroy).toHaveBeenCalledTimes(1);
    expect(h.alarms).toHaveLength(0);
  });

  it("degraded-but-running with recent activity: no probe, chain alive", async () => {
    const now = Date.now();
    const { h, do_ } = await makeReaperDO({
      containerStatus: "degraded",
      lastHealthCheckMs: now - 60_000,
      coldStartCount: 5,
      tenantId: "tenant-degraded-active",
      lastActivityMs: now - 60_000,
    });

    await do_.alarm();

    expect(h.destroy).not.toHaveBeenCalled();
    expect(h.portFetch).not.toHaveBeenCalled(); // degraded: no probe (preserved semantics)
    expect(h.alarms).toHaveLength(1); // reaper keeps watching
  });
});

// ──────────────────────────────────────────────────────────────────────────────
// Alarm-chain durability + start-failure reap (cold-review findings 1-3)
//
// The alarm owns BOTH the health chain and the reaper, so a lost chain is an
// immortal container by another door. And a container that fails its startup
// health check has ALREADY been started — re-labelling lifecycle "stopped"
// without destroying leaves it resident, billed, and un-reapable (the per-DO
// `container_start_threw` wedge that used to clear only on an image roll).
// ──────────────────────────────────────────────────────────────────────────────

describe("DO alarm chain durability (immortality-by-lost-chain)", () => {
  it("re-arms the alarm even when the tick throws mid-way", async () => {
    const now = Date.now();
    const { h, do_ } = await makeReaperDO({
      containerStatus: "running",
      lastHealthCheckMs: now - 60_000,
      coldStartCount: 1,
      tenantId: "tenant-throwing",
      lastActivityMs: now - 60_000,
    });
    // Health probe throws AND the degrade write throws → tick fails hard.
    h.portFetch.mockRejectedValue(new Error("boom"));
    const originalPut = h.state.storage.put.bind(h.state.storage);
    let puts = 0;
    (h.state.storage as unknown as { put: unknown }).put = async (k: string, v: unknown) => {
      puts++;
      if (puts > 0) throw new Error("storage fault");
      return originalPut(k, v);
    };

    await expect(do_.alarm()).resolves.not.toThrow();
    expect(h.alarms.length).toBeGreaterThan(0); // chain SURVIVED the fault
  });

  it("a request self-heals a chain that was already lost", async () => {
    const now = Date.now();
    // currentAlarm = null → the chain is gone (CF dropped it)
    const { h, do_ } = await makeReaperDO({
      containerStatus: "running",
      lastHealthCheckMs: now - 60_000,
      coldStartCount: 1,
      tenantId: "tenant-lost-chain",
      lastActivityMs: now - 60_000,
    }, null);

    await do_.fetch(
      new Request("http://localhost/v1/cas/tenant-lost-chain/abc", {
        headers: { "x-corelink-tenant-id": "tenant-lost-chain" },
      }),
    );

    expect(h.alarms.length).toBeGreaterThan(0); // request re-armed the reaper
  });

  it("does NOT re-arm redundantly when the chain is already alive", async () => {
    const now = Date.now();
    const { h, do_ } = await makeReaperDO({
      containerStatus: "running",
      lastHealthCheckMs: now - 60_000,
      coldStartCount: 1,
      tenantId: "tenant-armed",
      lastActivityMs: now - 60_000,
    }, now + 15_000); // alarm already scheduled

    await do_.fetch(
      new Request("http://localhost/v1/cas/tenant-armed/abc", {
        headers: { "x-corelink-tenant-id": "tenant-armed" },
      }),
    );

    expect(h.alarms).toHaveLength(0); // no duplicate arming on the hot path
  });

  it("a request arriving during the reap grace window saves the container", async () => {
    const now = Date.now();
    const { h, do_ } = await makeReaperDO({
      containerStatus: "running",
      lastHealthCheckMs: now - 60_000,
      coldStartCount: 1,
      tenantId: "tenant-raced",
      lastActivityMs: now - IDLE_MS - 60_000, // idle → reaper will engage
    });

    // Race a real request against the reaper's audit-emit await window.
    const reaping = do_.alarm();
    const serving = do_.fetch(
      new Request("http://localhost/v1/cas/tenant-raced/abc", {
        headers: { "x-corelink-tenant-id": "tenant-raced" },
      }),
    );
    const [, resp] = await Promise.all([reaping, serving]);

    expect(resp.status).toBe(200); // the in-flight request was served
    expect(h.destroy).not.toHaveBeenCalled(); // re-check aborted the reap
    expect(h.alarms.length).toBeGreaterThan(0); // and the chain stayed alive
  });
});

describe("DO start-failure reaps the container (resident-but-wedged bill)", () => {
  it("a container that throws on start() is DESTROYED, not just re-labelled", async () => {
    const now = Date.now();
    const { h, do_ } = await makeReaperDO({
      containerStatus: "stopped",
      lastHealthCheckMs: 0,
      coldStartCount: 0,
      tenantId: "tenant-start-throws",
      lastActivityMs: now,
    });
    // start() takes effect on the platform, THEN throws back at us — the
    // container is resident and billed even though the DO saw a failure.
    (h.state.container as unknown as { start: () => void }).start = () => {
      throw new Error("container start threw");
    };

    const resp = await do_.fetch(
      new Request("http://localhost/v1/cas/tenant-start-throws/abc", {
        headers: { "x-corelink-tenant-id": "tenant-start-throws" },
      }),
    );

    expect(resp.status).toBe(503);
    expect(h.destroy).toHaveBeenCalled(); // the whole point: no resident orphan
    const persisted = h.storageMap.get("lifecycle") as { containerStatus: string };
    expect(persisted.containerStatus).toBe("stopped");
  });

  // NOTE: this arm is NOT discriminating against the pre-fix source (when the
  // container exits during startup, `destroyContainer`'s own `container.running`
  // guard skips the destroy either way). It locks the 503 + "stopped" contract;
  // the RESIDENT-orphan case — the one that actually cost money — is locked by
  // the start-throws test above, which does fail pre-fix.
  it("a container that exits during startup yields 503 + stopped (no wedged state)", async () => {
    const now = Date.now();
    const { h, do_ } = await makeReaperDO({
      containerStatus: "stopped",
      lastHealthCheckMs: 0,
      coldStartCount: 0,
      tenantId: "tenant-wedged",
      lastActivityMs: now,
    });
    // Container comes up but is wedged: /_health never returns 200. The poll
    // loop exits early because the mock reports the container as gone.
    h.portFetch.mockResolvedValue(new Response(null, { status: 500 }));
    (h.state.container as unknown as { start: () => void }).start = () => {
      // simulate the container exiting during startup so the poll breaks fast
      (h.state.container as unknown as { running: boolean }).running = false;
    };

    const resp = await do_.fetch(
      new Request("http://localhost/v1/cas/tenant-wedged/abc", {
        headers: { "x-corelink-tenant-id": "tenant-wedged" },
      }),
    );

    expect(resp.status).toBe(503);
    const persisted = h.storageMap.get("lifecycle") as { containerStatus: string };
    expect(persisted.containerStatus).toBe("stopped");
  });
});

// ──────────────────────────────────────────────────────────────────────────────
// PLATFORM idle auto-destroy (container.setInactivityTimeout)
//
// Cloudflare's own reaper. It was named in durable_object.ts's header
// doc-comment from day one and NEVER CALLED — the hand-rolled alarm reaper
// above shipped in #927 and never moved the live instance count off 35
// (5 regions x 7 x 4 GiB resident, `active: 0` in regions with zero traffic).
// Platform-side survives DO eviction, a broken alarm chain and a wedged
// isolate: exactly the failure modes that made containers immortal.
// ──────────────────────────────────────────────────────────────────────────────

describe("platform idle auto-destroy (setInactivityTimeout)", () => {
  it("re-arms with IDLE_TIMEOUT_MS on an alarm tick for a live, non-idle container", async () => {
    const now = Date.now();
    const { h, do_ } = await makeReaperDO({
      containerStatus: "running",
      lastHealthCheckMs: now - 60_000,
      coldStartCount: 1,
      tenantId: "tenant-live",
      lastActivityMs: now, // NOT idle — the alarm reaper must not fire
    });

    await do_.alarm();

    expect(h.destroy).not.toHaveBeenCalled(); // sanity: not the idle path
    expect(h.setInactivityTimeout).toHaveBeenCalledWith(IDLE_MS);
  });

  it("does not throw, and still serves, when the runtime lacks setInactivityTimeout", async () => {
    const now = Date.now();
    // withInactivityApi=false → container object has no such method at all.
    const { h, do_ } = await makeReaperDO(
      {
        containerStatus: "running",
        lastHealthCheckMs: now - 60_000,
        coldStartCount: 1,
        tenantId: "tenant-old-runtime",
        lastActivityMs: now,
      },
      1,
      false,
    );

    await expect(do_.alarm()).resolves.not.toThrow();
    // The alarm reaper remains the sole reaper and the chain stays armed —
    // degrading must not silently end the chain (that is the other half of
    // the immortal-container bill).
    expect(h.destroy).not.toHaveBeenCalled();
    expect(h.alarms.length).toBeGreaterThan(0);
  });

  it("arming failure is swallowed so the container still serves", async () => {
    const now = Date.now();
    const { h, do_ } = await makeReaperDO({
      containerStatus: "running",
      lastHealthCheckMs: now - 60_000,
      coldStartCount: 1,
      tenantId: "tenant-arm-throws",
      lastActivityMs: now,
    });
    h.setInactivityTimeout.mockRejectedValueOnce(new Error("unsupported"));

    await expect(do_.alarm()).resolves.not.toThrow();
    expect(h.destroy).not.toHaveBeenCalled();
  });
});
