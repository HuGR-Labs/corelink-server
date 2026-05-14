"use client";

import * as React from "react";
import { Slot } from "@radix-ui/react-slot";
import { cn } from "@/lib/cn";

export type ButtonVariant = "primary" | "secondary" | "ghost" | "danger";
export type ButtonSize = "sm" | "md" | "lg";

export interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
  size?: ButtonSize;
  asChild?: boolean;
}

const variantClass: Record<ButtonVariant, string> = {
  primary:
    "bg-blue-700 text-white hover:bg-blue-800 focus-visible:ring-blue-900 disabled:bg-slate-300 disabled:text-slate-600",
  secondary:
    "bg-white text-slate-900 border border-slate-300 hover:bg-slate-50 focus-visible:ring-slate-700",
  ghost:
    "bg-transparent text-slate-900 hover:bg-slate-100 focus-visible:ring-slate-700",
  danger:
    "bg-red-700 text-white hover:bg-red-800 focus-visible:ring-red-900",
};

const sizeClass: Record<ButtonSize, string> = {
  // WCAG 2.5.8: minimum target 24x24 px; default 40px height.
  sm: "min-h-[32px] px-3 text-sm",
  md: "min-h-[40px] px-4 text-base",
  lg: "min-h-[48px] px-5 text-lg",
};

export const Button = React.forwardRef<HTMLButtonElement, ButtonProps>(function Button(
  { variant = "primary", size = "md", asChild = false, className, disabled, type, ...props },
  ref
) {
  const Comp: React.ElementType = asChild ? Slot : "button";
  return (
    <Comp
      ref={ref}
      type={asChild ? undefined : type ?? "button"}
      aria-disabled={disabled || undefined}
      disabled={asChild ? undefined : disabled}
      className={cn(
        "inline-flex items-center justify-center rounded-md font-medium",
        "transition-colors",
        "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-offset-2",
        "disabled:cursor-not-allowed disabled:opacity-60",
        variantClass[variant],
        sizeClass[size],
        className
      )}
      {...props}
    />
  );
});
