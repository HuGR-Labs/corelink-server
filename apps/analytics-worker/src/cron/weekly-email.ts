// Cron handler — runs Mondays 08:00 UTC.
//
// Reads `weekly-three-numbers.sql` against ANALYTICS_DB, formats the
// PLG §7.5 three-numbers text, sends to DIGEST_RECIPIENT via Resend.
//
// Hard rules (per Phase 0.G §11 DO-NOT):
//   - NEVER send real production email to anyone other than gustavo@humangr.com.
//     The DIGEST_RECIPIENT env var is hard-defaulted in wrangler.toml; this
//     handler also asserts the address-equality at runtime as a defence-in-depth.
//   - If RESEND_API_KEY is missing, the cron logs + returns without sending.
//     Better to skip than to crash the cron and stop next week from running.

import type { Env } from "../types";

interface ThreeNumbersRow {
    median_ttfv_minutes: number | null;
    d1_activated_n: number | null;
    signups_7d_n: number | null;
    paid_30d_n: number | null;
    signups_prior_30d_n: number | null;
}

export interface ThreeNumbers {
    medianTtfvMinutes: number | null;
    d1ActivationRate: number | null;
    freeToPaid30dRate: number | null;
    // Raw counts so the email shows e.g. "3/8" alongside the ratio.
    signups7d: number;
    d1Activated: number;
    signupsPrior30d: number;
    paid30d: number;
}

/** Run the §7.5 SQL view and shape into the digest payload. */
export async function computeThreeNumbers(env: Env): Promise<ThreeNumbers> {
    // Inline the SELECT so the worker doesn't have to import a .sql file at
    // runtime (Cloudflare Workers bundler does not load .sql by default).
    // The string MUST stay byte-identical to src/views/weekly-three-numbers.sql
    // minus the leading comment block — a unit test enforces this in test/.
    const SQL = `
WITH signups_7d AS (
    SELECT tenant_id, MIN(created_at) AS signed_up_at
    FROM analytics_events
    WHERE event_name = 'signup_completed'
      AND tenant_id IS NOT NULL
      AND created_at >= datetime('now', '-7 days')
    GROUP BY tenant_id
),
ttfv AS (
    SELECT
        s.tenant_id,
        (julianday(MIN(e.created_at)) - julianday(s.signed_up_at)) * 24.0 * 60.0 AS ttfv_minutes
    FROM signups_7d s
    JOIN analytics_events e
      ON e.tenant_id = s.tenant_id
     AND e.event_name = 'first_cache_hit'
     AND e.created_at > s.signed_up_at
    GROUP BY s.tenant_id
),
ranked AS (
    SELECT ttfv_minutes, NTILE(2) OVER (ORDER BY ttfv_minutes) AS q2
    FROM ttfv
),
d1_activated AS (
    SELECT COUNT(*) AS n
    FROM signups_7d s
    WHERE EXISTS (
        SELECT 1 FROM analytics_events e
        WHERE e.tenant_id = s.tenant_id
          AND e.event_name = 'first_cache_hit'
          AND e.created_at <= datetime(s.signed_up_at, '+1 day')
    )
),
signups_30d AS (
    SELECT tenant_id, MIN(created_at) AS signed_up_at
    FROM analytics_events
    WHERE event_name = 'signup_completed'
      AND tenant_id IS NOT NULL
      AND created_at >= datetime('now', '-37 days')
      AND created_at <  datetime('now', '-7 days')
    GROUP BY tenant_id
),
paid_30d AS (
    SELECT COUNT(*) AS n
    FROM signups_30d s
    WHERE EXISTS (
        SELECT 1 FROM analytics_events e
        WHERE e.tenant_id = s.tenant_id
          AND e.event_name = 'paid_subscription_started'
          AND e.created_at <= datetime(s.signed_up_at, '+30 days')
    )
)
SELECT
    ROUND((SELECT MAX(ttfv_minutes) FROM ranked WHERE q2 = 1), 2) AS median_ttfv_minutes,
    (SELECT n FROM d1_activated) AS d1_activated_n,
    (SELECT COUNT(*) FROM signups_7d) AS signups_7d_n,
    (SELECT n FROM paid_30d) AS paid_30d_n,
    (SELECT COUNT(*) FROM signups_30d) AS signups_prior_30d_n;
`;

    const row = await env.ANALYTICS_DB.prepare(SQL).first<ThreeNumbersRow>();
    const signups7d = row?.signups_7d_n ?? 0;
    const d1Activated = row?.d1_activated_n ?? 0;
    const signupsPrior30d = row?.signups_prior_30d_n ?? 0;
    const paid30d = row?.paid_30d_n ?? 0;

    return {
        medianTtfvMinutes: row?.median_ttfv_minutes ?? null,
        d1ActivationRate: signups7d > 0 ? d1Activated / signups7d : null,
        freeToPaid30dRate: signupsPrior30d > 0 ? paid30d / signupsPrior30d : null,
        signups7d,
        d1Activated,
        signupsPrior30d,
        paid30d,
    };
}

