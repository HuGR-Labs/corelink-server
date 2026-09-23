# Ownership wave 013 — CLI, data-transfer, OpenAPI and SBOM tools

Source/static-only four-artifact contract. The verified OKF is the canonical
reference: apply it as routing only; do not copy, revalidate or redefine it.

| Package | Manifest | Profile |
|---|---|---|
| `corelink-cli` | `tools/cli/Cargo.toml` | S |
| `corelink-dt-cli` | `tools/dt-cli/Cargo.toml` | S |
| `corelink-dt-reconcile` | `tools/dt-reconcile/Cargo.toml` | S |
| `corelink-dt-webhook` | `crates/corelink-dt-webhook/Cargo.toml` | S |
| `corelink-openapi` | `tools/openapi/Cargo.toml` | S |
| `sbom-publish` | `tools/sbom-publish/Cargo.toml` | S |

Commands, webhook names, OpenAPI and SBOM targets are source/build surfaces.
They do not prove a CLI invocation, transfer, webhook delivery, publication,
release, provider state or runtime behavior.
