#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
gen-api-reference.py — Auto-generate the typed REST API reference under
``apps/docs/docs/reference/api/endpoints/`` from
``openapi/corelink-v1.yaml``.

For every ``paths.<path>.<method>`` entry the generator emits one MDX
page containing:

* HTTP method + canonical path (badge + heading)
* Summary + description (OpenAPI ``summary``/``description``)
* Tag, operationId, security requirements, rate-limit class
* Parameter table (path / query / header)
* Request body schema (typed; ``$ref`` resolved one level)
* Response status table + per-status schema
* Example invocations in **five** languages:
  curl, Rust (``reqwest``-style), Python (``httpx``-style),
  Go (``net/http``-style), JavaScript (``fetch``).

A landing page (``index.mdx``) is also written with an alphabetical
endpoint list grouped by tag.

Charter compliance:

* Strict ``argparse`` CLI; ``--help`` exits 0.
* ``--dry-run`` exits 0 without writing files.
* Explicit ``sys.exit`` on every error path (no silent ``except``).
* No external deps beyond ``pyyaml`` (already pinned for
  ``scripts/openapi_sync.py``).

This file is deterministic — running twice on the same YAML emits
byte-identical output (sorted keys, stable ordering).

CI drift gate: ``.github/workflows/api-reference-sync.yml``.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Iterable

try:
    import yaml  # type: ignore[import-untyped]
except ImportError as exc:  # pragma: no cover - bootstrap guard
    sys.stderr.write(
        "fatal: PyYAML is required (pip install pyyaml)\n"
        f"       {exc}\n"
    )
    sys.exit(2)

REPO_ROOT = Path(__file__).resolve().parent.parent
DEFAULT_SPEC = REPO_ROOT / "openapi" / "corelink-v1.yaml"
DEFAULT_OUT = REPO_ROOT / "apps" / "docs" / "docs" / "reference" / "api"

HTTP_METHODS = (
    "get",
    "post",
    "put",
    "patch",
    "delete",
    "head",
    "options",
    "trace",
)

# Tag → display group for the landing page. Tags not in this map fall
# through to the "Other" bucket (printed in alphabetical order).
TAG_GROUPS: dict[str, str] = {
    "signup": "Auth & Onboarding",
    "dpa": "Auth & Onboarding",
    "tier": "Auth & Onboarding",
    "pat": "Auth & Onboarding",
    "users": "Auth & Onboarding",
    "billing": "Billing",
    "privacy-dsr": "Privacy",
    "privacy-consent": "Privacy",
    "data-categories": "Privacy",
    "admin": "Admin",
    "enterprise": "Admin",
    "ops": "Ops",
}

# Rate-limit class taxonomy — mirrors the canonical buckets documented in
# ``specs/policies/rate_limit_policy.md``. The classification here is
# best-effort by tag/path; future work may upgrade this to an
# explicit ``x-rate-limit-class`` extension on each path.
def _rate_limit_class(tag: str, path: str, method: str) -> str:
    if path.startswith("/api/health"):
        return "unmetered"
    if tag in {"signup", "enterprise"}:
        return "anonymous-strict (5 req/min/IP)"
    if tag in {"admin"}:
        return "admin-strict (60 req/min/PAT, dual-approval gate)"
    if tag in {"billing"}:
        return "webhook-source (Stripe signature gated; not user-metered)"
    if tag in {"privacy-dsr"}:
        return "privacy-strict (10 req/hour/subject)"
    if tag in {"privacy-consent"}:
        return "consent-default (60 req/min/tenant)"
    if tag in {"pat"} and method.lower() == "post":
        return "pat-issue (10 req/hour/tenant)"
    return "default (600 req/min/PAT)"


def _slugify_path(path: str) -> str:
    """Stable, filesystem-safe slug from an OpenAPI path expression.

    Examples:
        /v1/signup                       -> v1-signup
        /v1/pats/{pat_id}                -> v1-pats-by-pat_id
        /v1/privacy/dsr/{request_id}/status
            -> v1-privacy-dsr-by-request_id-status
    """
    out = path.strip("/")
    # Replace {var} with by-var BEFORE stripping non-alnum so we keep the
    # variable name.
    out = re.sub(r"\{([^}]+)\}", r"by-\1", out)
    out = out.replace("/", "-")
    out = re.sub(r"[^a-zA-Z0-9_\-]", "-", out)
    out = re.sub(r"-+", "-", out)
    return out.strip("-")


