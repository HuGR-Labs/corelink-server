# Pilot calibration current-byte readback — 2026-09-22

The historical calibration remains preserved. This readback measures the four
artifact paths at the current campaign HEAD before this evidence commit.

| Pilot | Profile | Skill | Reference | Blast | Maintenance |
|---|---|---:|---:|---:|---:|
| `corelink-hash` | H | 101 / 781 / 6,934 | 396 / 2,911 / 31,295 | 661 / 5,218 / 56,675 | 202 / 1,868 / 15,837 |
| `corelink-billing` | H | 94 / 659 / 5,093 | 457 / 2,893 / 23,734 | 478 / 3,224 / 28,012 | 262 / 1,802 / 14,623 |
| `corelink-server` | H | 79 / 483 / 3,644 | 265 / 1,764 / 13,753 | 797 / 6,570 / 57,197 | 204 / 1,511 / 11,669 |
| `corelink-cf-bindings` | S | 84 / 587 / 4,881 | 196 / 1,467 / 13,421 | 299 / 2,601 / 24,999 | 121 / 927 / 8,623 |
| `e2e-billing-flow` | S | 74 / 635 / 4,851 | 151 / 1,204 / 10,460 | 158 / 1,375 / 11,872 | 122 / 1,295 / 11,389 |

Values are lines / words / bytes. All remain within their declared S/H caps;
the current CF and server counts include the target-specific audit and
current-main source corrections recorded after previous readbacks. This is
capacity evidence only: it does not approve the pilots, freeze the standard,
or prove runtime reachability.
