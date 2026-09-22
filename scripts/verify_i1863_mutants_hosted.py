#!/usr/bin/env python3
"""Static contract check for the dispatch-only #1863 hosted mutants lane."""

from pathlib import Path


WORKFLOW = Path(".github/workflows/issue-1863-mutants-hosted.yml")
REQUIRED = (
    "workflow_dispatch:",
    "permissions:\n  contents: read",
    "runs-on: ubuntu-latest",
    "timeout-minutes: 240",
    "timeout-minutes: 225",
    "actions/checkout@9c091bb21b7c1c1d1991bb908d89e4e9dddfe3e0",
    "Swatinem/rust-cache@400e7407cfd7a091e5fbb6afec01ec146c432b7c",
    "arduino/setup-protoc@f4d5893b897028ff5739576ea0409746887fa536",
    "taiki-e/install-action@07b4745e0c39a41822af610387492e3e53aa222b",
    "tool: cargo-mutants@27.0.0",
    "fallback: cargo-binstall",
    "cargo mutants --workspace --no-shuffle --minimum-test-timeout=600",
)


def main() -> int:
    text = WORKFLOW.read_text(encoding="utf-8")
    if "schedule:" in text or "pull_request:" in text or "push:" in text:
        raise SystemExit("#1863 lane must remain workflow_dispatch-only")
    missing = [marker for marker in REQUIRED if marker not in text]
    if missing:
        raise SystemExit("missing contract markers: " + ", ".join(missing))
    if "self-hosted" in text or "corelink-builder" in text or "runs-on: corelink" in text:
        raise SystemExit("#1863 lane must use GitHub-hosted Linux only")
    if "publish" in text.lower() or "deploy" in text.lower():
        raise SystemExit("#1863 lane must not publish or deploy")
    required_guards = (
        "github.repository == 'HuGR-dev/corelink-server'",
        "github.ref == 'refs/heads/main'",
        "github.ref_protected",
        "cancel-in-progress: false",
    )
    missing_guards = [marker for marker in required_guards if marker not in text]
    if missing_guards:
        raise SystemExit("missing dispatch guards: " + ", ".join(missing_guards))
    if "run: cargo mutants --workspace --no-shuffle --minimum-test-timeout=600" not in text:
        raise SystemExit("mutants command drifted from the nightly lane")
    print("issue #1863 hosted mutants contract: PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
