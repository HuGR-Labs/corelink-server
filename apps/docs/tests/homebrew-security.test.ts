import fs from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";

const DOCS_ROOT = path.resolve(__dirname, "..");
const FIXTURE = path.join(
  __dirname,
  "fixtures",
  "homebrew-no-env-proof.txt",
);

const HOMEBREW_DOCS = [
  "docs/integrations/homebrew.md",
  "i18n/de/docusaurus-plugin-content-docs/current/integrations/homebrew.md",
  "i18n/es-419/docusaurus-plugin-content-docs/current/integrations/homebrew.md",
  "i18n/pt-BR/docusaurus-plugin-content-docs/current/integrations/homebrew.md",
] as const;

const CONTRACT_MARKER = "WP-B161-AUTH-NO-FALLBACK-20260901";

/**
 * Scan shell command blocks, not prose. A registry token is safe only when the
 * same block pins the artifact domain and disables Homebrew's upstream
 * fallback; otherwise a failed mirror can disclose the bearer to ghcr.io.
 */
export function scanHomebrewDocs(source: string): string[] {
  const violations: string[] = [];
  const lines = source.split(/\r?\n/);
  let block: { start: number; lines: string[] } | undefined;
  const inspectBlock = (current: { start: number; lines: string[] }) => {
    const hasArtifact = current.lines.some((line) =>
      /^\s*(?:export\s+)?HOMEBREW_ARTIFACT_DOMAIN\s*=\s*\S+/i.test(line),
    );
    const hasNoFallback = current.lines.some((line) =>
      /^\s*(?:export\s+)?HOMEBREW_ARTIFACT_DOMAIN_NO_FALLBACK\s*=\s*1\s*(?:#.*)?$/i.test(
        line,
      ),
    );
    current.lines.forEach((line, offset) => {
      const lineNumber = current.start + offset + 1;
      const token = /^\s*(?:export\s+)?HOMEBREW_DOCKER_REGISTRY_TOKEN\s*=/i.test(
        line,
      );
      const artifact = /^\s*(?:export\s+)?HOMEBREW_ARTIFACT_DOMAIN\s*=/i.test(
        line,
      );
      const bottle = /^\s*(?:export\s+)?HOMEBREW_BOTTLE_DOMAIN\s*=/i.test(
        line,
      );
      const noFallback =
        /^\s*(?:export\s+)?HOMEBREW_ARTIFACT_DOMAIN_NO_FALLBACK\s*=/i.test(
          line,
        );
      if (token && (!hasArtifact || !hasNoFallback)) {
        violations.push(`credential assignment without pinned mirror on line ${lineNumber}`);
      }
      if (bottle) {
        violations.push(`unauthenticated bottle domain assignment on line ${lineNumber}`);
      }
      if (artifact && !hasNoFallback) {
        violations.push(`artifact domain without no-fallback on line ${lineNumber}`);
      }
      if (noFallback && !hasArtifact) {
        violations.push(`no-fallback without artifact domain on line ${lineNumber}`);
      }
    });
  };
  lines.forEach((line, index) => {
    if (/^\s*```(?:bash|sh|shell)\s*$/i.test(line)) {
      if (block) inspectBlock(block);
      block = { start: index + 1, lines: [] };
      return;
    }
    if (/^\s*```\s*$/.test(line)) {
      if (block) inspectBlock(block);
      block = undefined;
      return;
    }
    if (block) {
      block.lines.push(line);
      return;
    }
    if (
      /^\s*(?:export\s+)?(?:CORELINK_PAT|HOMEBREW_(?:ARTIFACT_DOMAIN|BOTTLE_DOMAIN|ARTIFACT_DOMAIN_NO_FALLBACK|DOCKER_REGISTRY_TOKEN))\s*=/i.test(
        line,
      )
    ) {
      violations.push(`active Homebrew assignment outside shell block on line ${index + 1}`);
    }
  });
  if (block) inspectBlock(block);
  if (/\bcorelink_pat_[A-Za-z0-9_.-]+/i.test(source)) {
    violations.push("PAT-shaped literal in documentation");
  }
  return violations;
}

