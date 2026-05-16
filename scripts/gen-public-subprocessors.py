#!/usr/bin/env python3
"""gen-public-subprocessors.py — Auto-generate the public sub-processors page
from `specs/_compliance/VENDOR-RISK-REGISTER.md`.

This is the **single source of truth → public surface** generator. It enforces
that the customer-facing
`apps/docs/docs/trust/subprocessors.mdx` page can never drift away from the
internal vendor register, satisfying:

- LGPD Art. 39 / Art. 27 §4º (public sub-processor list + 30-day advance
  notice mechanism — see `scripts/subprocessor-change-notify.py`).
- GDPR Art. 28 §2 (controller right to object to processor changes).
- DPA §6.2 contractual commitment to maintain a public list.

# Scope filter (public-visibility)

A row in the internal register is emitted to the public page **only** when:

1. `Cat.` is `C` (Critical) **or** `I` (Important).
   Standard-tier vendors are internal-only by default unless they also share
   customer data (`pii` / `payment` / `audit-logs` containing user content).
2. `Data sharing` is non-empty (a literal `none` value excludes the row,
   e.g. HashiCorp Vault customer-side).

A small set of "internal-only customer-side" vendors (BYOK custodians AWS
KMS / GCP KMS / Azure Key Vault / HashiCorp Vault) are emitted into a
separate **"BYOK key custodians"** info table because they are
sub-processors *to the customer*, not to CoreLink — but customers expect to
see them disclosed regardless.

# Usage

    # Default: write apps/docs/docs/trust/subprocessors.mdx
    python3 scripts/gen-public-subprocessors.py

    # Dry-run: print the rendered MDX to stdout, no file written
    python3 scripts/gen-public-subprocessors.py --dry-run

    # CI drift gate: exit 1 if the existing MDX differs from regenerated
    python3 scripts/gen-public-subprocessors.py --check

# Cross-references

- Compliance matrix: `specs/03_architecture/compliance_matrix.md` §7
  (GDPR Art. 28 + LGPD Art. 27 §4º + Art. 39).
- Runbook: `specs/_runbooks/RB-SUBPROCESSOR-CHANGE.md` (operational flow).
- Workflow: `.github/workflows/subprocessors-sync.yml` (drift gate + auto-PR).
- Notify hook: `scripts/subprocessor-change-notify.py` (30-day grace clock).
"""

from __future__ import annotations

import argparse
import datetime as dt
import pathlib
import re
import sys
from dataclasses import dataclass, field
from typing import Iterable


REPO_ROOT = pathlib.Path(__file__).resolve().parents[1]
REGISTER_PATH = REPO_ROOT / "specs" / "_compliance" / "VENDOR-RISK-REGISTER.md"
OUTPUT_PATH = REPO_ROOT / "apps" / "docs" / "docs" / "trust" / "subprocessors.mdx"

# Standard-tier vendors that share PII/payment/account data and must still
# appear on the public page (rare; documented in runbook §3).
STANDARD_TIER_PUBLIC_OVERRIDES = {"Twilio, Inc. (SendGrid + Twilio SMS)"}

# BYOK-custodian vendors emitted into the separate info table (customer-side;
# CoreLink never sees plaintext key material). Keyed by the `Vendor` column
# value in the register.
BYOK_CUSTODIANS = {
    "Amazon Web Services, Inc.",
    "Google LLC (Google Cloud)",
    "Microsoft Corporation (Azure)",
    "HashiCorp, Inc. (Vault customer-managed)",
}

# Internal-LLM vendors (no customer data; documented separately).
INTERNAL_LLM_VENDORS = {"Anthropic, PBC", "OpenAI, LLC"}

# Vendors completely excluded from public disclosure (out of scope, listed in
# register §4 as exclusions).
PUBLIC_EXCLUDED = {
    "Cybot A/S (Cookiebot)",  # consent vendor; documented under /privacy
}


@dataclass(frozen=True)
class VendorRow:
    """One parsed row from the register's section §2 table."""

    number: int
    vendor: str
    service: str
    category: str  # 'C' | 'I' | 'S'
    data_sharing: str
    regulatory_scope: str
    contract: str
    attestation: str
    inherent: str
    cef: str
    residual: str
    cadence: str
    last_review: str
    next_review: str
    owner: str

    @property
    def is_byok_custodian(self) -> bool:
        return self.vendor in BYOK_CUSTODIANS

    @property
    def is_internal_llm(self) -> bool:
        return self.vendor in INTERNAL_LLM_VENDORS

    @property
    def is_public_excluded(self) -> bool:
        return self.vendor in PUBLIC_EXCLUDED

    @property
    def is_customer_data_processor(self) -> bool:
        """True if the vendor actually receives CoreLink-customer data and
        must appear on the public list per GDPR Art. 28 / LGPD Art. 39."""
        if self.is_byok_custodian or self.is_internal_llm or self.is_public_excluded:
            return False
        if self.data_sharing.strip().lower() == "none":
            return False
        if self.category in ("C", "I"):
            return True
        # Standard tier: only if explicitly overridden (vendor handles user PII).
        return self.vendor in STANDARD_TIER_PUBLIC_OVERRIDES


