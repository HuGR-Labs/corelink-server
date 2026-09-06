#!/usr/bin/env python3
"""Static, comment-safe guard for the D01 Dependency-Track sunset seam."""

from __future__ import annotations

import argparse
from pathlib import Path
import sys


ROOT = Path(__file__).resolve().parents[1]
HANDLER = ROOT / "crates/corelink-dt-webhook/src/handler.rs"
WORKFLOW = ROOT / ".github/workflows/b249-dt-webhook-sunset.yml"
CHANGELOG = ROOT / "changelog.d/PENDING-b249-dt-webhook-sunset.md"


class VerificationError(RuntimeError):
    pass


def read(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8")
    except OSError as error:
        raise VerificationError(f"missing required artifact: {path}: {error}") from error


def strip_rust_comments(source: str) -> str:
    """Blank Rust comments while preserving strings and code structure."""
    out: list[str] = []
    index = 0
    block_depth = 0
    while index < len(source):
        if block_depth:
            if source.startswith("/*", index):
                block_depth += 1
                out.extend("  ")
                index += 2
            elif source.startswith("*/", index):
                block_depth -= 1
                out.extend("  ")
                index += 2
            else:
                out.append("\n" if source[index] == "\n" else " ")
                index += 1
            continue
        if source.startswith("//", index):
            out.extend("  ")
            index += 2
            while index < len(source) and source[index] != "\n":
                out.append(" ")
                index += 1
            continue
        if source.startswith("/*", index):
            block_depth = 1
            out.extend("  ")
            index += 2
            continue
        if source[index] == '"':
            out.append(source[index])
            index += 1
            while index < len(source):
                out.append(source[index])
                if source[index] == "\\" and index + 1 < len(source):
                    index += 1
                    out.append(source[index])
                elif source[index] == '"':
                    index += 1
                    break
                index += 1
            continue
        out.append(source[index])
        index += 1
    if block_depth:
        raise VerificationError("unterminated Rust block comment")
    return "".join(out)


def require(text: str, markers: tuple[str, ...], label: str) -> None:
    missing = [marker for marker in markers if marker not in text]
    if missing:
        raise VerificationError(f"{label} missing: {', '.join(missing)}")


def verify_texts(handler: str, workflow: str, changelog: str) -> None:
    code = strip_rust_comments(handler)
    require(
        code,
        (
            "pub fn new(webhook_secret: Vec<u8>, mock_injection_enabled: bool)",
            "Self::new_with_clock(webhook_secret, mock_injection_enabled, SystemTime::now)",
            "pub fn new_with_clock<F>(",
            "F: Fn() -> SystemTime + Send + Sync + 'static",
            "clock: Arc<dyn Fn() -> SystemTime + Send + Sync>",
            "fn check_patch_suppression(\n        event: &DtWebhookEvent,\n        now: SystemTime",
            "chrono_mini::today_from_system_time(now)",
            "if days > 90",
            "fn patched_locally_suppressed_at_day_90()",
            "fn patched_locally_alerts_at_day_91()",
            "handler_at(\"2026-07-30\")",
            "handler_at(\"2026-07-31\")",
            "assert_eq!(result.channels.len(), 3)",
        ),
        "D01 handler",
    )
    if "if days >= 90" in code:
        raise VerificationError("day-90 suppression boundary was weakened to >= 90")
    if "2026-05-01\".into()); // Recent" in handler:
        raise VerificationError("time-bombed Recent test annotation remains")

    require(
        workflow,
        (
            "verify_b249_dt_webhook_sunset.py --self-test",
            "tests/test_verify_b249_dt_webhook_sunset.py",
            "No Cargo/CI",
        ),
        "B249 workflow",
    )
    require(
        changelog,
        (
            "B249",
            "injected clock",
            "day 90",
            "day 91",
            "production sunset semantics",
        ),
        "B249 changelog",
    )


def verify_repo() -> None:
    verify_texts(read(HANDLER), read(WORKFLOW), read(CHANGELOG))


MUTATIONS = (
    ("inclusive-boundary", "if days > 90", "if days >= 90"),
    ("clock-use", "chrono_mini::today_from_system_time(now)", "chrono_mini::today()"),
    ("clock-constructor", "Self::new_with_clock(webhook_secret, mock_injection_enabled, SystemTime::now)", "Self {"),
    ("day-90-test", "fn patched_locally_suppressed_at_day_90()", "fn patched_locally_suppressed()"),
    ("day-91-test", "fn patched_locally_alerts_at_day_91()", "fn patched_locally_alerts()"),
    ("workflow", "verify_b249_dt_webhook_sunset.py --self-test", "verify_other_sunset.py --self-test"),
)


def verify_mutations() -> int:
    handler = read(HANDLER)
    workflow = read(WORKFLOW)
    changelog = read(CHANGELOG)
    passed = 0
    for name, old, new in MUTATIONS:
        if name == "workflow":
            mutated_handler, mutated_workflow, mutated_changelog = handler, workflow.replace(old, new, 1), changelog
        else:
            mutated_handler, mutated_workflow, mutated_changelog = handler.replace(old, new, 1), workflow, changelog
        if mutated_handler == handler and mutated_workflow == workflow:
            raise VerificationError(f"mutation {name} did not change its fixture")
        try:
            verify_texts(mutated_handler, mutated_workflow, mutated_changelog)
        except VerificationError:
            passed += 1
        else:
            raise VerificationError(f"mutation {name} was accepted")

    comment_mutant = handler.replace(
        "chrono_mini::today_from_system_time(now)",
        "// chrono_mini::today_from_system_time(now)",
        1,
    )
    comment_mutant += "\n// chrono_mini::today_from_system_time(now)\n"
    try:
        verify_texts(comment_mutant, workflow, changelog)
    except VerificationError:
        passed += 1
    else:
        raise VerificationError("comment-only clock marker mutation was accepted")
    return passed


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true", help="run mutation checks")
    args = parser.parse_args(argv)
    try:
        verify_repo()
        mutations = verify_mutations() if args.self_test else 0
    except VerificationError as error:
        print(f"B249 FAIL: {error}", file=sys.stderr)
        return 1
    suffix = f"; mutations={mutations}/{len(MUTATIONS) + 1}" if args.self_test else ""
    print(f"B249 PASS: D01 sunset clock is injected and day-90/day-91 guarded{suffix}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
