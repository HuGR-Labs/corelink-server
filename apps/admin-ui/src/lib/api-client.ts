/**
 * Typed API client for the CoreLink backend.
 *
 * Security requirements (CTRL-CRED-001 + CTRL-PRIV-001):
 *  - Bearer token NEVER logged to console, Sentry, or window.
 *  - Token MUST be passed through Authorization header only — never query string.
 *  - On error, we surface response body text but redact anything that
 *    looks like a token before re-throwing.
 */

export interface ApiError {
  status: number;
  body: string;
  message: string;
}

export class ApiClientError extends Error implements ApiError {
  public readonly status: number;
  public readonly body: string;

  constructor(args: ApiError) {
    super(args.message);
    this.status = args.status;
    this.body = args.body;
  }
}

export interface ApiCallOptions {
  /** Bearer token from Clerk session. Required for authenticated routes. */
  token?: string;
  /** Override base URL — used by tests. */
  baseUrl?: string;
  /** Custom fetch impl — injected by tests. */
  fetchImpl?: typeof fetch;
  signal?: AbortSignal;
}

const TOKEN_RE = /(corelink_(?:prod|test)_[A-Za-z0-9_-]+)/g;

/** Replace any bearer-like token in arbitrary text with a placeholder. */
export function redactTokens(text: string): string {
  return text.replace(TOKEN_RE, "corelink_***_REDACTED");
}

function defaultBaseUrl(): string {
  // Read at call time so tests can stub process.env.
  const fromEnv =
    typeof process !== "undefined" ? process.env?.CORELINK_API_URL : undefined;
  return fromEnv ?? "https://corelink-api.humangr.com";
}

function buildHeaders(opts: ApiCallOptions): Headers {
  const headers = new Headers();
  headers.set("Accept", "application/json");
  headers.set("Content-Type", "application/json");
  if (opts.token) {
    headers.set("Authorization", `Bearer ${opts.token}`);
  }
  return headers;
}

async function unwrap<T>(res: Response): Promise<T> {
  const text = await res.text();
  if (!res.ok) {
    const safe = redactTokens(text);
    throw new ApiClientError({
      status: res.status,
      body: safe,
      message: `API ${res.status}: ${safe.slice(0, 200)}`,
    });
  }
  if (text.length === 0) return undefined as unknown as T;
  return JSON.parse(text) as T;
}

export async function apiGet<T>(
  path: string,
  opts: ApiCallOptions = {},
): Promise<T> {
  const f = opts.fetchImpl ?? fetch;
  const url = `${opts.baseUrl ?? defaultBaseUrl()}${path}`;
  const res = await f(url, {
    method: "GET",
    headers: buildHeaders(opts),
    signal: opts.signal,
  });
  return unwrap<T>(res);
}

export async function apiPost<T>(
  path: string,
  body: unknown,
  opts: ApiCallOptions = {},
): Promise<T> {
  const f = opts.fetchImpl ?? fetch;
  const url = `${opts.baseUrl ?? defaultBaseUrl()}${path}`;
  const res = await f(url, {
    method: "POST",
    headers: buildHeaders(opts),
    body: JSON.stringify(body),
    signal: opts.signal,
  });
  return unwrap<T>(res);
}
