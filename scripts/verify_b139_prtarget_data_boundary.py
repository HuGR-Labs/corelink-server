#!/usr/bin/env python3
"""Fail-closed structural guard for pull_request_target trust boundaries."""
from __future__ import annotations

import argparse
import hashlib
import re
from pathlib import Path
from typing import Any

import yaml


APPROVED_CHECKOUT = {
    "9f698171ed81b15d1823a05fc7211befd50c8ae0",
    "9c091bb21b7c1c1d1991bb908d89e4e9dddfe3e0",
}
KNOWN = {"backlog-verify.yml", "dependabot-policy-trust-boundary.yml",
         "dependabot-policy.yml", "file-size-ratchet.yml", "secrets-drift.yml"}
TARGET_ALLOWLIST = KNOWN | {"actionlint.yml", "dependabot-auto-merge.yml", "pr-labels.yml", "welcome-first-pr.yml"}
SUPPRESSION_RULE = "yaml.github-actions.security.pull-request-target-code-checkout.pull-request-target-code-checkout"
EXPECTED_CHECKOUTS = {
    "backlog-verify.yml": [("9f698171ed81b15d1823a05fc7211befd50c8ae0", "${{ github.event.pull_request.head.sha || github.sha }}", "_candidate", 1, False), ("9f698171ed81b15d1823a05fc7211befd50c8ae0", "${{ github.event.pull_request.base.sha || github.sha }}", "_base", 1, False)],
    "dependabot-policy-trust-boundary.yml": [("9c091bb21b7c1c1d1991bb908d89e4e9dddfe3e0", "${{ github.event.pull_request.base.sha }}", "_base", 1, False), ("9c091bb21b7c1c1d1991bb908d89e4e9dddfe3e0", "refs/pull/${{ github.event.pull_request.number }}/merge", "_pr-data", 1, False)],
    "dependabot-policy.yml": [("9c091bb21b7c1c1d1991bb908d89e4e9dddfe3e0", "refs/pull/${{ github.event.pull_request.number }}/merge", "_pr-data", 2, False), ("9c091bb21b7c1c1d1991bb908d89e4e9dddfe3e0", "${{ github.event.pull_request.base.sha }}", "_base", 0, False)],
    "file-size-ratchet.yml": [("9c091bb21b7c1c1d1991bb908d89e4e9dddfe3e0", "${{ github.event.pull_request.head.sha || github.sha }}", "", 0, True)],
    "secrets-drift.yml": [("9c091bb21b7c1c1d1991bb908d89e4e9dddfe3e0", "${{ github.event.pull_request.base.sha || github.sha }}", ".trusted", None, False), ("9c091bb21b7c1c1d1991bb908d89e4e9dddfe3e0", "${{ github.event.pull_request.head.sha }}", ".candidate", 1, False)],
}
EXPECTED_ACTIONS = {
    "actions/checkout@9f698171ed81b15d1823a05fc7211befd50c8ae0", "actions/checkout@9c091bb21b7c1c1d1991bb908d89e4e9dddfe3e0",
    "dependabot/fetch-metadata@25dd0e34f4fe68f24cc83900b1fe3fe149efef98", "taiki-e/install-action@07b4745e0c39a41822af610387492e3e53aa222b",
    "actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a",
}
EXPECTED_DOCUMENT_DIGESTS = {
    "backlog-verify.yml": "ae50530b65f97c69c52b9fccd01f0d7b0ed4a09a46640c6c2fa842fa8140eb5d",
    "dependabot-policy-trust-boundary.yml": "41fcc16dc71db96537a55378a940b6038bcf832e0b99c71828d65e1313adad24",
    "dependabot-policy.yml": "6183e9ac87fdaf7c279c74d8e5bd7e7894369dbe79ee99f2c0ee87e777b5d516",
    "file-size-ratchet.yml": "d70f0d8de137f1a891610f9bbb993a2ef4077882e50b2b7477886f7ccbcad7eb",
    "secrets-drift.yml": "b89a89ddd67a0715c69653a39ce3a839d53fbb1d6d1ecfa279c40bef170d0170",
    "actionlint.yml": "74f365c7270e852b80a1d5c02af3157456c191bb2848227061b1ae6345bd05b4",
    "dependabot-auto-merge.yml": "4d8b6c66a3b9ed423d4e5ce057dff3a02155982428d9b1c06d440e0ea15e469b",
    "pr-labels.yml": "7b15def5757cb513c8a81ec49553acef8baea561d9bb981cfe0417f3980cc5ce",
    "welcome-first-pr.yml": "55a53db698ed017f91d4280d343f3a13dbf57ae0bf489b65f8d20552e1f30c16",
}
# Reviewed BASE run bodies.  A changed/new run is a policy change and must be
# explicitly reviewed before this allowlist is updated.
REVIEWED_RUN_DIGESTS = {
    "cadbde59835aa9522312011502794ce317670eeb6109ab7d27dd42bafecdbdcb",
    "bd0ed1f58962449b697e639ea9811541a3ba920ba5e92854b41d4b215d167c26",
    "8978bfa5cfd8f5be6c3a7e1a0c7794c485ca33ffe2fe438787760c47efcee3da",
    "4622ab94dd76b2e4e0c3501f0fbbbc6a3fafaa53f7deaa7586633afda7117ace",
    "dbf049fa4a797f80c5839dac2eac90e31938225803443308f664ab8ad8ff5f9b",
    "40b7047431988063439c69606f230afd9dead4ac0e12ac4bfb02e0794e52878c",
    "5e18da528291d9626773db8825b82a08ca7ca4684eb1b6e34e65e559ab4b6895",
    "99f72f21543c12789cc024dd0aade1b0d20b2fcea999c1c02dc9348e1efcbad3",
    "2ce8944dc765bafdb5c6607e0f5cb937fadc455b5e31b0c2a6c74aede16d8735",
    "992897018d57f2018a681e07b62a95171d03aac03884836d40e85c72e4fe1931",
    "f99648936687e9d803fe12cebe2f3fe892416091b18498e246987c9b9bb6d23e",
    "64d6fd0c456ef362c9c499916975a829c62a7e18bd27f3ce1fb9ca32d50eebab",
    "bcc41c287349a31a4d013d12afc0a54afdbbb6a43c1c616915f4d5e7a020f03e",
    "1f8e71a94504feb3273f28fd86276e73ed342de3170ed3d83a6376afde7dcbd2",
    "ecdc49901a2582fee7136c74730ce020204e9453b9e406c1a36c54dd5aaaf902",
    "e9c5246cadc4be94ccb27822b9e89d0eaf4065d2b41c527b25b3ae622045660f",
    "e7d23ee0c046628e2bb0445b95566b17888f79cd96c5bb42ddcee958b934aba4",
    "529def8410d9efd04133569f5348bd16001409ff136ae967213fcadcb3195d71",
    "077e780a2d1245cf24f9b96a5cf97a0ebff9fcd2f6c1c9ded0e828336fa587ed",
    "ed9d96b121085ad91ea47cb037a785acf415067ec0afc534055f5deab7e80bed",
    "f1b68992eea3fb136e7edd9cd761c03452e4b5ca1124afb9bb8209748b12e364",
    "ba1023fd3ad2577d860122e59cacca7d7381146db54b39282fb6f3a91bd0ab55",
    "48fdd551de70dbb91219fd5a1e6df094b02c299b13fbef1d719e7dcb38db8be1",
    "4c2373a382ab6d8903c539ed6362b2eec1a115b95b61fb621288d785bc4d4df0",
    "46a4a64df782c4d5cb3243cb9c4f2be7d00caaec4e59e267811ff761d9822268",
    "52b956ee857bd330033d3eade5f88b59bbb56b26dbab1beea2325ab5e8441d34",
    "308e754680cd595ebb3e3da409ad267a180b2da1e0a9b05ce7749d26a2b484b4",
    "25613c51943026ccf2d412460f083ed0ea8fe8b0c5e280227299fe7b28a68e3e",
    "b3766b4f588aa79ee90b2a33b88d2130333a93773a90c88553df5f198206315f",
}


