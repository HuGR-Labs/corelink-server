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
      // Build OUTPUT — never lint generated bundles. `next lint` skipped these
      // implicitly; the codemod's `eslint .` does not, so restore the prior scope.
      "**/.open-next/",
      "**/.wrangler/",
      "**/node_modules/",
      "**/dist/",
      "**/coverage/",
      "**/playwright/",
      "**/playwright.config.ts",
      // Build script + the eslint flat-config itself (non-app tooling; `next lint`
      // never covered these — console in a build script is intentional).
      "scripts/**",
      "eslint.config.mjs",
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
  {
    // Next 16's eslint-config-next@16 ships the NEW react-compiler-era
    // eslint-plugin-react-hooks rules, which flag PRE-EXISTING patterns across
    // the app (e.g. set-state-in-effect). Adopting those rules is a separate,
    // deliberate refactor pass — NOT part of this framework-major bump. We keep
    // the PRIOR lint bar (nothing that passed before now fails) by downgrading
    // ONLY these brand-new rules to `warn` (still visible/tracked, not CI-fatal).
    // Cleanup tracked as a follow-up; do NOT silently drop them to `off`.
    // Flat config: the rule's owning plugin must be registered in the SAME
    // object — re-register react-hooks + import from the next flat-config (idx 0).
    plugins: {
      "react-hooks": nextCoreWebVitals[0].plugins["react-hooks"],
      import: nextCoreWebVitals[0].plugins["import"],
    },
    rules: {
      "react-hooks/set-state-in-effect": "warn",
      "react-hooks/static-components": "warn",
      "react-hooks/refs": "warn",
      "react-hooks/purity": "warn",
      "react-hooks/globals": "warn",
      "import/no-anonymous-default-export": "warn",
    },
  },
];
