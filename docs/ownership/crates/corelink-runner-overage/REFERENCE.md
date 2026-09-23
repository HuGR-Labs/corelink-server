---
schema: corelink-ownership/1.1
document: reference
package: corelink-runner-overage
manifest: crates/corelink-runner-overage/Cargo.toml
source_commit: 6be030999de1f0e0fe62d3a9abb04ec2a4fefde6
profile: S
state: author_validated
evidence_set: w010-runner-overage-source-static-20260920
---

# corelink-runner-overage — ownership reference

This reference records SOURCE evidence from the manifest and `src/lib.rs` at
the fixed commit. It is an arithmetic contract map, not evidence that a runner
executes, usage is received, a meter is sent, a charge is made, or an external
system is configured. The verified OKF routing reference is
`docs/internal/okf-wiki/concept-manifest.yaml`; it is not revalidated here.

[Identity](#r01) · [Boundary](#r02) · [API](#r03) · [Arithmetic](#r04) · [Axioms](#r05) · [Configuration](#r06) · [Failures](#r07) · [Evidence](#r08)

<a id="r01"></a>
## R01 — Identity and evidence

| Field | Source-defined value |
|---|---|
| Package / manifest | `corelink-runner-overage` / `crates/corelink-runner-overage/Cargo.toml` |
| Source inspected | `src/lib.rs` |
| Declared dependencies | None; `[dependencies]` has no entries |
| Role | Integer-only tier allowance, overage, decimal-rendering, and shadow-charge helpers |
| Evidence class | SOURCE and static manifest only |

Workspace version, edition, rust version, license, publish, and lint settings
are inherited manifest declarations. They do not establish a selected build or
release artifact.

<a id="r02"></a>
## R02 — Boundary and ownership

The crate owns the local `RunnerTier` enum, two constants, four tier methods,
and four free helpers. Callers supply total or overage quantities and, for
`shadow_charge_millicents`, a rate; this source does not own their provenance.
The crate has no source-visible adapter, storage, environment parser, network
client, credential, binary target, or dependency implementation.

<a id="r03"></a>
## R03 — Public source contract

| Symbol | Source-defined contract |
|---|---|
| `SECONDS_PER_VCPU_HOUR` | `u64` constant `3600` |
| `DEFAULT_OVERAGE_RATE_CENTS_PER_VCPU_HOUR` | `u64` constant `20` |
| `RunnerTier` | Non-exhaustive enum: `Starter`, `Pro`, `Team`, `Scale`, `Max` |
| Tier methods | `max_vcpu_h`, `allowance_vcpu_seconds`, `sku`, and `from_sku` |
| Overage helper | `overage_vcpu_seconds(total: u128, tier) -> u128` |
| Render helper | `overage_vcpu_hours_decimal(overage: u128) -> String` |
| Charge helpers | `shadow_charge_millicents(overage: u128, rate: u64) -> u128`; `millicents_to_cents(u128) -> u128` |

`lib.rs` forbids unsafe code. `RunnerTier` is non-exhaustive, so downstream
matches must allow future variants. The root module exports no trait, error
type, configuration object, or I/O surface.

<a id="r04"></a>
## R04 — Static arithmetic and mapping rules

The source maps tiers to `(max_vcpu_h, sku)` as Starter `(100,
runner_starter)`, Pro `(240, runner_pro)`, Team `(600, runner_team)`,
Scale `(1200, runner_scale)`, and Max `(2400, runner_max)`. `from_sku`
returns the corresponding variant only for those exact strings.

Allowance seconds are `max_vcpu_h * 3600`. Overage is
`total_vcpu_seconds.saturating_sub(allowance_vcpu_seconds)`. Decimal rendering
uses `whole = seconds / 3600` and a six-place, floored fractional component
`(seconds % 3600) * 1_000_000 / 3600`. The charge helper returns the floored
integer expression `overage_seconds.saturating_mul(rate).saturating_mul(1000)
/ 3600`; the display helper floors with `millicents / 1000`.

These formulas describe only source behavior for supplied values. Names or
comments containing external terms do not establish any real-world outcome.

<a id="r05"></a>
## R05 — Five falsifiable source axioms

| ID | Predicate tied to inspected source |
|---|---|
| AX-001 | `RunnerTier::allowance_vcpu_seconds()` equals `max_vcpu_h() * SECONDS_PER_VCPU_HOUR` for every current variant. |
| AX-002 | `overage_vcpu_seconds(total, tier)` is zero when `total <= tier.allowance_vcpu_seconds()` and otherwise equals the unsigned difference. |
| AX-003 | `overage_vcpu_hours_decimal(s)` always returns one dot and exactly six fractional digits, with fractional value floored from `s % 3600`. |
| AX-004 | `shadow_charge_millicents(s, r)` uses saturating multiplication before integer division by `3600`; it contains no floating-point operand. |
| AX-005 | For every current tier, `from_sku(tier.sku()) == Some(tier)`; an unmatched string returns `None`. |

These predicates are falsifiable by the named functions. They do not establish
an input source, rate authority, consumer behavior, or external rounding rule.

<a id="r06"></a>
## R06 — Configuration and compatibility

No package-local feature, environment variable, target-specific dependency,
or configuration parser is declared. The two public constants, five tier
variants, SKU strings, function signatures, integer widths, floor points, and
string precision are compatibility-sensitive source surfaces. Manifest
inheritance does not reveal the effective workspace configuration.

<a id="r07"></a>
## R07 — Failure and boundary model

No public `Result` or error enum is defined. Unknown SKU input is represented
by `None`; arithmetic paths use unsigned saturation/flooring rather than a
source-visible error return. `format!` allocates the rendering output. The
source does not establish allocation failure behavior, caller validation,
policy correctness, or any downstream handling of a computed value.

<a id="r08"></a>
## R08 — Evidence, done gate, and unknowns

SOURCE evidence was read at `6be030999de1f0e0fe62d3a9abb04ec2a4fefde6` from
the manifest and `src/lib.rs`. Documentary success is correct identity,
complete public-symbol coverage, five falsifiable axioms, and stated limits.
Done requires the companion blast-radius and maintenance documents, structural
checks, scope-only diff review, and independent review.

Unknowns: all reverse consumers; selected workspace build features; test
execution; callers' input provenance; external price/rate authority; any
runner, provider, meter, ledger, credential, or deployment behavior. No such
claim is made by this reference.

[Back to start](#r01)