class VerificationError(RuntimeError):
    pass


class _Loader(yaml.SafeLoader):
    pass

# GitHub's `on` is a string, unlike YAML 1.1's boolean interpretation.
_Loader.yaml_implicit_resolvers = {
    _ch: list(_resolvers) for _ch, _resolvers in _Loader.yaml_implicit_resolvers.items()
}
for _ch, _resolvers in list(_Loader.yaml_implicit_resolvers.items()):
    _Loader.yaml_implicit_resolvers[_ch] = [
        (_tag, _rx) for _tag, _rx in _resolvers
        if _tag != "tag:yaml.org,2002:bool"
    ]
_BOOL_RE = re.compile(r"^(?:true|True|TRUE|false|False|FALSE)$")
for _ch in "tTfF":
    _Loader.add_implicit_resolver("tag:yaml.org,2002:bool", _BOOL_RE, [_ch])


def _no_dupes(loader: _Loader, node: yaml.MappingNode, deep: bool = False):
    out = {}
    for key, value in node.value:
        k = loader.construct_object(key, deep=deep)
        if k in out:
            raise VerificationError(f"duplicate YAML key: {k!r}")
        out[k] = loader.construct_object(value, deep=deep)
    return out


_Loader.add_constructor(yaml.resolver.BaseResolver.DEFAULT_MAPPING_TAG, _no_dupes)


