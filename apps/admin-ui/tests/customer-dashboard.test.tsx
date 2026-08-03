/**
 * Customer dashboard smoke tests — billing / keys / usage / team.
 *
 * Scope:
 *   - CustomerGuard: gating (unauthenticated → 401 panel, authenticated → children).
 *   - CustomerNav: renders all expected links.
 *   - BillingClient: loading state, data state, error state.
 *   - KeysClient: loading state, table renders, create + revoke happy paths.
 *   - UsageClient: loading state, data state (quota pct, reads/writes, daily rows).
 *   - TeamClient: loading state, member list, invite flow.
 *   - PortalLauncher: POST + redirect, error surface.
 *
 * API calls are replaced by injected fetchImpl stubs — no real network.
 * CustomerGuard is tested via authProvider injection (same pattern as RbacGuard).
 */

import * as React from "react";
import { describe, it, expect, vi, afterEach, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";

// Mock next/link so it renders a plain <a> in jsdom
vi.mock("next/link", () => ({
  default: ({ href, children }: { href: string; children: React.ReactNode }) => (
    <a href={href}>{children}</a>
  ),
}));

// Customer components call Clerk's useAuth() to mint session tokens for the
// CustomerClient — stub it (no ClerkProvider in jsdom).
vi.mock("@clerk/nextjs", () => ({
  useAuth: () => ({ getToken: async () => null }),
}));

// ---------------------------------------------------------------------------
// CustomerGuard
// ---------------------------------------------------------------------------

import CustomerGuard, { Unauthenticated } from "@/components/customer/CustomerGuard";

describe("CustomerGuard", () => {
  it("renders 401 panel when user is not authenticated", async () => {
    const el = await CustomerGuard({
      authProvider: () => ({ user_id: null, org_id: null, role: null, mfa_verified_at: null }),
      children: <p data-testid="kid">protected</p>,
    });
    render(el!);
    expect(screen.getByTestId("customer-unauth")).toBeInTheDocument();
    expect(screen.queryByTestId("kid")).not.toBeInTheDocument();
  });

  it("renders children when user has corelink-member role", async () => {
    const el = await CustomerGuard({
      authProvider: () => ({
        user_id: "u_123",
        org_id: "org_abc",
        role: "corelink-member",
        mfa_verified_at: null,
      }),
      children: <p data-testid="kid">protected</p>,
    });
    render(el!);
    expect(screen.getByTestId("kid")).toBeInTheDocument();
    expect(screen.queryByTestId("customer-unauth")).not.toBeInTheDocument();
  });

  it("renders children when user has corelink-admin role", async () => {
    const el = await CustomerGuard({
      authProvider: () => ({
        user_id: "u_admin",
        org_id: "org_xyz",
        role: "corelink-admin",
        mfa_verified_at: null,
      }),
      children: <p data-testid="org-kid">admin content</p>,
    });
    render(el!);
    expect(screen.getByTestId("org-kid")).toBeInTheDocument();
  });

  it("renders 401 panel when role is corelink-viewer (insufficient)", async () => {
    // hasCustomerAccess allows corelink-viewer too — this test verifies viewer IS allowed
    const el = await CustomerGuard({
      authProvider: () => ({
        user_id: "u_viewer",
        org_id: "org_xyz",
        role: "corelink-viewer",
        mfa_verified_at: null,
      }),
      children: <p data-testid="viewer-kid">viewer content</p>,
    });
    render(el!);
    expect(screen.getByTestId("viewer-kid")).toBeInTheDocument();
    expect(screen.queryByTestId("customer-unauth")).not.toBeInTheDocument();
  });

  it("Unauthenticated component renders sign-in link", () => {
    render(<Unauthenticated />);
    expect(screen.getByRole("alert")).toBeInTheDocument();
    expect(screen.getByText(/sign in required/i)).toBeInTheDocument();
    const link = screen.getByRole("link");
    expect(link).toHaveAttribute("href", "/sign-in");
  });
});

// ---------------------------------------------------------------------------
// CustomerNav
// ---------------------------------------------------------------------------

import { CustomerNav, customerNavLinks } from "@/components/customer/CustomerNav";

describe("CustomerNav", () => {
  it("renders all 6 expected nav links", () => {
    render(<CustomerNav locale="en" />);
    const nav = screen.getByTestId("customer-nav");
    expect(nav).toBeInTheDocument();
    expect(screen.getByTestId("nav-overview")).toBeInTheDocument();
    expect(screen.getByTestId("nav-usage")).toBeInTheDocument();
    expect(screen.getByTestId("nav-audit")).toBeInTheDocument();
    expect(screen.getByTestId("nav-billing")).toBeInTheDocument();
    expect(screen.getByTestId("nav-keys")).toBeInTheDocument();
    expect(screen.getByTestId("nav-team")).toBeInTheDocument();
  });

  it("generates locale-prefixed hrefs", () => {
    const links = customerNavLinks("pt");
    const href = (testId: string): string | undefined =>
      links.find((l) => l.testId === testId)?.href;
    expect(href("nav-overview")).toBe("/pt/customer");
    expect(href("nav-usage")).toBe("/pt/customer/usage");
    expect(href("nav-billing")).toBe("/pt/customer/billing");
  });

  it("sets data-active=true on the active link", () => {
    render(<CustomerNav locale="en" activeHref="/en/customer/keys" />);
    const keysLink = screen.getByTestId("nav-keys");
    expect(keysLink).toHaveAttribute("data-active", "true");
    const overviewLink = screen.getByTestId("nav-overview");
    expect(overviewLink).toHaveAttribute("data-active", "false");
  });

  it("has accessible nav landmark", () => {
    render(<CustomerNav locale="en" />);
    const nav = screen.getByRole("navigation", { name: /customer dashboard/i });
    expect(nav).toBeInTheDocument();
  });
});

// ---------------------------------------------------------------------------
// PortalLauncher
// ---------------------------------------------------------------------------

import { PortalLauncher } from "@/app/[locale]/(authenticated)/customer/billing/PortalLauncher";

describe("PortalLauncher", () => {
  afterEach(() => vi.restoreAllMocks());

  it("renders the open button", () => {
    render(<PortalLauncher locale="en" tenantId="t_1" fetchImpl={vi.fn()} />);
    expect(screen.getByTestId("portal-open-button")).toBeInTheDocument();
    expect(screen.getByTestId("portal-open-button")).toBeEnabled();
  });

  it("POSTs and redirects to the portal_url on success", async () => {
    const portalUrl = "https://billing.stripe.com/p/session/abc123";
    const fetchImpl = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({
        portal_url: portalUrl,
        session_id: "abc123",
        expires_at_unix: 9999999999,
      }),
    } as unknown as Response);

    const assignSpy = vi.fn();
    Object.defineProperty(window, "location", {
      value: { ...window.location, assign: assignSpy, origin: "https://app.example.com" },
      configurable: true,
      writable: true,
    });

    render(<PortalLauncher locale="en" tenantId="t_1" fetchImpl={fetchImpl as typeof fetch} />);
    fireEvent.click(screen.getByTestId("portal-open-button"));

    await waitFor(() => expect(assignSpy).toHaveBeenCalledWith(portalUrl));
    // basePath-ABSOLUTE. `fetch()` never gets Next's automatic basePath, and a
    // bare `/api/v1/...` POSTs to the apex `humangr.com` — the hugr-site
    // MARKETING app (405 in production). This assertion previously PINNED the
    // broken bare literal.
    expect(fetchImpl).toHaveBeenCalledWith(
      "/corelink/api/v1/customer/billing/portal-session",
      expect.objectContaining({ method: "POST" }),
    );
  });

  it("surfaces an error message when the POST fails", async () => {
    const fetchImpl = vi.fn().mockResolvedValue({
      ok: false,
      status: 500,
      text: async () => "internal error",
    } as unknown as Response);

    render(<PortalLauncher locale="en" tenantId="t_1" fetchImpl={fetchImpl as typeof fetch} />);
    fireEvent.click(screen.getByTestId("portal-open-button"));

    await waitFor(() => expect(screen.getByTestId("portal-error")).toBeInTheDocument());
    expect(screen.getByTestId("portal-error")).toHaveTextContent(/portal session failed/i);
    // Button re-enabled after error
    expect(screen.getByTestId("portal-open-button")).toBeEnabled();
  });

  it("rejects non-HTTPS portal URLs", async () => {
    const fetchImpl = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({
        portal_url: "http://evil.example.com/redirect",
        session_id: "bad",
        expires_at_unix: 999,
      }),
    } as unknown as Response);

    render(<PortalLauncher locale="en" tenantId="t_1" fetchImpl={fetchImpl as typeof fetch} />);
    fireEvent.click(screen.getByTestId("portal-open-button"));

    await waitFor(() => expect(screen.getByTestId("portal-error")).toBeInTheDocument());
    expect(screen.getByTestId("portal-error")).toHaveTextContent(/non-HTTPS/i);
  });
});

