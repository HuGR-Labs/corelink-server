"use client";

import * as React from "react";
import * as RadixCheckbox from "@radix-ui/react-checkbox";
import { cn } from "@/lib/cn";

export interface CheckboxProps {
  label: string;
  checked?: boolean | "indeterminate";
  defaultChecked?: boolean | "indeterminate";
  onCheckedChange?: (value: boolean | "indeterminate") => void;
  id?: string;
  name?: string;
  disabled?: boolean;
}

export function Checkbox({
  label,
  checked,
  defaultChecked,
  onCheckedChange,
  id,
  name,
  disabled,
}: CheckboxProps) {
  const reactId = React.useId();
  const cbId = id ?? `cb-${reactId}`;
  const labelId = `${cbId}-label`;
  return (
    <div className="flex items-center gap-2">
      <RadixCheckbox.Root
        id={cbId}
        aria-labelledby={labelId}
        name={name}
        checked={checked}
        defaultChecked={defaultChecked}
        onCheckedChange={onCheckedChange}
        disabled={disabled}
        className={cn(
          "inline-flex h-6 w-6 items-center justify-center rounded border-2 border-slate-700 bg-white",
          "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-700 focus-visible:ring-offset-2",
          "data-[state=checked]:bg-blue-700 data-[state=indeterminate]:bg-blue-700",
          "disabled:cursor-not-allowed disabled:opacity-60"
        )}
      >
        <RadixCheckbox.Indicator className="text-white">
          {checked === "indeterminate" ? "–" : "✓"}
        </RadixCheckbox.Indicator>
      </RadixCheckbox.Root>
      <label id={labelId} htmlFor={cbId} className="cursor-pointer text-sm text-slate-900">
        {label}
      </label>
    </div>
  );
}
