import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, act } from "@testing-library/react";
import { PatModal } from "@/components/onboarding/PatModal";
import {
  setPlaintextPat,
  getPlaintextPat,
  clearPlaintextPat,
} from "@/lib/onboarding-state";

const labels = {
  title: "Save this token now",
  warning: "Shown only once. Cannot be retrieved.",
  copy: "Copy",
  confirmSaved: "I saved it",
  finish: "Continue",
};

describe("PatModal lifecycle", () => {
  beforeEach(() => {
    clearPlaintextPat();
  });

  it("renders nothing when closed", () => {
    const { container } = render(
      <PatModal open={false} onConfirm={() => {}} labels={labels} />,
    );
    expect(container.firstChild).toBeNull();
  });

  it("shows warning and disables finish until copied + confirmed", () => {
    setPlaintextPat("corelink_prod_xyz_abc");
    render(<PatModal open onConfirm={() => {}} labels={labels} />);
    expect(screen.getByTestId("pat-warning").textContent).toContain(
      "only once",
    );
    const finish = screen.getByTestId("pat-finish") as HTMLButtonElement;
    expect(finish.disabled).toBe(true);
  });

  it("clears the in-memory PAT after confirm", async () => {
    setPlaintextPat("corelink_prod_xyz_abc");
    const onConfirm = vi.fn();
    const copyImpl = vi.fn().mockResolvedValue(undefined);
    render(
      <PatModal
        open
        onConfirm={onConfirm}
        labels={labels}
        copyImpl={copyImpl}
      />,
    );

    await act(async () => {
      fireEvent.click(screen.getByTestId("pat-copy"));
      // Wait for the async copy + state update to flush.
      await Promise.resolve();
      await Promise.resolve();
    });
    fireEvent.click(screen.getByTestId("pat-confirm-checkbox"));

    const finish = screen.getByTestId("pat-finish") as HTMLButtonElement;
    expect(finish.disabled).toBe(false);
    fireEvent.click(finish);

    expect(onConfirm).toHaveBeenCalledOnce();
    expect(getPlaintextPat()).toBeNull();
    expect(copyImpl).toHaveBeenCalledWith("corelink_prod_xyz_abc");
  });
});
