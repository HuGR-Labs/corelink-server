/** Closed-population and active-wiring guard for B-126 M3.
 *
 * This verifier deliberately tokenises imports instead of searching substrings:
 * comments, strings, and dynamic imports cannot satisfy a domain boundary.
 */
import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const ROOT = resolve(import.meta.dirname, "..");

const originalRoots = [
  "src/durable_object.ts",
  "src/index.ts",
  "../apps/signup-worker/src/webhooks/clerk.ts",
  "../apps/signup-worker/src/webhooks/stripe.ts",
  "tests/durable_object.test.ts",
  "tests/runner_mint.test.ts",
  "tests/index.test.ts",
  "../apps/signup-worker/tests/clerk.test.ts",
  "../apps/signup-worker/tests/stripe.test.ts",
] as const;

const extractedFiles = [
  "src/durable_object_probes.ts",
  "src/durable_object_start.ts",
  "src/lib/devenv_cleanup_route.ts",
  "src/route_match.ts",
  "src/index_observability.ts",
  "src/index_auth.ts",
  "src/index_common.ts",
  "src/index_fetch.ts",
  "src/index_schedule.ts",
  "src/index_public_routes.ts",
  "src/index_special_routes.ts",
  "src/index_auth_stage.ts",
  "src/index_quota_stage.ts",
  "src/index_routing_stage.ts",
  "src/index_edge_stage.ts",
  "src/index_finish_stage.ts",
  "../apps/signup-worker/src/webhooks/clerk_identity.ts",
  "../apps/signup-worker/src/webhooks/clerk_erasure.ts",
  "../apps/signup-worker/src/webhooks/stripe_contract.ts",
  "../apps/signup-worker/src/webhooks/stripe_persistence.ts",
  "../apps/signup-worker/src/webhooks/stripe_signature.ts",
  "tests/b126_m3_refactor.test.ts",
  "tests/durable_object_part2.test.ts",
  "tests/runner_mint_part2.test.ts",
  "tests/runner_mint_part3.test.ts",
  "tests/index_part2.test.ts",
  "tests/index_part3.test.ts",
  "tests/index_part4.test.ts",
  "tests/index_part5.test.ts",
  "../apps/signup-worker/tests/clerk_part2.test.ts",
  "../apps/signup-worker/tests/stripe_contract_part.test.ts",
  "../apps/signup-worker/tests/stripe_handler_part2.test.ts",
  "../apps/signup-worker/tests/stripe_handler_part3.test.ts",
  "../apps/signup-worker/tests/stripe_handler_part4.test.ts",
  "../apps/signup-worker/tests/stripe_signature_part2.test.ts",
] as const;

const expectedPopulation = [...originalRoots, ...extractedFiles];

const importBoundaries = [
  { parent: "src/durable_object.ts", module: "./durable_object_start.js" },
  { parent: "src/durable_object.ts", module: "./lib/devenv_cleanup_route.js" },
  { parent: "src/index.ts", module: "./index_fetch.js" },
  { parent: "../apps/signup-worker/src/webhooks/clerk.ts", module: "./clerk_identity.js" },
  { parent: "../apps/signup-worker/src/webhooks/stripe.ts", module: "./stripe_persistence.js" },
] as const;

function source(relative: string): string {
  return readFileSync(resolve(ROOT, relative), "utf8");
}

function maskInactive(text: string): string {
  let out = "";
  let i = 0;
  while (i < text.length) {
    const c = text[i]!;
    const n = text[i + 1] ?? "";
    if (c === "/" && n === "/") {
      const end = text.indexOf("\n", i + 2);
      const next = end < 0 ? text.length : end;
      out += " ".repeat(next - i);
      i = next;
      continue;
    }
    if (c === "/" && n === "*") {
      const end = text.indexOf("*/", i + 2);
      const next = end < 0 ? text.length : end + 2;
      out += " ".repeat(next - i);
      i = next;
      continue;
    }
    if (c === "'" || c === "\"" || c === "`") {
      const quote = c;
      let j = i + 1;
      while (j < text.length) {
        if (text[j] === "\\") {
          j += 2;
          continue;
        }
        if (text[j] === quote) {
          j++;
          break;
        }
        j++;
      }
      out += " ".repeat(j - i);
      i = j;
      continue;
    }
    out += c;
    i++;
  }
  return out;
}

