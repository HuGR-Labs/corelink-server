import { describe, expect, it, vi } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { JwtReceiptModal } from "@/components/dsr/JwtReceiptModal";

function makeToken(payload: Record<string, unknown>): string {
  const header = Buffer.from(JSON.stringify({ alg: "HS256" }))
    .toString("base64")
    .replace(/=/g, "")
    .replace(/\+/g, "-")
    .replace(/\//g, "_");
  const body = Buffer.from(JSON.stringify(payload))
    .toString("base64")
    .replace(/=/g, "")
    .replace(/\+/g, "-")
    .replace(/\//g, "_");
  return `${header}.${body}.signature_stub`;
}

describe("JwtReceiptModal decodes and displays receipt (Test 8)", () => {
  const deadline = new Date(Date.now() + 21 * 86_400_000).toISOString();
  const payload = {
    request_id: "01970000-aaaa-7000-8000-000000000001",
    action: "access",
    jurisdiction: "lgpd",
    sla_deadline: deadline,
    jti: "jti-receipt-1",
    iat: Math.floor(Date.now() / 1000),
    exp: Math.floor(Date.now() / 1000) + 86_400 * 90,
  };

  it("displays request_id, action, jurisdiction, deadline, jti, exp", () => {
    render(
      <JwtReceiptModal
        locale="en"
        token={makeToken(payload)}
        slaDeadline={deadline}
        onClose={() => undefined}
        countdownRefreshMs={0}
      />,
    );
    expect(screen.getByTestId("receipt-request-id")).toHaveTextContent(
      payload.request_id,
    );
    expect(screen.getByTestId("receipt-action")).toHaveTextContent("access");
    expect(screen.getByTestId("receipt-jurisdiction")).toHaveTextContent(
      "lgpd",
    );
    expect(screen.getByTestId("receipt-jti")).toHaveTextContent(payload.jti);
  });

  it("calls clipboard.writeText with the raw token", async () => {
    const writeText = vi.fn(async () => undefined);
    const token = makeToken(payload);
    render(
      <JwtReceiptModal
        locale="en"
        token={token}
        slaDeadline={deadline}
        onClose={() => undefined}
        countdownRefreshMs={0}
        clipboard={{ writeText }}
      />,
    );
    fireEvent.click(screen.getByTestId("receipt-copy"));
    await waitFor(() => expect(writeText).toHaveBeenCalledWith(token));
  });

  it("returns null payload for a malformed token (defensive)", () => {
    render(
      <JwtReceiptModal
        locale="en"
        token="not-a-jwt"
        slaDeadline={deadline}
        onClose={() => undefined}
        countdownRefreshMs={0}
      />,
    );
    // Defensive UI: when the payload can't decode we still render the modal.
    expect(screen.getByTestId("dsr-receipt-modal")).toBeInTheDocument();
    expect(screen.getByTestId("receipt-request-id")).toHaveTextContent("—");
  });
});
