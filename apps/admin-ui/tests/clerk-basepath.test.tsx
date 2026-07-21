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
