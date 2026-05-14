import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));

export default defineConfig({
  plugins: [react()],
  esbuild: {
    jsx: "automatic",
  },
  test: {
    globals: true,
    environment: "jsdom",
    setupFiles: ["./vitest.setup.ts", "./src/test/setup.ts"],
    include: [
      "tests/**/*.test.{ts,tsx}",
      "src/**/*.test.{ts,tsx}",
      "src/**/__tests__/**/*.test.{ts,tsx}",
    ],
    css: false,
    coverage: {
      provider: "v8",
      reporter: ["text"],
    },
  },
  resolve: {
    alias: {
      "@": resolve(here, "src"),
    },
  },
});
