#!/usr/bin/env python3
"""Fail-closed focal contract for B-226 alert-channel wiring."""

from __future__ import annotations

import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
ALERTER = Path("crates/corelink-ops/src/alerts/alerter.rs")
CHANNEL = Path("crates/corelink-ops/src/alerts/channel.rs")
CONFIG = Path("crates/corelink-ops/src/alerts/config.rs")


class ContractError(ValueError):
    """B-226 executable wiring is absent or weakened."""


def _read(root: Path, relative: Path) -> str:
    path = root / relative
    if not path.is_file() or path.is_symlink():
        raise ContractError(f"missing canonical source: {relative}")
    try:
        return path.read_text(encoding="utf-8")
    except (OSError, UnicodeError) as error:
        raise ContractError(f"source unreadable: {relative}") from error


def _without_comments(source: str) -> str:
    """Mask comments and normal/raw Rust strings before structural scans."""
    out: list[str] = []
    index = 0
    while index < len(source):
        if source.startswith("//", index):
            end = source.find("\n", index)
            end = len(source) if end < 0 else end
            out.extend(" " for _ in source[index:end])
            index = end
            continue
        if source.startswith("/*", index):
            end = index + 2
            depth = 1
            while end < len(source) and depth:
                if source.startswith("/*", end):
                    depth += 1
                    end += 2
                elif source.startswith("*/", end):
                    depth -= 1
                    end += 2
                else:
                    end += 1
            if depth:
                raise ContractError("unterminated Rust block comment")
            out.extend("\n" if char == "\n" else " " for char in source[index:end])
            index = end
            continue
        raw_hash_start = index + 2 if source.startswith("br", index) else index + 1 if source.startswith("r", index) else -1
        if raw_hash_start >= 0:
            hashes = 0
            while raw_hash_start + hashes < len(source) and source[raw_hash_start + hashes] == "#":
                hashes += 1
            quote = raw_hash_start + hashes
            if quote < len(source) and source[quote] == '"':
                terminator = '"' + ("#" * hashes)
                close = source.find(terminator, quote + 1)
                if close < 0:
                    raise ContractError("unterminated Rust raw string")
                end = close + len(terminator)
                out.extend("\n" if char == "\n" else " " for char in source[index:end])
                index = end
                continue
        if source[index] == '"':
            end = index + 1
            escaped = False
            while end < len(source):
                char = source[end]
                if char == '"' and not escaped:
                    break
                if char == "\n" and not escaped:
                    raise ContractError("unterminated Rust normal string")
                escaped = char == "\\" and not escaped
                if char != "\\":
                    escaped = False
                end += 1
            if end >= len(source):
                raise ContractError("unterminated Rust normal string")
            end += 1
            out.extend("\n" if char == "\n" else " " for char in source[index:end])
            index = end
            continue
        out.append(source[index])
        index += 1
    return "".join(out)


def _function(source: str, signature: str) -> str:
    start = source.find(signature)
    if start < 0:
        raise ContractError(f"function missing: {signature}")
    brace = source.find("{", start)
    if brace < 0:
        raise ContractError(f"function body missing: {signature}")
    depth = 0
    for index in range(brace, len(source)):
        if source[index] == "{":
            depth += 1
        elif source[index] == "}":
            depth -= 1
            if depth == 0:
                return source[start : index + 1]
    raise ContractError(f"unterminated function: {signature}")


