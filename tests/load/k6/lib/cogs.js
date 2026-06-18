// R2 op-class COGS instrumentation for the cache load tests.
//
// WHY this exists
// ---------------
// For a content-addressed cache on Cloudflare R2 the COGS knob is NOT egress
// (R2 egress is $0) — it is the **operation count**, split into two price
// classes:
//
//   - Class A  (mutations: PutObject / CopyObject / ListObjects / …)  $4.50 / 1M
//   - Class B  (reads:     GetObject / HeadObject / …)                $0.36 / 1M
//
// A Class-A op costs 12.5× a Class-B op. So a workload that does many small
// writes (Turbo/sccache PUT storms) is the real margin risk, while a read-heavy
// workload served largely from edge cache is cheap. This module lets a load
// test attribute its synthetic traffic to R2 op-classes and print an estimated
// $ cost — turning "margin is asserted" into "margin is measured".
//
// HONESTY / model scope
// ---------------------
// This is a deliberately **first-order** model, mapping each cache operation to
// the R2 op-class it *typically* drives:
//   - one Class-A per successful write request, and
//   - one Class-B per read MISS (a cache HIT is served from edge ≈ 0 R2 GET).
// Real per-route counts can differ (e.g. a content-addressed write may do a
// dedup HEAD = Class-B before/instead of a PutObject; a range GET is still one
// Class-B). Treat the output as an order-of-magnitude COGS signal for catching
// op-storm regressions, not a billing oracle. The caller decides the mapping.

import { Counter } from 'k6/metrics';

// $ per single R2 operation (Cloudflare R2 standard pricing).
export const PRICE_CLASS_A_PER_OP = 4.5 / 1e6;
export const PRICE_CLASS_B_PER_OP = 0.36 / 1e6;

export const r2ClassA = new Counter('r2_class_a_ops');
export const r2ClassB = new Counter('r2_class_b_ops');

// Caller maps each cache op to its R2 op-class.
export function recordClassA(n = 1) { r2ClassA.add(n); }
export function recordClassB(n = 1) { r2ClassB.add(n); }

// Compute the COGS block from a k6 handleSummary `data` object.
export function cogsBlock(data) {
  const a = (data.metrics.r2_class_a_ops && data.metrics.r2_class_a_ops.values.count) || 0;
  const b = (data.metrics.r2_class_b_ops && data.metrics.r2_class_b_ops.values.count) || 0;
  const costA = a * PRICE_CLASS_A_PER_OP;
  const costB = b * PRICE_CLASS_B_PER_OP;
  const total = costA + costB;
  const ops = a + b;
  // Normalize to a stable "$ per 1M cache ops" figure for run-over-run COGS comparison.
  const perMillionOps = ops > 0 ? (total / ops) * 1e6 : 0;
  return { classA: a, classB: b, costA, costB, total, perMillionOps };
}

// A human-readable COGS block for the console / summary.
export function formatCogs(data) {
  const c = cogsBlock(data);
  return [
    '',
    '  ── R2 COGS (op-class model; R2 egress is $0 — op-count is the knob) ──',
    `  Class-A ops (writes: Put/Copy/List)      : ${c.classA}  → $${c.costA.toFixed(4)}`,
    `  Class-B ops (reads:  Get/Head, on MISS)  : ${c.classB}  → $${c.costB.toFixed(4)}`,
    `  R2 op-cost this run                      : $${c.total.toFixed(4)}`,
    `  Normalized COGS                          : $${c.perMillionOps.toFixed(4)} / 1M cache ops`,
    '  NOTE: first-order — 1 Class-A per write, 1 Class-B per read-MISS (a cache',
    '  HIT serves from edge ≈ 0 R2 GET). Dedup HEADs / range reads can shift this.',
    '',
  ].join('\n');
}
