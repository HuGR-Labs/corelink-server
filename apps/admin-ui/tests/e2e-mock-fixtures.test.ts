import { beforeEach, describe, expect, it } from "vitest";
import { __resetMockState, getFixtureResponse } from "@/lib/e2e-mock-fixtures";

function invite(body: unknown) {
  return getFixtureResponse({
    method: "POST",
    path: "/v1/customer/team/invite",
    query: {},
    body,
    headers: {},
  });
}

function team() {
  return getFixtureResponse({
    method: "GET",
    path: "/v1/customer/team",
    query: {},
    body: undefined,
    headers: {},
  }).body as { members: Array<{ role: string }> };
}

describe("E2E customer-team invite fixture", () => {
  beforeEach(__resetMockState);

  it("persists and returns only the canonical assignable-role matrix", () => {
    for (const [requested, effective] of [
      ["admin", "admin"],
      [" MEMBER ", "member"],
      ["viewer", "viewer"],
    ]) {
      const response = invite({ email: `${effective}@example.com`, role: requested });
      expect(response.status).toBe(201);
      expect((response.body as { member: { role: string } }).member.role).toBe(effective);
    }

    expect(team().members.slice(-3).map((member) => member.role)).toEqual([
      "admin",
      "member",
      "viewer",
    ]);
  });

  it("rejects missing, owner, and unknown roles without mutating the mock", () => {
    const before = team().members;
    for (const [body, status] of [
      [{ email: "missing@example.com" }, 422],
      [{ email: "owner@example.com", role: "owner" }, 403],
      [{ email: "developer@example.com", role: "Developer" }, 400],
      [{ email: "operator@example.com", role: "operator" }, 400],
      [null, 422],
    ] as const) {
      expect(invite(body).status).toBe(status);
      expect(team().members).toEqual(before);
    }
  });
});