def verify_sources(alerter: str, channel: str, config: str) -> None:
    alerter = _without_comments(alerter)
    channel = _without_comments(channel)
    config = _without_comments(config)

    channel_enum = _function(channel, "pub enum AlertChannel")
    variants = re.findall(r"^\s*(Dashboard|Email|InApp|Slack),", channel_enum, re.MULTILINE)
    if variants != ["Dashboard", "Email", "InApp", "Slack"]:
        raise ContractError(f"closed channel population drifted: {variants}")
    error_enum = _function(channel, "pub enum AlertTransportError")
    if error_enum.count("#[error(") != 4:
        raise ContractError("all four transport errors must retain thiserror displays")
    if len(re.findall(r"\bchannel:\s*AlertChannel", error_enum)) != 4:
        raise ContractError("every channel-bearing error must use AlertChannel")
    if error_enum.count("detail: String") != 2 or "status: u16" not in error_enum:
        raise ContractError("transport error field population drifted")
    for marker in (
        "pub struct AlertEnvelope",
        "pub struct DeliveryReceipt",
        "impl std::fmt::Display for AlertChannel",
        "formatter.write_str(self.as_str())",
        "pub trait AlertTransport",
        "async fn deliver(",
        "pub struct RecordingAlertTransport",
    ):
        if marker not in channel:
            raise ContractError(f"provider boundary marker missing: {marker}")

    dispatch = _function(alerter, "async fn dispatch_envelope(")
    expected_dispatch = (
        r"self\s*\.dispatch_channel\(\s*AlertChannel::Dashboard,\s*envelope\)\s*\.await",
        r"self\s*\.dispatch_channel\(\s*AlertChannel::Email,\s*envelope\)\s*\.await",
        r"self\s*\.dispatch_channel\(\s*AlertChannel::InApp,\s*envelope\)\s*\.await",
        r"self\s*\.dispatch_channel\(\s*AlertChannel::Slack,\s*envelope\)\s*\.await",
    )
    positions = []
    for marker in expected_dispatch:
        match = re.search(marker, dispatch)
        if match is None:
            raise ContractError(f"channel dispatch missing: {marker}")
        positions.append(match.start())
    if positions != sorted(positions) or len(set(positions)) != 4:
        raise ContractError("four channel dispatches are not closed and ordered")

    dispatch_channel = _function(alerter, "async fn dispatch_channel(")
    for marker in (
        "if !self.channel_enabled(channel)",
        "self.transport.deliver(channel, envelope.clone()).await",
        "ChannelOutcome::Failed(error.to_string())",
    ):
        if marker not in dispatch_channel:
            raise ContractError(f"fail-closed dispatch marker missing: {marker}")
    if re.search(
        r"Ok\s*\(\s*receipt\s*\)\s*=>\s*self\.receipt_outcome\(\s*receipt\s*\)",
        dispatch_channel,
    ) is None:
        raise ContractError("successful provider responses must be converted from their DeliveryReceipt")
    if "not wired" in alerter or 'ChannelOutcome::Success(format!("dashboard:' in alerter:
        raise ContractError("production path still fabricates an unwired success")
    recovery = _function(alerter, "async fn alert_recovery(")
    if "self.dispatch_envelope(&envelope).await" not in recovery:
        raise ContractError("recovery does not use the four-channel provider")
    if "Err(RevocationError::AlertDeliveryFailed" not in recovery:
        raise ContractError("recovery failure is not fail-closed")

    transport = _function(alerter, "async fn deliver(")
    for marker in (
        "endpoint(channel)",
        "connect_timeout(Duration::from_secs(3))",
        ".timeout(Duration::from_secs(10))",
        "if !status.is_success()",
        "AlertTransportError::Rejected",
        "DeliveryReceipt {",
    ):
        if marker not in alerter:
            # The HTTP function is below the impl, so search the full source
            # for markers which are split around the nested function helper.
            if marker not in transport:
                raise ContractError(f"bounded HTTP provider marker missing: {marker}")

    for marker in (
        "dashboard_endpoint: Option<String>",
        "email_endpoint: Option<String>",
        "in_app_endpoint: Option<String>",
        "slack_webhook_url: Option<String>",
    ):
        if marker not in config:
            raise ContractError(f"provider endpoint population missing: {marker}")
    for marker in (
        "configured_transport_receives_all_four_closed_channels",
        "configured_transport_receives_recovery_and_returns_receipts",
        "RecordingAlertTransport::default",
        "AlertChannel::Dashboard",
        "AlertChannel::Email",
        "AlertChannel::InApp",
        "AlertChannel::Slack",
    ):
        if marker not in alerter:
            raise ContractError(f"focal behavioral evidence missing: {marker}")


def verify(root: Path = ROOT) -> None:
    verify_sources(
        _read(root, ALERTER),
        _read(root, CHANNEL),
        _read(root, CONFIG),
    )


def main() -> int:
    try:
        verify(ROOT)
    except ContractError as error:
        print(f"B226 RED: {error}", file=sys.stderr)
        return 1
    print("B226 alert wiring contract: PASS (four channels; receipts; fail-closed)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
