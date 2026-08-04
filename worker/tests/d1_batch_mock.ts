/**
 * `db.batch()` for the hand-rolled D1 mocks in this suite.
 *
 * # Why this exists
 *
 * The Worker's quota gate issues the monthly-counter UPSERT and the storage
 * `SUM(bytes_used)` read as ONE `db.batch()` round trip (`lib/quota.ts`
 * `runQuotaBatch` — the `wdb` latency fix). Every D1 mock in this suite stubs
 * `batch` as `async () => []`, which is not "no rows": it is a batch that
 * returned FEWER results than statements, i.e. a fault. The gate reads that
 * fail-closed on a write (a 429 rather than a silent "0 bytes used"), so the
 * stub turns every mocked PUT into a 429 — the failure signature that proves the
 * batched path is really the one under test.
 *
 * This helper implements `batch` in terms of each statement's own `first()`, so
 * a mock keeps ONE routing table (its `prepare(sql).bind(args).first()` switch)
 * and both the serial and the batched call sites resolve through it. That
 * matters beyond convenience: a separately-written `batch` mock would be free to
 * answer differently from the `first()` path, and the test would stop comparing
 * the two.
 *
 * Fidelity to real D1, on the two properties the gate depends on:
 *   - each statement's rows come back positionally, in the order submitted;
 *   - a batch is a TRANSACTION — a throwing statement fails the whole call, so a
 *     mock that throws on one statement rejects the batch (`Promise.all`),
 *     exactly as D1 does, rather than yielding a partial result array.
 */

/** The subset of a mocked bound statement this helper needs. */
interface BoundStatementLike {
  first: <T>() => Promise<T | null>;
}

/**
 * Build a `batch` implementation that resolves each statement through the mock's
 * own `first()`. A `null` row becomes an empty `results` array (what D1 returns
 * for a statement that matched nothing).
 */
export function batchViaFirst(): (statements: unknown[]) => Promise<unknown[]> {
  return async (statements: unknown[]) =>
    Promise.all(
      statements.map(async (statement) => {
        const row = await (statement as BoundStatementLike).first<unknown>();
        return {
          success: true as const,
          meta: {} as never,
          results: row === null || row === undefined ? [] : [row],
        };
      }),
    );
}
