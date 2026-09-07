import { describe, it, expect } from "vitest";
import { CoreLinkServer, timingSafeEqual, makeMockState, makeEnv, makeReaperDO, IDLE_MS } from "./durable_object_part2_test_helpers.js";

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
describe("DO durable idle reaper (immortal-container regression)", () => {
  it("keeps signup admission state across a DO instance recycle", async () => {
    const now = Date.now();
    const { h, do_: first } = await makeReaperDO({
      containerStatus: "running",
      tenantId: null,
      lastHealthCheckMs: now,
      lastActivityMs: now,
      coldStartCount: 1,
    });
    const request = () =>
      new Request("http://localhost/v1/signup/pilot", {
        method: "POST",
        headers: { "x-corelink-client-ip": "203.0.113.7" },
      });
    for (let i = 0; i < 5; i += 1) {
      expect((await first.fetch(request())).status).not.toBe(429);
    }
    // A new JS instance sharing the same durable storage must not receive a
    // fresh burst after the old isolate is gone.
    const second = new CoreLinkServer(h.state, makeEnv());
    await new Promise<void>((resolve) => setTimeout(resolve, 5));
    const denied = await second.fetch(request());
    expect(denied.status).toBe(429);
    expect(denied.headers.get("retry-after")).toBeTruthy();
  });

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
