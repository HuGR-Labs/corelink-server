#!/usr/bin/env python3
"""Mutation checks for the closed-world B-157 published credential census."""

from __future__ import annotations

import importlib.util
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("verify_b157_published_credentials", ROOT / "scripts/verify_b157_published_credentials.py")
assert SPEC and SPEC.loader
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


def main() -> int:
    result, failures = MODULE.verify(ROOT)
    assert not failures, failures
    assert len(result["helper_lines"]) >= 40, len(result["helper_lines"])
    assert len(result["stdin_curls"]) >= 30, len(result["stdin_curls"])
    with tempfile.TemporaryDirectory(prefix="b157-published-") as raw:
        root = Path(raw)
        target = root / "apps/docs/i18n/es-419/docusaurus-plugin-content-docs/current/integrations/bazel.md"
        target.parent.mkdir(parents=True)
        target.write_text(
            "build --credential_helper=corelink-api.humangr.com=%workspace%/.bazel/helper.sh\n"
            "build --remote_header=Authorization=Bearer ${CORELINK_PAT}\n",
            encoding="utf-8",
        )
        mutated, mutated_failures = MODULE.verify(root)
        assert any("remote_header" in failure for failure in mutated_failures), mutated

        target.write_text(
            "curl https://example.invalid \\\n  -H \"Authorization: Bearer $CORELINK_PAT\"\n",
            encoding="utf-8",
        )
        _, mutated_failures = MODULE.verify(root)
        assert any("curl_argv" in failure for failure in mutated_failures), mutated_failures

        target.write_text(
            "curl https://status.example.invalid " + chr(92) + "\n"
            "  -H \"Authorization: Token token=${STATUSPAGE_API_KEY}\"\n",
            encoding="utf-8",
        )
        _, mutated_failures = MODULE.verify(root)
        assert any("curl_argv" in failure for failure in mutated_failures), mutated_failures

        target.write_text(
            "curl https://example.invalid " + chr(92) + "\n"
            "  -H \"Authorization: Bearer ${CORELINK_" + chr(92) + "\nPAT}\"\n",
            encoding="utf-8",
        )
        _, mutated_failures = MODULE.verify(root)
        assert any("curl_argv" in failure for failure in mutated_failures), mutated_failures

        target.write_text(
            "curl https://collector.example.invalid " + chr(92) + "\n"
            "  -H \"Authorization: Bearer ${OTEL_BEARER_" + chr(92) + "\nTOKEN}\"\n",
            encoding="utf-8",
        )
        _, mutated_failures = MODULE.verify(root)
        assert any("curl_argv" in failure for failure in mutated_failures), mutated_failures

        target.write_text(
            "curl --config - <<EOF\n"
            "header = \"X-Not-Authorization: $CORELINK_PAT\"\n"
            "EOF\n",
            encoding="utf-8",
        )
        _, mutated_failures = MODULE.verify(root)
        assert any("malformed_stdin_config" in failure for failure in mutated_failures), mutated_failures
    print("B-157 published credential census: all locales covered; non-English mutations halt")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
