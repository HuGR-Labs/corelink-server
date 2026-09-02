import fs from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";

const DOC_PATH = path.resolve(
  __dirname,
  "..",
  "..",
  "..",
  "apps",
  "docs",
  "docs",
  "integrations",
  "sccache-cargo.md",
);
const DOC_PATHS = {
  "en-US": DOC_PATH,
  de: path.resolve(
    __dirname,
    "..",
    "..",
    "..",
    "apps",
    "docs",
    "i18n",
    "de",
    "docusaurus-plugin-content-docs",
    "current",
    "integrations",
    "sccache-cargo.md",
  ),
  "es-419": path.resolve(
    __dirname,
    "..",
    "..",
    "..",
    "apps",
    "docs",
    "i18n",
    "es-419",
    "docusaurus-plugin-content-docs",
    "current",
    "integrations",
    "sccache-cargo.md",
  ),
  "pt-BR": path.resolve(
    __dirname,
    "..",
    "..",
    "..",
    "apps",
    "docs",
    "i18n",
    "pt-BR",
    "docusaurus-plugin-content-docs",
    "current",
    "integrations",
    "sccache-cargo.md",
  ),
} as const;
const CARGO_ROUTE_PATH = path.resolve(
  __dirname,
  "..",
  "..",
  "..",
  "crates",
  "corelink-container",
  "src",
  "routes",
  "cargo.rs",
);

const PUBLISHED_METHODS = ["GET", "PUT", "HEAD", "PROPFIND", "MKCOL", "DELETE"];
const DELETE_AUTH_ASSERTIONS: Record<keyof typeof DOC_PATHS, RegExp> = {
  "en-US": /cache-write scope[\s\S]*PAT with write capability/i,
  de: /Cache-Schreibberechtigung[\s\S]*PAT mit Schreibberechtigung/i,
  "es-419": /alcance de escritura de caché[\s\S]*PAT con capacidad de escritura/i,
  "pt-BR": /escopo de escrita no cache[\s\S]*PAT com capacidade de escrita/i,
};
const PROBE_ONLY_ASSERTIONS: Record<keyof typeof DOC_PATHS, RegExp> = {
  "en-US": /\.sccache_check[\s\S]*probe-only[\s\S]*not a build-artifact key/i,
  de: /\.sccache_check[\s\S]*ausschließlich für die Sonde bestimmt[\s\S]*kein\s+Build-Artefakt-Schlüssel/i,
  "es-419": /\.sccache_check[\s\S]*exclusiva de la sonda[\s\S]*no\s+una clave de artefacto de build/i,
  "pt-BR": /\.sccache_check[\s\S]*exclusiva da sonda[\s\S]*não\s+uma chave de artefato de build/i,
};

function withoutRustComments(source: string): string {
  return source
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .replace(/(^|[^:])\/\/.*$/gm, "$1");
}

