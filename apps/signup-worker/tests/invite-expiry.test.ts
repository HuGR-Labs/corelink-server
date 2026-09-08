import { describe, it, expect } from "vitest";
import {
  acceptTeamInvitation,
  TEAM_INVITATION_TTL_MS,
} from "../src/lib/d1.js";

/**
 * B-073 defense (3): a team invitation MUST expire.
 *
 * Before this suite, `acceptTeamInvitation` matched on `email_hash` +
 * `status='invited'` alone, so an invite row was a PERMANENT bearer credential:
 * the first `invited` row in the whole database matching the hash was handed to
 * whoever next signed up with that address — including the next holder of a
 * reassigned corporate mailbox, years later.
 *
 * The window is enforced IN THE SQL (`invited_at_ms > ?4 AND invited_at_ms <= ?5`), not in JS after the
 * fact: with `LIMIT 1`, a JS-side filter would let the database pick a stale row
 * and then report "no invitation" while a valid row sat further down the table.
 * The fake D1 below therefore models the predicate the way SQLite would — it
 * only applies the cutoff if the query actually asks for it, which is what makes
 * this suite RED against the unfixed helper.
 *
 * NOTE: this covers expiry ONLY. The invite remains an unauthenticated bearer
 * WITHIN the window (no token, no tenant scope, no Clerk email-verification
 * check) — see B-073 for the three defenses still open.
 */

const HASH = "a".repeat(64);
const TOKEN = "b".repeat(64);
const NOW = 1_800_000_000_000; // fixed clock; the helper takes `nowMs` in.

/**
 * Fake D1 holding ONE `team_member` invite row, honoring the accept-lookup
 * `WHERE invitation_token_hash = ?1 AND email_hash IN (?2, ?3) AND status = 'invited'
 * AND invited_at_ms > ?4 AND invited_at_ms <= ?5`.
 * Records every UPDATE so a refusal can be asserted as "row NOT flipped".
 */
function fakeDbWithInvite(invitedAtMs: number, updateChanges = 1) {
  const flips: unknown[][] = [];
  const db = {
    prepare(query: string) {
      const stmt = {
        _binds: [] as unknown[],
        bind(...values: unknown[]) {
          stmt._binds = values;
          if (query.includes("WHERE invitation_token_hash = ?1")) {
            // Model the real SQLite prepared statement: five placeholders and
            // five positional values. A stale ?3/?4 query or a missing upper
            // bound must fail this fixture instead of being silently ignored.
            expect(values).toHaveLength(5);
            expect(query).toContain("invited_at_ms > ?4");
            expect(query).toContain("invited_at_ms <= ?5");
          }
          if (query.startsWith("UPDATE team_member")) flips.push(values);
          return stmt;
        },
        async first<T>(): Promise<T | null> {
          // The real SELECT starts with the token-digest predicate and then
          // contains the candidate-email predicate; do not require the
          // substring to begin at `WHERE` (the token clause comes first).
          if (
            !query.includes("invitation_token_hash = ?1") ||
            !query.includes("email_hash IN")
          ) return null;
          if (!stmt._binds.slice(1, 3).includes(HASH)) return null;
          // The expiry predicate is applied by the DATABASE, exactly as the SQL
          // declares it — absent from the query means absent from the fake. Keep
          // the operator data-driven so a mutation from `>` to `>=` is observable
          // at the exact-TTL boundary instead of being masked by the fixture.
          const expiry = query.match(/invited_at_ms\s*(>=|>)\s*\?4/);
          if (expiry) {
            const cutoff = stmt._binds[3] as number;
            if (typeof cutoff !== "number") {
              throw new Error(
                "expiry predicate present but cutoff bind ?4 is not a number",
              );
            }
            const isFresh =
              expiry[1] === ">"
                ? invitedAtMs > cutoff
                : invitedAtMs >= cutoff;
            if (!isFresh) return null;
          }
          if (query.includes("invited_at_ms <= ?5")) {
            const upperBound = stmt._binds[4] as number;
            if (typeof upperBound !== "number") {
              throw new Error(
                "future-date predicate present but upper-bound bind ?5 is not a number",
              );
            }
            if (invitedAtMs > upperBound) return null;
          }
          return { tenant_id: "t_1", user_id: "inv_1" } as unknown as T;
        },
        async run() {
          return { success: true, meta: { changes: updateChanges } };
        },
      };
      return stmt;
    },
  };
  return {
    db: db as unknown as import("../src/lib/d1.js").D1Database,
    flips,
  };
}

