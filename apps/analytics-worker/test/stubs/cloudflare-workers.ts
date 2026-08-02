/**
 * Test stub for the `cloudflare:workers` runtime module.
 *
 * `cloudflare:workers` is a workerd built-in — it does not exist on disk and
 * cannot be resolved by Node, so importing `src/index.ts` (which extends
 * `WorkerEntrypoint`) under the plain-Node vitest environment needs this
 * stand-in. It is aliased in `vitest.config.ts`; production bundling is
 * unaffected (wrangler/workerd resolves the real built-in).
 *
 * The real base class is `abstract class WorkerEntrypoint<Env> { ctx; env; }`
 * with a `(ctx, env)` constructor — that constructor shape is the ONLY part of
 * it our entrypoint uses (`this.env.ANALYTICS_DB`), so the stub reproduces
 * exactly that and nothing else. It deliberately does NOT emulate the RPC
 * transport: these tests assert the method's LOGIC, while the trust property
 * ("only a Worker holding the binding can call it") is enforced by the
 * platform, not by code we could unit-test.
 */
export class WorkerEntrypoint<E = unknown> {
    readonly ctx: ExecutionContext;
    readonly env: E;

    constructor(ctx: ExecutionContext, env: E) {
        this.ctx = ctx;
        this.env = env;
    }
}
