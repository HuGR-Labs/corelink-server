// WI-S16-005 — TenantSearch debounce test.

import React from "react";
import { render, screen, fireEvent, act, waitFor } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import TenantSearch from "@/components/admin/TenantSearch";
import { AdminClient } from "@/lib/admin-client";

describe("TenantSearch", () => {
  it("debounces query input — single request after rapid typing", async () => {
    vi.useFakeTimers();
    let callCount = 0;
    let lastQuery = "";
    const fetchImpl = (async (url: string) => {
      callCount += 1;
      const u = new URL(url);
      lastQuery = u.searchParams.get("q") ?? "";
      return new Response(JSON.stringify({ tenants: [] }), {
        status: 200,
        headers: { "content-type": "application/json" },
      });
    }) as unknown as typeof fetch;
    const client = new AdminClient({ baseUrl: "https://api.test", fetchImpl });

    render(<TenantSearch client={client} debounceMs={200} />);
    const input = screen.getByTestId("tenant-search-input");

    // Initial mount triggers one fetch (empty query) — flush its timer first.
    await act(async () => {
      vi.advanceTimersByTime(200);
      await Promise.resolve();
    });
    const baselineCalls = callCount;

    // Type 3 chars in quick succession.
    fireEvent.change(input, { target: { value: "a" } });
    fireEvent.change(input, { target: { value: "ac" } });
    fireEvent.change(input, { target: { value: "acm" } });

    // Before debounce window elapses, no new fetch.
    await act(async () => {
      vi.advanceTimersByTime(100);
      await Promise.resolve();
    });
    expect(callCount).toBe(baselineCalls);

    // After 200ms of inactivity, exactly one new fetch fires.
    await act(async () => {
      vi.advanceTimersByTime(200);
      await Promise.resolve();
    });
    expect(callCount).toBe(baselineCalls + 1);
    expect(lastQuery).toBe("acm");

    vi.useRealTimers();
  });

  it("renders tenant rows from search results", async () => {
    const fetchImpl = (async () => {
      return new Response(
        JSON.stringify({
          tenants: [
            {
              tenant_id: "tenant_a",
              name: "Acme",
              plan: "team",
              region: "us-east",
              byok_status: "active",
              created_at: "2026-01-01T00:00:00Z",
            },
          ],
        }),
        { status: 200, headers: { "content-type": "application/json" } },
      );
    }) as unknown as typeof fetch;
    const client = new AdminClient({ baseUrl: "https://api.test", fetchImpl });
    render(<TenantSearch client={client} debounceMs={0} />);
    await waitFor(() => {
      expect(screen.getByTestId("tenant-row-tenant_a")).toBeInTheDocument();
    });
  });
});
