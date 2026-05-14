import { describe, expect, it } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { DsrStatusListClient } from "../status/DsrStatusListClient";
import type { DsrRequestSummary } from "@/lib/dsr-types";

function makeRow(i: number): DsrRequestSummary {
  return {
    request_id: `01970000-aaaa-7000-8000-${String(i).padStart(12, "0")}`,
    action: "access",
    status: "pending",
    submitted_at: "2026-05-14T10:00:00Z",
    sla_deadline: new Date(Date.now() + 21 * 86_400_000).toISOString(),
    jurisdiction: "lgpd",
  };
}

describe("Status list pagination (Test 10)", () => {
  it("renders page 1 and advances on Next", () => {
    const page1 = {
      items: [makeRow(1), makeRow(2)],
      next_cursor: "cursor-2",
    };
    const page2 = { items: [makeRow(3)], next_cursor: undefined };
    render(
      <DsrStatusListClient locale="en" pages={[page1, page2]} />,
    );
    expect(
      screen.getByTestId("dsr-status-page-indicator"),
    ).toHaveTextContent("Page 1");
    expect(screen.getByTestId(`dsr-row-${page1.items[0].request_id}`)).toBeInTheDocument();
    fireEvent.click(screen.getByTestId("dsr-status-next"));
    expect(
      screen.getByTestId("dsr-status-page-indicator"),
    ).toHaveTextContent("Page 2");
    expect(
      screen.getByTestId(`dsr-row-${page2.items[0].request_id}`),
    ).toBeInTheDocument();
  });

  it("disables Next when there is no cursor", () => {
    render(
      <DsrStatusListClient
        locale="en"
        pages={[{ items: [makeRow(1)], next_cursor: undefined }]}
      />,
    );
    expect(screen.getByTestId("dsr-status-next")).toBeDisabled();
  });

  it("renders empty state when no items", () => {
    render(
      <DsrStatusListClient locale="en" pages={[{ items: [] }]} />,
    );
    expect(screen.getByText(/no data rights requests yet/i)).toBeInTheDocument();
  });
});
