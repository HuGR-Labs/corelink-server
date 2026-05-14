// Minimal ESLint flat config for @corelink/docs. Lints the TypeScript helpers
// + tests under `src/` and `tests/`; the MDX content under `docs/` is gated
// by the cross-functional test, not by ESLint.
import tseslint from "typescript-eslint";

export default tseslint.config(
  {
    ignores: [
      "node_modules/**",
      "dist/**",
      "docs/**",
      "src/**/*.tsx",
      "**/*.mdx",
    ],
  },
  ...tseslint.configs.recommended,
  {
    files: ["src/**/*.ts", "tests/**/*.ts"],
    languageOptions: {
      ecmaVersion: 2022,
      sourceType: "module",
    },
    rules: {
      "@typescript-eslint/no-unused-vars": [
        "error",
        { argsIgnorePattern: "^_", varsIgnorePattern: "^_" },
      ],
      "@typescript-eslint/no-non-null-assertion": "off",
      "no-empty": "error",
      "no-constant-condition": "error",
      eqeqeq: ["error", "always"],
    },
  },
);