# --------------------------------------------------------------------------- #
# Spec model
# --------------------------------------------------------------------------- #


@dataclass
class Endpoint:
    method: str  # uppercase
    path: str
    op: dict[str, Any]
    tag: str
    operation_id: str
    summary: str
    description: str
    parameters: list[dict[str, Any]]
    request_body: dict[str, Any] | None
    responses: dict[str, dict[str, Any]]
    security: list[dict[str, list[str]]] | None
    rate_limit_class: str = field(init=False)

    def __post_init__(self) -> None:
        self.rate_limit_class = _rate_limit_class(self.tag, self.path, self.method)

    @property
    def slug(self) -> str:
        return f"{self.method.lower()}-{_slugify_path(self.path)}"

    @property
    def file_name(self) -> str:
        return f"{self.slug}.mdx"


@dataclass
class Spec:
    raw: dict[str, Any]

    @property
    def components(self) -> dict[str, Any]:
        return self.raw.get("components", {}) or {}

    @property
    def global_security(self) -> list[dict[str, list[str]]] | None:
        return self.raw.get("security")

    def parameter(self, name: str) -> dict[str, Any]:
        params = self.components.get("parameters", {}) or {}
        if name not in params:
            sys.stderr.write(
                f"fatal: unresolved $ref components/parameters/{name}\n"
            )
            sys.exit(3)
        return params[name]

    def response(self, name: str) -> dict[str, Any]:
        resps = self.components.get("responses", {}) or {}
        if name not in resps:
            sys.stderr.write(
                f"fatal: unresolved $ref components/responses/{name}\n"
            )
            sys.exit(3)
        return resps[name]

    def schema(self, name: str) -> dict[str, Any]:
        schemas = self.components.get("schemas", {}) or {}
        if name not in schemas:
            sys.stderr.write(
                f"fatal: unresolved $ref components/schemas/{name}\n"
            )
            sys.exit(3)
        return schemas[name]

    def example(self, name: str) -> dict[str, Any]:
        exs = self.components.get("examples", {}) or {}
        if name not in exs:
            sys.stderr.write(
                f"fatal: unresolved $ref components/examples/{name}\n"
            )
            sys.exit(3)
        return exs[name]

    def resolve_ref(self, ref: str) -> dict[str, Any]:
        """Resolve a local ``$ref`` like ``#/components/schemas/X``."""
        if not ref.startswith("#/"):
            sys.stderr.write(f"fatal: only local $ref supported (got {ref})\n")
            sys.exit(3)
        parts = ref.lstrip("#/").split("/")
        node: Any = self.raw
        for p in parts:
            if not isinstance(node, dict) or p not in node:
                sys.stderr.write(f"fatal: unresolved $ref {ref}\n")
                sys.exit(3)
            node = node[p]
        if not isinstance(node, dict):
            sys.stderr.write(
                f"fatal: $ref {ref} resolved to non-object {type(node).__name__}\n"
            )
            sys.exit(3)
        return node


# --------------------------------------------------------------------------- #
# Loading & flattening
# --------------------------------------------------------------------------- #


def load_spec(spec_path: Path) -> Spec:
    if not spec_path.is_file():
        sys.stderr.write(f"fatal: spec file not found: {spec_path}\n")
        sys.exit(2)
    with spec_path.open("r", encoding="utf-8") as fh:
        data = yaml.safe_load(fh)
    if not isinstance(data, dict):
        sys.stderr.write("fatal: spec root is not a mapping\n")
        sys.exit(2)
    return Spec(raw=data)


