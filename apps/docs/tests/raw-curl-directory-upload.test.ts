import { execFileSync, spawn, spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { clearTimeout as clearTimer, setTimeout as setTimer } from "node:timers";
import { describe, expect, it } from "vitest";

const DOCS_ROOT = path.resolve(__dirname, "..");
const RECIPE = path.join(__dirname, "fixtures", "raw-curl-directory-upload.sh");
const RECORDER = path.join(__dirname, "fixtures", "raw-curl-recorder.sh");
const HTTP_SERVER = path.join(__dirname, "fixtures", "raw-curl-http-server.mjs");
const HTTP_READY_TIMEOUT_MS = 15_000;
const HTTP_REQUEST_TIMEOUT_MS = 10_000;
const HTTP_STOP_TIMEOUT_MS = 2_000;

const LOCALE_PAGES = [
  "docs/integrations/raw-curl.md",
  "i18n/pt-BR/docusaurus-plugin-content-docs/current/integrations/raw-curl.md",
  "i18n/es-419/docusaurus-plugin-content-docs/current/integrations/raw-curl.md",
  "i18n/de/docusaurus-plugin-content-docs/current/integrations/raw-curl.md",
];

const HTTP_FAILURE_CASES = [
  ...(["bash", "zsh"] as const).flatMap((shell) =>
    LOCALE_PAGES.flatMap((page) =>
      ([422, 500] as const).map((status) => ({ shell, page, status })),
    ),
  ),
];

function commandPath(name: string): string | undefined {
  try {
    return execFileSync("sh", ["-c", `command -v ${name}`], {
      encoding: "utf8",
      stdio: ["ignore", "pipe", "ignore"],
    }).trim();
  } catch {
    return undefined;
  }
}

function digestOf(file: string): string {
  return execFileSync("b3sum", [file], { encoding: "utf8" })
    .trim()
    .split(/\s+/)[0];
}

function extractRecipe(relative: string): string {
  const page = fs.readFileSync(path.join(DOCS_ROOT, relative), "utf8");
  const marker = page.indexOf("ARCHIVE=");
  const fenceStart = page.lastIndexOf("```bash", marker);
  const snippetStart = page.indexOf("\n", fenceStart) + 1;
  const snippetEnd = page.indexOf("\n```", snippetStart);
  expect(marker, `${relative} has an archive recipe`).toBeGreaterThan(fenceStart);
  expect(snippetEnd, `${relative} has a closed shell fence`).toBeGreaterThan(
    snippetStart,
  );
  return page.slice(snippetStart, snippetEnd);
}

function makeRecorder(binDir: string, recorderDir: string, mode: string): void {
  const recorder = fs.readFileSync(RECORDER, "utf8");
  const curlPath = path.join(binDir, "curl");
  fs.writeFileSync(curlPath, recorder, { mode: 0o755 });
  fs.mkdirSync(recorderDir, { recursive: true });
  if (mode === "tar-fail") {
    fs.writeFileSync(
      path.join(binDir, "tar"),
      "#!/bin/sh\necho 'forced tar failure' >&2\nexit 73\n",
      { mode: 0o755 },
    );
  }
  if (mode === "curl-fail") {
    fs.writeFileSync(
      curlPath,
      "#!/bin/sh\necho 'forced curl failure' >&2\nexit 74\n",
      { mode: 0o755 },
    );
  }
}

function runRecipe(
  shell: string,
  workDir: string,
  recorderDir: string,
  staleDigest: string,
  recipe: string,
  mode = "success",
): { url: string; uploadedDigest: string; output: string } {
  const binDir = fs.mkdtempSync(path.join(workDir, "bin-"));
  makeRecorder(binDir, recorderDir, mode);
  const recipePath = path.join(workDir, `recipe-${mode}.sh`);
  fs.writeFileSync(recipePath, `${recipe}\n`);
  const env = {
    ...process.env,
    LC_ALL: "C",
    PATH: `${binDir}:${process.env.PATH ?? ""}`,
    TMPDIR: workDir,
    CORELINK_PAT: "fixture-token",
    CORELINK_TENANT: "fixture-tenant",
    CORELINK_BASE: "https://wp-b163-fixture.invalid",
    CORELINK_CURL_RECORDER: recorderDir,
  };
  fs.writeFileSync(path.join(workDir, "digest.txt"), staleDigest);
  const output = execFileSync(shell, [recipePath], {
    cwd: workDir,
    env,
    encoding: "utf8",
    timeout: 15_000,
  });
  const url = fs.readFileSync(path.join(recorderDir, "url"), "utf8").trim();
  const uploadedDigest = fs
    .readFileSync(path.join(recorderDir, "uploaded.digest"), "utf8")
    .trim();
  return { url, uploadedDigest, output };
}

function waitForServerReady(server: ReturnType<typeof spawn>): Promise<string> {
  return new Promise((resolve, reject) => {
    let settled = false;
    let buffer = "";
    let timer: ReturnType<typeof setTimer>;
    const finish = (error?: Error, port?: string): void => {
      if (settled) return;
      settled = true;
      clearTimer(timer);
      server.stdout?.removeListener("data", onData);
      server.removeListener("error", onError);
      server.removeListener("exit", onExit);
      if (error) reject(error);
      else resolve(port!);
    };
    const onData = (chunk: Uint8Array | string): void => {
      buffer += chunk.toString();
      const lines = buffer.split("\n");
      buffer = lines.pop() ?? "";
      for (const line of lines) {
        if (!line) continue;
        try {
          const message = JSON.parse(line) as { ready?: boolean; port?: number };
          if (message.ready && Number.isInteger(message.port) && message.port! > 0) {
            finish(undefined, String(message.port));
            return;
          }
        } catch {
          // Ignore non-protocol fixture output while waiting for readiness.
        }
      }
    };
    const onError = (error: Error): void => finish(error);
    const onExit = (code: number | null, signal: string | null): void =>
      finish(new Error(`HTTP fixture server exited before readiness (${code ?? signal ?? "unknown"})`));
    timer = setTimer(
      () => finish(new Error("HTTP fixture server readiness timed out")),
      HTTP_READY_TIMEOUT_MS,
    );
    server.stdout?.setEncoding("utf8");
    server.stdout?.on("data", onData);
    server.once("error", onError);
    server.once("exit", onExit);
    if (server.exitCode !== null || server.signalCode !== null) {
      finish(
        new Error(
          `HTTP fixture server exited before readiness (${server.exitCode ?? server.signalCode ?? "unknown"})`,
        ),
      );
    }
  });
}

function waitForFile(file: string, timeoutMs: number): Promise<void> {
  if (fs.existsSync(file)) return Promise.resolve();
  return new Promise((resolve, reject) => {
    let settled = false;
    let watcher: fs.FSWatcher | undefined;
    const finish = (error?: Error): void => {
      if (settled) return;
      settled = true;
      clearTimer(timer);
      watcher?.close();
      if (error) reject(error);
      else resolve();
    };
    const timer = setTimer(
      () => finish(new Error(`Timed out waiting for ${file}`)),
      timeoutMs,
    );
    try {
      watcher = fs.watch(path.dirname(file), () => {
        if (fs.existsSync(file)) finish();
      });
      // The request may complete between the initial check and watcher setup.
      if (fs.existsSync(file)) finish();
    } catch (error) {
      finish(error as Error);
    }
  });
}

function waitForExit(server: ReturnType<typeof spawn>, timeoutMs: number): Promise<boolean> {
  if (server.exitCode !== null || server.signalCode !== null) return Promise.resolve(true);
  return new Promise((resolve) => {
    const timer = setTimer(() => {
      server.removeListener("exit", onExit);
      resolve(false);
    }, timeoutMs);
    const onExit = (): void => {
      clearTimer(timer);
      resolve(true);
    };
    server.once("exit", onExit);
  });
}

async function stopServer(server: ReturnType<typeof spawn>): Promise<void> {
  if (server.exitCode !== null || server.signalCode !== null) return;
  server.kill("SIGTERM");
  if (await waitForExit(server, HTTP_STOP_TIMEOUT_MS)) return;
  server.kill("SIGKILL");
  await waitForExit(server, HTTP_STOP_TIMEOUT_MS);
}

async function runRecipeAgainstHttp(
  shell: string,
  workDir: string,
  relative: string,
  status: number,
): Promise<{
  result: ReturnType<typeof spawnSync>;
  request: { method: string; url: string; bytes: number };
}> {
  const portFile = path.join(workDir, `http-${status}.port`);
  const requestFile = path.join(workDir, `http-${status}.request`);
  const server = spawn(process.execPath, [HTTP_SERVER], {
    env: {
      ...process.env,
      CORELINK_HTTP_PORT_FILE: portFile,
      CORELINK_HTTP_REQUEST_FILE: requestFile,
      CORELINK_HTTP_STATUS: String(status),
    },
    stdio: ["ignore", "pipe", "pipe"],
  });
  try {
    const port = await waitForServerReady(server);
    const recipePath = path.join(workDir, `http-recipe-${status}.sh`);
    fs.writeFileSync(recipePath, `${extractRecipe(relative)}\n`);
    const distDir = path.join(workDir, "dist");
    fs.mkdirSync(distDir, { recursive: true });
    fs.writeFileSync(path.join(distDir, "http.txt"), "fixture HTTP upload\n");
    const result = spawnSync(shell, [recipePath], {
      cwd: workDir,
      env: {
        ...process.env,
        LC_ALL: "C",
        TMPDIR: workDir,
        CORELINK_PAT: "fixture-token",
        CORELINK_TENANT: "fixture-tenant",
        CORELINK_BASE: `http://127.0.0.1:${port}`,
      },
      encoding: "utf8",
      timeout: 15_000,
    });
    await waitForFile(requestFile, HTTP_REQUEST_TIMEOUT_MS);
    return { result, request: JSON.parse(fs.readFileSync(requestFile, "utf8")) };
  } finally {
    await stopServer(server);
  }
}

describe("raw-curl directory upload recipe", () => {
  it("keeps the corrected materialize → digest → URL recipe in every locale", () => {
    for (const relative of LOCALE_PAGES) {
      const section = extractRecipe(relative);
      const required = [
        "set -eu",
        'ARCHIVE="$(mktemp "${TMPDIR:-/tmp}/corelink-dist.XXXXXX")"',
        "trap 'rm -f \"$ARCHIVE\"' EXIT",
        'tar -czf "$ARCHIVE" ./dist/',
        'DIGEST="$(b3sum "$ARCHIVE"',
        '[ -n "$DIGEST" ] ||',
        "curl --fail-with-body -sS -X PUT",
        '--data-binary "@$ARCHIVE"',
        '"$CORELINK_BASE/v1/cas/$CORELINK_TENANT/$DIGEST"',
      ];
      let previous = -1;
      for (const line of required) {
        const offset = section.indexOf(line);
        expect(offset, `${relative} missing ${line}`).toBeGreaterThan(previous);
        previous = offset;
      }
      const executableLines = section
        .split("\n")
        .filter((line) => !/^\s*#/.test(line))
        .join("\n");
      expect(executableLines, `${relative} must bind HTTP failure to executable curl`)
        .toMatch(/^\s*curl\b[^\n]*--fail-with-body\b/m);
      expect(section, relative).not.toContain("/tmp/digest.txt");
      expect(section, relative).not.toContain("$(cat");

      const fixture = fs.readFileSync(RECIPE, "utf8");
      for (const line of required) {
        expect(fixture, `fixture missing ${line}`).toContain(line);
      }
    }
  });

  it.each(["bash", "zsh"])(
    "B-163 binds URL digest to uploaded bytes and ignores stale state under %s",
    (shell) => {
      expect(commandPath(shell), `${shell} is required for the shell matrix`).toBeTruthy();
      const workDir = fs.mkdtempSync(path.join(os.tmpdir(), "raw-curl-"));
      const distDir = path.join(workDir, "dist");
      fs.mkdirSync(distDir);
      fs.writeFileSync(path.join(distDir, "alpha.txt"), "same bytes\n");
      fs.writeFileSync(path.join(distDir, "beta.txt"), "same bytes too\n");
      const digests: string[] = [];
      const recipe = extractRecipe(LOCALE_PAGES[0]);

      for (let run = 0; run < 4; run += 1) {
        const mtime = new Date(1_700_000_000_000 + run * 60_000);
        fs.utimesSync(path.join(distDir, "alpha.txt"), mtime, mtime);
        fs.utimesSync(path.join(distDir, "beta.txt"), mtime, mtime);
        const recorderDir = fs.mkdtempSync(path.join(workDir, "record-"));
        const result = runRecipe(
          shell,
          workDir,
          recorderDir,
          `stale-${run}`,
          recipe,
        );
        expect(result.url).not.toContain(`stale-${run}`);
        expect(result.url.endsWith(`/${result.uploadedDigest}`)).toBe(true);
        expect(result.output).toContain(`Uploaded as ${result.uploadedDigest}`);
        expect(result.uploadedDigest).toMatch(/^[0-9a-f]{64}$/);
        expect(result.uploadedDigest).toBe(
          digestOf(path.join(recorderDir, "uploaded.bin")),
        );
        expect(
          fs.readdirSync(workDir).filter((name) => name.startsWith("corelink-dist.")),
        ).toEqual([]);
        digests.push(result.uploadedDigest);
      }

      // The recipe makes the byte binding honest, but does not claim that
      // tar/gzip metadata (mtime, order, or gzip timestamp) is reproducible.
      expect(new Set(digests).size).toBe(4);
    },
    60_000,
  );

  it.each(LOCALE_PAGES.flatMap((page) => [
    [page, "tar-fail"],
    [page, "curl-fail"],
  ]))(
    "fails closed with no Uploaded-as success for %s (%s)",
    (relative, mode) => {
      const shell = commandPath("bash");
      expect(shell, "bash is required for failure teeth").toBeTruthy();
      const workDir = fs.mkdtempSync(path.join(os.tmpdir(), "raw-curl-fail-"));
      const distDir = path.join(workDir, "dist");
      fs.mkdirSync(distDir);
      fs.writeFileSync(path.join(distDir, "input.txt"), "fixture\n");
      const recorderDir = fs.mkdtempSync(path.join(workDir, "record-"));
      const binDir = fs.mkdtempSync(path.join(workDir, "bin-"));
      makeRecorder(binDir, recorderDir, mode);
      const recipePath = path.join(workDir, "published-recipe.sh");
      fs.writeFileSync(recipePath, `${extractRecipe(relative)}\n`);
      const env = {
        ...process.env,
        LC_ALL: "C",
        PATH: `${binDir}:${process.env.PATH ?? ""}`,
        TMPDIR: workDir,
        CORELINK_PAT: "fixture-token",
        CORELINK_TENANT: "fixture-tenant",
        CORELINK_BASE: "https://wp-b163-fixture.invalid",
        CORELINK_CURL_RECORDER: recorderDir,
      };
      let failure: { status?: number; stdout?: string; stderr?: string };
      try {
        execFileSync(shell!, [recipePath], {
          cwd: workDir,
          env,
          encoding: "utf8",
          timeout: 15_000,
        });
        throw new Error(`${mode} unexpectedly succeeded`);
      } catch (error) {
        const result = error as {
          status?: number;
          stdout?: string;
          stderr?: string;
        };
        failure = result;
      }
      expect(failure.status, `${relative} ${mode} must be nonzero`).not.toBe(0);
      expect(String(failure.stdout ?? "")).not.toContain("Uploaded as");
      expect(fs.existsSync(path.join(recorderDir, "url"))).toBe(false);
    },
    60_000,
  );

  it.each(HTTP_FAILURE_CASES)(
    "fails closed on a real HTTP $shell × $page × $status response",
    async ({ shell, page: relative, status }) => {
      const shellPath = commandPath(shell);
      expect(shellPath, `${shell} is required for HTTP failure teeth`).toBeTruthy();
      expect(commandPath("curl"), "curl is required for HTTP failure teeth").toBeTruthy();
      const workDir = fs.mkdtempSync(path.join(os.tmpdir(), "raw-curl-http-"));
      const { result, request } = await runRecipeAgainstHttp(shellPath!, workDir, relative, status);
      expect(request.method).toBe("PUT");
      expect(request.url).toMatch(new RegExp(`/v1/cas/fixture-tenant/[0-9a-f]{64}$`));
      expect(request.bytes).toBeGreaterThan(0);
      expect(result.status, `HTTP ${status} must make the shell fail`).not.toBe(0);
      expect(result.stdout ?? "").not.toContain("Uploaded as");
    },
    60_000,
  );
});
