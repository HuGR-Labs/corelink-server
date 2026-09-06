#!/usr/bin/env python3
"""Regression contract for the public ``corelink version`` documentation.

The CLI has no published, independently retrievable attestation bundle today.
Its public reference, locale mirrors, and installation copy must therefore
describe the emitted build metadata and the absent-now/optional-later contract,
never a fabricated URL or JSON field.
"""

from __future__ import annotations

import re
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parent.parent
DOCS_ROOT = REPO_ROOT / "apps" / "docs"
JSON_SCHEMA_DOC = REPO_ROOT / "docs" / "cli" / "json-output-schema.md"
LOCALES = ("de", "es-419", "pt-BR")
VERSION_DOCS = (
    DOCS_ROOT / "docs/reference/cli/version.mdx",
    *(
        DOCS_ROOT
        / "i18n"
        / locale
        / "docusaurus-plugin-content-docs/current/reference/cli/version.mdx"
        for locale in LOCALES
    ),
)
CLI_INDEX_DOCS = (
    DOCS_ROOT / "docs/reference/cli/index.mdx",
    *(
        DOCS_ROOT
        / "i18n"
        / locale
        / "docusaurus-plugin-content-docs/current/reference/cli/index.mdx"
        for locale in LOCALES
    ),
)
INSTALLATION_DOCS = (
    DOCS_ROOT / "docs/tutorial/01-installation.mdx",
    *(
        DOCS_ROOT
        / "i18n"
        / locale
        / "docusaurus-plugin-content-docs/current/tutorial/01-installation.mdx"
        for locale in LOCALES
    ),
)
SDK_GUIDE_DOCS = (
    *(
        DOCS_ROOT / "docs/how-to/sdk-cli" / name
        for name in ("01-authenticate.mdx", "06-ci-integration.mdx")
    ),
    *(
        DOCS_ROOT
        / "i18n"
        / locale
        / "docusaurus-plugin-content-docs/current/how-to/sdk-cli"
        / name
        for locale in LOCALES
        for name in ("01-authenticate.mdx", "06-ci-integration.mdx")
    ),
)
RELEASE_SECURITY_DOCS = (
    DOCS_ROOT / "docs/explanation/security/index.mdx",
    *(
        DOCS_ROOT
        / "i18n"
        / locale
        / "docusaurus-plugin-content-docs/current/explanation/security/index.mdx"
        for locale in LOCALES
    ),
)
RELEASE_PRICING_DOCS = (
    DOCS_ROOT / "docs/pricing/comparison.mdx",
    *(
        DOCS_ROOT
        / "i18n"
        / locale
        / "docusaurus-plugin-content-docs/current/pricing/comparison.mdx"
        for locale in LOCALES
    ),
)
TRUST_DOCS = (
    *(
        DOCS_ROOT / "docs/trust" / name
        for name in ("compliance.mdx", "iso27001.mdx", "fedramp-info.mdx", "index.mdx")
    ),
    *(
        DOCS_ROOT
        / "i18n"
        / locale
        / "docusaurus-plugin-content-docs/current/trust"
        / name
        for locale in LOCALES
        for name in ("compliance.mdx", "iso27001.mdx", "fedramp-info.mdx", "index.mdx")
    ),
)
SBOM_DOCS = (
    DOCS_ROOT / "docs/explanation/compliance/sbom-access.mdx",
    *(
        DOCS_ROOT
        / "i18n"
        / locale
        / "docusaurus-plugin-content-docs/current/explanation/compliance/sbom-access.mdx"
        for locale in LOCALES
    ),
)
PUBLIC_RELEASE_DOCS = (
    *SDK_GUIDE_DOCS,
    *RELEASE_SECURITY_DOCS,
    *RELEASE_PRICING_DOCS,
    *TRUST_DOCS,
    *SBOM_DOCS,
)
FABRICATED_TOKENS = ("attest.corelink.humangr.com", "slsa_attestation_url")
SIGNING_TERMS = (
    "signed",
    "signature",
    "signing",
    "signiert",
    "signierte",
    "firma",
    "firmado",
    "firmada",
    "assinatura",
    "assinado",
    "assinada",
)
SIGNING_RE = re.compile(
    r"\b(?:signed|signature|signing|signiert\w*|firm\w*|assin\w*)\b",
    flags=re.IGNORECASE,
)
TRUST_MECHANISM_RE = re.compile(
    r"\b(?:slsa(?:\s+l\d)?|sigstore|cosign|rekor|attestation|provenance)\b",
    flags=re.IGNORECASE,
)
RELEASE_CONTEXT_RE = re.compile(
    r"\b(?:release|releases|binary|binaries|binar\w*|binári\w*|"
    r"asset|assets|worker|container|deploy\w*|lanç\w*|artefat\w*|ejecut\w*)\b",
    flags=re.IGNORECASE,
)
EXPLICIT_ABSENCE_BEFORE_RE = re.compile(
    r"\b(?:not|no|never|without|isn't|aren't|nicht|kein\w*|keine\w*|"
    r"não|nao|nunca|sem|sin|ningún|ninguna)\W+"
    r"(?:(?:a|an|the|currently|yet|é|e)\W+)?"
    r"(?:signed|signature|signing|signiert\w*|firm\w*|assin\w*|"
    r"slsa(?:\s+l\d)?|sigstore|cosign|rekor|attestation|provenance)"
    r"(?:\W+(?:signed|signature|signing|signiert\w*|firm\w*|assin\w*|"
    r"slsa(?:\s+l\d)?|sigstore|cosign|rekor|attestation|provenance))*\W*$",
    flags=re.IGNORECASE,
)
EXPLICIT_ABSENCE_AFTER_RE = re.compile(
    r"(?:^[^\n.;|]{0,100}\b(?:is|are|has|have)?\W*"
    r"(?:not|never)\W+(?:(?:currently|yet|still)\W+)?"
    r"(?:part|live|published|offered|available|created|distributed|run|"
    r"a\W+customer-facing\W+artifact|a\W+customer\W+artifact|"
    r"a\W+public\W+artifact|a\W+public\W+release)\b"
    r"|\bnot\W+(?:a\W+|an\W+|the\W+)?"
    r"(?:release|binary|asset|worker|container)\W+artifact\b"
    r"|\bnot\W+live\b"
    r"|\b(?:is|are)\W+still\W+a\W+placeholder\b"
    r"|\b(?:is|are)\W+not\W+(?:a\W+)?(?:release|binary|asset|worker|container)\b"
    r"|\b(?:não|nao)\W+(?:é|e)\W+(?:uma?\W+)?(?:assinatura|firma)\W+de\W+release\b)",
    flags=re.IGNORECASE,
)