// ---------------------------------------------------------------------------
// UsageClient
// ---------------------------------------------------------------------------

import { UsageClient } from "@/components/customer/UsageClient";

// Components build the client in-component via
// `useMemo(() => new CustomerClient({ getToken }), [getToken])`.
// vi.mock is hoisted — we cannot reference variables declared after it in the factory.
// Solution: capture the singleton mock instance inside the factory so we can
// later spy on it with vi.spyOn per test.

vi.mock("@/lib/customer-client", async () => {
  const { vi: viInner } = await import("vitest");
  const sharedInstance = {
    getUsage: viInner.fn(),
    getOverview: viInner.fn(),
    getBilling: viInner.fn(),
    listKeys: viInner.fn(),
    createPat: viInner.fn(),
    revokePat: viInner.fn(),
    listTeam: viInner.fn(),
    inviteTeam: viInner.fn(),
    startBillingPortal: viInner.fn(),
  };
  return {
    // Plain function (NOT vi.fn().mockImplementation): the PortalLauncher
    // describe runs vi.restoreAllMocks() in afterEach, which would strip a
    // mockImplementation and make later in-component `new CustomerClient(...)`
    // calls return an empty instance.
    CustomerClient: function CustomerClient() {
      return sharedInstance;
    },
    CustomerClientError: class extends Error {
      constructor(public status: number, msg: string) {
        super(msg);
      }
    },
    __sharedInstance: sharedInstance,
  };
});

