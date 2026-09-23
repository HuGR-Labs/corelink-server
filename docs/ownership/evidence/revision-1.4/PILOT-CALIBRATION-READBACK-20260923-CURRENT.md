# Pilot calibration current-byte readback — 2026-09-23

This readback supersedes earlier calibration tables for the five pilot artifacts
in this checkout. All twenty artifact counts below were measured with
`wc -l -w -c` from the current worktree after server REL-088–098 were added.
Counts are lines / words / bytes. Population values are taken from the
current pilot documents and cited census evidence; they are documentary counts,
not runtime or deployment claims.

| Pilot | Profile | Skill L/W/B | Reference L/W/B | Blast L/W/B | Maintenance L/W/B |
|---|---|---:|---:|---:|---:|
| `corelink-hash` | H | 101 / 775 / 6,909 | 396 / 2,911 / 31,295 | 724 / 6,023 / 63,678 | 202 / 1,868 / 15,837 |
| `corelink-billing` | H | 94 / 659 / 5,093 | 486 / 3,982 / 39,798 | 478 / 3,224 / 28,012 | 262 / 1,802 / 14,623 |
| `corelink-server` | H | 79 / 484 / 3,656 | 306 / 2,287 / 17,888 | 1,199 / 10,351 / 95,319 | 237 / 1,915 / 15,125 |
| `corelink-cf-bindings` | S | 84 / 587 / 4,881 | 321 / 2,932 / 29,152 | 299 / 2,601 / 24,999 | 121 / 927 / 8,623 |
| `e2e-billing-flow` | S | 74 / 635 / 4,851 | 151 / 1,204 / 10,460 | 159 / 1,398 / 12,134 | 133 / 1,354 / 11,997 |

## Populations and capacity decisions

| Pilot | Material population in current artifacts | Overflow | Capacity decision |
|---|---|---|---|
| `corelink-hash` | 43 blast-radius relation records; 5 implementation source files; 6 maintenance procedures ([B02](../../crates/corelink-hash/BLAST_RADIUS.md#b02), [M03](../../crates/corelink-hash/MAINTENANCE.md#m03)). | NO | H is justified by more than 40 recorded relations. All four artifacts also fit S by size; the profile remains H because the population trigger governs. |
| `corelink-billing` | 27 blast-radius relation records; 49 source-visible semantic module declarations across 47 non-test source files; 7 maintenance procedures ([B03](../../crates/corelink-billing/BLAST_RADIUS.md#b03), [module census](BILLING-MODULE-CENSUS-20260922.md), [M03](../../crates/corelink-billing/MAINTENANCE.md#m03)). | NO | H is justified by more than 20 semantic modules. All four artifacts fit H; the reference exceeds S. |
| `corelink-server` | 97 blast-radius relation records including 11 source-only customer method peers; 401 Rust files under `src/`; 14 Rust files under `tests/`; 31 declared first-party dependencies; 6 maintenance procedures ([B02](../../crates/corelink-server/BLAST_RADIUS.md#b02), [R01](../../crates/corelink-server/REFERENCE.md#r01), [M03](../../crates/corelink-server/MAINTENANCE.md#m03)), pinned to `91630ba`. | NO | H is justified by more than 40 recorded relations. All four artifacts fit H; the blast document exceeds S. |
| `corelink-cf-bindings` | 39 atomic blast-radius relations; 16 Cargo dependency declarations (9 normal/target-specific and 7 host dev); 5 maintenance procedures ([B02](../../crates/corelink-cf-bindings/BLAST_RADIUS.md#b02), [M03](../../crates/corelink-cf-bindings/MAINTENANCE.md#m03)). | NO | S is justified: the documented populations do not meet any H trigger (>40 relations, >20 semantic modules, or >8 procedures). All four artifacts fit S. |
| `e2e-billing-flow` | 9 semantic blast-radius relation records; 5 maintenance procedures ([B03](../../crates/e2e-billing-flow/BLAST_RADIUS.md#b03), [M03](../../crates/e2e-billing-flow/MAINTENANCE.md#m03)). | NO | S is justified: the documented populations do not meet any H trigger. All four artifacts fit S. |

`NO` means only that measured artifact bytes fit the selected profile caps: S
uses 180/1,200/12 KiB for the skill, 400/3,000/36 KiB for the reference,
900/7,000/80 KiB for blast radius, and 400/3,000/36 KiB for maintenance; H
uses the same skill caps, then 700/5,500/64 KiB, 1,800/14,000/160 KiB, and
700/5,500/64 KiB for the other artifacts. It does not assert semantic
completeness. No pilot overflows its declared profile. The five capacity
decisions are therefore retain H for hash, billing and server, and retain S for
cf-bindings and e2e-billing-flow.

This is capacity evidence only. It does not approve any pilot, freeze the
standard, prove runtime reachability, or authorize issue publication. Cold
review verdicts, peer reconciliation, source drift, runtime evidence, and
remaining semantic findings stay as separate gates in the revision-1.4 record.
