#!/usr/bin/env python3
"""Adversarial checks for the B-156 published-claims census."""

from __future__ import annotations

import importlib.util
import json
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("published_claims_census", ROOT / "scripts/published_claims_census.py")
assert SPEC and SPEC.loader
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


def write_fixture(root: Path) -> Path:
    (root / "apps/docs").mkdir(parents=True)
    (root / "marketing").mkdir()
    (root / "legal").mkdir()
    (root / "apps/docs/page.mdx").write_text("CoreLink BYOK is documented.\n", encoding="utf-8")
    (root / "marketing/page.md").write_text("Buck2 is not supported here.\n", encoding="utf-8")
    (root / "legal/page.html").write_text("<p>pentest status is pending.</p>\n", encoding="utf-8")
    inventory = root / "inventory.json"
    inventory.write_text(json.dumps(MODULE.build_inventory(root), indent=2) + "\n", encoding="utf-8")
    return inventory


def test_published_claims_census_mutations() -> None:
    # Exercise the checked-in inventory through the same pytest lane that
    # runs these mutations; a standalone `--check` must not be the only proof.
    assert MODULE.validate(ROOT, ROOT / "scripts/published_claims_inventory.json") == []
    current = MODULE.collect_occurrences(ROOT)
    pentest = [entry for entry in current if str(entry.get("term", "")).lower() == "pentest"]
    assert len(pentest) == 203
    assert len({str(entry["path"]) for entry in pentest}) == 60

    with tempfile.TemporaryDirectory(prefix="b156-census-") as raw:
        root = Path(raw)
        inventory = write_fixture(root)
        assert MODULE.validate(root, inventory) == []
        original = (root / "apps/docs/page.mdx").read_text(encoding="utf-8")

        # A new published file cannot hide behind the old file/hash population.
        (root / "apps/docs/new.mdx").write_text("CoreLink BYOK is live.\n", encoding="utf-8")
        failures = MODULE.validate(root, inventory)
        assert any("file population changed" in failure for failure in failures), failures
        assert any("new/changed occurrence" in failure for failure in failures), failures

        # A term mutation in an existing line is also position/content-sensitive.
        (root / "apps/docs/new.mdx").unlink()
        (root / "apps/docs/page.mdx").write_text("CoreLink BYOK and pentest are documented.\n", encoding="utf-8")
        failures = MODULE.validate(root, inventory)
        assert any("file bytes changed" in failure for failure in failures), failures
        assert any("new/changed occurrence" in failure for failure in failures), failures
        (root / "apps/docs/page.mdx").write_text(original, encoding="utf-8")
        assert MODULE.validate(root, inventory) == []
    # Keep the synonym in the same canonical population as the short form.
    with tempfile.TemporaryDirectory(prefix="b156-synonym-") as raw:
        root = Path(raw)
        (root / "apps/docs").mkdir(parents=True)
        (root / "marketing").mkdir()
        (root / "legal").mkdir()
        (root / "apps/docs/page.mdx").write_text("External penetration testing is planned.\n", encoding="utf-8")
        occurrences = MODULE.collect_occurrences(root)
        assert [entry["term"] for entry in occurrences] == ["pentest"]
        (root / "apps/docs/page.mdx").write_text("External penetration-testing is planned.\n", encoding="utf-8")
        occurrences = MODULE.collect_occurrences(root)
        assert [entry["term"] for entry in occurrences] == ["pentest"]


def test_enterprise_adoption_claim_guard_is_closed_world() -> None:
    with tempfile.TemporaryDirectory(prefix="b156-enterprise-claims-") as raw:
        root = Path(raw)
        surface = root / "marketing/launch/SOCIAL"
        surface.mkdir(parents=True)
        (root / "apps/docs").mkdir(parents=True)
        (root / "legal").mkdir()
        (root / "apps/docs/page.mdx").write_text("CoreLink BYOK is documented.\n", encoding="utf-8")
        page = surface / "POST.md"
        page.write_text(
            "No external lighthouse customer has adopted CoreLink yet.\n",
            encoding="utf-8",
        )
        inventory = root / "inventory.json"
        inventory.write_text(json.dumps(MODULE.build_inventory(root), indent=2) + "\n", encoding="utf-8")
        assert MODULE.enterprise_adoption_violations(root) == []
        assert MODULE.validate(root, inventory) == []

        page.write_text("Three lighthouse customers attested before GA.\n", encoding="utf-8")
        failures = MODULE.validate(root, inventory)
        assert any("stale positive enterprise-adoption claim" in failure for failure in failures), failures

        page.write_text("One enterprise BYOK deployment completed a 30-day SLA observation.\n", encoding="utf-8")
        failures = MODULE.validate(root, inventory)
        assert any("stale positive enterprise-adoption claim" in failure for failure in failures), failures

        page.write_text("An enterprise BYOK customer adopted CoreLink.\n", encoding="utf-8")
        failures = MODULE.validate(root, inventory)
        assert any("stale positive enterprise-adoption claim" in failure for failure in failures), failures

        page.write_text("Sanitized variant maintained for NDA-gated sales distribution.\n", encoding="utf-8")
        failures = MODULE.validate(root, inventory)
        assert any("stale positive enterprise-adoption claim" in failure for failure in failures), failures


def main() -> int:
    test_published_claims_census_mutations()
    print("B-156 census mutation guard: new files, changed occurrences, and synonyms halt")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
