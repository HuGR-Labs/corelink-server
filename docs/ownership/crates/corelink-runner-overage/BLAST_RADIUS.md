---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-runner-overage
manifest: crates/corelink-runner-overage/Cargo.toml
source_commit: 6be030999de1f0e0fe62d3a9abb04ec2a4fefde6
profile: S
state: draft
evidence_set: w010-runner-overage-source-static-20260920
---

# corelink-runner-overage — blast radius

This is a SOURCE-only map of atomic relations in the package. A relation names
an in-module dependency, input/output transformation, or review impact. It
does not prove a caller runs, a runner operates, a meter is delivered, a
charge is applied, or an external provider has any state.

[Scope](#b01) · [Tier map](#b02) · [Unit arithmetic](#b03) · [Formatting](#b04) · [Public surface](#b05) · [Unknowns](#b06)

<a id="b01"></a>
## B01 — Scope and reading rules

Record index: [REL-001](#rel-001) · [REL-002](#rel-002) · [REL-003](#rel-003) · [REL-004](#rel-004) · [REL-005](#rel-005) · [REL-006](#rel-006) · [REL-007](#rel-007)

The owned surface is the manifest plus `src/lib.rs`. The manifest has no direct
or development dependency entry, and the source consists of constants, enum,
functions, and local tests. A textual mention outside this scope is not a
reverse-consumer proof. The verified OKF reference routes this pure leaf away
from broader narratives; it adds no relation or operational evidence.

<a id="b02"></a>
## B02 — Tier and SKU relations

<a id="rel-001"></a>
### REL-001 — Variant-to-allowance relation

**Dependency:** `RunnerTier::allowance_vcpu_seconds` depends on
`max_vcpu_h` and `SECONDS_PER_VCPU_HOUR`. **Flow:** current variant → whole
hour allowance → multiplication by `3600` → `u64` seconds. **Impact:** changing
a tier value, conversion constant, or integer type changes the subtraction
threshold in REL-003. **Evidence:** SOURCE `src/lib.rs`. **Limit:** no
entitlement, subscription, or runner state is established. [Index](#b02) [Relation index](#b03)

<a id="rel-002"></a>
### REL-002 — Variant-to-SKU relation

**Dependency:** `sku` and `from_sku` each match the current enum variants.
**Flow:** variant → exact static string; an exact static string → `Some(variant)`;
all other strings → `None`. **Impact:** changing a string or variant can break
source-level round trips and static callers. **Evidence:** SOURCE `src/lib.rs`.
**Limit:** no external catalog or metadata write is proven. [Index](#b02) [Relation index](#b03)

<a id="b03"></a>
## B03 — Integer-unit relations

<a id="rel-003"></a>
### REL-003 — Total-to-overage relation

**Dependency:** `overage_vcpu_seconds` consumes caller-supplied `u128` total
and a tier-derived `u64` allowance. **Flow:** total and widened allowance enter
`u128::saturating_sub`. **Impact:** changing the threshold, saturation, or
units changes returned source values for boundary inputs. **Evidence:** SOURCE
`src/lib.rs`. **Limit:** the package does not establish how totals are measured
or used. [Index](#b03)

<a id="rel-004"></a>
### REL-004 — Overage-to-millicent relation

**Dependency:** `shadow_charge_millicents` consumes caller-supplied seconds
and rate plus `SECONDS_PER_VCPU_HOUR`. **Flow:** seconds × rate × 1000 uses two
saturating `u128` multiplies, then integer-divides by 3600. **Impact:** changing
operand order, saturation, scale, or division changes source-level floors and
large-input behavior. **Evidence:** SOURCE `src/lib.rs`. **Limit:** neither a
rate authority nor a financial outcome is established. [Index](#b03)

<a id="b04"></a>
## B04 — Rendering relation

<a id="rel-005"></a>
### REL-005 — Seconds-to-decimal-string relation

**Dependency:** `overage_vcpu_hours_decimal` depends on the 3600 constant.
**Flow:** whole quotient and floored six-digit fractional quotient are rendered
by `format!` as `whole.frac`. **Impact:** changing precision, floor behavior,
or separator changes output strings and consumer compatibility. **Evidence:**
SOURCE `src/lib.rs`. **Limit:** no external parser or accepted format is known.
[Index](#b04) [Relation index](#b03)

<a id="rel-006"></a>
### REL-006 — Millicent-to-cent display relation

**Dependency:** `millicents_to_cents` accepts the output unit of REL-004.
**Flow:** supplied `u128` → integer division by `1000` → floored `u128`.
**Impact:** altering divisor or rounding changes display-level source values.
**Evidence:** SOURCE `src/lib.rs`. **Limit:** no stored record, presentation,
or settlement behavior is proven. [Index](#b04) [Relation index](#b03)

<a id="b05"></a>
## B05 — Public compatibility relation

<a id="rel-007"></a>
### REL-007 — Crate-root export relation

**Dependency:** `lib.rs` defines all public constants, enum methods, and
helpers directly at the crate root. **Flow:** a compile-time consumer may name
these public paths. **Impact:** removals, renamed SKU values, changed enum
variants, signatures, widths, or formatting can be source-breaking. **Evidence:**
SOURCE `src/lib.rs`. **Limit:** no complete reverse dependency graph or
compiled consumer is established. [Index](#b05) [Relation index](#b03)

<a id="b06"></a>
## B06 — Coverage and explicit unknowns

Relation coverage is limited to seven atomic relations in the inspected
manifest/module packet. Unknowns are reverse consumers, selected features and
targets, test execution, input provenance, rate policy, external formatting
requirements, and every runtime/provider/deployment behavior. Reconsider each
relation before modifying its named symbol; do not convert an impact review
obligation into an assertion of external operation.

Known reverse consumer: **RC-001** — the `corelink-runner-aggregate` manifest
declares this crate as a dependency, and its `src/lib.rs` imports
`RunnerTier` and the overage/formatting helpers into the aggregation module.
Activation: the consumer package/module must be selected, and its aggregation
path must call those helpers. Evidence: `crates/corelink-runner-aggregate/Cargo.toml`;
`crates/corelink-runner-aggregate/src/lib.rs`. Limit: this declaration and
source use do not prove an aggregate run, metering, billing, or Stripe contact;
Cargo target/feature resolution and runtime invocation remain unknown.

[Reference](REFERENCE.md#r01) · [Maintenance](MAINTENANCE.md#m01) · [Ownership guide](../../../../.claude/skills/own-corelink-runner-overage/SKILL.md#s01)
