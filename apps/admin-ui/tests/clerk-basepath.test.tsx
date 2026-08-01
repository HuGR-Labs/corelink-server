/**
 * Regression lock for the 2026-07-21 prod sign-in/sign-up BLANK-PAGE outage.
 *
 * When the app moved behind Next's `basePath: "/corelink"` (f92da211), the
 * client-side Clerk `<SignIn>`/`<SignUp>` widgets kept rendering their mount
 * container but produced NO UI: Clerk uses `routing="path"` and matches its
 * configured path against `window.location.pathname` (which DOES carry the
 * basePath, `/corelink/sign-in`), but the components were given no `path`, so
 * Clerk's default (`/sign-in`, basePath-LESS) never matched and the widget
 * silently bailed. New users hit a blank page — the money-path entry was down,
 * yet the HTTP-200 checks stayed green.
 *
 * Next only auto-applies `basePath` to framework-generated links, never to the
 * URLs Clerk builds internally (same class the middleware fixes by hand — see
 * src/lib/route-matcher.ts). So the widgets must be told the basePath-prefixed
 * `path`/`signInUrl`/`signUpUrl` explicitly. This test captures the props each
 * widget receives and asserts they carry `APP_BASE_PATH`.
 */
import * as React from "react";
import { describe, it, expect, beforeEach, vi } from "vitest";
import { render } from "@testing-library/react";

import { APP_BASE_PATH } from "@/lib/route-matcher";

const captured: {
  provider?: Record<string, unknown>;
  signIn?: Record<string, unknown>;
  signUp?: Record<string, unknown>;
} = {};

vi.mock("@clerk/nextjs", () => ({
  ClerkProvider: (props: { children?: React.ReactNode } & Record<string, unknown>) => {
    captured.provider = props;
    return React.createElement(React.Fragment, null, props.children);
  },
  SignIn: (props: Record<string, unknown>) => {
    captured.signIn = props;
    return React.createElement("div", { "data-testid": "clerk-signin" });
  },
  SignUp: (props: Record<string, unknown>) => {
    captured.signUp = props;
    return React.createElement("div", { "data-testid": "clerk-signup" });
  },
}));

// Imported AFTER the mock so the widgets resolve to the prop-capturing stubs.
import ClerkSignIn from "@/app/sign-in/[[...sign-in]]/ClerkSignIn";
import ClerkSignUp from "@/app/sign-up/[[...sign-up]]/ClerkSignUp";

beforeEach(() => {
  captured.provider = undefined;
  captured.signIn = undefined;
  captured.signUp = undefined;
});

describe("Clerk auth widgets are basePath-aware (blank-page outage regression)", () => {
  it("mounts <SignIn> under the app basePath with path routing", () => {
    render(React.createElement(ClerkSignIn));
    expect(captured.signIn?.["path"]).toBe(`${APP_BASE_PATH}/sign-in`);
    expect(captured.signIn?.["routing"]).toBe("path");
    // Cross-links ("Sign up instead") must point at OUR page, not Clerk's
    // hosted Account Portal on the retired domain.
    expect(captured.provider?.["signInUrl"]).toBe(`${APP_BASE_PATH}/sign-in`);
    expect(captured.provider?.["signUpUrl"]).toBe(`${APP_BASE_PATH}/sign-up`);
  });

  it("mounts <SignUp> under the app basePath with path routing", () => {
    render(React.createElement(ClerkSignUp));
    expect(captured.signUp?.["path"]).toBe(`${APP_BASE_PATH}/sign-up`);
    expect(captured.signUp?.["routing"]).toBe("path");
    expect(captured.provider?.["signInUrl"]).toBe(`${APP_BASE_PATH}/sign-in`);
    expect(captured.provider?.["signUpUrl"]).toBe(`${APP_BASE_PATH}/sign-up`);
  });

  it("both widgets agree the basePath is non-empty (else path routing regresses)", () => {
    expect(APP_BASE_PATH).toBe("/corelink");
    expect(APP_BASE_PATH.length).toBeGreaterThan(0);
  });
});

