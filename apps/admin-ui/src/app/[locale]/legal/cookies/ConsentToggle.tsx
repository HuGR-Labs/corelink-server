"use client";

import * as React from "react";
import * as RadixSwitch from "@radix-ui/react-switch";

/**
 * ConsentToggle — Linear-doctrine, dark-surface switch for the cookie-consent
 * page. Same Radix `role="switch"` + `aria-checked` + disabled semantics as the
 * shared `@/components/ui/Switch` (which is light-themed `text-slate-900` and
 * would fail contrast on the dark public shell), restyled with tokens only so
 * the label stays on `--t1` (WCAG-AA on `--bg`). Track/thumb use `--t*` opacity
 * tokens; the ON state uses white (`--t1`) — the doctrine's only accent.
 */
export interface ConsentToggleProps {
  label: string;
  checked?: boolean;
  onCheckedChange?: (checked: boolean) => void;
  id?: string;
  disabled?: boolean;
}

export function ConsentToggle({
  label,
  checked,
  onCheckedChange,
  id,
  disabled,
}: ConsentToggleProps): React.ReactElement {
  const reactId = React.useId();
  const swId = id ?? `sw-${reactId}`;
  const labelId = `${swId}-label`;
  return (
    <div className="flex items-center gap-3">
      <RadixSwitch.Root
        id={swId}
        aria-labelledby={labelId}
        checked={checked}
        onCheckedChange={onCheckedChange}
        disabled={disabled}
        className={[
          "relative inline-flex h-5 w-9 cursor-pointer items-center rounded-full",
          "border border-[var(--line-2)] bg-[rgba(255,255,255,0.06)]",
          "transition-colors data-[state=checked]:bg-[var(--t1)] data-[state=checked]:border-[var(--t1)]",
          "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--line-2)] focus-visible:ring-offset-2 focus-visible:ring-offset-[var(--bg)]",
          "disabled:cursor-not-allowed disabled:opacity-50",
        ].join(" ")}
      >
        <RadixSwitch.Thumb
          className={[
            "block h-3.5 w-3.5 translate-x-0.5 rounded-full bg-[var(--t2)] shadow",
            "transition-transform data-[state=checked]:translate-x-[18px] data-[state=checked]:bg-[var(--bg)]",
          ].join(" ")}
        />
      </RadixSwitch.Root>
      <label id={labelId} htmlFor={swId} className="cursor-pointer text-sm text-[var(--t1)]">
        {label}
      </label>
    </div>
  );
}

export default ConsentToggle;
