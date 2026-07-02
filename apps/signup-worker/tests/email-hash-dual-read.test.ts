import { describe, it, expect } from "vitest";
import {
  emailHashFor,
  emailHashLegacy,
  emailHashCandidates,
} from "../src/webhooks/clerk.js";
import { acceptTeamInvitation } from "../src/lib/d1.js";

const EMAIL = "alice@example.com";
const SALT = "server-secret-salt";

/** Reference legacy hash: hex(SHA-256(trim + lowercase)). */
async function legacyRef(email: string): Promise<string> {
  const d = await crypto.subtle.digest(
    "SHA-256",
    new TextEncoder().encode(email.trim().toLowerCase()),
  );
  return Array.from(new Uint8Array(d))
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}

describe("emailHashFor / emailHashLegacy (cross-lang parity with the container)", () => {
  it("unset salt is byte-identical to the legacy unsalted SHA-256", async () => {
    expect(await emailHashFor(EMAIL)).toBe(await legacyRef(EMAIL));
    expect(await emailHashFor(EMAIL, undefined)).toBe(await legacyRef(EMAIL));
    expect(await emailHashFor(EMAIL, "")).toBe(await legacyRef(EMAIL));
  });

  it("set salt differs from unsalted and is deterministic", async () => {
    const salted = await emailHashFor(EMAIL, SALT);
    expect(salted).not.toBe(await legacyRef(EMAIL));
    expect(salted).toBe(await emailHashFor(EMAIL, SALT)); // deterministic
    expect(salted).toHaveLength(64); // HMAC-SHA256 = 32 bytes hex
  });

  it("legacy helper stays unsalted regardless of a salt argument shape", async () => {
    expect(await emailHashLegacy(EMAIL)).toBe(await legacyRef(EMAIL));
  });

  it("normalizes (trim + lowercase) in both modes", async () => {
    expect(await emailHashFor("  ALICE@Example.COM  ")).toBe(
      await emailHashFor(EMAIL),
    );
    expect(await emailHashFor("  ALICE@Example.COM  ", SALT)).toBe(
      await emailHashFor(EMAIL, SALT),
    );
  });
});

describe("emailHashCandidates (dual-read candidate set)", () => {
  // INVARIANT (a): salt UNSET → one candidate == today.
  it("unset salt yields a single candidate equal to the legacy hash", async () => {
    const c = await emailHashCandidates(EMAIL);
    expect(c).toHaveLength(1);
    expect(c[0]).toBe(await legacyRef(EMAIL));
  });

  // INVARIANT (b)+(c): salt SET → {salted, legacy}, covering both stored schemes.
  it("set salt yields both the salted and the legacy candidate", async () => {
    const c = await emailHashCandidates(EMAIL, SALT);
    expect(c).toHaveLength(2);
    expect(c).toContain(await emailHashFor(EMAIL, SALT)); // (c) salted-stored row
    expect(c).toContain(await legacyRef(EMAIL)); // (b) legacy-stored row
  });

  // INVARIANT (d): distinct emails never share a candidate (no false-positive).
  it("does not collide across different emails", async () => {
    const a = await emailHashCandidates("alice@example.com", SALT);
    const b = await emailHashCandidates("bob@example.com", SALT);
    for (const x of a) expect(b).not.toContain(x);
  });
});

/**
 * Fake D1 that stores a single `team_member`-style row keyed by its stored
 * `email_hash` and honors the `WHERE email_hash IN (?1, ?2)` accept-lookup.
 */
function fakeDbWithInvite(storedEmailHash: string | null) {
  const flips: unknown[][] = [];
  const db = {
    prepare(query: string) {
      const stmt = {
        _query: query,
        _binds: [] as unknown[],
        bind(...values: unknown[]) {
          stmt._binds = values;
          if (query.startsWith("UPDATE team_member")) flips.push(values);
          return stmt;
        },
        async first<T>(): Promise<T | null> {
          if (
            storedEmailHash !== null &&
            query.includes("WHERE email_hash IN") &&
            stmt._binds.slice(0, 2).includes(storedEmailHash)
          ) {
            return { tenant_id: "t_1", user_id: "inv_1" } as unknown as T;
          }
          return null;
        },
        async run() {
          return { success: true };
        },
      };
      return stmt;
    },
  };
  return { db: db as unknown as import("@cloudflare/workers-types").D1Database, flips };
}

describe("acceptTeamInvitation dual-read", () => {
  // INVARIANT (a): salt UNSET, legacy-stored row → binds (== today).
  it("binds a legacy-stored invite when salt is unset", async () => {
    const stored = await legacyRef(EMAIL);
    const { db, flips } = fakeDbWithInvite(stored);
    const ok = await acceptTeamInvitation(
      db,
      "clerk_user_x",
      await emailHashCandidates(EMAIL), // salt unset → single candidate
    );
    expect(ok).toBe(true);
    expect(flips).toHaveLength(1);
  });

  // INVARIANT (b): salt SET, LEGACY-stored row → STILL binds (no false-negative).
  it("binds a legacy-stored invite even after the salt is set", async () => {
    const stored = await legacyRef(EMAIL); // written pre-salt
    const { db, flips } = fakeDbWithInvite(stored);
    const ok = await acceptTeamInvitation(
      db,
      "clerk_user_x",
      await emailHashCandidates(EMAIL, SALT), // salt set → {salted, legacy}
    );
    expect(ok).toBe(true);
    expect(flips).toHaveLength(1);
  });

  // INVARIANT (c): salt SET, SALTED-stored row → binds.
  it("binds a salted-stored invite when the salt is set", async () => {
    const stored = await emailHashFor(EMAIL, SALT); // written post-salt
    const { db } = fakeDbWithInvite(stored);
    const ok = await acceptTeamInvitation(
      db,
      "clerk_user_x",
      await emailHashCandidates(EMAIL, SALT),
    );
    expect(ok).toBe(true);
  });

  // INVARIANT (d): a genuinely-absent email → no match (no false-positive).
  it("returns false when no outstanding invite matches", async () => {
    const { db, flips } = fakeDbWithInvite(null);
    const ok = await acceptTeamInvitation(
      db,
      "clerk_user_x",
      await emailHashCandidates("nobody@example.com", SALT),
    );
    expect(ok).toBe(false);
    expect(flips).toHaveLength(0);
  });
});