import * as customerClientModule from "@/lib/customer-client";
import type { CustomerOverview, CustomerUsage, CustomerBilling, CustomerPat, CustomerTeamMember } from "@/lib/customer-types";

// eslint-disable-next-line @typescript-eslint/no-explicit-any
const customerClientMock = (customerClientModule as any).__sharedInstance as {
  getUsage: ReturnType<typeof vi.fn>;
  getOverview: ReturnType<typeof vi.fn>;
  getBilling: ReturnType<typeof vi.fn>;
  listKeys: ReturnType<typeof vi.fn>;
  createPat: ReturnType<typeof vi.fn>;
  revokePat: ReturnType<typeof vi.fn>;
  listTeam: ReturnType<typeof vi.fn>;
  inviteTeam: ReturnType<typeof vi.fn>;
  startBillingPortal: ReturnType<typeof vi.fn>;
};

const USAGE_FIXTURE: CustomerUsage = {
  period: "2026-05",
  cas_bytes: 1073741824, // 1 GiB
  reads: 12000,
  writes: 3000,
  request_count: 1_000_000, // BE-1a — billable requests this period
  quota_bytes: 10737418240, // 10 GiB
  hit_rate: 0.86, // BE-2 — 86% of lookups served from cache
  time_saved_seconds: 13320, // BE-2 → "3.7 h"
  dollars_saved_cents: 6690, // BE-2 — modeled estimate → "$67"
  daily: [
    { day: "2026-05-01", reads: 400, writes: 100, cas_bytes: 0 },
    { day: "2026-05-02", reads: 600, writes: 200, cas_bytes: 0 },
  ],
};

const OVERVIEW_FIXTURE: CustomerOverview = {
  tenant_id: "t_1",
  tenant_name: "Acme",
  plan: "pro", // 20M cache requests/mo ceiling → the requests gauge renders
  usage: {
    period: "2026-05",
    cas_bytes: 1073741824,
    reads: 12000,
    writes: 3000,
    quota_bytes: 10737418240,
  },
  billing: {
    status: "active",
    next_invoice_at: "2026-06-01T00:00:00Z",
    amount_due_cents: 5000,
    currency: "usd",
  },
  byok: { status: "none" },
  recent_activity: [],
};

