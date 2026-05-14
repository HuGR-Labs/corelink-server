"use client";

import * as React from "react";
import * as RadixRadioGroup from "@radix-ui/react-radio-group";
import { cn } from "@/lib/cn";

export interface RadioOption {
  value: string;
  label: string;
  disabled?: boolean;
}

export interface RadioGroupProps {
  label: string;
  name?: string;
  value?: string;
  defaultValue?: string;
  onValueChange?: (value: string) => void;
  options: RadioOption[];
}

export function RadioGroup({
  label,
  name,
  value,
  defaultValue,
  onValueChange,
  options,
}: RadioGroupProps) {
  const reactId = React.useId();
  const groupId = `rg-${reactId}`;
  return (
    <fieldset className="flex flex-col gap-2">
      <legend id={groupId} className="text-sm font-medium text-slate-900">
        {label}
      </legend>
      <RadixRadioGroup.Root
        aria-labelledby={groupId}
        name={name}
        value={value}
        defaultValue={defaultValue}
        onValueChange={onValueChange}
        className="flex flex-col gap-1"
      >
        {options.map((opt) => {
          const id = `${groupId}-${opt.value}`;
          const labelId = `${id}-label`;
          return (
            <div key={opt.value} className="flex items-center gap-2">
              <RadixRadioGroup.Item
                id={id}
                value={opt.value}
                disabled={opt.disabled}
                aria-labelledby={labelId}
                className={cn(
                  "inline-flex h-6 w-6 items-center justify-center rounded-full border-2 border-slate-700 bg-white",
                  "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-700 focus-visible:ring-offset-2",
                  "disabled:cursor-not-allowed disabled:opacity-60"
                )}
              >
                <RadixRadioGroup.Indicator className="block h-3 w-3 rounded-full bg-blue-700" />
              </RadixRadioGroup.Item>
              <label id={labelId} htmlFor={id} className="cursor-pointer text-sm text-slate-900">
                {opt.label}
              </label>
            </div>
          );
        })}
      </RadixRadioGroup.Root>
    </fieldset>
  );
}
