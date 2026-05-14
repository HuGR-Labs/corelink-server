"use client";

import * as React from "react";
import * as Dialog from "@radix-ui/react-dialog";
import { cn } from "@/lib/cn";

export interface ModalProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: string;
  description?: string;
  children: React.ReactNode;
}

/**
 * Accessible modal dialog.
 * - aria-labelledby + aria-describedby wired by Radix.
 * - focus is trapped while open and returned to trigger on close.
 * - ESC closes (default Radix behaviour).
 */
export function Modal({ open, onOpenChange, title, description, children }: ModalProps) {
  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <Dialog.Portal>
        <Dialog.Overlay
          className={cn(
            "fixed inset-0 z-50 bg-black/50",
            "data-[state=open]:animate-in data-[state=closed]:animate-out"
          )}
        />
        <Dialog.Content
          className={cn(
            "fixed left-1/2 top-1/2 z-50 w-[90vw] max-w-md -translate-x-1/2 -translate-y-1/2",
            "rounded-lg bg-white p-6 shadow-xl",
            "focus-visible:outline-none"
          )}
        >
          <Dialog.Title className="mb-2 text-lg font-semibold text-slate-900">
            {title}
          </Dialog.Title>
          {description && (
            <Dialog.Description className="mb-4 text-sm text-slate-700">
              {description}
            </Dialog.Description>
          )}
          {children}
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
