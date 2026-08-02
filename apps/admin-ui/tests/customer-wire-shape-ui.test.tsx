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

describe("KeysClient against the real POST /v1/customer/keys wire", () => {
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

describe("SettingsClient against the real POST /v1/customer/account/delete wire", () => {
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
