"""Render language examples for generated OpenAPI endpoint pages.

The generator keeps specification parsing and page assembly separate from this
presentation-only module.  Callers supply the canonical URL, header, and body
builders so the rendering remains tied to the same endpoint model and schema
resolution rules.
"""

from __future__ import annotations

import json
from collections.abc import Callable
from typing import Any


def render_examples(
    endpoint: Any,
    spec: Any,
    example_url: Callable[[Any, Any], str],
    example_headers: Callable[[Any, Any], list[tuple[str, str]]],
    example_body: Callable[[Any, Any], Any | None],
) -> str:
    """Render curl, Rust, Python, Go, and JavaScript examples."""
    url = example_url(endpoint, spec)
    headers = example_headers(endpoint, spec)
    body = example_body(endpoint, spec)
    body_json = json.dumps(body, indent=2, sort_keys=True) if body is not None else None

    # ---- curl ----
    curl_parts: list[str] = [f"curl -X {endpoint.method}", f"  '{url}'"]
    for h, v in headers:
        curl_parts.append(f"  -H '{h}: {v}'")
    if body_json is not None:
        body_inline = body_json.replace("'", "'\\''")
        curl_parts.append(f"  -d '{body_inline}'")
    curl = " \\\n".join(curl_parts)

    # ---- Rust (reqwest-style) ----
    rust_headers = "\n".join(
        f'        .header("{h}", "{v}")' for h, v in headers
    )
    if body_json is not None:
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
    py_body = f"    body = {body_json}\n" if body_json is not None else ""
    py_call = (
        f'    resp = httpx.{endpoint.method.lower()}(\n'
        f'        "{url}",\n'
        f"        headers=headers,\n"
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
    go_body_reader = "bytes.NewReader(body)" if body_json is not None else "nil"
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
    js_body = f"  body: JSON.stringify({body_json}),\n" if body_json is not None else ""
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
