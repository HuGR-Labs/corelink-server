/**
 * WIRE-SHAPE, END TO END THROUGH THE UI.
 *
 * `tests/customer-client.test.ts` pins the client's unwrapping in isolation, and
 * `tests/customer-dashboard.test.tsx` renders the screens against a MOCKED
 * client. Neither can catch a mismatch between the two: the dashboard mock hands
 * the component whatever shape the test author picked, so a client that unwraps
 * correctly and a screen that destructures the old shape would both stay green.
 *
 * These tests close that seam. They mount the REAL screen with the REAL
 * `CustomerClient`, fed the EXACT JSON the Rust handler emits (copied from
 * `crates/corelink-container/src/routes/customer.rs`), and assert on what the
 * USER sees. `request<T>()` only CASTS the parsed JSON, so a wrong declared type
 * is invisible to `tsc` and surfaces as `undefined` in rendered copy — that
 * rendered copy is what is asserted here.
 */

import * as React from "react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";

// Clerk is not mounted in jsdom. SettingsClient reads identity from
// useUser/useOrganization; the client hook is replaced below, so useAuth is only
// needed for completeness.
vi.mock("@clerk/nextjs", () => ({
  useAuth: () => ({ getToken: async () => null }),
  useUser: () => ({ isLoaded: true, user: null }),
  useOrganization: () => ({ isLoaded: true, organization: null, membership: null }),
}));

// The screens obtain their client through this hook. Swap the hook (NOT the
// client) so the REAL CustomerClient — including its unwrapping — is exercised.
const wireFetch = vi.fn();
vi.mock("@/lib/use-customer-client", async () => {
  const { CustomerClient } = await import("@/lib/customer-client");
  return {
    useCustomerClient: () =>
      new CustomerClient({
        baseUrl: "https://api.test",
        fetchImpl: ((...args: unknown[]) => wireFetch(...args)) as unknown as typeof fetch,
      }),
  };
});

import { KeysClient } from "@/components/customer/KeysClient";
import { SettingsClient } from "@/components/customer/SettingsClient";
import { TeamClient } from "@/components/customer/TeamClient";

// Mounting a whole screen in jsdom costs ~1.1 s isolated, but this Mac is also the
// self-hosted CI runner fleet (load average has hit 700+) and the first case here
// was measured at 5742 ms under that load — over vitest's 5000 ms default. Give
// every suite in this file explicit headroom instead of asserting less.
const WIRE_UI_TIMEOUT_MS = 30_000;

