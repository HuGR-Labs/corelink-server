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
// next.config.ts), so those rules are surfaced as "warn" here — visible for a
// future React-Compiler adoption pass, without falsely gating the build on
// compiler discipline the runtime does not yet apply. The long-standing hooks
// rules stay at full strength: rules-of-hooks = error, exhaustive-deps = warn.
import nextCoreWebVitals from 'eslint-config-next/core-web-vitals';

const projectRules = {
  'no-console': ['error', { allow: ['warn', 'error'] }],
  'no-eval': 'error',
  'no-implied-eval': 'error',
  'react/no-danger': 'error',
  // Keep the two original, always-enforced hooks rules explicit.
  'react-hooks/rules-of-hooks': 'error',
  'react-hooks/exhaustive-deps': 'warn',
  // React Compiler enforcement suite (react-hooks 7) — warn until the compiler
  // is adopted. Not disabled: violations remain visible in lint output.
  'react-hooks/static-components': 'warn',
  'react-hooks/use-memo': 'warn',
  'react-hooks/preserve-manual-memoization': 'warn',
  'react-hooks/immutability': 'warn',
  'react-hooks/globals': 'warn',
  'react-hooks/refs': 'warn',
  'react-hooks/set-state-in-effect': 'warn',
  'react-hooks/set-state-in-render': 'warn',
  'react-hooks/error-boundaries': 'warn',
  'react-hooks/purity': 'warn',
  'react-hooks/config': 'warn',
  'react-hooks/gating': 'warn',
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
];

export default config;
