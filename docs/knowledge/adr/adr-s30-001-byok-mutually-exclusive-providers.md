---
type: "ADR"
title: "ADR-S30-001 — BYOK real providers: mutually-exclusive compile-time cargo features"
description: "Ratifies that the BYOK orchestrator enforces exactly one real KMS provider per binary via pairwise compile_error! macros, making the --all-features build fail by design and CI run a per-provider matrix instead."
source_files:
  - "specs/03_architecture/adrs/ADR-S30-001-byok-mutually-exclusive-providers.md"
checkpoint_sha: "10218d5bf423d6666228c796ee4118222f3456d7"
provenance: "AUTHORED"
tags: ["adr", "storage", "byok", "kms", "cargo-features", "compile-error", "fips", "s30"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-S30-001 — BYOK real providers: mutually-exclusive compile-time cargo features

CoreLink ships BYOK envelope encryption across four KMS providers (AWS, GCP, Azure, Vault), but the orchestrator exposes exactly one provider trait object per binary, resolved at compile time. This ADR promotes the long-standing `compile_error!` design exception into a first-class decision: enabling two real-provider features is a hard compile error, so `cargo build --workspace --all-features` fails by design and CI runs a per-provider build matrix instead. It exists so reviewers, `/techlead` automation, and future contributors stop re-discovering the `--all-features` failure as a regression. Related: [BYOK envelope encryption](/storage/byok-envelope-encryption.md).

# Context

Each real provider links a fundamentally different SDK + TLS stack + credential resolver, so linking all four into one binary triples the supply-chain attack surface, sprawls the FIPS-validated-component matrix (a four-provider binary advertises a FIPS posture it does not have), and forks the per-provider BLAKE3 audit chain across up to four trust roots — and a wave-18 incident showed a verifier falsely reporting `--all-features: pass` while the build had been structurally broken by the orchestrator's guard, as recorded at `specs/03_architecture/adrs/ADR-S30-001-byok-mutually-exclusive-providers.md:41-108`.

# Decision

The orchestrator enforces exactly one real-provider feature per binary (zero yields the in-memory fake; two or more is a hard compile error) implemented as six pairwise `compile_error!` macros for diagnostic precision; `cargo build --workspace --all-features` therefore fails by design and CI runs a five-config per-provider matrix instead, and this ADR is the authoritative `/techlead` AP-11 ratification cite — recorded across the decision section `specs/03_architecture/adrs/ADR-S30-001-byok-mutually-exclusive-providers.md:110-187`. Runtime provider selection and dynamic factory patterns were rejected because they link all four SDKs and lose the load-bearing compile-time "this binary IS the AWS binary" invariant.

# Consequences

There is a single canonical reference for the `--all-features` failure mode, a compile-time guarantee that production binaries link exactly one provider (FIPS-matrix and audit-chain integrity preserved), at the accepted cost that `--all-features` fails by design and CI pays a five-config matrix per touching commit, with per-tenant provider mixing explicitly unsupported at the binary level (multi-binary routing is the GA shape), per `specs/03_architecture/adrs/ADR-S30-001-byok-mutually-exclusive-providers.md:189-223`.

# Citations

1. `specs/03_architecture/adrs/ADR-S30-001-byok-mutually-exclusive-providers.md:41-108` — the 4-provider surface, why runtime multi-provider is unsafe (surface bloat, FIPS, audit-chain fork), and the wave-18 incident (Context).
2. `specs/03_architecture/adrs/ADR-S30-001-byok-mutually-exclusive-providers.md:110-187` — D1-D5: one-feature rule, six pairwise compile_error! macros, --all-features fails by design, per-provider CI matrix, AP-11 cite.
3. `specs/03_architecture/adrs/ADR-S30-001-byok-mutually-exclusive-providers.md:189-223` — positive/accepted consequences and the not-supported per-tenant-mixing note.