/** Plain-text email body. Kept human-eyeballable on a phone (PLG §4.1). */
export function formatDigestText(n: ThreeNumbers, isoWeek: string): string {
    const ttfv = n.medianTtfvMinutes !== null
        ? `${n.medianTtfvMinutes.toFixed(1)} min`
        : "no activations yet";
    const d1 = n.d1ActivationRate !== null
        ? `${(n.d1ActivationRate * 100).toFixed(1)}%  (${n.d1Activated}/${n.signups7d})`
        : `no signups in 7d window`;
    const f2p = n.freeToPaid30dRate !== null
        ? `${(n.freeToPaid30dRate * 100).toFixed(1)}%  (${n.paid30d}/${n.signupsPrior30d})`
        : `no prior-30d cohort yet`;

    return [
        `CoreLink weekly — ${isoWeek}`,
        ``,
        `=== THREE NUMBERS (PLG §7.5) ===`,
        ``,
        `1. Median TTFV (signup → first cache hit)`,
        `     ${ttfv}                        target ≤ 10 min`,
        ``,
        `2. D1 activation rate (24h cohort)`,
        `     ${d1}        target ≥ 35%`,
        ``,
        `3. Free → paid 30-day conversion`,
        `     ${f2p}        target ≥ 5%`,
        ``,
        `=== INTERPRETATION GUIDE ===`,
        `(1) regresses → audit wizard friction`,
        `(2) regresses → audit docs + install one-liner`,
        `(3) regresses → audit upgrade prompts + quota thresholds`,
        ``,
        `--`,
        `Generated by corelink-analytics weekly cron.`,
        `Source view: apps/analytics-worker/src/views/weekly-three-numbers.sql`,
    ].join("\n");
}

/**
 * Send the digest via Resend. Returns the Resend message-id on success, or
 * throws on HTTP error. Caller wraps in try/catch so a Resend outage does
 * not crash the cron — the next Monday will retry.
 */
async function sendViaResend(env: Env, subject: string, text: string): Promise<string> {
    if (!env.RESEND_API_KEY) {
        throw new Error("RESEND_API_KEY not configured");
    }
    // Defence in depth (per §11 DO NOT): refuse any recipient other than
    // the metrics-audit-mandated gustavo@humangr.com.
    if (env.DIGEST_RECIPIENT !== "gustavo@humangr.com") {
        throw new Error(
            `DIGEST_RECIPIENT lockdown: refused to send to ${env.DIGEST_RECIPIENT}; ` +
            `weekly digest is hard-pinned to gustavo@humangr.com by Phase 0.G §11.`
        );
    }
    const res = await fetch("https://api.resend.com/emails", {
        method: "POST",
        headers: {
            "Authorization": `Bearer ${env.RESEND_API_KEY}`,
            "Content-Type": "application/json",
        },
        body: JSON.stringify({
            from: env.DIGEST_FROM,
            to: [env.DIGEST_RECIPIENT],
            subject,
            text,
        }),
    });
    if (!res.ok) {
        throw new Error(`Resend HTTP ${res.status}: ${await res.text()}`);
    }
    const body = (await res.json()) as { id?: string };
    return body.id ?? "unknown";
}

/** Compute ISO week label like `W22-2026`. */
function isoWeekLabel(d: Date): string {
    // Per ISO 8601: week 1 contains the year's first Thursday.
    const target = new Date(Date.UTC(d.getUTCFullYear(), d.getUTCMonth(), d.getUTCDate()));
    const dayNr = (target.getUTCDay() + 6) % 7;
    target.setUTCDate(target.getUTCDate() - dayNr + 3);
    const firstThursday = target.getTime();
    target.setUTCMonth(0, 1);
    if (target.getUTCDay() !== 4) {
        target.setUTCMonth(0, 1 + ((4 - target.getUTCDay()) + 7) % 7);
    }
    const week = 1 + Math.ceil((firstThursday - target.getTime()) / 604800000);
    return `W${String(week).padStart(2, "0")}-${d.getUTCFullYear()}`;
}

/** Cron entry-point. Idempotent; safe to invoke manually for smoke (§7 acceptance #5). */
export async function runWeeklyDigest(env: Env): Promise<void> {
    const numbers = await computeThreeNumbers(env);
    const week = isoWeekLabel(new Date());
    const subject = `CoreLink weekly — ${week}`;
    const text = formatDigestText(numbers, week);

    if (!env.RESEND_API_KEY) {
        // No-op: cron remains alive for next week. Log so `wrangler tail`
        // shows the would-have-sent body for operator verification.
        console.log(`[weekly-digest] RESEND_API_KEY missing; skipping send. Body:\n${text}`);
        return;
    }
    try {
        const msgId = await sendViaResend(env, subject, text);
        console.log(`[weekly-digest] sent message=${msgId} env=${env.ENVIRONMENT}`);
    } catch (err) {
        // Never let an email-send error fail the cron — we want the next
        // week's invocation to happen. Log + swallow.
        console.error(`[weekly-digest] send failed: ${(err as Error).message}`);
    }
}
