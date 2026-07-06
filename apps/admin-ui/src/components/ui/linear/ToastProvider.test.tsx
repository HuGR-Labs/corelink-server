import { describe, it, expect } from "vitest";
import userEvent from "@testing-library/user-event";
import { renderWithProviders, screen } from "@/test-utils/render";
import { ToastProvider, useToast } from "./ToastProvider";

function Trigger() {
  const { toast } = useToast();
  return <button onClick={() => toast({ title: "Saved!" })}>fire</button>;
}

describe("ToastProvider / useToast", () => {
  it("renders children", () => {
    renderWithProviders(
      <ToastProvider>
        <span>app content</span>
      </ToastProvider>,
    );
    expect(screen.getByText("app content")).toBeInTheDocument();
  });

  it("renders a toast when toast() is called", async () => {
    renderWithProviders(
      <ToastProvider>
        <Trigger />
      </ToastProvider>,
    );
    await userEvent.click(screen.getByRole("button", { name: "fire" }));
    expect(screen.getByRole("status")).toHaveTextContent("Saved!");
  });
});