describe("acceptTeamInvitation — invitation expiry (B-073 defense 3)", () => {
  it("exposes a bounded, non-zero TTL constant", () => {
    expect(TEAM_INVITATION_TTL_MS).toBeGreaterThan(0);
    // Sanity band: at least a day, at most 30 days. A TTL outside this range is
    // either useless (too short to accept) or back to "effectively permanent".
    expect(TEAM_INVITATION_TTL_MS).toBeGreaterThanOrEqual(24 * 60 * 60 * 1000);
    expect(TEAM_INVITATION_TTL_MS).toBeLessThanOrEqual(30 * 24 * 60 * 60 * 1000);
  });

  // POSITIVE CONTROL — without this, a helper that refused EVERYTHING would pass
  // a suite made only of denials. This is what makes the denials meaningful.
  it("still accepts an invitation inside the window", async () => {
    const { db, flips } = fakeDbWithInvite(NOW - TEAM_INVITATION_TTL_MS / 2);
    const ok = await acceptTeamInvitation(db, "clerk_user_x", [HASH], NOW, TOKEN);
    expect(ok).toBe(true);
    expect(flips).toHaveLength(1);
    expect(flips[0]?.[0]).toBe(NOW);
  });

  it("accepts an invitation issued this instant", async () => {
    const { db, flips } = fakeDbWithInvite(NOW);
    const ok = await acceptTeamInvitation(db, "clerk_user_x", [HASH], NOW, TOKEN);
    expect(ok).toBe(true);
    expect(flips).toHaveLength(1);
    expect(flips[0]?.[0]).toBe(NOW);
  });

  it("refuses a future-dated invitation", async () => {
    const future = fakeDbWithInvite(NOW + 1);
    expect(await acceptTeamInvitation(future.db, "u", [HASH], NOW, TOKEN)).toBe(false);
    expect(future.flips).toHaveLength(0);
  });

  it("refuses an invitation older than the TTL and does NOT flip the row", async () => {
    const { db, flips } = fakeDbWithInvite(
      NOW - TEAM_INVITATION_TTL_MS - 60_000,
    );
    const ok = await acceptTeamInvitation(db, "clerk_user_x", [HASH], NOW, TOKEN);
    expect(ok).toBe(false);
    expect(flips).toHaveLength(0);
  });

  it("refuses a years-old invite for a recycled corporate address", async () => {
    const { db, flips } = fakeDbWithInvite(NOW - 3 * 365 * 24 * 60 * 60 * 1000);
    const ok = await acceptTeamInvitation(db, "clerk_user_new", [HASH], NOW, TOKEN);
    expect(ok).toBe(false);
    expect(flips).toHaveLength(0);
  });

  // BOUNDARY: the lower predicate is `invited_at_ms > nowMs - TTL`, so an age
  // of EXACTLY the TTL falls on the REFUSED side (the window is half-open);
  // one millisecond younger is accepted. Future timestamps are also refused.
  it("refuses at exactly the TTL edge and accepts one ms inside it", async () => {
    const edge = fakeDbWithInvite(NOW - TEAM_INVITATION_TTL_MS);
    expect(await acceptTeamInvitation(edge.db, "u", [HASH], NOW, TOKEN)).toBe(false);
    expect(edge.flips).toHaveLength(0);

    const inside = fakeDbWithInvite(NOW - TEAM_INVITATION_TTL_MS + 1);
    expect(await acceptTeamInvitation(inside.db, "u", [HASH], NOW, TOKEN)).toBe(true);
    expect(inside.flips).toHaveLength(1);
    expect(inside.flips[0]?.[0]).toBe(NOW);
  });

  it("returns false when a concurrent acceptance makes the UPDATE change zero rows", async () => {
    const raced = fakeDbWithInvite(NOW - TEAM_INVITATION_TTL_MS / 2, 0);
    expect(await acceptTeamInvitation(raced.db, "u", [HASH], NOW, TOKEN)).toBe(false);
    expect(raced.flips).toHaveLength(1);
    expect(raced.flips[0]?.[0]).toBe(NOW);
  });

  it("defaults the clock to now when no nowMs is supplied", async () => {
    const fresh = fakeDbWithInvite(Date.now());
    expect(await acceptTeamInvitation(fresh.db, "u", [HASH], Date.now(), TOKEN)).toBe(true);

    const stale = fakeDbWithInvite(Date.now() - TEAM_INVITATION_TTL_MS - 1);
    expect(await acceptTeamInvitation(stale.db, "u", [HASH], Date.now(), TOKEN)).toBe(false);
  });
});
