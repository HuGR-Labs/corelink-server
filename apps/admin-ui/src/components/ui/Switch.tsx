"use client";

import * as React from "react";
import * as RadixSwitch from "@radix-ui/react-switch";
import { cn } from "@/lib/cn";

export interface SwitchProps {
  label: string;
  checked?: boolean;
  defaultChecked?: boolean;
  onCheckedChange?: (checked: boolean) => void;
  id?: string;
  disabled?: boolean;
}

export function Switch({
  label,
  checked,
  defaultChecked,
  onCheckedChange,
  id,
  disabled,
}: SwitchProps) {
  const reactId = React.useId();
  const swId = id ?? `sw-${reactId}`;
  const labelId = `${swId}-label`;
  return (
    <div className="flex items-center gap-2">
      <RadixSwitch.Root
        id={swId}
        aria-labelledby={labelId}
        checked={checked}
        defaultChecked={defaultChecked}
        onCheckedChange={onCheckedChange}
        disabled={disabled}
        className={cn(
          "relative inline-flex h-6 w-10 cursor-pointer items-center rounded-full bg-slate-300",
          "data-[state=checked]:bg-blue-700",
          "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-700 focus-visible:ring-offset-2",
          "disabled:cursor-not-allowed disabled:opacity-60"
        )}
      >
        <RadixSwitch.Thumb
          className={cn(
            "block h-5 w-5 translate-x-0.5 rounded-full bg-white shadow",
            "transition-transform",
            "data-[state=checked]:translate-x-[18px]"
          )}
        />
      </RadixSwitch.Root>
      <label id={labelId} htmlFor={swId} className="cursor-pointer text-sm text-slate-900">
        {label}
      </label>
    </div>
  );
}