def iter_endpoints(spec: Spec) -> Iterable[Endpoint]:
    paths: dict[str, Any] = spec.raw.get("paths", {}) or {}
    for path in sorted(paths.keys()):
        path_item = paths[path] or {}
        # Path-level shared parameters apply to every operation.
        shared_params = list(path_item.get("parameters", []) or [])
        for method in HTTP_METHODS:
            op = path_item.get(method)
            if not isinstance(op, dict):
                continue
            tags = op.get("tags") or ["other"]
            tag = tags[0] if isinstance(tags, list) and tags else "other"
            params = shared_params + list(op.get("parameters", []) or [])
            yield Endpoint(
                method=method.upper(),
                path=path,
                op=op,
                tag=tag,
                operation_id=op.get("operationId") or f"{method}_{path}",
                summary=op.get("summary") or "",
                description=op.get("description") or "",
                parameters=params,
                request_body=op.get("requestBody"),
                responses=op.get("responses") or {},
                security=op.get("security"),
            )


# --------------------------------------------------------------------------- #
# MDX rendering helpers
# --------------------------------------------------------------------------- #


_MDX_ESCAPE = str.maketrans({
    "<": "&lt;",
    ">": "&gt;",
    "{": "\\{",
    "}": "\\}",
})


def mdx_escape(text: str) -> str:
    """Escape MDX-significant chars in inline text.

    Note: only used in *cell* contexts (tables, inline). Code fences are
    left untouched.
    """
    if text is None:
        return ""
    return str(text).translate(_MDX_ESCAPE).replace("\n", " ")


def front_matter(endpoint: Endpoint) -> str:
    sidebar_label = f"{endpoint.method} {endpoint.path}"
    title = f"{endpoint.method} {endpoint.path}"
    desc = endpoint.summary or f"CoreLink REST endpoint {endpoint.method} {endpoint.path}"
    # YAML in front matter — quote everything to avoid Docusaurus mishaps.
    return (
        "---\n"
        f"id: {endpoint.slug}\n"
        f"title: {json.dumps(title)}\n"
        f"sidebar_label: {json.dumps(sidebar_label)}\n"
        f"description: {json.dumps(desc)}\n"
        f"tags: [{endpoint.tag}, rest-api]\n"
        "---\n\n"
    )


def render_security(endpoint: Endpoint, spec: Spec) -> str:
    sec = endpoint.security if endpoint.security is not None else spec.global_security
    if sec is None or sec == []:
        return "_Public — no authentication required._"
    lines: list[str] = []
    for scheme_map in sec:
        for scheme, scopes in (scheme_map or {}).items():
            scope_repr = f" (scopes: `{', '.join(scopes)}`)" if scopes else ""
            lines.append(f"- `{scheme}`{scope_repr}")
    return "\n".join(lines) if lines else "_Public — no authentication required._"


def _resolve_param(param: dict[str, Any], spec: Spec) -> dict[str, Any]:
    if "$ref" in param:
        return spec.resolve_ref(param["$ref"])
    return param


def _schema_type_repr(schema: dict[str, Any] | None, spec: Spec, depth: int = 0) -> str:
    if not schema:
        return "_unspecified_"
    if "$ref" in schema:
        ref = schema["$ref"]
        name = ref.rsplit("/", 1)[-1]
        return f"[`{name}`](#schema-{name.lower()})"
    if "enum" in schema:
        return "`" + " \\| ".join(json.dumps(v) for v in schema["enum"]) + "`"
    t = schema.get("type")
    fmt = schema.get("format")
    if t == "array":
        return f"`array<{_schema_type_repr(schema.get('items', {}), spec, depth + 1)}>`"
    if t == "object" and depth == 0:
        return "`object`"
    if isinstance(t, list):
        return "`" + " \\| ".join(t) + "`"
    if t and fmt:
        return f"`{t} <{fmt}>`"
    if t:
        return f"`{t}`"
    if "allOf" in schema:
        return " & ".join(
            _schema_type_repr(s, spec, depth + 1) for s in schema["allOf"]
        )
    if "oneOf" in schema:
        return " \\| ".join(
            _schema_type_repr(s, spec, depth + 1) for s in schema["oneOf"]
        )
    return "_unspecified_"


def render_parameters(endpoint: Endpoint, spec: Spec) -> str:
    if not endpoint.parameters:
        return "_None._"
    rows = ["| Name | In | Required | Type | Description |", "| --- | --- | --- | --- | --- |"]
    for raw in endpoint.parameters:
        p = _resolve_param(raw, spec)
        name = p.get("name", "?")
        loc = p.get("in", "?")
        required = "yes" if p.get("required") else "no"
        type_repr = _schema_type_repr(p.get("schema"), spec)
        desc = mdx_escape(p.get("description") or "")
        rows.append(f"| `{name}` | `{loc}` | {required} | {type_repr} | {desc} |")
    return "\n".join(rows)


