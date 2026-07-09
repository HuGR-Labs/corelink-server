// Flat ESLint config (ESLint 9 + eslint-config-next 16).
// Replaces the legacy `.eslintrc.json` + `next lint` (removed in Next 16).
//
// Project rules (including the react/no-danger security rule) are merged into
// the shared next config object that already registers the `react` plugin —
// flat config requires a plugin-namespaced rule to live in the same config
// object that defines that plugin.
//
// react-hooks 7 (bundled by eslint-config-next 16) ships the React Compiler
// enforcement rule suite in `recommended`, all at "error". This app has NOT
// adopted the React Compiler (no babel-plugin-react-compiler / reactCompiler in
// next.config.ts), so those advisory-for-a-compiler-we-don't-run rules are
// turned OFF here. The standing project decision is to PRESERVE THE PRIOR LINT
// BAR across the eslint-config-next 16 bump — not to adopt React-Compiler
// discipline the runtime does not apply. The two long-standing hooks rules that
// predate react-hooks 7 stay exactly as before: rules-of-hooks = error,
// exhaustive-deps = warn. (Re-enable the suite in a dedicated Compiler-adoption
// PR if/when next.config wires reactCompiler.)
import nextCoreWebVitals from 'eslint-config-next/core-web-vitals';

const projectRules = {
  'no-console': ['error', { allow: ['warn', 'error'] }],
  'no-eval': 'error',
  'no-implied-eval': 'error',
  'react/no-danger': 'error',
  // Keep the two original, always-enforced hooks rules explicit.
  'react-hooks/rules-of-hooks': 'error',
  'react-hooks/exhaustive-deps': 'warn',
  // React Compiler enforcement suite (new in react-hooks 7) — OFF: the app has
  // not adopted the compiler, so these do not describe a bar the runtime holds.
  'react-hooks/static-components': 'off',
  'react-hooks/use-memo': 'off',
  'react-hooks/preserve-manual-memoization': 'off',
  'react-hooks/immutability': 'off',
  'react-hooks/globals': 'off',
  'react-hooks/refs': 'off',
  'react-hooks/set-state-in-effect': 'off',
  'react-hooks/set-state-in-render': 'off',
  'react-hooks/error-boundaries': 'off',
  'react-hooks/purity': 'off',
  'react-hooks/config': 'off',
  'react-hooks/gating': 'off',
};

const config = [
  {
    ignores: [
      '.next/',
      'node_modules/',
      'dist/',
      'coverage/',
      'playwright/',
      'playwright.config.ts',
      '.open-next/',
    ],
  },
  ...nextCoreWebVitals.map((cfg) =>
    cfg?.plugins?.react
      ? { ...cfg, rules: { ...cfg.rules, ...projectRules } }
      : cfg,
  ),
  {
    // Node build/tooling scripts and config files legitimately use console.
    files: ['scripts/**', 'eslint.config.mjs', '*.config.{js,cjs,mjs,ts,mts}'],
    rules: { 'no-console': 'off' },
  },
  {
    // Playwright e2e suite. Its fixtures take a `use` callback
    // (`async ({ page }, use) => { … await use(page); }`) — that `use` is
    // Playwright's fixture setter, NOT React's `use` hook, but react-hooks 7's
    // rules-of-hooks now recognizes a bare `use(...)` call as a React hook and
    // false-positives on it. These files are Playwright test infra, not React
    // render code, so the react-hooks rule set does not apply to them. (The
    // sibling legacy `playwright/` dir is already fully ignored above.)
    files: ['tests/e2e/**'],
    rules: {
      'react-hooks/rules-of-hooks': 'off',
      'react-hooks/exhaustive-deps': 'off',
    },
  },
];

export default config;