describe("sign-in honours ?redirect_url= (upgrade-funnel round-trip regression)", () => {
  /**
   * The signed-out buyer funnel depends on Clerk's redirect_url precedence:
   * /upgrade?plan=<tier> redirects to /sign-in?redirect_url=/<locale>/upgrade
   * ([locale]/upgrade/page.tsx) and the visitor must land BACK there after
   * sign-in so checkout auto-fires. `forceRedirectUrl` unconditionally
   * overrides the redirect_url query param (Clerk docs + clerk-js
   * buildAfterSignInUrl), so <SignIn> must only ever set the FALLBACK —
   * force strands the buyer on the dashboard and drops the ?plan intent.
   * (Introduced by 9f2fac50 (#270); fixed here.)
   */
  it("<SignIn> sets fallbackRedirectUrl but NEVER forceRedirectUrl", () => {
    render(React.createElement(ClerkSignIn));
    expect(captured.signIn?.["forceRedirectUrl"]).toBeUndefined();
    expect(captured.signIn?.["fallbackRedirectUrl"]).toBe(`${APP_BASE_PATH}/en/customer`);
    // The provider-level option would beat ?redirect_url= through the same
    // precedence chain (RedirectUrls#getRedirectUrl: force > query > fallback)
    // — pin its absence too, or the widget-prop lock above can be bypassed.
    expect(captured.provider?.["signInForceRedirectUrl"]).toBeUndefined();
  });

  it("<SignUp> keeps forceRedirectUrl to /en/welcome (DPA-first onboarding, BY DESIGN)", () => {
    // Sign-UP intentionally forces /en/welcome: the one-time PAT reveal +
    // DPA-first ordering must precede any tier-select (the backend 403s the
    // checkout until the DPA is accepted) — see [locale]/upgrade/page.tsx.
    render(React.createElement(ClerkSignUp));
    expect(captured.signUp?.["forceRedirectUrl"]).toBe(`${APP_BASE_PATH}/en/welcome`);
  });
});

describe("after-auth redirect targets carry the basePath (marketing-site bounce)", () => {
  /**
   * PROVEN against live prod with a throwaway Clerk user: a plain sign-in
   * landed on `humangr.com/en/customer` — basePath LOST — which is not the
   * app but the hugr-site marketing landing. Clerk's after-auth navigation
   * does not go through Next's router, and the router is the only thing that
   * auto-applies basePath, so every redirect target handed to Clerk must
   * carry `/corelink` explicitly. A bare `/en/...` here means "user signs in
   * and gets bounced out of the app" — and for <SignUp> it also means the
   * one-time PAT reveal + DPA onboarding is silently skipped.
   *
   * Prefixing is safe under BOTH navigation mechanisms: Clerk's own
   * `removeBasePath` strips it before any `router.push` (Next re-adds it),
   * and a hard `window.location` navigation gets the correct absolute path.
   */
  it("<SignIn> fallbackRedirectUrl is basePath-prefixed", () => {
    render(React.createElement(ClerkSignIn));
    const target = captured.signIn?.["fallbackRedirectUrl"] as string;
    expect(target.startsWith(`${APP_BASE_PATH}/`)).toBe(true);
    expect(target).toBe(`${APP_BASE_PATH}/en/customer`);
  });

  it("<SignUp> force AND fallback redirect targets are basePath-prefixed", () => {
    render(React.createElement(ClerkSignUp));
    for (const key of ["forceRedirectUrl", "fallbackRedirectUrl"]) {
      const target = captured.signUp?.[key] as string;
      expect(target.startsWith(`${APP_BASE_PATH}/`)).toBe(true);
    }
  });

  it("no redirect target handed to Clerk is a bare locale path", () => {
    // Guard for the CLASS, not the two known sites: any future
    // `/en/...`-shaped redirect prop would escape the app the same way.
    render(React.createElement(ClerkSignIn));
    render(React.createElement(ClerkSignUp));
    const targets = [
      captured.signIn?.["fallbackRedirectUrl"],
      captured.signIn?.["forceRedirectUrl"],
      captured.signUp?.["fallbackRedirectUrl"],
      captured.signUp?.["forceRedirectUrl"],
    ].filter((v): v is string => typeof v === "string");
    expect(targets.length).toBeGreaterThan(0);
    for (const t of targets) {
      expect(t).not.toMatch(/^\/[a-z]{2}\//); // e.g. "/en/..." with no basePath
    }
  });
});