def _content_schema(content: dict[str, Any] | None) -> tuple[str | None, dict[str, Any] | None]:
    if not content:
        return None, None
    # Prefer application/json, then any other.
    if "application/json" in content:
        return "application/json", content["application/json"].get("schema")
    for mime, body in content.items():
        return mime, body.get("schema") if isinstance(body, dict) else None
    return None, None


def render_request_body(endpoint: Endpoint, spec: Spec) -> str:
    body = endpoint.request_body
    if not body:
        return "_No request body._"
    if "$ref" in body:
        body = spec.resolve_ref(body["$ref"])
    required = body.get("required", False)
    mime, schema = _content_schema(body.get("content"))
    if mime is None:
        return "_No request body._"
    lines = [
        f"- **Media type:** `{mime}`",
        f"- **Required:** {'yes' if required else 'no'}",
        f"- **Schema:** {_schema_type_repr(schema, spec)}",
    ]
    return "\n".join(lines)


def render_responses(endpoint: Endpoint, spec: Spec) -> str:
    if not endpoint.responses:
        return "_No documented responses._"
    rows = ["| Status | Schema | Description |", "| --- | --- | --- |"]
    for status in sorted(endpoint.responses.keys()):
        raw_resp = endpoint.responses[status] or {}
        resp = raw_resp
        if "$ref" in raw_resp:
            resp = spec.resolve_ref(raw_resp["$ref"])
        desc = mdx_escape(resp.get("description") or "")
        _, schema = _content_schema(resp.get("content"))
        type_repr = _schema_type_repr(schema, spec) if schema else "_(empty body)_"
        rows.append(f"| `{status}` | {type_repr} | {desc} |")
    return "\n".join(rows)


# --------------------------------------------------------------------------- #
# Example synthesis
# --------------------------------------------------------------------------- #


_DEFAULT_BASE_URL = "https://corelink-api.humangr.com"


def _base_url(spec: Spec) -> str:
    """Resolve the example base URL.

    Prefers the first **HTTPS** ``servers[].url`` declared in the OpenAPI
    spec (the canonical production host), falling back to
    ``_DEFAULT_BASE_URL`` when the spec declares none. This keeps the
    generated examples pointed at whatever host the contract advertises
    rather than a hardcoded literal that can drift dead.
    """
    servers = spec.raw.get("servers")
    if isinstance(servers, list):
        for entry in servers:
            url = (entry or {}).get("url") if isinstance(entry, dict) else None
            if isinstance(url, str) and url.startswith("https://"):
                return url.rstrip("/")
    return _DEFAULT_BASE_URL


def _example_payload(schema: dict[str, Any] | None, spec: Spec, seen: set[str] | None = None) -> Any:
    """Synthesize a JSON-shaped example value for ``schema``."""
    seen = seen or set()
    if schema is None:
        return {}
    if "$ref" in schema:
        ref = schema["$ref"]
        if ref in seen:
            return {}
        seen = seen | {ref}
        return _example_payload(spec.resolve_ref(ref), spec, seen)
    if "examples" in schema and isinstance(schema["examples"], list) and schema["examples"]:
        return schema["examples"][0]
    if "example" in schema:
        return schema["example"]
    if "enum" in schema and schema["enum"]:
        return schema["enum"][0]
    t = schema.get("type")
    fmt = schema.get("format")
    if t == "array":
        return [_example_payload(schema.get("items", {}), spec, seen)]
    if t == "object" or (t is None and "properties" in schema):
        props = schema.get("properties", {}) or {}
        required = set(schema.get("required", []) or [])
        out: dict[str, Any] = {}
        for k, v in props.items():
            if required and k not in required and len(out) >= 6:
                continue
            out[k] = _example_payload(v, spec, seen)
        return out
    if "allOf" in schema:
        merged: dict[str, Any] = {}
        for sub in schema["allOf"]:
            val = _example_payload(sub, spec, seen)
            if isinstance(val, dict):
                merged.update(val)
        return merged
    if t == "string":
        if fmt == "uuid":
            return "00000000-0000-7000-8000-000000000000"
        if fmt == "email":
            return "test-tenant@example.com"
        if fmt == "uri":
            return "https://corelink.humangr.com/example"
        if fmt == "date-time":
            return "2026-01-01T00:00:00Z"
        pattern = schema.get("pattern")
        if pattern == "^[0-9a-f]{64}$":
            return "0" * 64
        return "string"
    if t == "integer":
        return 0
    if t == "number":
        return 0.0
    if t == "boolean":
        return False
    return None


