import "@testing-library/jest-dom/vitest";

// happy-dom provides a stub IntersectionObserver that never fires. Delete it
// so components fall back to the scroll-event path which we can drive
// deterministically from tests.
// eslint-disable-next-line @typescript-eslint/no-explicit-any
delete (globalThis as any).IntersectionObserver;
// eslint-disable-next-line @typescript-eslint/no-explicit-any
if (typeof window !== "undefined") delete (window as any).IntersectionObserver;
