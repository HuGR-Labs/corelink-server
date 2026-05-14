import "@testing-library/jest-dom/vitest";
import { expect } from "vitest";
import { toHaveNoViolations } from "jest-axe";

expect.extend(toHaveNoViolations as never);

// Polyfills for Radix in jsdom
if (typeof window !== "undefined") {
  // matchMedia
  if (!window.matchMedia) {
    Object.defineProperty(window, "matchMedia", {
      writable: true,
      value: (query: string) => ({
        matches: false,
        media: query,
        onchange: null,
        addListener: () => undefined,
        removeListener: () => undefined,
        addEventListener: () => undefined,
        removeEventListener: () => undefined,
        dispatchEvent: () => false,
      }),
    });
  }
  // ResizeObserver
  if (!("ResizeObserver" in window)) {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (window as any).ResizeObserver = class {
      observe() {}
      unobserve() {}
      disconnect() {}
    };
  }
  // IntersectionObserver
  if (!("IntersectionObserver" in window)) {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (window as any).IntersectionObserver = class {
      observe() {}
      unobserve() {}
      disconnect() {}
      takeRecords() {
        return [];
      }
    };
  }
  // Element.scrollIntoView
  if (typeof Element !== "undefined" && !Element.prototype.scrollIntoView) {
    Element.prototype.scrollIntoView = function () {};
  }
  // Element.hasPointerCapture (Radix)
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  if (typeof Element !== "undefined" && !(Element.prototype as any).hasPointerCapture) {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (Element.prototype as any).hasPointerCapture = function () {
      return false;
    };
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (Element.prototype as any).setPointerCapture = function () {};
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (Element.prototype as any).releasePointerCapture = function () {};
  }
}