def _example_body_for_endpoint(endpoint: Endpoint, spec: Spec) -> Any | None:
    body = endpoint.request_body
    if not body:
        return None
    if "$ref" in body:
        body = spec.resolve_ref(body["$ref"])
    content = body.get("content") or {}
    json_body = content.get("application/json") or {}
    examples = json_body.get("examples")
    if isinstance(examples, dict) and examples:
        first = next(iter(examples.values()))
        if isinstance(first, dict) and "$ref" in first:
            return spec.resolve_ref(first["$ref"]).get("value")
        if isinstance(first, dict) and "value" in first:
            return first["value"]
    if "example" in json_body:
        return json_body["example"]
    schema = json_body.get("schema")
    return _example_payload(schema, spec)


def _example_path(endpoint: Endpoint, spec: Spec) -> str:
    """Substitute path parameters with example values."""
    out = endpoint.path
    for raw in endpoint.parameters:
        p = _resolve_param(raw, spec)
        if p.get("in") != "path":
            continue
        name = p.get("name", "")
        value = _example_payload(p.get("schema") or {}, spec)
        if isinstance(value, (dict, list)):
            value = "example"
        out = out.replace("{" + name + "}", str(value))
    return out


def _example_headers(endpoint: Endpoint, spec: Spec) -> list[tuple[str, str]]:
    headers: list[tuple[str, str]] = []
    # Auth header.
    sec = endpoint.security if endpoint.security is not None else spec.global_security
    if sec:
        for scheme_map in sec:
            for scheme in (scheme_map or {}).keys():
                if scheme == "BearerPAT":
                    headers.append(("Authorization", "Bearer <YOUR_PAT>"))
                    break
                if scheme == "StripeSignature":
                    headers.append(("Stripe-Signature", "t=1714000000,v1=<hex>"))
                    break
            if headers:
                break
    # Required header params.
    for raw in endpoint.parameters:
        p = _resolve_param(raw, spec)
        if p.get("in") != "header" or not p.get("required"):
            continue
        name = p.get("name", "")
        value = _example_payload(p.get("schema") or {}, spec)
        if isinstance(value, (dict, list)):
            value = "example"
        headers.append((name, str(value)))
    if endpoint.request_body:
        headers.append(("Content-Type", "application/json"))
    return headers


def _example_url(endpoint: Endpoint, spec: Spec) -> str:
    """Build a full example URL including substituted path and query params."""
    base = _base_url(spec) + _example_path(endpoint, spec)
    query: list[str] = []
    for raw in endpoint.parameters:
        p = _resolve_param(raw, spec)
        if p.get("in") != "query" or not p.get("required"):
            continue
        name = p.get("name", "")
        value = _example_payload(p.get("schema") or {}, spec)
        if isinstance(value, (dict, list)):
            value = "example"
        query.append(f"{name}={value}")
    if query:
        base += "?" + "&".join(query)
    return base


