#!/usr/bin/env python3
"""Fail-closed verifier for B-070's root-worker deployment truth.

B-070 is closed by the honest path: the root Worker never shipped a staging
target, so the false ``[env.staging]`` declaration and its staging-to-prod
promotion claim are removed.  This verifier deliberately uses a closed,
named population of shipped surfaces.  It does not grep historical audit
records.  It does, however, pin the known unrelated analytics Worker's staging
sentinel so the root-worker boundary cannot be "verified" by deleting another
Worker's valid staging configuration.

The gate has two independent halves:

* configuration truth: no staging-like root Worker environment exists, while
  the complete five-environment production fleet and its workflow matrix do;
* claim truth: the named active config/runbook/tool surfaces no longer promise
  a root staging deploy or automatic staging-to-production promotion.

Every check returns a named gap.  An empty gap list is the only ``done`` state.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
import tomllib
from pathlib import Path

try:
    import yaml
except ImportError as error:  # pragma: no cover - verifier instrumentation failure
    yaml = None
    _YAML_IMPORT_ERROR = error


ROOT_WORKER_CONFIG = Path("wrangler.toml")
PROD_WORKFLOW = Path(".github/workflows/cf-deploy-prod.yml")
BACKLOG = Path("BACKLOG.md")
DEPLOYMENT_DOC = Path("docs/operator/deployment-environments.md")
CHANGELOG = Path("changelog.d/070-staging-declaration-removed.md")
ANALYTICS_WORKER_CONFIG = Path("apps/analytics-worker/wrangler.toml")
ANALYTICS_WORKER_PACKAGE = Path("apps/analytics-worker/package.json")

# These are active, shipped surfaces.  Historical sealed reports and unrelated
# application Workers are intentionally not in this population.
CLAIM_SURFACES: dict[str, tuple[Path, tuple[re.Pattern[str], ...]]] = {
    "wrangler-staging-mirror-claim": (
        ROOT_WORKER_CONFIG,
        (
            re.compile(r"1:1\s+mirror\s+of\s+prod", re.IGNORECASE),
            re.compile(
                r"canary\s*\+\s*rollout\s+flows\s+promote\s+artifacts\s+staging\s*[-→>]\s*prod",
                re.IGNORECASE,
            ),
        ),
    ),
    "wrangler-staging-comment-claim": (
        ROOT_WORKER_CONFIG,
        (
            # These are retired root-worker comments, not a blanket ban on
            # mentioning unrelated Workers' staging environments.
            re.compile(r"^#\s*ALL envs .*\[env\.staging\]", re.IGNORECASE | re.MULTILINE),
            re.compile(r"^#\s*Default\s*=\s*dev/staging", re.IGNORECASE | re.MULTILINE),
            re.compile(r"^#\s*Scope note:.*\[env\.staging\]", re.IGNORECASE | re.MULTILINE),
        ),
    ),
    "d1-runner-staged-rollout-claim": (
        Path("scripts/d1-migration-runner.sh"),
        (
            re.compile(r"canonical entrypoint for staged D1 schema rollouts", re.IGNORECASE),
            re.compile(r"\(dev\s*→\s*staging\s*→\s*prod\)", re.IGNORECASE),
        ),
    ),
    "d1-runner-root-staging-mode": (
        Path("scripts/d1-migration-runner.sh"),
        (
            re.compile(r"--env\s+staging", re.IGNORECASE),
            re.compile(r"dev\|staging\|prod", re.IGNORECASE),
        ),
    ),
    "residency-promotion-claim": (
        Path("scripts/verify-lgpd-residency.py"),
        (re.compile(r"gates promotion to\s+production", re.IGNORECASE),),
    ),
    "d1-runbook-staged-rollout-claim": (
        Path("specs/_runbooks/RB-D1-MIGRATION-APPLY.md"),
        (
            re.compile(r"^## 3\. Apply procedure \(dev\s*→\s*staging\s*→\s*prod canary\)", re.MULTILINE),
            re.compile(r"scripts/d1-migration-runner\.sh\s+--env\s+staging", re.IGNORECASE),
            re.compile(r"wrangler\s+d1\s+(?:export|migrations|execute)[^\n]*--env\s+staging", re.IGNORECASE),
        ),
    ),
    "ga-cutover-live-staging-claim": (
        Path("specs/_runbooks/RB-GA-CUTOVER.md"),
        (
            re.compile(r"^## 2\. T-24h staging \+ endurance", re.MULTILINE),
            re.compile(r"^### 2\.1 Deploy to staging", re.MULTILINE),
            re.compile(r"pnpm\s+--filter\s+@corelink/worker\s+run\s+deploy:staging"),
            re.compile(r"against a real staging environment", re.IGNORECASE),
            re.compile(r"against\s+staging\b", re.IGNORECASE),
            re.compile(r"staging\s+baseline", re.IGNORECASE),
            re.compile(r"staging-[12]", re.IGNORECASE),
            re.compile(r"for\s+purpose\s+in[^\n]*\bstaging\b", re.IGNORECASE),
            re.compile(r"wrangler\s+r2\s+bucket\s+(?:list|create)[^\n]*\bstag(?:e|ing)", re.IGNORECASE),
            re.compile(r"--env(?:=|\s+)production\b", re.IGNORECASE),
        ),
    ),
}

GA_CUTOVER_REQUIRED_MARKERS = (
    "### 3.1 Step 1 — Verify production R2 bindings",
    "PROD_ENVS=(prod prod-sam prod-lhr prod-nrt prod-syd)",
    'wrangler deploy --env "$env" --dry-run >/dev/null',
    "PROD_BUCKETS=(",
    'BUCKET_INVENTORY="$(wrangler r2 bucket list)"',
    'grep -F "$bucket" <<<"$BUCKET_INVENTORY" >/dev/null',
    "wrangler deploy --gradual=1 --env prod",
    "wrangler secret list --env prod",
    "wrangler cron trigger --env prod --cron-name=audit-chain-neon-sync",
    "wrangler cron trigger --env prod --cron-name=dsr-statuspage-publish",
)

ANALYTICS_STAGING_D1 = {
    "binding": "ANALYTICS_DB",
    "database_name": "corelink-analytics-staging",
    "database_id": "PLACEHOLDER_ANALYTICS_DB_STAGING_ID",
    "migrations_dir": "migrations",
}

EXPECTED_PRODUCTION_R2_POPULATION = 19

# This is an independent desired-state pin, deliberately not inferred from
# wrangler.toml.  Deriving the expected value from the configuration under test
# would make a fleet-wide stale repin self-validating.  Keep this aligned with
# the tag blessed by the production container build/repin change; the separate
# check-container-pin-fresh.sh gate proves that its SHA still reflects the
# container-affecting source at deploy time.
EXPECTED_PRODUCTION_CONTAINER_TAG = "2cd609d25-r1"

REQUIRED_PROD_ENVS = {
    "prod": "corelink-prod",
    "prod-sam": "corelink-prod-sam",
    "prod-lhr": "corelink-prod-lhr",
    "prod-nrt": "corelink-prod-nrt",
    "prod-syd": "corelink-prod-syd",
}
EXPECTED_WORKFLOW_MATRIX = (
    'matrix={"env":["prod","prod-sam","prod-lhr","prod-nrt","prod-syd"]}'
)

# The production fleet is a deliberately closed topology.  This is kept here
# as a small, reviewable contract so removing a binding cannot be hidden by a
# weaker "env exists" check.  Values are committed configuration, not secrets.
_KV = (
    {"binding": "METADATA_KV", "id": "56f8e99f36ad4ec2aa3b876562409ccc"},
    {"binding": "CLERK_JWKS_KV", "id": "924c6c0f9ee4439f96ec3a75a98eef8b"},
    {"binding": "NEGATIVE_CACHE_KV", "id": "b024aef6d57f4a49a4c1715563794392"},
)
_D1 = (
    {
        "binding": "CONFIG_DB",
        "database_name": "corelink-config-prod",
        "database_id": "d64742ea-e102-40b2-a844-ff02e3f94562",
        "migrations_dir": "migrations/d1",
    },
)
_DO = (
    {"name": "CORELINK_SERVER", "class_name": "CoreLinkServer"},
    {"name": "ROLLOUT_DO", "class_name": "RolloutController"},
    {"name": "EVENT_LOG_DO", "class_name": "EventLogDO"},
    {"name": "REPLICATION_COORDINATOR_DO", "class_name": "ReplicationCoordinatorDO"},
    {"name": "REQUEST_METER_COORDINATOR_DO", "class_name": "RequestMeterCoordinatorDO"},
    {"name": "REQUEST_METER_SHARD_DO", "class_name": "RequestMeterShardDO"},
)
_R2_COMMON = (
    {"binding": "AC_BUCKET_SAM", "bucket_name": "corelink-ac-sam"},
    {"binding": "AC_BUCKET_IAD", "bucket_name": "corelink-ac-iad"},
    {"binding": "AC_BUCKET_NRT", "bucket_name": "corelink-ac-nrt"},
    {"binding": "AC_BUCKET_SYD", "bucket_name": "corelink-ac-syd"},
    {"binding": "CHUNK_BUCKET_SAM", "bucket_name": "corelink-chunk-sam"},
    {"binding": "CHUNK_BUCKET_IAD", "bucket_name": "corelink-chunk-iad"},
    {"binding": "CHUNK_BUCKET_LHR", "bucket_name": "corelink-chunk-lhr"},
    {"binding": "CHUNK_BUCKET_NRT", "bucket_name": "corelink-chunk-nrt"},
    {"binding": "CHUNK_BUCKET_SYD", "bucket_name": "corelink-chunk-syd"},
    {"binding": "MANIFEST_BUCKET_SAM", "bucket_name": "corelink-manifest-sam"},
    {"binding": "MANIFEST_BUCKET_IAD", "bucket_name": "corelink-manifest-iad"},
    {"binding": "MANIFEST_BUCKET_LHR", "bucket_name": "corelink-manifest-lhr"},
    {"binding": "MANIFEST_BUCKET_NRT", "bucket_name": "corelink-manifest-nrt"},
    {"binding": "MANIFEST_BUCKET_SYD", "bucket_name": "corelink-manifest-syd"},
)


def _r2(cas: dict, lhr: dict | None = None) -> tuple[dict, ...]:
    entries = ({"binding": "CAS_BUCKET", **cas},) + _R2_COMMON[:2]
    if lhr:
        entries += (lhr,)
    else:
        entries += ({"binding": "AC_BUCKET_LHR", "bucket_name": "corelink-ac-lhr"},)
    return entries + _R2_COMMON[2:]


def _expected_topology() -> dict[str, dict[str, tuple]]:
    routes = {
        "prod": ({"pattern": "corelink-api.humangr.com/*", "zone_name": "humangr.com"},
                  {"pattern": "corelink-oci.humangr.com/*", "zone_name": "humangr.com"}),
        "prod-sam": ({"pattern": "sam.corelink-api.humangr.com", "custom_domain": True},),
        "prod-lhr": ({"pattern": "lhr.corelink-api.humangr.com", "custom_domain": True},),
        "prod-nrt": ({"pattern": "nrt.corelink-api.humangr.com", "custom_domain": True},),
        "prod-syd": ({"pattern": "syd.corelink-api.humangr.com", "custom_domain": True},),
    }
    cas = {
        "prod": {"bucket_name": "corelink-cas-prod"},
        "prod-sam": {"bucket_name": "corelink-cas-prod"},
        "prod-lhr": {"bucket_name": "corelink-cas-eu", "jurisdiction": "eu"},
        "prod-nrt": {"bucket_name": "corelink-cas-apac"},
        "prod-syd": {"bucket_name": "corelink-cas-prod"},
    }
    lhr = {"binding": "AC_BUCKET_LHR", "bucket_name": "corelink-ac-eu", "jurisdiction": "eu"}
    images = {
        name: f"registry.cloudflare.com/6a1fc1c626fc2628823e60b9db01f5cd/{worker}-corelinkserver-prod:{EXPECTED_PRODUCTION_CONTAINER_TAG}"
        for name, worker in REQUIRED_PROD_ENVS.items()
    }
    result = {}
    for name in REQUIRED_PROD_ENVS:
        result[name] = {
            "routes": routes[name],
            "r2_buckets": _r2(cas[name], lhr if name == "prod-lhr" else None),
            "kv_namespaces": _KV,
            "d1_databases": _D1,
            "durable_objects": _DO,
            "containers": ({"class_name": "CoreLinkServer", "image": images[name],
                            "max_instances": 200, "instance_type": "basic"},),
        }
    return result


EXPECTED_PROD_TOPOLOGY = _expected_topology()


class InstrumentError(RuntimeError):
    """A required verifier input is missing or cannot be parsed."""


def _read(root: Path, path: Path) -> str:
    try:
        return (root / path).read_text(encoding="utf-8")
    except (OSError, UnicodeError) as error:
        raise InstrumentError(f"cannot read {path}: {error}") from error


def _load_wrangler(root: Path) -> tuple[str, dict]:
    text = _read(root, ROOT_WORKER_CONFIG)
    try:
        parsed = tomllib.loads(text)
    except tomllib.TOMLDecodeError as error:
        raise InstrumentError(f"cannot parse {ROOT_WORKER_CONFIG}: {error}") from error
    return text, parsed


def _active_lines(text: str) -> str:
    """Drop full-line comments for workflow command checks.

    A comment describing a retired command must not reopen the live deployment
    contract.  Inline comments are also removed; URLs in comments are not
    deployment wiring.
    """

    return "\n".join(line.split("#", 1)[0] for line in text.splitlines())


def _freeze(value: object) -> object:
    """Make TOML/YAML-shaped values comparable without ordering noise."""

    if isinstance(value, dict):
        return tuple(sorted((str(key), _freeze(item)) for key, item in value.items()))
    if isinstance(value, (list, tuple)):
        return tuple(sorted((_freeze(item) for item in value), key=repr))
    return value


def _topology_fingerprint(env: dict) -> dict[str, object]:
    durable_objects = env.get("durable_objects", {})
    bindings = (
        durable_objects.get("bindings", [])
        if isinstance(durable_objects, dict)
        else durable_objects
    )
    return {
        "routes": _freeze(env.get("routes", [])),
        "r2_buckets": _freeze(env.get("r2_buckets", [])),
        "kv_namespaces": _freeze(env.get("kv_namespaces", [])),
        "d1_databases": _freeze(env.get("d1_databases", [])),
        "durable_objects": _freeze(bindings),
        "containers": _freeze(env.get("containers", [])),
    }


def _unrelated_worker_staging_gaps(root: Path) -> list[str]:
    """Keep the B-070 boundary honest without banning another Worker's staging."""

    config_text = _read(root, ANALYTICS_WORKER_CONFIG)
    try:
        config = tomllib.loads(config_text)
    except tomllib.TOMLDecodeError as error:
        raise InstrumentError(f"cannot parse {ANALYTICS_WORKER_CONFIG}: {error}") from error

    envs = config.get("env")
    staging = envs.get("staging") if isinstance(envs, dict) else None
    gaps: list[str] = []
    if not isinstance(staging, dict):
        gaps.append("unrelated-worker-staging-table")
    else:
        if staging.get("name") != "corelink-analytics-staging":
            gaps.append("unrelated-worker-staging-name")
        vars_table = staging.get("vars")
        if not isinstance(vars_table, dict) or vars_table.get("ENVIRONMENT") != "staging":
            gaps.append("unrelated-worker-staging-environment")
        observability = staging.get("observability")
        if (
            not isinstance(observability, dict)
            or observability.get("enabled") is not True
            or observability.get("head_sampling_rate") != 1
        ):
            gaps.append("unrelated-worker-staging-observability")
        if _freeze(staging.get("d1_databases", [])) != _freeze([ANALYTICS_STAGING_D1]):
            gaps.append("unrelated-worker-staging-d1")
        triggers = staging.get("triggers")
        if not isinstance(triggers, dict) or triggers.get("crons") != ["0 8 * * 1"]:
            gaps.append("unrelated-worker-staging-trigger")

    package_text = _read(root, ANALYTICS_WORKER_PACKAGE)
    try:
        package = json.loads(package_text)
    except json.JSONDecodeError as error:
        raise InstrumentError(f"cannot parse {ANALYTICS_WORKER_PACKAGE}: {error}") from error
    scripts = package.get("scripts") if isinstance(package, dict) else None
    if not isinstance(scripts, dict):
        gaps.append("unrelated-worker-staging-scripts")
    else:
        if scripts.get("deploy:staging") != "wrangler deploy --env staging":
            gaps.append("unrelated-worker-staging-deploy")
        if scripts.get("schema:apply:staging") != (
            "wrangler d1 execute corelink-analytics-staging --file src/schema.sql --remote"
        ):
            gaps.append("unrelated-worker-staging-schema")
    return gaps


