#!/usr/bin/env python3
"""Hosted current-main provenance and approval-boundary verifier."""
from __future__ import annotations
import hashlib, json, subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parent
FORBIDDEN = {"owner", "owners", "signoff", "signoffs", "approved_by", "approval_signature", "approval_signer"}

def fail(msg: str) -> None:
    raise SystemExit("OWNERSHIP_PREPARATION_INVALID: " + msg)

def git(*args: str) -> str:
    return subprocess.check_output(["git", *args], cwd=ROOT.parents[3], text=True).strip()

def check_file(path: str, commit: str, expected_blob: str | None = None, expected_sha: str | None = None) -> None:
    full = ROOT.parents[3] / path
    if not full.is_file(): fail(f"missing provenance path: {path}")
    blob = git("hash-object", path)
    sha = hashlib.sha256(full.read_bytes()).hexdigest()
    if expected_blob and blob != expected_blob: fail(f"stale blob: {path}")
    if expected_sha and sha != expected_sha: fail(f"stale sha256: {path}")

def walk(value, path=""):
    if isinstance(value, dict):
        for key, child in value.items():
            low = key.lower()
            if low in FORBIDDEN and child not in (None, "", [], {}, False, "UNASSIGNED", "NOT_ASSIGNED", "UNAPPROVED"):
                fail(f"fabricated owner/signoff field: {path}/{key}")
            if low in {"issue_ready", "deep_semantic_relations_complete", "publication_approved"} and child is True:
                fail(f"fabricated approval state: {path}/{key}")
            walk(child, f"{path}/{key}")
    elif isinstance(value, list):
        for n, child in enumerate(value): walk(child, f"{path}/{n}")

def main() -> None:
    census = json.loads((ROOT / "census.json").read_text())
    sources = json.loads((ROOT / "source-packets.json").read_text())
    seeds = json.loads((ROOT / "seeds.json").read_text())
    summary = json.loads((ROOT / "summary.json").read_text())
    head = git("rev-parse", "HEAD")
    source = census["source_commit"]
    if summary["source_commit"] != source or summary["head_after"] != source:
        fail("summary and census provenance disagree")
    try:
        subprocess.check_call(["git", "merge-base", "--is-ancestor", source, head], cwd=ROOT.parents[3])
    except subprocess.CalledProcessError:
        fail("census provenance is not an ancestor of the checked-out PR")
    names = {p["package"] for p in census["packages"]}
    if len(names) != 105 or names != set(sources) or names != set(seeds):
        fail("package population is not exactly 105 and aligned")
    for package in census["packages"]:
        manifest = package["manifest"]
        check_file(manifest, source, package["manifest_blob"], package["manifest_sha256"])
        if package.get("owner") or package.get("signoff"):
            fail(f"owner/signoff claimed for {package['package']}")
        for table in (sources[package["package"]], seeds[package["package"]]):
            if table.get("source_commit") != source: fail(f"stale source packet: {package['package']}")
            for evidence in table.get("seed_evidence", []):
                check_file(evidence["path"], source, evidence.get("blob"), evidence.get("sha256"))
    for obj in (census, sources, seeds, summary): walk(obj)
    print(json.dumps({"result": "CURRENT_MAIN_PROVENANCE_VALID", "packages": 105, "publication_approved": False}))

if __name__ == "__main__": main()
