"use client";

import * as React from "react";
import * as RadixTabs from "@radix-ui/react-tabs";
import { cn } from "@/lib/cn";

export interface TabItem {
  value: string;
  label: string;
  content: React.ReactNode;
  disabled?: boolean;
}

export interface TabsProps {
  label: string;
  items: TabItem[];
  defaultValue?: string;
  value?: string;
  onValueChange?: (value: string) => void;
  orientation?: "horizontal" | "vertical";
}

export function Tabs({
  label,
  items,
  defaultValue,
  value,
  onValueChange,
  orientation = "horizontal",
}: TabsProps) {
  return (
    <RadixTabs.Root
      value={value}
      defaultValue={defaultValue ?? items[0]?.value}
      onValueChange={onValueChange}
      orientation={orientation}
      className="flex flex-col gap-2"
    >
      <RadixTabs.List
        aria-label={label}
        className={cn(
          "flex border-b border-slate-200",
          orientation === "vertical" && "flex-col border-b-0 border-r"
        )}
      >
        {items.map((it) => (
          <RadixTabs.Trigger
            key={it.value}
            value={it.value}
            disabled={it.disabled}
            className={cn(
              "min-h-[40px] px-4 text-sm font-medium text-slate-700",
              "data-[state=active]:border-b-2 data-[state=active]:border-blue-700 data-[state=active]:text-blue-900",
              "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-700",
              "disabled:cursor-not-allowed disabled:opacity-60"
            )}
          >
            {it.label}
          </RadixTabs.Trigger>
        ))}
      </RadixTabs.List>
      {items.map((it) => (
        <RadixTabs.Content key={it.value} value={it.value} className="p-2">
          {it.content}
        </RadixTabs.Content>
      ))}
    </RadixTabs.Root>
  );
}
