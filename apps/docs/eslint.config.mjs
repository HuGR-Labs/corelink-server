// ESLint v9 flat config for the CoreLink docs site.
//
// Scoped to TypeScript/TSX source under src/ and tests/. MDX content is
// linted by Vale (prose) and Docusaurus (build-time MDX compilation).

import js from "@eslint/js";
import tsParser from "@typescript-eslint/parser";
import tsPlugin from "@typescript-eslint/eslint-plugin";
import reactPlugin from "eslint-plugin-react";

export default [
  {
    ignores: [
      "build/**",
      ".docusaurus/**",
      "node_modules/**",
      "static/**",
      "i18n/**",
      "styles/**",
      "*.config.ts",
      "sidebars.ts",
    ],
  },
  js.configs.recommended,
  {
    files: ["src/**/*.{ts,tsx,js,jsx}", "tests/**/*.{ts,tsx}"],
    languageOptions: {
      parser: tsParser,
      parserOptions: {
        ecmaVersion: "latest",
        sourceType: "module",
        ecmaFeatures: { jsx: true },
      },
      globals: {
        // browser
        window: "readonly",
        document: "readonly",
        // Fetch API (Node 18+ globals; needed for the Pages-Functions
        // contract tests under tests/middleware-pages.test.ts).
        Request: "readonly",
        Response: "readonly",
        URL: "readonly",
        Headers: "readonly",
        fetch: "readonly",
        // node
        process: "readonly",
        console: "readonly",
        __dirname: "readonly",
        // vitest globals
        describe: "readonly",
        it: "readonly",
        expect: "readonly",
        vi: "readonly",
        beforeAll: "readonly",
        afterAll: "readonly",
        beforeEach: "readonly",
        afterEach: "readonly",
      },
    },
    plugins: {
      "@typescript-eslint": tsPlugin,
      react: reactPlugin,
    },
    settings: { react: { version: "detect" } },
    rules: {
      ...tsPlugin.configs.recommended.rules,
      ...reactPlugin.configs.recommended.rules,
      ...reactPlugin.configs["jsx-runtime"].rules,
      "react/prop-types": "off",
      "no-unused-vars": "off",
      "@typescript-eslint/no-unused-vars": [
        "error",
        { argsIgnorePattern: "^_", varsIgnorePattern: "^_" },
      ],
      "no-restricted-syntax": [
        "error",
        {
          selector: "JSXAttribute[name.name='dangerouslySetInnerHTML']",
          message:
            "dangerouslySetInnerHTML is forbidden in CoreLink docs components (WI-S18-001 constraint).",
        },
      ],
    },
  },
];
