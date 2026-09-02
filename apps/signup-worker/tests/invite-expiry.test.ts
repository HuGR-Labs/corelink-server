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
 * The window is enforced IN THE SQL (`invited_at_ms > ?3`), not in JS after the
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
const NOW = 1_800_000_000_000; // fixed clock; the helper takes `nowMs` in.

/**
 * Fake D1 holding ONE `team_member` invite row, honoring the accept-lookup
 * `WHERE email_hash IN (?1, ?2) AND status = 'invited' [AND invited_at_ms > ?3]`.
 * Records every UPDATE so a refusal can be asserted as "row NOT flipped".
 */
function fakeDbWithInvite(invitedAtMs: number) {
  const flips: unknown[][] = [];
  const db = {
    prepare(query: string) {
      const stmt = {
        _binds: [] as unknown[],
        bind(...values: unknown[]) {
          stmt._binds = values;
          if (query.startsWith("UPDATE team_member")) flips.push(values);
          return stmt;
        },
        async first<T>(): Promise<T | null> {
          if (!query.includes("WHERE email_hash IN")) return null;
          if (!stmt._binds.slice(0, 2).includes(HASH)) return null;
          // The expiry predicate is applied by the DATABASE, exactly as the SQL
          // declares it — absent from the query means absent from the fake. Keep
          // the operator data-driven so a mutation from `>` to `>=` is observable
          // at the exact-TTL boundary instead of being masked by the fixture.
          const expiry = query.match(/invited_at_ms\s*(>=|>)\s*\?3/);
          if (expiry) {
            const cutoff = stmt._binds[2] as number;
            if (typeof cutoff !== "number") {
              throw new Error(
                "expiry predicate present but cutoff bind ?3 is not a number",
              );
            }
            const isFresh =
              expiry[1] === ">"
                ? invitedAtMs > cutoff
                : invitedAtMs >= cutoff;
            if (!isFresh) return null;
          }
          return { tenant_id: "t_1", user_id: "inv_1" } as unknown as T;
        },
        async run() {
          return { success: true };
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
    const ok = await acceptTeamInvitation(db, "clerk_user_x", [HASH], NOW);
    expect(ok).toBe(true);
    expect(flips).toHaveLength(1);
  });

  it("accepts an invitation issued this instant", async () => {
    const { db, flips } = fakeDbWithInvite(NOW);
    const ok = await acceptTeamInvitation(db, "clerk_user_x", [HASH], NOW);
    expect(ok).toBe(true);
    expect(flips).toHaveLength(1);
  });

  it("refuses an invitation older than the TTL and does NOT flip the row", async () => {
    const { db, flips } = fakeDbWithInvite(
      NOW - TEAM_INVITATION_TTL_MS - 60_000,
    );
    const ok = await acceptTeamInvitation(db, "clerk_user_x", [HASH], NOW);
    expect(ok).toBe(false);
    expect(flips).toHaveLength(0);
  });

  it("refuses a years-old invite for a recycled corporate address", async () => {
    const { db, flips } = fakeDbWithInvite(NOW - 3 * 365 * 24 * 60 * 60 * 1000);
    const ok = await acceptTeamInvitation(db, "clerk_user_new", [HASH], NOW);
    expect(ok).toBe(false);
    expect(flips).toHaveLength(0);
  });

  // BOUNDARY: the predicate is `invited_at_ms > nowMs - TTL`, so an age of
  // EXACTLY the TTL falls on the REFUSED side (the window is half-open); one
  // millisecond younger is accepted.
  it("refuses at exactly the TTL edge and accepts one ms inside it", async () => {
    const edge = fakeDbWithInvite(NOW - TEAM_INVITATION_TTL_MS);
    expect(await acceptTeamInvitation(edge.db, "u", [HASH], NOW)).toBe(false);
    expect(edge.flips).toHaveLength(0);

    const inside = fakeDbWithInvite(NOW - TEAM_INVITATION_TTL_MS + 1);
    expect(await acceptTeamInvitation(inside.db, "u", [HASH], NOW)).toBe(true);
    expect(inside.flips).toHaveLength(1);
  });

  it("defaults the clock to now when no nowMs is supplied", async () => {
    const fresh = fakeDbWithInvite(Date.now());
    expect(await acceptTeamInvitation(fresh.db, "u", [HASH])).toBe(true);

    const stale = fakeDbWithInvite(Date.now() - TEAM_INVITATION_TTL_MS - 1);
    expect(await acceptTeamInvitation(stale.db, "u", [HASH])).toBe(false);
  });
});
