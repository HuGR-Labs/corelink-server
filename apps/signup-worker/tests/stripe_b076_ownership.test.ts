import { describe, expect, it } from "vitest";
import {
    activatePaidTierSelection,
    recoverCheckoutLedger,
    upsertBillingPaid,
    type D1DatabaseLike,
} from "../src/webhooks/billing_checkout";

// The signup-worker CI runs on Node 22.  node:sqlite gives this focal a real
// SQLite execution engine, rather than a mock that only records SQL strings.
// @ts-expect-error Cloudflare's worker type set intentionally omits Node-only test APIs.
import { DatabaseSync } from "node:sqlite";

function fakeDb(changes: number | undefined = 1): D1DatabaseLike & { sql: string[] } {
    const db = {
        sql: [],
        prepare(query: string) {
            db.sql.push(query);
            return {
                bind() {
                    return { run: async () => changes === undefined ? {} : { meta: { changes } } };
                },
            };
        },
    } as unknown as D1DatabaseLike & { sql: string[] };
    return db;
}

type SqliteD1 = D1DatabaseLike & {
    exec(sql: string): void;
    get(sql: string, ...values: unknown[]): Record<string, unknown> | undefined;
};

function sqliteDb(): SqliteD1 {
    const raw = new DatabaseSync(":memory:");
    raw.exec(`
      CREATE TABLE stripe_checkout_ownership_ledger (
        correlation_id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL, tier TEXT NOT NULL,
        session_id TEXT, stripe_customer_id TEXT, state TEXT NOT NULL,
        created_at_ms INTEGER NOT NULL, updated_at_ms INTEGER NOT NULL
      );
      CREATE TABLE stripe_checkout_sessions (
        session_id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL, tier TEXT NOT NULL,
        correlation_id TEXT NOT NULL, created_at_ms INTEGER NOT NULL
      );
      CREATE TABLE tier_selections (
        tenant_id TEXT PRIMARY KEY, tier TEXT NOT NULL,
        subscription_state TEXT NOT NULL, stripe_customer_id TEXT,
        subscription_started_at_ms INTEGER, schema_version INTEGER,
        correlation_id TEXT
      );
      CREATE TABLE tenant_billing (
        tenant_id TEXT PRIMARY KEY, stripe_customer_id TEXT,
        stripe_subscription_id TEXT, status TEXT NOT NULL, plan TEXT,
        current_period_end_ms INTEGER, schema_version INTEGER,
        created_at_ms INTEGER, updated_at_ms INTEGER
      );
    `);
    const d1 = {
        exec(sql: string) {
            raw.exec(sql);
        },
        get(sql: string, ...values: unknown[]) {
            const normalized = sql.replace(/\?(\d+)/g, ":p$1");
            const params = Object.fromEntries(
                values.map((value, index) => [`p${index + 1}`, value]),
            );
            return raw.prepare(normalized).get(params) as Record<string, unknown> | undefined;
        },
        prepare(query: string) {
            let values: unknown[] = [];
            return {
                bind(...args: unknown[]) {
                    values = args;
                    return {
                        async run() {
                            // Node's experimental sqlite binding treats ?1 as
                            // a named parameter. Normalize D1's numbered
                            // placeholders to equivalent named parameters;
                            // repeated ?N references retain their identity.
                            const normalized = query.replace(/\?(\d+)/g, ":p$1");
                            const params = Object.fromEntries(
                                values.map((value, index) => [`p${index + 1}`, value]),
                            );
                            const result = raw.prepare(normalized).run(params);
                            return { meta: { changes: Number(result.changes) } };
                        },
                    };
                },
            };
        },
    } as unknown as SqliteD1;
    return d1;
}

