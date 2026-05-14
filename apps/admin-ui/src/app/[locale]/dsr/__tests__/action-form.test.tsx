import { describe, expect, it, vi } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { DsrActionPageClient } from "../[action]/DsrActionPageClient";
import type {
  DataCategory,
  DsrAction,
  DsrSubmitResponse,
  MeProfile,
} from "@/lib/dsr-types";

const profile: MeProfile = {
  email: "alice@example.com",
  name: "Alice Example",
  language: "en",
};

const categories: DataCategory[] = [
  { id: "billing", label: "Billing data" },
  { id: "telemetry", label: "Usage telemetry" },
];

function makeSubmitResponse(action: DsrAction): DsrSubmitResponse {
  return {
    request_id: "01970000-aaaa-7000-8000-000000000001",
    action,
    jurisdiction: "lgpd",
    sla_deadline: new Date(Date.now() + 20 * 86_400_000).toISOString(),
    jwt_receipt:
      "eyJhbGciOiJIUzI1NiJ9." +
      Buffer.from(
        JSON.stringify({
          request_id: "01970000-aaaa-7000-8000-000000000001",
          action,
          jurisdiction: "lgpd",
          sla_deadline: new Date(Date.now() + 20 * 86_400_000).toISOString(),
          jti: "jti-001",
          iat: Math.floor(Date.now() / 1000),
          exp: Math.floor(Date.now() / 1000) + 86_400 * 90,
        }),
      )
        .toString("base64")
        .replace(/=/g, "")
        .replace(/\+/g, "-")
        .replace(/\//g, "_") +
      ".sig",
  };
}

function renderAction(action: DsrAction, opts?: { verifiedAt?: number }) {
  const submit = vi.fn(async () => makeSubmitResponse(action));
  const verifier = {
    startVerification: vi.fn(async () => Date.now()),
  };
  const utils = render(
    <DsrActionPageClient
      locale="en"
      action={action}
      testOverrides={{
        profile,
        categories,
        token: "fake_token_test",
        verifier,
        submit,
        initialVerifiedAt: opts?.verifiedAt,
      }}
    />,
  );
  return { ...utils, submit, verifier };
}

describe("DSR action route per-action rendering (Test 2)", () => {
  const actions: DsrAction[] = [
    "access",
    "rectification",
    "erasure",
    "portability",
    "restriction",
    "objection",
  ];

  for (const action of actions) {
    it(`renders the correct form fields for action=${action}`, () => {
      renderAction(action, { verifiedAt: Date.now() });
      expect(screen.getByTestId(`dsr-form-${action}`)).toBeInTheDocument();
      if (action === "rectification") {
        expect(
          screen.getByTestId("dsr-rectification-fields"),
        ).toBeInTheDocument();
      }
      if (action === "erasure") {
        expect(screen.getByTestId("dsr-erasure-fields")).toBeInTheDocument();
      }
      if (action === "portability") {
        expect(
          screen.getByTestId("dsr-portability-fields"),
        ).toBeInTheDocument();
      }
      if (action === "objection") {
        expect(screen.getByTestId("dsr-objection-fields")).toBeInTheDocument();
      }
    });
  }
});

describe("ReAuthGate gates the form (Test 3)", () => {
  it("does not render the form until MFA verifies", async () => {
    renderAction("access");
    expect(screen.queryByTestId("dsr-form-access")).toBeNull();
    expect(screen.getByTestId("dsr-reauth-gate")).toBeInTheDocument();
    fireEvent.click(screen.getByTestId("dsr-reauth-start"));
    await waitFor(() =>
      expect(screen.getByTestId("dsr-form-access")).toBeInTheDocument(),
    );
  });
});

describe("Rectification pre-fills profile (Test 4)", () => {
  it("seeds name/email/language fields from the profile", () => {
    renderAction("rectification", { verifiedAt: Date.now() });
    expect(
      (screen.getByTestId("dsr-field-name") as HTMLInputElement).value,
    ).toBe("Alice Example");
    expect(
      (screen.getByTestId("dsr-field-email") as HTMLInputElement).value,
    ).toBe("alice@example.com");
    expect(
      (screen.getByTestId("dsr-field-language") as HTMLInputElement).value,
    ).toBe("en");
  });
});

describe("Erasure multi-select (Test 5)", () => {
  it("toggles category state when switched to category scope", () => {
    renderAction("erasure", { verifiedAt: Date.now() });
    fireEvent.click(screen.getByTestId("dsr-erasure-scope-categories"));
    const billing = screen.getByTestId(
      "dsr-erasure-cat-billing",
    ) as HTMLInputElement;
    fireEvent.click(billing);
    expect(billing.checked).toBe(true);
    fireEvent.click(billing);
    expect(billing.checked).toBe(false);
  });
});

describe("Portability format choice (Test 6)", () => {
  it("supports json / csv / both selections", () => {
    renderAction("portability", { verifiedAt: Date.now() });
    const csv = screen.getByTestId(
      "dsr-portability-format-csv",
    ) as HTMLInputElement;
    fireEvent.click(csv);
    expect(csv.checked).toBe(true);
    const both = screen.getByTestId(
      "dsr-portability-format-both",
    ) as HTMLInputElement;
    fireEvent.click(both);
    expect(both.checked).toBe(true);
  });
});

describe("Restriction/objection require reason (Test 7)", () => {
  it("blocks restriction submit when reason is empty", async () => {
    const { submit } = renderAction("restriction", {
      verifiedAt: Date.now(),
    });
    fireEvent.submit(screen.getByTestId("dsr-form-restriction"));
    await waitFor(() =>
      expect(screen.getByTestId("dsr-form-error")).toBeInTheDocument(),
    );
    expect(submit).not.toHaveBeenCalled();
  });

  it("blocks objection submit when reason is empty", async () => {
    const { submit } = renderAction("objection", { verifiedAt: Date.now() });
    fireEvent.submit(screen.getByTestId("dsr-form-objection"));
    await waitFor(() =>
      expect(screen.getByTestId("dsr-form-error")).toBeInTheDocument(),
    );
    expect(submit).not.toHaveBeenCalled();
  });

  it("allows access submit with empty reason", async () => {
    const { submit } = renderAction("access", { verifiedAt: Date.now() });
    fireEvent.submit(screen.getByTestId("dsr-form-access"));
    await waitFor(() => expect(submit).toHaveBeenCalledTimes(1));
  });
});
