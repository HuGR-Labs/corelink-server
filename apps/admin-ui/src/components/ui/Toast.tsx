"use client";

import * as React from "react";
import * as RadixToast from "@radix-ui/react-toast";
import { cn } from "@/lib/cn";

export type ToastSeverity = "info" | "success" | "warning" | "error";

export interface ToastProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: string;
  description?: string;
  severity?: ToastSeverity;
  /** ms before auto-dismiss; 0 disables. Default 5000. */
  duration?: number;
}

const severityClass: Record<ToastSeverity, string> = {
  info: "bg-blue-50 border-blue-700 text-blue-900",
  success: "bg-green-50 border-green-700 text-green-900",
  warning: "bg-amber-50 border-amber-700 text-amber-900",
  error: "bg-red-50 border-red-700 text-red-900",
};

export function Toast({
  open,
  onOpenChange,
  title,
  description,
  severity = "info",
  duration = 5000,
}: ToastProps) {
  return (
    <RadixToast.Provider swipeDirection="right" duration={duration}>
      <RadixToast.Root
        open={open}
        onOpenChange={onOpenChange}
        type={severity === "error" || severity === "warning" ? "foreground" : "background"}
        className={cn(
          "rounded border-l-4 p-3 shadow-md",
          severityClass[severity]
        )}
      >
        <RadixToast.Title className="font-semibold">{title}</RadixToast.Title>
        {description && <RadixToast.Description>{description}</RadixToast.Description>}
        <RadixToast.Close aria-label="Close" className="ml-auto text-sm underline">
          Close
        </RadixToast.Close>
      </RadixToast.Root>
      <RadixToast.Viewport
        className="fixed bottom-4 right-4 z-50 flex w-96 flex-col gap-2"
      />
    </RadixToast.Provider>
  );
}
