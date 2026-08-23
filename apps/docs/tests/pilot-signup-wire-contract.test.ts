import fs from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";

// The pilot apply form is the only client of POST /v1/signup/pilot/{token},
// and nothing else in CI compiles the two sides against each other: the form
// is TypeScript, the handler is Rust, and the only shared artefact is the
// OpenAPI contract. That gap shipped a form that sent camelCase field names
// (`company`, `tierHint`, `useCase`) at a handler deserialising snake_case
// (`company_name`, `tier_hint`, `expected_use_case`), so axum's Json extractor
// rejected EVERY submission with 422 before the handler ran — while the page
// itself rendered fine and the route answered, which is why it read as healthy.
//
// These tests pin the form's wire payload to the published contract, in both
// directions, so the next rename fails here instead of in production.

const APPLY_TSX = path.resolve(
  __dirname,
  "..",
  "src",
  "pages",
  "pilot",
  "apply.tsx",
);
const OPENAPI_YAML = path.resolve(
  __dirname,
  "..",
  "..",
  "..",
  "openapi",
  "corelink-v1.yaml",
);

/** Field names the form actually puts on the wire, read from the fetch body. */
function wireFieldsFromForm(source: string): string[] {
  const bodyMatch = source.match(
    /body:\s*JSON\.stringify\(\{([\s\S]*?)\}\),/,
  );
  if (!bodyMatch) {
    throw new Error(
      "could not locate the JSON.stringify(...) submit body in apply.tsx",
    );
  }
  return [...bodyMatch[1].matchAll(/^\s*([A-Za-z_][A-Za-z0-9_]*)\s*:/gm)].map(
    (m) => m[1],
  );
}

/** `required:` list of the PilotSignupRequest schema in the OpenAPI contract. */
function requiredFieldsFromContract(yaml: string): string[] {
  const schemaIdx = yaml.indexOf("PilotSignupRequest:");
  if (schemaIdx === -1) {
    throw new Error("PilotSignupRequest schema not found in the contract");
  }
  const requiredMatch = yaml
    .slice(schemaIdx)
    .match(/required:\s*\[([^\]]*)\]/);
  if (!requiredMatch) {
    throw new Error("PilotSignupRequest has no inline `required: [...]` list");
  }
  return requiredMatch[1]
    .split(",")
    .map((f) => f.trim())
    .filter((f) => f.length > 0);
}

describe("pilot signup — form/API wire contract", () => {
  const source = fs.readFileSync(APPLY_TSX, "utf8");
  const contract = fs.readFileSync(OPENAPI_YAML, "utf8");

  it("sends exactly the field names the contract declares required", () => {
    expect(wireFieldsFromForm(source).sort()).toEqual(
      requiredFieldsFromContract(contract).sort(),
    );
  });

  it("posts at the live API host, not the retired dotted scheme", () => {
    const endpoint = source.match(
      /const SIGNUP_ENDPOINT\s*=\s*"([^"]+)"/,
    )?.[1];
    expect(endpoint).toBe("https://corelink-api.humangr.com/v1/signup/pilot");
  });

  it("maps 401 to the token message and 400/422 to the field message", () => {
    // The route answers 401 for a bad token and 400 for a bad field; axum
    // answers 422 for a body it cannot deserialise. Getting this backwards
    // sends an applicant to re-request a token that was never the problem.
    expect(source).toMatch(/status === 401/);
    expect(source).toMatch(/status === 400 \|\| status === 422/);
  });

  it("allows the endpoint's origin in the site's CSP connect-src", () => {
    // A cross-origin fetch this page makes must be listed in connect-src or
    // the browser refuses it before the request leaves — and the failure
    // surfaces in the form's catch branch as a "network error" naming a host
    // the request never reached, which reads as an outage rather than a
    // policy header. The API's own CORS is correct and cannot help here:
    // connect-src is enforced by the browser on the sending side.
    const endpoint = source.match(
      /const SIGNUP_ENDPOINT\s*=\s*"([^"]+)"/,
    )?.[1];
    const origin = new URL(endpoint as string).origin;
    const headers = fs.readFileSync(
      path.resolve(__dirname, "..", "static", "_headers"),
      "utf8",
    );
    const connectSrc = headers.match(
      /Content-Security-Policy:[^\n]*?connect-src ([^;]+);/,
    )?.[1];
    expect(connectSrc).toBeDefined();
    expect(connectSrc?.split(/\s+/)).toContain(origin);
  });
});
