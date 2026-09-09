"""Keep B-118 retirement claims aligned with the integration change date."""
from __future__ import annotations

from pathlib import Path

import pytest


ROOT = Path(__file__).resolve().parents[1]
RETIREMENT_DATE = "2026-09-08"
STALE_DATE = "2026-08-31"

RETIREMENT_DOCS = (
    ROOT / "BACKLOG.md",
    ROOT / "scripts/gen-public-subprocessors.py",
    ROOT / "apps/docs/docs/trust/index.mdx",
    ROOT / "apps/docs/docs/trust/subprocessors.mdx",
    ROOT / "apps/docs/i18n/de/docusaurus-plugin-content-docs/current/trust/index.mdx",
    ROOT / "apps/docs/i18n/de/docusaurus-plugin-content-docs/current/trust/subprocessors.mdx",
    ROOT / "apps/docs/i18n/es-419/docusaurus-plugin-content-docs/current/trust/index.mdx",
    ROOT / "apps/docs/i18n/es-419/docusaurus-plugin-content-docs/current/trust/subprocessors.mdx",
    ROOT / "apps/docs/i18n/pt-BR/docusaurus-plugin-content-docs/current/trust/index.mdx",
    ROOT / "apps/docs/i18n/pt-BR/docusaurus-plugin-content-docs/current/trust/subprocessors.mdx",
    ROOT / "docs/campaigns/remediation/B-134-docker-shim-experiment.md",
)


@pytest.mark.parametrize("path", RETIREMENT_DOCS, ids=lambda path: str(path.relative_to(ROOT)))
def test_b118_retirement_uses_integration_date(path: Path) -> None:
    text = path.read_text()
    assert RETIREMENT_DATE in text
    if path == ROOT / "BACKLOG.md":
        assert "cosign-sign.yml` foi removido em 2026-08-31" not in text
        assert "Fechado 2026-08-31: saída = remover" not in text
        assert "cosign-sign.yml` — **removido em 2026-08-31" not in text
    else:
        assert STALE_DATE not in text


def test_historical_build_observation_remains_historical() -> None:
    text = (ROOT / "BACKLOG.md").read_text()
    assert "container-build-push-prod.yml" in text
    assert "2026-08-31T00:39" in text
