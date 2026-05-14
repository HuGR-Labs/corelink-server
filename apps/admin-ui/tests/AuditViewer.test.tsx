// WI-S16-005 — AuditViewer integration tests.

import React from "react";
import { render, screen, fireEvent, waitFor, act } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import AuditViewer from "@/components/admin/AuditViewer";
import { AdminClient } from "@/lib/admin-client";
import type { AuditEventSummary } from "@/lib/types";

function makeRow(over: Partial<AuditEventSummary> = {}): AuditEventSummary {
  return {
    event_id: "evt_a",
    ts: "2026-05-14T10:00:00Z",
    tenant_id: "tenant_a",
    event_type: "auth.login",
    severity: "info",
    actor: "u_alice",
    summary: "login ok",
    correlation_id: "corr_1",
    ...over,
  };
}

describe("AuditViewer", () => {
  it("preserves filter when paginating with cursor", async () => {
    const calls: Array<Record<string, unknown>> = [];
    const client = new AdminClient({
      baseUrl: "https://api.test",
      fetchImpl: (async (url: string) => {
        const u = new URL(url);
        const params = Object.fromEntries(u.searchParams.entries());
        calls.push(params);
        const cursor = params.cursor;
        if (!cursor) {
          return new Response(
            JSON.stringify({
              rows: [makeRow({ event_id: "evt_a" })],
              next_cursor: "c_2",
            }),
            { status: 200, headers: { "content-type": "application/json" } },
          );
        }
        return new Response(
          JSON.stringify({
            rows: [makeRow({ event_id: "evt_b" })],
            next_cursor: null,
          }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }) as unknown as typeof fetch,
    });

    render(
      <AuditViewer
        client={client}
        initialFilter={{ tenant_id: "tenant_a", event_types: ["auth.login"] }}
      />,
    );

    await waitFor(() => {
      expect(screen.getByTestId("audit-row-evt_a")).toBeInTheDocument();
    });
    expect(calls[0]!.tenant_id).toBe("tenant_a");
    expect(calls[0]!.event_types).toBe("auth.login");

    await act(async () => {
      fireEvent.click(screen.getByTestId("next-page"));
    });

    await waitFor(() => {
      expect(screen.getByTestId("audit-row-evt_b")).toBeInTheDocument();
    });
    const lastCall = calls[calls.length - 1]!;
    // Filter MUST still be present in the second call.
    expect(lastCall.tenant_id).toBe("tenant_a");
    expect(lastCall.event_types).toBe("auth.login");
    expect(lastCall.cursor).toBe("c_2");
  });

  it("triggers export endpoint when export button is clicked", async () => {
    const fetchSpy = vi.fn(async (url: string, init?: RequestInit) => {
      if ((init?.method ?? "GET") === "GET") {
        return new Response(JSON.stringify({ rows: [], next_cursor: null }), {
          status: 200,
          headers: { "content-type": "application/json" },
        });
      }
      return new Response(
        JSON.stringify({
          signed_url: "https://signed.example/x",
          expires_at: "2026-05-15T10:00:00Z",
          format: "csv",
        }),
        { status: 200, headers: { "content-type": "application/json" } },
      );
    });
    const client = new AdminClient({
      baseUrl: "https://api.test",
      fetchImpl: fetchSpy as unknown as typeof fetch,
    });

    const originalOpen = window.open;
    const openSpy = vi.fn();
    window.open = openSpy as unknown as typeof window.open;

    try {
      render(<AuditViewer client={client} initialFilter={{ tenant_id: "t1" }} />);
      await waitFor(() => {
        expect(screen.getByTestId("audit-table")).toBeInTheDocument();
      });
      await act(async () => {
        fireEvent.click(screen.getByTestId("export-btn"));
      });
      await waitFor(() => {
        expect(openSpy).toHaveBeenCalledWith(
          "https://signed.example/x",
          "_blank",
          "noopener,noreferrer",
        );
      });
      // Verify the export request was the POST to /audit/export.
      const exportCall = fetchSpy.mock.calls.find(
        (c) => (c[1]?.method ?? "GET") === "POST",
      );
      expect(exportCall).toBeDefined();
      expect(exportCall![0]).toBe("https://api.test/v1/admin/audit/export");
    } finally {
      window.open = originalOpen;
    }
  });
});
