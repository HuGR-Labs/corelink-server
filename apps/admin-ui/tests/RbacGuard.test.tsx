// WI-S16-005 — RbacGuard tests.

import React from "react";
import { render, screen } from "@testing-library/react";
import { describe, it, expect } from "vitest";
import RbacGuard from "@/components/admin/RbacGuard";

describe("RbacGuard", () => {
  it("renders 403 panel when role is not corelink-admin", async () => {
    const element = await RbacGuard({
      authProvider: () => ({
        user_id: "u_1",
        org_id: "org_1",
        role: "corelink-viewer",
        mfa_verified_at: null,
      }),
      children: <p>secret</p>,
    });
    render(element!);
    expect(screen.getByTestId("rbac-forbidden")).toBeInTheDocument();
    expect(screen.queryByText("secret")).not.toBeInTheDocument();
  });

  it("renders children when role is corelink-admin", async () => {
    const element = await RbacGuard({
      authProvider: () => ({
        user_id: "u_1",
        org_id: "org_1",
        role: "corelink-admin",
        mfa_verified_at: null,
      }),
      children: <p data-testid="kid">secret</p>,
    });
    render(element!);
    expect(screen.getByTestId("kid")).toHaveTextContent("secret");
  });

  it("renders 403 panel when no auth context (null role)", async () => {
    const element = await RbacGuard({
      authProvider: () => ({
        user_id: null,
        org_id: null,
        role: null,
        mfa_verified_at: null,
      }),
      children: <p>secret</p>,
    });
    render(element!);
    expect(screen.getByText(/no authenticated operator session/)).toBeInTheDocument();
  });
});