function assertSccacheContract(
  markdown: string,
  cargoSource: string,
  locale: keyof typeof DOC_PATHS,
): void {
  const rows = [
    ...markdown.matchAll(/^\|\s*`([^`]+)`\s*\|[^\n]*$/gm),
  ].map((match) => match[1]);
  const methods = rows.filter((row) => PUBLISHED_METHODS.includes(row));
  expect(methods).toEqual(PUBLISHED_METHODS);
  const deleteRow = markdown.match(/^\|\s*`DELETE`\s*\|[^\n]*$/m)?.[0] ?? "";
  expect(deleteRow).toMatch(DELETE_AUTH_ASSERTIONS[locale]);
  expect(markdown).toMatch(PROBE_ONLY_ASSERTIONS[locale]);
  expect(markdown).toMatch(/read-only for the rest of\s+the daemon's lifetime|Nur-Lesen|solo lectura|somente leitura/i);
  expect(markdown).toMatch(/failed write|Schreibfehler|fallo de escritura|falha de escrita/i);
  expect(markdown).toMatch(/sccache --show-stats/);
  expect(markdown).toMatch(/Cache errors/i);
  expect(markdown).toMatch(/\.sccache_check/);
  // The probe key may be called cleanup, but the old contract incorrectly
  // demoted DELETE itself to an internal-only method. Reject that claim only
  // when it is attached to `.sccache_check`, so the truthful prose
  // "not internal-only" remains valid.
  expect(markdown).not.toMatch(
    /\.sccache_check[\s\S]{0,300}(internal cleanup|internal-only|not as a public|nicht nur intern|nicht als öffentliche|no solo interna|no como un método público|limpieza de control interno|não apenas interna|não como método público)/i,
  );

  const executableRoute = withoutRustComments(cargoSource);
  // A missing source file or an unparseable route is a hard failure. In
  // particular, a method mentioned only in a Rust comment must not satisfy
  // the served-method contract.
  expect(executableRoute.length).toBeGreaterThan(0);
  expect(executableRoute).toMatch(/Method::GET/);
  expect(executableRoute).toMatch(/Method::PUT/);
  expect(executableRoute).toMatch(/Method::HEAD/);
  expect(executableRoute).toMatch(/if\s+req\.method\(\)\.as_str\(\)\s*==\s*"PROPFIND"/);
  expect(executableRoute).toMatch(/if\s+req\.method\(\)\.as_str\(\)\s*==\s*"MKCOL"/);
  expect(executableRoute).toMatch(/if\s+req\.method\(\)\s*==\s*Method::DELETE/);

  // DELETE is an authenticated write, not an internal-only probe. Keep the
  // auth check adjacent to its executable branch so a future route cannot
  // silently publish unauthenticated arbitrary eviction.
  const deleteBranch = executableRoute.match(
    /if\s+req\.method\(\)\s*==\s*Method::DELETE[\s\S]*?\n\s{4}\}/,
  )?.[0] ?? "";
  expect(deleteBranch).toMatch(/requires_cache_write/);
  expect(deleteBranch).toMatch(/handle_delete/);
  expect(cargoSource).toMatch(/async fn handle_delete\([\s\S]*?resolve_with_capability/);
}

const cargoSource = fs.readFileSync(CARGO_ROUTE_PATH, "utf8");

describe("sccache/cargo published contract", () => {
  for (const [locale, docPath] of Object.entries(DOC_PATHS)) {
    it(`${locale} documents every served method and the write-health latch`, () => {
      expect(fs.existsSync(docPath), `${locale} translation is present`).toBe(true);
      assertSccacheContract(fs.readFileSync(docPath, "utf8"), cargoSource, locale as keyof typeof DOC_PATHS);
    });
  }

  const markdown = fs.readFileSync(DOC_PATH, "utf8");

  // These are deliberately small mutation fixtures for the contract gate.
  // They prevent a future edit from weakening the page while retaining a
  // superficially plausible paragraph or method table.
  const mutationFixtures: Array<[string, string, string]> = [
    [
      "missing PROPFIND",
      markdown.replace(/^\| `PROPFIND`[^\n]*\n/m, ""),
      "PROPFIND",
    ],
    [
      "missing MKCOL",
      markdown.replace(/^\| `MKCOL`[^\n]*\n/m, ""),
      "MKCOL",
    ],
    [
      "missing DELETE",
      markdown.replace(/^\| `DELETE`[^\n]*\n/m, ""),
      "DELETE",
    ],
    [
      "DELETE has no authorization requirement",
      markdown.replace(
        /^\| `DELETE`[^\n]*$/m,
        "| `DELETE` | Remove a cache key. |",
      ),
      "DELETE auth",
    ],
    [
      "missing read-only latch warning",
      markdown.replace(
        /:::warning A failed write makes sccache read-only[\s\S]*?\n:::\n/m,
        "",
      ),
      "read-only",
    ],
    [
      "PROPFIND present only in a comment",
      markdown,
      "PROPFIND",
    ],
    [
      "DELETE runtime branch removed",
      markdown,
      "DELETE runtime",
    ],
    [
      "DELETE runtime loses write authorization",
      markdown,
      "DELETE write auth",
    ],
  ];

  it.each(mutationFixtures)("rejects the %s mutation", (_name, mutatedDoc, method) => {
    const mutatedRoute =
      method === "PROPFIND" && _name === "PROPFIND present only in a comment"
        ? cargoSource.replace(
            /^(\s*)if req\.method\(\)\.as_str\(\) == "PROPFIND"/m,
            "$1// if req.method().as_str() == \"PROPFIND\"",
          )
        : method === "DELETE runtime"
          ? cargoSource.replace(
              /if req\.method\(\) == Method::DELETE/,
              "if req.method() == Method::PATCH",
            )
        : method === "DELETE write auth"
          ? cargoSource.replace(
              /(if req\.method\(\) == Method::DELETE \{\s+if !)requires_cache_write\(scope\)/,
              "$1requires_cache_read(scope)",
            )
        : cargoSource;
    expect(() => assertSccacheContract(mutatedDoc, mutatedRoute, "en-US")).toThrow();
  });
});