def _walk(value: Any):
    if isinstance(value, dict):
        yield value
        for v in value.values():
            yield from _walk(v)
    elif isinstance(value, list):
        for v in value:
            yield from _walk(v)


def _text(value: Any) -> str:
    return value if isinstance(value, str) else ""


def _parse(path: Path) -> dict[str, Any]:
    try:
        raw = path.read_text(encoding="utf-8")
        events = list(yaml.parse(raw))
        if any(isinstance(event, yaml.events.AliasEvent) or getattr(event, "anchor", None)
               for event in events):
            raise VerificationError(f"anchors/aliases are unsupported: {path.name}")
        value = yaml.load(raw, Loader=_Loader)
    except (OSError, UnicodeError, yaml.YAMLError) as exc:
        raise VerificationError(f"unsupported YAML {path.name}: {exc}") from exc
    if not isinstance(value, dict):
        raise VerificationError(f"workflow is not a mapping: {path.name}")
    return value


def _checkout_steps(doc: dict[str, Any]):
    for item in _walk(doc):
        if "uses" in item and isinstance(item["uses"], str) and item["uses"].split("@", 1)[0] == "actions/checkout":
            yield item


def _check_known(name: str, doc: dict[str, Any]) -> None:
    import json
    digest = hashlib.sha256(json.dumps(_jsonable(doc), sort_keys=True, separators=(",", ":")).encode()).hexdigest()
    if digest != EXPECTED_DOCUMENT_DIGESTS[name]:
        raise VerificationError(f"{name}: unreviewed workflow structure")
    jobs = doc.get("jobs")
    if not isinstance(jobs, dict):
        raise VerificationError(f"{name}: jobs must be a mapping")
    # A trusted push/schedule lane may contain an additional checkout of the
    # immutable event SHA; it is outside the PR data boundary.  PR candidate /
    # BASE checkouts, however, are an exact ordered contract.
    checkouts = [s for s in _checkout_steps(doc)
                 if _text(s.get("with", {}).get("ref")) != "${{ github.sha }}"]
    expected = EXPECTED_CHECKOUTS[name]
    if len(checkouts) != len(expected):
        raise VerificationError(f"{name}: no checkout steps")
    for step, (sha, expected_ref, expected_path, expected_depth, root_exception) in zip(checkouts, expected):
        uses = step["uses"]
        if uses != f"actions/checkout@{sha}":
            raise VerificationError(f"{name}: checkout is not approved and pinned")
        with_ = step.get("with", {})
        if not isinstance(with_, dict) or with_.get("persist-credentials") is not False:
            raise VerificationError(f"{name}: checkout credentials must be disabled")
        if with_.get("fetch-depth") != expected_depth or _text(with_.get("ref")) != expected_ref or _text(with_.get("path")) != expected_path:
            raise VerificationError(f"{name}: checkout fetch-depth must be 0 or 1")
        ref = _text(with_.get("ref")); path = _text(with_.get("path"))
        # file-size-ratchet is the sole reviewed root-checkout exception.
        if root_exception and not path:
            if with_.get("repository") != "${{ github.event.pull_request.head.repo.full_name || github.repository }}":
                raise VerificationError(f"{name}: root checkout exception is not exact")
            continue
        if not path:
            raise VerificationError(f"{name}: checkout path must isolate data/control")
        if path in {"_candidate", ".candidate", "_pr-data"}:
            if not ("head.sha" in ref or "refs/pull/" in ref):
                raise VerificationError(f"{name}: candidate ref is movable or malformed")
        elif path in {"_base", ".trusted"}:
            if "head.ref" in ref or "pull_request.head" in ref and "base.sha" not in ref:
                raise VerificationError(f"{name}: BASE checkout uses candidate ref")
        else:
            raise VerificationError(f"{name}: unknown checkout path {path!r}")
    for item in _walk(doc):
        if "uses" in item:
            action = _text(item["uses"])
            if action not in EXPECTED_ACTIONS:
                raise VerificationError(f"{name}: local or unpinned action")
        permissions = item.get("permissions")
        if isinstance(permissions, dict):
            for key, val in permissions.items():
                if val not in ("read", "none"):
                    raise VerificationError(f"{name}: permission {key} is writable")
        if "working-directory" in item:
            wd = _text(item["working-directory"])
            if wd in {"_candidate", ".candidate", "_pr-data"} or "candidate" in wd:
                raise VerificationError(f"{name}: candidate working-directory")
        for key in ("run", "shell", "env", "with"):
            blob = repr(item.get(key, ""))
            if "GITHUB_ACTION_PATH" in blob or "PATH" in blob and "candidate" in blob:
                raise VerificationError(f"{name}: candidate can influence execution")
            if key in {"run", "shell"} and "pull_request.head.ref" in blob:
                raise VerificationError(f"{name}: mutable PR ref reaches command execution")
        run = item.get("run")
        if isinstance(run, str):
            if hashlib.sha256(run.encode()).hexdigest() not in REVIEWED_RUN_DIGESTS:
                raise VerificationError(f"{name}: new or changed run step")
        if isinstance(run, str) and any(x in run for x in ("_candidate/", ".candidate/", "_pr-data/")):
            raise VerificationError(f"{name}: executes candidate content")

    # The one root checkout is deliberately allowed only because this exact
    # BASE-owned command extracts the validator with `git show` and then runs
    # that extracted script.  Keep this assertion independent of the document
    # digest so a future snapshot update cannot turn the exception into PR-code
    # execution by accident.
    if name == "file-size-ratchet.yml":
        runs = [item["run"] for item in _walk(doc) if isinstance(item.get("run"), str)]
        if len(runs) != 1 or "git show" not in runs[0] or "validate_file_size_ratchet.py" not in runs[0]:
            raise VerificationError(f"{name}: root checkout must use trusted git-show extraction")


