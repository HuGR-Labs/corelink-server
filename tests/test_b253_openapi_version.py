"""Static, mutation-backed contract for B-253 OpenAPI version authorities."""

from __future__ import annotations

from pathlib import Path
import re

import pytest


ROOT = Path(__file__).resolve().parents[1]
SOURCE_PATH = ROOT / "tools/openapi/src/lib.rs"


def validate_source(source: str) -> None:
    assert re.search(r'pub const SPEC_VERSION: &str = "v1";', source)
    assert 'pub const PACKAGE_VERSION: &str = env!("CARGO_PKG_VERSION");' in source
    assert "assert_eq!(info_version, PACKAGE_VERSION);" in source
    assert "assert_ne!(SPEC_VERSION, PACKAGE_VERSION);" in source
    assert "assert_eq!(PACKAGE_VERSION.split('.').count(), 3);" in source


def test_api_major_and_package_semver_authorities_are_explicit() -> None:
    validate_source(SOURCE_PATH.read_text(encoding="utf-8"))


@pytest.mark.parametrize(
    ("label", "mutate"),
    [
        (
            "conflate-info-with-api-major",
            lambda source: source.replace(
                "assert_eq!(info_version, PACKAGE_VERSION);",
                "assert_eq!(info_version, SPEC_VERSION);",
                1,
            ),
        ),
        (
            "remove-package-authority",
            lambda source: source.replace(
                'pub const PACKAGE_VERSION: &str = env!("CARGO_PKG_VERSION");\n', "", 1
            ),
        ),
        (
            "change-api-major",
            lambda source: source.replace(
                'pub const SPEC_VERSION: &str = "v1";',
                'pub const SPEC_VERSION: &str = "0.1.0";',
                1,
            ),
        ),
    ],
)
def test_authority_mutations_are_rejected(label: str, mutate) -> None:
    source = SOURCE_PATH.read_text(encoding="utf-8")
    with pytest.raises(AssertionError):
        validate_source(mutate(source))
