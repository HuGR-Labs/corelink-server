import fs from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";
import {
  classify,
  type BetterStackBadgePayload,
} from "../src/components/StatusPill/classify";

// The docs navbar StatusPill used to render a green "All systems
// operational" pill on EVERY page load, because its default badge endpoint
// (`status.corelink.humangr.com`) never resolved/handshaked and the
// component's fetch-failure fallback was "assume good". These tests pin
// (1) the real BetterStack `/index.json` shape maps to the right severity,
// (2) a failed/empty fetch never yields "ok" without evidence, and
// (3) the real endpoint's origin is actually reachable per the CSP.

function aggregatePayload(state: string): BetterStackBadgePayload {
  return { data: { attributes: { aggregate_state: state } } };
}

describe("StatusPill classify()", () => {
  it("maps every observed aggregate_state value", () => {
    expect(classify(aggregatePayload("operational"))).toBe("ok");
    expect(classify(aggregatePayload("degraded"))).toBe("degraded");
    expect(classify(aggregatePayload("maintenance"))).toBe("degraded");
    expect(classify(aggregatePayload("downtime"))).toBe("down");
    expect(classify(aggregatePayload("outage"))).toBe("down");
  });

  it("still maps the legacy badge.json `status` shape", () => {
    expect(classify({ status: "up" })).toBe("ok");
    expect(classify({ status: "down" })).toBe("down");
    expect(classify({ status: "outage" })).toBe("down");
    expect(classify({ status: "degraded" })).toBe("degraded");
    expect(classify({ status: "maintenance" })).toBe("degraded");
  });

  it("still maps the legacy Atlassian-compatible `indicator` shape", () => {
    expect(classify({ indicator: "none" })).toBe("ok");
    expect(classify({ indicator: "minor" })).toBe("degraded");
    expect(classify({ indicator: "major" })).toBe("down");
    expect(classify({ indicator: "critical" })).toBe("down");
  });

  it("classifies an unrecognised or empty payload as unknown, never ok", () => {
    expect(classify({})).toBe("unknown");
    expect(classify({ status: "validating" })).toBe("unknown");
    expect(classify({ status: "paused" })).toBe("unknown");
    expect(classify(aggregatePayload("something-new"))).toBe("unknown");
  });
});

describe("StatusPill source — no unverified 'ok' default", () => {
  const source = fs.readFileSync(
    path.resolve(__dirname, "..", "src", "components", "StatusPill", "StatusPill.tsx"),
    "utf8",
  );

  it("defaults the cold-render / fetch-failure severity to unknown, not ok", () => {
    // The cache-miss initial useState fallback must be "unknown".
    expect(source).toMatch(/cached\?\.severity\s*\?\?\s*"unknown"/);
    // Guard against the old regression pattern reappearing.
    expect(source).not.toMatch(/cached\?\.severity\s*\?\?\s*"ok"/);
  });

  it("fetches the real JSON endpoint (/index.json), not the HTML badge.json", () => {
    expect(source).toMatch(/statusBadgeUrl\s*=\s*`\$\{statuspageUrl\.replace\([^)]*\)\}\/index\.json`/);
  });
});

describe("StatusPill status endpoint — CSP wiring", () => {
  it("allows the live BetterStack index.json origin in connect-src", () => {
    const headers = fs.readFileSync(
      path.resolve(__dirname, "..", "static", "_headers"),
      "utf8",
    );
    const connectSrc = headers.match(
      /Content-Security-Policy:[^\n]*?connect-src ([^;]+);/,
    )?.[1];
    expect(connectSrc).toBeDefined();
    expect(connectSrc?.split(/\s+/)).toContain("https://hugrl.betteruptime.com");
    // The dead third-level host must be gone, not just supplemented.
    expect(connectSrc).not.toContain("https://status.corelink.humangr.com");
  });

  it("the default statuspage URL points at the live host", async () => {
    const { DEFAULT_STATUSPAGE_URL } = await import("../src/statuspage-url");
    expect(DEFAULT_STATUSPAGE_URL).toBe("https://hugrl.betteruptime.com");
  });
});