describe("B-076 paid Checkout ownership", () => {
    it("requires the exact pending session/customer before materializing billing", async () => {
        const db = sqliteDb();
        db.exec(`
          INSERT INTO stripe_checkout_sessions VALUES ('cs-a', 'tenant-a', 'pro', 'corr-a', 1000);
          INSERT INTO tier_selections VALUES ('tenant-a', 'pro', 'pending_checkout', 'cus-a', NULL, 1, 'corr-a');
        `);
        await upsertBillingPaid(db, {
            tenantId: "tenant-a",
            stripeCustomerId: "cus-a",
            stripeSubscriptionId: "sub-a",
            sessionId: "cs-a",
            plan: "pro",
            currentPeriodEndMs: null,
            nowMs: 2_000,
            checkoutCreatedAtMs: 1_000,
        });
        expect(db.get("SELECT status FROM tenant_billing WHERE tenant_id = ?1", "tenant-a"))
            .toEqual({ status: "paid" });
    });

    it("rejects an update that D1 says did not own the paid billing row", async () => {
        await expect(
            upsertBillingPaid(fakeDb(0), {
                tenantId: "tenant-a",
                stripeCustomerId: "cus-other",
                stripeSubscriptionId: "sub-other",
                sessionId: "cs-other",
                plan: "max",
                currentPeriodEndMs: null,
                nowMs: 2,
            }),
        ).rejects.toThrow("ownership conflict");
    });

    it("repeats the exact subscription owner at the entitlement mutation", async () => {
        const db = sqliteDb();
        db.exec(`
          INSERT INTO tenant_billing VALUES ('tenant-a', 'cus-a', 'sub-a', 'paid', 'pro', NULL, 1, 1, 1);
        `);
        await activatePaidTierSelection(db, {
            tenantId: "tenant-a",
            tier: "pro",
            stripeCustomerId: "cus-a",
            stripeSubscriptionId: "sub-a",
            sessionId: "cs-a",
            nowMs: 3,
            correlationId: "corr-a",
        });
        expect(db.get("SELECT subscription_state FROM tier_selections WHERE tenant_id = ?1", "tenant-a"))
            .toEqual({ subscription_state: "active" });
    });

    it("fails closed when activation has no D1 changes metadata", async () => {
        const db = fakeDb(2);
        await expect(activatePaidTierSelection(db, {
            tenantId: "tenant-a",
            tier: "pro",
            stripeCustomerId: "cus-a",
            stripeSubscriptionId: "sub-a",
            sessionId: "cs-a",
            nowMs: 4,
            correlationId: "corr-a",
        })).rejects.toThrow("changes metadata");
    });

    it("executes stale and fresh webhook recovery against SQLite", async () => {
        const db = sqliteDb();
        const createdAt = 1_700_000_000_000;
        db.exec(`
          INSERT INTO stripe_checkout_ownership_ledger
            VALUES ('corr-stale', 'tenant-a', 'pro', NULL, NULL, 'reserved', ${createdAt}, ${createdAt});
        `);
        await expect(recoverCheckoutLedger(db, {
            tenantId: "tenant-a",
            tier: "pro",
            sessionId: "cs-a",
            stripeCustomerId: "cus-a",
            checkoutCreatedAtMs: createdAt,
            nowMs: createdAt + 15 * 60 * 1000 + 1,
        })).resolves.toBe(false);
        expect(db.get("SELECT session_id FROM stripe_checkout_ownership_ledger WHERE correlation_id = ?1", "corr-stale"))
            .toEqual({ session_id: null });

        db.exec(`
          INSERT INTO stripe_checkout_ownership_ledger
            VALUES ('corr-fresh', 'tenant-a', 'max', NULL, NULL, 'reserved', ${createdAt}, ${createdAt});
        `);
        await expect(recoverCheckoutLedger(db, {
            tenantId: "tenant-a",
            tier: "max",
            sessionId: "cs-fresh",
            stripeCustomerId: "cus-a",
            checkoutCreatedAtMs: createdAt,
            nowMs: createdAt + 1_000,
        })).resolves.toBe(true);
        expect(db.get("SELECT session_id, state FROM stripe_checkout_ownership_ledger WHERE correlation_id = ?1", "corr-fresh"))
            .toEqual({ session_id: "cs-fresh", state: "session_created" });
    });
});
