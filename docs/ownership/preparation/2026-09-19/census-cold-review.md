# PASS

The structural Cargo census matches checkout `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`.

Checks completed:

- Confirmed exact `HEAD`, clean checkout, and unchanged `Cargo.lock`.
- Ran 11 independent metadata commands using Cargo 1.91.1:
  - Root workspace
  - 10 independent fuzz workspaces
  - Flags: `--locked --offline --no-deps --format-version=1`
- Compared all 105 eligible packages:
  - Package names and manifest paths
  - 631 targets, treating absent raw `required-features` as `[]`
  - Feature maps
  - Declared dependencies, including aliases, kinds, target cfgs, optionality, features, default features, requirements, sources, and local manifests
  - 235 independently regenerated reverse declared records using the six requested fields
- Verified summary counts:
  - Root workspace packages: 95
  - Independent fuzz packages: 10
  - Eligible packages: 105
  - Main-workspace targets: 608
  - Main-workspace internal edges: 222
  - Metadata commands succeeded: 11
- Classified all 107 tracked `Cargo.toml` files with no omissions or extras:
  - 95 root-workspace packages
  - 10 independent fuzz packages
  - 1 virtual root
  - 1 archived package
- Confirmed the root is virtual: `[workspace]` with no `[package]`.
- Confirmed `_archive/wi-s11-002-partial/Cargo.toml` declares `corelink-erasure`, has no inbound path dependency among eligible metadata records, and is excluded as historical material.
- Confirmed the archive rationale in `specs/04_sprints/S11/_spec_contract.md`: the archived partial implementation was mined for taxonomy patterns, while canonical implementation moved to `corelink-privacy-erasure-worker` and `corelink-privacy-pseudonymize`.
- No discrepancies found.

Hashes:

- `census.json`: `600105707e9a81c0825092434939116db38c7650ece2826f92ac6ffcf89094ac`
- `summary.json`: `15ec727075f0600dc8bbd97050bc75467ecd7a6a2bf65337933e4315477fe3e3`
- `Cargo.lock`: `c5c353f4dd9f7df3526cb85de018f7a42058bc88c2d94c5d2f47c693af6ef3c7`

Limitations: `--no-deps` validates declared structural metadata only. No resolved dependency graph, builds, tests, runtime behavior, source packets, seeds, semantic claims, documentation-standard approval, issue publication, production behavior, or final-artifact approval was reviewed.