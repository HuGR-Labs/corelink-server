import { Buffer } from "node:buffer";
import {
  mkdtempSync,
  readFileSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { describe, expect, it } from "vitest";

import { A11Y_ROUTES } from "../scripts/a11y-routes";
import CoreLinkA11yReporter, {
  parseAuditAttachment,
  promoteBaselineAtomically,
  renderSummary,
  validateBaselineCandidate,
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
    expect(summary).toContain(
      "**Audit status: FAILED — 0 runtime error(s), 1 blocking violation(s).**",
    );
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
      const summary = renderSummary(JSON.parse(readFileSync(outputFile, "utf8")));
      expect(summary).toContain(
        "**Audit status: FAILED — 1 runtime error(s), 0 blocking violation(s).**",
      );
      expect(summary).toContain("browser unavailable");
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  });

  it("never mutates a baseline for failed or incomplete evidence", () => {
    const directory = mkdtempSync(join(tmpdir(), "corelink-a11y-baseline-"));
    const baseline = join(directory, "baseline.json");
    const original = '[{"known":"baseline"}]\n';
    writeFileSync(baseline, original);
    try {
      expect(() =>
        promoteBaselineAtomically(
          baseline,
          [{ ...result, violations: [], error: "audit crashed" }],
          A11Y_ROUTES,
        ),
      ).toThrow("expected 15 routes");
      expect(readFileSync(baseline, "utf8")).toBe(original);

      const failed = A11Y_ROUTES.map((route, index) => ({
        ...result,
        route,
        ...(index === 7 ? { error: "audit crashed" } : {}),
      }));
      expect(() =>
        promoteBaselineAtomically(baseline, failed, A11Y_ROUTES),
      ).toThrow("1 runtime failure(s)");
      expect(readFileSync(baseline, "utf8")).toBe(original);
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  });

  it("promotes a complete 15-route baseline atomically", () => {
    const directory = mkdtempSync(join(tmpdir(), "corelink-a11y-baseline-"));
    const baseline = join(directory, "baseline.json");
    try {
      const complete = A11Y_ROUTES.map((route) => ({
        ...result,
        route,
        violations: [],
      }));
      expect(() => validateBaselineCandidate(complete, A11Y_ROUTES)).not.toThrow();
      promoteBaselineAtomically(baseline, complete, A11Y_ROUTES);
      expect(JSON.parse(readFileSync(baseline, "utf8"))).toHaveLength(15);
      expect(statSync(baseline).mode & 0o777).toBe(0o644);
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  });

  it("rejects a complete route set when the audit gate found blockers", () => {
    const blocked = A11Y_ROUTES.map((route) => ({ ...result, route }));
    expect(() => validateBaselineCandidate(blocked, A11Y_ROUTES)).toThrow(
      "15 blocking violation(s)",
    );
  });
});