def verify_workflows(workflow_dir: Path) -> None:
    if not workflow_dir.is_dir():
        raise VerificationError(f"missing workflow directory: {workflow_dir}")
    for path in sorted(workflow_dir.iterdir()):
        if path.suffix not in {".yml", ".yaml"}:
            continue
        if path.is_symlink():
            raise VerificationError(f"symlinked workflow is unsupported: {path.name}")
        doc = _parse(path)
        # PyYAML parses the YAML 1.1 key ``on`` as True; inspect keys robustly.
        if "on" not in doc or True in doc:
            raise VerificationError(f"{path.name}: trigger key is ambiguous or missing")
        target = doc["on"]
        if not isinstance(target, dict) or "pull_request_target" not in target:
            continue
        raw = path.read_text(encoding="utf-8")
        if (raw.count("${{") != raw.count("}}")
                or re.search(r"\$\{\{[^}]*\|\|\s*(?:\|\||}})", raw)):
            raise VerificationError(f"malformed expression in {path.name}")
        if path.name in KNOWN:
            _check_known(path.name, doc)
            continue
        if path.name not in TARGET_ALLOWLIST:
            raise VerificationError(f"{path.name}: unreviewed pull_request_target workflow")
        import json
        if hashlib.sha256(json.dumps(_jsonable(doc), sort_keys=True, separators=(",", ":")).encode()).hexdigest() != EXPECTED_DOCUMENT_DIGESTS[path.name]:
            raise VerificationError(f"{path.name}: unreviewed workflow structure")
        # Unknown target workflows may exist, but must not transport or execute PR code.
        for step in _checkout_steps(doc):
            with_ = step.get("with", {})
            ref = _text(with_.get("ref")) if isinstance(with_, dict) else ""
            if any(x in ref for x in ("head.ref", "head.sha", "refs/pull/")):
                raise VerificationError(f"{path.name}: unknown candidate checkout")