function json(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

/** `GET /v1/customer/keys` → `{ pats, byok }` (routes/customer.rs:749-766). */
const KEYS_LIST_WIRE = {
  pats: [
    {
      pat_id: "pat_001",
      name: "existing-token",
      scopes: ["cache:r"],
      created_at: "2026-04-01T00:00:00Z",
      last_used_at: null,
      revoked_at: null,
    },
  ],
  byok: { status: "inactive", cmk_id: null },
};

/** `POST /v1/customer/keys` → 201 `{ pat: {…}, token }` (routes/customer.rs:810-820). */
const KEYS_CREATE_WIRE = {
  pat: {
    pat_id: "pat_new",
    name: "ci-github",
    scopes: ["cache:r"],
    created_at: "2026-08-01T00:00:00Z",
    last_used_at: null,
    revoked_at: null,
  },
  token: "crl_pat_shown_once_secret",
};

/** `GET /v1/customer/team` → `{ members }` (routes/customer.rs:894-902). */
const TEAM_LIST_WIRE = {
  members: [
    {
      user_id: "usr_owner",
      email: "owner@company.com",
      role: "Owner",
      joined_at: "2026-04-01T00:00:00Z",
      status: "active",
    },
  ],
};

/** `POST /v1/customer/team/invite` → 201 `{ member: {…} }` (routes/customer.rs:961-969). */
const TEAM_INVITE_WIRE = {
  member: {
    user_id: "usr_invited",
    email: "teammate@company.com",
    role: "Developer",
    joined_at: "2026-08-01T00:00:00Z",
    status: "invited",
  },
};

describe("KeysClient against the real POST /v1/customer/keys wire", { timeout: WIRE_UI_TIMEOUT_MS }, () => {
  beforeEach(() => {
    wireFetch.mockReset();
  });

  it("names the token the user just minted in the success toast", async () => {
    wireFetch.mockImplementation(async (url: string, init?: RequestInit) => {
      if (init?.method === "POST") return json(KEYS_CREATE_WIRE, 201);
      return json(KEYS_LIST_WIRE);
    });

    render(<KeysClient />);
    await waitFor(() => expect(screen.getByTestId("keys-create")).toBeInTheDocument());

    fireEvent.change(screen.getByTestId("keys-create-name"), {
      target: { value: "ci-github" },
    });
    fireEvent.click(screen.getByTestId("keys-create-submit"));

    // The user-visible confirmation. Before the unwrap this rendered
    // `Token "undefined" created`.
    const toast = await screen.findByRole("status");
    expect(toast).toHaveTextContent('Token "ci-github" created');
    expect(toast).not.toHaveTextContent("undefined");

    // The shown-once secret still reaches the reveal modal.
    expect(screen.getByTestId("pat-value")).toHaveTextContent("crl_pat_shown_once_secret");
  });
});

describe("SettingsClient against the real POST /v1/customer/account/delete wire", { timeout: WIRE_UI_TIMEOUT_MS }, () => {
  beforeEach(() => {
    wireFetch.mockReset();
  });

  async function confirmDelete(): Promise<void> {
    render(<SettingsClient />);
    fireEvent.click(screen.getByTestId("settings-delete-account"));
    const confirmBtn = await screen.findByRole("button", { name: "Delete everything" });
    fireEvent.click(confirmBtn);
  }

  it("confirms the erasure without leaking an id the server never sends", async () => {
    // routes/customer.rs:1082-1085 — 202 `{ ok, status }`. NO request_id.
    wireFetch.mockResolvedValue(json({ ok: true, status: "erasure_requested" }, 202));

    await confirmDelete();

    // Before the fix this rendered "Erasure requested (undefined)".
    const toast = await screen.findByRole("status");
    expect(toast).not.toHaveTextContent("undefined");
    expect(toast).toHaveTextContent(/erasure requested/i);
    expect(toast).toHaveTextContent(/signed out/i);
  });

  it("tells the truth on the idempotent no_account ack", async () => {
    // routes/customer.rs:1090-1093 — 202 `{ ok: true, status: "no_account" }`.
    wireFetch.mockResolvedValue(json({ ok: true, status: "no_account" }, 202));

    await confirmDelete();

    const toast = await screen.findByRole("status");
    expect(toast).not.toHaveTextContent("undefined");
    expect(toast).toHaveTextContent(/nothing left to erase/i);
  });
});

// This is the envelope whose breakage users actually SAW: with the bare cast,
// `inviteTeam` handed the screen the `{ member }` wrapper itself, so `m.email`
// was `undefined` — the confirmation named nobody, and the address the user must
// pass on to their teammate (we store only a hash of it, so this is the one time
// it can be echoed back) was lost. `tests/customer-client.test.ts` pins the
// unwrap at client level; this pins what the USER reads.
describe(
  "TeamClient against the real POST /v1/customer/team/invite wire",
  { timeout: WIRE_UI_TIMEOUT_MS },
  () => {
    beforeEach(() => {
      wireFetch.mockReset();
    });

    it("names the teammate whose seat was just reserved", async () => {
      wireFetch.mockImplementation(async (url: string, init?: RequestInit) => {
        if (init?.method === "POST") return json(TEAM_INVITE_WIRE, 201);
        return json(TEAM_LIST_WIRE);
      });

      render(<TeamClient />);
      await waitFor(() => expect(screen.getByTestId("team-invite")).toBeInTheDocument());

      fireEvent.change(screen.getByTestId("team-invite-email"), {
        target: { value: "teammate@company.com" },
      });
      fireEvent.click(screen.getByTestId("team-invite-submit"));

      // The user-visible confirmation. Before the unwrap this named `undefined`.
      const toast = await screen.findByRole("status");
      expect(toast).toHaveTextContent("Seat reserved for teammate@company.com");
      expect(toast).not.toHaveTextContent("undefined");

      // The persistent callout echoes the same address back — it is the only
      // place the invitee's email is ever shown again.
      const callout = await screen.findByTestId("team-invite-success");
      expect(callout).toHaveTextContent("Seat reserved for teammate@company.com");
      expect(callout).not.toHaveTextContent("undefined");
    });
  },
);
