import { Buffer } from "node:buffer";
import { mkdtempSync, readFileSync, rmSync, statSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { describe, expect, it } from "vitest";

import CoreLinkA11yReporter, {
  parseAuditAttachment,
  renderSummary,
  type AuditResult,
} from "../scripts/a11y-reporter";

const result: AuditResult = {
  route: "/corelink/docs/",
  url: "http://localhost:3000/corelink/docs/",
  violations: [
    {
      id: "color-contrast",
      impact: "serious",
      help: "Elements must meet minimum color contrast ratio thresholds",
      helpUrl: "https://dequeuniversity.com/rules/axe/color-contrast",
      nodes: 2,
      tags: ["wcag2aa"],
    },
  ],
};

describe("CoreLink a11y reporter", () => {
  it("parses the normalized audit attachment", () => {
    expect(parseAuditAttachment(Buffer.from(JSON.stringify(result)))).toEqual(result);
  });

  it("rejects malformed attachments instead of emitting false evidence", () => {
    expect(() => parseAuditAttachment(Buffer.from('{"route":"/"}'))).toThrow(
      "invalid corelink-a11y-result attachment",
    );
    expect(() =>
      parseAuditAttachment(
        Buffer.from(
          JSON.stringify({ ...result, violations: [{ id: "color-contrast" }] }),
        ),
      ),
    ).toThrow("invalid violation in corelink-a11y-result attachment");
  });

  it("renders the stable severity summary", () => {
    const summary = renderSummary([result]);
    expect(summary).toContain("| `/corelink/docs/` | 1 | 0 | 1 | 0 | 0 |");
    expect(summary).toContain(
      "| **TOTAL** | **1** | **0** | **1** | **0** | **0** |",
    );
  });

  it("records runner failures instead of emitting empty evidence", () => {
    const directory = mkdtempSync(join(tmpdir(), "corelink-a11y-reporter-"));
    const outputFile = join(directory, "report.json");
    try {
      const reporter = new CoreLinkA11yReporter({ outputFile });
      reporter.onTestEnd(
        { title: "a11y: /corelink/docs/" } as never,
        {
          attachments: [],
          errors: [{ message: "browser unavailable" }],
          status: "failed",
        } as never,
      );
      reporter.onEnd({} as never);

      expect(JSON.parse(readFileSync(outputFile, "utf8"))).toEqual([
        expect.objectContaining({
          route: "/corelink/docs/",
          error: "browser unavailable",
          violations: [],
        }),
      ]);
      expect(statSync(outputFile).mode & 0o777).toBe(0o600);
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  });
});
