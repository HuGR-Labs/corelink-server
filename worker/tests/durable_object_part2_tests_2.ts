import { describe, it, expect } from "vitest";
import { CoreLinkServer, timingSafeEqual, makeMockState, makeEnv, makeReaperDO, IDLE_MS } from "./durable_object_part2_test_helpers.js";

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