def approved_suppression_sites(root: Path) -> set[tuple[str, int, str]]:
    """Return exact SARIF suppression coordinates after validating the tree."""
    workflow_dir = root / ".github" / "workflows"
    verify_workflows(workflow_dir)
    sites: set[tuple[str, int, str]] = set()
    for path in sorted(workflow_dir.iterdir()):
        if path.name not in KNOWN or path.suffix not in {".yml", ".yaml"}:
            continue
        lines = path.read_text(encoding="utf-8").splitlines()
        found = []
        for line_no, line in enumerate(lines, 1):
            if re.search(r"^\s*(?:-\s+)?uses:\s*actions/checkout@[0-9a-f]{40}\b", line):
                found.append((line_no, line))
        if path.name == "backlog-verify.yml":
            found = found[:2]
        for uses_line_no, uses_line in found:
            # Semgrep reports the checkout mapping's ``name`` key.  Associate
            # each approved checkout with its immediately enclosing step name,
            # and require the suppression annotation on that exact line.
            uses_indent = len(uses_line) - len(uses_line.lstrip())
            name_line_no = None
            for candidate_no in range(uses_line_no - 1, 0, -1):
                candidate = lines[candidate_no - 1]
                step = re.match(r"^(\s*)-\s+name:\s*", candidate)
                if step:
                    name_indent = len(step.group(1))
                    if name_indent + 2 != uses_indent:
                        raise VerificationError(
                            f"{path.name}:{uses_line_no}: checkout name indentation mismatch"
                        )
                    name_line_no = candidate_no
                    break
                if re.match(r"^\s*-\s+", candidate):
                    break
            if name_line_no is None:
                raise VerificationError(f"{path.name}:{uses_line_no}: checkout lacks named step")
            sites.add((path.relative_to(root).as_posix(), name_line_no, SUPPRESSION_RULE))
        for line_no, line in enumerate(lines, 1):
            if "nosemgrep:" in line and not any(line_no == site[1] for site in sites if site[0].endswith(path.name)):
                raise VerificationError(f"{path.name}:{line_no}: extra suppression annotation")
        for site in [s for s in sites if s[0].endswith(path.name)]:
            line = lines[site[1] - 1]
            if line.count("nosemgrep:") != 1:
                raise VerificationError(f"{path.name}:{site[1]}: multiple suppression annotations")
            annotation = re.search(
                rf"#\s*nosemgrep:\s*{re.escape(SUPPRESSION_RULE)}\s+#\s*reason:\s+([^#\n]+?)\s+ADR-0101\s*$",
                line,
            )
            if not annotation or not annotation.group(1).strip():
                raise VerificationError(f"{path.name}:{site[1]}: suppression lacks exact reason")
    expected_counts = {"backlog-verify.yml": 2, "dependabot-policy-trust-boundary.yml": 2,
                       "dependabot-policy.yml": 2, "file-size-ratchet.yml": 1, "secrets-drift.yml": 2}
    for name, count in expected_counts.items():
        if sum(1 for site in sites if site[0].endswith(name)) != count:
            raise VerificationError(f"{name}: suppression site census mismatch")
    return sites


def _jsonable(value: Any) -> Any:
    if isinstance(value, dict):
        return {("True" if k == "on" else str(k)): _jsonable(v) for k, v in value.items()}
    if isinstance(value, list):
        return [_jsonable(v) for v in value]
    return value


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("workflow_dir", nargs="?", type=Path, default=Path(".github/workflows"))
    args = parser.parse_args()
    try:
        verify_workflows(args.workflow_dir)
    except VerificationError as exc:
        print(f"B139 FAIL: {exc}")
        return 1
    print("B139 PASS: pull_request_target data boundaries verified")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