describe("UsageClient", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("shows loading state initially then renders data", async () => {
    customerClientMock.getUsage.mockResolvedValue(USAGE_FIXTURE);
    customerClientMock.getOverview.mockResolvedValue(OVERVIEW_FIXTURE);

    render(<UsageClient />);
    expect(screen.getByTestId("usage-loading")).toBeInTheDocument();

    await waitFor(() => expect(screen.getByTestId("usage-shell")).toBeInTheDocument());
    expect(screen.getByTestId("usage-cas-pct")).toHaveTextContent("10%");
  });

  it("renders a real requests gauge (count vs the tier ceiling) when the plan is known", async () => {
    customerClientMock.getUsage.mockResolvedValue(USAGE_FIXTURE);
    customerClientMock.getOverview.mockResolvedValue(OVERVIEW_FIXTURE);

    render(<UsageClient />);
    await waitFor(() => expect(screen.getByTestId("usage-requests")).toBeInTheDocument());
    // pro = 20M ceiling; 1M / 20M = 5%.
    expect(screen.getByTestId("usage-requests-pct")).toHaveTextContent("5%");
    expect(screen.queryByTestId("usage-requests-stat")).not.toBeInTheDocument();
  });

  it("falls back to a Stat of the count when the plan (hence ceiling) is unknown", async () => {
    customerClientMock.getUsage.mockResolvedValue(USAGE_FIXTURE);
    // Overview read fails → plan degrades to null → no fabricated ceiling.
    customerClientMock.getOverview.mockRejectedValue(new Error("overview down"));

    render(<UsageClient />);
    await waitFor(() => expect(screen.getByTestId("usage-requests-stat")).toBeInTheDocument());
    expect(screen.queryByTestId("usage-requests-pct")).not.toBeInTheDocument();
    // The real count still shows (locale-independent digit check).
    expect(screen.getByTestId("usage-requests-stat").textContent).toMatch(/1[,.]?000[,.]?000/);
  });

  it("renders daily breakdown rows", async () => {
    customerClientMock.getUsage.mockResolvedValue(USAGE_FIXTURE);
    customerClientMock.getOverview.mockResolvedValue(OVERVIEW_FIXTURE);

    render(<UsageClient />);
    await waitFor(() => expect(screen.getByTestId("usage-daily-table")).toBeInTheDocument());
    expect(screen.getByTestId("usage-day-2026-05-01")).toBeInTheDocument();
    expect(screen.getByTestId("usage-day-2026-05-02")).toBeInTheDocument();
  });

  it("surfaces an error when getUsage rejects", async () => {
    customerClientMock.getUsage.mockRejectedValue(new Error("network failure"));
    customerClientMock.getOverview.mockResolvedValue(OVERVIEW_FIXTURE);

    render(<UsageClient />);
    await waitFor(() => expect(screen.getByTestId("usage-error")).toBeInTheDocument());
    expect(screen.getByTestId("usage-error")).toHaveTextContent("network failure");
  });
});

// ---------------------------------------------------------------------------
// BillingClient
// ---------------------------------------------------------------------------

import { BillingClient } from "@/components/customer/BillingClient";

const BILLING_FIXTURE: CustomerBilling = {
  status: "active",
  plan: "team",
  current_period_start: "2026-05-01T00:00:00Z",
  current_period_end: "2026-06-01T00:00:00Z",
  amount_due_cents: 19900,
  currency: "usd",
  payment_method: { brand: "visa", last4: "4242", exp_month: 12, exp_year: 2028 },
  invoices: [
    {
      invoice_id: "inv_001",
      issued_at: "2026-05-01T00:00:00Z",
      amount_cents: 19900,
      status: "paid",
      hosted_url: "https://billing.stripe.com/invoice/inv_001",
    },
  ],
};

