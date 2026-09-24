#!/usr/bin/env python3
"""Static boundary for the bounded #1863 hosted mutants evidence lane."""

from pathlib import Path

from scripts.verify_i2457_mutants_shards import VerificationError, command_verify_workflow


WORKFLOW = Path(".github/workflows/issue-1863-mutants-hosted.yml")


def verify(text: str) -> None:
    scratch = WORKFLOW.with_suffix(".verify-tmp")
    try:
        scratch.write_text(text, encoding="utf-8")
        command_verify_workflow(type("Args", (), {"workflow": scratch})())
    except VerificationError as error:
        raise ValueError(str(error)) from error
    finally:
        scratch.unlink(missing_ok=True)


def main() -> int:
    verify(WORKFLOW.read_text(encoding="utf-8"))
    print("issue #1666 hosted mutants shard evidence contract: PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