# --------------------------------------------------------------------------
# Parser
# --------------------------------------------------------------------------

_TABLE_ROW = re.compile(r"^\|\s*(\d+)\s*\|")


def parse_register(register_text: str) -> tuple[list[VendorRow], str]:
    """Parse the register markdown and return (rows, last_review_date).

    The last_review date is taken from the `updated:` frontmatter field;
    register §1 also exposes a 'Baseline date'. We prefer frontmatter
    because it is what generators downstream pin against.
    """
    lines = register_text.splitlines()

    # Frontmatter `updated:`
    updated = "unknown"
    for ln in lines[:30]:
        m = re.match(r'^updated:\s*"?(\d{4}-\d{2}-\d{2})"?\s*$', ln)
        if m:
            updated = m.group(1)
            break

    rows: list[VendorRow] = []
    in_section_2 = False
    for ln in lines:
        if ln.startswith("## 2. Register"):
            in_section_2 = True
            continue
        if in_section_2 and ln.startswith("## "):
            # Left §2.
            break
        if not in_section_2:
            continue
        if not _TABLE_ROW.match(ln):
            continue
        cells = [c.strip() for c in ln.strip().strip("|").split("|")]
        # Expected 15 columns per §2 header.
        if len(cells) < 15:
            continue
        try:
            number = int(cells[0])
        except ValueError:
            continue
        rows.append(
            VendorRow(
                number=number,
                vendor=cells[1],
                service=cells[2],
                category=cells[3],
                data_sharing=cells[4],
                regulatory_scope=cells[5],
                contract=cells[6],
                attestation=cells[7],
                inherent=cells[8],
                cef=cells[9],
                residual=cells[10],
                cadence=cells[11],
                last_review=cells[12],
                next_review=cells[13],
                owner=cells[14],
            )
        )
    return rows, updated


# --------------------------------------------------------------------------
# Renderer
# --------------------------------------------------------------------------

DPA_LINK_RE = re.compile(r"\[([^\]]+DPA[^\]]*)\]\((https?://[^)]+)\)", re.IGNORECASE)
ATTESTATION_URL_RE = re.compile(r"\((https?://[^)]+)\)")


def extract_dpa_link(attestation_cell: str, vendor: str) -> str:
    """Best-effort DPA / trust-portal link extraction from the register
    attestation cell. Falls back to a 'on request' string."""
    m = DPA_LINK_RE.search(attestation_cell)
    if m:
        return f"[{m.group(1)}]({m.group(2)})"
    # Trust portal as DPA proxy.
    m2 = ATTESTATION_URL_RE.search(attestation_cell)
    if m2:
        return f"[Trust portal]({m2.group(1)})"
    return f"{vendor.split(',')[0]} DPA (on request)"


def shorten_region(scope: str, data_sharing: str) -> str:
    """Derive a public-facing region string. The register doesn't carry a
    dedicated region column for §2, so we encode 'Multi-region (per tenant
    primary_region pin)' as the default, with a Brazil-residency hint when
    LGPD scope is implied by the data-sharing class."""
    if "encrypted-blobs" in data_sharing or "pii" in data_sharing:
        return "Multi-region (per tenant `primary_region` pin)"
    return "US / EU (selectable)"


def shorten_service(service: str) -> str:
    """Trim the register's long service descriptions to a public summary."""
    # Cut at the first em-dash or " — " separator if it includes editorial.
    parts = re.split(r"\s+[—–-]\s+", service, maxsplit=1)
    public = parts[0].strip()
    # Final length cap.
    if len(public) > 90:
        public = public[:87].rstrip() + "..."
    return public


def shorten_data_sharing(ds: str) -> str:
    """Public-facing data-class label."""
    tokens = [t.strip() for t in ds.split("+")]
    pretty = ", ".join(tokens)
    return pretty


