from __future__ import annotations

import tomllib
from pathlib import Path
from urllib.parse import urlsplit


ROOT = Path(__file__).resolve().parents[1]
CONFIG = ROOT / "wrangler.toml"
EXPECTED_ORIGIN = "https://corelink-fabricd.gmhelmold.workers.dev"
PRODUCTION_ENVS = ("prod", "prod-sam", "prod-lhr", "prod-nrt", "prod-syd")


def test_credential_authority_origin_is_pinned_in_every_production_env() -> None:
    config = tomllib.loads(CONFIG.read_text(encoding="utf-8"))
    environments = config["env"]

    for name in PRODUCTION_ENVS:
        variables = environments[name]["vars"]
        origin = variables["FABRIC_CREDENTIAL_AUTHORITY_URL"]
        parsed = urlsplit(origin)

        assert origin == EXPECTED_ORIGIN
        assert parsed.scheme == "https"
        assert parsed.hostname == "corelink-fabricd.gmhelmold.workers.dev"
        assert parsed.username is None and parsed.password is None
        assert parsed.path == parsed.query == parsed.fragment == ""
        assert "FABRIC_CREDENTIAL_ISSUER_AUTH_KEY" not in variables
