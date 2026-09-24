#!/usr/bin/env python3
"""Hosted historical-source and current-manifest provenance verifier."""
from __future__ import annotations
import hashlib, json, subprocess, tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent
FORBIDDEN = {"owner", "owners", "signoff", "signoffs", "approved_by", "approval_signature", "approval_signer"}

def fail(msg: str) -> None:
    raise SystemExit("OWNERSHIP_PREPARATION_INVALID: " + msg)

class ProvenanceError(ValueError):
    pass

def git(*args: str) -> str:
    return subprocess.check_output(["git", *args], cwd=ROOT.parents[3], text=True).strip()

def git_bytes(commit: str, path: str) -> bytes:
    return subprocess.check_output(["git", "show", f"{commit}:{path}"], cwd=ROOT.parents[3])

def verify_expected_blob(path: str, actual_blob: str, expected_blob: str | None) -> None:
    if expected_blob and actual_blob != expected_blob:
        raise ProvenanceError(f"stale blob: {path}")

def check_file(path: str, commit: str, expected_blob: str | None = None, expected_sha: str | None = None) -> None:
    """Verify immutable evidence against the specified historical Git tree."""
    try:
        blob = git("rev-parse", f"{commit}:{path}")
        content = git_bytes(commit, path)
    except subprocess.CalledProcessError:
        fail(f"missing historical provenance path: {path} at {commit}")
    try:
        verify_expected_blob(path, blob, expected_blob)
    except ProvenanceError as error:
        fail(str(error))
    sha = hashlib.sha256(content).hexdigest()
    if expected_sha and sha != expected_sha: fail(f"stale sha256: {path}")

def check_recorded_evidence(path: str, source: str, observed_main: str, expected_blob: str | None, expected_sha: str | None) -> None:
    """Check a packet pin against its two explicitly recorded source snapshots."""
    errors = []
    for commit in dict.fromkeys((source, observed_main)):
        try:
            check_file(path, commit, expected_blob, expected_sha)
            return
        except SystemExit as error:
            errors.append(str(error))
    fail(f"seed evidence pin matches neither recorded snapshot: {path} ({'; '.join(errors)})")

def check_current_observation(observation: dict, source: str, census: dict) -> None:
    package_name = "corelink-audit-chain"
    package = next((item for item in census["packages"] if item["package"] == package_name), None)
    if package is None or observation.get("package") != package_name:
        fail("current manifest observation does not identify the census package")
    manifest = package["manifest"]
    if observation.get("manifest") != manifest or observation.get("historical_source_commit") != source:
        fail("current manifest observation is detached from the historical census")
    if (observation.get("historical_manifest_blob") != package["manifest_blob"] or
            observation.get("historical_manifest_sha256") != package["manifest_sha256"]):
        fail("current manifest observation rewrites historical census evidence")
    current_commit = observation.get("observed_main_commit")
    if current_commit != "a577032ab816c1ae5debb2f6ddf29494162f1336":
        fail("current manifest observation does not use the recorded main readback")
    try:
        if git("rev-parse", "refs/remotes/origin/main") != current_commit:
            fail("current manifest observation is stale relative to origin/main")
        subprocess.check_call(["git", "merge-base", "--is-ancestor", current_commit, git("rev-parse", "HEAD")], cwd=ROOT.parents[3])
        subprocess.check_call(["git", "merge-base", "--is-ancestor", observation["introducing_merge_commit"], current_commit], cwd=ROOT.parents[3])
    except (subprocess.CalledProcessError, KeyError):
        fail("current observation or introducing merge is not in the checked-out history")
    check_file(manifest, source, observation["historical_manifest_blob"], observation["historical_manifest_sha256"])
    check_file(manifest, current_commit, observation.get("observed_manifest_blob"), observation.get("observed_manifest_sha256"))

    old = tomllib.loads(git_bytes(source, manifest).decode())
    current = tomllib.loads(git_bytes(current_commit, manifest).decode())
    added_features = {key: value for key, value in current.get("features", {}).items() if key not in old.get("features", {})}
    added_dependencies = {
        name for name, value in current.get("dependencies", {}).items()
        if name not in old.get("dependencies", {}) and value.get("optional") is True
    }
    expected_features = observation["manifest_delta"]["added_feature"]
    expected_dependencies = set(observation["manifest_delta"]["added_optional_dependencies"])
    if added_features != expected_features or added_dependencies != expected_dependencies:
        fail("audit-chain manifest fields differ from the recorded #2347 delta")
    for key in ("feature_activation_proven", "runtime_verified", "provider_operation_proven", "publication_approved"):
        if observation.get(key) is not False: fail(f"unsupported current manifest claim: {key}")
    if observation.get("approval_state") != "UNASSIGNED": fail("current manifest approval state must remain unassigned")

