// ESLint flat config (migrated from .eslintrc.json by @next/codemod
// next-lint-to-eslint-cli during the Next 16 upgrade). Next 16 removed
// `next lint`; the plugin now ships a flat-config array under
// `eslint-config-next/core-web-vitals`. Written as a plain flat-config array
// (rather than the codemod's `eslint/config` helper) so it does not require an
// eslint newer than the pinned 9.21.
import nextCoreWebVitals from "eslint-config-next/core-web-vitals";

export default [
  {
    ignores: [
      "**/.next/",
      "**/node_modules/",
      "**/dist/",
      "**/coverage/",
      "**/playwright/",
      "**/playwright.config.ts",
    ],
  },
  ...nextCoreWebVitals,
  {
    // Re-register the `react` plugin (owned by the `next` flat-config object)
    // so the `react/no-danger` rule below resolves under flat config.
    plugins: { react: nextCoreWebVitals[0].plugins.react },
    rules: {
      "no-console": ["error", { allow: ["warn", "error"] }],
      "no-eval": "error",
      "no-implied-eval": "error",
      "react/no-danger": "error",
    },
  },
];
