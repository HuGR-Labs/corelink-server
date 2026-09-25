import { describe, expect, it } from "vitest";
import {
  ownershipInsertStatement,
  verifyStagingOwnershipEnvelope,
  type StagingOwnershipContext,
} from "../src/staging_load_test_ownership.js";

const KEY = "01234567890123456789012345678901";
const RUN = "123456";
const SHA = "a".repeat(40);
const REQUEST = "b".repeat(32);
const ISSUED = 1_000_000;
const EXPIRES = ISSUED + 60_000;
const DOMAIN = "corelink/staging-ownership-envelope/v1\0";

function hex(value: ArrayBuffer): string {
  return Array.from(new Uint8Array(value), (byte) => byte.toString(16).padStart(2, "0")).join("");
}

async function envelope(overrides: Partial<{ scenario: string; request: string; expires: number }> = {}): Promise<string> {
  const scenario = overrides.scenario ?? "signup";
  const request = overrides.request ?? REQUEST;
  const expires = overrides.expires ?? EXPIRES;
  const payload = `v1.${RUN}.${scenario}.staging.${SHA}.${ISSUED}.${expires}.${request}`;
  const key = await crypto.subtle.importKey(
    "raw",
    new TextEncoder().encode(KEY),
    { name: "HMAC", hash: "SHA-256" },
    false,
    ["sign"],
  );
  const tag = await crypto.subtle.sign("HMAC", key, new TextEncoder().encode(DOMAIN + payload));
  return `${payload}.${hex(tag)}`;
}

function fakeDb(): { db: D1Database; statements: Array<{ sql: string; args: unknown[] }> } {
  const statements: Array<{ sql: string; args: unknown[] }> = [];
  const db = {
    prepare(sql: string) {
      const entry = { sql, args: [] as unknown[] };
      statements.push(entry);
      return {
        bind(...args: unknown[]) {
          entry.args = args;
          return this;
        },
      };
    },
  } as unknown as D1Database;
  return { db, statements };
}

describe("staging ownership envelope", () => {
  it("preserves ordinary traffic when the envelope is absent", async () => {
    await expect(verifyStagingOwnershipEnvelope(null, "signup", REQUEST, ISSUED, KEY)).resolves.toBeNull();
  });

  it("verifies a canonical request-bound envelope and builds its batch statement", async () => {
    const context = await verifyStagingOwnershipEnvelope(
      await envelope(),
      "signup",
      REQUEST,
      ISSUED + 1,
      KEY,
    );
    expect(context).toMatchObject({ runId: RUN, scenario: "signup", targetDeploymentSha: SHA, requestId: REQUEST });
    const { db, statements } = fakeDb();
    await ownershipInsertStatement(db, context!, "signup_artifact", "disposable", "opaque-1", ISSUED + 2);
    expect(statements).toHaveLength(1);
    expect(statements[0].sql).toContain("ON CONFLICT (run_id, scenario, resource_class, receipt_ref) DO NOTHING");
    expect(statements[0].args.slice(0, 3)).toEqual([RUN, "signup", "signup_artifact"]);
    expect(statements[0].args[3]).toMatch(/^[0-9a-f]{64}$/);
    expect(statements[0].args.slice(4)).toEqual(["opaque-1", "disposable", ISSUED + 2]);
  });

  it("fails closed for tamper, mismatch, expiry, weak keys, and forged objects", async () => {
    const valid = await envelope();
    const attempts = [
      verifyStagingOwnershipEnvelope(`${valid.slice(0, -1)}0`, "signup", REQUEST, ISSUED + 1, KEY),
      verifyStagingOwnershipEnvelope(valid, "webhook", REQUEST, ISSUED + 1, KEY),
      verifyStagingOwnershipEnvelope(valid, "signup", "c".repeat(32), ISSUED + 1, KEY),
      verifyStagingOwnershipEnvelope(valid, "signup", REQUEST, EXPIRES, KEY),
      verifyStagingOwnershipEnvelope(valid, "signup", REQUEST, ISSUED + 1, "short"),
    ];
    for (const attempt of attempts) {
      await expect(attempt).rejects.toThrow("staging ownership envelope is invalid");
    }
    const forged = {
      runId: RUN,
      scenario: "signup",
      targetDeploymentSha: SHA,
      issuedAtMs: ISSUED,
      expiresAtMs: EXPIRES,
      requestId: REQUEST,
    } as StagingOwnershipContext;
    const { db } = fakeDb();
    await expect(
      ownershipInsertStatement(db, forged, "signup_artifact", "disposable", "opaque-1", ISSUED + 2),
    ).rejects.toThrow("staging ownership envelope is invalid");
  });
});
