import { fileURLToPath } from "node:url";
import { defineConfig } from "vitest/config";

export default defineConfig({
    resolve: {
        alias: {
            // `cloudflare:workers` is a workerd BUILT-IN with no on-disk module,
            // so Node cannot resolve it when a test imports `src/index.ts` (which
            // extends `WorkerEntrypoint`). Alias it to a minimal stub. Deploy-time
            // bundling is untouched — wrangler/workerd resolve the real built-in.
            "cloudflare:workers": fileURLToPath(
                new URL("./test/stubs/cloudflare-workers.ts", import.meta.url),
            ),
        },
    },
    test: {
        include: ["test/**/*.test.ts"],
        environment: "node",
    },
});