type TsToken =
  | { kind: "identifier"; value: string; line: number }
  | { kind: "string"; value: string; line: number }
  | { kind: "punct"; value: string; line: number };

/** Lex only enough TypeScript to distinguish real imports from lexical bait. */
function lexTypeScript(text: string): readonly TsToken[] {
  const tokens: TsToken[] = [];
  let i = 0;
  let line = 1;
  while (i < text.length) {
    const c = text[i]!;
    const n = text[i + 1] ?? "";
    if (/\s/.test(c)) {
      if (c === "\n") line++;
      i++;
      continue;
    }
    if (c === "/" && n === "/") {
      i += 2;
      while (i < text.length && text[i] !== "\n") i++;
      continue;
    }
    if (c === "/" && n === "*") {
      i += 2;
      while (i < text.length) {
        if (text[i] === "\n") line++;
        if (text[i] === "*" && text[i + 1] === "/") {
          i += 2;
          break;
        }
        i++;
      }
      continue;
    }
    if (c === "'" || c === '"') {
      const quote = c;
      const tokenLine = line;
      let value = "";
      i++;
      while (i < text.length) {
        const current = text[i]!;
        if (current === "\\") {
          value += current;
          i++;
          if (i < text.length) value += text[i++];
          continue;
        }
        if (current === quote) {
          i++;
          break;
        }
        if (current === "\n") line++;
        value += current;
        i++;
      }
      tokens.push({ kind: "string", value, line: tokenLine });
      continue;
    }
    if (c === "`") {
      // A template is an expression/string container, never a static import source.
      i++;
      while (i < text.length) {
        if (text[i] === "\\") {
          i += 2;
          continue;
        }
        if (text[i] === "`") {
          i++;
          break;
        }
        if (text[i] === "\n") line++;
        i++;
      }
      continue;
    }
    if (/[A-Za-z_$]/.test(c)) {
      const tokenLine = line;
      const start = i++;
      while (i < text.length && /[A-Za-z0-9_$]/.test(text[i]!)) i++;
      tokens.push({ kind: "identifier", value: text.slice(start, i), line: tokenLine });
      continue;
    }
    tokens.push({ kind: "punct", value: c, line });
    i++;
  }
  return tokens;
}

function activeImportSources(text: string): ReadonlySet<string> {
  const sources = new Set<string>();
  const tokens = lexTypeScript(text);
  for (let i = 0; i < tokens.length; i++) {
    const token = tokens[i]!;
    if (token.kind !== "identifier" || token.value !== "import") continue;
    const next = tokens[i + 1];
    // `import()` is an expression and must never satisfy a static wiring check.
    if (!next || (next.kind === "punct" && next.value === "(")) continue;
    // Side-effect import: import "./module";
    if (next.kind === "string") {
      sources.add(next.value);
      continue;
    }
    // Import clauses may contain comments/newlines. Comments have already been
    // removed, so only a real identifier followed immediately by a string can
    // form the `from "./module"` clause. A comment-only `from` is not a token.
    for (let j = i + 1; j + 1 < tokens.length; j++) {
      const current = tokens[j]!;
      if (current.kind === "punct" && current.value === ";") break;
      const following = tokens[j + 1]!;
      if (
        current.kind === "identifier" &&
        current.value === "from" &&
        following.kind === "string"
      ) {
        sources.add(following.value);
        break;
      }
    }
  }
  return sources;
}

function activeReexportSources(text: string, symbol: string): ReadonlySet<string> {
  const sources = new Set<string>();
  const tokens = lexTypeScript(text);
  for (let i = 0; i < tokens.length; i++) {
    if (tokens[i]?.kind !== "identifier" || tokens[i]?.value !== "export") continue;
    let found = false;
    for (let j = i + 1; j + 1 < tokens.length; j++) {
      const token = tokens[j]!;
      if (token.kind === "punct" && token.value === ";") break;
      if (token.kind === "identifier" && token.value === symbol) found = true;
      if (token.kind === "identifier" && token.value === "from" && found) {
        const source = tokens[j + 1];
        if (source?.kind === "string") sources.add(source.value);
        break;
      }
    }
  }
  return sources;
}

function activeCode(text: string): string {
  return maskInactive(text);
}

