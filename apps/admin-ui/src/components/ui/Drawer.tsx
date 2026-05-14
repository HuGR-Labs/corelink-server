"use client";

import * as React from "react";
import * as Dialog from "@radix-ui/react-dialog";
import { cn } from "@/lib/cn";

export interface DrawerProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: string;
  side?: "left" | "right";
  children: React.ReactNode;
}

/**
 * Slide-in panel built on Radix Dialog (focus trap + ESC close).
 */
export function Drawer({ open, onOpenChange, title, side = "right", children }: DrawerProps) {
  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-40 bg-black/40" />
        <Dialog.Content
          className={cn(
            "fixed top-0 z-50 h-screen w-[min(420px,90vw)] bg-white p-6 shadow-2xl",
            "focus-visible:outline-none",
            side === "right" ? "right-0" : "left-0"
          )}
        >
          <Dialog.Title className="mb-4 text-lg font-semibold text-slate-900">
            {title}
          </Dialog.Title>
          {children}
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
