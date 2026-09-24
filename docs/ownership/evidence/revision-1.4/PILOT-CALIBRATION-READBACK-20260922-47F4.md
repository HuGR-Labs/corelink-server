# Pilot calibration readback after server re-anchor — 2026-09-22

The server source changed at `origin/main@47f4db7` in the Cargo storage-cap
forwarding path. This readback measures the current five artifact sets after
that documentation update. Values are lines / words / bytes.

| Pilot | Profile | Skill | Reference | Blast | Maintenance |
|---|---|---:|---:|---:|---:|
| `corelink-hash` | H | 101 / 781 / 6,934 | 396 / 2,911 / 31,295 | 661 / 5,218 / 56,675 | 202 / 1,868 / 15,837 |
| `corelink-billing` | H | 94 / 659 / 5,093 | 457 / 2,893 / 23,734 | 478 / 3,224 / 28,012 | 262 / 1,802 / 14,623 |
| `corelink-server` | H | 79 / 483 / 3,644 | 284 / 1,943 / 15,254 | 811 / 6,717 / 58,437 | 206 / 1,566 / 12,059 |
| `corelink-cf-bindings` | S | 84 / 587 / 4,881 | 196 / 1,467 / 13,421 | 299 / 2,601 / 24,999 | 121 / 927 / 8,623 |
| `e2e-billing-flow` | S | 74 / 635 / 4,851 | 151 / 1,204 / 10,460 | 158 / 1,375 / 11,872 | 133 / 1,354 / 11,997 |

All five sets remain within declared caps. This is capacity evidence only; the
server byte change invalidates its earlier cold review until an independent
review covers the current four files.
