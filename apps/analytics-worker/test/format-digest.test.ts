import { describe, expect, it } from "vitest";
import { formatDigestText } from "../src/cron/weekly-email";

describe("formatDigestText", () => {
    it("renders all-zeros cohort gracefully (matches acceptance §7.5)", () => {
        const out = formatDigestText(
            {
                medianTtfvMinutes: null,
                d1ActivationRate: null,
                freeToPaid30dRate: null,
                signups7d: 0,
                d1Activated: 0,
                signupsPrior30d: 0,
                paid30d: 0,
            },
            "W22-2026",
        );
        expect(out).toContain("CoreLink weekly — W22-2026");
        expect(out).toContain("no activations yet");
        expect(out).toContain("no signups in 7d window");
        expect(out).toContain("no prior-30d cohort yet");
        expect(out).toContain("target ≤ 10 min");
        expect(out).toContain("target ≥ 35%");
        expect(out).toContain("target ≥ 5%");
    });

    it("renders concrete values with raw n/d in parentheses", () => {
        const out = formatDigestText(
            {
                medianTtfvMinutes: 7.42,
                d1ActivationRate: 0.375,
                freeToPaid30dRate: 0.071,
                signups7d: 8,
                d1Activated: 3,
                signupsPrior30d: 14,
                paid30d: 1,
            },
            "W22-2026",
        );
        expect(out).toContain("7.4 min");
        expect(out).toContain("37.5%");
        expect(out).toContain("(3/8)");
        expect(out).toContain("7.1%");
        expect(out).toContain("(1/14)");
    });
});
