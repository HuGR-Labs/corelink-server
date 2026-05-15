/**
 * Page Object — /[locale]/admin/audit (wt-r3-7).
 */
import type { Locator, Page } from "@playwright/test";
import { expect } from "@playwright/test";

export class AuditPage {
  readonly table: Locator;
  readonly rows: Locator;
  readonly sinceFilter: Locator;
  readonly drawer: Locator;

  constructor(readonly page: Page) {
    this.table = page.getByTestId("audit-table");
    this.rows = page.locator("[data-testid^='audit-row-']");
    this.sinceFilter = page.getByTestId("filter-since");
    this.drawer = page.getByTestId("audit-event-drawer");
  }

  async visit(locale = "en"): Promise<void> {
    await this.page.goto(`/${locale}/admin/audit`);
    await expect(this.page.locator("h1#audit-heading")).toBeVisible({ timeout: 30_000 });
  }

  async expectTableLoaded(minRows = 1): Promise<void> {
    await expect(this.table).toBeVisible();
    // The viewer hydrates and fires its first fetch on mount; give it up to
    // 30 s (dev-server cold-start in CI can stall on first webpack compile).
    await expect
      .poll(async () => this.rows.count(), { timeout: 30_000, intervals: [500] })
      .toBeGreaterThanOrEqual(minRows);
  }

  async filterSince(isoDate: string): Promise<void> {
    // The viewer re-fetches when the filter shape (excluding cursor) changes.
    // We wait for the matching /v1/admin/audit GET that carries `since=` so the
    // assertion that follows isn't racing the previous render.
    const fetchPromise = this.page.waitForResponse(
      (r) =>
        r.url().includes("/v1/admin/audit") &&
        r.url().includes("since=") &&
        r.status() === 200,
      { timeout: 15_000 },
    );
    await this.sinceFilter.fill(isoDate);
    await fetchPromise.catch(() => undefined);
    await this.page.waitForLoadState("networkidle").catch(() => undefined);
  }

  async toggleEventType(eventType: string): Promise<void> {
    const cb = this.page.getByTestId(`filter-event-${eventType}`);
    await cb.click();
    await this.page.waitForLoadState("networkidle");
  }

  async openRow(eventId: string): Promise<void> {
    await this.page.getByTestId(`audit-row-${eventId}`).click();
    await expect(this.drawer).toBeVisible();
  }

  async expectDrawerEvent(eventId: string): Promise<void> {
    await expect(this.drawer).toBeVisible();
    await expect(this.drawer).toContainText(eventId);
  }

  async expectAuditRowFor(predicate: string | RegExp): Promise<void> {
    const text = typeof predicate === "string" ? new RegExp(predicate, "i") : predicate;
    await expect(this.table).toContainText(text);
  }
}
