"use client";

import * as React from "react";
import * as RadixSelect from "@radix-ui/react-select";
import { cn } from "@/lib/cn";

export interface SelectOption {
  value: string;
  label: string;
  disabled?: boolean;
}

export interface SelectProps {
  label: string;
  value?: string;
  defaultValue?: string;
  onValueChange?: (value: string) => void;
  options: SelectOption[];
  placeholder?: string;
  id?: string;
  /** Hide the visual label but keep it for screen readers. */
  hideLabel?: boolean;
  disabled?: boolean;
}

export function Select({
  label,
  value,
  defaultValue,
  onValueChange,
  options,
  placeholder = "Select…",
  id,
  hideLabel,
  disabled,
}: SelectProps) {
  const reactId = React.useId();
  const triggerId = id ?? `select-${reactId}`;
  const labelId = `${triggerId}-label`;
  return (
    <div className="flex flex-col gap-1">
      <label
        id={labelId}
        htmlFor={triggerId}
        className={cn("text-sm font-medium text-slate-900", hideLabel && "sr-only")}
      >
        {label}
      </label>
      <RadixSelect.Root
        value={value}
        defaultValue={defaultValue}
        onValueChange={onValueChange}
        disabled={disabled}
      >
        <RadixSelect.Trigger
          id={triggerId}
          aria-labelledby={labelId}
          className={cn(
            "inline-flex min-h-[40px] items-center justify-between rounded-md border border-slate-300 bg-white px-3 text-base",
            "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-700 focus-visible:ring-offset-2",
            "disabled:cursor-not-allowed disabled:opacity-60"
          )}
        >
          <RadixSelect.Value placeholder={placeholder} />
          <RadixSelect.Icon aria-hidden="true">▼</RadixSelect.Icon>
        </RadixSelect.Trigger>
        <RadixSelect.Portal>
          <RadixSelect.Content className="overflow-hidden rounded-md border border-slate-200 bg-white shadow-lg">
            <RadixSelect.Viewport className="p-1">
              {options.map((opt) => (
                <RadixSelect.Item
                  key={opt.value}
                  value={opt.value}
                  disabled={opt.disabled}
                  className={cn(
                    "relative flex min-h-[36px] cursor-pointer select-none items-center rounded px-3 text-base",
                    "data-[highlighted]:bg-blue-100 data-[highlighted]:outline-none",
                    "data-[disabled]:cursor-not-allowed data-[disabled]:opacity-50"
                  )}
                >
                  <RadixSelect.ItemText>{opt.label}</RadixSelect.ItemText>
                </RadixSelect.Item>
              ))}
            </RadixSelect.Viewport>
          </RadixSelect.Content>
        </RadixSelect.Portal>
      </RadixSelect.Root>
    </div>
  );
}