def render_examples(endpoint: Endpoint, spec: Spec) -> str:
    url = _example_url(endpoint, spec)
    headers = _example_headers(endpoint, spec)
    body = _example_body_for_endpoint(endpoint, spec)
    body_json = json.dumps(body, indent=2, sort_keys=True) if body is not None else None

    # ---- curl ----
    curl_parts: list[str] = [f"curl -X {endpoint.method}", f"  '{url}'"]
    for h, v in headers:
        curl_parts.append(f"  -H '{h}: {v}'")
    if body_json is not None:
        body_inline = body_json.replace("'", "'\\''")
        curl_parts.append(f"  -d '{body_inline}'")
    # Join with ' \\\n' so every continuation line ends with a backslash
    # except the last one.
    curl = " \\\n".join(curl_parts)

    # ---- Rust (reqwest-style) ----
    rust_headers = "\n".join(
        f'        .header("{h}", "{v}")' for h, v in headers
    )
    if body_json is not None:
        # Use a raw string literal so we don't need to escape the JSON.
        rust_body_decl = (
            "    let body: Value = serde_json::from_str(r#\"\n"
            f"{body_json}\n"
            "    \"#)?;\n"
        )
        rust_body_send = "        .json(&body)\n"
    else:
        rust_body_decl = ""
        rust_body_send = ""
    rust = (
        "use reqwest::Client;\n"
        "use serde_json::Value;\n\n"
        "#[tokio::main]\n"
        "async fn main() -> Result<(), Box<dyn std::error::Error>> {\n"
        "    let client = Client::new();\n"
        f"{rust_body_decl}"
        f"    let resp = client\n        .{endpoint.method.lower()}(\"{url}\")\n"
        f"{rust_headers}\n"
        f"{rust_body_send}"
        "        .send()\n"
        "        .await?\n"
        "        .json::<Value>()\n"
        "        .await?;\n"
        "    println!(\"{resp:#?}\");\n"
        "    Ok(())\n"
        "}\n"
    )

    # ---- Python (httpx-style) ----
    if headers:
        py_header_lines = "".join(f'        "{h}": "{v}",\n' for h, v in headers)
        py_headers_repr = "{\n" + py_header_lines + "    }"
    else:
        py_headers_repr = "{}"
    py_body = (
        f"    body = {body_json}\n" if body_json is not None else ""
    )
    py_call = (
        f'    resp = httpx.{endpoint.method.lower()}(\n'
        f'        "{url}",\n'
        f'        headers=headers,\n'
        + ("        json=body,\n" if body_json is not None else "")
        + "    )\n"
    )
    python = (
        "import httpx\n\n"
        "def main() -> None:\n"
        f"    headers = {py_headers_repr}\n"
        f"{py_body}"
        f"{py_call}"
        "    resp.raise_for_status()\n"
        "    print(resp.json())\n\n"
        "if __name__ == \"__main__\":\n"
        "    main()\n"
    )

    # ---- Go (net/http) ----
    go_headers = "\n".join(
        f'    req.Header.Set("{h}", "{v}")' for h, v in headers
    )
    go_body_init = (
        f"    body := []byte(`{body_json}`)\n"
        if body_json is not None
        else "    var body []byte\n"
    )
    go_body_reader = (
        "bytes.NewReader(body)" if body_json is not None else "nil"
    )
    go = (
        "package main\n\n"
        "import (\n"
        "    \"bytes\"\n"
        "    \"fmt\"\n"
        "    \"io\"\n"
        "    \"net/http\"\n"
        ")\n\n"
        "func main() {\n"
        f"{go_body_init}"
        f'    req, err := http.NewRequest("{endpoint.method}", "{url}", {go_body_reader})\n'
        "    if err != nil {\n        panic(err)\n    }\n"
        f"{go_headers}\n"
        "    resp, err := http.DefaultClient.Do(req)\n"
        "    if err != nil {\n        panic(err)\n    }\n"
        "    defer resp.Body.Close()\n"
        "    out, _ := io.ReadAll(resp.Body)\n"
        "    fmt.Println(string(out))\n"
        "}\n"
    )

    # ---- JavaScript (fetch) ----
    if headers:
        js_header_lines = "".join(f'    "{h}": "{v}",\n' for h, v in headers)
        js_headers = "{\n" + js_header_lines + "  }"
    else:
        js_headers = "{}"
    js_body = (
        f"  body: JSON.stringify({body_json}),\n" if body_json is not None else ""
    )
    js = (
        f"const resp = await fetch(\"{url}\", {{\n"
        f"  method: \"{endpoint.method}\",\n"
        f"  headers: {js_headers},\n"
        f"{js_body}"
        "});\n"
        "if (!resp.ok) {\n"
        "  throw new Error(`HTTP ${resp.status}`);\n"
        "}\n"
        "console.log(await resp.json());\n"
    )

    return (
        "### curl\n\n"
        f"```bash\n{curl}\n```\n\n"
        "### Rust (reqwest)\n\n"
        f"```rust\n{rust}```\n\n"
        "### Python (httpx)\n\n"
        f"```python\n{python}```\n\n"
        "### Go (net/http)\n\n"
        f"```go\n{go}```\n\n"
        "### JavaScript (fetch)\n\n"
        f"```javascript\n{js}```\n"
    )