def _runbook_r2_inventory(text: str) -> list[str] | None:
    match = re.search(r"(?ms)^PROD_BUCKETS=\(\s*(.*?)^\)", text)
    if not match:
        return None
    return re.findall(r"\bcorelink-[a-z0-9-]+\b", _active_lines(match.group(1)))


def assess(root: Path = Path(".")) -> list[str]:
    """Return named B-070 gaps; an empty list means the item is done."""

    wrangler_text, wrangler = _load_wrangler(root)
    workflow = _read(root, PROD_WORKFLOW)
    ga_cutover = _read(root, Path("specs/_runbooks/RB-GA-CUTOVER.md"))
    backlog = _read(root, BACKLOG)
    deployment_doc = _read(root, DEPLOYMENT_DOC)
    changelog = _read(root, CHANGELOG)
    gaps: list[str] = []

    if wrangler.get("main") != "worker/src/index.ts":
        gaps.append("root-worker-main")

    envs = wrangler.get("env")
    if not isinstance(envs, dict):
        gaps.append("production-env-table-missing")
        envs = {}

    # Catch both the original name and a renamed stage/staging/canary table.
    for name in envs:
        if re.search(r"(?:^|[-_])(stage|staging|canary)(?:$|[-_])", str(name), re.IGNORECASE):
            gaps.append(f"staging-like-env:{name}")

    for name, expected_worker_name in REQUIRED_PROD_ENVS.items():
        env = envs.get(name)
        if not isinstance(env, dict):
            gaps.append(f"production-env-missing:{name}")
            continue
        if env.get("name") != expected_worker_name:
            gaps.append(f"production-env-name:{name}")
        if not isinstance(env.get("containers"), list) or not env["containers"]:
            gaps.append(f"production-env-containers:{name}")
        expected = EXPECTED_PROD_TOPOLOGY[name]
        actual_fingerprint = _topology_fingerprint(env)
        expected_fingerprint = _topology_fingerprint(expected)
        for family, actual_value in actual_fingerprint.items():
            if actual_value != expected_fingerprint[family]:
                gaps.append(f"production-topology:{name}:{family}")

    # The root config must contain neither a parsed staging environment nor an
    # active staging header.  Relevant comments are checked as claim surfaces
    # below; unrelated Workers' staging notes remain allowed.
    if re.search(r"(?m)^\s*\[\[?env\.(?:stage|staging|canary)(?:[.\]])", _active_lines(wrangler_text), re.IGNORECASE):
        gaps.append("staging-like-env-header")

    active_workflow = _active_lines(workflow)
    if EXPECTED_WORKFLOW_MATRIX not in active_workflow:
        gaps.append("production-workflow-matrix")
    if re.search(r"(?i)wrangler\s+deploy[^\n]*--env(?:=|\s+)staging", active_workflow):
        gaps.append("staging-workflow-deploy")
    if re.search(r"(?i)--env(?:=|\s+)staging[^\n]*wrangler\s+deploy", active_workflow):
        gaps.append("staging-workflow-deploy-reversed")

    production_r2_names: set[str] = set()
    for name in REQUIRED_PROD_ENVS:
        env = envs.get(name)
        bindings = env.get("r2_buckets") if isinstance(env, dict) else None
        if not isinstance(bindings, list) or not bindings:
            gaps.append(f"production-r2-bindings-missing:{name}")
            continue
        for binding in bindings:
            bucket_name = binding.get("bucket_name") if isinstance(binding, dict) else None
            if not isinstance(bucket_name, str) or not bucket_name:
                gaps.append(f"production-r2-binding-invalid:{name}")
                continue
            production_r2_names.add(bucket_name)

    if len(production_r2_names) != EXPECTED_PRODUCTION_R2_POPULATION:
        gaps.append(f"production-r2-population:{len(production_r2_names)}")
    runbook_r2_tokens = _runbook_r2_inventory(ga_cutover)
    if runbook_r2_tokens is None:
        gaps.append("runbook-r2-inventory-missing")
    else:
        runbook_r2_names = set(runbook_r2_tokens)
        if len(runbook_r2_tokens) != EXPECTED_PRODUCTION_R2_POPULATION:
            gaps.append(f"runbook-r2-population:{len(runbook_r2_tokens)}")
        if len(runbook_r2_tokens) != len(runbook_r2_names):
            gaps.append("runbook-r2-inventory-duplicate")
        if runbook_r2_names != production_r2_names:
            gaps.append("runbook-r2-inventory-mismatch")
        for missing in sorted(production_r2_names - runbook_r2_names):
            gaps.append(f"runbook-r2-missing:{missing}")
        for extra in sorted(runbook_r2_names - production_r2_names):
            gaps.append(f"runbook-r2-extra:{extra}")

    if yaml is None:
        raise InstrumentError(f"PyYAML is required for workflow semantics: {_YAML_IMPORT_ERROR}")
    try:
        workflow_data = yaml.safe_load(workflow)
    except yaml.YAMLError as error:
        raise InstrumentError(f"cannot parse {PROD_WORKFLOW}: {error}") from error
    jobs = workflow_data.get("jobs") if isinstance(workflow_data, dict) else None
    gate = jobs.get("gate-secrets-checklist") if isinstance(jobs, dict) else None
    deploy = jobs.get("deploy") if isinstance(jobs, dict) else None
    gate_outputs = gate.get("outputs", {}) if isinstance(gate, dict) else {}
    gate_steps = gate.get("steps", []) if isinstance(gate, dict) else []
    matrix_step = next((step for step in gate_steps if step.get("id") == "set-matrix"), None)
    if gate_outputs.get("matrix") != "${{ steps.set-matrix.outputs.matrix }}" or not isinstance(matrix_step, dict):
        gaps.append("workflow-matrix-output")
    elif EXPECTED_WORKFLOW_MATRIX not in str(matrix_step.get("run", "")):
        gaps.append("workflow-matrix-output")
    deploy_strategy = deploy.get("strategy", {}) if isinstance(deploy, dict) else {}
    if deploy_strategy.get("matrix") != "${{ fromJson(needs.gate-secrets-checklist.outputs.matrix) }}":
        gaps.append("workflow-matrix-strategy")
    deploy_env = deploy.get("env", {}) if isinstance(deploy, dict) else {}
    if deploy_env.get("WRANGLER_ENV") != "${{ matrix.env }}":
        gaps.append("workflow-matrix-env")
    deploy_steps = deploy.get("steps", []) if isinstance(deploy, dict) else []
    deploy_runs = "\n".join(str(step.get("run", "")) for step in deploy_steps if isinstance(step, dict))
    if 'bash scripts/deploy-container-prod.sh --apply --env "${WRANGLER_ENV}"' not in deploy_runs:
        gaps.append("workflow-env-deploy")

    for reason, (path, patterns) in CLAIM_SURFACES.items():
        text = _read(root, path)
        for pattern in patterns:
            if pattern.search(text):
                gaps.append(reason)
                break

    for marker in GA_CUTOVER_REQUIRED_MARKERS:
        if marker not in ga_cutover:
            gaps.append(f"ga-cutover-required-marker:{marker}")

    gaps.extend(_unrelated_worker_staging_gaps(root))

    # The canonical operator document must make the selected disposition
    # explicit, including the boundary around the known unrelated Worker.
    required_doc_markers = (
        "five production targets",
        "no `[env.staging]` declaration",
        "no staging leg",
        "no automatic staging-to-production promotion",
        "owner-provisioned external",
    )
    for marker in required_doc_markers:
        if marker not in deployment_doc:
            gaps.append(f"deployment-doc:{marker}")

    # Do not allow the backlog or changelog to claim completion without the
    # verifier that proves it.  This ties narrative closure to the semantic
    # guard rather than to a hand-edited status alone.
    heading = re.search(r"(?m)^### B-070\b", backlog)
    block_match = (
        re.search(r"(?ms)^```backlog\n(.*?)^```", backlog[heading.end() :])
        if heading
        else None
    )
    if not block_match or not re.search(r"(?m)^id:\s*B-070\s*$", block_match.group(1)):
        gaps.append("backlog-b070-block-missing")
    else:
        block = block_match.group(1)
        if not re.search(r"(?m)^status:\s*done\s*$", block):
            gaps.append("backlog-b070-not-done")
        if "verify_b070_staging_truth.py --expect done" not in block:
            gaps.append("backlog-b070-verifier-wiring")

    required_changelog_markers = (
        "B-070",
        "removed",
        "no root-worker staging deployment",
        "no automatic staging-to-production promotion",
    )
    for marker in required_changelog_markers:
        if marker.lower() not in changelog.lower():
            gaps.append(f"changelog:{marker}")

    return gaps


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path("."))
    parser.add_argument("--expect", choices=("open", "done"), required=True)
    args = parser.parse_args(argv)
    try:
        gaps = assess(args.root)
    except InstrumentError as error:
        print(f"instrument error: {error}", file=sys.stderr)
        return 2

    actual = "open" if gaps else "done"
    print(f"B-070 {actual}: {len(gaps)} gap(s)")
    for gap in gaps:
        print(f"- {gap}")
    if actual != args.expect:
        print(f"expected {args.expect}, found {actual}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