def render_active_table(rows: Iterable[VendorRow]) -> str:
    out = [
        "| # | Vendor | Service to CoreLink | Customer-data class | Region(s) | DPA |",
        "| --- | --- | --- | --- | --- | --- |",
    ]
    n = 0
    for r in rows:
        n += 1
        out.append(
            f"| {n} | **{r.vendor}** | {shorten_service(r.service)} | "
            f"{shorten_data_sharing(r.data_sharing)} | "
            f"{shorten_region(r.regulatory_scope, r.data_sharing)} | "
            f"{extract_dpa_link(r.attestation, r.vendor)} |"
        )
    return "\n".join(out)


def render_byok_table(rows: Iterable[VendorRow]) -> str:
    out = [
        "| Vendor | Service | Role |",
        "| --- | --- | --- |",
    ]
    for r in rows:
        short = shorten_service(r.service)
        out.append(f"| **{r.vendor}** | {short} | Customer-controlled CMK (BYOK option) |")
    return "\n".join(out)


def render_internal_llm_table(rows: Iterable[VendorRow]) -> str:
    out = [
        "| Vendor | Service | Used for |",
        "| --- | --- | --- |",
    ]
    for r in rows:
        out.append(f"| {r.vendor} | {shorten_service(r.service)} | Internal CoreLink eng / ops tooling |")
    return "\n".join(out)


MDX_TEMPLATE = '''---
title: "Sub-processors"
slug: "/trust/subprocessors"
description: "Public list of CoreLink sub-processors — vendor, service, region, DPA link, and the 30-day advance-notice mechanism for changes."
draft: false
generator: "scripts/gen-public-subprocessors.py"
source: "specs/_compliance/VENDOR-RISK-REGISTER.md"
cross_functional_review: TBD
pending_signoff:
  - "Security Lead"
  - "Legal"
  - "DPO"
sources:
  - "specs/_compliance/VENDOR-RISK-REGISTER.md"
  - "legal/sub-processors.md"
last_updated: "{last_updated}"
---

{{/* AUTO-GENERATED FILE — DO NOT EDIT BY HAND.
     Source of truth: specs/_compliance/VENDOR-RISK-REGISTER.md
     Generator:       scripts/gen-public-subprocessors.py
     CI drift gate:   .github/workflows/subprocessors-sync.yml
     30-day notify:   scripts/subprocessor-change-notify.py
*/}}

# Sub-processors

CoreLink uses a small set of carefully selected sub-processors to operate
the service. This page is the **public list** maintained per GDPR Art. 28
§2 and LGPD Art. 39 + Art. 27 §4º. It is a subset of our internal
[Vendor Risk Register](https://github.com/humangr-labs/corelink/blob/main/specs/_compliance/VENDOR-RISK-REGISTER.md)
(19 vendors total) — only those who *process customer personal data on
CoreLink's behalf* appear in the **Active sub-processors** table.

> **Last refreshed:** {last_updated}. This page is **auto-generated** from
> the internal vendor register on every change; see
> `.github/workflows/subprocessors-sync.yml` for the drift gate.

## Notice of changes (30-day grace)

Per our DPA (`legal/dpa/v1.0.0` §6) and LGPD Art. 27 §4º + GDPR Art. 28 §2,
we will give **at least 30 calendar days' written notice** before adding or
replacing a sub-processor that processes customer personal data.

Subscribe to change notices:

- **Email digest** — register a `subprocessor-changes@` distribution
  address inside your tenant settings. We send a digest the moment a
  change is queued, and a reminder 7 days before the change takes effect.
- **Status page subscription** — sub-processor changes are also published
  as a *Maintenance / Informational* item at
  [status.corelink.humangr.com](https://status.corelink.humangr.com). Subscribe via RSS,
  email, SMS, or webhook.
- **RSS feed (sub-processor changes only):**
  `https://corelink.humangr.com/trust/subprocessors.rss` (post-GA).

If you object to a proposed sub-processor change you have the rights set
out in DPA §6.4 (objection window, escalation, termination-for-cause if
unresolved).

## Active sub-processors

{active_table}

### BYOK key custodians (customer-side; not sub-processors)

The four KMS providers below are listed for completeness. CoreLink does
**not** process customer key material — keys remain inside the customer's
KMS account at all times. These providers are sub-processors *to you*, not
to us. CoreLink only holds wrapped DEKs.

{byok_table}

See [BYOK customer-managed keys](/security/byok) for the full provider
matrix and key-flow diagram.

### Internal LLM tooling (no customer data)

The providers below are used by CoreLink **internally** for engineering
and operations tooling. **No customer data is sent to either provider.**
Internal prompts are routed through enterprise zero-retention APIs.

{internal_llm_table}

If your DPO requires these to be listed as sub-processors despite the
zero-retention guarantee and no-customer-data scope, please raise it via
`privacy@humangr.com` and we will update the table on a per-tenant basis.

## Out-of-scope (referenced for completeness)

The following appear in our internal registers but are **not**
sub-processors:

- **Sigstore (Linux Foundation)** — public transparency log; no PII is
  written; treated as open-source infrastructure.
- **Self-hosted Dependency-Track** — operated by CoreLink; no third party.
- **Per-customer HashiCorp Vault instances** — customer-side infrastructure
  outside CoreLink's processor relationship.

## Auditing this list

The internal sub-processor file (with the full DPA matrix and termination
clauses) is at
[`legal/sub-processors.md`](https://github.com/humangr-labs/corelink/blob/main/legal/sub-processors.md).
The full vendor risk register (19 vendors with inherent risk scoring,
control-effectiveness factors, and quarterly review cadence) is at
[`specs/_compliance/VENDOR-RISK-REGISTER.md`](https://github.com/humangr-labs/corelink/blob/main/specs/_compliance/VENDOR-RISK-REGISTER.md).

## Change management

Operational runbook: [`specs/_runbooks/RB-SUBPROCESSOR-CHANGE.md`](https://github.com/humangr-labs/corelink/blob/main/specs/_runbooks/RB-SUBPROCESSOR-CHANGE.md).
The 30-day customer-broadcast pipeline is implemented by
`scripts/subprocessor-change-notify.py` and the
`corelink-privacy-sub-processor-emit` crate (CloudEvents
`corelink.privacy.subprocessor.notify_required`).

## Related

- [Trust Center overview](./)
- [Compliance](./compliance)
- [Data handling](./data-handling)
- [Incident response](./incident-response)
'''


