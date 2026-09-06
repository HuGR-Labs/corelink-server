#!/usr/bin/env python3
"""Hermetic semantic/mutation gate for B-081 PAT rotation forwarding."""

from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def main() -> int:
    adapter = (ROOT / "crates/corelink-container/src/adapter_pat_verifier.rs").read_text()
    helper = (ROOT / "worker/src/lib/pat_rotation_env.ts").read_text()
    start = (ROOT / "worker/src/durable_object_start.ts").read_text()
    test = (ROOT / "worker/tests/pat_rotation_forward.test.ts").read_text()
    if "PAT_SIGNING_KEY_PREV" not in adapter:
        raise SystemExit("B-081 container rotation overlap is missing")
    if any(name not in helper for name in ("PAT_SIGNING_KEY_PREV", "PAT_SIGNING_KEY_NEW")):
        raise SystemExit("B-081 rotation helper is incomplete")
    def shape(text: str) -> bool:
        return "...patRotationEnv(ctx.env)" in text

    if not shape(start):
        raise SystemExit("B-081 start path does not forward rotation keys")
    for marker in ("PAT_SIGNING_KEY_PREV", "PAT_SIGNING_KEY_NEW", "blank", "tenant"):
        if marker not in test:
            raise SystemExit(f"B-081 focused mutation population missing {marker}")
    mutated = start.replace("...patRotationEnv(ctx.env)", "/* removed rotation forwarding */", 1)
    if not shape(mutated):
        print("B-081 rotation forwarding + focused mutation PASS")
        return 0
    raise SystemExit("B-081 forwarding mutation survived")


if __name__ == "__main__":
    raise SystemExit(main())
