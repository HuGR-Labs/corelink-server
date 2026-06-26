---
type: "ComplianceControl"
title: "Dual MIT/Apache-2.0 licensing"
description: "CoreLink is offered under the standard permissive dual license; both texts ship at the repo root."
source_files:
  - "LICENSE-MIT"
  - "LICENSE-APACHE-2.0"
checkpoint_sha: "6ac15839857761aecf0ecb7edd87051026dfc4b5"
provenance: "AUTHORED"
tags: ["compliance", "licensing"]
timestamp: "2026-06-26T00:00:00Z"
---

# Dual MIT/Apache-2.0 licensing

CoreLink ships under the conventional permissive dual license: a downstream
consumer may take the code under MIT or under Apache-2.0, at their option. Both
full license texts live at the repository root and are the authoritative grant —
this concept exists so the compliance surface has a grounded anchor to the exact
files a redistribution audit must reproduce.

# Role
The dual license is the legal envelope around every published artifact. It is
the thing a packager, an OSS-compliance scanner, or an acquirer's counsel reads
first.

# How it works
- The MIT grant is declared by its SPDX identifier on the first line of the MIT
  text (`LICENSE-MIT:1`).
- The Apache-2.0 grant is carried in the second license file at the root
  (`LICENSE-APACHE-2.0:1`), so a consumer can elect either grant.

# Invariants
- Both license files MUST remain present and unmodified at the repo root
  (`LICENSE-MIT:1`; `LICENSE-APACHE-2.0:1`).

# Gotchas
- The "OR" semantics matter: a redistributor picks ONE license; they do not have
  to satisfy both simultaneously.

# Citations
1. `LICENSE-MIT:1` — the MIT SPDX grant line.
2. `LICENSE-APACHE-2.0:1` — the Apache-2.0 grant file.