describe("Homebrew documentation safety", () => {
  it("keeps the focused no-env evidence fixture deterministic and secret-free", () => {
    const fixture = fs.readFileSync(FIXTURE, "utf8");
    expect(fixture).toContain(CONTRACT_MARKER);
    expect(fixture).toContain("result: exit 0");
    expect(fixture).toContain("corelink_environment: absent");
    expect(fixture).not.toMatch(/corelink_pat_|HOMEBREW_DOCKER_REGISTRY_TOKEN=/i);
  });

  it("covers every locale with the safe authenticated classification", () => {
    const localizedHomebrewDocs = fs
      .readdirSync(path.join(DOCS_ROOT, "i18n"), { withFileTypes: true })
      .filter((entry) => entry.isDirectory())
      .map(
        (entry) =>
          `i18n/${entry.name}/docusaurus-plugin-content-docs/current/integrations/homebrew.md`,
      )
      .filter((relativePath) =>
        fs.existsSync(path.join(DOCS_ROOT, relativePath)),
      );
    expect([
      "docs/integrations/homebrew.md",
      ...localizedHomebrewDocs,
    ].sort()).toEqual([...HOMEBREW_DOCS].sort());

    const retainedByLocale = [
      "This page is retained",
      "bleibt erhalten, weil",
      "Esta página se conserva",
      "é mantida porque",
    ];
    for (const [index, relativePath] of HOMEBREW_DOCS.entries()) {
      const filePath = path.join(DOCS_ROOT, relativePath);
      expect(fs.existsSync(filePath), `${relativePath} must exist`).toBe(true);
      const source = fs.readFileSync(filePath, "utf8");
      expect(source.length, `${relativePath} must not be truncated`).toBeGreaterThan(
        1800,
      );
      expect(source).toContain(CONTRACT_MARKER);
      expect(source).toContain("HOMEBREW_ARTIFACT_DOMAIN");
      expect(source).toContain("HOMEBREW_ARTIFACT_DOMAIN_NO_FALLBACK");
      expect(source).toContain("HOMEBREW_BOTTLE_DOMAIN");
      expect(source).toContain("HOMEBREW_DOCKER_REGISTRY_TOKEN");
      expect(source).toMatch(/401/);
      expect(source).toMatch(/403/);
      expect(source).toContain(retainedByLocale[index]);
      expect(scanHomebrewDocs(source)).toEqual([]);
    }
  });

  it("turns red when a token export is added without the pinned mirror", () => {
    const source = fs.readFileSync(
      path.join(DOCS_ROOT, HOMEBREW_DOCS[0]),
      "utf8",
    );
    const mutated = `${source}\n\nexport HOMEBREW_DOCKER_REGISTRY_TOKEN=\"$TOKEN\"\n`;
    expect(scanHomebrewDocs(mutated)).not.toEqual([]);
  });

  it("turns red when the no-fallback pin is removed", () => {
    const source = fs.readFileSync(
      path.join(DOCS_ROOT, HOMEBREW_DOCS[0]),
      "utf8",
    );
    const mutated = source.replace(
      /^export HOMEBREW_ARTIFACT_DOMAIN_NO_FALLBACK=1\n/m,
      "",
    );
    const firstShellBlock = mutated.match(/```(?:bash|sh|shell)\s*\n([\s\S]*?)```/i)?.[1];
    expect(firstShellBlock).toBeDefined();
    expect(firstShellBlock).not.toMatch(
      /^export HOMEBREW_ARTIFACT_DOMAIN_NO_FALLBACK=1$/m,
    );
    expect(scanHomebrewDocs(mutated)).toEqual(
      expect.arrayContaining([
        expect.stringMatching(/^artifact domain without no-fallback on line \d+$/),
      ]),
    );
  });

  it("turns red when the legacy bottle domain is added", () => {
    const source = fs.readFileSync(
      path.join(DOCS_ROOT, HOMEBREW_DOCS[0]),
      "utf8",
    );
    const mutated = source.replace(
      /^export HOMEBREW_ARTIFACT_DOMAIN_NO_FALLBACK=1$/m,
      '$&\nexport HOMEBREW_BOTTLE_DOMAIN="https://mirror.example.invalid"',
    );
    expect(mutated).toContain("HOMEBREW_BOTTLE_DOMAIN=");
    expect(scanHomebrewDocs(mutated)).toEqual(
      expect.arrayContaining([
        expect.stringMatching(
          /^unauthenticated bottle domain assignment on line \d+$/,
        ),
      ]),
    );
  });
});
