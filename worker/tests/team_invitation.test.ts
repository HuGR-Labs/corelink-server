import { describe, expect, it, vi } from "vitest";
import {
  emailHashCandidates,
  redeemTeamInvitation,
  verifiedPrimaryEmail,
} from "../src/lib/team_invitation.js";

const TOKEN = "a".repeat(64);
const HASH = "b".repeat(64);
const NOW = 1_800_000_000_000;

function fakeDb(updateChanges = 1) {
  const sql: string[] = [];
  const binds: unknown[][] = [];
  const db = {
    prepare(query: string) {
      sql.push(query);
      const statement = {
        bind: vi.fn((...values: unknown[]) => {
          binds.push(values);
          return statement;
        }),
        first: vi.fn(async () =>
          query.startsWith("SELECT tenant_id")
            ? { tenant_id: "tenant-a", user_id: "inv-a", role: "member" }
            : null,
        ),
      };
      return statement;
    },
    batch: vi.fn(async () => [
      { success: true, meta: { changes: updateChanges } },
      { success: true, meta: { changes: updateChanges } },
    ]),
  };
  return { db: db as never, sql, binds, batch: db.batch };
}

describe("B-073 team invitation redemption", () => {
  it("rejects malformed capabilities before touching D1", async () => {
    const { db, batch } = fakeDb();
    await expect(
      redeemTeamInvitation(db, {
        clerkUserId: "user-1",
        invitationToken: "not-a-token",
        emailHashCandidates: [HASH],
        nowMs: NOW,
      }),
    ).resolves.toBeNull();
    expect(batch).not.toHaveBeenCalled();
  });

  it("binds the tenant from the token row and kills a replay", async () => {
    const first = fakeDb(1);
    await expect(
      redeemTeamInvitation(first.db, {
        clerkUserId: "user-1",
        invitationToken: TOKEN,
        emailHashCandidates: [HASH],
        nowMs: NOW,
      }),
    ).resolves.toEqual({ tenantId: "tenant-a", role: "member" });
    const selectSql = first.sql[0] ?? "";
    // The batch is [audit INSERT, conditional UPDATE]; inspect the UPDATE at
    // index 2 in the prepared/bind capture, not the audit statement at index 1.
    const updateSql = first.sql[2] ?? "";
    expect(selectSql).toContain("invitation_token_hash = ?1");
    expect(selectSql).toContain("email_hash IN (?2, ?3)");
    expect(selectSql).toContain("status = 'invited'");
    expect(selectSql).toContain("invited_at_ms > ?4");
    expect(selectSql).toContain("invited_at_ms <= ?5");
    expect(updateSql).toContain(
      "WHERE tenant_id = ? AND user_id = ? AND invitation_token_hash = ? AND status = 'invited'",
    );
    // Mutation-sensitive binding assertions: the lookup must use the SHA-256
    // digest of the opaque capability, and the conditional update must bind the
    // tenant selected from that token row. SQL-shape-only tests miss both.
    const expectedDigest = Array.from(
      new Uint8Array(await crypto.subtle.digest("SHA-256", new TextEncoder().encode(TOKEN))),
      (byte) => byte.toString(16).padStart(2, "0"),
    ).join("");
    expect(first.binds[0]?.[0]).toBe(expectedDigest);
    expect(first.binds[2]?.[2]).toBe("tenant-a");
    expect(first.binds[2]?.[4]).toBe(expectedDigest);
    expect(first.batch).toHaveBeenCalledTimes(1);

    const replay = fakeDb(0);
    await expect(
      redeemTeamInvitation(replay.db, {
        clerkUserId: "user-1",
        invitationToken: TOKEN,
        emailHashCandidates: [HASH],
        nowMs: NOW,
      }),
    ).resolves.toBeNull();
  });

  it("accepts only an explicitly verified Clerk primary address", async () => {
    const fetcher = vi.fn(async () =>
      Response.json({
        primary_email_address_id: "email-1",
        email_addresses: [{ id: "email-1", email_address: "alice@example.com", verification: { status: "verified" } }],
      }),
    );
    await expect(verifiedPrimaryEmail("user-1", "sk_test", fetcher)).resolves.toBe("alice@example.com");
    fetcher.mockResolvedValueOnce(
      Response.json({
        primary_email_address_id: "email-1",
        email_addresses: [{ id: "email-1", email_address: "alice@example.com", verification: { status: "unverified" } }],
      }),
    );
    await expect(verifiedPrimaryEmail("user-1", "sk_test", fetcher)).resolves.toBeNull();
  });

  it("preserves salted/legacy email-hash dual read", async () => {
    const candidates = await emailHashCandidates(" Alice@Example.com ", "salt");
    expect(candidates).toHaveLength(2);
    expect(candidates.every((candidate) => /^[0-9a-f]{64}$/.test(candidate))).toBe(true);
  });
});