def render_mdx(rows: list[VendorRow], last_updated: str) -> str:
    active = [r for r in rows if r.is_customer_data_processor]
    byok = [r for r in rows if r.is_byok_custodian]
    llm = [r for r in rows if r.is_internal_llm]
    return MDX_TEMPLATE.format(
        last_updated=last_updated,
        active_table=render_active_table(active),
        byok_table=render_byok_table(byok),
        internal_llm_table=render_internal_llm_table(llm),
    )


# --------------------------------------------------------------------------
# CLI
# --------------------------------------------------------------------------


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--register", type=pathlib.Path, default=REGISTER_PATH,
                        help="Path to VENDOR-RISK-REGISTER.md (default: specs/_compliance/VENDOR-RISK-REGISTER.md).")
    parser.add_argument("--out", type=pathlib.Path, default=OUTPUT_PATH,
                        help="Path to write the generated MDX (default: apps/docs/docs/trust/subprocessors.mdx).")
    parser.add_argument("--dry-run", action="store_true",
                        help="Print the rendered MDX to stdout; do not write.")
    parser.add_argument("--check", action="store_true",
                        help="Exit 1 if the regenerated MDX differs from the file on disk (CI drift gate).")
    args = parser.parse_args(argv)

    if not args.register.exists():
        print(f"error: register not found at {args.register}", file=sys.stderr)
        return 2

    text = args.register.read_text(encoding="utf-8")
    rows, updated = parse_register(text)
    if not rows:
        print("error: parsed 0 vendor rows from register §2", file=sys.stderr)
        return 2

    mdx = render_mdx(rows, updated)

    if args.dry_run:
        # Print compact summary at the bottom for human eyes.
        sys.stdout.write(mdx)
        print(
            f"\n--- dry-run: {len(rows)} register rows; "
            f"{sum(1 for r in rows if r.is_customer_data_processor)} active; "
            f"{sum(1 for r in rows if r.is_byok_custodian)} BYOK; "
            f"{sum(1 for r in rows if r.is_internal_llm)} internal-LLM ---",
            file=sys.stderr,
        )
        return 0

    if args.check:
        if not args.out.exists():
            print(f"error: --check but {args.out} does not exist", file=sys.stderr)
            return 1
        existing = args.out.read_text(encoding="utf-8")
        if existing != mdx:
            print("error: subprocessors.mdx is stale relative to VENDOR-RISK-REGISTER.md", file=sys.stderr)
            print("       run: python3 scripts/gen-public-subprocessors.py", file=sys.stderr)
            return 1
        print("ok: subprocessors.mdx is in sync with VENDOR-RISK-REGISTER.md")
        return 0

    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(mdx, encoding="utf-8")
    print(f"wrote: {args.out} ({len(mdx)} bytes; last_updated={updated})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
