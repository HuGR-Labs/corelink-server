/**
 * Grep-style guard: assert that nowhere in the onboarding sources do we
 * write a PAT to localStorage or echo it via console.log.
 *
 * This complements the runtime state-persistence test by verifying the
 * static source tree, mirroring the CI grep gate in the WI spec.
 */
import { describe, it, expect } from "vitest";
import { promises as fs } from "node:fs";
import path from "node:path";

const FORBIDDEN = [
  // localStorage at all is forbidden in the onboarding code path (we use
  // sessionStorage only); a specific PAT-targeted write would be even worse.
  /localStorage\s*\.\s*setItem/,
  // any console log/error/warn that mentions a PAT plaintext variable.
  /console\.(log|error|warn)\([^)]*(plaintext|pat\.plaintext|getPlaintextPat)/i,
];

const ROOT = path.resolve(__dirname, "../../../../");

async function walk(dir: string, out: string[] = []): Promise<string[]> {
  const entries = await fs.readdir(dir, { withFileTypes: true });
  for (const e of entries) {
    if (e.name === "node_modules" || e.name === ".next") continue;
    const full = path.join(dir, e.name);
    if (e.isDirectory()) await walk(full, out);
    else if (/\.(ts|tsx)$/.test(e.name) && !/__tests__/.test(full)) {
      out.push(full);
    }
  }
  return out;
}

describe("source guard: no PAT in localStorage or console logs", () => {
  it("scans all .ts/.tsx files under src/", async () => {
    const files = await walk(ROOT);
    expect(files.length).toBeGreaterThan(5);
    const violations: { file: string; pattern: string }[] = [];
    for (const f of files) {
      const text = await fs.readFile(f, "utf8");
      for (const pat of FORBIDDEN) {
        if (pat.test(text)) {
          violations.push({ file: f, pattern: pat.toString() });
        }
      }
    }
    expect(violations).toEqual([]);
  });
});