def _unqualified_release_signing_promise(text: str) -> re.Match[str] | None:
    """Find a current release/worker signing promise, allowing explicit gaps.

    The context window intentionally spans whitespace and punctuation so a
    claim split across Markdown lines cannot evade the guard. Explicit
    negative language (for example, ``not signed`` or ``signing ... not live``)
    is truthful posture, while ``every release ... firma`` remains a failure.
    """
    claims = (*SIGNING_RE.finditer(text), *TRUST_MECHANISM_RE.finditer(text))
    for claim in claims:
        for context in RELEASE_CONTEXT_RE.finditer(text):
            if abs(claim.start() - context.start()) > 140:
                continue
            between = text[min(claim.end(), context.end()) : max(claim.start(), context.start())]
            if "|" in between:
                # Markdown table rows commonly contain unrelated signed
                # policy/document names next to a release column.
                continue
            claim_before = text[max(0, claim.start() - 55) : claim.end()]
            claim_after = text[claim.start() : min(len(text), claim.end() + 105)]
            if EXPLICIT_ABSENCE_BEFORE_RE.search(claim_before):
                continue
            if EXPLICIT_ABSENCE_AFTER_RE.search(claim_after):
                continue
            return claim
    return None


def _text(path: Path) -> str:
    assert path.is_file(), f"missing public CLI document: {path.relative_to(REPO_ROOT)}"
    return path.read_text(encoding="utf-8")


def test_version_reference_has_exact_current_output_and_optional_future_contract():
    """Every locale reference preserves the four emitted keys and no artifact key."""
    expected_keys = ("version", "git_rev", "build_timestamp", "target_triple")
    for path in VERSION_DOCS:
        text = _text(path)
        for key in expected_keys:
            assert f'"{key}"' in text, path
        for forbidden in (*FABRICATED_TOKENS, '"slsa_attestation"'):
            assert forbidden not in text, path
        normalized = text.lower()
        assert "intentionally absent" in normalized, path
        assert "future published bundle" in normalized, path
        assert "optional field" in normalized, path
        assert "must not\n  derive an artifact url" in normalized, path
        assert "source_date_epoch" in normalized, path
        assert "git_commit_sha" in normalized, path
        assert "target`" in normalized, path
        assert "unknown" in normalized, path


def test_json_schema_omits_unpublished_attestation_field_and_url():
    """The machine-readable schema cannot preserve the removed wire claim."""
    text = _text(JSON_SCHEMA_DOC)
    assert '"slsa_attestation"' not in text
    assert "corelink.humangr.com/attestations" not in text
    normalized = text.lower()
    assert "intentionally absent" in normalized
    assert "future schema version" in normalized
    assert "optional artifact field" in normalized


