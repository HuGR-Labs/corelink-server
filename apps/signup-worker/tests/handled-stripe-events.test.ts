/**
 * Guardrail for the single-source-of-truth Stripe event set.
 *
 * handled-stripe-events.json drives BOTH the runtime dispatch allowlist
 * (HANDLED_EVENT_TYPES) AND the live Stripe endpoint's `enabled_events`
 * (scripts/ops/stripe-reconcile-webhook-events.sh reconciles the dashboard to it).
 *
 * These tests fail CI if a careless edit drops a downgrade path — the exact bug
 * that let a live endpoint be subscribed to fewer events than the code handles,
 * so a cancel / dunning-failure never reached the authoritative downgrade handler
 * and a churned customer kept paid access.
 */

import { describe, it, expect } from "vitest";
import { HANDLED_EVENT_TYPES } from "../src/webhooks/stripe.js";
import handled from "../src/webhooks/handled-stripe-events.json";

describe("handled-stripe-events.json — single source of truth", () => {
    it("runtime HANDLED_EVENT_TYPES is built from the JSON (no drift)", () => {
        expect([...HANDLED_EVENT_TYPES].sort()).toEqual([...handled.enabled_events].sort());
    });

    it("every downgrade-critical event is in enabled_events", () => {
        // If any of these is dropped, a cancel / plan-downgrade / terminal dunning
        // failure is never dispatched → churned customer keeps paid access.
        for (const ev of handled.downgrade_critical) {
            expect(handled.enabled_events).toContain(ev);
        }
    });

    it("the three revenue-leak triggers are pinned explicitly", () => {
        // Belt-and-suspenders: pin the exact strings so renaming/removing one
        // trips here even if downgrade_critical itself is edited.
        expect(handled.enabled_events).toContain("customer.subscription.updated");
        expect(handled.enabled_events).toContain("customer.subscription.deleted");
        expect(handled.enabled_events).toContain("invoice.payment_failed");
    });

    it("enabled_events has no duplicates and is well-formed", () => {
        const events = handled.enabled_events;
        expect(new Set(events).size).toBe(events.length);
        for (const ev of events) {
            // Stripe event types are lowercase dotted namespaces.
            expect(ev).toMatch(/^[a-z0-9_]+(\.[a-z0-9_]+)+$/);
        }
    });

    it("endpoint_url points at the signup-worker (downgrade authority)", () => {
        expect(handled.endpoint_url).toBe(
            "https://corelink-signup.humangr.com/webhooks/stripe",
        );
    });
});
