### Fixed

- **Stopped public docs and `corelink version` from advertising non-existent B-166 SLSA/Sigstore release provenance.** The text and JSON outputs omit the fabricated URL until a published, independently retrievable bundle exists; the JSON schema records the missing-field compatibility contract and real build-metadata fallbacks. Security, pricing, installation, CI, Trust Center, and SBOM-access locales now describe the shipped `.sha256` checksums, make no current signed-binary/Worker/SBOM claim, and distinguish root `--version` (SemVer only) from the `version` metadata subcommand.