def check_stale_blob_rejected(path: str, commit: str, expected_blob: str) -> None:
    """Exercise the stale-object rejection path with an intentionally wrong blob."""
    try:
        actual = git("rev-parse", f"{commit}:{path}")
        verify_expected_blob(path, actual, expected_blob)
    except ProvenanceError as error:
        if str(error) != f"stale blob: {path}": raise
        return
    fail("negative stale-blob fixture was accepted")

def check_preserved_boundary(main_commit: str) -> None:
    """Ensure this reconciliation leaves historical package and state ledgers byte-identical."""
    protected = (
        "docs/ownership/preparation/2026-09-19/census.json",
        "docs/ownership/preparation/2026-09-19/source-packets.json",
        "docs/ownership/preparation/2026-09-19/seeds.json",
        "docs/ownership/preparation/2026-09-19/index.md",
        "docs/ownership/registry.json",
        "docs/ownership/plans/publication-ledger.json",
    )
    for path in protected:
        if git("rev-parse", f"HEAD:{path}") != git("rev-parse", f"{main_commit}:{path}"):
            fail(f"historical packet or ownership state changed: {path}")
    registry = json.loads(git_bytes("HEAD", "docs/ownership/registry.json"))
    entries = registry["packages"]
    if len(entries) != 105 or registry.get("population_count") != 105 or registry.get("publication_count") != 0:
        fail("ownership registry no longer records 105 packages and zero publications")
    if any(entry.get("cold_review") != "UNVERIFIED" or entry.get("publication") != "NOT_PUBLISHED" for entry in entries):
        fail("ownership registry approval/publication boundary changed")
    ledger = json.loads(git_bytes("HEAD", "docs/ownership/plans/publication-ledger.json"))
    items = ledger["items"]
    if len(items) != 105 or any(item.get("state") != "BLOCKED" for item in items):
        fail("publication ledger no longer records 105 BLOCKED packages")

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
    current_observations = json.loads((ROOT / "current-manifest-observations.json").read_text())
    observations = current_observations.get("observations", [])
    if len(observations) != 1: fail("current manifest observation set must contain exactly one scoped record")
    current_main = observations[0].get("observed_main_commit")
    if current_main != "a577032ab816c1ae5debb2f6ddf29494162f1336":
        fail("seed evidence observation does not use the recorded main readback")
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
                # Existing packet pins span the historical source and the explicitly
                # recorded current-main readback. Validate bytes in either tree without
                # rewriting those packet records or treating current evidence as old.
                check_recorded_evidence(evidence["path"], source, current_main, evidence.get("blob"), evidence.get("sha256"))
    check_current_observation(observations[0], source, census)
    check_stale_blob_rejected(observations[0]["manifest"], observations[0]["observed_main_commit"], "0" * 40)
    check_preserved_boundary(observations[0]["observed_main_commit"])
    for obj in (census, sources, seeds, summary, current_observations): walk(obj)
    print(json.dumps({
        "result": "OWNERSHIP_PROVENANCE_VALID",
        "packages": 105,
        "current_manifest_observations": 1,
        "stale_blob_negative_case": "REJECTED",
        "publication_approved": False,
        "cold_review": "105 UNVERIFIED",
        "publication": "105 NOT_PUBLISHED",
        "blocked": "105 BLOCKED",
        "publication_count": 0,
        "historical_records_unchanged": True
    }))

if __name__ == "__main__": main()
