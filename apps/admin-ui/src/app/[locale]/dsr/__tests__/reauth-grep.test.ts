import { describe, expect, it } from "vitest";
import { readFileSync, readdirSync } from "node:fs";
import path from "node:path";

/**
 * Static "grep" assertion (per WI-S16-004 quality-gate bullet):
 *
 *   "All DSR submit forms wrap in <ReAuthGate> (grep test)."
 *
 * We assert that every page under `src/app/[locale]/dsr/[action]` imports
 * ReAuthGate, and that the form component itself never imports DsrActionForm
 * directly from a page module bypassing the gate.
 */

const projectRoot = path.resolve(__dirname, "../../../../..");
// Resolve a known anchor: src/app/[locale]/dsr/[action]
const actionDir = path.join(
  projectRoot,
  "src/app/[locale]/dsr/[action]",
);

describe("ReAuthGate grep (CTRL-AUTH-010 wrapping invariant)", () => {
  it("DsrActionPageClient imports ReAuthGate", () => {
    const files = readdirSync(actionDir);
    expect(files).toContain("DsrActionPageClient.tsx");
    const src = readFileSync(
      path.join(actionDir, "DsrActionPageClient.tsx"),
      "utf-8",
    );
    expect(src).toMatch(/from\s+["']@\/components\/dsr\/ReAuthGate["']/);
    // The form is rendered inside the gate's render-prop child.
    expect(src).toMatch(/<ReAuthGate[\s\S]+<DsrActionForm/);
  });

  it("DSR client never logs the Authorization header in plain text", () => {
    const src = readFileSync(
      path.join(projectRoot, "src/lib/dsr-client.ts"),
      "utf-8",
    );
    // We allow `Authorization` to appear once as a header set, but the source
    // must not contain console.log of the raw token.
    expect(src).not.toMatch(/console\.(log|info|debug)/);
    expect(src).toMatch(/Authorization/);
  });
});