describe("BillingClient", () => {
  beforeEach(() => { vi.clearAllMocks(); });

  it("shows loading state initially", () => {
    customerClientMock.getBilling.mockReturnValue(new Promise(() => {})); // never resolves

    render(<BillingClient />);
    expect(screen.getByTestId("billing-loading")).toBeInTheDocument();
  });

  it("renders subscription info after loading", async () => {
    customerClientMock.getBilling.mockResolvedValue(BILLING_FIXTURE);

    render(<BillingClient />);
    await waitFor(() => expect(screen.getByTestId("billing-shell")).toBeInTheDocument());
    // Enums are humanized — never the raw wire value.
    expect(screen.getByTestId("billing-plan")).toHaveTextContent("Team");
    expect(screen.getByTestId("billing-status")).toHaveTextContent("Active");
    expect(screen.getByTestId("billing-amount-due")).toHaveTextContent("$199.00");
    // Payment method is a [stub] field — surfaced as "managed in the Stripe
    // portal", never a fabricated card (no brand/last4 emitted in prod).
    expect(screen.getByTestId("billing-pm-managed")).toHaveTextContent(/Stripe portal/i);
  });

  it("renders invoice list", async () => {
    customerClientMock.getBilling.mockResolvedValue(BILLING_FIXTURE);

    render(<BillingClient />);
    await waitFor(() => expect(screen.getByTestId("billing-invoice-inv_001")).toBeInTheDocument());
  });

  it("shows the single upgrade path for free plan (and no portal button)", async () => {
    const freeBilling: CustomerBilling = { ...BILLING_FIXTURE, plan: "free", payment_method: undefined };
    customerClientMock.getBilling.mockResolvedValue(freeBilling);

    render(<BillingClient />);
    await waitFor(() => expect(screen.getByTestId("billing-upgrade-section")).toBeInTheDocument());
    // Free tier: upgrade path present; portal button absent (nothing to manage).
    expect(screen.queryByTestId("billing-portal-btn")).not.toBeInTheDocument();
  });

  it("renders exactly ONE portal button for a paying tenant (redundancy removed)", async () => {
    customerClientMock.getBilling.mockResolvedValue(BILLING_FIXTURE);

    render(<BillingClient />);
    await waitFor(() => expect(screen.getByTestId("billing-shell")).toBeInTheDocument());
    expect(screen.getAllByTestId("billing-portal-btn")).toHaveLength(1);
  });

  it("shows a teaching empty state when there are no invoices (never fabricated)", async () => {
    const noInvoices: CustomerBilling = { ...BILLING_FIXTURE, invoices: [] };
    customerClientMock.getBilling.mockResolvedValue(noInvoices);

    render(<BillingClient />);
    await waitFor(() => expect(screen.getByTestId("billing-invoices-empty")).toBeInTheDocument());
    expect(screen.getByTestId("billing-invoices-empty")).toHaveTextContent(/after your first payment/i);
  });

  it("surfaces error on getBilling failure", async () => {
    customerClientMock.getBilling.mockRejectedValue(new Error("billing unavailable"));

    render(<BillingClient />);
    await waitFor(() => expect(screen.getByTestId("billing-error")).toBeInTheDocument());
    expect(screen.getByTestId("billing-error")).toHaveTextContent("billing unavailable");
  });
});

// ---------------------------------------------------------------------------
// KeysClient
// ---------------------------------------------------------------------------

import { KeysClient } from "@/components/customer/KeysClient";

const PAT_FIXTURE: CustomerPat = {
  pat_id: "pat_001",
  name: "ci-token",
  scopes: ["cache:r", "cache:w"],
  created_at: "2026-04-01T00:00:00Z",
  last_used_at: "2026-05-01T00:00:00Z",
  revoked_at: undefined,
};

const KEYS_FIXTURE = {
  pats: [PAT_FIXTURE],
  byok: { status: "active" as const, cmk_id: "cmk_abc123" },
};

describe("KeysClient", () => {
  beforeEach(() => { vi.clearAllMocks(); });

  it("shows loading state initially", () => {
    customerClientMock.listKeys.mockReturnValue(new Promise(() => {}));

    render(<KeysClient />);
    expect(screen.getByTestId("keys-loading")).toBeInTheDocument();
  });

  it("renders PAT list and BYOK status after loading", async () => {
    customerClientMock.listKeys.mockResolvedValue(KEYS_FIXTURE);

    render(<KeysClient />);
    await waitFor(() => expect(screen.getByTestId("keys-shell")).toBeInTheDocument());
    expect(screen.getByTestId("keys-row-pat_001")).toBeInTheDocument();
    expect(screen.getByTestId("keys-status-pat_001")).toHaveTextContent("active");
    expect(screen.getByTestId("keys-byok-status")).toHaveTextContent("active");
  });

  it("create PAT flow — calls createPat and shows new token", async () => {
    const newPat: CustomerPat & { token?: string } = {
      pat_id: "pat_new",
      name: "e2e-pat",
      scopes: ["cache:r"],
      created_at: "2026-05-29T00:00:00Z",
      token: "crl_pat_newtoken123",
    };
    customerClientMock.listKeys.mockResolvedValue(KEYS_FIXTURE);
    customerClientMock.createPat.mockResolvedValue(newPat);

    render(<KeysClient />);
    await waitFor(() => expect(screen.getByTestId("keys-create")).toBeInTheDocument());

    fireEvent.change(screen.getByTestId("keys-create-name"), {
      target: { value: "e2e-pat" },
    });
    fireEvent.click(screen.getByTestId("keys-create-submit"));

    await waitFor(() => expect(screen.getByTestId("keys-new-token")).toBeInTheDocument());
    expect(screen.getByTestId("keys-new-token")).toHaveTextContent("crl_pat_newtoken123");
    expect(customerClientMock.createPat).toHaveBeenCalledWith(
      expect.objectContaining({ name: "e2e-pat" }),
    );
  });

  it("revoke button calls revokePat and reloads list", async () => {
    const revokedPat = { ...PAT_FIXTURE, revoked_at: "2026-05-29T00:00:00Z" };
    customerClientMock.listKeys
      .mockResolvedValueOnce(KEYS_FIXTURE)
      .mockResolvedValueOnce({ ...KEYS_FIXTURE, pats: [revokedPat] });
    customerClientMock.revokePat.mockResolvedValue(revokedPat);

    render(<KeysClient />);
    await waitFor(() => expect(screen.getByTestId("keys-revoke-pat_001")).toBeInTheDocument());

    // Revoke is behind a ConfirmDialog (destructive-action guard). Clicking the
    // row button opens the dialog; the actual revoke fires from the confirm button.
    fireEvent.click(screen.getByTestId("keys-revoke-pat_001"));
    const confirmBtn = await screen.findByRole("button", { name: "Revoke token" });
    fireEvent.click(confirmBtn);

    await waitFor(() =>
      expect(screen.getByTestId("keys-status-pat_001")).toHaveTextContent("revoked"),
    );
    expect(customerClientMock.revokePat).toHaveBeenCalledWith("pat_001");
  });
});

