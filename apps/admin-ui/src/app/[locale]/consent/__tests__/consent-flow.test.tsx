/**
 * @vitest-environment happy-dom
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { ConsentCaptureFlow } from "@/components/consent/ConsentCaptureFlow";
import { ConsentDashboard } from "@/components/consent/ConsentDashboard";
import { ConsentHistory } from "@/components/consent/ConsentHistory";
import { WithdrawForm } from "@/components/consent/WithdrawForm";
import { JwtReceiptDisplay } from "@/components/consent/JwtReceiptDisplay";
import { __setHtml2Canvas } from "@/lib/consent-screenshot";
import { makeJwtReceipt, makeMockApi, type MockApiState } from "./test-utils";

// The consent components now mint a Clerk session bearer via `useConsentApi()`
// so their `/v1/consent/*` calls are authenticated against the real API origin
// (they previously fetched bare `/v1/...` paths, which resolved against the
// apex MARKETING app). Every test here injects its own `api` stub, so Clerk is
// never actually consulted — but the hook still runs, and `useAuth()` throws
// outside a <ClerkProvider>. Same module mock the customer-screen suites use.
vi.mock("@clerk/nextjs", () => ({
  useAuth: () => ({ getToken: async () => null }),
}));

const NOTICE_TEXT = "Test notice body for hashing.";
const NOTICE_VERSION = "1.2.3";
const LOCALE_PT = "pt-BR";

function setCookie(name: string, value: string): void {
  document.cookie = `${name}=${value}; path=/`;
}

function clearCookies(): void {
  document.cookie.split(";").forEach((c) => {
    const eq = c.indexOf("=");
    const name = eq > -1 ? c.substring(0, eq).trim() : c.trim();
    document.cookie = `${name}=; expires=Thu, 01 Jan 1970 00:00:00 GMT; path=/`;
  });
}

function stubScreenshot(includeText: string[]): void {
  __setHtml2Canvas(async (el: HTMLElement) => {
    // Simulate html2canvas snapshotting the element's text content.
    const captured = el.textContent ?? "";
    // Build a minimal "canvas-like" returning a deterministic data url that
    // embeds whether each required token was present (so the test can
    // assert the 6 fields + timestamp appeared inside the captured DOM).
    const flags = includeText.map((t) => (captured.includes(t) ? "1" : "0")).join("");
    const dataUrlPayload = Buffer.from(`flags=${flags}`).toString("base64");
    return {
      toDataURL: (_mime?: string) => `data:image/png;base64,${dataUrlPayload}`,
    } as unknown as HTMLCanvasElement;
  });
}

// Snapshot the IntersectionObserver that may be provided by the active DOM
// shim. The ScrollToBottomGuard tests below rely on a synthetic `scroll`
// event firing the "reach bottom" callback — to make that deterministic in
// happy-dom we install a stub that:
//   1. lets next/link's prefetch hook obtain an IO instance (so <Link> doesn't
//      crash with ReferenceError), and
//   2. fires its callback with `intersectionRatio: 1` on a `scroll` event,
//      which is what test code dispatches to simulate scroll-to-end.
const ORIGINAL_IO: unknown = (globalThis as { IntersectionObserver?: unknown })
  .IntersectionObserver;

class ScrollDrivenIO {
  callback: IntersectionObserverCallback;
  targets: Element[] = [];
  scrollHandler: () => void;
  constructor(cb: IntersectionObserverCallback) {
    this.callback = cb;
    this.scrollHandler = () => {
      const entries = this.targets.map(
        (t) =>
          ({
            isIntersecting: true,
            intersectionRatio: 1,
            target: t,
            boundingClientRect: t.getBoundingClientRect(),
            intersectionRect: t.getBoundingClientRect(),
            rootBounds: null,
            time: 0,
          }) as unknown as IntersectionObserverEntry,
      );
      this.callback(entries, this as unknown as IntersectionObserver);
    };
    if (typeof window !== "undefined") {
      window.addEventListener("scroll", this.scrollHandler);
    }
  }
  observe(target: Element): void {
    this.targets.push(target);
  }
  unobserve(target: Element): void {
    this.targets = this.targets.filter((t) => t !== target);
  }
  disconnect(): void {
    this.targets = [];
    if (typeof window !== "undefined") {
      window.removeEventListener("scroll", this.scrollHandler);
    }
  }
  takeRecords(): IntersectionObserverEntry[] {
    return [];
  }
  root: Element | Document | null = null;
  rootMargin = "";
  thresholds: ReadonlyArray<number> = [];
}

beforeEach(async () => {
  // Ensure Web Crypto subtle is available (happy-dom provides it; safety guard).
  if (typeof crypto === "undefined" || !crypto.subtle) {
    const nodeCrypto = await import("node:crypto");
    (globalThis as unknown as { crypto: Crypto }).crypto =
      nodeCrypto.webcrypto as unknown as Crypto;
  }
  (globalThis as { IntersectionObserver?: unknown }).IntersectionObserver =
    ScrollDrivenIO;
  if (typeof window !== "undefined") {
    (window as unknown as { IntersectionObserver?: unknown }).IntersectionObserver =
      ScrollDrivenIO;
  }
  clearCookies();
  setCookie("corelink_locale", LOCALE_PT);
  document.documentElement.lang = LOCALE_PT;
});

afterEach(() => {
  __setHtml2Canvas(null);
  vi.restoreAllMocks();
  (globalThis as { IntersectionObserver?: unknown }).IntersectionObserver = ORIGINAL_IO;
  if (typeof window !== "undefined") {
    (window as unknown as { IntersectionObserver?: unknown }).IntersectionObserver =
      ORIGINAL_IO;
  }
});

describe("ConsentCaptureFlow — 6 fields + a11y", () => {
  it("renders all 6 CTRL-PRIV-CONSENT fields with proper labels (a11y)", async () => {
    stubScreenshot([]);
    const state: MockApiState = {
      active: [],
      historyRows: [],
      historyCalls: [],
      grantCalls: [],
      withdrawCalls: [],
    };
    render(
      <ConsentCaptureFlow
        locale={LOCALE_PT}
        noticeText={NOTICE_TEXT}
        noticeVersion={NOTICE_VERSION}
        thirdParties={["Cloudflare"]}
        api={makeMockApi(state)}
      />,
    );
    expect(screen.getByTestId("field-purpose")).toHaveAttribute("aria-required", "true");
    expect(screen.getByLabelText(/1\. Finalidade/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/2\. Base legal/i)).toBeInTheDocument();
    expect(screen.getByText(/3\. Categorias de dados/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/4\. Periodo de retencao/i)).toBeInTheDocument();
    expect(screen.getByText(/5\. Sub-processadores/i)).toBeInTheDocument();
    expect(screen.getByText(/6\. Metodo de revogacao/i)).toBeInTheDocument();
    expect(screen.getByTestId("field-third-parties")).toHaveAttribute(
      "aria-labelledby",
      "consent-third-parties-label",
    );
    expect(screen.getByTestId("field-withdrawal-method")).toHaveAttribute(
      "aria-labelledby",
      "consent-withdrawal-label",
    );
    expect(screen.getByTestId("field-withdrawal-method")).toHaveTextContent(
      /Withdraw via \/consent\/withdraw/,
    );
  });
});

describe("ScrollToBottomGuard", () => {
  it("disables submit before scroll and enables after reach-bottom + locale match", async () => {
    stubScreenshot([NOTICE_TEXT]);
    const state: MockApiState = {
      active: [],
      historyRows: [],
      historyCalls: [],
      grantCalls: [],
      withdrawCalls: [],
    };
    render(
      <ConsentCaptureFlow
        locale={LOCALE_PT}
        noticeText={NOTICE_TEXT}
        noticeVersion={NOTICE_VERSION}
        thirdParties={["Cloudflare"]}
        api={makeMockApi(state)}
      />,
    );

    // Fill required fields in review step.
    fireEvent.change(screen.getByTestId("field-purpose"), { target: { value: "Analytics" } });
    fireEvent.click(screen.getByTestId("cat-usage"));
    fireEvent.click(screen.getByTestId("to-scroll-step"));

    // Scroll step rendered, button not yet visible.
    expect(screen.getByTestId("step-scroll")).toBeInTheDocument();
    expect(screen.queryByTestId("consent-submit")).toBeNull();

    // Manually trigger the IntersectionObserver-equivalent path by
    // dispatching a scroll event. The fallback handler will fire because
    // happy-dom does not provide IntersectionObserver out of the box.
    // The component switches step → "consent" once onReachBottom fires.
    await act(async () => {
      window.dispatchEvent(new Event("scroll"));
    });

    await waitFor(() => expect(screen.getByTestId("step-consent")).toBeInTheDocument());
    expect(screen.getByTestId("consent-submit")).not.toBeDisabled();
  });
});

describe("CTRL-PRIV-CONSENT-005 — locale match", () => {
  it("refuses to submit when cookie locale != active i18n locale and does NOT read navigator.language", async () => {
    stubScreenshot([NOTICE_TEXT]);
    // Cookie says en-US but the [locale] segment is pt-BR → mismatch.
    setCookie("corelink_locale", "en-US");
    document.documentElement.lang = "en-US";

    // Sabotage navigator.language to prove we don't read it.
    Object.defineProperty(window.navigator, "language", {
      value: "pt-BR",
      configurable: true,
    });

    const state: MockApiState = {
      active: [],
      historyRows: [],
      historyCalls: [],
      grantCalls: [],
      withdrawCalls: [],
    };
    render(
      <ConsentCaptureFlow
        locale={LOCALE_PT}
        noticeText={NOTICE_TEXT}
        noticeVersion={NOTICE_VERSION}
        thirdParties={["Cloudflare"]}
        api={makeMockApi(state)}
      />,
    );

    fireEvent.change(screen.getByTestId("field-purpose"), { target: { value: "Analytics" } });
    fireEvent.click(screen.getByTestId("cat-usage"));
    fireEvent.click(screen.getByTestId("to-scroll-step"));
    await act(async () => {
      window.dispatchEvent(new Event("scroll"));
    });
    await waitFor(() => expect(screen.getByTestId("step-consent")).toBeInTheDocument());
    fireEvent.click(screen.getByTestId("consent-submit"));

    await waitFor(() =>
      expect(screen.getByTestId("submit-error")).toHaveTextContent(/locale_mismatch/),
    );
    expect(state.grantCalls).toHaveLength(0);
  });
});

describe("Screenshot evidence (EVT-012)", () => {
  it("captures a PNG data URL whose source DOM contains the 6 fields + timestamp", async () => {
    // The stub returns a data URL encoding which tokens were present in DOM.
    const tokens = [
      "1. Finalidade",
      "2. Base legal",
      "3. Categorias de dados",
      "4. Periodo de retencao",
      "5. Sub-processadores",
      "6. Metodo de revogacao",
      "Capturado em",
    ];
    stubScreenshot(tokens);
    const state: MockApiState = {
      active: [],
      historyRows: [],
      historyCalls: [],
      grantCalls: [],
      withdrawCalls: [],
    };
    render(
      <ConsentCaptureFlow
        locale={LOCALE_PT}
        noticeText={NOTICE_TEXT}
        noticeVersion={NOTICE_VERSION}
        thirdParties={["Cloudflare"]}
        api={makeMockApi(state)}
      />,
    );
    fireEvent.change(screen.getByTestId("field-purpose"), { target: { value: "Analytics" } });
    fireEvent.click(screen.getByTestId("cat-usage"));
    fireEvent.click(screen.getByTestId("to-scroll-step"));
    await act(async () => {
      window.dispatchEvent(new Event("scroll"));
    });
    await waitFor(() => expect(screen.getByTestId("step-consent")).toBeInTheDocument());
    fireEvent.click(screen.getByTestId("consent-submit"));
    await waitFor(() => expect(state.grantCalls).toHaveLength(1));

    const payload = state.grantCalls[0] as { screenshot_evidence_base64: string };
    expect(payload.screenshot_evidence_base64).toMatch(/^data:image\/png;base64,/);
    const base64Part = payload.screenshot_evidence_base64.split(",")[1];
    if (!base64Part) throw new Error("unreachable: screenshot has no base64 payload");
    const flagsPayload = Buffer.from(base64Part, "base64").toString("utf8");
    const flags = flagsPayload.replace("flags=", "");
    // All 7 tokens (6 fields + timestamp label) must have been present.
    expect(flags).toBe("1".repeat(tokens.length));
  });
});

describe("Submit payload schema", () => {
  it("populates all 6 fields + locale + wording_id + ui_capture_ts + hash + version", async () => {
    stubScreenshot([NOTICE_TEXT]);
    const state: MockApiState = {
      active: [],
      historyRows: [],
      historyCalls: [],
      grantCalls: [],
      withdrawCalls: [],
    };
    render(
      <ConsentCaptureFlow
        locale={LOCALE_PT}
        noticeText={NOTICE_TEXT}
        noticeVersion={NOTICE_VERSION}
        thirdParties={["Cloudflare", "Stripe"]}
        api={makeMockApi(state)}
      />,
    );
    fireEvent.change(screen.getByTestId("field-purpose"), {
      target: { value: "Improve product analytics" },
    });
    fireEvent.click(screen.getByTestId("cat-usage"));
    fireEvent.click(screen.getByTestId("cat-derived"));
    fireEvent.change(screen.getByTestId("field-legal-basis"), { target: { value: "consent" } });
    fireEvent.change(screen.getByTestId("field-retention"), { target: { value: "90d" } });
    fireEvent.click(screen.getByTestId("to-scroll-step"));
    await act(async () => {
      window.dispatchEvent(new Event("scroll"));
    });
    await waitFor(() => expect(screen.getByTestId("step-consent")).toBeInTheDocument());
    fireEvent.click(screen.getByTestId("consent-submit"));
    await waitFor(() => expect(state.grantCalls).toHaveLength(1));

    const p = state.grantCalls[0] as Record<string, unknown>;
    expect(p.purpose).toBe("Improve product analytics");
    expect(p.legal_basis).toBe("consent");
    expect(p.data_categories).toEqual(["usage", "derived"]);
    expect(p.retention_period).toBe("90d");
    expect(p.third_parties).toEqual(["Cloudflare", "Stripe"]);
    expect(p.withdrawal_method).toMatch(/privacy@/);
    expect(p.locale).toBe(LOCALE_PT);
    expect(typeof p.wording_id).toBe("string");
    expect((p.wording_id as string).length).toBeGreaterThan(0);
    expect(typeof p.ui_capture_ts).toBe("number");
    expect(typeof p.notice_text_hash).toBe("string");
    expect((p.notice_text_hash as string)).toMatch(/^[0-9a-f]{64}$/);
    expect(p.notice_version).toBe(NOTICE_VERSION);
  });
});

describe("JwtReceiptDisplay", () => {
  it("decodes header + payload and renders the 6 claim fields", () => {
    const jwt = makeJwtReceipt({
      tenant_id: "tnt_abc",
      consent_id: "csn_xyz",
      granted_at: "2026-05-14T10:00:00Z",
      locale: "pt-BR",
      jti: "jti-1",
      exp: 1900000000,
    });
    render(<JwtReceiptDisplay jwt={jwt} />);
    expect(screen.getByTestId("claim-tenant_id")).toHaveTextContent("tnt_abc");
    expect(screen.getByTestId("claim-consent_id")).toHaveTextContent("csn_xyz");
    expect(screen.getByTestId("claim-granted_at")).toHaveTextContent("2026-05-14T10:00:00Z");
    expect(screen.getByTestId("claim-locale")).toHaveTextContent("pt-BR");
    expect(screen.getByTestId("claim-jti")).toHaveTextContent("jti-1");
    expect(screen.getByTestId("claim-exp")).toHaveTextContent("1900000000");
  });
});

describe("WithdrawForm — MFA required", () => {
  it("blocks submit until Clerk MFA verifies, then calls api.withdraw", async () => {
    const state: MockApiState = {
      active: [],
      historyRows: [],
      historyCalls: [],
      grantCalls: [],
      withdrawCalls: [],
    };
    const clerkClient = {
      session: {
        startVerification: vi.fn(async () => ({ verified: true })),
      },
    };
    render(
      <WithdrawForm consentId="csn_42" api={makeMockApi(state)} clerkClient={clerkClient} />,
    );

    // Try submitting without MFA first.
    fireEvent.click(screen.getByTestId("withdraw-submit"));
    // Button is disabled, so the click is a no-op; verify by attribute.
    expect(screen.getByTestId("withdraw-submit")).toBeDisabled();
    expect(state.withdrawCalls).toHaveLength(0);

    // Verify MFA path.
    fireEvent.click(screen.getByTestId("mfa-button"));
    await waitFor(() => expect(clerkClient.session.startVerification).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(screen.getByTestId("withdraw-submit")).not.toBeDisabled());

    fireEvent.change(screen.getByTestId("withdraw-reason"), {
      target: { value: "no longer needed" },
    });
    fireEvent.click(screen.getByTestId("withdraw-submit"));
    await waitFor(() => expect(state.withdrawCalls).toHaveLength(1));
    expect(state.withdrawCalls[0]).toEqual({ id: "csn_42", reason: "no longer needed" });
  });
});

describe("ConsentHistory — endpoint + pagination", () => {
  it("calls /v1/consent/history with the correct page + page_size", async () => {
    const state: MockApiState = {
      active: [],
      historyRows: [
        { id: "csn_1", purpose: "billing", granted_at: "2026-04-01", status: "active" },
        { id: "csn_2", purpose: "analytics", granted_at: "2026-04-02", status: "withdrawn" },
      ],
      historyCalls: [],
      grantCalls: [],
      withdrawCalls: [],
    };
    render(<ConsentHistory api={makeMockApi(state)} />);
    await waitFor(() => expect(state.historyCalls.length).toBeGreaterThan(0));
    expect(state.historyCalls[0]).toMatchObject({ page: 1, page_size: 20 });

    // Advance to page 2.
    await waitFor(() => expect(screen.getByTestId("page-next")).not.toBeDisabled());
    fireEvent.click(screen.getByTestId("page-next"));
    await waitFor(() => expect(state.historyCalls.length).toBeGreaterThan(1));
    const lastCall = state.historyCalls[state.historyCalls.length - 1] as { page: number };
    expect(lastCall.page).toBe(2);
  });
});

describe("ConsentHistory — filter state preserved across pages", () => {
  it("keeps purpose + status filters when paginating", async () => {
    const state: MockApiState = {
      active: [],
      historyRows: [
        { id: "csn_1", purpose: "billing", granted_at: "2026-04-01", status: "active" },
      ],
      historyCalls: [],
      grantCalls: [],
      withdrawCalls: [],
    };
    render(<ConsentHistory api={makeMockApi(state)} />);
    await waitFor(() => expect(state.historyCalls.length).toBeGreaterThan(0));

    fireEvent.change(screen.getByTestId("filter-status"), { target: { value: "active" } });
    fireEvent.change(screen.getByTestId("filter-purpose"), { target: { value: "billing" } });

    await waitFor(() => {
      const last = state.historyCalls[state.historyCalls.length - 1] as Record<string, unknown>;
      expect(last.status).toBe("active");
      expect(last.purpose).toBe("billing");
    });

    // Now advance one page; status + purpose must remain in the query.
    await waitFor(() => expect(screen.getByTestId("page-next")).not.toBeDisabled());
    fireEvent.click(screen.getByTestId("page-next"));
    await waitFor(() => {
      const last = state.historyCalls[state.historyCalls.length - 1] as Record<string, unknown>;
      expect(last.page).toBe(2);
      expect(last.status).toBe("active");
      expect(last.purpose).toBe("billing");
    });
  });
});

describe("CTRL-PRIV-001 — no PII in console.log", () => {
  it("never emits email/tenant_id strings through safeLog during a full capture", async () => {
    stubScreenshot([NOTICE_TEXT]);
    const state: MockApiState = {
      active: [],
      historyRows: [],
      historyCalls: [],
      grantCalls: [],
      withdrawCalls: [],
    };
    const logSpy = vi.spyOn(console, "info").mockImplementation(() => {});
    const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    const logFn = vi.spyOn(console, "log").mockImplementation(() => {});

    render(
      <ConsentCaptureFlow
        locale={LOCALE_PT}
        noticeText={NOTICE_TEXT}
        noticeVersion={NOTICE_VERSION}
        thirdParties={["Cloudflare"]}
        api={makeMockApi(state)}
      />,
    );
    fireEvent.change(screen.getByTestId("field-purpose"), { target: { value: "Analytics" } });
    fireEvent.click(screen.getByTestId("cat-usage"));
    fireEvent.click(screen.getByTestId("to-scroll-step"));
    await act(async () => {
      window.dispatchEvent(new Event("scroll"));
    });
    await waitFor(() => expect(screen.getByTestId("step-consent")).toBeInTheDocument());
    fireEvent.click(screen.getByTestId("consent-submit"));
    await waitFor(() => expect(state.grantCalls).toHaveLength(1));

    const collected = [logSpy, warnSpy, errorSpy, logFn]
      .flatMap((s) => s.mock.calls.flatMap((c) => c.map(String)))
      .join(" ");
    expect(collected).not.toMatch(/[a-z0-9._%+-]+@[a-z0-9.-]+\.[a-z]{2,}/i);
    expect(collected).not.toMatch(/tnt_[a-z0-9_-]+/i);
  });
});

describe("ConsentDashboard", () => {
  it("renders active consent rows and exposes view + withdraw actions", async () => {
    const state: MockApiState = {
      active: [
        { id: "csn_a", purpose: "analytics", granted_at: "2026-04-01", status: "active" },
        { id: "csn_b", purpose: "billing", granted_at: "2026-04-02", status: "active" },
      ],
      historyRows: [],
      historyCalls: [],
      grantCalls: [],
      withdrawCalls: [],
    };
    render(<ConsentDashboard api={makeMockApi(state)} />);
    await waitFor(() => expect(screen.getByTestId("row-csn_a")).toBeInTheDocument());
    expect(screen.getByLabelText("Withdraw consent csn_a")).toHaveAttribute(
      "href",
      "/corelink/consent/withdraw/csn_a",
    );
    expect(screen.getByLabelText("View consent csn_b")).toHaveAttribute("href", "/corelink/consent/csn_b");
  });
});