function hasExecutableTests(file: string, text = source(file)): boolean {
  const code = activeCode(text);
  if (/\b(describe|it|test)\s*\(/.test(code)) return true;
  const imports = [...activeImportSources(text)].filter((value) => value.startsWith("./") && value.endsWith(".js"));
  if (imports.length === 0) return false;
  return imports.every((value) => {
    const target = resolve(ROOT, file, "..");
    const targetFile = resolve(target, value.slice(2).replace(/\.js$/, ".ts"));
    try {
      return /\b(describe|it|test)\s*\(/.test(activeCode(readFileSync(targetFile, "utf8")));
    } catch {
      return false;
    }
  });
}

function validatePopulation(population: readonly string[]): void {
  const unique = new Set(population);
  if (unique.size !== expectedPopulation.length || expectedPopulation.some((file) => !unique.has(file))) {
    throw new Error("B-126 M3 closed population changed: original/extracted root omitted");
  }
  for (const file of expectedPopulation) {
    const lines = source(file).split("\n").length;
    if (lines > 1000) throw new Error(`${file} exceeds the 1000-line ownership bound`);
  }
}

function validateImportWiring(parent: string, module: string, text: string): void {
  if (!activeImportSources(text).has(module)) {
    throw new Error(`${parent}: active static import of ${module} is missing`);
  }
}

function validateAuthHelper(facadeText: string, policyText: string): void {
  if (!activeReexportSources(facadeText, "extractBasicAuthPassword").has("./index_auth_policy.js")) {
    throw new Error("index_auth.ts: active policy import is missing");
  }
  const policy = activeCode(policyText);
  if (!/\bexport\s+function\s+extractBasicAuthPassword\s*\(/.test(policy)) {
    throw new Error("index_auth_policy.ts: extractBasicAuthPassword implementation is missing");
  }
}

describe("B-126 M3 domain boundaries", () => {
  it("keeps the complete 43-path original/extracted population bounded", () => {
    validatePopulation(expectedPopulation);
    for (const file of extractedFiles.filter((path) => path.includes("tests/"))) {
      expect(hasExecutableTests(file), `${file} must remain executable`).toBe(true);
    }
  });

  it("fails closed when any original root is omitted", () => {
    expect(() => validatePopulation(expectedPopulation.slice(1))).toThrow();
  });

  it("keeps every parent import active and the Basic helper wired", () => {
    for (const boundary of importBoundaries) {
      validateImportWiring(boundary.parent, boundary.module, source(boundary.parent));
    }
    validateAuthHelper(source("src/index_auth.ts"), source("src/index_auth_policy.ts"));
  });

  it("rejects comment-wrapped import bait and a missing Basic helper", () => {
    for (const boundary of importBoundaries) {
      const parentText = source(boundary.parent);
      const bait = parentText.replaceAll(
        `from "${boundary.module}"`,
        `/* from "${boundary.module}" */`,
      );
      expect(() => validateImportWiring(boundary.parent, boundary.module, bait)).toThrow();
    }
    const missingReexport = source("src/index_auth.ts").replaceAll(
      'from "./index_auth_policy.js"',
      'from "./removed_policy.js"',
    );
    expect(() => validateAuthHelper(missingReexport, source("src/index_auth_policy.ts"))).toThrow();
    const missingHelper = source("src/index_auth_policy.ts").replace(
      "export function extractBasicAuthPassword",
      "function removedBasicAuthPassword",
    );
    expect(() => validateAuthHelper(source("src/index_auth.ts"), missingHelper)).toThrow();
  });

  it("rejects an unrelated import as executable test wiring", () => {
    const file = "tests/durable_object_part2.test.ts";
    const unrelated = `import "./unrelated.js";`;
    expect(hasExecutableTests(file, unrelated)).toBe(false);
  });

  it("rejects the exact line/block/nested comment, string, and dynamic-import baits", () => {
    const module = "./foo";
    const baits = [
      `import { x } /* from "${module}" */;`,
      `import { x } // from "${module}"\n;`,
      `import { x } /* outer /* from "${module}" */ */;`,
      `const bait = 'import { x } from "${module}"';`,
      `const bait = import("${module}");`,
    ];
    for (const bait of baits) {
      expect(() => validateImportWiring("synthetic.ts", module, bait)).toThrow();
    }
  });

  it("retains real static imports across comments and escaped strings", () => {
    const module = "./foo";
    const real = [
      `import { x } /* explanation */ from "${module}";`,
      `import /* explanation */ type { x } from '${module}';`,
      `import "${module}";`,
    ];
    for (const importText of real) {
      expect(() => validateImportWiring("synthetic.ts", module, importText)).not.toThrow();
    }
  });
});
