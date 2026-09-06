import { execFileSync } from "node:child_process";
import { cpSync, existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { tmpdir } from "node:os";
import { describe, expect, it } from "vitest";

const focal = resolve(process.cwd(), "playwright/e2e");
const scriptCandidates = [
  resolve(process.cwd(), "../../scripts/verify-b069-e2e.sh"),
  resolve(process.cwd(), "scripts/verify-b069-e2e.sh"),
];
const verifier = scriptCandidates.find((candidate) => existsSync(candidate));
if (!verifier) throw new Error("B-069 verifier script is missing");

function run(dir: string): void {
  execFileSync("bash", [verifier!], {
    env: { ...process.env, B069_E2E_DIR: dir },
    stdio: "pipe",
  });
}

function copyFocal(): string {
  const dir = mkdtempSync(join(tmpdir(), "corelink-b069-"));
  cpSync(focal, dir, { recursive: true });
  return dir;
}

describe("B-069 fail-closed verifier", () => {
  it("accepts the checked-in focal journeys", () => {
    expect(() => run(focal)).not.toThrow();
  });

  it("rejects a mutant that restores test.fixme", () => {
    const dir = copyFocal();
    try {
      const file = join(dir, "04-dsr-access.spec.ts");
      const source = readFileSync(file, "utf8");
      writeFileSync(file, source.replace("test(\"fresh MFA", "test.fixme(\"fresh MFA"));
      expect(() => run(dir)).toThrow();
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("rejects a mutant that removes all executable expect assertions", () => {
    const dir = copyFocal();
    try {
      const file = join(dir, "05-dsr-erasure.spec.ts");
      const source = readFileSync(file, "utf8");
      writeFileSync(file, source.replaceAll("expect", "assert").replaceAll("toBe", "is"));
      expect(() => run(dir)).toThrow();
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("rejects a mutant that removes a currently shipped journey", () => {
    const dir = copyFocal();
    try {
      rmSync(join(dir, "06-admin-audit-viewer.spec.ts"));
      expect(() => run(dir)).toThrow();
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});