# --------------------------------------------------------------------------- #
# Page assembly
# --------------------------------------------------------------------------- #


_AUTOGEN_BANNER = (
    "{/* AUTO-GENERATED by scripts/gen-api-reference.py — DO NOT EDIT. "
    "Source: openapi/corelink-v1.yaml. Drift gated by "
    ".github/workflows/api-reference-sync.yml. */}\n\n"
)


def render_endpoint_mdx(endpoint: Endpoint, spec: Spec) -> str:
    fm = front_matter(endpoint)
    parts: list[str] = [fm, _AUTOGEN_BANNER]

    parts.append(f"# `{endpoint.method}` `{endpoint.path}`\n\n")

    if endpoint.summary:
        parts.append(f"> {endpoint.summary}\n\n")

    meta_rows = [
        f"- **Operation ID:** `{endpoint.operation_id}`",
        f"- **Tag:** `{endpoint.tag}`",
        f"- **Rate-limit class:** {endpoint.rate_limit_class}",
    ]
    parts.append("\n".join(meta_rows) + "\n\n")

    if endpoint.description.strip():
        parts.append("## Description\n\n")
        parts.append(endpoint.description.rstrip() + "\n\n")

    parts.append("## Authentication\n\n")
    parts.append(render_security(endpoint, spec) + "\n\n")

    parts.append("## Parameters\n\n")
    parts.append(render_parameters(endpoint, spec) + "\n\n")

    parts.append("## Request body\n\n")
    parts.append(render_request_body(endpoint, spec) + "\n\n")

    parts.append("## Responses\n\n")
    parts.append(render_responses(endpoint, spec) + "\n\n")

    parts.append("## Examples\n\n")
    parts.append(
        "_Snippets are auto-generated from the OpenAPI contract. "
        "Replace `<YOUR_PAT>` with a Personal Access Token issued via "
        "[`POST /v1/pats`](./post-v1-pats.mdx)._\n\n"
    )
    parts.append(render_examples(endpoint, spec) + "\n")

    return "".join(parts)


def render_index_mdx(endpoints: list[Endpoint]) -> str:
    """Landing page: tag-grouped, alphabetical-within-group endpoint list."""
    parts: list[str] = []
    # Note: no explicit `id:` — Docusaurus infers it from the file path
    # (`reference/api/index`), which avoids collision with the parent
    # `reference/index` doc.
    parts.append(
        "---\n"
        "title: REST API Reference\n"
        "sidebar_label: REST API Reference\n"
        "description: Auto-generated typed REST API reference for the CoreLink customer, privacy, billing-webhook, admin and ops surface.\n"
        "---\n\n"
    )
    parts.append(_AUTOGEN_BANNER)
    parts.append("# REST API Reference\n\n")
    parts.append(
        "Auto-generated from "
        "[`openapi/corelink-v1.yaml`](https://github.com/humangr-labs/corelink-server/blob/main/openapi/corelink-v1.yaml). "
        "One typed page per endpoint, with example invocations in "
        "**curl**, **Rust**, **Python**, **Go**, and **JavaScript**.\n\n"
    )
    parts.append(
        "Drift between this page set and the canonical YAML is gated by "
        "[`api-reference-sync.yml`](https://github.com/humangr-labs/corelink-server/blob/main/.github/workflows/api-reference-sync.yml) "
        "— every PR that edits the YAML must include the regenerated MDX.\n\n"
    )
    parts.append(
        "See also the raw spec page at [REST API spec](./openapi.mdx).\n\n"
    )

    # Group by display group.
    groups: dict[str, list[Endpoint]] = {}
    for ep in endpoints:
        group = TAG_GROUPS.get(ep.tag, "Other")
        groups.setdefault(group, []).append(ep)

    for group_name in sorted(groups.keys()):
        parts.append(f"## {group_name}\n\n")
        parts.append("| Endpoint | Tag | Summary |\n| --- | --- | --- |\n")
        # Stable: sort by (path, method).
        for ep in sorted(groups[group_name], key=lambda e: (e.path, e.method)):
            link = f"[`{ep.method} {ep.path}`](./endpoints/{ep.slug}.mdx)"
            parts.append(
                f"| {link} | `{ep.tag}` | {mdx_escape(ep.summary)} |\n"
            )
        parts.append("\n")

    return "".join(parts)


