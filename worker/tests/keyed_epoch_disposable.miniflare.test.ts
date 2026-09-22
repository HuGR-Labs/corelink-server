/**
 * Disposable B-054 workflow exercised in Miniflare's workerd runtime.
 *
 * This is a transport/persistence harness, not a production audit writer or
 * an independent witness. It uses an in-process witness and deterministic
 * fixture key so it can prove E0→E1, restart, receipt integrity, and
 * fail-closed mutation handling without credentials or external state.
 * Independent witness and live-partition evidence remain #1793+ work.
 */

import { afterEach, describe, expect, it } from "vitest";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

// The literal is evaluated by workerd, not by Node. SHA-256/HMAC-SHA-256 are
// deliberately scoped to this disposable receipt transport fixture; production
// B-054 link and witness formulas remain implemented by the Rust runtime.
const HARNESS = String.raw`
const text = new TextEncoder();
const zero = "0".repeat(64);
const fixtureKey = new Uint8Array(32).fill(7);
const hex = bytes => Array.from(new Uint8Array(bytes), b => b.toString(16).padStart(2, "0")).join("");
const hash = async value => hex(await crypto.subtle.digest("SHA-256", text.encode(value)));
const keyed = async value => {
  const key = await crypto.subtle.importKey("raw", fixtureKey, { name: "HMAC", hash: "SHA-256" }, false, ["sign"]);
  return hex(await crypto.subtle.sign("HMAC", key, text.encode(value)));
};
const canonical = ({ sequence, epoch, previous, payload }) => sequence + "|" + epoch + "|" + previous + "|" + payload;
const response = (body, status = 200) => Response.json(body, { status });

async function read(db) {
  const row = await db.prepare("SELECT body FROM epoch_harness WHERE id = 1").first();
  return row ? JSON.parse(row.body) : null;
}
async function write(db, state) {
  await db.prepare("INSERT OR REPLACE INTO epoch_harness (id, body) VALUES (1, ?1)").bind(JSON.stringify(state)).run();
}
async function seal(state, epoch, payload) {
  const sequence = state.rows.length;
  const previous = state.head;
  const row = { sequence, epoch, previous, payload };
  row.hash = epoch === 0 ? await hash("E0\0" + canonical(row)) : await keyed("E1\0" + canonical(row));
  state.rows.push(row);
  state.head = row.hash;
}
async function witness(state) {
  const previous = state.receipts.length ? state.receipts.at(-1).hash : zero;
  const receipt = { sequence: state.receipts.length, previous, head: state.head };
  receipt.hash = await hash("receipt-v1\0" + JSON.stringify(receipt));
  state.receipts.push(receipt);
}
async function verify(state) {
  if (!state || state.epochs.length !== 2 || state.epochs[0].state !== "closed" || state.epochs[1].state !== "active") return false;
  if (state.rows.length !== 3 || state.receipts.length !== 2) return false;
  let previous = zero;
  for (const [sequence, row] of state.rows.entries()) {
    if (row.sequence !== sequence || row.previous !== previous || row.epoch !== (sequence < 2 ? 0 : 1)) return false;
    const expected = row.epoch === 0 ? await hash("E0\0" + canonical(row)) : await keyed("E1\0" + canonical(row));
    if (row.hash !== expected) return false;
    previous = row.hash;
  }
  if (state.head !== previous) return false;
  let receiptPrevious = zero;
  for (const [sequence, receipt] of state.receipts.entries()) {
    if (receipt.sequence !== sequence || receipt.previous !== receiptPrevious) return false;
    const expected = await hash("receipt-v1\0" + JSON.stringify({ sequence: receipt.sequence, previous: receipt.previous, head: receipt.head }));
    if (receipt.hash !== expected) return false;
    receiptPrevious = receipt.hash;
  }
  return state.receipts.at(-1).head === state.head;
}
async function transitionReady(state) {
  if (!state || state.epochs.length !== 1 || state.epochs[0].state !== "active" || state.rows.length !== 2 || state.receipts.length !== 1) return false;
  const probe = structuredClone(state);
  probe.epochs[0].state = "closed"; probe.epochs.push({ id: 1, state: "active", algorithm: "E1", key_id: 1 });
  await seal(probe, 1, "keyed-epoch-a"); await witness(probe);
  return verify(probe);
}

export default {
  async fetch(request, env) {
    const db = env.AUDIT_HARNESS_DB;
    await db.exec("CREATE TABLE IF NOT EXISTS epoch_harness (id INTEGER PRIMARY KEY CHECK (id = 1), body TEXT NOT NULL)");
    const path = new URL(request.url).pathname;
    let state = await read(db);
    if (path === "/bootstrap") {
      state = { head: zero, epochs: [{ id: 0, state: "active", algorithm: "E0" }], rows: [], receipts: [] };
      await seal(state, 0, "legacy-prefix-a"); await seal(state, 0, "legacy-prefix-b"); await witness(state);
      await write(db, state); return response({ epoch: 0, receipt_hash: state.receipts[0].hash }, 201);
    }
    if (path === "/transition") {
      if (!(await transitionReady(state))) return response({ code: "INDETERMINATE" }, 409);
      state.epochs[0].state = "closed"; state.epochs.push({ id: 1, state: "active", algorithm: "E1", key_id: 1 });
      await seal(state, 1, "keyed-epoch-a"); await witness(state); await write(db, state);
      return response({ epoch: 1, receipt_hash: state.receipts[1].hash }, 201);
    }
    if (path.startsWith("/tamper/")) {
      if (!state) return response({ code: "INDETERMINATE" }, 409);
      const kind = path.slice(8);
      if (kind === "mutate") state.rows[2].payload = "mutated";
      else if (kind === "delete") state.rows.splice(1, 1);
      else if (kind === "replay") state.rows.push({ ...state.rows[2] });
      else if (kind === "reorder") state.rows.reverse();
      else if (kind === "receipt") state.receipts[1].hash = zero;
      else return response({ code: "UNKNOWN_MUTATION" }, 400);
      await write(db, state); return response({ mutation: kind }, 202);
    }
    if (path === "/verify" || path === "/resume") return (await verify(state)) ? response({ status: "VERIFY_OK", head: state.head, receipt_hash: state.receipts.at(-1).hash }) : response({ code: "INDETERMINATE" }, 409);
    return response({ code: "NOT_FOUND" }, 404);
  },
};
`;