def test_cli_indexes_and_installation_mirrors_describe_build_metadata_not_slsa():
    """Indexes and quick-start mirrors cannot reintroduce the removed claim."""
    for path in (*CLI_INDEX_DOCS, *INSTALLATION_DOCS):
        text = _text(path).lower()
        assert "slsa" not in text, path
        for token in FABRICATED_TOKENS:
            assert token not in text, path
        assert "build metadata" in text, path


def test_sdk_guides_require_checksum_and_make_no_unverifiable_provenance_claim():
    """Install/auth guides must describe the shipped checksum, not SLSA theater."""
    forbidden = ("slsa", "attest", "provenance", *SIGNING_TERMS)
    for path in SDK_GUIDE_DOCS:
        text = _text(path).lower()
        for token in forbidden:
            assert token not in text, (token, path)
        assert ".sha256" in text, path


def test_complete_public_release_doc_population_has_only_current_checksum_contract():
    """Every public binary-release surface is scanned, not just one canonical locale."""
    forbidden = ("slsa", "sigstore", "provenance", "attest", "cosign")
    assert len(TRUST_DOCS) == 16
    assert len(SBOM_DOCS) == 4
    assert len(PUBLIC_RELEASE_DOCS) == 36
    for path in PUBLIC_RELEASE_DOCS:
        text = _text(path).lower()
        # Trust pages may name an unavailable mechanism only in an explicit
        # negative statement; release/security/pricing surfaces must not.
        if path not in TRUST_DOCS:
            for token in forbidden:
                assert token not in text, (token, path)
        assert ".sha256" in text, path
        assert not _unqualified_release_signing_promise(text), path
    for path in (*RELEASE_SECURITY_DOCS, *RELEASE_PRICING_DOCS):
        text = _text(path).lower()
        assert "current public release contract" in text, path
        assert "future independently retrievable bundle" in text, path


def test_release_signing_mutations_are_red_across_trust_surface_and_locales():
    """The semantic tooth catches reviewed EN/ES/PT and multiline mutations."""
    mutations = (
        "Every release is signing its binary.",
        "Cada release publica una firma del binario.",
        "Cada lançamento publica uma assinatura do binário.",
        "Worker container\nimage is signed on every release.",
        "Signing is not optional for every release binary.",
        "Not only the checksum: the signed binary ships with every release.",
        "No solo la firma del binario se publica en cada release.",
        "A assinatura não opcional aparece em cada release.",
        "Not just the checksum; every release asset is signed.",
    )
    for mutant in mutations:
        assert _unqualified_release_signing_promise(mutant), mutant

    # Exact review mutations for each newly covered trust surface. These are
    # deliberately positive claims; the live pages use checksums or explicit
    # unavailable/not-live qualification instead.
    trust_surface_mutations = {
        "trust/compliance": "Rekor transparency-log entries for every release.",
        "trust/iso27001": "Cosign-signed releases are published.",
        "trust/fedramp-info": "SLSA L3 signed deploys are live.",
        "trust/index": "Worker container image — signed keyless on every release.",
    }
    assert set(trust_surface_mutations) == {
        "trust/compliance",
        "trust/iso27001",
        "trust/fedramp-info",
        "trust/index",
    }
    for mutant in trust_surface_mutations.values():
        assert _unqualified_release_signing_promise(mutant), mutant

    for legitimate in (
        "Every signed payload is canonicalized before signing.",
        "The audit signature is verified offline; it is not a release artifact.",
        "Assinatura de eventos auditáveis não é assinatura de release.",
    ):
        assert not _unqualified_release_signing_promise(legitimate), legitimate


def test_global_version_flag_is_not_claimed_equivalent_to_metadata_subcommand():
    """Clap's root --version prints only SemVer; `version` prints metadata too."""
    for path in CLI_INDEX_DOCS:
        text = _text(path)
        assert "Same as the [`version`](./version.mdx) subcommand" not in text, path
        assert "package version" in text, path
        assert "build metadata" in text, path


def test_published_docs_contain_no_fabricated_host_or_version_slsa_promise():
    """A new public URL or version→SLSA claim is a contract regression."""
    public_docs = tuple(
        path
        for path in DOCS_ROOT.rglob("*")
        if path.suffix in {".md", ".mdx", ".html", ".htm"}
        and "node_modules" not in path.parts
        and "build" not in path.parts
    )
    assert public_docs, "public-doc scan must not become vacuous"
    for path in public_docs:
        text = path.read_text(encoding="utf-8")
        lowered = text.lower()
        for token in FABRICATED_TOKENS:
            assert token not in lowered, path
        assert not re.search(
            r"corelink version.{0,120}slsa|slsa.{0,120}corelink version|"
            r"print version.{0,80}slsa|version \+ git rev \+ slsa",
            lowered,
            flags=re.DOTALL,
        ), path
