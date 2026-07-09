---
type: "ComplianceControl"
title: "Bad SHA"
description: "A grounded license anchor used by the validator fixtures."
source_files:
  - "LICENSE-MIT"
# MALFORMED (non-40-hex) checkpoint — the surviving C4 hard-fail (format).
# NB: a well-formed-but-UNREACHABLE 40-hex (e.g. the old `ffff…` value) is NO
# LONGER a C4 failure — it is a squash-orphan, tolerated with a warning while C5
# re-anchors freshness to the base ref (fix(okf): squash-merge resilience). The
# orphan-tolerance + preserved-freshness path is proven by the
# `assert_c4_squash_orphan_tolerant` git harness in run_fixtures.sh.
checkpoint_sha: "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz"
provenance: "AUTHORED"
tags: ["compliance"]
---

# Bad SHA

Lead paragraph establishing the license file as the grounded anchor this
fixture concept rests on.

# How it works
- The MIT SPDX grant is on the first line (`LICENSE-MIT:1`).

# Invariants
- The license file stays present at the repo root (`LICENSE-MIT:1`).

# Citations
1. `LICENSE-MIT:1` — the MIT grant line.
