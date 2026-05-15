/**
 * Page Object — sign-in flow (wt-r3-7).
 *
 * In CoreLink admin-ui Clerk is the SSO provider. The dev / e2e variant
 * uses a cookie-based test-mode session (see fixtures/users.ts) — the
 * page object hides the Clerk-vs-stub distinction so specs read like a
 * real login.
 */
import type { BrowserContext, Page } from "@playwright/test";
import { expect } from "@playwright/test";
import { type E2EUser, signInAs, signOut, USERS } from "../fixtures/users";

export class LoginPage {
  constructor(
    readonly page: Page,
    readonly context: BrowserContext,
    readonly baseURL: string,
  ) {}

  /** Visit the public sign-in route — used to assert the Clerk SDK or its
   *  test fallback renders without leaking session state. */
  async visit(): Promise<void> {
    await this.page.goto("/sign-in");
    await expect(this.page.locator("h1")).toBeVisible();
  }

  /** Install the synthetic Clerk session for `role` and return the user. */
  async signInAs(role: keyof typeof USERS): Promise<E2EUser> {
    const user = USERS[role];
    await signInAs(this.context, user, this.baseURL);
    return user;
  }

  /** Clear the synthetic session (logout). */
  async signOut(): Promise<void> {
    await signOut(this.context);
  }

  /** Assert that a session cookie is present in the browser context. */
  async expectSessionCookie(): Promise<void> {
    const cookies = await this.context.cookies(this.baseURL);
    const session = cookies.find((c) => c.name === "__corelink_e2e_session");
    expect(session, "expected __corelink_e2e_session cookie to be set").toBeDefined();
    expect(session?.value).not.toEqual("");
  }

  /** Assert that no session cookie is present. */
  async expectNoSessionCookie(): Promise<void> {
    const cookies = await this.context.cookies(this.baseURL);
    const session = cookies.find((c) => c.name === "__corelink_e2e_session");
    expect(session, "expected no __corelink_e2e_session cookie").toBeUndefined();
  }
}
