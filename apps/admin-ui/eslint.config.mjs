// Minimal ESLint flat config for the admin-ui workspace.
// WI-S16-001 ships the canonical config; we keep a stub here so `pnpm lint`
// runs in isolation during this worktree.
import tsParser from "@typescript-eslint/parser";

export default [
  {
    ignores: ["node_modules", ".next", "next-env.d.ts", "**/*.config.*"],
  },
  {
    files: ["src/**/*.ts", "src/**/*.tsx"],
    languageOptions: {
      parser: tsParser,
      parserOptions: {
        ecmaVersion: "latest",
        sourceType: "module",
        ecmaFeatures: { jsx: true },
      },
    },
    rules: {
      "no-console": ["error", { allow: ["warn", "error"] }],
    },
  },
];