# --------------------------------------------------------------------------- #
# I/O
# --------------------------------------------------------------------------- #


def write_if_changed(path: Path, content: str, *, dry_run: bool) -> bool:
    """Return True if the file was (or would be) written."""
    if path.is_file():
        try:
            current = path.read_text(encoding="utf-8")
        except OSError as exc:
            sys.stderr.write(f"fatal: cannot read {path}: {exc}\n")
            sys.exit(4)
        if current == content:
            return False
    if dry_run:
        return True
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8")
    return True


def clean_endpoints_dir(endpoints_dir: Path, keep_slugs: set[str], *, dry_run: bool) -> int:
    """Delete stale endpoint files. Returns the count of removals."""
    if not endpoints_dir.is_dir():
        return 0
    removed = 0
    for f in sorted(endpoints_dir.iterdir()):
        if f.suffix != ".mdx":
            continue
        if f.stem not in keep_slugs:
            removed += 1
            if not dry_run:
                f.unlink()
    return removed


# --------------------------------------------------------------------------- #
# CLI
# --------------------------------------------------------------------------- #


def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(
        prog="gen-api-reference.py",
        description=(
            "Auto-generate the CoreLink REST API reference MDX set from "
            "openapi/corelink-v1.yaml. Idempotent; emits byte-identical "
            "output for the same input. Drift is enforced by "
            ".github/workflows/api-reference-sync.yml."
        ),
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    p.add_argument(
        "--spec",
        type=Path,
        default=DEFAULT_SPEC,
        help=f"Path to OpenAPI YAML (default: {DEFAULT_SPEC.relative_to(REPO_ROOT)}).",
    )
    p.add_argument(
        "--out-dir",
        type=Path,
        default=DEFAULT_OUT,
        help=(
            "Output directory for the generated reference set "
            f"(default: {DEFAULT_OUT.relative_to(REPO_ROOT)})."
        ),
    )
    p.add_argument(
        "--dry-run",
        action="store_true",
        help="Print summary without writing any files.",
    )
    p.add_argument(
        "--check",
        action="store_true",
        help=(
            "Exit non-zero if the on-disk output differs from what would be "
            "generated. Used by the CI drift gate."
        ),
    )
    return p


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)

    spec = load_spec(args.spec)
    endpoints = list(iter_endpoints(spec))
    if not endpoints:
        sys.stderr.write("fatal: no endpoints found in spec\n")
        return 5

    endpoints_dir = args.out_dir / "endpoints"
    index_path = args.out_dir / "index.mdx"

    changes: list[str] = []
    keep: set[str] = set()
    for ep in endpoints:
        keep.add(ep.slug)
        mdx = render_endpoint_mdx(ep, spec)
        target = endpoints_dir / ep.file_name
        if write_if_changed(target, mdx, dry_run=args.dry_run or args.check):
            changes.append(str(target.relative_to(REPO_ROOT)))

    index = render_index_mdx(endpoints)
    if write_if_changed(index_path, index, dry_run=args.dry_run or args.check):
        changes.append(str(index_path.relative_to(REPO_ROOT)))

    removed = clean_endpoints_dir(endpoints_dir, keep, dry_run=args.dry_run or args.check)

    print(f"endpoints: {len(endpoints)}")
    print(f"changed:   {len(changes)}")
    print(f"stale removed: {removed}")
    if args.dry_run:
        print("dry-run: no files written")
        return 0
    if args.check:
        if changes or removed:
            sys.stderr.write(
                "drift: regenerate via `python3 scripts/gen-api-reference.py`\n"
            )
            for c in changes:
                sys.stderr.write(f"  changed: {c}\n")
            return 1
        return 0
    return 0


if __name__ == "__main__":
    sys.exit(main())
