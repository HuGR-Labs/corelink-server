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
      "**Audit status: FAILED — 0 runtime error(s); 1 blocking violation(s).**",
    );
    expect(summary).toContain("| `/corelink/docs/` | 1 | 0 | 1 | 0 | 0 |");
    expect(summary).toContain(
      "| **TOTAL** | **1** | **0** | **1** | **0** | **0** |",
    );
  });

  it("records runner failures instead of emitting empty evidence", () => {
    const directory = mkdtempSync(join(tmpdir(), "corelink-a11y-reporter-"));
    const outputFile = join(directory, "report.json");
    const summaryFile = join(directory, "report.md");
    try {
      const reporter = new CoreLinkA11yReporter({ outputFile, summaryFile });
      reporter.onTestEnd(
        { title: "a11y: /corelink/docs/" } as never,
        {
          attachments: [],
          errors: [{ message: "browser unavailable" }],
          status: "failed",
        } as never,
      );
      reporter.onEnd({ status: "failed" } as never);

      expect(JSON.parse(readFileSync(outputFile, "utf8"))).toEqual([
        expect.objectContaining({
          route: "/corelink/docs/",
          error: "browser unavailable",
          violations: [],
        }),
      ]);
      expect(statSync(outputFile).mode & 0o777).toBe(0o600);
      const summary = readFileSync(summaryFile, "utf8");
      expect(summary).toContain("**Audit status: FAILED");
      expect(summary).toContain("run status=failed");
      expect(summary).toContain("route coverage=1/15");
      expect(summary).toContain("1 runtime error(s)");
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
      const previousUmask = process.umask(0o077);
      try {
        promoteBaselineAtomically(baseline, complete, A11Y_ROUTES);
      } finally {
        process.umask(previousUmask);
      }
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

  it("marks empty, partial, and globally failed runs as FAILED", () => {
    const empty = renderSummary([], {
      runStatus: "passed",
      expectedRoutes: A11Y_ROUTES,
    });
    expect(empty).toContain("Audit status: FAILED");
    expect(empty).toContain("route coverage=0/15");

    const partial = renderSummary(
      [{ ...result, violations: [] }],
      { runStatus: "passed", expectedRoutes: A11Y_ROUTES },
    );
    expect(partial).toContain("Audit status: FAILED");
    expect(partial).toContain("route coverage=1/15");

    const complete = A11Y_ROUTES.map((route) => ({
      ...result,
      route,
      violations: [],
    }));
    const interrupted = renderSummary(complete, {
      runStatus: "interrupted",
      expectedRoutes: A11Y_ROUTES,
    });
    expect(interrupted).toContain("Audit status: FAILED");
    expect(interrupted).toContain("run status=interrupted");
  });

  it("onEnd marks an empty passed run as incomplete", () => {
    const directory = mkdtempSync(join(tmpdir(), "corelink-a11y-empty-"));
    const outputFile = join(directory, "report.json");
    const summaryFile = join(directory, "report.md");
    try {
      const reporter = new CoreLinkA11yReporter({ outputFile, summaryFile });
      reporter.onEnd({ status: "passed" } as never);
      const summary = readFileSync(summaryFile, "utf8");
      expect(summary).toContain("Audit status: FAILED");
      expect(summary).toContain("route coverage=0/15");
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  });

  it("onEnd honors a global failure for an otherwise complete run", () => {
    const directory = mkdtempSync(join(tmpdir(), "corelink-a11y-global-"));
    const outputFile = join(directory, "report.json");
    const summaryFile = join(directory, "report.md");
    try {
      const reporter = new CoreLinkA11yReporter({ outputFile, summaryFile });
      for (const route of A11Y_ROUTES) {
        reporter.onTestEnd(
          { title: `a11y: ${route}` } as never,
          {
            attachments: [
              {
                name: "corelink-a11y-result",
                body: Buffer.from(
                  JSON.stringify({ ...result, route, violations: [] }),
                ),
              },
            ],
            errors: [],
            status: "passed",
          } as never,
        );
      }
      reporter.onEnd({ status: "interrupted" } as never);
      const summary = readFileSync(summaryFile, "utf8");
      expect(summary).toContain("Audit status: FAILED");
      expect(summary).toContain("run status=interrupted");
      expect(summary).not.toContain("route coverage=");
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  });
});