// eslint-disable-next-line @typescript-eslint/no-explicit-any
let mf: any;
let persistRoot: string | undefined;

async function start(): Promise<void> {
  const { Miniflare } = await import("miniflare");
  mf = new Miniflare({
    script: HARNESS,
    modules: true,
    compatibilityDate: "2026-04-01",
    d1Databases: { AUDIT_HARNESS_DB: "disposable-keyed-epoch" },
    resourcePersistencePath: persistRoot,
  });
}
async function call(path: string): Promise<Response> {
  return mf.dispatchFetch(`https://epoch-harness.test${path}`, { method: "POST" });
}
async function body(response: Response): Promise<Record<string, string>> {
  return (await response.json()) as Record<string, string>;
}
async function dispose(): Promise<void> {
  if (mf) await mf.dispose();
  mf = undefined;
}

afterEach(async () => {
  await dispose();
  if (persistRoot) rmSync(persistRoot, { recursive: true, force: true });
  persistRoot = undefined;
});

describe("B-054 disposable E0→E1 workerd harness", () => {
  it("transitions, hashes receipts, then resumes after a workerd restart", async () => {
    persistRoot = mkdtempSync(join(tmpdir(), "corelink-epoch-"));
    await start();
    const bootstrap = await call("/bootstrap");
    expect(bootstrap.status).toBe(201);
    expect((await body(bootstrap)).epoch).toBe(0);
    const transition = await call("/transition");
    expect(transition.status).toBe(201);
    const transitioned = await body(transition);
    expect(transitioned.epoch).toBe(1);
    expect(transitioned.receipt_hash).toMatch(/^[0-9a-f]{64}$/);
    const beforeRestart = await call("/verify");
    expect(beforeRestart.status).toBe(200);
    const latest = await body(beforeRestart);

    await dispose();
    await start();
    const resumed = await call("/resume");
    expect(resumed.status).toBe(200);
    const resumedBody = await body(resumed);
    expect(resumedBody.status).toBe("VERIFY_OK");
    expect(resumedBody.head).toBe(latest.head);
    expect(resumedBody.receipt_hash).toBe(latest.receipt_hash);
  });

  it.each(["mutate", "delete", "replay", "reorder", "receipt"])("fails closed after %s tampering", async (kind) => {
    persistRoot = mkdtempSync(join(tmpdir(), "corelink-epoch-"));
    await start();
    await call("/bootstrap"); await call("/transition");
    expect((await call(`/tamper/${kind}`)).status).toBe(202);
    const verification = await call("/verify");
    expect(verification.status).toBe(409);
    expect((await body(verification)).code).toBe("INDETERMINATE");
  });
});
