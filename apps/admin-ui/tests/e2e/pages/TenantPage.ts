/**
 * Page Object — /[locale]/admin/tenants + tenant deep-dive (wt-r3-7).
 */
import type { Locator, Page } from "@playwright/test";
import { expect } from "@playwright/test";

export class TenantPage {
  readonly search: Locator;
  readonly table: Locator;
  readonly rows: Locator;

  constructor(readonly page: Page) {
    this.search = page.getByTestId("tenant-search-input");
    this.table = page.getByTestId("tenant-table");
    this.rows = page.locator("[data-testid^='tenant-row-']");
  }

  async visit(locale = "en"): Promise<void> {
    await this.page.goto(`/${locale}/admin/tenants`);
    // Dev-server cold compile can stall the first request in CI.
    await expect(this.page.locator("h1#tenants-heading")).toBeVisible({ timeout: 30_000 });
  }

  async expectListLoaded(): Promise<void> {
    await expect(this.table).toBeVisible();
    await expect
      .poll(async () => this.rows.count(), { timeout: 30_000, intervals: [500] })
      .toBeGreaterThan(0);
  }

  async searchFor(text: string): Promise<void> {
    await this.search.fill(text);
    // Debounce is 200ms in TenantSearch.tsx.
    await this.page.waitForTimeout(350);
  }

  async expectRowCount(min: number, max?: number): Promise<void> {
    const count = await this.rows.count();
    expect(count).toBeGreaterThanOrEqual(min);
    if (max !== undefined) expect(count).toBeLessThanOrEqual(max);
  }

  async clickTenant(tenantId: string): Promise<void> {
    const row = this.page.getByTestId(`tenant-row-${tenantId}`);
    await expect(row).toBeVisible();
    await row.getByRole("link", { name: /view/i }).click();
  }

  async expectDeepDive(tenantId: string): Promise<void> {
    await expect(this.page.getByTestId("tenant-deep-dive")).toBeVisible();
    await expect(this.page.locator("h1")).toContainText(tenantId);
  }
}
