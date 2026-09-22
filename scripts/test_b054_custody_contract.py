#!/usr/bin/env python3
"""Adversarial mutations for the B-054 hosted custody contract."""

from __future__ import annotations

from pathlib import Path
import sys

from verify_b054_custody_contract import ContractError, assess, read_files


def must_reject(label: str, files: dict[str, str]) -> None:
    try:
        assess(files)
    except ContractError:
        return
    raise AssertionError(f"B-054 adversarial mutation survived: {label}")


def main() -> int:
    try:
        baseline = read_files()
        assess(baseline)

        mutant = dict(baseline)
        mutant["workflow"] = mutant["workflow"].replace(
            "github.ref == 'refs/heads/main'", "github.ref == 'refs/heads/feature'", 1
        )
        must_reject("non-main dispatch", mutant)

        mutant = dict(baseline)
        mutant["workflow"] = mutant["workflow"].replace(
            "environment: b054-key-custody-drill", "environment: production", 1
        )
        must_reject("wrong environment", mutant)

        secret_line = "          B054_E1_LINK_KEY_HEX: ${{ secrets.B054_E1_LINK_KEY_HEX }}\n"
        mutant = dict(baseline)
        mutant["workflow"] = mutant["workflow"].replace(secret_line, "", 1).replace(
            "  protected-rotation:\n", "  protected-rotation:\n", 1
        )
        mutant["workflow"] = mutant["workflow"].replace(
            "  static-contract:\n", "  static-contract:\n    env:\n" + secret_line, 1
        )
        must_reject("secret reaches pull request job", mutant)

        mutant = dict(baseline)
        mutant["workflow"] = mutant["workflow"].replace(
            '          path: ${{ runner.temp }}/b054-custody-receipt.json\n',
            '          path: target/\n',
            1,
        )
        must_reject("artifact includes build output", mutant)

        mutant = dict(baseline)
        mutant["scanner"] = mutant["scanner"].replace("decoded))", "))", 1)
        must_reject("scanner ignores decoded key bytes", mutant)

        mutant = dict(baseline)
        mutant["doc"] = mutant["doc"].replace("self-approval is disabled", "self-approval allowed", 1)
        must_reject("self-approval protection removed", mutant)

        mutant = dict(baseline)
        mutant["doc"] += '\nB054_E1_SIGNING_SEED_HEX = "' + ("0" * 64) + '"\n'
        must_reject("key value added to docs", mutant)
    except (OSError, ContractError, AssertionError) as error:
        print(f"B-054 custody adversarial contract: FAIL ({error})", file=sys.stderr)
        return 1

    print("B-054 custody adversarial contract: PASS (7 unsafe mutations rejected)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
