import { Buffer } from "node:buffer";
import { readFileSync } from "node:fs";

import {
  parseAuditAttachment,
  promoteBaselineAtomically,
  type AuditResult,
} from "./a11y-reporter";
import { A11Y_ROUTES } from "./a11y-routes";

const [reportPath, baselinePath] = process.argv.slice(2);
if (!reportPath || !baselinePath) {
  throw new Error("usage: promote-a11y-baseline.ts <report.json> <baseline.json>");
}

const raw: unknown = JSON.parse(readFileSync(reportPath, "utf8"));
if (!Array.isArray(raw)) throw new Error("a11y report must be a JSON array");
const results: AuditResult[] = raw.map((entry) =>
  parseAuditAttachment(Buffer.from(JSON.stringify(entry))),
);
promoteBaselineAtomically(baselinePath, results, A11Y_ROUTES);
