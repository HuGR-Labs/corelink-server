"use client";

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
} from "react";
import { createPortal } from "react-dom";
import type { ReactNode } from "react";

export interface ToastInput {
  title: string;
  tone?: "neutral" | "success" | "danger";
}

interface ToastRecord extends ToastInput {
  id: number;
}

export interface ToastContextValue {
  toast: (t: ToastInput) => void;
}

const ToastContext = createContext<ToastContextValue | null>(null);

export interface ToastProviderProps {
  children: ReactNode;
}

export function ToastProvider({ children }: ToastProviderProps) {
  const [toasts, setToasts] = useState<Array<ToastRecord>>([]);
  const nextId = useRef(0);
  const [mounted, setMounted] = useState(false);

  useEffect(() => {
    setMounted(true);
  }, []);

  const toast = useCallback((t: ToastInput) => {
    const id = nextId.current++;
    setToasts((prev) => [...prev, { ...t, id }]);
    setTimeout(() => {
      setToasts((prev) => prev.filter((x) => x.id !== id));
    }, 3500);
  }, []);

  return (
    <ToastContext.Provider value={{ toast }}>
      {children}
      {mounted && typeof document !== "undefined"
        ? createPortal(
            <div className="lin-toasts">
              {toasts.map((t) => {
                const cls = ["lin-toast"];
                if (t.tone === "success") cls.push("lin-badge--success");
                else if (t.tone === "danger") cls.push("lin-badge--danger");
                return (
                  <div key={t.id} className={cls.join(" ")} role="status">
                    {t.title}
                  </div>
                );
              })}
            </div>,
            document.body,
          )
        : null}
    </ToastContext.Provider>
  );
}

export function useToast(): ToastContextValue {
  const ctx = useContext(ToastContext);
  if (ctx == null) {
    throw new Error("useToast must be used within a ToastProvider");
  }
  return ctx;
}
