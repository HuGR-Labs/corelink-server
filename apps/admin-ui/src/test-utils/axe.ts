import { configureAxe } from "jest-axe";

/**
 * Pre-configured axe runner with WCAG 2.2 AA rules enabled.
 * Used by every primitive's `.test.tsx` to assert zero violations.
 */
export const axe = configureAxe({
  rules: {
    // Color-contrast assumes computed styles; jsdom has none, so this rule
    // is unreliable in unit tests — verified at E2E/CI level instead.
    "color-contrast": { enabled: false },
    // landmark-one-main only applies to full pages.
    "landmark-one-main": { enabled: false },
    region: { enabled: false },
  },
});
