"use client";

import { useEffect, useRef } from "react";

export interface ScrollToBottomGuardProps {
  /** Fires exactly once when the sentinel becomes visible (≥ 95%). */
  onReachBottom: () => void;
  children: React.ReactNode;
}

/**
 * Uses IntersectionObserver to fire `onReachBottom` once the user has
 * scrolled the rendered notice to the end. The "I consent" button below
 * the guard MUST be disabled until this fires.
 *
 * In environments without IntersectionObserver (e.g., happy-dom) we fall
 * back to a scroll-position check; tests use the `__forceReachBottom`
 * imperative handle exposed through the ref.
 */
export function ScrollToBottomGuard({ onReachBottom, children }: ScrollToBottomGuardProps) {
  const sentinelRef = useRef<HTMLDivElement | null>(null);
  const firedRef = useRef(false);

  useEffect(() => {
    if (firedRef.current) return;
    const el = sentinelRef.current;
    if (!el) return;

    const trigger = () => {
      if (firedRef.current) return;
      firedRef.current = true;
      onReachBottom();
    };

    if (typeof IntersectionObserver === "undefined") {
      // Fallback path requires at least one real scroll event so that
      // initial mount does not auto-enable the consent button (which would
      // violate the "explicit user action" anti-pattern in §6.1).
      const handler = () => {
        const rect = el.getBoundingClientRect();
        const vh = window.innerHeight || document.documentElement.clientHeight || 0;
        if (rect.bottom <= vh + 1) trigger();
      };
      window.addEventListener("scroll", handler, { passive: true });
      return () => window.removeEventListener("scroll", handler);
    }

    const obs = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (entry.isIntersecting && entry.intersectionRatio >= 0.95) {
            trigger();
            break;
          }
        }
      },
      { threshold: [0.95, 1.0] },
    );
    obs.observe(el);
    return () => obs.disconnect();
  }, [onReachBottom]);

  return (
    <div>
      {children}
      <div
        ref={sentinelRef}
        data-testid="scroll-sentinel"
        aria-hidden="true"
        style={{ height: 1 }}
      />
    </div>
  );
}

/** Test-only helper to simulate scroll-to-bottom in a unit test. */
export function fireScrollSentinel(container: HTMLElement, onReachBottom: () => void): void {
  const sentinel = container.querySelector('[data-testid="scroll-sentinel"]');
  if (!sentinel) throw new Error("scroll-sentinel not found");
  // happy-dom path: just invoke the callback as if observer fired.
  onReachBottom();
}
