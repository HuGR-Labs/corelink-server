import { describe, it, expect, vi, afterEach } from "vitest";
import userEvent from "@testing-library/user-event";
import { renderWithProviders, screen } from "@/test-utils/render";
import { axe } from "@/test-utils/axe";
import { LocaleSwitcher } from "./LocaleSwitcher";
import { LOCALE_COOKIE } from "@/i18n/LocaleContext";

describe("LocaleSwitcher", () => {
  afterEach(() => {
    // Clear cookies.
    document.cookie = `${LOCALE_COOKIE}=; Path=/; Max-Age=0`;
  });

  it("renders with current locale", () => {
    renderWithProviders(<LocaleSwitcher />);
    expect(screen.getByRole("combobox", { name: "Language" })).toBeInTheDocument();
  });

  it("persists choice to cookie and emits change", async () => {
    const onChange = vi.fn();
    renderWithProviders(<LocaleSwitcher onChange={onChange} />);
    const trigger = screen.getByRole("combobox", { name: "Language" });
    await userEvent.click(trigger);
    const option = await screen.findByRole("option", { name: "Português" });
    await userEvent.click(option);
    expect(onChange).toHaveBeenCalledWith("pt");
    expect(document.cookie).toContain(`${LOCALE_COOKIE}=pt`);
  });

  it("has no a11y violations", async () => {
    const { container } = renderWithProviders(<LocaleSwitcher />);
    expect(await axe(container)).toHaveNoViolations();
  });
});
