// WI-S16-005 — TenantDeepDive: all 5 cards render; payment_method redacted to last4.

import React from "react";
import { render, screen } from "@testing-library/react";
import { describe, it, expect } from "vitest";
import TenantDeepDive from "@/components/admin/TenantDeepDive";

describe("TenantDeepDive", () => {
  it("renders all 5 cards (usage, billing, consents, dsr, pats)", () => {
    render(
      <TenantDeepDive
        tenant={{
          tenant_id: "tenant_a",
          name: "Acme",
          plan: "team",
          region: "us-east",
          byok_status: "active",
          created_at: "2026-01-01T00:00:00Z",
        }}
        usage={{ cas_hit_ratio: 0.95, gb_stored: 12.3, gb_egress: 4.5 }}
        billing={{
          plan: "team",
          mrr_usd: 199,
          next_invoice: "2026-06-01",
          payment_method_last4: "4242",
        }}
        consents={{ granted: 7, revoked: 1, last_capture: "2026-05-10" }}
        dsr={{ pending: 2, in_progress: 1, completed: 5 }}
        pats={[{ pat_id: "pat_1", scope: "read", created_at: "2026-05-01" }]}
      />,
    );
    expect(screen.getByTestId("card-usage")).toBeInTheDocument();
    expect(screen.getByTestId("card-billing")).toBeInTheDocument();
    expect(screen.getByTestId("card-consents")).toBeInTheDocument();
    expect(screen.getByTestId("card-dsr")).toBeInTheDocument();
    expect(screen.getByTestId("card-pats")).toBeInTheDocument();
  });

  it("redacts payment_method to last4 — never full PAN", () => {
    render(
      <TenantDeepDive
        tenant={{
          tenant_id: "tenant_a",
          name: "Acme",
          plan: "team",
          region: "us-east",
          byok_status: "active",
          created_at: "2026-01-01T00:00:00Z",
        }}
        billing={{
          plan: "team",
          mrr_usd: 199,
          next_invoice: "2026-06-01",
          payment_method_last4: "4242",
        }}
      />,
    );
    const pm = screen.getByTestId("payment-method");
    expect(pm.textContent).toBe("••••4242");
    // Defensive — no other 16-digit-looking sequence anywhere in DOM.
    expect(document.body.textContent).not.toMatch(/\d{13,}/);
  });
});
