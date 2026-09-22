import { describe, expect, it, vi } from "vitest";
import {
    reconcileRunnersEntitlement,
    resolveAuthoritativeRunnerSubscription,
} from "../src/webhooks/stripe_persistence_runner.js";

const runnerPrices = new Set(["price_runner_team"]);

function subscription(id: string, status: string, created = 1_700_000_000) {
    return {
        id,
        customer: "cus_shared",
        status,
        created,
        items: { data: [{ price: { id: "price_runner_team" } }] },
    };
}

describe("Runners provider authority", () => {
    it("chooses the unique current same-second successor even when its opaque id sorts first", async () => {
        const old = subscription("sub_zzz_predecessor", "canceled");
        const current = subscription("sub_aaa_successor", "active");
        vi.stubGlobal("fetch", vi.fn(async (input: string) => ({
            ok: true,
            status: 200,
            json: async () => input.includes("?") ? { data: [old, current], has_more: false } : old,
        })));

        await expect(resolveAuthoritativeRunnerSubscription({
            eventSubscriptionId: old.id,
            stripeSecretKey: "sk_test_authority",
            runnerPriceIds: runnerPrices,
        })).resolves.toEqual({
            subscriptionId: current.id,
            status: "active",
            priceId: "price_runner_team",
            subscriptionCreatedAtMs: 1_700_000_000_000,
            authorityIsCurrent: true,
        });
    });

    it("fails closed when current Runners authority is ambiguous or absent", async () => {
        const old = subscription("sub_old", "canceled");
        for (const [data, eventSubscription] of [
            [[subscription("sub_a", "active"), subscription("sub_b", "trialing")], old],
            [[], { ...old, items: { data: [{ price: { id: "price_cache" } }] } }],
        ]) {
            vi.stubGlobal("fetch", vi.fn(async (input: string) => ({
                ok: true,
                status: 200,
                json: async () => input.includes("?") ? { data, has_more: false } : eventSubscription,
            })));
            await expect(resolveAuthoritativeRunnerSubscription({
                eventSubscriptionId: old.id,
                stripeSecretKey: "sk_test_authority",
                runnerPriceIds: runnerPrices,
            })).rejects.toThrow(/ambiguous|no current/);
        }
    });

    it("rejects a stale identity after an atomic D1 batch reads a different fence", async () => {
        const statements: Array<{ sql: string; params: unknown[] }> = [];
        const db = {
            prepare(sql: string) {
                const params: unknown[] = [];
                const statement = {
                    sql,
                    params,
                    bind(...values: unknown[]) { params.push(...values); return statement; },
                    async run() { return { results: [] }; },
                };
                statements.push(statement);
                return statement;
            },
            async batch(batch: Array<{ run(): Promise<unknown> }>) {
                expect(batch).toHaveLength(3);
                await Promise.all(batch.map((statement) => statement.run()));
                return [{ results: [] }, { results: [] }, {
                    results: [{
                        stripe_subscription_id: "sub_aaa_successor",
                        subscription_created_at_ms: 1_700_000_000_000,
                        stripe_event_created_at_ms: 2_000,
                        stripe_event_id: "evt_successor",
                    }],
                }];
            },
        };
        await expect(reconcileRunnersEntitlement(db, {
            tenantId: "ten_1",
            runnerSubscriptionId: "sub_zzz_predecessor",
            subscriptionCreatedAtMs: 1_700_000_000_000,
            stripeEventCreatedAtMs: 3_000,
            stripeEventId: "evt_predecessor_replay",
            authorityIsCurrent: false,
            entitlement: { maxConcurrency: 80, maxVcpuH: 600 },
            nowMs: 4_000,
        })).rejects.toThrow("stale runner entitlement authority rejected");
        expect(statements[0]!.sql).toContain("authority_is_current = 1");
        expect(statements[0]!.params).toContain(0);
    });
});
