/**
 * Config gate — the `ANALYTICS_DB` D1 binding must exist in the admin-ui Worker
 * config, and must point at the SAME database that `apps/analytics-worker`
 * writes `analytics_events` into.
 *
 * W7 defect 1: apps/admin-ui/wrangler.toml had no `[[d1_databases]]` block at
 * all, so `env.ANALYTICS_DB` was permanently `undefined` and
 * `/api/welcome/stream` short-circuited to heartbeats forever. Nothing at
 * build- or test-time noticed, because a missing binding is not a type error.
 *
 * The id is asserted against apps/analytics-worker/wrangler.toml rather than a
 * hard-coded literal so the two configs cannot drift apart silently.
 */

import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
const adminUiToml = readFileSync(resolve(here, "../wrangler.toml"), "utf8");
const analyticsToml = readFileSync(
  resolve(here, "../../analytics-worker/wrangler.toml"),
  "utf8",
);

/** Extract the `[[<header>]]` block that declares `binding = "<name>"`. */
function d1Block(toml: string, header: string, binding: string): string {
  const blocks: string[] = toml
    .split(new RegExp(`^\\[\\[${header}\\]\\]$`, "m"))
    .slice(1)
    // Each split tail runs to the next TOML header — trim it back.
    .map((tail) => tail.split(/^\[/m)[0] ?? "");
  const match = blocks.find((b) => b.includes(`binding = "${binding}"`));
  expect(match, `no [[${header}]] block binding ${binding}`).toBeDefined();
  return match as string;
}

function field(block: string, key: string): string | undefined {
  return new RegExp(`^${key} = "([^"]+)"$`, "m").exec(block)?.[1];
}

describe("admin-ui wrangler.toml ANALYTICS_DB binding", () => {
  const adminBlock = d1Block(adminUiToml, "d1_databases", "ANALYTICS_DB");
  const analyticsProdBlock = d1Block(
    analyticsToml,
    "env.prod.d1_databases",
    "ANALYTICS_DB",
  );

  it("declares the binding the welcome-stream route reads", () => {
    expect(field(adminBlock, "binding")).toBe("ANALYTICS_DB");
  });

  it("points at corelink-analytics-prod — the only DB holding analytics_events", () => {
    expect(field(adminBlock, "database_name")).toBe("corelink-analytics-prod");
  });

  it("uses the same database_id as apps/analytics-worker's prod binding", () => {
    const analyticsId = field(analyticsProdBlock, "database_id");
    expect(analyticsId).toMatch(/^[0-9a-f-]{36}$/);
    expect(field(adminBlock, "database_id")).toBe(analyticsId);
  });

  it("is NOT pointed at the control-plane database", () => {
    // corelink-config-prod (CONFIG_DB) — analytics_events does not live there.
    expect(field(adminBlock, "database_id")).not.toBe(
      "d64742ea-e102-40b2-a844-ff02e3f94562",
    );
  });

  it("does not claim ownership of the analytics migrations", () => {
    // apps/analytics-worker owns the schema; admin-ui is a read-only consumer.
    expect(field(adminBlock, "migrations_dir")).toBeUndefined();
  });
});