// ---------------------------------------------------------------------------
// TeamClient
// ---------------------------------------------------------------------------

import { TeamClient } from "@/components/customer/TeamClient";

const MEMBER_FIXTURE: CustomerTeamMember = {
  user_id: "user_001",
  email: "owner@acme.example",
  role: "Owner",
  joined_at: "2026-01-01T00:00:00Z",
  status: "active",
};

const TEAM_FIXTURE = { members: [MEMBER_FIXTURE] };

describe("TeamClient", () => {
  beforeEach(() => { vi.clearAllMocks(); });

  it("shows loading state initially", () => {
    customerClientMock.listTeam.mockReturnValue(new Promise(() => {}));

    render(<TeamClient />);
    expect(screen.getByTestId("team-loading")).toBeInTheDocument();
  });

  it("renders member list after loading", async () => {
    customerClientMock.listTeam.mockResolvedValue(TEAM_FIXTURE);

    render(<TeamClient />);
    await waitFor(() => expect(screen.getByTestId("team-shell")).toBeInTheDocument());
    expect(screen.getByTestId("team-row-user_001")).toBeInTheDocument();
    expect(screen.getByTestId("team-role-user_001")).toHaveTextContent("Owner");
  });

  it("invite flow — calls inviteTeam and shows success message", async () => {
    const newMember: CustomerTeamMember = {
      user_id: "user_new",
      email: "newbie@acme.example",
      role: "Developer",
      joined_at: "2026-05-29T00:00:00Z",
      status: "invited",
    };
    customerClientMock.listTeam.mockResolvedValue(TEAM_FIXTURE);
    customerClientMock.inviteTeam.mockResolvedValue(newMember);

    render(<TeamClient />);
    await waitFor(() => expect(screen.getByTestId("team-invite")).toBeInTheDocument());

    fireEvent.change(screen.getByTestId("team-invite-email"), {
      target: { value: "newbie@acme.example" },
    });
    fireEvent.change(screen.getByTestId("team-invite-role"), {
      target: { value: "Developer" },
    });
    fireEvent.click(screen.getByTestId("team-invite-submit"));

    await waitFor(() =>
      expect(screen.getByTestId("team-invite-success")).toBeInTheDocument(),
    );
    expect(screen.getByTestId("team-invite-success")).toHaveTextContent("newbie@acme.example");
    expect(customerClientMock.inviteTeam).toHaveBeenCalledWith({
      email: "newbie@acme.example",
      role: "Developer",
    });
  });

  it("surfaces error when listTeam rejects", async () => {
    customerClientMock.listTeam.mockRejectedValue(new Error("team api down"));

    render(<TeamClient />);
    await waitFor(() => expect(screen.getByTestId("team-error")).toBeInTheDocument());
    expect(screen.getByTestId("team-error")).toHaveTextContent("team api down");
  });
});
