import { describe, it, expect, vi } from "vitest";
import userEvent from "@testing-library/user-event";
import { renderWithProviders, screen } from "@/test-utils/render";
import { axe } from "@/test-utils/axe";
import { Table } from "./Table";

interface Row {
  id: string;
  name: string;
  region: string;
}

const rows: Row[] = [
  { id: "1", name: "Cloudflare", region: "Global" },
  { id: "2", name: "Clerk", region: "US" },
];

const columns = [
  { key: "name" as const, header: "Name", sortable: true },
  { key: "region" as const, header: "Region" },
];

describe("Table", () => {
  it("renders semantic table with caption", () => {
    renderWithProviders(<Table caption="Sub-processors" columns={columns} rows={rows} />);
    expect(screen.getByRole("table", { name: "Sub-processors" })).toBeInTheDocument();
    expect(screen.getByRole("columnheader", { name: /Name/ })).toBeInTheDocument();
  });

  it("emits sort change on header click", async () => {
    const onSort = vi.fn();
    renderWithProviders(
      <Table caption="t" columns={columns} rows={rows} onSortChange={onSort} />
    );
    await userEvent.click(screen.getByRole("button", { name: /Name/ }));
    expect(onSort).toHaveBeenCalledWith("name", "ascending");
  });

  it("supports row selection with select-all indeterminate", async () => {
    const onChange = vi.fn();
    renderWithProviders(
      <Table
        caption="t"
        columns={columns}
        rows={rows}
        selectable
        selectedIds={new Set(["1"])}
        onSelectChange={onChange}
      />
    );
    const selectAll = screen.getByLabelText("Select all rows") as HTMLInputElement;
    expect(selectAll.indeterminate).toBe(true);
  });

  it("has no a11y violations", async () => {
    const { container } = renderWithProviders(
      <Table caption="t" columns={columns} rows={rows} />
    );
    expect(await axe(container)).toHaveNoViolations();
  });
});
